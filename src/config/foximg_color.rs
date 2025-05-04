use std::{fmt::Display, ops::Deref};

use raylib::prelude::*;
use serde::{Deserialize, Serialize, de::Visitor};

#[derive(Copy, Clone, Serialize)]
#[repr(transparent)]
pub struct FoximgColor(pub(super) Color);

impl Display for FoximgColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#010x}", self.color_to_int())
    }
}

impl<'de> Deserialize<'de> for FoximgColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum FoximgColorField {
            Rgb,
            R,
            G,
            B,
            A,
        }

        struct FoximgColorVisitor;

        impl<'de> Visitor<'de> for FoximgColorVisitor {
            type Value = FoximgColor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "Color")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                macro_rules! de_field {
                    ($f:ident) => {{
                        if $f.is_some() {
                            return Err(serde::de::Error::duplicate_field(stringify!($f)));
                        }
                        $f = Some(map.next_value()?);
                    }};
                }
                let mut rgb: Option<i32> = None;
                let mut r: Option<u8> = None;
                let mut g: Option<u8> = None;
                let mut b: Option<u8> = None;
                let mut a: Option<u8> = None;

                while let Some(key) = map.next_key::<FoximgColorField>()? {
                    match key {
                        FoximgColorField::Rgb => de_field!(rgb),
                        FoximgColorField::R => de_field!(r),
                        FoximgColorField::G => de_field!(g),
                        FoximgColorField::B => de_field!(b),
                        FoximgColorField::A => de_field!(a),
                    }
                }
                Ok(FoximgColor(match rgb {
                    Some(rgb) => {
                        if r.is_some() || g.is_some() || b.is_some() {
                            return Err(serde::de::Error::duplicate_field("rgb"));
                        }
                        let b = rgb % 0x100;
                        let g = (rgb - b) / 0x100 % 0x100;
                        let r = (rgb - g) / 0x10000;
                        Color::new(r as u8, g as u8, b as u8, a.unwrap_or(255))
                    }
                    None => Color::new(
                        r.ok_or_else(|| serde::de::Error::missing_field("r"))?,
                        g.ok_or_else(|| serde::de::Error::missing_field("g"))?,
                        b.ok_or_else(|| serde::de::Error::missing_field("b"))?,
                        a.unwrap_or(255),
                    ),
                }))
            }
        }

        deserializer.deserialize_map(FoximgColorVisitor)
    }
}

impl Deref for FoximgColor {
    type Target = Color;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<FoximgColor> for ffi::Color {
    fn from(value: FoximgColor) -> Self {
        value.0.into()
    }
}
