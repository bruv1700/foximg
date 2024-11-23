use std::{
    cell::RefCell,
    error::Error,
    fs::File,
    io::BufReader,
    marker::PhantomData,
    ops::Not,
    os::raw::c_void,
    path::{Path, PathBuf},
    ptr::NonNull,
    rc::Rc,
};

use anyhow::anyhow;
use image::{
    codecs::{gif::GifDecoder, webp::WebPDecoder},
    error::{DecodingError, ImageFormatHint, UnsupportedError, UnsupportedErrorKind},
    AnimationDecoder, ColorType, DynamicImage, EncodableLayout, ExtendedColorType, Frame, Frames,
    ImageDecoder, ImageError, ImageReader, ImageResult, RgbaImage,
};
use raylib::prelude::*;

use crate::{foximg_error, Foximg};

struct FoximgImageTexturesContext<'imgs, const MAX: usize = 64> {
    textures: [*mut FoximgImageTexture; MAX],
    len: usize,
    marker: PhantomData<&'imgs FoximgImages<'imgs>>,
}

impl<const MAX: usize> FoximgImageTexturesContext<'_, MAX> {
    pub fn new() -> Self {
        let textures = [std::ptr::null_mut(); MAX];
        Self {
            textures,
            len: 0,
            marker: PhantomData,
        }
    }

    pub fn push(&mut self, value: NonNull<FoximgImageTexture>) {
        if self.len == MAX {
            unsafe { self.textures[0].as_mut() }.unwrap().uninit();
            self.textures.copy_within(1.., 0);
            self.textures[MAX - 1] = value.as_ptr();
        } else {
            self.textures[self.len] = value.as_ptr();
            self.len += 1;
        }
    }
}

struct FoximgImageStaticDecoder<'a> {
    dyn_image: DynamicImage,
    file: &'a Path,
}

impl<'a> FoximgImageStaticDecoder<'a> {
    pub fn new(file: &'a Path) -> ImageResult<Self> {
        let reader = BufReader::new(File::open(file)?);
        let dyn_image = ImageReader::new(reader).with_guessed_format()?.decode()?;
        Ok(Self { dyn_image, file })
    }

    fn load_image(self) -> ImageResult<Image> {
        use ffi::PixelFormat::*;
        use DynamicImage::*;
        let error_format = |kind: UnsupportedErrorKind| {
            Err(image::ImageError::Unsupported(
                UnsupportedError::from_format_and_kind(
                    ImageFormatHint::PathExtension(self.file.extension().unwrap().into()),
                    kind,
                ),
            ))
        };
        let unsupported_format =
            |color_type: ExtendedColorType| error_format(UnsupportedErrorKind::Color(color_type));
        let unkown_format = || {
            let bpp_u32 = self.dyn_image.as_bytes().len() as u32 / self.dyn_image.width()
                * self.dyn_image.height();
            let bpp: Result<u8, _> = bpp_u32.try_into();
            match bpp {
                Ok(bpp) => unsupported_format(ExtendedColorType::Unknown(bpp)),
                Err(_) => error_format(UnsupportedErrorKind::GenericFeature(format!(
                    "Color formats with more than 255 BPP not supported ({bpp_u32})"
                ))),
            }
        };
        let image = ffi::Image {
            data: self.dyn_image.as_bytes().as_ptr() as *mut c_void,
            width: self.dyn_image.width() as i32,
            height: self.dyn_image.height() as i32,
            mipmaps: 1,
            format: match self.dyn_image {
                ImageRgb8(_) => PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32,
                ImageRgba8(_) => PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
                ImageRgb16(_) => PIXELFORMAT_UNCOMPRESSED_R16G16B16 as i32,
                ImageRgba16(_) => PIXELFORMAT_UNCOMPRESSED_R16G16B16A16 as i32,
                ImageRgb32F(_) => PIXELFORMAT_UNCOMPRESSED_R32G32B32 as i32,
                ImageRgba32F(_) => PIXELFORMAT_UNCOMPRESSED_R32G32B32A32 as i32,
                ImageLuma8(_) => PIXELFORMAT_UNCOMPRESSED_GRAYSCALE as i32,
                ImageLumaA8(_) => PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA as i32,
                ImageLuma16(_) => return unsupported_format(ExtendedColorType::L16),
                ImageLumaA16(_) => return unsupported_format(ExtendedColorType::La16),
                _ => return unkown_format(),
            },
        };

        std::mem::forget(self.dyn_image);
        Ok(unsafe { Image::from_raw(image) })
    }

