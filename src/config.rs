use std::{
    fmt::{Debug, Display},
    fs::{self, File},
    io::{self, Write},
    ops::Deref,
};

use raylib::prelude::*;
use serde::{de::Visitor, Deserialize, Serialize};

use crate::{foximg_error, Foximg};

pub enum FoximgConfigError {
    IO(io::Error),
    TOML(anyhow::Error),
}

impl Display for FoximgConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => write!(f, "{e}"),
            Self::TOML(e) => write!(f, "{e}"),
        }
    }
}

impl Debug for FoximgConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(e) => write!(f, "{e}"),
            Self::TOML(e) => write!(f, "{e:?}"),
        }
    }
}

impl From<io::Error> for FoximgConfigError {
    fn from(e: io::Error) -> Self {
        Self::IO(e)
    }
}

impl From<toml::de::Error> for FoximgConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::TOML(e.into())
    }
}

impl From<toml::ser::Error> for FoximgConfigError {
    fn from(e: toml::ser::Error) -> Self {
        Self::TOML(e.into())
    }
}

pub trait FoximgConfig
where
    Self: Sized + Default + Serialize + for<'de> Deserialize<'de>,
{
    fn try_new(path: &str) -> Result<Self, FoximgConfigError> {
        let file = fs::read_to_string(path)?;
        let settings: Self = toml::from_str(&file)?;
        Ok(settings)
    }

    fn new(path: &str) -> (Self, Option<FoximgConfigError>) {
        match Self::try_new(path) {
            Ok(settings) => (settings, None),
            Err(e) => {
                let settings = Self::default();
                settings.to_file(path);
                (settings, Some(e))
            }
        }
    }

    fn try_to_file(&self, path: &str) -> Result<(), FoximgConfigError> {
        let settings = toml::to_string(self)?;
        let mut file = File::create(path)?;
        write!(&mut file, "{settings}")?;
        Ok(())
    }

    fn to_file(&self, path: &str) {
        if let Err(e) = self.try_to_file(path) {
            foximg_error::show(&format!("Couldn't create '{path}': {e:?}"));
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct FoximgState {
    pub w: i32,
    pub h: i32,
    pub xy: Option<(i32, i32)>,

    pub maximized: bool,
    pub fullscreen: bool,
}

impl Default for FoximgState {
    fn default() -> Self {
        Self {
            w: 640,
            h: 480,
            xy: None,
            maximized: false,
            fullscreen: false,
        }
    }
}

impl FoximgState {
    pub const PATH: &str = "foximg_state.toml";
}

impl FoximgConfig for FoximgState {}

#[derive(Copy, Clone, Serialize)]
#[repr(transparent)]
pub struct FoximgColor(Color);

impl<'de> Deserialize<'de> for FoximgColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum FoximgColorField {
            RGB,
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
                        FoximgColorField::RGB => de_field!(rgb),
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

impl Into<ffi::Color> for FoximgColor {
    fn into(self) -> ffi::Color {
        self.0.into()
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FoximgStyleOptionals {
    pub bg_disabled: Option<FoximgColor>,
    pub bg_focused: Option<FoximgColor>,
    pub border: Option<FoximgColor>,
    pub border_disabled: Option<FoximgColor>,
    pub border_focused: Option<FoximgColor>,
    pub text: Option<FoximgColor>,
    pub text_disabled: Option<FoximgColor>,
    pub text_focused: Option<FoximgColor>,
}

#[derive(Serialize, Deserialize)]
pub struct FoximgStyle {
    pub dark: bool,
    pub accent: FoximgColor,
    pub bg: FoximgColor,
    #[serde(flatten)]
    pub optionals: FoximgStyleOptionals,
}

impl FoximgStyle {
    pub const PATH: &str = "foximg_style.toml";

    fn update_titlebar(&self, rl: &mut RaylibHandle) {
        #[cfg(windows)]
        self.update_titlebar_win32(rl);
    }

    pub fn update_style(&self, rl: &mut RaylibHandle) {
        let border = match self.optionals.border {
            Some(border) => {
                let border = border.color_to_int();
                rl.gui_set_style(
                    GuiControl::DEFAULT,
                    GuiControlProperty::BORDER_COLOR_NORMAL as i32,
                    border,
                );
                border
            }
            None => rl.gui_get_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BORDER_COLOR_NORMAL as i32,
            ),
        };

        rl.gui_set_style(
            GuiControl::DEFAULT,
            GuiControlProperty::BASE_COLOR_NORMAL as i32,
            self.bg.color_to_int(),
        );

        match self.optionals.bg_focused {
            Some(bg_focused) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BASE_COLOR_FOCUSED as i32,
                bg_focused.color_to_int(),
            ),
            None => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BASE_COLOR_FOCUSED as i32,
                self.accent.alpha(1.).color_to_int(),
            ),
        }

        match self.optionals.bg_disabled {
            Some(bg_disabled) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BASE_COLOR_DISABLED as i32,
                bg_disabled.color_to_int(),
            ),
            None => {
                let border_disabled = rl.gui_get_style(
                    GuiControl::DEFAULT,
                    GuiControlProperty::BORDER_COLOR_DISABLED as i32,
                );
                rl.gui_set_style(
                    GuiControl::DEFAULT,
                    GuiControlProperty::BASE_COLOR_DISABLED as i32,
                    if self.dark { border } else { border_disabled },
                )
            }
        }

        match self.optionals.border_disabled {
            Some(border_disabled) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BORDER_COLOR_DISABLED as i32,
                border_disabled.color_to_int(),
            ),
            None => {
                if !self.dark {
                    let bg_disabled = rl.gui_get_style(
                        GuiControl::DEFAULT,
                        GuiControlProperty::BASE_COLOR_DISABLED as i32,
                    );
                    rl.gui_set_style(
                        GuiControl::DEFAULT,
                        GuiControlProperty::BORDER_COLOR_DISABLED as i32,
                        bg_disabled,
                    )
                }
            }
        }

        match self.optionals.border_focused {
            Some(border_focused) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BORDER_COLOR_FOCUSED as i32,
                border_focused.color_to_int(),
            ),
            None => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::BORDER_COLOR_FOCUSED as i32,
                border,
            ),
        }

        match self.optionals.text {
            Some(text) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::TEXT_COLOR_NORMAL as i32,
                text.color_to_int(),
            ),
            None => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::TEXT_COLOR_NORMAL as i32,
                if self.dark {
                    Color::WHITE.color_to_int()
                } else {
                    Color::BLACK.color_to_int()
                },
            ),
        }

        match self.optionals.text_disabled {
            Some(text_disabled) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::TEXT_COLOR_DISABLED as i32,
                text_disabled.color_to_int(),
            ),
            None => {
                if !self.dark {
                    rl.gui_set_style(
                        GuiControl::DEFAULT,
                        GuiControlProperty::TEXT_COLOR_DISABLED as i32,
                        Color::DIMGRAY.color_to_int(),
                    );
                }
            }
        }

        match self.optionals.text_focused {
            Some(text_focused) => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::TEXT_COLOR_FOCUSED as i32,
                text_focused.color_to_int(),
            ),
            None => rl.gui_set_style(
                GuiControl::DEFAULT,
                GuiControlProperty::TEXT_COLOR_FOCUSED as i32,
                if self.dark {
                    Color::BLACK.color_to_int()
                } else {
                    Color::WHITE.color_to_int()
                },
            ),
        }

        self.update_titlebar(rl);
    }
}

