#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::{self, File, OpenOptions}, io::{self, IsTerminal, Write}, path::{Path, PathBuf}, str::Chars, sync::LazyLock, time::Duration
};

use aho_corasick::{AhoCorasick, MatchKind};
use config::{FoximgConfig, FoximgIcon, FoximgSettings, FoximgState, FoximgStyle};
use foximg_log::FoximgLogOut;
use images::FoximgImages;
use menu::FoximgMenu;
use raylib::prelude::*;
use resources::FoximgResources;

mod config;
mod controls;
mod foximg_log;
mod images;
mod menu;
mod resources;

struct FoximgInstance {
    path: PathBuf,
    update_counter: Option<f32>,
}

impl FoximgInstance {
    /// 6 hours - 1 second
    const UPDATE_DELAY: f32 = Duration::from_secs(3600 * 6 - 1).as_secs_f32();

    #[inline(always)]
    fn instance_path_current_exe(name: &str) -> io::Result<PathBuf> {
        let mut path = std::env::current_exe()?;

        path.pop();
        path.push(name);
        Ok(path)
    }

    fn instances_path_env(rl: Option<&mut RaylibHandle>, env: Result<PathBuf, std::env::VarError>) -> io::Result<PathBuf> {
        const INSTANCES_FOLDER_NO_RUNTIME_DIR: &str = if cfg!(target_os = "windows") {
            "instances"
        } else {
            ".foximg_instances"
        };

        if cfg!(debug_assertions) {
            return Self::instance_path_current_exe("instances");
        }

        let Ok(mut runtime) = env else {
            if let Some(rl) = rl {
                rl.trace_log(
                    TraceLogLevel::LOG_WARNING, 
                    "FOXIMG: \"XDG_RUNTIME_DIR\" enviroment variable not set. Using instance folder in executable's directory"
                );
            }
            return Self::instance_path_current_exe(INSTANCES_FOLDER_NO_RUNTIME_DIR);
        };

        runtime.push("foximg/instances");
        Ok(runtime)
    }

    pub fn instances_path() -> io::Result<PathBuf> {
        Self::instances_path_env(None, std::env::var("XDG_RUNTIME_DIR").map(|path| path.into()))
    }

    fn instance_count(instances_path: impl AsRef<Path>) -> io::Result<usize> {
        Ok(fs::read_dir(instances_path)?.count())
    }

    fn try_new(rl: &mut RaylibHandle) -> io::Result<Self> {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map(|path| path.into());
        let update_counter = runtime_dir.as_ref().map(|_| Self::UPDATE_DELAY).ok();
        let instances_path = Self::instances_path_env(Some(rl), runtime_dir)?;

        if !fs::exists(&instances_path)? {
            fs::create_dir_all(&instances_path)?;
        }

        let mut instances = Self::instance_count(&instances_path)?;
        let path = loop {
            let new_instance = instances.to_string();
            let path = instances_path.join(new_instance);
            if !path.exists() {
                break path;
            }
            instances += 1;
        };

        File::create(&path)?;
        Ok(Self { path, update_counter })
    }

    pub fn new(rl: &mut RaylibHandle) -> Option<Self> {
        match Self::try_new(rl) {
            Ok(instance) => {
                rl.trace_log(
                    TraceLogLevel::LOG_INFO,
                    &format!("FOXIMG: Created instance marker {:?}", instance.path),
                );
                Some(instance)
            }
            Err(e) => {
                rl.trace_log(
                    TraceLogLevel::LOG_WARNING,
                    "FOXIMG: Failed to create instance marker:",
                );
                rl.trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
                None
            }
        }
    }

    pub fn owner(&self) -> io::Result<bool> {
        Ok(Self::instance_count(Self::instances_path()?)? == 1)
    }

    fn try_update(&mut self) -> io::Result<()> {
        let file = OpenOptions::new().write(true).open(&self.path)?;
        let now = fs::FileTimes::new().set_accessed(std::time::SystemTime::now());
        file.set_times(now)?;
        Ok(())
    }

