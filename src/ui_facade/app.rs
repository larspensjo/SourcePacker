// Manages application instance and the message loop

use super::error::{Result, UiError};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage},
    },
    core::PCWSTR,
};

#[derive(Clone, Copy)] // HINSTANCE is effectively a pointer, so Copy is fine
pub struct App {
    pub instance: HINSTANCE,
}

impl App {
    pub fn new() -> Result<Self> {
        unsafe {
            // GetModuleHandleW with null gets the handle to the current executable
            let instance = GetModuleHandleW(PCWSTR::null())?;
            Ok(App {
                instance: instance.into(), // Convert HMODULE to HINSTANCE
            })
        }
    }

    pub fn run(&self) -> Result<()> {
        unsafe {
            let mut msg = MSG::default();
            // HWND::default() (which is HWND(0)) means retrieve messages for any window
            // belonging to the current thread.
            while GetMessageW(&mut msg, Some(HWND::default()), 0, 0).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        // GetMessageW returns 0 (false) on WM_QUIT, -1 on error.
        // If it's -1, an error occurred, but windows-rs's .as_bool() might mask this
        // specific error reporting. For now, assume WM_QUIT means success.
        Ok(())
    }
}
