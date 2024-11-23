use raylib::prelude::*;

/// `grad.png`
static GRAD: &[u8] = include_bytes!("grad.png");
/// `"flip.png"`. Designed by nawicon from Flaticon
static FLIP: &[u8] = include_bytes!("flip.png");

pub struct FoximgResources {
    grad_l: Texture2D,
    grad_r: Texture2D,
    flip_h: Texture2D,
    flip_v: Texture2D,
}

impl FoximgResources {
    pub fn new(rl: &mut RaylibHandle, rl_thread: &RaylibThread) -> Self {
        rl.trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: === Loading resources ===");

        fn load_image(bytes: &[u8]) -> Image {
            Image::load_image_from_mem(".png", bytes).unwrap()
        }
        let mut load_texture =
            |image: &Image| rl.load_texture_from_image(rl_thread, image).unwrap();

        let mut grad = load_image(GRAD);
        let grad_l = load_texture(&grad);
        grad.flip_horizontal();
        let grad_r = load_texture(&grad);

        let mut flip = load_image(FLIP);
        let flip_v = load_texture(&flip);
        flip.rotate(90);
        let flip_h = load_texture(&flip);

        rl.trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: === Loaded resources ===");
        Self {
            grad_l,
            grad_r,
            flip_h,
            flip_v,
        }
    }

    pub fn grad_l(&self) -> &Texture2D {
        &self.grad_l
    }

    pub fn grad_r(&self) -> &Texture2D {
        &self.grad_r
    }

    pub fn flip_h(&self) -> &Texture2D {
        &self.flip_h
    }

    pub fn flip_v(&self) -> &Texture2D {
        &self.flip_v
    }
}
