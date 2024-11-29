#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fmt::Debug,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use config::{FoximgConfig, FoximgConfigError, FoximgState, FoximgStyle};
use image::{FoximgImage, FoximgImages, ScaleMult};
use menu::FoximgMenu;
use raylib::prelude::*;
use resources::FoximgResources;

mod config;
mod foximg_error;
pub mod foximg_help;
mod foximg_log;
mod image;
mod menu;
mod resources;

#[derive(Default)]
struct FoximgInstance {
    path: Option<PathBuf>,
}

impl FoximgInstance {
    const INSTANCE: &str = "./.instance";

    fn instance_count() -> io::Result<usize> {
        Ok(fs::read_dir(Self::INSTANCE)?.count())
    }

    fn try_new() -> io::Result<Self> {
        if !fs::exists(Self::INSTANCE)? {
            fs::create_dir(Self::INSTANCE)?;
        }

        let mut instances = Self::instance_count()?;
        let path = loop {
            let new_instance = format!("{instances}");
            let path = Path::new(Self::INSTANCE).join(new_instance);
            if !path.exists() {
                break path;
            }
            instances += 1;
        };
        File::create(&path)?;
        Ok(Self { path: Some(path) })
    }

    pub fn new() -> (Self, Option<io::Error>) {
        match Self::try_new() {
            Ok(instance) => (instance, None),
            Err(e) => (Self::default(), Some(e)),
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn owner(&self) -> io::Result<bool> {
        Ok(Self::instance_count()? == 1)
    }
}

impl Drop for FoximgInstance {
    fn drop(&mut self) {
        if let Some(ref path) = self.path {
            let try_drop = || {
                fs::remove_file(path)?;
                if Self::instance_count()? == 0 {
                    fs::remove_dir(Self::INSTANCE)?;
                }
                io::Result::Ok(())
            };
            if let Err(e) = try_drop() {
                foximg_error::show(&format!("Couldn't delete {path:?}:\n - {e}"));
            }
        }
    }
}

#[derive(Debug)]
struct FoximgBtnsBounds {
    mouse_pos: Vector2,
    left: f32,
    right: f32,
    mouse_pos_left: bool,
    mouse_pos_right: bool,
}

impl FoximgBtnsBounds {
    pub fn new(rl: &RaylibHandle) -> Self {
        let mouse_pos = rl.get_mouse_position();
        let left = rl.get_screen_width().as_f32() / 6.;
        let right = rl.get_screen_width().as_f32() - left;
        let mouse_pos_left = mouse_pos.x < left;
        let mouse_pos_right = mouse_pos.x > right;
        Self {
            mouse_pos,
            left,
            right,
            mouse_pos_left,
            mouse_pos_right,
        }
    }

    fn mouse_pos(&self) -> Vector2 {
        self.mouse_pos
    }

    pub fn left(&self) -> f32 {
        self.left
    }

    pub fn right(&self) -> f32 {
        self.right
    }

    pub fn mouse_pos_left(&self) -> bool {
        self.mouse_pos_left
    }

    pub fn mouse_pos_right(&self) -> bool {
        self.mouse_pos_right
    }
}

struct FoximgDraw<'a, 'imgs> {
    d: RaylibDrawHandle<'a>,
    style: &'a mut FoximgStyle,
    mouse_wheel: &'a mut f32,
    camera: &'a mut Camera2D,
    resources: &'a FoximgResources,
    images: &'a mut Option<FoximgImages<'imgs>>,
    bounds: FoximgBtnsBounds,
}

impl<'a, 'imgs> FoximgDraw<'a, 'imgs> {
    pub fn new_with_bounds(foximg: &'a mut Foximg<'imgs>, bounds: FoximgBtnsBounds) -> Self {
        let d = foximg.rl.begin_drawing(&foximg.rl_thread);
        Self {
            d,
            style: &mut foximg.style,
            mouse_wheel: &mut foximg.mouse_wheel,
            camera: &mut foximg.camera,
            resources: &foximg.resources,
            images: &mut foximg.images,
            bounds,
        }
    }

    pub fn new(foximg: &'a mut Foximg<'imgs>) -> Self {
        let bounds = FoximgBtnsBounds::new(&foximg.rl);
        Self::new_with_bounds(foximg, bounds)
    }

    fn draw_img_symbols(&mut self) {
        if let Some(images) = self.images {
            /// Length of the flip texture
            const SYMBOL_SIDE: i32 = 64;
            const SYMBOL_PAD: i32 = 10;

            if images.get().width_mult == ScaleMult::Inverted {
                self.d.draw_texture(
                    self.resources.flip_h(),
                    SYMBOL_PAD,
                    self.d.get_screen_height() - SYMBOL_PAD - SYMBOL_SIDE,
                    self.style.accent,
                );
            }
            if images.get().height_mult == ScaleMult::Inverted {
                self.d.draw_texture(
                    self.resources.flip_v(),
                    if images.get().width_mult == ScaleMult::Inverted {
                        SYMBOL_PAD * 2 + SYMBOL_SIDE
                    } else {
                        SYMBOL_PAD
                    },
                    self.d.get_screen_height() - SYMBOL_PAD - SYMBOL_SIDE,
                    self.style.accent,
                );
            }

            if images.get().rotation != 0. {
                let text = format!("{}", images.get().rotation);
                let text_size = self.d.measure_text(&text, SYMBOL_SIDE);

                self.d.draw_text(
                    &text,
                    self.d.get_screen_width() - SYMBOL_PAD * 2 - text_size,
                    self.d.get_screen_height() - SYMBOL_PAD - SYMBOL_SIDE,
                    SYMBOL_SIDE,
                    self.style.accent,
                );
            }
        }
    }

    pub fn draw_img(&mut self, enable_zoom: bool) {
        self.d.clear_background(self.style.bg);
        if let Some(images) = self.images {
            images.get_mut().update();

            let screen_width = self.d.get_screen_width().as_f32();
            let screen_height = self.d.get_screen_height().as_f32();
            let scale = {
                let screen_ratio = screen_width / screen_height;
                let texture_ratio = images.get().width().as_f32() / images.get().height().as_f32();

                if screen_ratio > texture_ratio {
                    screen_height / images.get().height().as_f32()
                } else {
                    screen_height / images.get().width().as_f32()
                }
            };
            let mouse_wheel = self.d.get_mouse_wheel_move();

            if mouse_wheel != 0.
                && enable_zoom
                && !((mouse_wheel < 0. && *self.mouse_wheel < 0.)
                    || (mouse_wheel > 0. && *self.mouse_wheel >= 25.))
            {
                let mouse_world_pos = self
                    .d
                    .get_screen_to_world2D(self.bounds.mouse_pos(), *self.camera);

                self.camera.offset = self.bounds.mouse_pos();
                self.camera.target = mouse_world_pos;
                self.camera.zoom += mouse_wheel * 0.25;

                if self.camera.zoom < 0.25 {
                    self.camera.zoom = 0.25;
                }

                *self.mouse_wheel += mouse_wheel;
            }

            fn draw_img_generic_mode<D>(
                d: &mut D,
                img: &FoximgImage,
                screen_width: f32,
                screen_height: f32,
                scale: f32,
            ) where
                D: RaylibDraw,
            {
                d.draw_texture_pro(
                    img,
                    Rectangle {
                        x: 0.,
                        y: 0.,
                        width: img.width().as_f32() * img.width_mult.as_f32(),
                        height: img.height().as_f32() * img.height_mult.as_f32(),
                    },
                    Rectangle {
                        x: screen_width / 2.,
                        y: screen_height / 2.,
                        width: img.width().as_f32() * scale,
                        height: img.height().as_f32() * scale,
                    },
                    Vector2::new((img.width().as_f32()) / 2., (img.height().as_f32()) / 2.) * scale,
                    img.rotation,
                    Color::WHITE,
                );
            }

            if *self.mouse_wheel > 0. {
                if self.bounds.mouse_pos().x >= self.bounds.left()
                    && self.bounds.mouse_pos().x <= self.bounds.right()
                    && self.d.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT)
                    && enable_zoom
                {
                    let mut delta = self.d.get_mouse_delta();
                    delta.scale(-1. / scale);
                    self.camera.target += delta;
                }

                let mut c = self.d.begin_mode2D(*self.camera);
                draw_img_generic_mode(&mut c, images.get(), screen_width, screen_height, scale);
            } else {
                draw_img_generic_mode(
                    &mut self.d,
                    images.get(),
                    screen_width,
                    screen_height,
                    scale,
                );
                self.camera.offset = Vector2::default();
                self.camera.target = Vector2::default();
            }
        }

        self.draw_img_symbols();
    }

    pub fn draw_btns(&mut self) {
        if let Some(images) = self.images {
            if self.bounds.mouse_pos_left() && images.can_dec() {
                self.d
                    .set_mouse_cursor(MouseCursor::MOUSE_CURSOR_POINTING_HAND);
                self.d.draw_texture_pro(
                    self.resources.grad_l(),
                    Rectangle {
                        x: 0.,
                        y: 0.,
                        width: self.resources.grad_l().width().as_f32(),
                        height: self.resources.grad_l().height().as_f32(),
                    },
                    Rectangle {
                        x: 0.,
                        y: 0.,
                        width: self.bounds.left(),
                        height: self.d.get_screen_height().as_f32(),
                    },
                    Vector2 { x: 0., y: 0. },
                    0.,
                    self.style.accent,
                );
            } else if self.bounds.mouse_pos_right() && images.can_inc() {
                self.d
                    .set_mouse_cursor(MouseCursor::MOUSE_CURSOR_POINTING_HAND);
                self.d.draw_texture_pro(
                    self.resources.grad_r(),
                    Rectangle {
                        x: 0.,
                        y: 0.,
                        width: self.resources.grad_r().width().as_f32(),
                        height: self.resources.grad_r().height().as_f32(),
                    },
                    Rectangle {
                        x: self.bounds.right(),
                        y: 0.,
                        width: self.bounds.left(),
                        height: self.d.get_screen_height().as_f32(),
                    },
                    Vector2 { x: 0., y: 0. },
                    0.,
                    self.style.accent,
                );
            } else {
                self.d.set_mouse_cursor(MouseCursor::MOUSE_CURSOR_ARROW);
            }
        }
    }

    pub fn draw_gradient(&mut self) {
        self.d.draw_rectangle(
            0,
            0,
            self.d.get_screen_width(),
            self.d.get_screen_height(),
            self.style.accent,
        );
    }

    fn draw_panel(&mut self, x: i32, y: i32, width: i32, height: i32) {
        const PANEL_BORDER_WIDTH: i32 = 2;

        fn color(d: &RaylibDrawHandle, property: GuiDefaultProperty) -> Color {
            Color::get_color(d.gui_get_style(GuiControl::DEFAULT, property as i32) as u32)
        }

        self.d.draw_rectangle(
            x,
            y,
            width,
            height,
            color(&self.d, GuiDefaultProperty::BACKGROUND_COLOR),
        );
        self.d.draw_rectangle(
            x,
            y,
            width,
            PANEL_BORDER_WIDTH,
            color(&self.d, GuiDefaultProperty::LINE_COLOR),
        );
        self.d.draw_rectangle(
            x,
            y + PANEL_BORDER_WIDTH,
            PANEL_BORDER_WIDTH,
            height - 2 * PANEL_BORDER_WIDTH,
            color(&self.d, GuiDefaultProperty::LINE_COLOR),
        );
        self.d.draw_rectangle(
            x + width - PANEL_BORDER_WIDTH,
            y + PANEL_BORDER_WIDTH,
            PANEL_BORDER_WIDTH,
            height - 2 * PANEL_BORDER_WIDTH,
            color(&self.d, GuiDefaultProperty::LINE_COLOR),
        );
        self.d.draw_rectangle(
            x,
            y + height - PANEL_BORDER_WIDTH,
            width,
            PANEL_BORDER_WIDTH,
            color(&self.d, GuiDefaultProperty::LINE_COLOR),
        );
    }
}

pub struct Foximg<'imgs> {
    state: FoximgState,
    style: FoximgStyle,
    rl: RaylibHandle,
    rl_thread: RaylibThread,
    fullscreen: bool,
    mouse_wheel: f32,
    camera: Camera2D,
    resources: FoximgResources,
    images: Option<FoximgImages<'imgs>>,
    should_exit: bool,
    instance: FoximgInstance,
}