    pub fn init_texture(self) -> ImageResult<ffi::Texture> {
        let image = self.load_image()?;
        Ok(unsafe { ffi::LoadTextureFromImage(*image) })
    }
}

struct FoximgImageAnimated {
    frames: Vec<Frame>,
    idx: usize,
    current_delay: f32,
}

impl FoximgImageAnimated {
    pub fn new(frames_iter: Frames) -> ImageResult<Self> {
        Ok(Self {
            frames: frames_iter.collect_frames()?,
            idx: 0,
            current_delay: 0.,
        })
    }

    pub fn get_image(&self) -> ffi::Image {
        let texture = self.frames[self.idx].buffer();
        ffi::Image {
            data: texture.as_bytes().as_ptr() as *mut c_void,
            width: texture.width() as i32,
            height: texture.height() as i32,
            mipmaps: 1,
            format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
        }
    }

    pub fn update_frame(&mut self) -> bool {
        self.current_delay += unsafe { ffi::GetFrameTime() } * 1000.;
        if self.current_delay > self.frames[self.idx].delay().numer_denom_ms().0 as f32 {
            self.current_delay = 0.;
            self.idx += 1;
            if self.frames.len() == self.idx {
                self.idx = 0;
            }

            true
        } else {
            false
        }
    }
}

enum FoximgImageWebp {
    Static,
    Animated(FoximgImageAnimated),
}

struct FoximgImageWebpDecoder<'a> {
    webp: &'a mut FoximgImageWebp,
    static_webp: Option<(RgbaImage, ColorType)>,
}

impl<'a> FoximgImageWebpDecoder<'a> {
    pub fn new(webp: &'a mut Option<FoximgImageWebp>, file: &'a Path) -> ImageResult<Self> {
        let reader = BufReader::new(File::open(file)?);
        let decoder = WebPDecoder::new(reader)?;
        let static_webp: Option<(RgbaImage, ColorType)>;

        *webp = Some(match decoder.has_animation() {
            true => {
                let frames_iter = decoder.into_frames();
                static_webp = None;
                FoximgImageWebp::Animated(FoximgImageAnimated::new(frames_iter)?)
            }
            false => {
                fn decoding_err<E>(err: E) -> ImageError
                where
                    E: Into<Box<dyn Error + Send + Sync>>,
                {
                    ImageError::Decoding(DecodingError::new(
                        ImageFormatHint::Exact(image::ImageFormat::WebP),
                        err,
                    ))
                }

                let buf_size: usize = decoder.total_bytes().try_into().map_err(decoding_err)?;
                let mut buf = Vec::with_capacity(buf_size);
                unsafe { buf.set_len(buf_size) };

                let (w, h) = decoder.dimensions();
                let color_type = decoder.color_type();
                let color_mult = if color_type == ColorType::Rgb8 { 2 } else { 3 };

                decoder.read_image(buf.as_mut_slice())?;
                buf.reserve_exact(buf_size * color_mult);
                unsafe { buf.set_len(buf_size * color_mult) };

                let buf_size = buf.len();
                static_webp = Some((
                    RgbaImage::from_vec(w, h, buf).ok_or_else(|| {
                        decoding_err(anyhow!(
                            "Buffer is not big enough\n - Buffer length: {}\n - Necessary length: {w}x{h}x{color_mult}BPP = {}",
                            buf_size, w * h * color_mult as u32
                        ))
                    })?,
                    color_type,
                ));
                FoximgImageWebp::Static
            }
        });

        let webp = unsafe { webp.as_mut().unwrap_unchecked() };
        Ok(Self { webp, static_webp })
    }

    fn init_texture_static(self) -> ffi::Texture {
        match self.static_webp {
            Some((mut buf, color_type)) => {
                let image = ffi::Image {
                    data: buf.as_mut_ptr() as *mut c_void,
                    width: buf.width() as i32,
                    height: buf.height() as i32,
                    mipmaps: 1,
                    format: if color_type == ColorType::Rgb8 {
                        ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32
                    } else {
                        ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32
                    },
                };

                std::mem::forget(buf);
                let image = unsafe { Image::from_raw(image) };
                unsafe { ffi::LoadTextureFromImage(*image) }
            }
            None => unreachable!("Can't initialize a static WebP from an animated one"),
        }
    }

    pub fn init_texture(self) -> ffi::Texture {
        match self.webp {
            FoximgImageWebp::Static => self.init_texture_static(),
            FoximgImageWebp::Animated(animation) => {
                let image = animation.get_image();
                unsafe { ffi::LoadTextureFromImage(image) }
            }
        }
    }
}

