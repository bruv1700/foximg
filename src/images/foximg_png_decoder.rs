//! Copy of the Png decoder source in image-rs 0.25.6 with additional functionality.
//!
//! Decoding and Encoding of PNG Images
//!
//! PNG (Portable Network Graphics) is an image format that supports lossless compression.
//!
//! # Related Links
//! * <http://www.w3.org/TR/PNG/> - The PNG Specification

use std::io::{BufRead, Seek};
use std::num::NonZeroU32;

use png::{BlendOp, DisposeOp};

use image::error::{
    DecodingError, ImageError, ImageResult, LimitError, LimitErrorKind,
    ParameterError, ParameterErrorKind, UnsupportedError, UnsupportedErrorKind,
};
use image::{AnimationDecoder, ImageDecoder, ImageFormat, Pixel};
use image::{ColorType, ExtendedColorType};
use image::{DynamicImage, GenericImage, ImageBuffer, Luma, LumaA, Rgb, Rgba, RgbaImage};
use image::{Frame, Frames};
use image::{GenericImageView, Limits};

use super::{AnimationLoops, AnimationLoopsDecoder};

/// PNG decoder
pub struct PngDecoder<R: BufRead + Seek> {
    color_type: ColorType,
    reader: png::Reader<R>,
    limits: Limits,
}

impl<R: BufRead + Seek> PngDecoder<R> {
    /// Creates a new decoder that decodes from the stream ```r```
    pub fn new(r: R) -> ImageResult<PngDecoder<R>> {
        Self::with_limits(r, Limits::no_limits())
    }

    /// Creates a new decoder that decodes from the stream ```r``` with the given limits.
    pub fn with_limits(r: R, limits: Limits) -> ImageResult<PngDecoder<R>> {
        limits.check_support(&image::LimitSupport::default())?;

        let max_bytes = usize::try_from(limits.max_alloc.unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        let mut decoder = png::Decoder::new_with_limits(r, png::Limits { bytes: max_bytes });
        decoder.set_ignore_text_chunk(true);

        let info = decoder.read_header_info().map_err(error_from_png)?;
        limits.check_dimensions(info.width, info.height)?;

        // By default the PNG decoder will scale 16 bpc to 8 bpc, so custom
        // transformations must be set. EXPAND preserves the default behavior
        // expanding bpc < 8 to 8 bpc.
        decoder.set_transformations(png::Transformations::EXPAND);
        let reader = decoder.read_info().map_err(error_from_png)?;
        let (color_type, bits) = reader.output_color_type();
        let color_type = match (color_type, bits) {
            (png::ColorType::Grayscale, png::BitDepth::Eight) => ColorType::L8,
            (png::ColorType::Grayscale, png::BitDepth::Sixteen) => ColorType::L16,
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => ColorType::La8,
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Sixteen) => ColorType::La16,
            (png::ColorType::Rgb, png::BitDepth::Eight) => ColorType::Rgb8,
            (png::ColorType::Rgb, png::BitDepth::Sixteen) => ColorType::Rgb16,
            (png::ColorType::Rgba, png::BitDepth::Eight) => ColorType::Rgba8,
            (png::ColorType::Rgba, png::BitDepth::Sixteen) => ColorType::Rgba16,

            (png::ColorType::Grayscale, png::BitDepth::One) => {
                return Err(unsupported_color(ExtendedColorType::L1));
            }
            (png::ColorType::GrayscaleAlpha, png::BitDepth::One) => {
                return Err(unsupported_color(ExtendedColorType::La1));
            }
            (png::ColorType::Rgb, png::BitDepth::One) => {
                return Err(unsupported_color(ExtendedColorType::Rgb1));
            }
            (png::ColorType::Rgba, png::BitDepth::One) => {
                return Err(unsupported_color(ExtendedColorType::Rgba1));
            }

            (png::ColorType::Grayscale, png::BitDepth::Two) => {
                return Err(unsupported_color(ExtendedColorType::L2));
            }
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Two) => {
                return Err(unsupported_color(ExtendedColorType::La2));
            }
            (png::ColorType::Rgb, png::BitDepth::Two) => {
                return Err(unsupported_color(ExtendedColorType::Rgb2));
            }
            (png::ColorType::Rgba, png::BitDepth::Two) => {
                return Err(unsupported_color(ExtendedColorType::Rgba2));
            }

            (png::ColorType::Grayscale, png::BitDepth::Four) => {
                return Err(unsupported_color(ExtendedColorType::L4));
            }
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Four) => {
                return Err(unsupported_color(ExtendedColorType::La4));
            }
            (png::ColorType::Rgb, png::BitDepth::Four) => {
                return Err(unsupported_color(ExtendedColorType::Rgb4));
            }
            (png::ColorType::Rgba, png::BitDepth::Four) => {
                return Err(unsupported_color(ExtendedColorType::Rgba4));
            }

