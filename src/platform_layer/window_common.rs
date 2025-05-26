use super::app::Win32ApiInternalState;
use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, MessageSeverity, PlatformCommand, TreeItemId, WindowId};

use windows::{
    Win32::{
        Foundation::{
            COLORREF, ERROR_INVALID_WINDOW_HANDLE, GetLastError, HWND, LPARAM, LRESULT, POINT,
            RECT, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, COLOR_WINDOW, COLOR_WINDOWTEXT, EndPaint, FillRect, GetSysColor,
            GetSysColorBrush, HBRUSH, HDC, InvalidateRect, PAINTSTRUCT, ScreenToClient, SetBkMode,
            SetTextColor, TRANSPARENT,
        },
        System::SystemServices::{IMAGE_DOS_HEADER, SS_LEFT},
        UI::Controls::*,
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, HSTRING, PCWSTR},
};

use std::ffi::c_void;
use std::sync::{Arc, Mutex}; // Mutex might not be needed here unless for specific shared state within this file

// Control IDs
pub(crate) const ID_BUTTON_GENERATE_ARCHIVE: i32 = 1002;
pub(crate) const ID_STATUS_BAR_CTRL: i32 = 1003;

pub(crate) const ID_MENU_FILE_LOAD_PROFILE: i32 = 2001;
pub(crate) const ID_MENU_FILE_SAVE_PROFILE_AS: i32 = 2002;
pub(crate) const ID_MENU_FILE_REFRESH: i32 = 2003;
pub(crate) const ID_MENU_FILE_SET_ARCHIVE: i32 = 2004;

pub(crate) const ID_DIALOG_INPUT_EDIT: i32 = 3001;
pub(crate) const ID_DIALOG_INPUT_PROMPT_STATIC: i32 = 3002;

const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");
const WC_STATIC: PCWSTR = windows::core::w!("STATIC");

pub(crate) const WM_APP_TREEVIEW_CHECKBOX_CLICKED: u32 = WM_APP + 0x100;

