use std::ffi::c_void;

use raylib::prelude::*;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
    },
    core::BOOL,
};

use super::FoximgStyle;

impl FoximgStyle {
    /// Updates the color of the title bar according to whether theme is dark or not. This only
    /// does something on Windows 10 and above.
    fn update_titlebar_color(&self, rl: &mut RaylibHandle, hwnd: HWND) -> bool {
        unsafe {
            let value: BOOL = BOOL::from(self.dark);

            if let Err(e) = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &value as *const BOOL as *const c_void,
                size_of::<BOOL>() as u32,
            ) {
                rl.trace_log(
                    TraceLogLevel::LOG_WARNING,
                    "FOXIMG: Failed to update title bar color:",
                );
                rl.trace_log(TraceLogLevel::LOG_WARNING, &format!("    > {e}"));
                false
            } else {
                true
            }
        }
    }

    pub(super) fn update_titlebar_win32(&self, rl: &mut RaylibHandle) {
        let hwnd = HWND(unsafe { rl.get_window_handle() });
        if !self.update_titlebar_color(rl, hwnd) {
            return;
        }

        // Quickly resize window to update the titlebar.
        rl.set_window_size(rl.get_screen_width() + 1, rl.get_screen_height());
        rl.set_window_size(rl.get_screen_width() - 1, rl.get_screen_height());
    }
}