            (png::ColorType::Indexed, bits) => {
                return Err(unsupported_color(ExtendedColorType::Unknown(bits as u8)));
            }
        };

        Ok(PngDecoder {
            color_type,
            reader,
            limits,
        })
    }

    /// Returns the gamma value of the image or None if no gamma value is indicated.
    ///
    /// If an sRGB chunk is present this method returns a gamma value of 0.45455 and ignores the
    /// value in the gAMA chunk. This is the recommended behavior according to the PNG standard:
    ///
    /// > When the sRGB chunk is present, [...] decoders that recognize the sRGB chunk but are not
    /// > capable of colour management are recommended to ignore the gAMA and cHRM chunks, and use
    /// > the values given above as if they had appeared in gAMA and cHRM chunks.
    pub fn gamma_value(&self) -> ImageResult<Option<f64>> {
        Ok(self
            .reader
            .info()
            .source_gamma
            .map(|x| f64::from(x.into_scaled()) / 100_000.0))
    }

    /// Turn this into an iterator over the animation frames.
    ///
    /// Reading the complete animation requires more memory than reading the data from the IDAT
    /// frame–multiple frame buffers need to be reserved at the same time. We further do not
    /// support compositing 16-bit colors. In any case this would be lossy as the interface of
    /// animation decoders does not support 16-bit colors.
    ///
    /// If something is not supported or a limit is violated then the decoding step that requires
    /// them will fail and an error will be returned instead of the frame. No further frames will
    /// be returned.
    pub fn apng(self) -> ImageResult<ApngDecoder<R>> {
        Ok(ApngDecoder::new(self))
    }

    /// Returns if the image contains an animation.
    ///
    /// Note that the file itself decides if the default image is considered to be part of the
    /// animation. When it is not the common interpretation is to use it as a thumbnail.
    ///
    /// If a non-animated image is converted into an `ApngDecoder` then its iterator is empty.
    pub fn is_apng(&self) -> ImageResult<bool> {
        Ok(self.reader.info().animation_control.is_some())
    }
}

fn unsupported_color(ect: ExtendedColorType) -> ImageError {
    ImageError::Unsupported(UnsupportedError::from_format_and_kind(
        ImageFormat::Png.into(),
        UnsupportedErrorKind::Color(ect),
    ))
}

impl<R: BufRead + Seek> ImageDecoder for PngDecoder<R> {
    fn dimensions(&self) -> (u32, u32) {
        self.reader.info().size()
    }

    fn color_type(&self) -> ColorType {
        self.color_type
    }

    fn icc_profile(&mut self) -> ImageResult<Option<Vec<u8>>> {
        Ok(self.reader.info().icc_profile.as_ref().map(|x| x.to_vec()))
    }

    fn read_image(mut self, buf: &mut [u8]) -> ImageResult<()> {
        use byteorder_lite::{BigEndian, ByteOrder, NativeEndian};

        assert_eq!(u64::try_from(buf.len()), Ok(self.total_bytes()));
        self.reader.next_frame(buf).map_err(error_from_png)?;
        // PNG images are big endian. For 16 bit per channel and larger types,
        // the buffer may need to be reordered to native endianness per the
        // contract of `read_image`.
        // TODO: assumes equal channel bit depth.
        let bpc = self.color_type().bytes_per_pixel() / self.color_type().channel_count();

        match bpc {
            1 => (), // No reodering necessary for u8
            2 => buf.chunks_exact_mut(2).for_each(|c| {
                let v = BigEndian::read_u16(c);
                NativeEndian::write_u16(c, v);
            }),
            _ => unreachable!(),
        }
        Ok(())
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
        (*self).read_image(buf)
    }