    pub fn update(&mut self, rl: &RaylibHandle) {
        let Some(ref mut update_counter) = self.update_counter else {
            return;
        };

        *update_counter -= rl.get_frame_time();
        if *update_counter <= 0. {
            *update_counter = Self::UPDATE_DELAY;

            if let Err(e) = self.try_update() {
                rl.trace_log(
                    TraceLogLevel::LOG_WARNING, 
                    &format!("FOXIMG: Failed to modify access time timestamp of {:?}:", self.path
                ));
                rl.trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
            } else {
                rl.trace_log(
                    TraceLogLevel::LOG_DEBUG, 
                    &format!("FOXIMG: Modified access time timestamp of {:?}", self.path
                ));
            }
        }
    }

    fn try_delete(&self) -> io::Result<()> {
        let instances_path = Self::instances_path()?;

        fs::remove_file(&self.path)?;
        if Self::instance_count(&instances_path)? == 0 {
            fs::remove_dir(instances_path)?;
        }
        Ok(())
    }

    pub fn delete(self, rl: &RaylibHandle) {
        if let Err(e) = self.try_delete() {
            rl.trace_log(
                TraceLogLevel::LOG_WARNING,
                "FOXIMG: Failed to delete instance marker:",
            );
            rl.trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
        } else {
            rl.trace_log(
                TraceLogLevel::LOG_INFO,
                &format!("FOXIMG: Deleted instance marker: {:?}", self.path),
            );
        }
    }
}

/// Represents the bounds of the side buttons that traverse the loaded image gallery on a current frame.
/// This struct holds just enough data to extrapolate the exact dimensions of each button.
///
/// It also holds information regarding the state of the current mouse position in relation to the
/// buttons: whether the mouse is hovering over either the left or right button.
#[derive(Default, Clone, Copy)]
struct FoximgBtnsBounds {
    btn_width: f32,
    btn_height: f32,
    right_btn_x: f32,
    mouse_on_left_btn: bool,
    mouse_on_right_btn: bool,
}

impl FoximgBtnsBounds {
    /// Constructs a new `FoximgBtnsBounds`. Takes in a [`RaylibHandle`] to calculate the width of
    /// the buttons based on the window's width, and a [`Vector2`] of the mouse's current position.
    /// Get the mouse position using [`get_mouse_position`].
    ///
    /// [`get_mouse_position`]: raylib::core::window::RaylibHandle::get_mouse_position
    pub fn new(rl: &RaylibHandle, mouse_pos: Vector2) -> Self {
        let window_width = rl.get_screen_width().as_f32();
        let window_height = rl.get_screen_height().as_f32();
        let btn_width = window_width / 6.;
        let right_btn_x = window_width - btn_width;
        let mouse_on_left_btn = mouse_pos.x < btn_width;
        let mouse_on_right_btn = mouse_pos.x > right_btn_x;

        Self {
            btn_height: window_height,
            btn_width,
            right_btn_x,
            mouse_on_left_btn,
            mouse_on_right_btn,
        }
    }

    pub const fn left_btn(&self) -> Rectangle {
        Rectangle::new(0., 0., self.btn_width, self.btn_height)
    }

    pub const fn right_btn(&self) -> Rectangle {
        Rectangle::new(self.right_btn_x, 0., self.btn_width, self.btn_height)
    }

    /// Returns whether the mouse is hovering over the left button.
    pub fn mouse_on_left_btn(&self) -> bool {
        self.mouse_on_left_btn
    }

    /// Returns whether the mouse is hovering over the right button.
    pub fn mouse_on_right_btn(&self) -> bool {
        self.mouse_on_right_btn
    }
}

/// Represents a foximg frame that can be be drawn to.
struct FoximgDraw<'a> {
    d: RaylibDrawHandle<'a>,
    style: &'a FoximgStyle,
    state: &'a FoximgState,
    resources: &'a FoximgResources,
    mouse_wheel: &'a mut f32,
    camera: &'a mut Camera2D,
    title: &'a str,
    rl_thread: &'a RaylibThread,
    btn_bounds: FoximgBtnsBounds,
    scaleto: bool,
}

