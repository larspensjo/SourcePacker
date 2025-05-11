use super::app::Win32ApiInternalState; // The shared internal state
use super::control_treeview; // For TreeView specific data and event handling
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, PlatformCommand, WindowId}; // For event generation

use windows::{
    Win32::{
        Foundation::{
            ERROR_INVALID_WINDOW_HANDLE, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM,
        },
        Graphics::Gdi::{BeginPaint, COLOR_WINDOW, EndPaint, FillRect, HBRUSH, PAINTSTRUCT}, // For basic painting
        System::SystemServices::IMAGE_DOS_HEADER, // Used to get base address for HINSTANCE
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR},
};

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

// Control IDs
pub(crate) const ID_BUTTON_GENERATE_ARCHIVE: i32 = 1002;
const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON"); // Helper for button class name

// Layout constants
pub const BUTTON_AREA_HEIGHT: i32 = 50; // Also used in other files.
const BUTTON_X_PADDING: i32 = 10;
const BUTTON_Y_PADDING_IN_AREA: i32 = 10; // Padding from top of button area to button
const BUTTON_WIDTH: i32 = 150;
const BUTTON_HEIGHT: i32 = 30;

/// Holds native data associated with a specific window managed by the platform layer.
/// This includes the native window handle (`HWND`) and any control-specific states.
#[derive(Debug)]
pub(crate) struct NativeWindowData {
    pub(crate) hwnd: HWND,
    pub(crate) id: WindowId, // The platform-agnostic ID for this window
    /// State specific to a TreeView control, if one exists in this window.
    pub(crate) treeview_state: Option<control_treeview::TreeViewInternalState>,
    /// Handle to the "Generate Archive" button, if created.
    pub(crate) hwnd_button_generate: Option<HWND>,
}

/// Context passed to `CreateWindowExW` via `lpCreateParams`.
/// This allows the static `WndProc` to retrieve the necessary `Arc`-ed state
/// for the specific window instance being created.
struct WindowCreationContext {
    internal_state_arc: Arc<Win32ApiInternalState>,
    window_id: WindowId,
}

/// Registers the main window class for the application.
///
/// This function should be called once, typically during platform initialization,
/// before any windows are created. It uses the application name from `Win32ApiInternalState`
/// to create a unique class name.
pub(crate) fn register_window_class(
    internal_state: &Arc<Win32ApiInternalState>,
) -> PlatformResult<()> {
    let class_name_hstring = HSTRING::from(format!(
        "{}_PlatformWindowClass",
        internal_state.app_name_for_class
    ));
    let class_name_pcwstr = PCWSTR(class_name_hstring.as_ptr());

    unsafe {
        // Check if class is already registered
        let mut wc_test = WNDCLASSEXW::default();
        if GetClassInfoExW(
            Some(internal_state.h_instance),
            class_name_pcwstr,
            &mut wc_test,
        )
        .is_ok()
        {
            // Class already registered, no need to do it again.
            return Ok(());
        }

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC, // CS_OWNDC can be useful for custom rendering
            lpfnWndProc: Some(facade_wnd_proc_router),
            cbClsExtra: 0,
            cbWndExtra: 0, // We use GWLP_USERDATA for per-instance context
            hInstance: internal_state.h_instance,
            hIcon: LoadIconW(None, IDI_APPLICATION)?, // Default application icon
            hCursor: LoadCursorW(None, IDC_ARROW)?,   // Default arrow cursor
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void), // Default window background
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name_pcwstr,
            hIconSm: LoadIconW(None, IDI_APPLICATION)?, // Small icon
        };

        if RegisterClassExW(&wc) == 0 {
            let error = GetLastError();
            Err(PlatformError::InitializationFailed(format!(
                "RegisterClassExW failed: {:?}",
                error
            )))
        } else {
            Ok(())
        }
    }
}