    fn set_limits(&mut self, limits: Limits) -> ImageResult<()> {
        limits.check_support(&image::LimitSupport::default())?;
        let info = self.reader.info();
        limits.check_dimensions(info.width, info.height)?;
        self.limits = limits;
        // TODO: add `png::Reader::change_limits()` and call it here
        // to also constrain the internal buffer allocations in the PNG crate
        Ok(())
    }
}

/// An [`AnimationDecoder`] adapter of [`PngDecoder`].
///
/// See [`PngDecoder::apng`] for more information.
///
/// [`AnimationDecoder`]: ../trait.AnimationDecoder.html
/// [`PngDecoder`]: struct.PngDecoder.html
/// [`PngDecoder::apng`]: struct.PngDecoder.html#method.apng
pub struct ApngDecoder<R: BufRead + Seek> {
    inner: PngDecoder<R>,
    /// The current output buffer.
    current: Option<RgbaImage>,
    /// The previous output buffer, used for dispose op previous.
    previous: Option<RgbaImage>,
    /// The dispose op of the current frame.
    dispose: DisposeOp,

    /// The region to dispose of the previous frame.
    dispose_region: Option<(u32, u32, u32, u32)>,
    /// The number of image still expected to be able to load.
    remaining: u32,
    /// The next (first) image is the thumbnail.
    has_thumbnail: bool,
}

impl<R: BufRead + Seek> ApngDecoder<R> {
    fn new(inner: PngDecoder<R>) -> Self {
        let info = inner.reader.info();
        let remaining = match info.animation_control() {
            // The expected number of fcTL in the remaining image.
            Some(actl) => actl.num_frames,
            None => 0,
        };
        // If the IDAT has no fcTL then it is not part of the animation counted by
        // num_frames. All following fdAT chunks must be preceded by an fcTL
        let has_thumbnail = info.frame_control.is_none();
        ApngDecoder {
            inner,
            current: None,
            previous: None,
            dispose: DisposeOp::Background,
            dispose_region: None,
            remaining,
            has_thumbnail,
        }
    }

    // TODO: thumbnail(&mut self) -> Option<impl ImageDecoder<'_>>

