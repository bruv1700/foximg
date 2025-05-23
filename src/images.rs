use std::{
    cell::{RefCell, RefMut},
    ffi::c_void,
    fmt::Display,
    fs::ReadDir,
    mem::ManuallyDrop,
    num::NonZeroU32,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

use circular_buffer::CircularBuffer;
use foximg_image_loader::FoximgImageLoader;
use image::{EncodableLayout, Frame, Frames, ImageResult};
use raylib::prelude::*;

use crate::{Foximg, config::FoximgStyle, resources::FoximgResources};

mod foximg_gif_decoder;
mod foximg_image_loader;
mod foximg_png_decoder;
mod foximg_webp_decoder;

pub use foximg_image_loader::{new_resource, set_window_icon};

/// Number of repetitions in an animated image.
#[derive(Copy, Clone)]
enum AnimationLoops {
    /// Finite number of repetitions
    Finite(NonZeroU32),
    /// Infinite number of repetitions
    Infinite,
}

impl Display for AnimationLoops {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimationLoops::Finite(i) => write!(f, "{i}"),
            AnimationLoops::Infinite => write!(f, "infinite"),
        }
    }
}

impl From<image_webp::LoopCount> for AnimationLoops {
    fn from(value: image_webp::LoopCount) -> Self {
        match value {
            image_webp::LoopCount::Times(i) => Self::Finite(i.into()),
            image_webp::LoopCount::Forever => Self::Infinite,
        }
    }
}

impl From<gif::Repeat> for AnimationLoops {
    fn from(value: gif::Repeat) -> Self {
        match value {
            gif::Repeat::Finite(0) => Self::Finite(NonZeroU32::new(1).unwrap()),
            gif::Repeat::Finite(i) => Self::Finite(NonZeroU32::new(i as u32).unwrap()),
            gif::Repeat::Infinite => Self::Infinite,
        }
    }
}

/// Trait for animated image decoders that can get how many times the animation iterates.
trait AnimationLoopsDecoder {
    /// Returns how many times the decoded animation iterates.
    fn get_loop_count(&self) -> AnimationLoops;
}

struct FoximgImageAnimated {
    frames: Vec<Frame>,
    current: usize,
    current_delay: f32,
    loops: Option<AnimationLoops>,
}

impl FoximgImageAnimated {
    pub fn new(frames_iter: Frames, loops: AnimationLoops) -> ImageResult<Self> {
        Ok(Self {
            frames: frames_iter.collect_frames()?,
            loops: Some(loops),
            current: 0,
            current_delay: 0.,
        })
    }

    /// Returns how many frames the animation has.
    pub fn get_frames_len(&self) -> usize {
        self.frames.len()
    }

    pub fn get_loops(&self) -> Option<AnimationLoops> {
        self.loops
    }

    /// Updates the state of the animation according to `frame_time`. Returns `Some(true)` if it's
    /// time to update the current frame and `Some(false)` otherwise. Returns `None` if the animation
    /// has finished and there's no more frames to update. The `FoximgImageAnimated` object can be
    /// dropped after this.
    pub fn update_frame(&mut self, rl: &RaylibHandle) -> Option<bool> {
        let loops = &mut self.loops?;
        self.current_delay += rl.get_frame_time() * 1000.;

        let frame_delay = self.frames[self.current].delay().numer_denom_ms().0 as f32
            / self.frames[self.current].delay().numer_denom_ms().1 as f32;

        if self.current_delay <= frame_delay {
            return Some(false);
        }

        rl.trace_log(
            TraceLogLevel::LOG_TRACE,
            &format!(
                "FOXIMG: Animation frame: {}: {frame_delay}ms ({})",
                self.current, loops
            ),
        );
        self.current_delay = 0.;
        self.current += 1;

        if self.frames.len() != self.current {
            return Some(true);
        }

        if let AnimationLoops::Finite(i) = loops {
            let new_i = NonZeroU32::new(i.get() - 1);
            match new_i {
                Some(new_i) => {
                    *i = new_i;
                    self.current = 0;
                    Some(true)
                }
                None => {
                    self.loops.take();
                    None
                }
            }
        } else {
            self.current = 0;
            Some(true)
        }
    }