impl Default for FoximgStyle {
    #[cfg(windows)]
    fn default() -> Self {
        Self::default_win32()
    }

    #[cfg(not(windows))]
    fn default() -> Self {
        Self::default_unix()
    }
}

impl FoximgConfig for FoximgStyle {}

#[cfg(windows)]
mod foximg_style_win32 {
    use raylib::prelude::*;
    use winapi::{
        shared::{
            minwindef::{BOOL, DWORD},
            windef::HWND,
        },
        um::dwmapi,
    };
    use windows::UI::ViewManagement::{UIColorType, UISettings};

    use super::{FoximgColor, FoximgStyle, FoximgStyleOptionals};

    impl FoximgStyle {
        pub(super) fn default_win32() -> Self {
            const WIN_BLACK: windows::UI::Color = windows::UI::Color {
                R: 0x00,
                G: 0x00,
                B: 0x00,
                A: 0xFF,
            };
            const MIX_BLACK: Color = Color::new(35, 35, 35, 255);
            const MIX_WHITE: Color = Color::GAINSBORO;

            let ui_settings = UISettings::new().unwrap();
            let dark = ui_settings.GetColorValue(UIColorType::Background).unwrap();
            let dark = dark == WIN_BLACK;
            let create_theme = |dark: bool| {
                let accent = ui_settings
                    .GetColorValue(if dark {
                        UIColorType::AccentLight3
                    } else {
                        UIColorType::AccentDark3
                    })
                    .unwrap();
                let bg = ui_settings
                    .GetColorValue(if dark {
                        UIColorType::AccentDark1
                    } else {
                        UIColorType::AccentLight1
                    })
                    .unwrap();

                FoximgStyle {
                    dark,
                    accent: FoximgColor(Color::new(accent.R, accent.G, accent.B, accent.A / 2)),
                    bg: FoximgColor(Color::new(bg.R, bg.G, bg.B, bg.A).tint(if dark {
                        MIX_BLACK
                    } else {
                        MIX_WHITE
                    })),
                    optionals: FoximgStyleOptionals::default(),
                }
            };

            let main_theme = create_theme(dark);
            let alt_theme = create_theme(!dark);

            main_theme
        }

        pub(super) fn update_titlebar_win32(&self, rl: &mut RaylibHandle) {
            const DWMWA_USE_IMMERSIVE_DARK_MODE: DWORD = 20;

            unsafe {
                let hwnd = rl.get_window_handle();
                let value: BOOL = self.dark as BOOL;
                dwmapi::DwmSetWindowAttribute(
                    hwnd as HWND,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    &value as *const BOOL as *const winapi::ctypes::c_void,
                    size_of::<BOOL>() as DWORD,
                );
            }
            // Resize window to update titlebar
            rl.set_window_size(rl.get_screen_width() + 1, rl.get_screen_height());
            rl.set_window_size(rl.get_screen_width() - 1, rl.get_screen_height());
        }
    }
}

#[cfg(not(windows))]
mod foximg_style_unix {
    use raylib::prelude::*;

    use super::{FoximgColor, FoximgStyle, FoximgStyleOptionals};

    impl FoximgStyle {
        pub(super) fn default_unix() -> Self {
            Self {
                dark: true,
                accent: FoximgColor(Color::new(245, 213, 246, 127)),
                bg: FoximgColor(Color::new(34, 12, 35, 255)),
                optionals: FoximgStyleOptionals::default(),
            }
        }
    }
}

impl Foximg<'_> {
    pub(crate) fn save_state(&mut self) {
        self.state.fullscreen = self.fullscreen;
        if self.fullscreen {
            self.toggle_fullscreen();
        }

        self.state.maximized = unsafe { ffi::IsWindowMaximized() };
        unsafe { ffi::RestoreWindow() };

        self.state.w = self.rl.get_screen_width();
        self.state.h = self.rl.get_screen_height();
        self.state.xy = {
            let position = self.rl.get_window_position();
            Some((position.x as i32, position.y as i32))
        };

        self.state.to_file(FoximgState::PATH);
    }
}