    /// Decode one subframe and overlay it on the canvas.
    fn mix_next_frame(&mut self) -> Result<Option<&RgbaImage>, ImageError> {
        // The iterator always produces RGBA8 images
        const COLOR_TYPE: ColorType = ColorType::Rgba8;

        // Allocate the buffers, honoring the memory limits
        let (width, height) = self.inner.dimensions();
        {
            let limits = &mut self.inner.limits;
            if self.previous.is_none() {
                limits.reserve_buffer(width, height, COLOR_TYPE)?;
                self.previous = Some(RgbaImage::new(width, height));
            }

            if self.current.is_none() {
                limits.reserve_buffer(width, height, COLOR_TYPE)?;
                self.current = Some(RgbaImage::new(width, height));
            }
        }

        // Remove this image from remaining.
        self.remaining = match self.remaining.checked_sub(1) {
            None => return Ok(None),
            Some(next) => next,
        };

        // Shorten ourselves to 0 in case of error.
        let remaining = self.remaining;
        self.remaining = 0;

        // Skip the thumbnail that is not part of the animation.
        if self.has_thumbnail {
            // Clone the limits so that our one-off allocation that's destroyed after this scope doesn't persist
            let mut limits = self.inner.limits.clone();
            limits.reserve_usize(self.inner.reader.output_buffer_size())?;
            let mut buffer = vec![0; self.inner.reader.output_buffer_size()];
            // TODO: add `png::Reader::change_limits()` and call it here
            // to also constrain the internal buffer allocations in the PNG crate
            self.inner
                .reader
                .next_frame(&mut buffer)
                .map_err(error_from_png)?;
            self.has_thumbnail = false;
        }

        self.animatable_color_type()?;

        // We've initialized them earlier in this function
        let previous = self.previous.as_mut().unwrap();
        let current = self.current.as_mut().unwrap();

        // Dispose of the previous frame.

        match self.dispose {
            DisposeOp::None => {
                previous.clone_from(current);
            }
            DisposeOp::Background => {
                previous.clone_from(current);
                if let Some((px, py, width, height)) = self.dispose_region {
                    let mut region_current = current.sub_image(px, py, width, height);

                    // FIXME: This is a workaround for the fact that `pixels_mut` is not implemented
                    let pixels: Vec<_> = region_current.pixels().collect();

                    for (x, y, _) in &pixels {
                        region_current.put_pixel(*x, *y, Rgba::from([0, 0, 0, 0]));
                    }
                } else {
                    // The first frame is always a background frame.
                    current.pixels_mut().for_each(|pixel| {
                        *pixel = Rgba::from([0, 0, 0, 0]);
                    });
                }
            }
            DisposeOp::Previous => {
                let (px, py, width, height) = self
                    .dispose_region
                    .expect("The first frame must not set dispose=Previous");
                let region_previous = previous.sub_image(px, py, width, height);
                current
                    .copy_from(&region_previous.to_image(), px, py)
                    .unwrap();
            }
        }

        // The allocations from now on are not going to persist,
        // and will be destroyed at the end of the scope.
        // Clone the limits so that any changes to them die with the allocations.
        let mut limits = self.inner.limits.clone();

        // Read next frame data.
        let raw_frame_size = self.inner.reader.output_buffer_size();
        limits.reserve_usize(raw_frame_size)?;
        let mut buffer = vec![0; raw_frame_size];
        // TODO: add `png::Reader::change_limits()` and call it here
        // to also constrain the internal buffer allocations in the PNG crate
        self.inner
            .reader
            .next_frame(&mut buffer)
            .map_err(error_from_png)?;
        let info = self.inner.reader.info();

        // Find out how to interpret the decoded frame.
        let (width, height, px, py, blend);
        match info.frame_control() {
            None => {
                width = info.width;
                height = info.height;
                px = 0;
                py = 0;
                blend = BlendOp::Source;
            }
            Some(fc) => {
                width = fc.width;
                height = fc.height;
                px = fc.x_offset;
                py = fc.y_offset;
                blend = fc.blend_op;
                self.dispose = fc.dispose_op;
            }
        };

        self.dispose_region = Some((px, py, width, height));

        // Turn the data into an rgba image proper.
        limits.reserve_buffer(width, height, COLOR_TYPE)?;
        let source = match self.inner.color_type {
            ColorType::L8 => {
                let image = ImageBuffer::<Luma<_>, _>::from_raw(width, height, buffer).unwrap();
                DynamicImage::ImageLuma8(image).into_rgba8()
            }
            ColorType::La8 => {
                let image = ImageBuffer::<LumaA<_>, _>::from_raw(width, height, buffer).unwrap();
                DynamicImage::ImageLumaA8(image).into_rgba8()
            }
            ColorType::Rgb8 => {
                let image = ImageBuffer::<Rgb<_>, _>::from_raw(width, height, buffer).unwrap();
                DynamicImage::ImageRgb8(image).into_rgba8()
            }
            ColorType::Rgba8 => ImageBuffer::<Rgba<_>, _>::from_raw(width, height, buffer).unwrap(),
            ColorType::L16 | ColorType::Rgb16 | ColorType::La16 | ColorType::Rgba16 => {
                // TODO: to enable remove restriction in `animatable_color_type` method.
                unreachable!("16-bit apng not yet support")
            }
            _ => unreachable!("Invalid png color"),
        };
        // We've converted the raw frame to RGBA8 and disposed of the original allocation
        limits.free_usize(raw_frame_size);

        match blend {
            BlendOp::Source => {
                current
                    .copy_from(&source, px, py)
                    .expect("Invalid png image not detected in png");
            }
            BlendOp::Over => {
                // TODO: investigate speed, speed-ups, and bounds-checks.
                for (x, y, p) in source.enumerate_pixels() {
                    current.get_pixel_mut(x + px, y + py).blend(p);
                }
            }
        }

        // Ok, we can proceed with actually remaining images.
        self.remaining = remaining;
        // Return composited output buffer.

        Ok(Some(self.current.as_ref().unwrap()))
    }