impl<'a> FoximgDraw<'a> {
    pub fn draw_large_centered_text(&mut self, text: &str) {
        const FONT_SIZE: f32 = 32.;
        const FONT_SPACING: f32 = resources::yudit_spacing(FONT_SIZE);

        let screen_width = self.d.get_screen_width() as f32;
        let screen_height = self.d.get_screen_height() as f32;
        let yudit = &self.resources.yudit;
        let text_width = yudit.measure_text(text, FONT_SIZE, FONT_SPACING).x;

        self.d.draw_text_ex(
            yudit,
            text,
            rvec2(
                screen_width / 2. - text_width / 2.,
                screen_height / 2. - FONT_SIZE / 2.,
            ),
            FONT_SIZE,
            FONT_SPACING,
            self.style.accent,
        );
    }

    pub fn draw_current_img(&mut self, images: &mut FoximgImages) {
        let Some(img) = images.img_get(&mut self.d, self.rl_thread) else {
            self.draw_large_centered_text(":(");
            return;
        };

        img.borrow_mut().update_texture(&self.d);
        let img = img.borrow();

        let screen_width = self.d.get_screen_width().as_f32();
        let screen_height = self.d.get_screen_height().as_f32();
        let scale = if self.scaleto { 1. } else {
            let screen_ratio = screen_width / screen_height;
            let texture_ratio = img.width().as_f32() / img.height().as_f32();

            if screen_ratio > texture_ratio {
                screen_height / img.height().as_f32()
            } else {
                screen_height / img.width().as_f32()
            }
        };

        if *self.mouse_wheel > 0. {
            let mut c = self.d.begin_mode2D(*self.camera);
            img.draw_center_scaled(&mut c, screen_width, screen_height, scale);
        } else {
            *self.camera = Camera2D {
                zoom: 1.,
                ..Default::default()
            };
            img.draw_center_scaled(&mut self.d, screen_width, screen_height, scale);
        }
        img.draw_manipulation_info(
            &mut self.d,
            self.resources,
            self.style,
            screen_width,
            screen_height,
        );

        if self.state.fullscreen {
            const FONT_SIZE: f32 = 16.;
            const FONT_SPACING: f32 = resources::yudit_spacing(FONT_SIZE);

            self.d.draw_text_ex(
                &self.resources.yudit, 
                self.title, 
                rvec2(10, 10), 
                FONT_SIZE, 
                FONT_SPACING, 
                self.style.accent
            );
        }
    }

    fn draw_btns(&mut self, images: &mut FoximgImages) {
        if self.btn_bounds.mouse_on_left_btn() && images.can_dec() {
            self.d.draw_texture_pro(
                &self.resources.grad,
                rrect(
                    0,
                    0,
                    self.resources.grad.width(),
                    self.resources.grad.height(),
                ),
                self.btn_bounds.left_btn(),
                rvec2(0, 0),
                0.,
                self.style.accent,
            );
        } else if self.btn_bounds.mouse_on_right_btn() && images.can_inc() {
            self.d.draw_texture_pro(
                &self.resources.grad,
                rrect(
                    0,
                    0,
                    -self.resources.grad.width(),
                    self.resources.grad.height(),
                ),
                self.btn_bounds.right_btn(),
                rvec2(0, 0),
                0.,
                self.style.accent,
            );
        }
    }

    pub fn begin(
        foximg: &'a mut Foximg,
        f: impl FnOnce(FoximgDraw<'a>, Option<&'a mut Box<FoximgImages>>),
    ) {
        let d = foximg.rl.begin_drawing(&foximg.rl_thread);
        let mut d = Self {
            d,
            style: &foximg.style,
            state: &foximg.state,
            resources: &foximg.resources,
            mouse_wheel: &mut foximg.mouse_wheel,
            camera: &mut foximg.camera,
            title: &foximg.title,
            rl_thread: &foximg.rl_thread,
            btn_bounds: foximg.btn_bounds,
            scaleto: foximg.scaleto,
        };

        d.d.clear_background(foximg.style.bg);
        if let (None | Some(FoximgLock::Images), None) = (foximg.lock, &foximg.images) {
            d.draw_large_centered_text("drag + drop an image");
        }

        f(d, foximg.images.as_mut());
    }
}

