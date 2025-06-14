use raylib::prelude::*;

use crate::{Foximg, FoximgDraw, resources};

#[derive(PartialEq)]
enum MenuBtnType {
    OnPressedExit(fn(&mut FoximgMenu) -> bool),
    OnPressed(fn(&mut FoximgMenu)),
    OnDown(fn(&mut FoximgMenu)),
    SubMenu(&'static [MenuBtn]),
}

#[derive(PartialEq)]
struct MenuBtn {
    pub name: &'static str,
    pub shortcut_text: Option<&'static str>,
    pub btn_type: MenuBtnType,
}

impl MenuBtn {
    pub const HEIGHT: f32 = 20.;
    pub const WIDTH: f32 = 180.;

    pub const fn new(name: &'static str, btn_type: MenuBtnType) -> Self {
        Self {
            name,
            btn_type,
            shortcut_text: None,
        }
    }

    pub const fn new_shortcut(
        name: &'static str,
        btn_type: MenuBtnType,
        shortcut: &'static str,
    ) -> Self {
        Self {
            name,
            btn_type,
            shortcut_text: Some(shortcut),
        }
    }

    pub fn update(&self, fm: &mut FoximgMenu) -> (bool, bool) {
        match self.btn_type {
            MenuBtnType::OnPressedExit(event) => {
                if fm
                    .f
                    .rl
                    .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                {
                    return (false, event(fm));
                }
            }
            MenuBtnType::OnPressed(event) => {
                if fm
                    .f
                    .rl
                    .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                {
                    event(fm)
                }
            }
            MenuBtnType::OnDown(event) => {
                if fm.f.rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT) {
                    event(fm)
                }
            }
            MenuBtnType::SubMenu(_) => {
                if fm
                    .f
                    .rl
                    .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                {
                    fm.delay = 0.
                }
            }
        }
        (true, true)
    }
}

/// The index at which the foximg right-click menu must be shown from when no image gallery is loaded.
const FOXIMG_MENU_NO_IMAGES: usize = 3;

