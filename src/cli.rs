use std::{borrow::Cow, collections::HashSet, fs::File, io::BufReader, path::PathBuf, rc::Rc};

use exif::{Exif, Value};
use image::{
    AnimationDecoder, ExtendedColorType, ImageDecoder, ImageReader, ImageResult,
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    foximg::{AnimationLoops, AnimationLoopsDecoder},
};
use raylib::prelude::*;
use serde::{Serialize, ser::SerializeMap};

use crate::{FoximgArgs, FoximgInfoLanguage, foximg_log};

type FoximgInfoTracelog = Rc<dyn Fn(TraceLogLevel, &str)>;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FoximgImageAnimationInfo {
    frames: usize,
    loops: AnimationLoops,
}

impl FoximgImageAnimationInfo {
    fn new(frames: usize, loops: AnimationLoops) -> Self {
        Self { frames, loops }
    }

    pub fn png(png: PngDecoder<BufReader<File>>) -> anyhow::Result<Option<Self>> {
        let mut info: Option<Self> = None;
        if png.is_apng()? {
            let apng = png.apng()?;
            let loops = apng.get_loop_count();
            let frames = apng.into_frames().collect_frames()?.len();

            info = Some(Self::new(frames, loops));
        }

        Ok(info)
    }

    pub fn gif(gif: GifDecoder<BufReader<File>>) -> anyhow::Result<Option<Self>> {
        let loops = gif.get_loop_count();
        let frames = gif.into_frames().collect_frames()?.len();

        if frames <= 1 {
            Ok(None)
        } else {
            Ok(Some(Self::new(frames, loops)))
        }
    }

    pub fn webp(webp: WebPDecoder<BufReader<File>>) -> anyhow::Result<Option<Self>> {
        let mut info: Option<Self> = None;
        if webp.has_animation() {
            let loops = webp.get_loop_count();
            let frames = webp.into_frames().collect_frames()?.len();

            info = Some(Self::new(frames, loops));
        }

        Ok(info)
    }
}

struct FoximgExifInfo {
    exif: Exif,
    tracelog: FoximgInfoTracelog,
}

impl Serialize for FoximgExifInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let fields = self.exif.fields();
        let fields_len = fields.len();
        let mut parsed_keys = HashSet::with_capacity(fields_len);
        let mut map = serializer.serialize_map(Some(fields_len))?;

        for f in fields {
            if let Value::Undefined(_, _) | Value::Unknown(_, _, _) = f.value {
                continue;
            } else if f.tag.description().is_none() {
                continue;
            }

            let key: Rc<str> = Rc::from(f.tag.to_string());
            if !parsed_keys.insert(key.clone()) {
                continue;
            }

            let value = f.display_value().with_unit(&self.exif).to_string();
            map.serialize_entry(&*key, &value)?;
        }

        (self.tracelog)(
            TraceLogLevel::LOG_DEBUG,
            &format!("Serialized EXIF metadata successfully ({} fields)", parsed_keys.len()),
        );

        map.end()
    }
}

struct FoximgInfoDecoder {
    pub dimensions: (u32, u32),
    pub color_type: ExtendedColorType,
    pub animation_info: Option<FoximgImageAnimationInfo>,
    pub exif_info: Option<FoximgExifInfo>,

    tracelog: FoximgInfoTracelog,
    no_exif: bool,
}

impl FoximgInfoDecoder {
    pub fn new(tracelog: FoximgInfoTracelog, no_exif: bool) -> Self {
        Self {
            dimensions: (0, 0),
            color_type: unsafe { std::mem::zeroed() },
            animation_info: None,
            exif_info: None,
            tracelog,
            no_exif,
        }
    }

    pub fn decode<T>(
        &mut self,
        decoder: impl FnOnce() -> ImageResult<T>,
        animation_info: impl FnOnce(T) -> anyhow::Result<Option<FoximgImageAnimationInfo>>,
    ) -> anyhow::Result<()>
    where
        T: ImageDecoder,
    {
        let mut decoder = decoder()?;
        self.dimensions = decoder.dimensions();
        self.color_type = decoder.original_color_type();
        if !self.no_exif {
            self.exif_info = decoder
                .exif_metadata()?
                .or_else(|| {
                    (self.tracelog)(
                        TraceLogLevel::LOG_DEBUG,
                        "   > Image doesn't contain EXIF metadata",
                    );

                    None
                })
                .and_then(|exif| {
                    let exif = exif::Reader::new()
                        .continue_on_error(true)
                        .read_raw(exif)
                        .or_else(|e| {
                            e.distill_partial_result(|errors| {
                                (self.tracelog)(
                                    TraceLogLevel::LOG_DEBUG,
                                    "   > Errors while decoding EXIF metadata:",
                                );

                                for e in errors {
                                    (self.tracelog)(
                                        TraceLogLevel::LOG_DEBUG,
                                        &format!("      > {e}"),
                                    );
                                }
                            })
                        })
                        .ok()?;

                    (self.tracelog)(
                        TraceLogLevel::LOG_DEBUG,
                        "   > Decoded EXIF metadata successfully",
                    );

                    Some(FoximgExifInfo {
                        exif,
                        tracelog: self.tracelog.clone(),
                    })
                });
        }

        self.animation_info = animation_info(decoder)?;
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FoximgImageInfo<'a> {
    pub filename: Cow<'a, str>,
    pub width: u32,
    pub height: u32,
    pub mime: &'static str,
    pub extensions: &'static [&'static str],
    pub color_type: ExtendedColorType,