pub struct Foximg {
    style: FoximgStyle,
    state: FoximgState,
    settings: FoximgSettings,
    resources: FoximgResources,
    images: Option<Box<FoximgImages>>,

    mouse_pos: Vector2,
    btn_bounds: FoximgBtnsBounds,
    mouse_wheel: f32,
    camera: Camera2D,

    lock: Option<FoximgLock>,
    title_format: String,
    title: String,
    scaleto: bool,

    rl: RaylibHandle,
    rl_thread: RaylibThread,
    instance: Option<FoximgInstance>,
}

impl Foximg {
    pub fn init(
        lock: Option<FoximgLock>,
        verbose: bool, 
        override_state: Option<FoximgState>, 
        override_style: Option<FoximgStyle>,
        title_format: String, 
        scaleto: bool
    ) -> Self {
        // SAFETY: As of raylib-rs 5.5.1, this always returns Ok.
        callbacks::set_trace_log_callback(foximg_log::tracelog).unwrap();

        let mut rl_builder = raylib::init();
        rl_builder.vsync()
            .log_level(if verbose {
                TraceLogLevel::LOG_ALL
            } else {
                TraceLogLevel::LOG_INFO
            });

        if !scaleto {
            rl_builder.resizable();
        }

        let (mut rl, rl_thread) = rl_builder.build();
        rl.set_exit_key(None);
        rl.set_target_fps(60);

        let title = self::format_title(&mut rl, &rl_thread, &title_format, None);
        rl.set_window_title(&rl_thread, &title);

        // We don't remember state if it's manually overriden using arguments.
        let instance = if override_state.is_some() {
            None
        } else {
            FoximgInstance::new(&mut rl)
        };

        // Style must be initialized before state because on Windows the titlebar's color gets updated
        // only once it's resized. The window can't get resized if it's already maximized, so the
        // window appears in light mode on startup otherwise.
        let style = override_style
            .inspect(|style| {
                rl.trace_log(TraceLogLevel::LOG_INFO, "Loaded style from arguments:");
                style.update(&mut rl);
            })
            .unwrap_or_else(|| FoximgStyle::new(&mut rl));

        let state = if instance
            .as_ref()
            .is_some_and(|instance| matches!(instance.owner(), Ok(true)))
        {
            FoximgState::new(&mut rl)
        } else {
            override_state.inspect(|state| {
                rl.trace_log(TraceLogLevel::LOG_INFO, "Loaded state from arguments:");
                state.update(&mut rl);
            }).unwrap_or_default()
        };

        let settings = FoximgSettings::new(&mut rl);
        let resources = FoximgResources::new(&mut rl, &rl_thread);
        let icon = FoximgIcon::new(&mut rl);

        images::set_window_icon(&mut rl, &style, icon);
        rl.trace_log(
            TraceLogLevel::LOG_INFO,
            "FOXIMG: Foximg initialized successfully",
        );

        Self {
            images: None,
            mouse_pos: Vector2::zero(),
            btn_bounds: FoximgBtnsBounds::default(),
            mouse_wheel: 0.,
            camera: Camera2D {
                zoom: 1.,
                ..Default::default()
            },
            state,
            settings,
            style,
            resources,
            lock,
            title_format,
            title,
            scaleto,
            rl,
            rl_thread,
            instance,
        }
    }

    fn toggle_fullscreen(&mut self) {
        if self.rl.is_key_pressed(KeyboardKey::KEY_F11) {
            self.state.fullscreen = !self.state.fullscreen;
            self.rl.toggle_borderless_windowed();
        }
    }

    fn update(&mut self) {
        if let Some(ref mut instsance) = self.instance {
            instsance.update(&self.rl);
        }

        self.toggle_fullscreen();
        self.mouse_pos = self.rl.get_mouse_position();
    }

    fn get_dropped_img(&mut self) {
        if self.rl.is_file_dropped() {
            let files = self.rl.load_dropped_files();
            if let Some(path) = files.paths().first() {
                self.load_folder(path);
            }
        }
    }