/// Creates a native Win32 window.
///
/// This function handles the `CreateWindowExW` call and sets up the
/// initial context for the window's `WndProc`.
pub(crate) fn create_native_window(
    internal_state_arc: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
    width: i32,
    height: i32,
) -> PlatformResult<HWND> {
    let class_name_hstring = HSTRING::from(format!(
        "{}_PlatformWindowClass",
        internal_state_arc.app_name_for_class
    ));

    // Prepare the context to be passed to CreateWindowExW's lpCreateParams.
    // This context will be retrieved in WM_NCCREATE.
    let creation_context = Box::new(WindowCreationContext {
        internal_state_arc: Arc::clone(internal_state_arc),
        window_id,
    });

    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),                           // dwExStyle
            &class_name_hstring,                                  // lpClassName
            &HSTRING::from(title),                                // lpWindowName
            WS_OVERLAPPEDWINDOW,                                  // dwStyle
            CW_USEDEFAULT,                                        // X
            CW_USEDEFAULT,                                        // Y
            width,                                                // nWidth
            height,                                               // nHeight
            None,                                                 // hWndParent
            None,                                                 // hMenu
            Some(internal_state_arc.h_instance),                  // hInstance
            Some(Box::into_raw(creation_context) as *mut c_void), // lpParam
        )?; // The '?' operator will convert windows::core::Error to PlatformError::Win32

        Ok(hwnd)
    }
}

/// The main window procedure (WndProc) router for all windows created by this platform layer.
///
/// This static function receives messages from the OS. It retrieves the
/// per-window `WindowCreationContext` (which contains an `Arc` to `Win32ApiInternalState`
/// and the `WindowId`) and then calls the instance method `handle_window_message`
/// on `Win32ApiInternalState` to process the message.
unsafe extern "system" fn facade_wnd_proc_router(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Retrieve the WindowCreationContext pointer stored in GWLP_USERDATA.
    // This pointer was set during WM_NCCREATE.
    let context_ptr = if msg == WM_NCCREATE {
        let create_struct = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let context_raw_ptr = create_struct.lpCreateParams as *mut WindowCreationContext;
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, context_raw_ptr as isize) };
        context_raw_ptr // Return for immediate use if needed
    } else {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowCreationContext }
    };

    if context_ptr.is_null() {
        // This can happen for messages processed before WM_NCCREATE or after WM_NCDESTROY clean-up.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    // We have a valid pointer to our context.
    let context = unsafe { &*context_ptr }; // Convert raw pointer to reference. Safe as long as context lives.
    let internal_state_arc = &context.internal_state_arc;
    let window_id = context.window_id;

    // Delegate to the instance method for actual message handling.
    let result = internal_state_arc.handle_window_message(hwnd, msg, wparam, lparam, window_id);

    // If WM_NCDESTROY, the context is about to be invalid, so we reclaim and drop the Box.
    if msg == WM_NCDESTROY {
        let _ = unsafe { Box::from_raw(context_ptr) }; // This drops the WindowCreationContext.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) }; // Clear the pointer.
    }

    result
}

// Helper functions to extract low/high order values from WPARAM and LPARAM
// Returns as i32 for convenience, but the value is effectively a u16.

#[inline]
pub(crate) fn loword_from_wparam(wparam: WPARAM) -> i32 {
    (wparam.0 & 0xFFFF) as i32
}

#[inline]
pub(crate) fn highord_from_wparam(wparam: WPARAM) -> i32 {
    // For WPARAM, HIWORD is the upper 16 bits of the full usize.
    // On 64-bit systems, wparam.0 is usize (u64).
    // If we only care about the traditional 32-bit meaning,
    // we might need to be careful, but for WM_COMMAND, hiword(wparam) is the notification code.
    // Let's assume standard interpretation where it fits in 16 bits.
    (wparam.0 >> 16) as i32 // This will take upper bits of the usize.
}

#[inline]
pub(crate) fn loword_from_lparam(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xFFFF) as i32
}

#[inline]
pub(crate) fn hiword_from_lparam(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xFFFF) as i32
}

