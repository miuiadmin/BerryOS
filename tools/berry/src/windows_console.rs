//! Windows console behavior: keep a normal console binary so cmd/PowerShell
//! CLI output works; hide the console window for a no-arg double-click before
//! opening the device shell.

use std::ffi::c_void;

const SW_HIDE: i32 = 0;

#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleWindow() -> *mut c_void;
}

#[link(name = "user32")]
extern "system" {
    fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
}

pub fn hide_console() {
    // SAFETY: Win32 calls on the current process console. A null window means
    // there is no console (e.g. launched by a service); nothing to hide.
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}