    pub animated: Option<FoximgImageAnimationInfo>,
    pub exif: Option<FoximgExifInfo>,
}

struct FoximgInfo {
    path: PathBuf,
    no_exif: bool,
    language: FoximgInfoLanguage,

    tracelog: FoximgInfoTracelog,
}

impl FoximgInfo {
    pub fn init(args: &FoximgArgs, language: FoximgInfoLanguage) -> anyhow::Result<Self> {
        let path = match args.path {
            Some(path) => PathBuf::from(path).canonicalize()?,
            None => anyhow::bail!("Must input path"),
        };

        let tracelog_level = if args.verbose {
            TraceLogLevel::LOG_ALL
        } else {
            TraceLogLevel::LOG_INFO
        };

        let tracelog = Rc::new(move |level: TraceLogLevel, msg: &str| {
            if (level as i32) < (tracelog_level as i32) {
                return;
            }

            foximg_log::tracelog(level, msg);
        });

        tracelog(TraceLogLevel::LOG_DEBUG, "Foximg initialized successfully");
        Ok(Self {
            no_exif: args.quiet,
            path,
            language,
            tracelog,
        })
    }

    pub fn run(self) -> anyhow::Result<()> {
        let filename = self
            .path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Path cannot be '..'"))?
            .to_string_lossy();

        let mut reader = BufReader::new(File::open(&self.path)?);
        let image_reader = ImageReader::new(&mut reader).with_guessed_format()?;
        let format = image_reader
            .format()
            .ok_or_else(|| anyhow::anyhow!("Not a recognized or supported image"))?;

        let mime = format.to_mime_type();
        let extensions = format.extensions_str();

        (self.tracelog)(
            TraceLogLevel::LOG_DEBUG,
            &format!("Decoding {extensions:?} image ({}):", self.path.display()),
        );

        let mut decoder = FoximgInfoDecoder::new(self.tracelog.clone(), self.no_exif);
        match format {
            image::ImageFormat::Png => {
                decoder.decode(|| PngDecoder::new(reader), FoximgImageAnimationInfo::png)
            }
            image::ImageFormat::Gif => {
                decoder.decode(|| GifDecoder::new(reader), FoximgImageAnimationInfo::gif)
            }
            image::ImageFormat::WebP => {
                decoder.decode(|| WebPDecoder::new(reader), FoximgImageAnimationInfo::webp)
            }
            _ => decoder.decode(|| image_reader.into_decoder(), |_| Ok(None)),
        }?;

        (self.tracelog)(TraceLogLevel::LOG_DEBUG, "Decoded image successfully");
        let info = FoximgImageInfo {
            width: decoder.dimensions.0,
            height: decoder.dimensions.1,
            color_type: decoder.color_type,
            animated: decoder.animation_info,
            exif: decoder.exif_info,
            filename,
            mime,
            extensions,
        };

        let info = match self.language {
            FoximgInfoLanguage::Toml => toml::to_string(&info)?,
            FoximgInfoLanguage::Json => serde_json::to_string_pretty(&info)?,
        };

        println!("{info}");
        Ok(())
    }
}

fn try_run(args: &FoximgArgs, language: FoximgInfoLanguage) -> anyhow::Result<()> {
    FoximgInfo::init(args, language)?.run()?;
    Ok(())
}

pub fn run(args: FoximgArgs, language: FoximgInfoLanguage) {
    if let Err(e) = self::try_run(&args, language) {
        foximg_log::tracelog(TraceLogLevel::LOG_ERROR, &format!("{e}"));
    } else if args.verbose {
        foximg_log::tracelog(
            TraceLogLevel::LOG_DEBUG,
            "Foximg uninitialized successfully. Goodbye!",
        );
    }
}