// Instance method on Win32ApiInternalState to handle window messages.
// This is called by facade_wnd_proc_router.
impl Win32ApiInternalState {
    fn handle_window_message(
        self: &Arc<Self>, // Arc to Win32ApiInternalState
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> LRESULT {
        let event_handler_opt = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|weak_handler| weak_handler.upgrade());

        let mut app_event_to_send: Option<AppEvent> = None;

        match msg {
            WM_CREATE => {
                println!(
                    "Platform: WM_CREATE for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                unsafe {
                    match CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        WC_BUTTON, // Button class
                        &HSTRING::from("Generate Archive"),
                        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                        0,
                        0,
                        0,
                        0,          // Positioned and sized in WM_SIZE
                        Some(hwnd), // Parent
                        Some(HMENU(ID_BUTTON_GENERATE_ARCHIVE as *mut c_void)),
                        Some(self.h_instance),
                        None,
                    ) {
                        Ok(h_btn) => {
                            println!(
                                "Platform: Generate Archive button created successfully with HWND {:?}.",
                                h_btn
                            );
                            if let Some(mut windows_map_guard) = self.windows.write().ok() {
                                if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                                    window_data.hwnd_button_generate = Some(h_btn);
                                } else {
                                    eprintln!(
                                        "Platform: WM_CREATE - WindowId {:?} not found in map to store button HWND.",
                                        window_id
                                    );
                                }
                            } else {
                                eprintln!(
                                    "Platform: WM_CREATE - Failed to get write lock on windows map to store button HWND."
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Platform: Failed to create Generate Archive button. Error: {:?}",
                                e
                            );
                        }
                    }
                }
            }
            WM_SIZE => {
                let client_width = loword_from_lparam(lparam);
                let client_height = hiword_from_lparam(lparam);
                println!(
                    "Platform: WM_SIZE for WindowId {:?}, new client_width: {}, client_height: {}",
                    window_id, client_width, client_height
                );
                app_event_to_send = Some(AppEvent::WindowResized {
                    window_id,
                    width: client_width,
                    height: client_height,
                });

                if let Some(windows_guard) = self.windows.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        // Resize TreeView
                        if let Some(ref tv_state) = window_data.treeview_state {
                            if !tv_state.hwnd.is_invalid() {
                                let tv_height = client_height - BUTTON_AREA_HEIGHT;
                                println!(
                                    "Platform: WM_SIZE resizing TreeView HWND {:?} to W:{}, H:{}",
                                    tv_state.hwnd, client_width, tv_height
                                );
                                unsafe {
                                    let _ = MoveWindow(
                                        tv_state.hwnd,
                                        0,
                                        0,
                                        client_width,
                                        tv_height,
                                        true,
                                    )
                                    .map_err(|e| {
                                        eprintln!("MoveWindow for TreeView failed: {:?}", e)
                                    });
                                }
                            }
                        }
                        // Resize/Reposition Button
                        if let Some(hwnd_btn) = window_data.hwnd_button_generate {
                            if !hwnd_btn.is_invalid() {
                                let btn_x_pos = BUTTON_X_PADDING;
                                let btn_y_pos =
                                    client_height - BUTTON_AREA_HEIGHT + BUTTON_Y_PADDING_IN_AREA;
                                let btn_width = BUTTON_WIDTH;
                                let btn_height = BUTTON_HEIGHT;
                                println!(
                                    "Platform: WM_SIZE moving button HWND {:?} to X:{}, Y:{}, W:{}, H:{}",
                                    hwnd_btn, btn_x_pos, btn_y_pos, btn_width, btn_height
                                );
                                unsafe {
                                    let _ = MoveWindow(
                                        hwnd_btn, btn_x_pos, btn_y_pos, btn_width, btn_height, true,
                                    )
                                    .map_err(|e| {
                                        eprintln!("MoveWindow for Button failed: {:?}", e)
                                    });
                                }
                            } else {
                                println!(
                                    "Platform: WM_SIZE - button HWND is invalid for window_id {:?}.",
                                    window_id
                                );
                            }
                        } else {
                            println!(
                                "Platform: WM_SIZE - hwnd_button_generate is None for window_id {:?}.",
                                window_id
                            );
                        }
                    }
                } else {
                    eprintln!("Platform: WM_SIZE - Failed to get read lock on windows map.");
                }
            }
            WM_COMMAND => {
                let control_id = loword_from_wparam(wparam); // Gets the control ID from WPARAM
                let notification_code = highord_from_wparam(wparam); // Gets the notification code from WPARAM

                if notification_code as u32 == BN_CLICKED {
                    // Button click notification
                    if control_id == ID_BUTTON_GENERATE_ARCHIVE {
                        println!(
                            "Platform: Generate Archive button (ID {}) clicked.",
                            control_id
                        );
                        app_event_to_send = Some(AppEvent::ButtonClicked {
                            window_id,
                            control_id,
                        });
                    }
                }
            }

            WM_CLOSE => {
                println!(
                    "Platform: WM_CLOSE for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                let close_requested_event = AppEvent::WindowCloseRequested { window_id };
                let mut commands_from_app_logic = Vec::new();

                if let Some(handler) = event_handler_opt.clone() {
                    if let Ok(mut handler_guard) = handler.lock() {
                        commands_from_app_logic = handler_guard.handle_event(close_requested_event);
                    } else {
                        eprintln!("Platform: Failed to lock event handler during WM_CLOSE.");
                    }
                } else {
                    eprintln!("Platform: Event handler not available during WM_CLOSE.");
                }

                let mut app_logic_confirmed_close = false;
                for cmd in &commands_from_app_logic {
                    if let PlatformCommand::CloseWindow {
                        window_id: cmd_window_id,
                    } = cmd
                    {
                        if *cmd_window_id == window_id {
                            app_logic_confirmed_close = true;
                            break;
                        }
                    }
                }

                if app_logic_confirmed_close {
                    println!(
                        "Platform: AppLogic confirmed close for WindowId {:?}. Calling DestroyWindow.",
                        window_id
                    );
                    unsafe {
                        if DestroyWindow(hwnd).is_err() {
                            let err = GetLastError();
                            if err.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                                eprintln!(
                                    "DestroyWindow call from WM_CLOSE failed for {:?}. Error: {:?}",
                                    window_id, err
                                );
                            }
                        }
                    }
                } else {
                    println!(
                        "Platform: AppLogic did not command CloseWindow for WindowId {:?}. Window will not close now.",
                        window_id
                    );
                }
                return LRESULT(0);
            }

            WM_DESTROY => {
                println!(
                    "Platform: WM_DESTROY for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                app_event_to_send = Some(AppEvent::WindowDestroyed { window_id });
                if let Some(mut windows_map_guard) = self.windows.write().ok() {
                    windows_map_guard.remove(&window_id);
                }
                self.decrement_active_windows();
            }
            WM_NCDESTROY => {
                println!(
                    "Platform: WM_NCDESTROY for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            WM_PAINT => {
                unsafe {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    if !hdc.is_invalid() {
                        FillRect(
                            hdc,
                            &ps.rcPaint,
                            HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
                        );
                        EndPaint(hwnd, &ps);
                    }
                }
                return LRESULT(0);
            }
            WM_NOTIFY => {
                if let Some(event) =
                    control_treeview::handle_treeview_notification(self, window_id, lparam)
                {
                    app_event_to_send = Some(event);
                }
            }
            _ => {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
        }

        let mut commands_to_execute = Vec::new();
        if let Some(event) = app_event_to_send {
            if let Some(handler) = event_handler_opt {
                if let Ok(mut handler_guard) = handler.lock() {
                    commands_to_execute = handler_guard.handle_event(event);
                } else {
                    eprintln!("Platform: Failed to lock event handler.");
                }
            } else {
                eprintln!("Platform: Event handler is not available.");
            }
        }

        if !commands_to_execute.is_empty() {
            self.process_commands_from_event_handler(commands_to_execute);
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

// Public helper functions for PlatformInterface to call

pub(crate) fn set_window_title(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.windows.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            unsafe { SetWindowTextW(window_data.hwnd, &HSTRING::from(title))? };
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for SetWindowTitle",
                window_id
            )))
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock on windows map".into(),
        ))
    }
}