struct FoximgImageGifDecoder<'a> {
    gif: &'a mut FoximgImageAnimated,
}

impl<'a> FoximgImageGifDecoder<'a> {
    pub fn new(gif: &'a mut Option<FoximgImageAnimated>, file: &'a Path) -> ImageResult<Self> {
        let reader = BufReader::new(File::open(file)?);
        let decoder = GifDecoder::new(reader)?;
        let frames_iter = decoder.into_frames();

        *gif = Some(FoximgImageAnimated::new(frames_iter)?);

        let gif = unsafe { gif.as_mut().unwrap_unchecked() };
        Ok(Self { gif })
    }

    pub fn init_texture(self) -> ffi::Texture {
        let image = self.gif.get_image();
        unsafe { ffi::LoadTextureFromImage(image) }
    }
}

impl<'a> From<&'a mut FoximgImageAnimated> for FoximgImageGifDecoder<'a> {
    fn from(gif: &'a mut FoximgImageAnimated) -> Self {
        Self { gif }
    }
}

enum FoximgImageTextureType {
    Static,
    Webp(Option<FoximgImageWebp>),
    Gif(Option<FoximgImageAnimated>),
}

impl FoximgImageTextureType {
    fn init_texture(&mut self, file: &Path) -> ImageResult<ffi::Texture> {
        match self {
            FoximgImageTextureType::Static => {
                let decoder = FoximgImageStaticDecoder::new(file)?;
                decoder.init_texture()
            }
            FoximgImageTextureType::Webp(ref mut webp) => {
                let decoder = FoximgImageWebpDecoder::new(webp, file)?;
                Ok(decoder.init_texture())
            }
            FoximgImageTextureType::Gif(ref mut gif) => {
                let decoder = FoximgImageGifDecoder::new(gif, file)?;
                Ok(decoder.init_texture())
            }
        }
    }
}

struct FoximgImageTexture {
    texture: ffi::Texture,
    image_type: FoximgImageTextureType,
}

impl FoximgImageTexture {
    fn new(image_type: FoximgImageTextureType) -> Self {
        let texture = ffi::Texture {
            id: 0,
            width: 0,
            height: 0,
            mipmaps: 0,
            format: 0,
        };
        Self {
            texture,
            image_type,
        }
    }

    fn init(
        &mut self,
        file: &Path,
        _: &RaylibThread,
        texture_context: Rc<RefCell<FoximgImageTexturesContext>>,
    ) {
        match self.image_type.init_texture(file) {
            Ok(texture) => {
                self.texture = texture;
                texture_context.borrow_mut().push(self.into());
            }
            Err(e) => foximg_error::show(&format!("Couldn't load image:\n - {e}")),
        }
    }

    fn update(&mut self) {
        if let FoximgImageTextureType::Gif(Some(ref mut frames))
        | FoximgImageTextureType::Webp(Some(FoximgImageWebp::Animated(ref mut frames))) =
            self.image_type
        {
            if frames.update_frame() {
                let image = frames.get_image();
                unsafe {
                    ffi::UpdateTexture(self.texture, image.data);
                }
            }
        }
    }

    fn uninit(&mut self) {
        unsafe { ffi::UnloadTexture(self.texture) };
        self.texture.id = 0;
        if let FoximgImageTextureType::Gif(ref mut gif) = self.image_type {
            gif.take();
        } else if let FoximgImageTextureType::Webp(ref mut webp) = self.image_type {
            webp.take();
        }
    }

    fn is_init(&self) -> bool {
        self.texture.id > 0
    }
}

impl Drop for FoximgImageTexture {
    fn drop(&mut self) {
        self.uninit();
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ScaleMult {
    Normal,
    Inverted,
}

impl ScaleMult {
    pub const fn as_f32(self) -> f32 {
        match self {
            ScaleMult::Normal => 1.,
            ScaleMult::Inverted => -1.,
        }
    }
}

impl Not for ScaleMult {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            ScaleMult::Normal => ScaleMult::Inverted,
            ScaleMult::Inverted => ScaleMult::Normal,
        }
    }
}

pub struct FoximgImage {
    file: PathBuf,
    texture: FoximgImageTexture,
    pub width_mult: ScaleMult,
    pub height_mult: ScaleMult,
    pub rotation: f32,
}

impl FoximgImage {
    fn new(file: PathBuf, image_type: FoximgImageTextureType) -> Self {
        let texture = FoximgImageTexture::new(image_type);
        Self {
            file,
            texture,
            width_mult: ScaleMult::Normal,
            height_mult: ScaleMult::Normal,
            rotation: 0.,
        }
    }

