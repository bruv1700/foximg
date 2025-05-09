//! Functions that initialize a `FoximgImage`.
#![allow(clippy::uninit_vec)]

use std::{
    cell::RefCell,
    ffi::{OsStr, c_void},
    fs::File,
    io::{BufReader, Cursor},
    mem::ManuallyDrop,
    path::Path,
    rc::Rc,
};

use image::{
    AnimationDecoder, ColorType, DynamicImage, ExtendedColorType, ImageDecoder, ImageError,
    ImageFormat, ImageReader, ImageResult, RgbaImage,
    error::{ImageFormatHint, UnsupportedError, UnsupportedErrorKind},
};
use raylib::prelude::*;

use crate::images::foximg_png_decoder::PngDecoder;

use super::{
    AnimationLoops, AnimationLoopsDecoder, FoximgImage, FoximgImageAnimated,
    foximg_gif_decoder::GifDecoder, foximg_png_decoder::ApngDecoder,
    foximg_webp_decoder::WebPDecoder,
};

/// Represents a function that constructs a `FoximgImage.`
pub type FoximgImageLoader =
    fn(&mut RaylibHandle, &RaylibThread, &Path) -> anyhow::Result<Rc<RefCell<FoximgImage>>>;

struct FoximgDynamicImage<'a> {
    ext: &'a OsStr,
    dynamic_image: DynamicImage,
}

impl<'a> FoximgDynamicImage<'a> {
    pub fn new(path: &'a Path) -> ImageResult<Self> {
        let reader = BufReader::new(File::open(path)?);
        let image_reader = ImageReader::new(reader).with_guessed_format()?;
        let ext = path.extension().unwrap_or_default();

        let dynamic_image = image_reader.decode()?;
        Ok(Self { ext, dynamic_image })
    }

    fn unsupported_format(&self, color_type: ExtendedColorType) -> ImageError {
        image::ImageError::Unsupported(UnsupportedError::from_format_and_kind(
            ImageFormatHint::PathExtension(self.ext.into()),
            UnsupportedErrorKind::Color(color_type),
        ))
    }

    fn unknown_format(&self) -> anyhow::Error {
        let bpp_u32 = self.dynamic_image.as_bytes().len() as u32 / self.dynamic_image.width()
            * self.dynamic_image.height();
        let bpp: Result<u8, _> = (bpp_u32 * 8).try_into();
        match bpp {
            Ok(bpp) => self
                .unsupported_format(ExtendedColorType::Unknown(bpp))
                .into(),
            Err(_) => {
                anyhow::anyhow!("Color formats with more than 255 BPP not supported ({bpp_u32})")
            }
        }
    }

    pub fn decode(self) -> anyhow::Result<Image> {
        use DynamicImage::*;
        use ffi::PixelFormat::*;

        let image = ffi::Image {
            data: self.dynamic_image.as_bytes().as_ptr() as *mut c_void,
            width: self.dynamic_image.width() as i32,
            height: self.dynamic_image.height() as i32,
            mipmaps: 1,
            format: match self.dynamic_image {
                ImageRgb8(_) => PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32,
                ImageRgba8(_) => PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
                ImageRgb16(_) => PIXELFORMAT_UNCOMPRESSED_R16G16B16 as i32,
                ImageRgba16(_) => PIXELFORMAT_UNCOMPRESSED_R16G16B16A16 as i32,
                ImageRgb32F(_) => PIXELFORMAT_UNCOMPRESSED_R32G32B32 as i32,
                ImageRgba32F(_) => PIXELFORMAT_UNCOMPRESSED_R32G32B32A32 as i32,
                ImageLuma8(_) => PIXELFORMAT_UNCOMPRESSED_GRAYSCALE as i32,
                ImageLumaA8(_) => PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA as i32,
                ImageLuma16(_) => anyhow::bail!(self.unsupported_format(ExtendedColorType::L16)),
                ImageLumaA16(_) => anyhow::bail!(self.unsupported_format(ExtendedColorType::La16)),
                _ => anyhow::bail!(self.unknown_format()),
            },
        };

        std::mem::forget(self.dynamic_image);
        Ok(unsafe { Image::from_raw(image) })
    }
}

impl FoximgImage {
    fn new(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        image: &Image,
        animation: Option<FoximgImageAnimated>,
    ) -> anyhow::Result<FoximgImage> {
        Ok(FoximgImage {
            texture: rl.load_texture_from_image(rl_thread, image)?,
            animation,
            rotation: 0.,
            width_mult: 1,
            height_mult: 1,
        })
    }

    fn log_loader(rl: &RaylibHandle, path: &Path, exts: &[&str]) {
        rl.trace_log(
            TraceLogLevel::LOG_DEBUG,
            &format!("FOXIMG: Loading {path:?} as {exts:?} image"),
        );
    }