pub(crate) fn show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.windows.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            let cmd = if show { SW_SHOW } else { SW_HIDE };
            unsafe { ShowWindow(window_data.hwnd, cmd) };
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for ShowWindow",
                window_id
            )))
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock on windows map".into(),
        ))
    }
}

pub(crate) fn send_close_message(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.windows.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            unsafe { PostMessageW(Some(window_data.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))? };
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CloseWindow (send_close_message)",
                window_id
            )))
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock on windows map".into(),
        ))
    }
}

pub(crate) fn destroy_native_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    let hwnd_to_destroy: Option<HWND>;
    {
        let windows_read_guard = internal_state.windows.read().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to acquire read lock on windows map for destroy_native_window".into(),
            )
        })?;
        hwnd_to_destroy = windows_read_guard.get(&window_id).map(|data| data.hwnd);
    }

    if let Some(hwnd) = hwnd_to_destroy {
        if !hwnd.is_invalid() {
            println!(
                "Platform: Calling DestroyWindow for HWND {:?}, WindowId {:?}",
                hwnd, window_id
            );
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let err = GetLastError();
                    if err.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        eprintln!(
                            "DestroyWindow call failed for {:?}. Error: {:?}",
                            window_id, err
                        );
                    } else {
                        println!(
                            "DestroyWindow call for {:?} reported invalid handle, likely already destroyed.",
                            window_id
                        );
                    }
                }
            }
        } else {
            println!(
                "Platform: Attempted to destroy an invalid HWND for WindowId {:?}",
                window_id
            );
        }
        Ok(())
    } else {
        println!(
            "Platform: WindowId {:?} not found for destroy_native_window, likely already processed.",
            window_id
        );
        Ok(())
    }
}