    fn init(
        &mut self,
        rl_thread: &RaylibThread,
        texture_context: Rc<RefCell<FoximgImageTexturesContext>>,
    ) {
        if self.texture.is_init() {
            return;
        }

        self.texture.init(&self.file, rl_thread, texture_context);
    }

    pub fn update(&mut self) {
        self.texture.update();
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn width(&self) -> i32 {
        self.texture.texture.width
    }

    pub fn height(&self) -> i32 {
        self.texture.texture.height
    }

    pub fn rotate_n90(&mut self) {
        self.rotation -= 90.;
        if self.rotation == -90. {
            self.rotation = 270.;
        }
    }

    pub fn rotate_90(&mut self) {
        self.rotation += 90.;
        if self.rotation == 360. {
            self.rotation = 0.;
        }
    }

    pub fn flip_horizontal(&mut self) {
        self.width_mult = !self.width_mult;
    }

    pub fn flip_vertical(&mut self) {
        self.height_mult = !self.height_mult;
    }
}

impl AsRef<ffi::Texture> for FoximgImage {
    fn as_ref(&self) -> &ffi::Texture {
        &self.texture.texture
    }
}

pub struct FoximgImagesMutIncIter<'a, 'imgs> {
    images: &'a mut FoximgImages<'imgs>,
}

impl<'a, 'imgs> FoximgImagesMutIncIter<'a, 'imgs> {
    fn new(images: &'a mut FoximgImages<'imgs>) -> Self {
        FoximgImagesMutIncIter { images }
    }

    pub fn inc_once(self, rl: &RaylibHandle, rl_thread: &RaylibThread) {
        self.images.idx += 1;
        self.images.init(rl, rl_thread);
    }
}

pub struct FoximgImagesMutDecIter<'a, 'imgs> {
    images: &'a mut FoximgImages<'imgs>,
}

impl<'a, 'imgs> FoximgImagesMutDecIter<'a, 'imgs> {
    fn new(images: &'a mut FoximgImages<'imgs>) -> Self {
        FoximgImagesMutDecIter { images }
    }

    pub fn dec_once(self, rl: &RaylibHandle, rl_thread: &RaylibThread) {
        self.images.idx -= 1;
        self.images.init(rl, rl_thread);
    }
}

pub struct FoximgImages<'imgs> {
    images: Vec<FoximgImage>,
    folder: PathBuf,
    idx: usize,
    texture_context: Rc<RefCell<FoximgImageTexturesContext<'imgs>>>,
}

impl<'imgs> FoximgImages<'imgs> {
    fn new(images: Vec<FoximgImage>, folder: &Path, idx: usize, rl: &RaylibHandle) -> Self {
        assert!(
            !images.is_empty(),
            "FoximgImages can't be constructed with an empty Vec<FoximgImage>"
        );
        assert!(
            idx < images.len(),
            "FoximgImages can't be constructed with idx >= images.len() [{idx} >= {}]",
            images.len()
        );

        let folder = folder.to_path_buf();
        let text = format!(
            "FOXIMG: Loaded folder {:?} with {} images",
            folder,
            images.len()
        );

        rl.trace_log(TraceLogLevel::LOG_INFO, &text);
        Self {
            images,
            folder,
            idx,
            texture_context: Rc::new(RefCell::new(FoximgImageTexturesContext::new())),
        }
    }

    fn init(&mut self, rl: &RaylibHandle, rl_thread: &RaylibThread) {
        let texture_context = self.texture_context.clone();
        let image = self.get_mut();
        let display_path = {
            let mut path = format!("{:?}", image.file());
            if cfg!(windows) {
                path = path.replace(r"\\", r"\");
            }
            path
        };

        rl.set_window_title(rl_thread, &format!("foximg - {display_path}"));
        image.init(rl_thread, texture_context);
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!("FOXIMG: Loaded {display_path}"),
        );
    }

    pub fn get(&self) -> &FoximgImage {
        &self.images[self.idx]
    }

    pub fn get_mut(&mut self) -> &mut FoximgImage {
        &mut self.images[self.idx]
    }

    pub fn can_inc(&self) -> bool {
        self.idx < self.images.len() - 1
    }

    pub fn can_dec(&self) -> bool {
        self.idx > 0
    }

