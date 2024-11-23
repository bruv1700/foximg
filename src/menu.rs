use std::ffi::CString;

use raylib::prelude::*;

use crate::{foximg_log, Foximg, FoximgDraw};

type FoximgMenuBtnCallback<'imgs> = dyn FnMut(&mut Foximg<'imgs>);

#[derive(Default)]
struct FoximgMenuBtn<'imgs> {
    caption: CString,
    callback: Option<Box<FoximgMenuBtnCallback<'imgs>>>,
}

impl<'imgs> FoximgMenuBtn<'imgs> {
    pub const HEIGHT: f32 = 20.;
    pub const WIDTH: f32 = 145.;

    pub fn new(caption: CString, callback: Option<Box<FoximgMenuBtnCallback<'imgs>>>) -> Self {
        Self { caption, callback }
    }

    pub fn draw(&self, d: &mut FoximgDraw, rect: &Rectangle) {
        match self.callback {
            Some(_) => {
                d.d.gui_button(rect, Some(self.caption.as_c_str()));
            }
            None => {
                d.d.gui_disable();
                d.d.gui_button(rect, Some(self.caption.as_c_str()));
                d.d.gui_enable();
            }
        };
    }

    fn callback_mut(&mut self) -> Option<&mut Box<FoximgMenuBtnCallback<'imgs>>> {
        self.callback.as_mut()
    }
}

impl FoximgDraw<'_, '_> {
    pub(self) fn draw_menu(&mut self, btns: &[(FoximgMenuBtn, Rectangle)]) {
        for (btn, rect) in btns {
            btn.draw(self, rect);
        }
    }
}