static FOXIMG_MENU: &[MenuBtn] = {
    const EXIT_SHORTCUT: &str = if cfg!(target_os = "windows") {
        "Alt+F4"
    } else {
        ""
    };

    static FOXIMG_MENU_ROTATE: &[MenuBtn] = &[
        MenuBtn::new_shortcut("+90 deg", MenuBtnType::OnPressed(btn_90deg), "E"),
        MenuBtn::new_shortcut("-90 deg", MenuBtnType::OnPressed(btn_n90deg), "Q"),
        MenuBtn::new_shortcut("+1 deg", MenuBtnType::OnDown(btn_1deg), "Ctrl+E"),
        MenuBtn::new_shortcut("-1 deg", MenuBtnType::OnDown(btn_n1deg), "Ctrl+Q"),
    ];

    static FOXIMG_MENU_MIRROR: &[MenuBtn] = &[
        MenuBtn::new_shortcut(
            "Horizontal",
            MenuBtnType::OnPressed(btn_horizontal),
            "Shift+Q",
        ),
        MenuBtn::new_shortcut("Vertical", MenuBtnType::OnPressed(btn_vertical), "Shift+E"),
    ];

    static FOXIMG_MENU_NAVIGATE: &[MenuBtn] = &[
        MenuBtn::new_shortcut(
            "First Image",
            MenuBtnType::OnPressedExit(btn_first_img),
            "0",
        ),
        MenuBtn::new_shortcut(
            "Last Image",
            MenuBtnType::OnPressedExit(btn_last_img),
            "Shift+4",
        ),
    ];

    fn btn_open(fm: &mut FoximgMenu<'_>) -> bool {
        static FILTER: (&[&str], &str) = (
            &[
                "*.jpg", "*.jpeg", "*.jpe", "*.jif", "*.jfif", "*.jfi", "*.dds", "*.hdr", "*.ico",
                "*.qoi", "*.tiff", "*.pgm", "*.pbm", "*.ppm", "*.pnm", "*.exr", "*.apng", "*.png",
                "*.webp", "*.gif",
            ],
            "Image File",
        );

        if let Some(path) = tinyfiledialogs::open_file_dialog("Open...", "", Some(FILTER)) {
            fm.f.load_folder(path);
        } else {
            fm.f.rl
                .trace_log(TraceLogLevel::LOG_INFO, "FOXIMG: No file opened");
        }

        true
    }

    fn btn_toggle_fullscreen(fm: &mut FoximgMenu<'_>) -> bool {
        fm.f.state.fullscreen = !fm.f.state.fullscreen;
        fm.f.rl.toggle_borderless_windowed();
        true
    }

    fn btn_90deg(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.rotate_90(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_n90deg(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.rotate_n90(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_1deg(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.rotate_1(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_n1deg(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.rotate_n1(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_horizontal(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.flip_horizontal(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_vertical(fm: &mut FoximgMenu<'_>) {
        if let Some(ref mut images) = fm.f.images {
            images.flip_vertical(&mut fm.f.rl, &fm.f.rl_thread);
        }
    }

    fn btn_first_img(fm: &mut FoximgMenu<'_>) -> bool {
        fm.f.images_with(|f, images| {
            images.set_current(0);
            images.update_window(f);
        });

        true
    }

    fn btn_last_img(fm: &mut FoximgMenu<'_>) -> bool {
        fm.f.images_with(|f, images| {
            images.set_current(images.len() - 1);
            images.update_window(f);
        });

        true
    }

    &[
        MenuBtn::new("Rotate", MenuBtnType::SubMenu(FOXIMG_MENU_ROTATE)),
        MenuBtn::new("Mirror", MenuBtnType::SubMenu(FOXIMG_MENU_MIRROR)),
        MenuBtn::new("Navigate", MenuBtnType::SubMenu(FOXIMG_MENU_NAVIGATE)),
        MenuBtn::new("Open...", MenuBtnType::OnPressedExit(btn_open)),
        MenuBtn::new_shortcut(
            "Toggle Fullscreen",
            MenuBtnType::OnPressedExit(btn_toggle_fullscreen),
            "F11",
        ),
        MenuBtn::new_shortcut("Exit", MenuBtnType::OnPressedExit(|_| false), EXIT_SHORTCUT),
    ]
};

struct FoximgUpdateSubMenu<'a, 'b> {
    fm: &'b mut FoximgMenu<'a>,
    col: usize,
    row: usize,
}

impl<'a, 'b> FoximgUpdateSubMenu<'a, 'b> {
    const CLOSE_DELAY: f32 = 600.;

    fn open_sub_menu(&mut self, sub_menu: &'static [MenuBtn]) {
        self.fm.f.rl.trace_log(
            TraceLogLevel::LOG_DEBUG,
            &format!("FOXIMG: Opened sub-menu (Depth: {})", self.col + 1),
        );
        self.fm.menus.push(sub_menu);
        self.fm.rects.push(self::get_rect(
            rvec2(self.fm.rects[self.col].x, self.fm.rects[self.col].y)
                + rvec2(
                    MenuBtn::WIDTH as u32,
                    MenuBtn::HEIGHT as u32 * self.row as u32,
                ),
            sub_menu,
        ));
        self.fm.delay = Self::CLOSE_DELAY;
        self.fm.showing = (self.col, self.row);
    }

    fn close_sub_menu(&mut self) -> bool {
        self.fm.delay -= self.fm.f.rl.get_frame_time() * 1000.;
        self.fm.delay = self.fm.delay.clamp(0., Self::CLOSE_DELAY);
        if self.fm.delay > 0. {
            return false;
        }

        self.fm.f.rl.trace_log(
            TraceLogLevel::LOG_DEBUG,
            &format!("FOXIMG: Closed sub-menu (Depth: {})", self.col + 1),
        );
        self.fm.menus.truncate(self.col + 1);
        self.fm.rects.truncate(self.col + 1);
        self.fm.showing = self.fm.hovering_on;
        true
    }

    pub fn new(fm: &'b mut FoximgMenu<'a>) -> Self {
        let col = fm.hovering_on.0;
        let row = fm.hovering_on.1;
        Self { fm, col, row }
    }

    pub fn update(mut self) {
        if let MenuBtnType::SubMenu(sub_menu) = self.fm.menus[self.col][self.row].btn_type {
            if self.fm.menus.len() < self.col + 2
                || (sub_menu != self.fm.menus[self.col + 1] && self.close_sub_menu())
            {
                self.open_sub_menu(sub_menu);
            }
        } else if self.fm.menus.len() >= self.col + 2 {
            self.close_sub_menu();
        } else {
            self.fm.showing = self.fm.hovering_on;
        }
    }
}

impl FoximgDraw<'_> {
    fn draw_menu_shadow(&mut self, menu: &'static [MenuBtn], x: f32, y: f32) {
        let shadow_x = x + MenuBtn::HEIGHT / 8.;
        let shadow_y = y + MenuBtn::HEIGHT / 8.;

        self.d.draw_rectangle(
            shadow_x as i32,
            shadow_y as i32,
            MenuBtn::WIDTH as i32,
            MenuBtn::HEIGHT as i32 * menu.len() as i32,
            self.style.bg.alpha(0.5),
        );
    }

    fn draw_menu(&mut self, menu: &'static [MenuBtn], x: f32, mut y: f32) {
        for btn in menu {
            self.d
                .gui_button(rrect(x, y, MenuBtn::WIDTH, MenuBtn::HEIGHT), btn.name);

            let border_color = Color::get_color(
                self.d
                    .gui_get_style(GuiControl::DEFAULT, GuiControlProperty::BORDER_COLOR_NORMAL)
                    as u32,
            );

            const PADDING: f32 = 6.;

            if let MenuBtnType::SubMenu(_) = btn.btn_type {
                let mut point_a = rvec2(x + MenuBtn::WIDTH, y + MenuBtn::HEIGHT / 2.);
                let mut point_b = rvec2(x + MenuBtn::WIDTH - MenuBtn::HEIGHT, y);
                let mut point_c = rvec2(x + MenuBtn::WIDTH - MenuBtn::HEIGHT, y + MenuBtn::HEIGHT);

                point_a.x -= PADDING;
                point_b.x += PADDING;
                point_b.y += PADDING;
                point_c.x += PADDING;
                point_c.y -= PADDING;

                self.d
                    .draw_triangle(point_a, point_b, point_c, border_color);
            }

            y += MenuBtn::HEIGHT;

            if let Some(shortcut_text) = btn.shortcut_text {
                const BUTTON_Y_OFFSET: f32 = 1.;
                const FONT_SIZE: f32 = resources::BUTTON_FONT_SIZE;
                const FONT_SPACING: f32 = resources::yudit_spacing(FONT_SIZE);

                let text_size =
                    self.resources
                        .yudit
                        .measure_text(shortcut_text, FONT_SIZE, FONT_SPACING);
                let text_position = rvec2(x + MenuBtn::WIDTH, y)
                    - text_size
                    - rvec2(PADDING, PADDING / 2. + BUTTON_Y_OFFSET);

                self.d.draw_text_ex(
                    &self.resources.yudit,
                    shortcut_text,
                    text_position,
                    FONT_SIZE,
                    FONT_SPACING,
                    border_color,
                );
            }
        }
    }

    fn draw_menu_objects(
        &mut self,
        menus: &[&'static [MenuBtn]],
        rects: &[Rectangle],
        hovering_on: (usize, usize),
        showing: (usize, usize),
        draw: fn(&mut Self, menu: &'static [MenuBtn], x: f32, y: f32),
    ) {
        let col = showing.0;
        let row = showing.1;
        let showing = &menus[col][row];

        if let MenuBtnType::SubMenu(sub_menu) = showing.btn_type {
            draw(self, sub_menu, rects[col + 1].x, rects[col + 1].y);
        }

        for i in 0..=hovering_on.0 {
            draw(self, menus[i], rects[i].x, rects[i].y);
        }
    }
}

pub struct FoximgMenu<'a> {
    f: &'a mut Foximg,

    menus: Vec<&'static [MenuBtn]>,
    rects: Vec<Rectangle>,
    hovering_on: (usize, usize),
    showing: (usize, usize),
    delay: f32,
}

impl<'a> FoximgMenu<'a> {
    pub fn init(f: &'a mut Foximg) -> Self {
        /// Maximum number of submenus + 1.
        const MAX_DEPTH: usize = 2;

        let mut menus = Vec::with_capacity(MAX_DEPTH);
        menus.push(if f.images.is_some() {
            self::FOXIMG_MENU
        } else {
            &self::FOXIMG_MENU[self::FOXIMG_MENU_NO_IMAGES..]
        });

        let mut rects = Vec::with_capacity(MAX_DEPTH);
        rects.push(self::get_rect(f.mouse_pos, menus[0]));

        let hovering_on = (0, 0);
        f.rl.trace_log(TraceLogLevel::LOG_DEBUG, "FOXIMG: Opened right-click menu");

        Self {
            f,
            menus,
            rects,
            hovering_on,
            showing: hovering_on,
            delay: 0.,
        }
    }

    fn get_bounds(&self) -> Option<(usize, usize)> {
        self.rects
            .iter()
            .enumerate()
            .find(|(_, rect)| rect.check_collision_point_rec(self.f.mouse_pos))
            .map(|(x, rect)| {
                let y = ((self.f.mouse_pos.y - rect.y) / MenuBtn::HEIGHT) as usize;
                (x, y)
            })
    }

    fn update_hovering_on(&mut self) {
        if let Some((col, row)) = self.get_bounds() {
            self.hovering_on = (col, row);
        }
    }

    fn update_sub_menu(&mut self) {
        FoximgUpdateSubMenu::new(self).update();
    }

    fn update_pos(&mut self) {
        let x = self.rects[0].x;
        let y = self.rects[0].y;
        let old_pos = rvec2(x, y);
        let pos_dif = self.f.mouse_pos - old_pos;

        for rect in &mut self.rects {
            rect.x += pos_dif.x;
            rect.y += pos_dif.y;
        }
    }

    pub fn run(mut self) -> bool {
        self.f.rl.set_mouse_cursor(MouseCursor::MOUSE_CURSOR_ARROW);

        while !self.f.rl.window_should_close() {
            self.f.update();
            self.update_hovering_on();
            self.update_sub_menu();

            if self
                .f
                .rl
                .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                && self.get_bounds().is_none()
            {
                return true;
            } else if self
                .f
                .rl
                .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_RIGHT)
                && self.get_bounds().is_none()
            {
                self.update_pos();
            }

            let col = self.hovering_on.0;
            let row = self.hovering_on.1;
            let (keep_menu, event_result) = self.menus[col][row].update(&mut self);

            if !keep_menu {
                return event_result;
            }

            FoximgDraw::begin(self.f, |mut d, images| {
                if let Some(images) = images {
                    d.draw_current_img(images);
                }

                d.draw_menu_objects(
                    &self.menus,
                    &self.rects,
                    self.hovering_on,
                    self.showing,
                    FoximgDraw::draw_menu_shadow,
                );

                d.draw_menu_objects(
                    &self.menus,
                    &self.rects,
                    self.hovering_on,
                    self.showing,
                    FoximgDraw::draw_menu,
                );
            });
        }

        false
    }
}

impl Drop for FoximgMenu<'_> {
    fn drop(&mut self) {
        self.f
            .rl
            .trace_log(TraceLogLevel::LOG_DEBUG, "FOXIMG: Closed right-click menu");
    }
}

fn get_rect(pos: Vector2, menu: &'static [MenuBtn]) -> Rectangle {
    rrect(
        pos.x,
        pos.y,
        MenuBtn::WIDTH,
        MenuBtn::HEIGHT * menu.len() as f32,
    )
}
