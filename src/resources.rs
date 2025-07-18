use raylib::prelude::*;

use crate::images;

const YUDIT_SIZE: f32 = 64.;

pub const BUTTON_FONT_SIZE: f32 = 12.;
/// The width of the whitespace pixels around each side of the flip texture.
pub const FLIP_OFFSET: f32 = 2.;
pub const SYMBOL_SIDE: f32 = 64.;
pub const SYMBOL_PADDING: f32 = 10.;
/// The width of the whitespace pixels on the right side of the yudit 0.
pub const TEXT_RIGHT_OFFSET: f32 = 9.;

pub const fn yudit_spacing(size: f32) -> f32 {
    size / self::YUDIT_SIZE
}

pub struct FoximgResources {
    pub flip: Texture2D,
    pub grad: Texture2D,
    pub yudit: Font,
}

impl FoximgResources {
    pub fn new(rl: &mut RaylibHandle, rl_thread: &RaylibThread) -> Self {
        static FLIP: &[u8] = include_bytes!("resources/flip.png");
        static GRAD: &[u8] = include_bytes!("resources/grad.png");
        static YUDIT: &[u8] = include_bytes!("resources/yudit.ttf");

        let flip = images::new_resource(rl, rl_thread, FLIP, "flip.png").unwrap();
        let grad = images::new_resource(rl, rl_thread, GRAD, "grad.png").unwrap();
        let yudit = rl
            .load_font_from_memory(rl_thread, ".ttf", YUDIT, self::YUDIT_SIZE as i32, None)
            .unwrap();

        yudit
            .texture()
            .set_texture_filter(rl_thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
        rl.gui_set_font(&yudit);
        rl.gui_set_style(
            GuiControl::DEFAULT,
            GuiDefaultProperty::TEXT_SIZE,
            self::BUTTON_FONT_SIZE as i32,
        );
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            "FOXIMG: Resources initialized successfully",
        );

        Self { flip, grad, yudit }
    }
}
