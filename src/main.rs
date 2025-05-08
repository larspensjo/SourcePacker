// P0.3: Basic main.rs and Window

#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]

use std::ffi::c_void;
use windows::{
    Win32::{
        Foundation::{GetLastError, HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{
            BeginPaint,
            COLOR_WINDOW, // Moved here and accessed as COLOR_WINDOW.0
            EndPaint,
            /*FillRect, GetStockObject,*/ HBRUSH,
            PAINTSTRUCT, /*WHITE_BRUSH,*/
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*, // For CS_*, WS_*, IDI_*, IDC_*, WM_*, etc.
    },
    core::*,
};

fn main() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let hinstance: HMODULE = instance.into();

        let window_class_name = w!("SourcePackerWindowClass");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance.into(), // Corrected: HMODULE to HINSTANCE
            hIcon: LoadIconW(None, IDI_APPLICATION)?,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // COLOR_WINDOW is SYS_COLOR_INDEX(u32), access its value with .0
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: window_class_name,
            hIconSm: LoadIconW(None, IDI_APPLICATION)?,
        };

        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            println!(
                "Failed to register window class. Error: {:?}",
                GetLastError()
            );
            return Err(Error::from_win32());
        }

        // CreateWindowExW in recent windows-rs returns Result<HWND, Error>
        // Use '?' to propagate error or get the HWND
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class_name,
            w!("SourcePacker - Basic Window"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            None,                   // Parent window: None for top-level
            None,                   // Menu: None for no menu
            Some(hinstance.into()), // Corrected: Option<HINSTANCE>
            None,
        )?; // This '?' handles the Result from CreateWindowExW

        // The ShowWindow and UpdateWindow calls are not strictly necessary
        // if WS_VISIBLE is used in CreateWindowExW and if the system
        // sends an initial WM_PAINT, but they don't hurt.
        // ShowWindow(hwnd, SW_SHOWDEFAULT);
        // UpdateWindow(hwnd)?;

        let mut msg = MSG::default();
        // HWND::default() for GetMessageW means messages for any window on this thread
        while GetMessageW(&mut msg, Some(HWND::default()), 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_DESTROY => {
                println!("WM_DESTROY received");
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_PAINT => {
                println!("WM_PAINT received");
                let mut ps = PAINTSTRUCT::default();
                let _hdc = BeginPaint(hwnd, &mut ps); // Prefix with _ if not used yet

                // Example: FillRect(...) would go here if uncommented
                // and FillRect, GetStockObject, WHITE_BRUSH were used.

                EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