    fn update_mouse_cursor(&mut self) {
        if let Some(ref images) = self.images {
            if self.rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT) && self.mouse_wheel > 0.
                || self.btn_bounds.mouse_on_left_btn() && images.can_dec()
                || self.btn_bounds.mouse_on_right_btn() && images.can_inc()
            {
                self.rl
                    .set_mouse_cursor(MouseCursor::MOUSE_CURSOR_POINTING_HAND);
            } else {
                self.rl.set_mouse_cursor(MouseCursor::MOUSE_CURSOR_DEFAULT);
            }
        }
    }

    fn manipulate_img(&mut self) {
        // // We want to poll for only one of these events every frame
        static POLL_IMG_EVENTS: &[fn(&mut Foximg) -> bool] = &[
            Foximg::zoom_in1_img, 
            Foximg::zoom_out1_img, 
            Foximg::zoom_in5_img,
            Foximg::zoom_out5_img,
            Foximg::flip_horizontal_img,
            Foximg::flip_vertical_img,
            Foximg::rotate_n1_img,
            Foximg::rotate_1_img,
            Foximg::rotate_n90_img,
            Foximg::rotate_90_img,
            Foximg::update_gallery,
        ];

        POLL_IMG_EVENTS.iter().find(|event| event(self));
        self.zoom_scroll_img();
        self.pan_img();
        self.pan_img_up();
        self.pan_img_down();
        self.pan_img_left();
        self.pan_img_right();
    }

    pub fn run(mut self, path: Option<&str>) {
        if let Some(path) = path {
            self.load_folder(path);
        }

        while !self.rl.window_should_close() {
            self.update();
            self.btn_bounds = FoximgBtnsBounds::new(&self.rl, self.mouse_pos);
            if let None | Some(FoximgLock::Images) = self.lock {
                self.get_dropped_img();
                self.update_mouse_cursor();
                self.manipulate_img();
    
                if self
                    .rl
                    .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_RIGHT)
                {
                    let keep_running = FoximgMenu::init(&mut self).run();
                    if !keep_running {
                        return;
                    }
                }
            }

            FoximgDraw::begin(&mut self, |mut d, images| {
                if let Some(images) = images {
                    d.draw_current_img(images);
                    d.draw_btns(images);
                }
            });
        }
    }
}

impl Drop for Foximg {
    fn drop(&mut self) {
        if let Some(instance) = self.instance.take() {
            match instance.owner() {
                Ok(true) => self.save_state(),
                Ok(false) => (),
                Err(e) => {
                    self.rl.trace_log(
                        TraceLogLevel::LOG_WARNING,
                        "FOXIMG: Failed to get whether this is the only instance:",
                    );
                    self.rl
                        .trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
                }
            }
            instance.delete(&self.rl)
        }
    }
}

pub fn format_title(
    rl: &mut RaylibHandle,
    rl_thread: &RaylibThread,
    title: &str,
    mut images: Option<&mut FoximgImages>,
) -> String {
    const PATTERN_PATH: usize = 0;
    const PATTERN_HEIGHT: usize = 1;
    const PATTERN_NAME: usize = 2;
    const PATTERN_IMAGES_LEN: usize = 3;
    const PATTERN_IMAGES_CURRENT: usize = 4;
    const PATTERN_WIDTH: usize = 5;
    const PATTERNS_LEN: usize = 8;

    static AC: LazyLock<AhoCorasick> = LazyLock::new(|| {
        static TITLE_PATTERNS: [&str; PATTERNS_LEN] = ["%f", "%h", "%n", "%l", "%u", "%w", "%v", "\\%"];

        AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostFirst)
            .build(TITLE_PATTERNS)
            .unwrap()
    });

    let mut replace_with: [String; PATTERNS_LEN] = [
        String::new(),
        "0".into(),
        String::new(),
        "0".into(),
        "0".into(),
        "0".into(),
        if cfg!(debug_assertions) {
            concat!(env!("CARGO_PKG_VERSION"), " [DEBUG BUILD]")
        } else {
            env!("CARGO_PKG_VERSION")
        }.into(),
        "%".into(),
    ];

    if let Some(ref mut images) = images {
        let path = images.img_path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        replace_with[PATTERN_PATH] = path.display().to_string();
        replace_with[PATTERN_NAME] = name;
        replace_with[PATTERN_IMAGES_LEN] = images.len().to_string();
        replace_with[PATTERN_IMAGES_CURRENT] = images.img_current().to_string();
        images.img_with(rl, rl_thread, |img| {
            replace_with[PATTERN_HEIGHT] = img.height().to_string();
            replace_with[PATTERN_WIDTH] = img.width().to_string();
        });
    }

    let title = AC.replace_all(title, &replace_with);
    title
        .split("%!")
        .enumerate()
        .filter(|(i, _)| images.is_some() || i % 2 == 0)
        .map(|(_, title)| title)
        .collect::<Vec<_>>()
        .concat()
}