    /// Returns a non-owning [`Image`] shallow copy of the current frame's image buffer.
    pub fn get_frame(&self) -> ManuallyDrop<Image> {
        let texture = self.frames[self.current].buffer();
        let image = unsafe {
            Image::from_raw(ffi::Image {
                data: texture.as_bytes().as_ptr() as *mut c_void,
                width: texture.width() as i32,
                height: texture.height() as i32,
                mipmaps: 1,
                format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
            })
        };

        ManuallyDrop::new(image)
    }
}

pub struct FoximgImage {
    texture: Texture2D,
    animation: Option<FoximgImageAnimated>,

    rotation: f32,
    width_mult: i32,
    height_mult: i32,
}

impl FoximgImage {
    /// Update the image. This will do nothing for static images, but update the frames of an animated
    /// image when appropriate.
    pub fn update_texture(&mut self, rl: &RaylibHandle) {
        if let Some(ref mut animation) = self.animation {
            let new_state = animation.update_frame(rl);
            if new_state == Some(true) {
                let new_image = animation.get_frame();
                // I don't want to bother with turning new_image.data into a validated u8 slice, only
                // for Texture::update_texture to validate it once again. So I just use the unsafe
                // FFI.
                unsafe {
                    ffi::UpdateTexture(*self.texture, new_image.data);
                }
            } else if new_state.is_none() {
                self.animation.take();
                rl.trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: Animation stopped");
            }
        }
    }

    pub fn width(&self) -> i32 {
        self.texture.width()
    }

    pub fn height(&self) -> i32 {
        self.texture.height()
    }

    pub fn draw_center_scaled(
        &self,
        d: &mut impl RaylibDraw,
        screen_width: f32,
        screen_height: f32,
        scale: f32,
    ) {
        let pos_offset = if let Some(ref animation) = self.animation {
            rvec2(
                animation.frames[animation.current].left(),
                animation.frames[animation.current].top(),
            ) * scale
        } else {
            rvec2(0, 0)
        };

        d.draw_texture_pro(
            &self.texture,
            rrect(
                0,
                0,
                self.width() * self.width_mult,
                self.height() * self.height_mult,
            ),
            rrect(
                screen_width / 2. + pos_offset.x,
                screen_height / 2. + pos_offset.y,
                self.width().as_f32() * scale,
                self.height().as_f32() * scale,
            ),
            rvec2(self.width() / 2, self.height() / 2) * scale,
            self.rotation,
            Color::WHITE,
        );
    }

    pub fn draw_manipulation_info(
        &self,
        d: &mut impl RaylibDraw,
        resources: &FoximgResources,
        style: &FoximgStyle,
        screen_width: f32,
        screen_height: f32,
    ) {
        const SYMBOL_SIDE: i32 = 64;
        const SYMBOL_PADDING: i32 = 10;

        let flipped_horizontal = self.width_mult == -1;
        let flipped_vertical = self.height_mult == -1;
        let flip = &resources.flip;
        let accent = style.accent;

        if flipped_horizontal {
            d.draw_texture_ex(
                flip,
                rvec2(SYMBOL_PADDING, screen_height - SYMBOL_PADDING as f32),
                -90.,
                1.,
                accent,
            );
        }
        if flipped_vertical {
            d.draw_texture(
                flip,
                if flipped_horizontal {
                    SYMBOL_PADDING * 2 + SYMBOL_SIDE
                } else {
                    SYMBOL_PADDING
                },
                screen_height as i32 - SYMBOL_SIDE - SYMBOL_PADDING,
                accent,
            );
        }

        /// The width of the whitespace pixels on the right side of the yudit 0.
        const TEXT_RIGHT_OFFSET: f32 = 9.;
        /// The width of the whitespace pixels around each side of the flip texture.
        const FLIP_OFFSET: f32 = 2.;

        if self.rotation != 0. {
            let text = self.rotation.to_string();
            let yudit = &resources.yudit;
            let text_width = yudit.measure_text(&text, SYMBOL_SIDE as f32, 1.).x;

            d.draw_text_ex(
                yudit,
                &text,
                rvec2(
                    screen_width - text_width - (SYMBOL_PADDING * 2) as f32 + TEXT_RIGHT_OFFSET,
                    screen_height - SYMBOL_SIDE as f32 - FLIP_OFFSET,
                ),
                SYMBOL_SIDE as f32,
                1.,
                accent,
            );
        }
    }
}