impl Foximg<'_> {
    pub fn init(quiet: bool) -> Self {
        std::panic::set_hook(Box::new(foximg_log::panic));

        let (instance, instance_err) = FoximgInstance::new();
        let (state, state_err) = match instance.owner() {
            Ok(true) => FoximgState::new(FoximgState::PATH),
            Ok(false) => (FoximgState::default(), None),
            Err(e) => (FoximgState::default(), Some(e.into())),
        };
        let (style, style_err) = FoximgStyle::new(FoximgStyle::PATH);
        let (mut rl, rl_thread) = raylib::init()
            .vsync()
            .resizable()
            .size(state.w, state.h)
            .title("foximg")
            .log_level(if quiet {
                TraceLogLevel::LOG_NONE
            } else {
                match cfg!(debug_assertions) {
                    true => TraceLogLevel::LOG_ALL,
                    false => TraceLogLevel::LOG_INFO,
                }
            })
            .build();
        let _ = rl.set_trace_log_callback(foximg_log::tracelog);

        match instance_err {
            Some(e) => rl.trace_log(
                TraceLogLevel::LOG_WARNING,
                &format!("FOXIMG: Couldn't create instance file:\n - {e}"),
            ),
            None => {
                if let Some(path) = instance.path() {
                    rl.trace_log(
                        TraceLogLevel::LOG_INFO,
                        &format!("FOXIMG: Created instance file '{path:?}' successfully"),
                    )
                }
            }
        }
        match state_err {
            Some(e) => {
                rl.trace_log(
                    TraceLogLevel::LOG_WARNING,
                    &format!("FOXIMG: Error loading '{}':", FoximgState::PATH),
                );

                let err = format!(" - {e:?}");
                // Using println instead of the tracelog because of the maximum character limit imposed
                // by raylib.
                println!("{err}");
                foximg_log::push(&err);
            }
            None => rl.trace_log(
                TraceLogLevel::LOG_INFO,
                &format!("FOXIMG: '{}' loaded successfully", FoximgState::PATH),
            ),
        }
        match style_err {
            Some(e) => {
                let err_header = format!("Error loading '{}':", FoximgStyle::PATH);
                rl.trace_log(TraceLogLevel::LOG_WARNING, &format!("FOXIMG: {err_header}"));

                let err = format!(" - {e:?}");
                // Using println instead of the tracelog because of the maximum character limit imposed
                // by raylib.
                println!("{err}");
                foximg_log::push(&err);

                if let FoximgConfigError::TOML(_) = e {
                    foximg_error::show(&format!("{err_header}{err}"));
                }
            }
            None => rl.trace_log(
                TraceLogLevel::LOG_INFO,
                &format!("FOXIMG: '{}' loaded successfully", FoximgStyle::PATH),
            ),
        }

        style.update_style(&mut rl);
        if let Some((x, y)) = state.xy {
            rl.set_window_position(x, y);
        }
        if state.maximized {
            unsafe { ffi::MaximizeWindow() }
        }

        let fullscreen = state.fullscreen;
        if fullscreen {
            rl.toggle_borderless_windowed();
        }

        rl.set_exit_key(None);
        rl.set_target_fps(60);

        let mouse_wheel = 0.;
        let camera = Camera2D {
            zoom: 1.,
            ..Default::default()
        };

        let resources = FoximgResources::new(&mut rl, &rl_thread);

        rl.trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: === Opened Foximg ===");

        Self {
            state,
            style,
            rl,
            rl_thread,
            fullscreen,
            mouse_wheel,
            camera,
            resources,
            instance,
            should_exit: false,
            images: None,
        }
    }

    fn rotate_n90(&mut self) {
        if let Some(ref mut images) = self.images {
            images.get_mut().rotate_n90();
        }
    }

    fn rotate_90(&mut self) {
        if let Some(ref mut images) = self.images {
            images.get_mut().rotate_90();
        }
    }

    fn flip_horizontal(&mut self) {
        if let Some(ref mut images) = self.images {
            images.get_mut().flip_horizontal();
        }
    }

    fn flip_vertical(&mut self) {
        if let Some(ref mut images) = self.images {
            images.get_mut().flip_vertical();
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.rl.toggle_borderless_windowed();
        self.fullscreen = !self.fullscreen;
    }

    fn draw(&mut self, bounds: FoximgBtnsBounds) {
        let mut d = FoximgDraw::new_with_bounds(self, bounds);
        d.draw_img(true);
        d.draw_btns();
    }

    pub fn run(mut self, arg: Option<&str>) {
        if let Some(image) = arg {
            self.load_img(Path::new(image));
        }

        let mut debug_menu = false;

        while !self.should_exit {
            if self.rl.window_should_close() {
                self.should_exit = true;
            }

            if self.rl.is_file_dropped() {
                self.load_dropped();
            }

            // Toggle debug menu
            if (self.rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
                || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL))
                && (self.rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
                    || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT))
                && self.rl.is_key_pressed(KeyboardKey::KEY_D)
            {
                debug_menu = true;

            // Mirroring shortcuts
            } else if (self.rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
                || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT))
                && self.rl.is_key_pressed(KeyboardKey::KEY_Q)
            {
                self.flip_horizontal();
            } else if (self.rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
                || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT))
                && self.rl.is_key_pressed(KeyboardKey::KEY_E)
            {
                self.flip_vertical();

            // Rotation shortcuts
            } else if self.rl.is_key_pressed(KeyboardKey::KEY_Q) {
                self.rotate_n90();
            } else if self.rl.is_key_pressed(KeyboardKey::KEY_E) {
                self.rotate_90();

            // Fullscreen shortcut
            } else if self.rl.is_key_pressed(KeyboardKey::KEY_F11) {
                self.toggle_fullscreen();
            }

            let bounds = FoximgBtnsBounds::new(&self.rl);

            // Show menus
            if self
                .rl
                .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_RIGHT)
            {
                let mut menu = if debug_menu {
                    debug_menu = false;
                    FoximgMenu::init_debug(&mut self, bounds.mouse_pos())
                } else {
                    FoximgMenu::init(&mut self, bounds.mouse_pos())
                };
                menu.set_state();
                continue;
            }

            if let Some(ref mut images) = self.images {
                if self.rl.is_key_pressed(KeyboardKey::KEY_A) {
                    let mut_iter = images.try_dec_iter();
                    if let Some(mut_iter) = mut_iter {
                        mut_iter.dec_once(&self.rl, &self.rl_thread);
                    }
                } else if self.rl.is_key_pressed(KeyboardKey::KEY_D) {
                    let mut_iter = images.try_inc_iter();
                    if let Some(mut_iter) = mut_iter {
                        mut_iter.inc_once(&self.rl, &self.rl_thread);
                    }
                }

                if self
                    .rl
                    .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                {
                    if bounds.mouse_pos_left() {
                        let mut_iter = images.try_dec_iter();
                        if let Some(mut_iter) = mut_iter {
                            mut_iter.dec_once(&self.rl, &self.rl_thread);
                        }
                    } else if bounds.mouse_pos_right() {
                        let mut_iter = images.try_inc_iter();
                        if let Some(mut_iter) = mut_iter {
                            mut_iter.inc_once(&self.rl, &self.rl_thread);
                        }
                    }
                }
            }

            self.draw(bounds);
        }
    }
}

impl Drop for Foximg<'_> {
    fn drop(&mut self) {
        self.rl
            .trace_log(TraceLogLevel::LOG_INFO, "=== Closing Foximg ===");

        if let Ok(true) = self.instance.owner() {
            self.save_state();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut arg = args.get(1).map(|path| path.as_str());
    let mut quiet = false;

    if let Some("--help") = arg {
        foximg_help::show();
    } else {
        if let Some("--quiet") | Some("-q") = arg {
            arg = args.get(2).map(|path| path.as_str());
            quiet = true;
        }
        let foximg = Foximg::init(quiet);
        foximg.run(arg);
    }
}