enum FoximgMode {
    Help(Option<anyhow::Error>),
    Version,
    Normal,
}

#[derive(Clone, Copy)]
pub enum FoximgLock {
    Images,
    Ui,
}

struct FoximgArgs<'a> {
    mode: FoximgMode,

    lock: Option<FoximgLock>,
    scaleto: bool,
    state: Option<FoximgState>,
    style: Option<FoximgStyle>,
    title: Option<&'a str>,
    verbose: bool,
    path: Option<&'a str>,
}

impl<'a> FoximgArgs<'a> {
    pub fn new() -> Self {
        Self {
            mode: FoximgMode::Normal,
            lock: None,
            scaleto: false,
            state: None,
            style: None,
            title: None,
            verbose: cfg!(debug_assertions),
            path: None,
        }
    }

    fn set_lock(&mut self) {
        if self.lock.is_none() {
            self.lock = Some(FoximgLock::Images);
        } else {
            self.lock = Some(FoximgLock::Ui);
        }
    }

    fn parse_long_option(&mut self, arg: &'a str) -> Result<(), Option<anyhow::Error>> {
        if arg == "--help" {
            return Err(None);
        } else if arg == "--lock" {
            self.set_lock();
        } else if arg == "--quiet" {
            foximg_log::quiet(true);
        } else if arg == "--scaleto" {
            self.scaleto = true;
        } else if let Some(state) = arg.strip_prefix("--state") {
            return self::parse_option_with_arg(arg, state, |state| {
                self::parse_toml_arg(&mut self.state, state)
            });
        } else if let Some(style) = arg.strip_prefix("--style") {
            return self::parse_option_with_arg(arg, style, |style| {
                self::parse_toml_arg(&mut self.style, style)
            });
        } else if let Some(title) = arg.strip_prefix("--title") {
            return self::parse_option_with_arg(arg, title, |title| {
                self.title = Some(title);
                Ok(())
            });
        } else if arg == "--verbose" {
            self.verbose = true;
        } else if arg == "--version" {
            self.mode = FoximgMode::Version;
        } else {
            return Err(Some(anyhow::anyhow!("Unknown option \"{arg}\"")));
        }
        Ok(())
    }

    fn parse_short_option(&mut self, arg: Chars) -> Result<(), Option<anyhow::Error>> {
        for c in arg {
            if c == 'h' {
                return Err(None);
            } else if c == 'l' {
                self.set_lock();
            } else if c == 'q' {
                foximg_log::quiet(true);
            } else if c == 's' {
                self.scaleto = true;
            } else if c == 'v' {
                self.verbose = true;
            } else {
                return Err(Some(anyhow::anyhow!("Unknown option \"-{c}\"")));
            } 
        }
        Ok(())
    }

