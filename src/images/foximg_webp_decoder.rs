//! Copy of the WebP decoder source in image-rs 0.25.6 with additional functionality.

use std::io::{BufRead, Cursor, Read, Seek};

use byteorder_lite::{BigEndian, LittleEndian, ReadBytesExt};
use image::buffer::ConvertBuffer;
use image::error::{DecodingError, ImageError, ImageResult};
use image::metadata::Orientation;
use image::{AnimationDecoder, ColorType, Delay, Frame, Frames, RgbImage, RgbaImage};
use image::{ImageDecoder, ImageFormat};

use raylib::color::Color;

use super::AnimationLoopsDecoder;

/// WebP Image format decoder.
///
/// Supports both lossless and lossy WebP images.
pub struct WebPDecoder<R> {
    inner: image_webp::WebPDecoder<R>,
    orientation: Option<Orientation>,
}

impl<R: BufRead + Seek> WebPDecoder<R> {
    /// Create a new `WebPDecoder` from the Reader `r`.
    pub fn new(r: R) -> ImageResult<Self> {
        Ok(Self {
            inner: image_webp::WebPDecoder::new(r).map_err(error_from_webp_decode)?,
            orientation: None,
        })
    }

    /// Returns true if the image as described by the bitstream is animated.
    pub fn has_animation(&self) -> bool {
        self.inner.is_animated()
    }

    /// Sets the background color if the image is an extended and animated webp.
    ///
    /// In Foximg, we're using the `Color` struct from raylib instead of `Rgba<u8>`
    pub fn set_background_color(&mut self, color: Color) -> ImageResult<()> {
        self.inner
            .set_background_color([color.r, color.g, color.b, color.a])
            .map_err(error_from_webp_decode)
    }
}

impl<R: BufRead + Seek> ImageDecoder for WebPDecoder<R> {
    fn dimensions(&self) -> (u32, u32) {
        self.inner.dimensions()
    }

    fn color_type(&self) -> ColorType {
        if self.inner.has_alpha() {
            ColorType::Rgba8
        } else {
            ColorType::Rgb8
        }
    }

    fn read_image(mut self, buf: &mut [u8]) -> ImageResult<()> {
        assert_eq!(u64::try_from(buf.len()), Ok(self.total_bytes()));

        self.inner.read_image(buf).map_err(error_from_webp_decode)
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
        (*self).read_image(buf)
    }

    fn icc_profile(&mut self) -> ImageResult<Option<Vec<u8>>> {
        self.inner.icc_profile().map_err(error_from_webp_decode)
    }

    fn exif_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
        let exif = self.inner.exif_metadata().map_err(error_from_webp_decode)?;

        self.orientation = Some(
            exif.as_ref()
                .and_then(|exif| orientation_from_exif_chunk(exif))
                .unwrap_or(Orientation::NoTransforms),
        );

        Ok(exif)
    }

    fn orientation(&mut self) -> ImageResult<Orientation> {
        // `exif_metadata` caches the orientation, so call it if `orientation` hasn't been set yet.
        if self.orientation.is_none() {
            let _ = self.exif_metadata()?;
        }
        Ok(self.orientation.unwrap())
    }
}