pub struct FoximgImages {
    images: Vec<Weak<RefCell<FoximgImage>>>,
    paths: Vec<PathBuf>,
    images_loader: Vec<FoximgImageLoader>,
    images_failed: Vec<bool>,
    current: usize,
    current_images: CircularBuffer<64, Rc<RefCell<FoximgImage>>>,
}

impl FoximgImages {
    pub(self) fn new(
        paths: Vec<PathBuf>,
        images_loader: Vec<FoximgImageLoader>,
        current: usize,
    ) -> Self {
        let mut images = Vec::with_capacity(paths.len());
        (0..paths.len()).for_each(|_| images.push(Weak::new()));

        Self {
            images,
            images_loader,
            images_failed: vec![false; paths.len()],
            current_images: CircularBuffer::new(),
            paths,
            current,
        }
    }

    pub fn img_path(&self) -> &Path {
        &self.paths[self.current]
    }

    /// Returns whether the current image failed to load.
    pub fn img_failed(&self) -> bool {
        self.images_failed[self.current]
    }

    pub fn img_get(
        &mut self,
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
    ) -> Option<Rc<RefCell<FoximgImage>>> {
        if self.img_failed() {
            return None;
        }

        match self.images[self.current].upgrade() {
            Some(texture) => Some(texture),
            None => {
                match self.images_loader[self.current](rl, rl_thread, &self.paths[self.current]) {
                    Ok(texture) => {
                        self.images[self.current] = Rc::downgrade(&texture);
                        self.current_images.push_back(texture.clone());

                        Some(texture)
                    }
                    Err(e) => {
                        self.images_failed[self.current] = true;
                        rl.trace_log(
                            TraceLogLevel::LOG_ERROR,
                            &format!("FOXIMG: Failed to load image: {e}"),
                        );

                        None
                    }
                }
            }
        }
    }

    /// Do something mutably with the current image. Calls the closure only if the current image can
    /// be initialized or got. Use this only if you don't care about handling what happens when the
    /// image is failed. Otherwise, prefer to use `img_get`.
    pub fn img_with(
        &mut self,
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        f: impl FnOnce(RefMut<'_, FoximgImage>),
    ) {
        let Some(image) = self.img_get(rl, rl_thread) else {
            return;
        };

        let image = image.borrow_mut();
        f(image);
    }

    pub fn can_inc(&self) -> bool {
        self.current < self.paths.len() - 1
    }

    pub fn can_dec(&self) -> bool {
        self.current > 0
    }

    pub fn img_current_string(&self) -> String {
        format!("[{} of {}]", self.current + 1, self.paths.len())
    }

    pub fn update_titlebar_and_log(
        &self,
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
    ) {
        let title = format!("{} {} - {path:?}", Foximg::TITLE, self.img_current_string());
        rl.set_window_title(rl_thread, &title);
        rl.trace_log(TraceLogLevel::LOG_INFO, &format!("FOXIMG: {path:?} opened"));
    }

    pub fn inc(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        if self.can_inc() {
            self.current += 1;
            self.update_titlebar_and_log(rl, rl_thread, self.img_path());
        }
    }

    pub fn dec(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        if self.can_dec() {
            self.current -= 1;
            self.update_titlebar_and_log(rl, rl_thread, self.img_path());
        }
    }

    pub fn flip_horizontal(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| img.width_mult = -img.width_mult);
    }