    pub fn parse_args(mut self, args: &'a [String]) -> Box<dyn FnOnce() + 'a> {
        let mut args = args.iter();
        // First argument always is the application path.
        args.next();

        while let Some(arg) = args.next().map(|arg| arg.as_str()) {
            let is_short_option = arg.chars().nth(0) == Some('-') 
                && arg.chars().nth(1) != Some('-');

            let is_long_option = arg.chars().nth(0) == Some('-') 
                && arg.chars().nth(1) == Some('-');

            if is_long_option {
                if let Err(e) = self.parse_long_option(arg) {
                    self.mode = FoximgMode::Help(e);
                }
            } else if is_short_option {
                let arg = arg[1..].chars();
                if let Err(e) = self.parse_short_option(arg) {
                    self.mode = FoximgMode::Help(e);
                }
            } else if self.path.is_none() && !is_short_option && !is_long_option {
                self.path = Some(arg);
                break;
            }
        }

        match self.mode {
            FoximgMode::Help(e) => Box::new(|| self::help(e)),
            FoximgMode::Normal => Box::new(|| self::run(self)),
            FoximgMode::Version => Box::new(self::version),
        }
    }
}

fn parse_option_with_arg<'a, F>(
    option: &str, 
    option_arg: &'a str,
    f: F,
) -> Result<(), Option<anyhow::Error>>
where 
    F: FnOnce(&'a str) -> Result<(), Option<anyhow::Error>>,
{
    if option_arg.is_empty() {
        return Err(Some(anyhow::anyhow!("\"{option}\" must have an argument")));
    } else if option_arg.chars().nth(0) != Some('=') {
        return Err(Some(anyhow::anyhow!("Unknown option \"{option}\"")));
    }

    let option_arg = &option_arg[1..];
    f(option_arg)
}

fn parse_toml_arg<T>(arg: &mut Option<T>, toml: &str) -> Result<(), Option<anyhow::Error>>
where 
    T: for<'de> serde::Deserialize<'de>
{
    const MAX_DEPTH: i32 = 2;

    let mut depth = 0;
    let mut toml = toml.replace(';', "\n");
    loop {
        depth += 1;
        if depth > MAX_DEPTH {
            return Err(Some(anyhow::anyhow!("Reached max depth when parsing TOML file")));
        }

        match toml::from_str(&toml) {
            Ok(state) => *arg = Some(state),
            _ if Path::new(&toml).exists() => {
                toml = fs::read_to_string(toml).unwrap();
                continue;
            },
            Err(e) => return Err(Some(e.into())),
        };
                    
        break Ok(());
    }
}

fn main() {
    std::panic::set_hook(Box::new(foximg_log::panic));

    #[cfg(all(debug_assertions, target_os = "windows"))]
    if let Err(e) = self::set_vt() {
        foximg_log::tracelog(
            TraceLogLevel::LOG_WARNING,
            "FOXIMG: Failed to enable virtual terminal processing. Log output is not guaranteed to look elligible:",
        );
        foximg_log::tracelog(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
    }

    let args: Vec<String> = std::env::args().collect();
    FoximgArgs::new().parse_args(&args)()
}

#[cfg(all(debug_assertions, target_os = "windows"))]
fn set_vt() -> windows::core::Result<()> {
    use windows::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
        STD_OUTPUT_HANDLE, SetConsoleMode,
    };

    unsafe {
        let hout = GetStdHandle(STD_OUTPUT_HANDLE)?;
        let mut mode = CONSOLE_MODE::default();

        GetConsoleMode(hout, &mut mode)?;
        mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;

        SetConsoleMode(hout, mode)?;
    }
    Ok(())
}

fn stdout_error(what: &str, e: io::Error) {
    const ERROR_COLOR: &str = "\x1b[1m\x1b[38;5;202m";
    const RESET_COLOR: &str = "\x1b[0m";

    eprintln!("{ERROR_COLOR}ERROR: {RESET_COLOR}Printing {what} to stdout failed: {e}");
}

fn help(e: Option<anyhow::Error>) {
    if let Err(e) = self::try_help(e) {
        self::stdout_error("help", e);
    }
}

fn try_help(e: Option<anyhow::Error>) -> io::Result<()> {
    const FOXIMG_VERSION: &str = env!("CARGO_PKG_VERSION");
    const FOXIMG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

    let mut error_color = String::new();
    let mut gray_color = String::new();
    let mut green_color = String::new();
    let mut reset_color = String::new();
    let mut pink_color = String::new();
    let mut out = std::io::stdout();

    if out.is_terminal() {
        error_color = "\x1b[1m\x1b[38;5;202m".into();
        gray_color = "\x1b[3m\x1b[38;5;8m".into();
        green_color = "\x1b[38;5;114m".into();
        reset_color = "\x1b[0m".into();
        pink_color = "\x1b[1m\x1b[38;5;219m".into();
    }

    if let Some(e) = e {
        writeln!(out, "{error_color}ERROR: {reset_color}{e}\n")?;
        writeln!(out, "Use \"foximg -h\" or \"foximg --help\" to see all the available options.")?;
        return Ok(());
    }

    writeln!(out, "{pink_color}foximg {FOXIMG_VERSION}:{reset_color} {FOXIMG_DESCRIPTION}\n")?;
    writeln!(out, "{green_color}Usage:{reset_color}")?;
    writeln!(out, "    foximg {gray_color}[OPTION...] [PATH]{reset_color}")?;
    writeln!(out, "{green_color}Options:{reset_color}")?;
    writeln!(out, "    {gray_color}-h, --help          {reset_color}Print help")?;
    writeln!(out, "    {gray_color}-l, --lock          {reset_color}Show only the input image. Use -ll to lock the UI as well")?;
    writeln!(out, "    {gray_color}-q, --quiet         {reset_color}Don't print log messages")?;
    writeln!(out, "    {gray_color}-s, --scaleto       {reset_color}Scale window to the size of the current image")?;
    writeln!(out, "    {gray_color}    --state=TOML    {reset_color}Set window's state according to the format in foximg_state.toml")?;
    writeln!(out, "    {gray_color}    --style=TOML    {reset_color}Set window's style according to the format in foximg_style.toml")?;
    writeln!(out, "    {gray_color}    --title=FORMAT  {reset_color}Set window's title")?;
    writeln!(out, "    {gray_color}-v, --verbose       {reset_color}Make TRACE and DEBUG log messages")?;
    writeln!(out, "    {gray_color}    --version       {reset_color}Print foximg's version")?;
    writeln!(out, "\n{green_color}TOML:{reset_color}")?;
    writeln!(out, "    Use either a TOML document with newlines substituted by semicolons, or a path to a TOML document.")?;
    writeln!(out, "\n{green_color}FORMAT specifiers:{reset_color}")?;
    writeln!(out, "    {gray_color}%f  {reset_color}Current image's path")?;
    writeln!(out, "    {gray_color}%h  {reset_color}Current image's height")?;
    writeln!(out, "    {gray_color}%n  {reset_color}Current image's name")?;
    writeln!(out, "    {gray_color}%l  {reset_color}Number of images loaded")?;
    writeln!(out, "    {gray_color}%u  {reset_color}Current image's number")?;
    writeln!(out, "    {gray_color}%w  {reset_color}Current image's width")?;
    writeln!(out, "    {gray_color}%v  {reset_color}foximg's version")?;
    writeln!(out, "    {gray_color}%!  {reset_color}If no images, omit the text on the right side until another {gray_color}%!{reset_color} or end of text")?;
    Ok(())
}

fn run(args: FoximgArgs) {
    foximg_log::out(FoximgLogOut::Stdout(std::io::stdout()));

    let default_format = if args.lock.is_none() {
        "foximg %v%! \n[%u of %l] - %f"
    } else {
        "foximg %v%! \n- %f"
    };

    let foximg = Foximg::init(
        args.lock,
        args.verbose, 
        args.state,
        args.style,
        args.title.unwrap_or(default_format).to_string(), 
        args.scaleto
    );
    
    foximg.run(args.path);
    foximg_log::tracelog(
        TraceLogLevel::LOG_INFO,
        "FOXIMG: Foximg uninitialized successfully. Goodbye!",
    );
}

fn version() {
    let out = std::io::stdout();
    if let Err(e) = writeln!(&out, "{}", env!("CARGO_PKG_VERSION")) {
        self::stdout_error("version", e);
    }
}