pub const BUTTON_AREA_HEIGHT: i32 = 50;
pub const STATUS_BAR_HEIGHT: i32 = 25;
const BUTTON_X_PADDING: i32 = 10;
const BUTTON_Y_PADDING_IN_AREA: i32 = 10;
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
    pub(crate) hwnd_status_bar: Option<HWND>,
    pub(crate) status_bar_current_text: String,
    pub(crate) status_bar_current_severity: MessageSeverity,
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
            // Class already registered
            println!(
                "Platform: Window class '{}' already registered.",
                internal_state.app_name_for_class
            );
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
            println!(
                "Platform: Window class '{}' registered successfully.",
                internal_state.app_name_for_class
            );
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
            None, // No parent for a top-level window
            None, // No menu for now (will be added in WM_CREATE)
            Some(internal_state_arc.h_instance),
            Some(Box::into_raw(creation_context) as *mut c_void), // Pass context
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
    let context_ptr = if msg == WM_NCCREATE {
        let create_struct = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let context_raw_ptr = create_struct.lpCreateParams as *mut WindowCreationContext;
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, context_raw_ptr as isize) };
        context_raw_ptr
    } else {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowCreationContext }
    };

    if context_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let context = unsafe { &*context_ptr };
    let internal_state_arc = &context.internal_state_arc;
    let window_id = context.window_id;

    // Delegate to the instance method on Win32ApiInternalState
    let result = internal_state_arc.handle_window_message(hwnd, msg, wparam, lparam, window_id);

    if msg == WM_NCDESTROY {
        let _ = unsafe { Box::from_raw(context_ptr) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }

    result
}

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

        let mut event_to_send: Option<AppEvent> = None;
        let mut lresult_override: Option<LRESULT> = None;

        match msg {
            WM_CREATE => {
                self.handle_wm_create(hwnd, wparam, lparam, window_id);
                // No AppEvent sent from WM_CREATE itself typically to MyAppLogic here,
                // but MyAppLogic::on_main_window_created is called from main.rs
            }
            WM_SIZE => {
                event_to_send = self.handle_wm_size(hwnd, wparam, lparam, window_id);
            }
            WM_COMMAND => {
                event_to_send = self.handle_wm_command(hwnd, wparam, lparam, window_id);
            }
            WM_CLOSE => {
                // WM_CLOSE is special: it first asks AppLogic if it's okay to close.
                // AppLogic responds by enqueuing PlatformCommand::CloseWindow if okay.
                event_to_send = Some(AppEvent::WindowCloseRequestedByUser { window_id });
                // We don't destroy the window here directly.
                // AppLogic handles the AppEvent::WindowCloseRequested.
                // If it decides to close, it enqueues PlatformCommand::CloseWindow.
                // The platform run loop will pick that up and execute it,
                // which then calls window_common::send_close_message -> DestroyWindow.
                // DefWindowProcW for WM_CLOSE calls DestroyWindow by default.
                // We want AppLogic to control this, so we handle the event,
                // and then if no specific command is issued by AppLogic to actually close,
                // we might just return 0 to indicate we handled it, or let DefWindowProc run.
                // For now, let's assume AppLogic will always enqueue a CloseWindow command if it wants to close.
                // If AppLogic doesn't enqueue CloseWindow, the window remains.
                // The original logic was to intercept and only destroy if app logic confirmed.
                // By sending the event, and AppLogic enqueuing CloseWindow, the run loop will handle it.
                // So, we just need to send the event. The LRESULT can be 0.
                lresult_override = Some(LRESULT(0)); // Indicate we've handled it; AppLogic decides actual close.
            }
            WM_DESTROY => {
                event_to_send = self.handle_wm_destroy(hwnd, wparam, lparam, window_id);
            }
            WM_NCDESTROY => {
                // This is the final stage of window destruction.
                // The GWLP_USERDATA is cleared by the facade_wnd_proc_router here.
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            WM_PAINT => {
                lresult_override = Some(self.handle_wm_paint(hwnd, wparam, lparam, window_id));
            }
            WM_NOTIFY => {
                event_to_send = self.handle_wm_notify(hwnd, wparam, lparam, window_id);
            }
            WM_APP_TREEVIEW_CHECKBOX_CLICKED => {
                event_to_send =
                    self.handle_wm_app_treeview_checkbox_clicked(hwnd, wparam, lparam, window_id);
            }
            WM_GETMINMAXINFO => {
                lresult_override =
                    Some(self.handle_wm_getminmaxinfo(hwnd, wparam, lparam, window_id));
            }
            WM_CTLCOLORSTATIC => {
                let hdc_static_ctrl = HDC(wparam.0 as *mut c_void);
                let hwnd_static_ctrl = HWND(lparam.0 as *mut c_void);

                if let Some(windows_guard) = self.window_map.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if Some(hwnd_static_ctrl) == window_data.hwnd_status_bar {
                            unsafe {
                                if window_data.status_bar_current_severity == MessageSeverity::Error
                                {
                                    SetTextColor(hdc_static_ctrl, COLORREF(0x000000FF));
                                } else {
                                    SetTextColor(
                                        hdc_static_ctrl,
                                        COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
                                    );
                                }
                                SetBkMode(hdc_static_ctrl, TRANSPARENT);
                                lresult_override =
                                    Some(LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize));
                            }
                        }
                    }
                }
                // If not handled by our status bar, let DefWindowProc handle it.
                if lresult_override.is_none() {
                    return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
                }
            }
            _ => {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
        }

        // If an AppEvent was generated, send it to MyAppLogic.
        // MyAppLogic will enqueue any resulting PlatformCommands.
        // The main run loop will pick those up.
        if let Some(event) = event_to_send {
            if let Some(handler_arc) = event_handler_opt {
                if let Ok(mut handler_guard) = handler_arc.lock() {
                    handler_guard.handle_event(event);
                } else {
                    eprintln!("Platform: Failed to lock event handler in handle_window_message.");
                }
            } else {
                eprintln!("Platform: Event handler not available in handle_window_message.");
            }
        }

        if let Some(lresult) = lresult_override {
            lresult
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    fn get_tree_item_toggle_event(
        self: &Arc<Self>,
        window_id: WindowId,
        h_item: HTREEITEM, // This h_item comes from the WPARAM of WM_APP_TREEVIEW_CHECKBOX_CLICKED
    ) -> Option<AppEvent> {
        eprintln!(
            "Platform (get_tree_item_toggle_event): Received h_item: {:?}",
            h_item
        ); // ADD THIS

        let windows_guard = self.window_map.read().ok()?;
        let window_data = windows_guard.get(&window_id)?;
        let tv_state = window_data.treeview_state.as_ref()?;

        eprintln!(
            "Platform (get_tree_item_toggle_event): TreeView HWND: {:?}",
            tv_state.hwnd
        ); // ADD THIS

        let mut tv_item_get = TVITEMEXW {
            mask: TVIF_STATE | TVIF_PARAM,
            hItem: h_item, // Use the h_item passed into this function
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
            // LRESULT is 0 on failure
            eprintln!(
                "Platform (get_tree_item_toggle_event): TVM_GETITEMW FAILED for h_item {:?}. Error: {:?}",
                h_item,
                unsafe { GetLastError() }
            );
            return None;
        }

        // ADD THESE LOGS:
        eprintln!(
            "Platform (get_tree_item_toggle_event): TVM_GETITEMW for h_item {:?} returned raw item state: {:#010X}, lParam: {}",
            h_item, tv_item_get.state, tv_item_get.lParam.0
        );

        let state_image_idx = (tv_item_get.state & TVIS_STATEIMAGEMASK.0) >> 12;
        eprintln!(
            "Platform (get_tree_item_toggle_event): Calculated state_image_idx: {}",
            state_image_idx
        );

        let new_check_state = if state_image_idx == 2 {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        };

        eprintln!(
            "Platform (get_tree_item_toggle_event): Determined new_check_state: {:?}",
            new_check_state
        );

        let app_item_id_val = tv_item_get.lParam.0 as u64;
        let app_item_id: TreeItemId;

        if app_item_id_val != 0 {
            app_item_id = TreeItemId(app_item_id_val);
            eprintln!(
                "Platform (get_tree_item_toggle_event): AppItemId from lParam: {:?}",
                app_item_id
            );
        } else {
            // Fallback to map if lParam was 0 (should ideally not happen with current add_item logic)
            if let Some(mapped_id) = tv_state.htreeitem_to_item_id.get(&(h_item.0)) {
                app_item_id = *mapped_id;
                eprintln!(
                    "Platform (get_tree_item_toggle_event): AppItemId from htreeitem_to_item_id map: {:?}",
                    app_item_id
                );
            } else {
                eprintln!(
                    "Platform (get_tree_item_toggle_event): AppItemId is 0 via lParam AND h_item {:?} not found in htreeitem_to_item_id map. Cannot create AppEvent.",
                    h_item
                );
                return None;
            }
        }

        Some(AppEvent::TreeViewItemToggledByUser {
            window_id,
            item_id: app_item_id,
            new_state: new_check_state,
        })
    }

    fn handle_wm_create(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) {
        println!(
            "Platform: WM_CREATE for HWND {:?}, WindowId {:?}",
            hwnd, window_id
        );
        unsafe {
            match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_BUTTON,
                &HSTRING::from("Save to archive"),
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
                    if let Some(mut windows_map_guard) = self.window_map.write().ok() {
                        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                            window_data.hwnd_button_generate = Some(h_btn);
                        }
                    }
                }
                Err(e) => eprintln!("Failed to create Generate Archive button: {:?}", e),
            }

            match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_STATIC,
                &HSTRING::from("Ready"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_LEFT.0), // Use SS_LEFT.0
                0,
                0,
                0,
                0,
                Some(hwnd),
                Some(HMENU(ID_STATUS_BAR_CTRL as *mut c_void)),
                Some(self.h_instance),
                None,
            ) {
                Ok(h_status) => {
                    if let Some(mut windows_map_guard) = self.window_map.write().ok() {
                        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                            window_data.hwnd_status_bar = Some(h_status);
                        }
                    }
                }
                Err(e) => eprintln!("Failed to create status bar: {:?}", e),
            }
        }
    }

    fn handle_wm_size(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let client_width = loword_from_lparam(lparam);
        let client_height = hiword_from_lparam(lparam);

        if let Some(windows_guard) = self.window_map.read().ok() {
            if let Some(window_data) = windows_guard.get(&window_id) {
                let treeview_height = client_height - BUTTON_AREA_HEIGHT - STATUS_BAR_HEIGHT;
                let button_area_y_pos = treeview_height;
                let status_bar_y_pos = treeview_height + BUTTON_AREA_HEIGHT;

                if let Some(ref tv_state) = window_data.treeview_state {
                    if !tv_state.hwnd.is_invalid() {
                        unsafe {
                            let _ = MoveWindow(
                                tv_state.hwnd,
                                0,
                                0,
                                client_width,
                                treeview_height,
                                true,
                            );
                        }
                    }
                }

                if let Some(hwnd_btn) = window_data.hwnd_button_generate {
                    if !hwnd_btn.is_invalid() {
                        let btn_x_pos = BUTTON_X_PADDING;
                        let btn_y_pos = button_area_y_pos + BUTTON_Y_PADDING_IN_AREA;
                        unsafe {
                            let _ = MoveWindow(
                                hwnd_btn,
                                btn_x_pos,
                                btn_y_pos,
                                BUTTON_WIDTH,
                                BUTTON_HEIGHT,
                                true,
                            );
                        }
                    }
                }

                if let Some(hwnd_status) = window_data.hwnd_status_bar {
                    if !hwnd_status.is_invalid() {
                        unsafe {
                            let _ = MoveWindow(
                                hwnd_status,
                                0,
                                status_bar_y_pos,
                                client_width,
                                STATUS_BAR_HEIGHT,
                                true,
                            );
                        }
                    }
                }
            }
        }
        Some(AppEvent::WindowResized {
            window_id,
            width: client_width,
            height: client_height,
        })
    }

    fn handle_wm_command(
        self: &Arc<Self>,
        _hwnd: HWND,
        wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let control_id = loword_from_wparam(wparam);
        let notification_code = highord_from_wparam(wparam);

        // Check for menu commands (notification_code is 0 for menu items from main menu, 1 for accelerator)
        if notification_code == 0 || notification_code == 1 {
            // Menu items or accelerators
            match control_id {
                ID_MENU_FILE_LOAD_PROFILE => {
                    println!("Platform: Menu 'Load Profile...' clicked.");
                    return Some(AppEvent::MenuLoadProfileClicked);
                }
                ID_MENU_FILE_SAVE_PROFILE_AS => {
                    println!("Platform: Menu 'Save Profile As...' clicked.");
                    return Some(AppEvent::MenuSaveProfileAsClicked);
                }
                ID_MENU_FILE_SET_ARCHIVE => {
                    // Handle new menu item
                    println!("Platform: Menu 'Set Archive Path...' clicked.");
                    return Some(AppEvent::MenuSetArchiveClicked);
                }
                ID_MENU_FILE_REFRESH => {
                    println!("Platform: Menu 'Refresh File List' clicked.");
                    return Some(AppEvent::MenuRefreshClicked);
                }
                _ => {} // Not a menu item we handle here
            }
        }

        if notification_code as u32 == BN_CLICKED {
            // BN_CLICKED is a WINDOW_STYLE
            if control_id == ID_BUTTON_GENERATE_ARCHIVE {
                return Some(AppEvent::ButtonClicked {
                    window_id,
                    control_id,
                });
            }
        }
        None
    }

    fn handle_wm_destroy(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        if let Some(mut windows_map_guard) = self.window_map.write().ok() {
            windows_map_guard.remove(&window_id);
        }
        self.decrement_active_windows();
        Some(AppEvent::WindowDestroyed { window_id })
    }

    fn handle_wm_paint(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        _window_id: WindowId,
    ) -> LRESULT {
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
        LRESULT(0)
    }

    fn handle_wm_notify(
        self: &Arc<Self>,
        hwnd: HWND, // HWND of the window that received WM_NOTIFY (parent of the control)
        _wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let nmhdr_ptr = lparam.0 as *const NMHDR;
        if nmhdr_ptr.is_null() {
            eprintln!("Platform: WM_NOTIFY - NMHDR pointer (lParam) is null.");
            return None;
        }
        let nmhdr = unsafe { &*nmhdr_ptr };

        // Ensure the notification is from our TreeView control
        if nmhdr.idFrom as i32 != control_treeview::ID_TREEVIEW_CTRL {
            return None; // Notification not from TreeView, ignore here
        }

        match nmhdr.code {
            NM_CLICK => {
                println!("\nPlatform: NM_CLICK received for TreeView control.");
                let hwnd_tv_from_notify = nmhdr.hwndFrom; // This is the HWND of the TreeView control itself

                if hwnd_tv_from_notify.is_invalid() {
                    eprintln!("Platform: NM_CLICK from invalid HWND in NMHDR (TreeView).");
                    return None;
                }

                // --- Get Cursor Screen Position using GetCursorPos ---
                let mut screen_pt_of_click = POINT::default();
                unsafe {
                    if GetCursorPos(&mut screen_pt_of_click).is_err() {
                        eprintln!(
                            "Platform: GetCursorPos FAILED for NM_CLICK. Error: {:?}",
                            GetLastError()
                        );
                        return None;
                    }
                }
                println!(
                    "Platform: GetCursorPos reported screen coords: (x:{}, y:{})",
                    screen_pt_of_click.x, screen_pt_of_click.y
                );

                // --- ScreenToClient Transformation ---
                let mut client_pt_for_hittest = screen_pt_of_click; // Copy for transformation
                println!(
                    "Platform: Values BEFORE ScreenToClient: treeview_hwnd={:?}, point_to_convert.x={}, point_to_convert.y={}",
                    hwnd_tv_from_notify, client_pt_for_hittest.x, client_pt_for_hittest.y
                );

                let s2c_success = unsafe {
                    ScreenToClient(hwnd_tv_from_notify, &mut client_pt_for_hittest).as_bool()
                };

                if !s2c_success {
                    eprintln!(
                        "Platform: ScreenToClient FAILED for TreeView HWND {:?}. Error: {:?}",
                        hwnd_tv_from_notify,
                        unsafe { GetLastError() }
                    );
                    return None;
                }
                println!(
                    "Platform: Values AFTER ScreenToClient (coords for HitTest): (x:{}, y:{})",
                    client_pt_for_hittest.x, client_pt_for_hittest.y
                );

                // Proceed with hit-testing using client_pt_for_hittest
                if let Some(windows_guard) = self.window_map.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if let Some(ref tv_state) = window_data.treeview_state {
                            if hwnd_tv_from_notify != tv_state.hwnd {
                                eprintln!(
                                    "Platform: NM_CLICK (TreeView) hwndFrom ({:?}) mismatch with cached tv_state.hwnd ({:?}). Aborting.",
                                    hwnd_tv_from_notify, tv_state.hwnd
                                );
                                return None;
                            }

                            let mut tvht_info = TVHITTESTINFO {
                                pt: client_pt_for_hittest,
                                flags: TVHITTESTINFO_FLAGS(0),
                                hItem: HTREEITEM(0),
                            };
                            println!(
                                "Platform: Hit testing TreeView {:?} at derived client coords (x:{}, y:{})",
                                hwnd_tv_from_notify,
                                client_pt_for_hittest.x,
                                client_pt_for_hittest.y
                            );
                            let h_item_hit = HTREEITEM(
                                unsafe {
                                    SendMessageW(
                                        hwnd_tv_from_notify,
                                        TVM_HITTEST,
                                        Some(WPARAM(0)),
                                        Some(LPARAM(&mut tvht_info as *mut _ as isize)),
                                    )
                                }
                                .0,
                            );
                            println!(
                                "Platform: TVM_HITTEST on TreeView {:?} returned hItem: {:?}, flags: {:#X}",
                                hwnd_tv_from_notify, h_item_hit, tvht_info.flags.0
                            );

                            if h_item_hit.0 != 0
                                && (tvht_info.flags.0 & TVHT_ONITEMSTATEICON.0) != 0
                            {
                                println!(
                                    "Platform: Click on STATE ICON of hItem {:?}. Posting WM_APP_TREEVIEW_CHECKBOX_CLICKED.",
                                    h_item_hit
                                );
                                unsafe {
                                    if PostMessageW(
                                        Some(hwnd), // Post to the main window
                                        WM_APP_TREEVIEW_CHECKBOX_CLICKED,
                                        WPARAM(h_item_hit.0 as usize),
                                        LPARAM(0),
                                    )
                                    .is_err()
                                    {
                                        eprintln!("Platform: Failed to post checkbox‐clicked.");
                                    }
                                }
                            } else {
                                // Detailed logging for non-state-icon clicks
                                if h_item_hit.0 != 0 {
                                    let mut flags_str = String::new();
                                    if (tvht_info.flags.0 & TVHT_ONITEMLABEL.0) != 0 {
                                        flags_str += "ONITEMLABEL ";
                                    }
                                    if (tvht_info.flags.0 & TVHT_ONITEMICON.0) != 0 {
                                        flags_str += "ONITEMICON ";
                                    }
                                    // Add other TVHT_ flags as needed for debugging
                                    println!(
                                        "Platform: Click was NOT on state icon but on item {:?} (flags: {:#X} -> {}).",
                                        h_item_hit,
                                        tvht_info.flags.0,
                                        flags_str.trim()
                                    );
                                } else {
                                    println!(
                                        "Platform: Click was NOT on state icon and NOT on any item (hItem: {:?}, flags: {:#X}).",
                                        h_item_hit, tvht_info.flags.0
                                    );
                                }
                            }
                        } else {
                            eprintln!(
                                "Platform: NM_CLICK - No TreeViewInternalState for window_id {:?}",
                                window_id
                            );
                        }
                    } else {
                        eprintln!(
                            "Platform: NM_CLICK - No NativeWindowData for window_id {:?}",
                            window_id
                        );
                    }
                } else {
                    eprintln!("Platform: NM_CLICK - Could not get read lock on window_map.");
                }
                return None; // NM_CLICK processing ends here for the TreeView
            }
            TVN_ITEMCHANGEDW => {
                // Handle TVN_ITEMCHANGEDW if needed (e.g., for selection changes not related to checkboxes)
                // For now, it just calls your existing handler.
                print!(
                    "Platform: TVN_ITEMCHANGEDW window id {:?} lparam: {:?}\n",
                    window_id, lparam
                );
                return control_treeview::handle_treeview_itemchanged_notification(
                    self, window_id, lparam,
                );
            }
            _ => { /* Other notification codes for the TreeView */ }
        }
        None
    }

    fn handle_wm_app_treeview_checkbox_clicked(
        self: &Arc<Self>,
        _hwnd: HWND,
        wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let h_item_val = wparam.0 as isize;
        if h_item_val == 0 {
            return None;
        }
        let h_item_from_message = HTREEITEM(h_item_val);
        self.get_tree_item_toggle_event(window_id, h_item_from_message)
    }

    fn handle_wm_getminmaxinfo(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        lparam: LPARAM,
        _window_id: WindowId,
    ) -> LRESULT {
        if lparam.0 != 0 {
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            mmi.ptMinTrackSize.x = 300;
            mmi.ptMinTrackSize.y = 200;
        }
        LRESULT(0)
    }
}

