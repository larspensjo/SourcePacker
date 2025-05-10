use super::app::Win32ApiInternalState; // The shared internal state
use super::control_treeview; // For TreeView specific data and event handling
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, PlatformCommand, WindowId}; // For event generation

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BeginPaint, COLOR_WINDOW, EndPaint, FillRect, HBRUSH, PAINTSTRUCT}, // For basic painting
        System::SystemServices::IMAGE_DOS_HEADER, // Used to get base address for HINSTANCE
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR},
};

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

/// Holds native data associated with a specific window managed by the platform layer.
/// This includes the native window handle (`HWND`) and any control-specific states.
#[derive(Debug)]
pub(crate) struct NativeWindowData {
    pub(crate) hwnd: HWND,
    pub(crate) id: WindowId, // The platform-agnostic ID for this window
    /// State specific to a TreeView control, if one exists in this window.
    pub(crate) treeview_state: Option<control_treeview::TreeViewInternalState>,
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

/// Extracts the low-order 16-bit value from an LPARAM.
/// Returns as i32 for convenience, but the value is effectively a u16.
#[inline]
pub(crate) fn loword_from_lparam(lparam: LPARAM) -> i32 {
    // Since lparam.0 is isize, this correctly handles both 32-bit and 64-bit lparam.
    // The result of (lparam.0 & 0xFFFF) will be a positive isize value
    // representing the unsigned 16-bit quantity.
    // Casting this to i32 is fine for dimensions.
    (lparam.0 & 0xFFFF) as i32
}

/// Extracts the high-order 16-bit value (from the lower 32 bits) of an LPARAM.
/// Returns as i32 for convenience, but the value is effectively a u16.
#[inline]
pub(crate) fn hiword_from_lparam(lparam: LPARAM) -> i32 {
    // Shift right by 16 to move the high word into the low word position.
    // Then mask with 0xFFFF to isolate it.
    // The result of the bitwise operations on isize will be an isize.
    // Casting this to i32 is fine for dimensions.
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
        // Attempt to upgrade the weak event_handler reference.
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
                // TreeView creation will happen here if a window is configured to have one.
                // This can be triggered by an initial PlatformCommand or a default setup.
                // For now, we assume PopulateTreeView command will create it if not exists.
            }
            WM_SIZE => {
                let width = loword_from_lparam(lparam);
                let height = hiword_from_lparam(lparam);
                app_event_to_send = Some(AppEvent::WindowResized {
                    window_id,
                    width,
                    height,
                });

                // If there's a TreeView, resize it.
                if let Some(windows_guard) = self.windows.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if let Some(ref tv_state) = window_data.treeview_state {
                            if tv_state.hwnd.is_invalid() {
                                // Check if HWND is valid
                                unsafe {
                                    let _ = SetWindowPos(
                                        // Assign to _ to ignore the Result<BOOL>
                                        tv_state.hwnd,
                                        None,
                                        0,
                                        0,
                                        width,
                                        height,
                                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                                    )
                                    .map_err(|e| {
                                        eprintln!("SetWindowPos for TreeView failed: {:?}", e)
                                    });
                                }
                            }
                        }
                    }
                }
            }
            WM_CLOSE => {
                println!(
                    "Platform: WM_CLOSE for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                app_event_to_send = Some(AppEvent::WindowCloseRequested { window_id });
                // Default behavior: if no event handler or it doesn't stop it, we destroy.
                // Here, we just send the event. AppLogic must send CloseWindow command
                // which should then call DestroyWindow.
                // If app_logic doesn't handle it/send CloseWindow, nothing happens yet.
                // A more robust system: if app_event_to_send is handled and NO CloseWindow command
                // is returned, then we DON'T call DestroyWindow.
                // For now, we assume AppLogic will send CloseWindow if it wants to proceed.
                // Let's change this: if WindowCloseRequested isn't "vetoed" by app logic returning
                // a specific command to *not* close, then proceed to destroy.
                // For now, the platform will *not* destroy on WM_CLOSE by default. AppLogic must command it.
            }
            WM_DESTROY => {
                println!(
                    "Platform: WM_DESTROY for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                app_event_to_send = Some(AppEvent::WindowDestroyed { window_id });

                // Clean up this window's entry from the internal state.
                if let Some(mut windows_map_guard) = self.windows.write().ok() {
                    windows_map_guard.remove(&window_id);
                }
                // Decrement active window count. This might trigger PostQuitMessage.
                self.decrement_active_windows();
            }
            WM_NCDESTROY => {
                // The GWLP_USERDATA cleanup is handled in facade_wnd_proc_router.
                println!(
                    "Platform: WM_NCDESTROY for HWND {:?}, WindowId {:?}",
                    hwnd, window_id
                );
                // This is the final message a window receives.
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }; // Return here as context is gone.
            }
            WM_PAINT => {
                // Basic paint handling, can be removed if all content is child controls.
                unsafe {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    if hdc.is_invalid() {
                        // Example: Fill with window color
                        FillRect(
                            hdc,
                            &ps.rcPaint,
                            HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
                        );
                        EndPaint(hwnd, &ps);
                    }
                }
                return LRESULT(0); // Indicate we handled WM_PAINT
            }
            WM_NOTIFY => {
                // Handle notifications, e.g., from TreeView
                // This needs to be routed to control_treeview if it's a treeview notification
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

        // If an application event was generated, send it to the event handler.
        let mut commands_to_execute = Vec::new();
        if let Some(event) = app_event_to_send {
            if let Some(handler) = event_handler_opt {
                // Check if handler is still valid
                if let Ok(mut handler_guard) = handler.lock() {
                    commands_to_execute = handler_guard.handle_event(event);
                } else {
                    eprintln!("Platform: Failed to lock event handler.");
                }
            } else {
                eprintln!("Platform: Event handler is not available (already dropped or not set).");
            }
        }

        // Execute any commands returned by the event handler using the new internal method.
        if !commands_to_execute.is_empty() {
            // `self` here is already the `Arc<Win32ApiInternalState>`
            self.process_commands_from_event_handler(commands_to_execute);
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

// --- Public helper functions for PlatformInterface to call ---

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
            unsafe { ShowWindow(window_data.hwnd, cmd) }; // ShowWindow returns BOOL (i32)
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
            // This sends WM_CLOSE. The WndProc will then generate AppEvent::WindowCloseRequested.
            // If AppLogic wants to proceed, it will then send PlatformCommand::CloseWindow (or similar).
            // The actual DestroyWindow should happen in response to THAT command.
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

/// Destroys a native window. This should be called in response to AppLogic confirming a close.
pub(crate) fn destroy_native_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.windows.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            unsafe {
                if DestroyWindow(window_data.hwnd).is_err() {
                    // DestroyWindow itself might fail if HWND is already invalid,
                    // but WM_DESTROY and WM_NCDESTROY should still be processed by the system.
                    eprintln!(
                        "DestroyWindow call failed for {:?}, HWND possibly already invalid. Error: {:?}",
                        window_id,
                        GetLastError()
                    );
                }
            }
            // The actual removal from map and decrementing count happens in WM_DESTROY.
            Ok(())
        } else {
            // Window might have already been destroyed and removed.
            println!(
                "Platform: WindowId {:?} not found for destroy_native_window, likely already processed.",
                window_id
            );
            Ok(())
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock on windows map for destroy".into(),
        ))
    }
}
