use super::app::Win32ApiInternalState; // The shared internal state
use super::control_treeview; // For TreeView specific data and event handling
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, PlatformCommand, TreeItemId, WindowId};

use windows::{
    Win32::{
        Foundation::{
            ERROR_INVALID_WINDOW_HANDLE, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::Gdi::{BeginPaint, COLOR_WINDOW, EndPaint, FillRect, HBRUSH, PAINTSTRUCT}, // For basic painting
        System::SystemServices::IMAGE_DOS_HEADER, // Used to get base address for HINSTANCE
        UI::Controls::{
            HTREEITEM, NM_CLICK, NMHDR, NMMOUSE, TVHITTESTINFO, TVHITTESTINFO_FLAGS,
            TVHT_ONITEMSTATEICON, TVIF_PARAM, TVIF_STATE, TVIS_STATEIMAGEMASK, TVITEMEXW,
            TVM_GETITEMW, TVM_HITTEST, TVN_ITEMCHANGEDW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR},
};

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

// Control IDs
pub(crate) const ID_BUTTON_GENERATE_ARCHIVE: i32 = 1002;
const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON"); // Helper for button class name

// Custom message for TreeView checkbox clicks
pub(crate) const WM_APP_TREEVIEW_CHECKBOX_CLICKED: u32 = WM_APP + 0x100;

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
    pub(crate) id: WindowId,
    pub(crate) treeview_state: Option<control_treeview::TreeViewInternalState>,
    pub(crate) hwnd_button_generate: Option<HWND>,
}

/// Context passed to `CreateWindowExW` via `lpCreateParams`.
/// This allows the static `WndProc` to retrieve the necessary `Arc`-ed state
/// for the specific window instance being created.
struct WindowCreationContext {
    internal_state_arc: Arc<Win32ApiInternalState>,
    window_id: WindowId,
}

pub(crate) fn register_window_class(
    internal_state: &Arc<Win32ApiInternalState>,
) -> PlatformResult<()> {
    let class_name_hstring = HSTRING::from(format!(
        "{}_PlatformWindowClass",
        internal_state.app_name_for_class
    ));
    let class_name_pcwstr = PCWSTR(class_name_hstring.as_ptr());

    unsafe {
        let mut wc_test = WNDCLASSEXW::default();
        if GetClassInfoExW(
            Some(internal_state.h_instance),
            class_name_pcwstr,
            &mut wc_test,
        )
        .is_ok()
        {
            return Ok(());
        }

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(facade_wnd_proc_router),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: internal_state.h_instance,
            hIcon: LoadIconW(None, IDI_APPLICATION)?,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name_pcwstr,
            hIconSm: LoadIconW(None, IDI_APPLICATION)?,
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
            WINDOW_EX_STYLE::default(),
            &class_name_hstring,
            &HSTRING::from(title),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            None,
            None,
            Some(internal_state_arc.h_instance),
            Some(Box::into_raw(creation_context) as *mut c_void),
        )?;

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
        context_raw_ptr
    } else {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowCreationContext }
    };

    if context_ptr.is_null() {
        // This can happen for messages processed before WM_NCCREATE or after WM_NCDESTROY clean-up.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let context = unsafe { &*context_ptr };
    let internal_state_arc = &context.internal_state_arc;
    let window_id = context.window_id;

    // Delegate to the instance method for actual message handling.
    let result = internal_state_arc.handle_window_message(hwnd, msg, wparam, lparam, window_id);

    // If WM_NCDESTROY, the context is about to be invalid, so we reclaim and drop the Box.
    if msg == WM_NCDESTROY {
        let _ = unsafe { Box::from_raw(context_ptr) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
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
    (wparam.0 >> 16) as i32
}

#[inline]
pub(crate) fn loword_from_lparam(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xFFFF) as i32
}

#[inline]
pub(crate) fn hiword_from_lparam(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xFFFF) as i32
}