    fn log_static(rl: &RaylibHandle, path: &Path) {
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!("FOXIMG: {path:?} loaded successfully"),
        );
    }

    fn log_animated(rl: &RaylibHandle, path: &Path, animation_len: usize, loops: AnimationLoops) {
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!("FOXIMG: {path:?} loaded successfully:"),
        );
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!("    > Frames:     {animation_len}"),
        );
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            &format!("    > Iterations: {loops}"),
        );
    }

    pub fn new_dynamic(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        const EXTS: &[&str] = &[
            "bmp", "jpg", "jpeg", "jpe", "jif", "jfif", "jfi", "dds", "hdr", "ico", "qoi", "tiff",
            "pgm", "pbm", "ppm", "pnm", "exr",
        ];

        Self::log_loader(rl, path, EXTS);

        let dynamic_image = match FoximgDynamicImage::new(path) {
            Ok(dynamic_image) => dynamic_image,
            Err(ImageError::Unsupported(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::Png) =>
            {
                return Self::new_png(rl, rl_thread, path);
            }
            Err(ImageError::Unsupported(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::WebP) =>
            {
                return Self::new_webp(rl, rl_thread, path);
            }
            Err(ImageError::Unsupported(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::Gif) =>
            {
                return Self::new_gif(rl, rl_thread, path);
            }
            Err(e) => anyhow::bail!(e),
        };

        let image = dynamic_image.decode()?;
        let texture = Self::new(rl, rl_thread, &image, None)?;

        Self::log_static(rl, path);

        Ok(Rc::new(RefCell::new(texture)))
    }

    fn decode_animated<'a>(
        decoder: impl AnimationDecoder<'a> + AnimationLoopsDecoder,
    ) -> anyhow::Result<FoximgImageAnimated> {
        let loops = decoder.get_loop_count();
        let frames_iter = decoder.into_frames();
        let animation = FoximgImageAnimated::new(frames_iter, loops)?;

        Ok(animation)
    }

    fn decode_static(decoder: impl ImageDecoder) -> anyhow::Result<Image> {
        use ffi::PixelFormat::*;

        let (w, h) = decoder.dimensions();
        let bpp = decoder.color_type().bytes_per_pixel() as usize;
        let buf_len = decoder.total_bytes().try_into()?;
        let format = match decoder.color_type() {
            ColorType::L8 => PIXELFORMAT_UNCOMPRESSED_GRAYSCALE as i32,
            ColorType::La8 => PIXELFORMAT_UNCOMPRESSED_GRAY_ALPHA as i32,
            ColorType::Rgb8 => PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32,
            ColorType::Rgba8 => PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
            ColorType::Rgb16 => PIXELFORMAT_UNCOMPRESSED_R16G16B16 as i32,
            ColorType::Rgba16 => PIXELFORMAT_UNCOMPRESSED_R16G16B16A16 as i32,
            ColorType::Rgb32F => PIXELFORMAT_UNCOMPRESSED_R32G32B32 as i32,
            ColorType::Rgba32F => PIXELFORMAT_UNCOMPRESSED_R32G32B32A32 as i32,
            color_type => anyhow::bail!(ImageError::Unsupported(
                UnsupportedError::from_format_and_kind(
                    ImageFormatHint::Exact(ImageFormat::Png),
                    UnsupportedErrorKind::Color(color_type.into()),
                )
            )),
        };

        let mut buf: Vec<u8> = Vec::with_capacity(buf_len);
        unsafe { buf.set_len(buf_len) };
        decoder.read_image(buf.as_mut_slice())?;
        buf.reserve_exact(buf_len * bpp);
        unsafe { buf.set_len(buf_len * bpp) };

        let mut image = ManuallyDrop::new(RgbaImage::from_vec(w, h, buf)
        .ok_or_else(|| anyhow::anyhow!(
                "Buffer is not big enough\n - Buffer length: {}\n - Necessary length: {w}x{h}x{bpp}BPP = {}",
                buf_len, w * h * bpp as u32
            )
        )?);

        Ok(unsafe {
            Image::from_raw(ffi::Image {
                data: image.as_mut_ptr() as *mut c_void,
                width: image.width() as i32,
                height: image.height() as i32,
                mipmaps: 1,
                format,
            })
        })
    }

    fn new_apng(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
        decoder: ApngDecoder<BufReader<File>>,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        let animation = Self::decode_animated(decoder)?;
        let animation_len = animation.get_frames_len();
        let loops = animation.get_loops().unwrap();
        let texture = Self::new(rl, rl_thread, &animation.get_frame(), Some(animation))?;

        Self::log_animated(rl, path, animation_len, loops);

        Ok(Rc::new(RefCell::new(texture)))
    }

    fn new_png_static(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
        decoder: PngDecoder<BufReader<File>>,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        let image = Self::decode_static(decoder)?;
        let texture = Self::new(rl, rl_thread, &image, None)?;
        Self::log_static(rl, path);

        Ok(Rc::new(RefCell::new(texture)))
    }

    pub fn new_png(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        const EXTS: &[&str] = &["apng", "png"];

        Self::log_loader(rl, path, EXTS);

        let reader = BufReader::new(File::open(path)?);
        let decoder = match PngDecoder::new(reader) {
            Ok(decoder) => decoder,
            Err(ImageError::Decoding(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::Png) =>
            {
                return Self::new_dynamic(rl, rl_thread, path);
            }
            Err(e) => anyhow::bail!(e),
        };

        if decoder.is_apng()? {
            Self::new_apng(rl, rl_thread, path, decoder.apng()?)
        } else {
            Self::new_png_static(rl, rl_thread, path, decoder)
        }
    }

    fn new_webp_animated(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
        mut decoder: WebPDecoder<BufReader<File>>,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        let bg_color = Color::get_color(
            rl.gui_get_style(GuiControl::DEFAULT, GuiDefaultProperty::BACKGROUND_COLOR) as u32,
        );
        decoder.set_background_color(bg_color)?;

        let animation = Self::decode_animated(decoder)?;
        let animation_len = animation.get_frames_len();
        let loops = animation.get_loops().unwrap();
        let texture = Self::new(rl, rl_thread, &animation.get_frame(), Some(animation))?;

        Self::log_animated(rl, path, animation_len, loops);

        Ok(Rc::new(RefCell::new(texture)))
    }

    fn new_webp_static(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
        decoder: WebPDecoder<BufReader<File>>,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        let image = Self::decode_static(decoder)?;
        let texture = Self::new(rl, rl_thread, &image, None)?;
        Self::log_static(rl, path);

        Ok(Rc::new(RefCell::new(texture)))
    }

    pub fn new_webp(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        Self::log_loader(rl, path, &["webp"]);

        let reader = BufReader::new(File::open(path)?);
        let decoder = match WebPDecoder::new(reader) {
            Ok(decoder) => decoder,
            Err(ImageError::Decoding(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::WebP) =>
            {
                return Self::new_dynamic(rl, rl_thread, path);
            }
            Err(e) => anyhow::bail!(e),
        };

        if decoder.has_animation() {
            Self::new_webp_animated(rl, rl_thread, path, decoder)
        } else {
            Self::new_webp_static(rl, rl_thread, path, decoder)
        }
    }

    pub fn new_gif(
        rl: &mut RaylibHandle,
        rl_thread: &RaylibThread,
        path: &Path,
    ) -> anyhow::Result<Rc<RefCell<FoximgImage>>> {
        Self::log_loader(rl, path, &["gif"]);

        let reader = BufReader::new(File::open(path)?);
        let decoder = match GifDecoder::new(reader) {
            Ok(decoder) => decoder,
            Err(ImageError::Decoding(e))
                if e.format_hint() == ImageFormatHint::Exact(ImageFormat::Gif) =>
            {
                return Self::new_dynamic(rl, rl_thread, path);
            }
            Err(e) => anyhow::bail!(e),
        };

        let animation = Self::decode_animated(decoder)?;
        let frame = animation.get_frame();
        let animation_len = animation.get_frames_len();
        let loops = animation.get_loops().unwrap();

        if animation_len > 1 {
            let texture = Self::new(rl, rl_thread, &frame, Some(animation))?;
            Self::log_animated(rl, path, animation_len, loops);

            Ok(Rc::new(RefCell::new(texture)))
        } else {
            let texture = Self::new(rl, rl_thread, &frame, None)?;
            Self::log_static(rl, path);

            Ok(Rc::new(RefCell::new(texture)))
        }
    }    
}

fn log_resource(rl: &RaylibHandle, resource_name: &str) {
    rl.trace_log(
        TraceLogLevel::LOG_INFO,
        &format!("FOXIMG: Resource \"{resource_name}\" loaded successfully"),
    );
}

pub fn new_resource(
    rl: &mut RaylibHandle,
    rl_thread: &RaylibThread,
    png_bytes: &[u8],
    resource_name: &str,
) -> anyhow::Result<Texture2D> {
    let reader = Cursor::new(png_bytes);
    let decoder = PngDecoder::new(reader)?;
    let image = FoximgImage::decode_static(decoder)?;
    let texture = rl.load_texture_from_image(rl_thread, &image)?;

    self::log_resource(rl, resource_name);

    Ok(texture)
}
