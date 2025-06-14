//! Defines the basic controls for manipulating the current image or zooming in and out.

use crate::Foximg;
use raylib::prelude::*;

const MOUSE_WHEEL_MIN: f32 = 0.;
const MOUSE_WHEEL_MAX: f32 = 25.;

impl Foximg {
    /// Returns true if either left or right Shift is held down.
    fn is_shift_down(&self) -> bool {
        self.rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
            || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT)
    }

    /// Returns true if either left or right Ctrl is held down.
    fn is_control_down(&self) -> bool {
        self.rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
            || self.rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL)
    }

    /// Zooms in the image by `current_mouse_wheel` * `ZOOM_MULTIPLIER`.
    pub fn zoom_img(&mut self, current_mouse_wheel: f32) {
        const ZOOM_MULTIPLIER: f32 = 0.4;

        if let Some(ref images) = self.images {
            if images.img_failed() {
                return;
            }

            if !((current_mouse_wheel < 0. && self.mouse_wheel <= self::MOUSE_WHEEL_MIN)
                || (current_mouse_wheel > 0. && self.mouse_wheel >= self::MOUSE_WHEEL_MAX))
            {
                let mouse_world_pos = self.rl.get_screen_to_world2D(self.mouse_pos, self.camera);
                self.camera.offset = self.mouse_pos;
                self.camera.target = mouse_world_pos;
                self.camera.zoom += current_mouse_wheel * ZOOM_MULTIPLIER;

                if self.camera.zoom < 1. {
                    self.camera.zoom = 1.;
                    self.mouse_wheel = 0.;
                } else {
                    self.mouse_wheel += current_mouse_wheel;
                    self.mouse_wheel = self
                        .mouse_wheel
                        .clamp(self::MOUSE_WHEEL_MIN, self::MOUSE_WHEEL_MAX);
                }
            }
        }
    }

    /// Zooms in the image by 0.1 when Ctrl+W is held down. Returns `true` if so
    pub fn zoom_in1_img(&mut self) -> bool {
        if self.is_control_down() && self.rl.is_key_down(KeyboardKey::KEY_W) {
            self.zoom_img(0.1);
            true
        } else {
            false
        }
    }

    /// Zooms out the image by 0.1 when Ctrl+S is held down. Returns `true` if so.
    pub fn zoom_out1_img(&mut self) -> bool {
        if self.is_control_down() && self.rl.is_key_down(KeyboardKey::KEY_S) {
            self.zoom_img(-0.1);
            true
        } else {
            false
        }
    }

    /// Zooms in the image by 0.5 when W is held down. Returns `true` if so.
    pub fn zoom_in5_img(&mut self) -> bool {
        if self.rl.is_key_down(KeyboardKey::KEY_W) {
            self.zoom_img(0.5);
            true
        } else {
            false
        }
    }

    /// Zooms out the image by 0.5 when S is held down. Returns `true` if so.
    pub fn zoom_out5_img(&mut self) -> bool {
        if self.rl.is_key_down(KeyboardKey::KEY_S) {
            self.zoom_img(-0.5);
            true
        } else {
            false
        }
    }

    /// Flips the image horizontally if Shift+Q is pressed. Returns true if so.
    pub fn flip_horizontal_img(&mut self) -> bool {
        let is_shift_down = self.is_shift_down();
        if let Some(ref mut images) = self.images {
            if is_shift_down && self.rl.is_key_pressed(KeyboardKey::KEY_Q) {
                images.flip_horizontal(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    /// Flips the image vertically if Shift+E is pressed. Returns true if so.
    pub fn flip_vertical_img(&mut self) -> bool {
        let is_shift_down = self.is_shift_down();
        if let Some(ref mut images) = self.images {
            if is_shift_down && self.rl.is_key_pressed(KeyboardKey::KEY_E) {
                images.flip_vertical(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    /// Rotates the image -1 deg if Ctrl+Q. Returns true if so.
    pub fn rotate_n1_img(&mut self) -> bool {
        let is_control_down = self.is_control_down();
        if let Some(ref mut images) = self.images {
            if is_control_down && self.rl.is_key_down(KeyboardKey::KEY_Q) {
                images.rotate_n1(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    /// Rotates the image 1 deg if Ctrl+E. Returns true if so.
    pub fn rotate_1_img(&mut self) -> bool {
        let is_control_down = self.is_control_down();
        if let Some(ref mut images) = self.images {
            if is_control_down && self.rl.is_key_down(KeyboardKey::KEY_E) {
                images.rotate_1(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    /// Rotates the image -90 deg if Q. Returns true if so.
    pub fn rotate_n90_img(&mut self) -> bool {
        if let Some(ref mut images) = self.images {
            if self.rl.is_key_pressed(KeyboardKey::KEY_Q) {
                images.rotate_n90(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    /// Rotates the image 90 deg if E. Returns true if so.
    pub fn rotate_90_img(&mut self) -> bool {
        if let Some(ref mut images) = self.images {
            if self.rl.is_key_pressed(KeyboardKey::KEY_E) {
                images.rotate_90(&mut self.rl, &self.rl_thread);
                return true;
            }
        }
        false
    }

    fn skip_count_to_usize(&mut self) -> usize {
        self.skip_count
            .parse()
            .inspect(|_| self.skip_count.clear())
            .unwrap()
    }

    /// Updates the current image on the gallery. Goes to the next one if D is pressed, and goes to
    /// the previous one if A is pressed. Returns true if so.
    pub fn update_gallery(&mut self) -> bool {
        let mut res = false;
        self.images_with(|f, images| {
            let pressed_a = f.rl.is_key_pressed(KeyboardKey::KEY_A);
            let pressed_d = f.rl.is_key_pressed(KeyboardKey::KEY_D);
            let amount = if !f.skip_count.is_empty() && (pressed_a || pressed_d) {
                f.skip_count_to_usize()
            } else {
                1
            };

            if images.can_dec()
                && (f.rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                    && f.btn_bounds.mouse_on_left_btn())
                || pressed_a
            {
                images.dec(f, amount);
                res = true;
            } else if images.can_inc()
                && (f.rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT)
                    && f.btn_bounds.mouse_on_right_btn())
                || pressed_d
            {
                images.inc(f, amount);
                res = true;
            }
        });

        res
    }

    /// Zooms in or out according to the scroll wheel.
    pub fn zoom_scroll_img(&mut self) {
        let current_mouse_wheel = self.rl.get_mouse_wheel_move();
        if current_mouse_wheel != 0. {
            self.zoom_img(current_mouse_wheel);
        }
    }

    pub fn pan_img(&mut self) {
        if self.mouse_wheel > 0.
            && self.mouse_pos.x >= self.btn_bounds.left_btn().width
            && self.mouse_pos.x <= self.btn_bounds.right_btn().x
            && self.rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT)
        {
            let mut delta = self.rl.get_mouse_delta();
            delta.scale(-1.);
            self.camera.target += delta;
        }
    }

    fn pan_img_direction<F>(&mut self, vim: KeyboardKey, arrow: KeyboardKey, f: F)
    where
        F: FnOnce(&mut Self, f32),
    {
        const PAN_MIN: f32 = self::MOUSE_WHEEL_MAX / 3.;
        const PAN_MAX: f32 = self::MOUSE_WHEEL_MAX - PAN_MIN;

        if self.mouse_wheel > 0. && (self.rl.is_key_down(vim) || self.rl.is_key_down(arrow)) {
            let d = self.mouse_wheel.clamp(PAN_MIN, PAN_MAX);
            let ctrl = self.is_control_down();
            f(self, if ctrl { d / 2. } else { d });
        }
    }

    pub fn pan_img_up(&mut self) {
        self.pan_img_direction(KeyboardKey::KEY_K, KeyboardKey::KEY_UP, |f, d| {
            f.camera.target.y -= d
        });
    }

    pub fn pan_img_down(&mut self) {
        self.pan_img_direction(KeyboardKey::KEY_J, KeyboardKey::KEY_DOWN, |f, d| {
            f.camera.target.y += d
        });
    }

    pub fn pan_img_left(&mut self) {
        self.pan_img_direction(KeyboardKey::KEY_H, KeyboardKey::KEY_LEFT, |f, d| {
            f.camera.target.x -= d
        });
    }

    pub fn pan_img_right(&mut self) {
        self.pan_img_direction(KeyboardKey::KEY_L, KeyboardKey::KEY_RIGHT, |f, d| {
            f.camera.target.x += d
        });
    }

    pub fn jump_to(&mut self) -> bool {
        let mut res = false;
        self.images_with(|f, images| {
            if !f.skip_count.is_empty() && f.rl.is_key_pressed(KeyboardKey::KEY_G) {
                let goto = f.skip_count_to_usize().clamp(1, images.len()) - 1;

                images.set_current(goto);
                images.update_window(f);
                res = true;
            }
        });

        res
    }

    pub fn jump_to_end(&mut self) -> bool {
        let mut res = false;
        self.images_with(|f, images| {
            if f.is_shift_down() && f.rl.is_key_pressed(KeyboardKey::KEY_FOUR) {
                images.set_current(images.len() - 1);
                images.update_window(f);
                res = true;
            }
        });

        res
    }

    pub fn delete_skip(&mut self) -> bool {
        if !self.skip_count.is_empty() && self.rl.is_key_pressed(KeyboardKey::KEY_BACKSPACE) {
            self.skip_count.pop();
            true
        } else {
            false
        }
    }

    pub fn escape_skip(&mut self) -> bool {
        if !self.skip_count.is_empty() && self.rl.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
            self.skip_count.clear();
            true
        } else {
            false
        }
    }

    pub fn skip_images(&mut self) -> bool {
        if self.images.is_none() {
            return false;
        }

        let Some(key) = self.rl.get_key_pressed() else {
            return false;
        };

        if key as u32 >= KeyboardKey::KEY_ONE as u32 && key as u32 <= KeyboardKey::KEY_NINE as u32 {
            self.skip_count.push(key as u32 as u8 as char);
            true
        } else if self.rl.is_key_pressed(KeyboardKey::KEY_ZERO) && !self.skip_count.is_empty() {
            self.skip_count.push('0');
            true
        } else {
            false
        }
    }

    pub fn jump_to_start(&mut self) -> bool {
        let mut res = false;
        self.images_with(|f, images| {
            if f.rl.is_key_pressed(KeyboardKey::KEY_ZERO) {
                images.set_current(0);
                images.update_window(f);
                res = true;
            }
        });

        res
    }
}