impl Win32ApiInternalState {
    fn handle_window_message(
        self: &Arc<Self>,
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
                        WC_BUTTON,
                        &HSTRING::from("Generate Archive"),
                        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                        0,
                        0,
                        0,
                        0,
                        Some(hwnd),
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
                app_event_to_send = Some(AppEvent::WindowResized {
                    window_id,
                    width: client_width,
                    height: client_height,
                });

                if let Some(windows_guard) = self.windows.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if let Some(ref tv_state) = window_data.treeview_state {
                            if !tv_state.hwnd.is_invalid() {
                                let tv_height = client_height - BUTTON_AREA_HEIGHT;
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
                                unsafe {
                                    let _ = MoveWindow(
                                        hwnd_btn, btn_x_pos, btn_y_pos, btn_width, btn_height, true,
                                    )
                                    .map_err(|e| {
                                        eprintln!("MoveWindow for Button failed: {:?}", e)
                                    });
                                }
                            }
                        }
                    }
                }
            }
            WM_COMMAND => {
                let control_id = loword_from_wparam(wparam);
                let notification_code = highord_from_wparam(wparam);
                if notification_code as u32 == BN_CLICKED
                    && control_id == ID_BUTTON_GENERATE_ARCHIVE
                {
                    app_event_to_send = Some(AppEvent::ButtonClicked {
                        window_id,
                        control_id,
                    });
                }
            }
            WM_CLOSE => {
                let close_requested_event = AppEvent::WindowCloseRequested { window_id };
                let mut commands_from_app_logic = Vec::new();
                if let Some(handler) = event_handler_opt.clone() {
                    if let Ok(mut handler_guard) = handler.lock() {
                        commands_from_app_logic = handler_guard.handle_event(close_requested_event);
                    }
                }
                let app_logic_confirmed_close = commands_from_app_logic.iter().any(|cmd| {
                    matches!(cmd, PlatformCommand::CloseWindow { window_id: cmd_window_id } if *cmd_window_id == window_id)
                });
                if app_logic_confirmed_close {
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
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                app_event_to_send = Some(AppEvent::WindowDestroyed { window_id });
                if let Some(mut windows_map_guard) = self.windows.write().ok() {
                    windows_map_guard.remove(&window_id);
                }
                self.decrement_active_windows();
            }
            WM_NCDESTROY => {
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
                let nmhdr_ptr = lparam.0 as *const NMHDR; // Pointer to NMHDR
                if nmhdr_ptr.is_null() {
                    return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
                }
                let nmhdr = unsafe { &*nmhdr_ptr };

                if nmhdr.idFrom as i32 == control_treeview::ID_TREEVIEW_CTRL {
                    match nmhdr.code {
                        NM_CLICK => {
                            // For NM_CLICK on TreeView, lParam is a pointer to NMMOUSE
                            let nmmouse_ptr = lparam.0 as *const NMMOUSE;
                            if nmmouse_ptr.is_null() {
                                return LRESULT(0); // Should not happen if nmhdr_ptr was valid
                            }
                            let nmmouse = unsafe { &*nmmouse_ptr };

                            if let Some(windows_guard_for_click) = self.windows.read().ok() {
                                if let Some(window_data_for_click) =
                                    windows_guard_for_click.get(&window_id)
                                {
                                    if let Some(ref tv_state_for_click) =
                                        window_data_for_click.treeview_state
                                    {
                                        // Use nmmouse.pt for coordinates relative to the client area of the control
                                        let mut tv_click_point = nmmouse.pt;

                                        // TVM_HITTEST expects coordinates relative to the TreeView's client area.
                                        // NMMOUSE.pt are already in client coordinates of the control.
                                        // If they were screen coordinates, you'd use ScreenToClient.

                                        let mut tvht_info = TVHITTESTINFO {
                                            pt: tv_click_point,
                                            flags: TVHITTESTINFO_FLAGS(0), // Initialize flags
                                            hItem: HTREEITEM(0),
                                        };

                                        let h_item_hit = HTREEITEM(
                                            unsafe {
                                                SendMessageW(
                                                    tv_state_for_click.hwnd,
                                                    TVM_HITTEST,
                                                    Some(WPARAM(0)),
                                                    Some(LPARAM(&mut tvht_info as *mut _ as isize)),
                                                )
                                            }
                                            .0,
                                        );

                                        // Check if h_item_hit is valid and if the click was on the state icon
                                        if h_item_hit.0 != 0
                                            && (tvht_info.flags.0 & TVHT_ONITEMSTATEICON.0) != 0
                                        {
                                            println!(
                                                "Platform: NM_CLICK on TreeView checkbox for hItem {:?}. Posting custom message.",
                                                h_item_hit
                                            );
                                            unsafe {
                                                if PostMessageW(
                                                    Some(hwnd),
                                                    WM_APP_TREEVIEW_CHECKBOX_CLICKED,
                                                    WPARAM(h_item_hit.0 as usize),
                                                    LPARAM(0),
                                                )
                                                .is_err()
                                                {
                                                    eprintln!(
                                                        "Platform: Failed to post WM_APP_TREEVIEW_CHECKBOX_CLICKED. Error: {:?}",
                                                        GetLastError()
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        TVN_ITEMCHANGEDW => {
                            if let Some(event) =
                                control_treeview::handle_treeview_itemchanged_notification(
                                    self, window_id, lparam,
                                )
                            {
                                app_event_to_send = Some(event);
                            }
                        }
                        _ => {}
                    }
                }
            }
            WM_APP_TREEVIEW_CHECKBOX_CLICKED => {
                println!("Platform: Received WM_APP_TREEVIEW_CHECKBOX_CLICKED");
                let h_item_val = wparam.0 as isize;
                if h_item_val == 0 {
                    return LRESULT(0);
                }
                let h_item_from_message = HTREEITEM(h_item_val);
                if let Some(event) = self.get_tree_item_toggle_event(window_id, h_item_from_message)
                {
                    app_event_to_send = Some(event);
                }
            }
            // Add WM_GETMINMAXINFO handler to prevent window from becoming too small
            WM_GETMINMAXINFO => {
                let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                mmi.ptMinTrackSize.x = 300;
                mmi.ptMinTrackSize.y = 200;
                return LRESULT(0);
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
                }
            }
        }

        if !commands_to_execute.is_empty() {
            self.process_commands_from_event_handler(commands_to_execute);
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn get_tree_item_toggle_event(
        self: &Arc<Self>,
        window_id: WindowId,
        h_item: HTREEITEM,
    ) -> Option<AppEvent> {
        let windows_guard = self.windows.read().ok()?;
        let window_data = windows_guard.get(&window_id)?;
        let tv_state = window_data.treeview_state.as_ref()?;

        let mut tv_item_get = TVITEMEXW {
            mask: TVIF_STATE | TVIF_PARAM,
            hItem: h_item,
            stateMask: TVIS_STATEIMAGEMASK.0,
            lParam: LPARAM(0),
            ..Default::default()
        };

        let get_item_result = unsafe {
            SendMessageW(
                tv_state.hwnd,
                TVM_GETITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tv_item_get as *mut _ as isize)),
            )
        };

        if get_item_result.0 == 0 {
            eprintln!(
                "Platform: TVM_GETITEMW failed for hItem {:?}. Error: {:?}",
                h_item,
                unsafe { GetLastError() }
            );
            return None;
        }

        let state_image_idx = (tv_item_get.state & TVIS_STATEIMAGEMASK.0) >> 12;
        let new_check_state = if state_image_idx == 2 {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        };

        let app_item_id_val = tv_item_get.lParam.0 as u64;
        if app_item_id_val == 0 && !tv_state.htreeitem_to_item_id.contains_key(&(h_item.0)) {
            eprintln!(
                "Platform: Could not resolve app_item_id for hItem {:?} (lParam was 0, and map lookup failed).",
                h_item
            );
            return None;
        }
        let app_item_id = TreeItemId(app_item_id_val);

        Some(AppEvent::TreeViewItemToggled {
            window_id,
            item_id: app_item_id,
            new_state: new_check_state,
        })
    }
}

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
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let err = GetLastError();
                    if err.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        eprintln!(
                            "DestroyWindow call failed for {:?}. Error: {:?}",
                            window_id, err
                        );
                    }
                }
            }
        }
        Ok(())
    } else {
        Ok(()) // Already destroyed or never existed, not an error for this operation
    }
}