    pub fn flip_vertical(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| img.height_mult = -img.height_mult);
    }

    pub fn rotate_n1(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| {
            img.rotation -= 1.;
            if img.rotation == -1. {
                img.rotation = 359.;
            }
        });
    }

    pub fn rotate_1(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| {
            img.rotation += 1.;
            if img.rotation == 360. {
                img.rotation = 0.;
            }
        });
    }

    pub fn rotate_n90(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| {
            let rot_mod90 = img.rotation % 90.;
            img.rotation -= if rot_mod90 == 0. { 90. } else { rot_mod90 };
            if img.rotation == -90. {
                img.rotation = 270.;
            }
        });
    }

    pub fn rotate_90(&mut self, rl: &mut RaylibHandle, rl_thread: &RaylibThread) {
        self.img_with(rl, rl_thread, |mut img| {
            img.rotation += 90. - img.rotation % 90.;
            if img.rotation == 360. {
                img.rotation = 0.;
            }
        });
    }
}

/// Intermediate struct that helps with loading folders into Foximg galleries.
struct FoximgFolder<'a> {
    f: &'a mut Foximg,
    path: &'a Path,
    folder: Option<&'a Path>,
    paths: Vec<PathBuf>,
    images_loader: Vec<FoximgImageLoader>,
    current: Option<usize>,
}

impl<'a> FoximgFolder<'a> {
    /// Create a new `FoximgFolder`. Takes in a path to a single image. Its directory will be figured
    /// out from it.
    pub fn new(f: &'a mut Foximg, path: &'a Path) -> Self {
        Self {
            f,
            path,
            folder: path.parent(),
            paths: vec![],
            images_loader: vec![],
            current: None,
        }
    }

    fn skip_reread(&mut self) -> Option<FoximgImages> {
        if let Some(ref mut images) = self.f.images {
            if self.folder.is_some()
                && images.paths.first().and_then(|path| path.parent()) == self.folder
            {
                self.f.rl.trace_log(
                    TraceLogLevel::LOG_INFO,
                    &format!(
                        "FOXIMG: Searching through already loaded gallery for {:?}",
                        self.path
                    ),
                );

                if let Some(current) = images
                    .paths
                    .iter()
                    .enumerate()
                    .find(|(_, path)| *path == self.path)
                    .map(|(i, _)| i)
                {
                    images.current = current;
                    return self.f.images.take();
                }
                self.f.rl.trace_log(
                    TraceLogLevel::LOG_INFO,
                    &format!("FOXIMG: Failed to find {:?}. Re-reading folder", self.path),
                );
            }
        }

        None
    }

    /// Creates an iterator over `folder` if it's `Some` or if it's accessible.
    fn get_folder_iter(&self) -> anyhow::Result<ReadDir> {
        self.folder.map_or_else(
            || Err(anyhow::anyhow!("File does not have a directory",)),
            |folder| folder.read_dir().map_err(anyhow::Error::from),
        )
    }

    /// Push a valid image and increment `i`.
    fn push_img(&mut self, i: &mut usize, current_path: PathBuf, loader: FoximgImageLoader) {
        if current_path == self.path {
            self.current = Some(*i);
        }

        *i += 1;
        self.paths.push(current_path);
        self.images_loader.push(loader);
    }