pub(crate) fn set_window_title(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.window_map.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            unsafe { SetWindowTextW(window_data.hwnd, &HSTRING::from(title))? };
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found",
                window_id
            )))
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock".into(),
        ))
    }
}

pub(crate) fn show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    if let Some(windows_guard) = internal_state.window_map.read().ok() {
        if let Some(window_data) = windows_guard.get(&window_id) {
            let cmd = if show { SW_SHOW } else { SW_HIDE };
            unsafe { ShowWindow(window_data.hwnd, cmd) };
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found",
                window_id
            )))
        }
    } else {
        Err(PlatformError::OperationFailed(
            "Failed to acquire read lock".into(),
        ))
    }
}

pub(crate) fn send_close_message(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    println!(
        "Platform: send_close_message received for WindowId {:?}, proceeding to destroy.",
        window_id
    );
    destroy_native_window(internal_state, window_id)
}

pub(crate) fn destroy_native_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    let hwnd_to_destroy: Option<HWND>;
    {
        let windows_read_guard = internal_state
            .window_map
            .read()
            .map_err(|_| PlatformError::OperationFailed("Failed to acquire read lock".into()))?;
        hwnd_to_destroy = windows_read_guard.get(&window_id).map(|data| data.hwnd);
    }

    if let Some(hwnd) = hwnd_to_destroy {
        if !hwnd.is_invalid() {
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let err = GetLastError();
                    if err.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        // Only log if not already invalid
                        eprintln!("DestroyWindow failed: {:?}", err);
                    }
                }
            }
        }
    }
    // Note: Removing from window_map and decrementing active_windows_count is now handled
    // by the WM_DESTROY handler in Win32ApiInternalState.
    Ok(())
}