    #[must_use = "use can_inc to ignore return value"]
    pub fn try_inc_iter<'a>(&'a mut self) -> Option<FoximgImagesMutIncIter<'a, 'imgs>> {
        match self.can_inc() {
            true => Some(FoximgImagesMutIncIter::new(self)),
            false => None,
        }
    }

    #[must_use = "use can_dec to ignore return value"]
    pub fn try_dec_iter<'a>(&'a mut self) -> Option<FoximgImagesMutDecIter<'a, 'imgs>> {
        match self.can_dec() {
            true => Some(FoximgImagesMutDecIter::new(self)),
            false => None,
        }
    }
}

impl Foximg<'_> {
    fn load_folder(&mut self, folder: &Path, path: &Path) {
        self.rl
            .trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: === Loading Folder ===");

        let folder_loaded_err = |e: &str| {
            self.rl.trace_log(
                TraceLogLevel::LOG_ERROR,
                &format!("Couldn't open folder: {e}"),
            )
        };
        let cmp_img_paths = |image: &FoximgImage| image.file.cmp(&path.to_path_buf());
        let folder_loaded_ok = || {
            self.rl
                .trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: === Loaded Folder ===")
        };
        let folder_iter = match folder.read_dir() {
            Ok(iter) => iter,
            Err(e) => return folder_loaded_err(&e.to_string()),
        };

        if let Some(ref mut images) = self.images {
            if images.folder == folder {
                if let Ok(idx) = images.images.binary_search_by(cmp_img_paths) {
                    images.idx = idx;
                    return folder_loaded_ok();
                }
            }
            self.images.take();
        }

        let mut i = 0;
        let mut idx = None;
        let mut images = vec![];
        for file in folder_iter {
            let file = match file {
                Ok(file) => file,
                Err(e) => {
                    self.rl.trace_log(
                        TraceLogLevel::LOG_WARNING,
                        &format!("FOXIMG: Couldn't open file: {e}"),
                    );
                    continue;
                }
            };
            let file_type = match file.file_type() {
                Ok(file_type) => file_type,
                Err(e) => {
                    self.rl.trace_log(
                        TraceLogLevel::LOG_WARNING,
                        &format!("FOXIMG: Couldn't get file type: {e}"),
                    );
                    continue;
                }
            };

            if !file_type.is_file() {
                continue;
            }

            let file = file.path();
            let ext = match file.extension() {
                Some(str) => str,
                None => continue,
            };
            let ext = ext.to_ascii_lowercase();
            let ext = ext.to_str();
            let push_image = |image_type: FoximgImageTextureType| {
                if file == path {
                    idx = Some(i);
                }
                i += 1;

                images.push(FoximgImage::new(file, image_type));
            };

            match ext {
                Some("png") | Some("bmp") | Some("tga") | Some("jpg") | Some("jpeg")
                | Some("jpe") | Some("jif") | Some("jfif") | Some("jfi") | Some("dds")
                | Some("hdr") | Some("ico") | Some("qoi") | Some("tiff") | Some("pgm")
                | Some("pbm") | Some("ppm") | Some("pnm") | Some("exr") => {
                    push_image(FoximgImageTextureType::Static)
                }
                Some("webp") => push_image(FoximgImageTextureType::Webp(None)),
                Some("gif") => push_image(FoximgImageTextureType::Gif(None)),
                _ => (),
            }
        }

        if !images.is_empty() {
            if idx.is_none() {
                idx = Some(unsafe {
                    images
                        .binary_search_by(cmp_img_paths)
                        .unwrap_err_unchecked()
                });
            }
            self.images = Some(FoximgImages::new(
                images,
                folder,
                unsafe { idx.unwrap_unchecked() },
                &self.rl,
            ));

            folder_loaded_ok();
        } else {
            folder_loaded_err("no images could be loaded!");
        }
    }

    fn load_img_unchecked(&mut self, path: &Path) {
        let folder = path.parent().unwrap(); // TODO: Deal with files that doesn't have a directory.
        self.load_folder(folder, path);
        if let Some(ref mut images) = self.images {
            images.init(&self.rl, &self.rl_thread);
        }
    }

    pub(crate) fn load_img(&mut self, path: &Path) {
        if !path.exists() {
            return self
                .rl
                .trace_log(TraceLogLevel::LOG_ERROR, "File does not exist!");
        }
        self.load_img_unchecked(path)
    }

    pub(crate) fn load_dropped(&mut self) {
        let files = self.rl.load_dropped_files();
        let mut files = unsafe { std::slice::from_raw_parts(files.paths, files.count as usize) }
            .iter()
            .map(|f| unsafe { std::ffi::CStr::from_ptr(*f) }.to_str().unwrap());

        if let Some(path) = files.next() {
            self.load_img_unchecked(Path::new(path));
        }
    }
}