    /// Iterates through the folder and pushes any images it can. Returns how many images it pushed.
    fn push_images(&mut self, folder_iter: ReadDir) -> usize {
        let mut i = 0;
        for file in folder_iter {
            let file = match file {
                Ok(file) => file,
                Err(e) => {
                    self.f
                        .rl
                        .trace_log(TraceLogLevel::LOG_WARNING, "FOXIMG: Couldn't open file:");
                    self.f
                        .rl
                        .trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
                    continue;
                }
            };

            let file_type = match file.file_type() {
                Ok(file_type) => file_type,
                Err(e) => {
                    self.f.rl.trace_log(
                        TraceLogLevel::LOG_WARNING,
                        "FOXIMG: Couldn't get file type:",
                    );
                    self.f
                        .rl
                        .trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
                    continue;
                }
            };

            if !file_type.is_file() {
                continue;
            }

            let current_path = file.path();
            let Some(ext) = current_path.extension() else {
                continue;
            };

            let ext = ext.to_ascii_lowercase();
            let ext = ext.to_str();

            match ext {
                Some("bmp") | Some("jpg") | Some("jpeg") | Some("jpe") | Some("jif")
                | Some("jfif") | Some("jfi") | Some("dds") | Some("hdr") | Some("ico")
                | Some("qoi") | Some("tiff") | Some("pgm") | Some("pbm") | Some("ppm")
                | Some("pnm") | Some("exr") => {
                    self.push_img(&mut i, current_path, FoximgImage::new_dynamic);
                }
                Some("apng") | Some("png") => {
                    self.push_img(&mut i, current_path, FoximgImage::new_png)
                }
                Some("webp") => self.push_img(&mut i, current_path, FoximgImage::new_webp),
                Some("gif") => self.push_img(&mut i, current_path, FoximgImage::new_gif),
                _ => (),
            }
        }
        i
    }

    /// Gets the closest image alphabetically to `path` if it points to an invalid image file. Searches
    /// through `paths` by calling parameter `search_by`. The return value of an `Err` must be the
    /// erroneous result of a binary search, that contains the index where a matching element could
    /// be inserted while maintaining a;lphabetical order.
    fn get_closest_image_alphabetically(&self) -> Option<usize> {
        self.f.rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!(
                "File {:?} isn't a valid image. Loading closest image alphabetically",
                self.path
            ),
        );
        self.paths
            .binary_search_by(|other: &PathBuf| {
                <PathBuf as AsRef<Path>>::as_ref(other).cmp(self.path)
            })
            .err()
    }

    /// Loads the folder into the gallery. This will return `Err` in case:
    /// - `path` doesn't lie inside a directory
    /// - An IO error
    /// - The folder doesn't have any valid images.
    pub fn load(mut self) -> anyhow::Result<FoximgImages> {
        if let Some(images) = self.skip_reread() {
            return Ok(images);
        }

        let folder_iter = self.get_folder_iter()?;
        let i = self.push_images(folder_iter);

        if i > 0 {
            let current = self
                .current
                .or_else(|| self.get_closest_image_alphabetically())
                .unwrap_or_default();
            let images = FoximgImages::new(self.paths, self.images_loader, current);

            self.f.rl.trace_log(
                TraceLogLevel::LOG_INFO,
                &format!(
                    "FOXIMG: Loaded {:?} successfully with {i} images.",
                    self.folder.unwrap_or(Path::new(""))
                ),
            );
            Ok(images)
        } else {
            Err(anyhow::anyhow!("No images could be loaded from the folder"))
        }
    }
}

impl Foximg {
    fn try_load_folder(&mut self, path: &Path) -> anyhow::Result<()> {
        let path = path.canonicalize()?;
        let images = FoximgFolder::new(self, &path).load()?;

        images.update_titlebar_and_log(&mut self.rl, &self.rl_thread, images.img_path());
        self.images = Some(images);
        Ok(())
    }

    pub fn load_folder(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Err(e) = self.try_load_folder(path) {
            self.rl.trace_log(
                TraceLogLevel::LOG_ERROR,
                &format!("FOXIMG: Could not open {path:?}: {e}"),
            );
        }
    }
}