/// Updates the text of the status bar control. The actual color change is triggered
/// by invalidating the control, which then leads to WM_CTLCOLORSTATIC being handled
/// in app.rs using the `status_bar_current_severity` stored in `NativeWindowData`.
/// This function is now the direct implementation called by _execute_platform_command.
pub(crate) fn update_status_bar_text_impl(
    window_data: &mut NativeWindowData, // Takes NativeWindowData directly
    text: &str,
    severity: MessageSeverity, // Severity is used by caller to update window_data.status_bar_current_severity
) -> PlatformResult<()> {
    // The caller (_execute_platform_command in app.rs) is responsible for:
    // 1. Comparing severities.
    // 2. Updating window_data.status_bar_current_text and window_data.status_bar_current_severity.
    // This function just performs the WinAPI calls.

    if let Some(hwnd_status) = window_data.hwnd_status_bar {
        // The `window_data.status_bar_current_severity` would have been updated by the caller
        // before this function is called if the severity condition was met.
        // This function now just sets the text and invalidates.
        unsafe {
            if SetWindowTextW(hwnd_status, &HSTRING::from(text)).is_err() {
                return Err(PlatformError::OperationFailed(format!(
                    "SetWindowTextW for status bar failed: {:?}",
                    GetLastError()
                )));
            }
            // Invalidate the status bar control to force a repaint.
            // WM_CTLCOLORSTATIC will then use the (already updated) severity for color.
            InvalidateRect(Some(hwnd_status), None, true); // true to erase background
        }
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "Status bar HWND not found for WindowId {:?}",
            window_data.id // Use id from window_data
        )))
    }
}