pub struct FoximgMenu<'a, 'imgs> {
    foximg: &'a mut Foximg<'imgs>,
    menu_rect: Rectangle,
    btns: Box<[(FoximgMenuBtn<'imgs>, Rectangle)]>,
}

impl<'a, 'imgs> FoximgMenu<'a, 'imgs> {
    fn init_from_array<const N: usize>(
        foximg: &'a mut Foximg<'imgs>,
        menu_xy: Vector2,
        btns: [FoximgMenuBtn<'imgs>; N],
    ) -> FoximgMenu<'a, 'imgs> {
        FoximgMenu {
            foximg,
            menu_rect: Rectangle::new(
                menu_xy.x,
                menu_xy.y,
                FoximgMenuBtn::WIDTH,
                FoximgMenuBtn::HEIGHT * N as f32,
            ),
            btns: btns
                .into_iter()
                .enumerate()
                .map(|(i, btns)| {
                    (
                        btns,
                        Rectangle::new(
                            menu_xy.x,
                            menu_xy.y + FoximgMenuBtn::HEIGHT * i as f32,
                            FoximgMenuBtn::WIDTH,
                            FoximgMenuBtn::HEIGHT,
                        ),
                    )
                })
                .collect(),
        }
    }

    pub fn init_debug(foximg: &'a mut Foximg<'imgs>, menu_xy: Vector2) -> Self {
        const BTNS_LEN: usize = 1;

        let mut btns_captions: [CString; BTNS_LEN] = [CString::from(rstr!("Create log file"))];
        let mut btns_callbacks: [Option<Box<FoximgMenuBtnCallback>>; BTNS_LEN] =
            [Some(Box::new(|foximg| match foximg_log::create_file() {
                Ok(_) => {
                    foximg
                        .rl
                        .trace_log(TraceLogLevel::LOG_INFO, foximg_log::CREATED_LOG_FILE_MSG);
                    let _ = native_dialog::MessageDialog::new()
                        .set_type(native_dialog::MessageType::Info)
                        .set_title("foximg - Info")
                        .set_text(foximg_log::CREATED_LOG_FILE_MSG)
                        .show_alert();
                }
                Err(e) => foximg.rl.trace_log(
                    TraceLogLevel::LOG_ERROR,
                    &format!("Couldn't create log file: {e:#}"),
                ),
            }))];
        let btns: [FoximgMenuBtn; BTNS_LEN] = std::array::from_fn(move |i| {
            let caption = std::mem::take(&mut btns_captions[i]);
            let callback = std::mem::take(&mut btns_callbacks[i]);
            FoximgMenuBtn::new(caption, callback)
        });

        Self::init_from_array(foximg, menu_xy, btns)
    }

    pub fn init(foximg: &'a mut Foximg<'imgs>, menu_xy: Vector2) -> Self {
        const BTNS_LEN: usize = BTNS_LEN_IMGS + BTNS_LEN_NO_IMGS;
        const BTNS_LEN_NO_FULLSCREEN: usize = BTNS_LEN - 1;
        const BTNS_LEN_NO_IMGS: usize = 2;
        const BTNS_LEN_NO_IMGS_FULLSCREEN: usize = BTNS_LEN_NO_IMGS + 1;
        const BTNS_LEN_IMGS: usize = 5;

        let mut btns_captions: [CString; BTNS_LEN] = [
            CString::from(rstr!("Rotate -90° (Q)")),
            CString::from(rstr!("Rotate 90° (E)")),
            CString::from(rstr!("Flip Horizontally (Shift-Q)")),
            CString::from(rstr!("Flip Vertically (Shift-E)")),
            match foximg.images {
                Some(ref images) => rstr!("{} x {}", images.get().width(), images.get().height()),
                None => CString::from(rstr!("0 x 0")),
            },
            CString::from(rstr!("Toggle Fullscreen (F11)")),
            CString::from(rstr!("Exit")),
        ];
        let mut btns_callbacks: [Option<Box<FoximgMenuBtnCallback>>; BTNS_LEN] = [
            Some(Box::new(Foximg::rotate_n90)),
            Some(Box::new(Foximg::rotate_90)),
            Some(Box::new(Foximg::flip_horizontal)),
            Some(Box::new(Foximg::flip_vertical)),
            None,
            Some(Box::new(Foximg::toggle_fullscreen)),
            Some(Box::new(|foximg| foximg.should_exit = true)),
        ];
        let mut btns: [FoximgMenuBtn; BTNS_LEN] = std::array::from_fn(move |i| {
            let caption = std::mem::take(&mut btns_captions[i]);
            let callback = std::mem::take(&mut btns_callbacks[i]);
            FoximgMenuBtn::new(caption, callback)
        });

        match foximg.images {
            Some(_) if foximg.fullscreen => {
                Self::init_from_array::<BTNS_LEN>(foximg, menu_xy, btns)
            }
            Some(_) => Self::init_from_array::<BTNS_LEN_NO_FULLSCREEN>(
                foximg,
                menu_xy,
                std::array::from_fn(|i| std::mem::take(&mut btns[i])),
            ),
            None if foximg.fullscreen => Self::init_from_array::<BTNS_LEN_NO_IMGS_FULLSCREEN>(
                foximg,
                menu_xy,
                std::array::from_fn(|i| std::mem::take(&mut btns[i + BTNS_LEN_IMGS - 1])),
            ),
            None => Self::init_from_array::<BTNS_LEN_NO_IMGS>(
                foximg,
                menu_xy,
                std::array::from_fn(|i| std::mem::take(&mut btns[i + BTNS_LEN_IMGS - 1])),
            ),
        }
    }

    fn draw(&mut self) {
        let mut d = FoximgDraw::new(self.foximg);
        d.draw_img(false);
        d.draw_menu(&self.btns);
    }

    pub fn set_state(&mut self) {
        self.foximg
            .rl
            .set_mouse_cursor(MouseCursor::MOUSE_CURSOR_DEFAULT);

        'exit_foximg: loop {
            if self.foximg.rl.window_should_close() {
                break 'exit_foximg;
            }

            let mouse_pos = self.foximg.rl.get_mouse_position();

            if self
                .foximg
                .rl
                .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
            {
                for (btn, ref rect) in &mut self.btns {
                    if rect.check_collision_point_rec(mouse_pos) {
                        if let Some(callback) = btn.callback_mut() {
                            callback(self.foximg);
                            return;
                        }
                    } else if !self.menu_rect.check_collision_point_rec(mouse_pos) {
                        return;
                    }
                }
            } else if self
                .foximg
                .rl
                .is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_RIGHT)
            {
                let mouse_pos = self.foximg.rl.get_mouse_position();
                self.menu_rect.x = mouse_pos.x;
                self.menu_rect.y = mouse_pos.y;
                for (i, (_, rect)) in self.btns.iter_mut().enumerate() {
                    rect.x = mouse_pos.x;
                    rect.y = mouse_pos.y + FoximgMenuBtn::HEIGHT * i as f32;
                }
            }

            self.draw();
        }

        self.foximg.should_exit = true;
    }
}