    fn animatable_color_type(&self) -> Result<(), ImageError> {
        match self.inner.color_type {
            ColorType::L8 | ColorType::Rgb8 | ColorType::La8 | ColorType::Rgba8 => Ok(()),
            // TODO: do not handle multi-byte colors. Remember to implement it in `mix_next_frame`.
            ColorType::L16 | ColorType::Rgb16 | ColorType::La16 | ColorType::Rgba16 => {
                Err(unsupported_color(self.inner.color_type.into()))
            }
            _ => unreachable!("{:?} not a valid png color", self.inner.color_type),
        }
    }
}

impl<'a, R: BufRead + Seek + 'a> AnimationDecoder<'a> for ApngDecoder<R> {
    fn into_frames(self) -> Frames<'a> {
        struct FrameIterator<R: BufRead + Seek>(ApngDecoder<R>);

        impl<R: BufRead + Seek> Iterator for FrameIterator<R> {
            type Item = ImageResult<Frame>;

            fn next(&mut self) -> Option<Self::Item> {
                let image = match self.0.mix_next_frame() {
                    Ok(Some(image)) => image.clone(),
                    Ok(None) => return None,
                    Err(err) => return Some(Err(err)),
                };

                let info = self.0.inner.reader.info();
                let fc = info.frame_control().unwrap();
                // PNG delays are rations in seconds.
                let num = u32::from(fc.delay_num) * 1_000u32;
                let denom = match fc.delay_den {
                    // The standard dictates to replace by 100 when the denominator is 0.
                    0 => 100,
                    d => u32::from(d),
                };
                // let delay = Delay::from_ratio(Ratio::new(num, denom));
                // HACKING our way into constructing an image::Delay from our own Ratio struct.
                let delay = {
                    /// Private struct copied from image-rs.
                    #[derive(Copy, Clone)]
                    #[allow(unused)]
                    struct Ratio {
                        numer: u32,
                        denom: u32,
                    }

                    impl Ratio {
                        #[inline]
                        pub fn new(numerator: u32, denominator: u32) -> Self {
                            assert_ne!(denominator, 0);
                            Self {
                                numer: numerator,
                                denom: denominator,
                            }
                        }
                    }
                    unsafe { std::mem::transmute::<Ratio, image::Delay>(Ratio::new(num, denom)) }
                };
                Some(Ok(Frame::from_parts(image, 0, 0, delay)))
            }
        }

        Frames::new(Box::new(FrameIterator(self)))
    }
}

impl<R: BufRead + Seek> AnimationLoopsDecoder for ApngDecoder<R> {
    fn get_loop_count(&self) -> AnimationLoops {
        self.inner
            .reader
            .info()
            .animation_control()
            .map(|animation_control| match animation_control.num_plays {
                0 => AnimationLoops::Infinite,
                i => AnimationLoops::Finite(NonZeroU32::new(i).unwrap()),
            })
            .unwrap_or(AnimationLoops::Finite(NonZeroU32::new(1).unwrap()))
    }
}

fn error_from_png(err: png::DecodingError) -> ImageError {
    use png::DecodingError::*;
    match err {
        IoError(err) => ImageError::IoError(err),
        // The input image was not a valid PNG.
        err @ Format(_) => ImageError::Decoding(DecodingError::new(ImageFormat::Png.into(), err)),
        // Other is used when:
        // - The decoder is polled for more animation frames despite being done (or not being animated
        //   in the first place).
        // - The output buffer does not have the required size.
        err @ Parameter(_) => ImageError::Parameter(ParameterError::from_kind(
            ParameterErrorKind::Generic(err.to_string()),
        )),
        LimitsExceeded => {
            ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[ignore = "path pointed to does not exist"]
    #[test]
    fn underlying_error() {
        use std::error::Error;

        let mut not_png =
            std::fs::read("tests/images/png/bugfixes/debug_triangle_corners_widescreen.png")
                .unwrap();
        not_png[0] = 0;

        let error = PngDecoder::new(Cursor::new(&not_png)).err().unwrap();
        let _ = error
            .source()
            .unwrap()
            .downcast_ref::<png::DecodingError>()
            .expect("Caused by a png error");
    }

    #[test]
    fn encode_bad_color_type() {
        // regression test for issue #1663
        let image = DynamicImage::new_rgb32f(1, 1);
        let mut target = Cursor::new(vec![]);
        let _ = image.write_to(&mut target, ImageFormat::Png);
    }
}
