pub mod cred;
pub mod gui;

use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::{HANDLE, TRUE};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, CONSOLE_MODE, CTRL_C_EVENT, ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle,
    STD_INPUT_HANDLE, SetConsoleCtrlHandler, SetConsoleMode,
};

pub struct Hidden(Option<(HANDLE, CONSOLE_MODE)>);

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub fn attach_console() {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

pub fn on_ctrl_c() {
    unsafe extern "system" fn handle(kind: u32) -> i32 {
        if kind != CTRL_C_EVENT || crate::CANCEL.swap(true, Ordering::Relaxed) {
            std::process::exit(130);
        }
        TRUE
    }

    unsafe { SetConsoleCtrlHandler(Some(handle), TRUE) };
}

impl Hidden {
    pub fn new(hidden: bool) -> Self {
        Self(hidden.then(|| unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            let mut mode = 0;
            GetConsoleMode(input, &mut mode);
            SetConsoleMode(input, mode & !ENABLE_ECHO_INPUT);
            (input, mode)
        }))
    }
}

impl Drop for Hidden {
    fn drop(&mut self) {
        if let Some((input, mode)) = self.0 {
            unsafe { SetConsoleMode(input, mode) };
            eprintln!();
        }
    }
}