impl<'a, R: 'a + BufRead + Seek> AnimationDecoder<'a> for WebPDecoder<R> {
    fn into_frames(self) -> Frames<'a> {
        struct FramesInner<R: Read + Seek> {
            decoder: WebPDecoder<R>,
            current: u32,
        }
        impl<R: BufRead + Seek> Iterator for FramesInner<R> {
            type Item = ImageResult<Frame>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.current == self.decoder.inner.num_frames() {
                    return None;
                }
                self.current += 1;
                let (width, height) = self.decoder.inner.dimensions();

                let (img, delay) = if self.decoder.inner.has_alpha() {
                    let mut img = RgbaImage::new(width, height);
                    match self.decoder.inner.read_frame(&mut img) {
                        Ok(delay) => (img, delay),
                        Err(image_webp::DecodingError::NoMoreFrames) => return None,
                        Err(e) => return Some(Err(error_from_webp_decode(e))),
                    }
                } else {
                    let mut img = RgbImage::new(width, height);
                    match self.decoder.inner.read_frame(&mut img) {
                        Ok(delay) => (img.convert(), delay),
                        Err(image_webp::DecodingError::NoMoreFrames) => return None,
                        Err(e) => return Some(Err(error_from_webp_decode(e))),
                    }
                };

                Some(Ok(Frame::from_parts(
                    img,
                    0,
                    0,
                    Delay::from_numer_denom_ms(delay, 1),
                )))
            }
        }

        Frames::new(Box::new(FramesInner::<R> {
            decoder: self,
            current: 0,
        }))
    }
}

impl<R: BufRead + Seek> AnimationLoopsDecoder for WebPDecoder<R> {
    fn get_loop_count(&self) -> super::AnimationLoops {
        self.inner.loop_count().into()
    }
}

fn error_from_webp_decode(e: image_webp::DecodingError) -> ImageError {
    match e {
        image_webp::DecodingError::IoError(e) => ImageError::IoError(e),
        _ => ImageError::Decoding(DecodingError::new(ImageFormat::WebP.into(), e)),
    }
}

/// Copy of `Orientation::from_exif_chunk`, which is private in image-rs.
fn orientation_from_exif_chunk(chunk: &[u8]) -> Option<Orientation> {
    let mut reader = Cursor::new(chunk);

    let mut magic = [0; 4];
    reader.read_exact(&mut magic).ok()?;

    match magic {
        [0x49, 0x49, 42, 0] => {
            let ifd_offset = reader.read_u32::<LittleEndian>().ok()?;
            reader.set_position(u64::from(ifd_offset));
            let entries = reader.read_u16::<LittleEndian>().ok()?;
            for _ in 0..entries {
                let tag = reader.read_u16::<LittleEndian>().ok()?;
                let format = reader.read_u16::<LittleEndian>().ok()?;
                let count = reader.read_u32::<LittleEndian>().ok()?;
                let value = reader.read_u16::<LittleEndian>().ok()?;
                let _padding = reader.read_u16::<LittleEndian>().ok()?;
                if tag == 0x112 && format == 3 && count == 1 {
                    return Orientation::from_exif(value.min(255) as u8);
                }
            }
        }
        [0x4d, 0x4d, 0, 42] => {
            let ifd_offset = reader.read_u32::<BigEndian>().ok()?;
            reader.set_position(u64::from(ifd_offset));
            let entries = reader.read_u16::<BigEndian>().ok()?;
            for _ in 0..entries {
                let tag = reader.read_u16::<BigEndian>().ok()?;
                let format = reader.read_u16::<BigEndian>().ok()?;
                let count = reader.read_u32::<BigEndian>().ok()?;
                let value = reader.read_u16::<BigEndian>().ok()?;
                let _padding = reader.read_u16::<BigEndian>().ok()?;
                if tag == 0x112 && format == 3 && count == 1 {
                    return Orientation::from_exif(value.min(255) as u8);
                }
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_with_overflow_size() {
        let bytes = vec![
            0x52, 0x49, 0x46, 0x46, 0xaf, 0x37, 0x80, 0x47, 0x57, 0x45, 0x42, 0x50, 0x6c, 0x64,
            0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xfb, 0x7e, 0x73, 0x00, 0x06, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65,
            0x40, 0xfb, 0xff, 0xff, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65, 0x65,
            0x00, 0x00, 0x00, 0x00, 0x62, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x49,
            0x49, 0x54, 0x55, 0x50, 0x4c, 0x54, 0x59, 0x50, 0x45, 0x33, 0x37, 0x44, 0x4d, 0x46,
        ];

        let data = std::io::Cursor::new(bytes);

        let _ = WebPDecoder::new(data);
    }
}
