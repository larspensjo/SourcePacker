/*
 * This module provides common Win32 windowing functionalities, including
 * window class registration, native window creation, and the main window
 * procedure (WndProc) for message handling. It defines `NativeWindowData`
 * to store per-window native state and helper functions for interacting
 * with the Win32 API.
 */
use super::app::Win32ApiInternalState;
use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{
    AppEvent, CheckState, MenuAction, MessageSeverity, PlatformCommand, TreeItemId, WindowId,
};

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
        UI::Controls::*,
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, HSTRING, PCWSTR},
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

// Control IDs
pub(crate) const ID_BUTTON_GENERATE_ARCHIVE: i32 = 1002;
pub(crate) const ID_STATUS_BAR_CTRL: i32 = 1003;

pub(crate) const ID_DIALOG_INPUT_EDIT: i32 = 3001;
pub(crate) const ID_DIALOG_INPUT_PROMPT_STATIC: i32 = 3002;

pub(crate) const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");
pub(crate) const WC_STATIC: PCWSTR = windows::core::w!("STATIC");
pub(crate) const SS_LEFT: WINDOW_STYLE = WINDOW_STYLE(0x00000000_u32);

pub(crate) const WM_APP_TREEVIEW_CHECKBOX_CLICKED: u32 = WM_APP + 0x100;

pub const BUTTON_AREA_HEIGHT: i32 = 50;
pub const STATUS_BAR_HEIGHT: i32 = 25;
const BUTTON_X_PADDING: i32 = 10;
const BUTTON_Y_PADDING_IN_AREA: i32 = 10;
const BUTTON_WIDTH: i32 = 150;
const BUTTON_HEIGHT: i32 = 30;

/*
 * Holds native data associated with a specific window managed by the platform layer.
 * This includes the native window handle (`HWND`), a map of control IDs to their
 * `HWND`s, any control-specific states (like for the TreeView), the current status
 * bar text and severity, a map for menu item actions (`menu_action_map`), and
 * a counter for generating unique menu item IDs (`next_menu_item_id_counter`).
 */
#[derive(Debug)]
pub(crate) struct NativeWindowData {
    pub(crate) hwnd: HWND,
    pub(crate) id: WindowId,
    /*
     * Stores the specific internal state for the TreeView control if one exists.
     * This is initialized by the `CreateTreeView` command handler.
     */
    pub(crate) treeview_state: Option<control_treeview::TreeViewInternalState>,
    /*
     * Stores HWNDs for various controls (buttons, status bar, treeview, etc.)
     * keyed by their logical control ID.
     */
    pub(crate) controls: HashMap<i32, HWND>,
    pub(crate) status_bar_current_text: String,
    pub(crate) status_bar_current_severity: MessageSeverity,
    /*
     * Maps dynamically generated `i32` menu item IDs to their semantic `MenuAction`.
     * This map is populated when the menu is created.
     */
    pub(crate) menu_action_map: HashMap<i32, MenuAction>,
    /*
     * Counter to generate unique `i32` IDs for menu items that have an action.
     * Initialized to a high value (e.g., 30000) to avoid clashes with other control IDs.
     */
    pub(crate) next_menu_item_id_counter: i32,
}

impl NativeWindowData {
    /*
     * Retrieves the HWND of a control stored in the `controls` map.
     * This provides a generic way to access control handles using their logical ID,
     * which is useful for various control manipulation tasks.
     */
    pub(crate) fn get_control_hwnd(&self, control_id: i32) -> Option<HWND> {
        self.controls.get(&control_id).copied()
    }

    /*
     * Generates a new unique `i32` ID for a menu item.
     * This is used during menu creation for items that have a `MenuAction`.
     * The counter is incremented to ensure uniqueness within this window.
     */
    pub(crate) fn generate_menu_item_id(&mut self) -> i32 {
        let id = self.next_menu_item_id_counter;
        self.next_menu_item_id_counter += 1;
        id
    }
}

// Context passed to `CreateWindowExW` via `lpCreateParams`.
// This allows the static `WndProc` to retrieve the necessary `Arc`-ed state
// for the specific window instance being created.
struct WindowCreationContext {
    internal_state_arc: Arc<Win32ApiInternalState>,
    window_id: WindowId,
}

/*
 * Registers the main window class for the application if not already registered.
 * This function sets up the `WNDCLASSEXW` structure with the window procedure
 * (`facade_wnd_proc_router`), icons, cursor, and background brush. It uses
 * the application name from `Win32ApiInternalState` to create a unique class name.
 */
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
            log::error!(
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
            log::debug!(
                "Platform: Window class '{}' registered successfully.",
                internal_state.app_name_for_class
            );
            Ok(())
        }
    }
}

/*
 * Creates a native Win32 window.
 * This function uses `CreateWindowExW` to create the window with the specified
 * title, dimensions, and style. It passes a `WindowCreationContext` (containing
 * an Arc to `Win32ApiInternalState` and the `WindowId`) as `lpCreateParams`,
 * which is retrieved by the `WndProc` during `WM_NCCREATE`.
 */
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
            None, // No menu for now (will be set by CreateMainMenu command)
            Some(internal_state_arc.h_instance),
            Some(Box::into_raw(creation_context) as *mut c_void), // Pass context
        )?;

        Ok(hwnd)
    }
}

/*
 * The main window procedure (WndProc) router for all windows created by this platform layer.
 *
 * This static function receives messages from the OS. It retrieves the
 * per-window `WindowCreationContext` (which contains an `Arc` to `Win32ApiInternalState`
 * and the `WindowId`) stored in the window's user data. It then calls the
 * instance method `handle_window_message` on `Win32ApiInternalState` to process the message.
 * On `WM_NCDESTROY`, it cleans up the `WindowCreationContext`.
 */
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
    /*
     * Handles window messages for a specific window instance.
     * This method is called by `facade_wnd_proc_router` and processes
     * relevant messages (e.g., WM_CREATE, WM_SIZE, WM_COMMAND, WM_CLOSE, WM_DESTROY,
     * WM_NOTIFY, WM_PAINT). It translates them into `AppEvent`s to be sent to the
     * application logic or performs direct actions. It may also override the default
     * message result (`lresult_override`).
     */
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
            }
            WM_SIZE => {
                event_to_send = self.handle_wm_size(hwnd, wparam, lparam, window_id);
            }
            WM_COMMAND => {
                event_to_send = self.handle_wm_command(hwnd, wparam, lparam, window_id);
            }
            WM_CLOSE => {
                event_to_send = Some(AppEvent::WindowCloseRequestedByUser { window_id });
                lresult_override = Some(LRESULT(0));
            }
            WM_DESTROY => {
                event_to_send = self.handle_wm_destroy(hwnd, wparam, lparam, window_id);
            }
            WM_NCDESTROY => {
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
                let hwnd_static_ctrl_from_msg = HWND(lparam.0 as *mut c_void);

                if let Some(windows_guard) = self.window_map.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if Some(hwnd_static_ctrl_from_msg)
                            == window_data.get_control_hwnd(ID_STATUS_BAR_CTRL)
                        {
                            unsafe {
                                if window_data.status_bar_current_severity == MessageSeverity::Error
                                {
                                    SetTextColor(hdc_static_ctrl, COLORREF(0x000000FF)); // Red
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
                if lresult_override.is_none() {
                    return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
                }
            }
            _ => {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
        }

        if let Some(event) = event_to_send {
            if let Some(handler_arc) = event_handler_opt {
                if let Ok(mut handler_guard) = handler_arc.lock() {
                    handler_guard.handle_event(event);
                } else {
                    log::error!("Platform: Failed to lock event handler in handle_window_message.");
                }
            } else {
                log::error!(
                    "Platform: Event handler not available in handle_window_message (Will be fixeed in Phase 8)."
                );
            }
        }

        if let Some(lresult) = lresult_override {
            lresult
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    /*
     * Retrieves the current state of a TreeView item after a click event
     * on its checkbox and constructs an `AppEvent::TreeViewItemToggledByUser`.
     * This function is called in response to `WM_APP_TREEVIEW_CHECKBOX_CLICKED`,
     * which is a custom message posted by the `WM_NOTIFY` handler when a
     * checkbox click is detected on the TreeView.
     */
    fn get_tree_item_toggle_event(
        self: &Arc<Self>,
        window_id: WindowId,
        h_item: HTREEITEM,
    ) -> Option<AppEvent> {
        let windows_guard = self.window_map.read().ok()?;
        let window_data = windows_guard.get(&window_id)?;

        let hwnd_treeview = window_data.get_control_hwnd(control_treeview::ID_TREEVIEW_CTRL)?;
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
                hwnd_treeview, // Use HWND from controls map
                TVM_GETITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tv_item_get as *mut _ as isize)),
            )
        };

        if get_item_result.0 == 0 {
            log::error!(
                "Platform (get_tree_item_toggle_event): TVM_GETITEMW FAILED for h_item {:?}. Error: {:?}",
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
        let app_item_id: TreeItemId;

        if app_item_id_val != 0 {
            app_item_id = TreeItemId(app_item_id_val);
        } else {
            if let Some(mapped_id) = tv_state.htreeitem_to_item_id.get(&(h_item.0)) {
                app_item_id = *mapped_id;
            } else {
                log::error!(
                    "Platform (get_tree_item_toggle_event): AppItemId is 0 via lParam AND h_item {:?} not found in htreeitem_to_item_id map.",
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

    /*
     * Handles the WM_CREATE message for a window.
     * This is called when the window is first created. Its primary responsibility
     * is now minimal, as most child control creation is command-driven by the
     * `ui_description_layer` and executed via `PlatformCommand`s. This function
     * currently only logs the event.
     */
    fn handle_wm_create(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) {
        log::debug!(
            "Platform: WM_CREATE for HWND {:?}, WindowId {:?}",
            hwnd,
            window_id
        );
    }

    /*
     * Handles the WM_SIZE message for a window.
     * This is called when the window's size changes. It's responsible for
     * resizing and repositioning child controls like the TreeView, buttons,
     * and status bar to fit the new window dimensions. It uses the generic
     * `get_control_hwnd` method on `NativeWindowData` to retrieve control handles.
     * It also generates an `AppEvent::WindowResized`.
     */
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

                // Resize TreeView using its control ID
                if let Some(hwnd_tv) =
                    window_data.get_control_hwnd(control_treeview::ID_TREEVIEW_CTRL)
                {
                    if !hwnd_tv.is_invalid() {
                        unsafe {
                            let _ = MoveWindow(
                                hwnd_tv,
                                0,
                                0,
                                client_width,
                                treeview_height.max(0),
                                true,
                            );
                        }
                    }
                }

                // Resize Button
                if let Some(hwnd_btn) = window_data.get_control_hwnd(ID_BUTTON_GENERATE_ARCHIVE) {
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

                // Resize Status Bar
                if let Some(hwnd_status) = window_data.get_control_hwnd(ID_STATUS_BAR_CTRL) {
                    if !hwnd_status.is_invalid() {
                        unsafe {
                            let _ = MoveWindow(
                                hwnd_status,
                                0,
                                status_bar_y_pos,
                                client_width,
                                STATUS_BAR_HEIGHT.max(0),
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

    /*
     * Handles the WM_COMMAND message for a window.
     * This is typically generated by menu selections or control interactions (like button clicks).
     * For menu items, it looks up the `control_id` in the `menu_action_map` of the
     * `NativeWindowData` to find the corresponding `MenuAction`. If found, it generates
     * an `AppEvent::MenuActionClicked`. For button clicks, it generates an
     * `AppEvent::ButtonClicked`.
     */
    fn handle_wm_command(
        self: &Arc<Self>,
        _hwnd: HWND,
        wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let control_id = loword_from_wparam(wparam);
        let notification_code = highord_from_wparam(wparam);

        if notification_code == 0 || notification_code == 1 {
            // Menu item or accelerator
            if let Ok(windows_guard) = self.window_map.read() {
                if let Some(window_data) = windows_guard.get(&window_id) {
                    if let Some(action) = window_data.menu_action_map.get(&control_id) {
                        log::debug!(
                            "Platform: Menu action {:?} (ID {}) clicked for window {:?}.",
                            action,
                            control_id,
                            window_id
                        );
                        return Some(AppEvent::MenuActionClicked {
                            window_id,
                            action: *action,
                        });
                    } else {
                        log::warn!(
                            "Platform: WM_COMMAND for unknown menu ID {} received for window {:?}.",
                            control_id,
                            window_id
                        );
                    }
                }
            } else {
                log::error!("Platform: Failed to get read lock for menu_action_map lookup.");
            }
        } else if notification_code as u32 == BN_CLICKED {
            if control_id == ID_BUTTON_GENERATE_ARCHIVE {
                log::debug!(
                    "Platform: Button ID {} clicked for window {:?}.",
                    control_id,
                    window_id
                );
                return Some(AppEvent::ButtonClicked {
                    window_id,
                    control_id,
                });
            }
        }
        None
    }

    /*
     * Handles the WM_DESTROY message for a window.
     * This is called when the window is being destroyed (after `WM_CLOSE` but before
     * `WM_NCDESTROY`). It removes the window's data from the internal `window_map`
     * and decrements the `active_windows_count`. If this count reaches zero,
     * it posts a `WM_QUIT` message to terminate the application's message loop.
     * It also generates an `AppEvent::WindowDestroyed`.
     */
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

    /*
     * Handles the WM_NOTIFY message, which is sent by common controls to their parent window.
     * This function specifically looks for notifications from the TreeView control,
     * such as NM_CLICK (for checkbox interactions) or TVN_ITEMCHANGEDW (for other state changes).
     * For NM_CLICK on a state icon (checkbox), it posts a custom `WM_APP_TREEVIEW_CHECKBOX_CLICKED`
     * message to handle the state change logic.
     * It translates TVN_ITEMCHANGEDW into appropriate `AppEvent`s via `control_treeview` module.
     */
    fn handle_wm_notify(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let nmhdr_ptr = lparam.0 as *const NMHDR;
        if nmhdr_ptr.is_null() {
            return None;
        }
        let nmhdr = unsafe { &*nmhdr_ptr };

        if nmhdr.idFrom as i32 != control_treeview::ID_TREEVIEW_CTRL {
            return None;
        }

        match nmhdr.code {
            NM_CLICK => {
                let hwnd_tv_from_notify = nmhdr.hwndFrom;
                if hwnd_tv_from_notify.is_invalid() {
                    return None;
                }

                let mut screen_pt_of_click = POINT::default();
                unsafe {
                    if GetCursorPos(&mut screen_pt_of_click).is_err() {
                        return None;
                    }
                }

                let mut client_pt_for_hittest = screen_pt_of_click;
                unsafe {
                    if ScreenToClient(hwnd_tv_from_notify, &mut client_pt_for_hittest) == BOOL(0) {
                        return None;
                    }
                }

                if let Some(windows_guard) = self.window_map.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        // Check if the notification is from THE TreeView we manage for this window
                        if Some(hwnd_tv_from_notify)
                            != window_data.get_control_hwnd(control_treeview::ID_TREEVIEW_CTRL)
                        {
                            log::warn!(
                                "Platform: NM_CLICK received from HWND {:?} which is not the registered TreeView HWND for WinID {:?}",
                                hwnd_tv_from_notify,
                                window_id
                            );
                            return None;
                        }
                        // Ensure treeview_state exists (though HWND is now from controls map)
                        if window_data.treeview_state.is_none() {
                            log::warn!(
                                "Platform: NM_CLICK received for TreeView, but no treeview_state for WinID {:?}",
                                window_id
                            );
                            return None;
                        }

                        let mut tvht_info = TVHITTESTINFO {
                            pt: client_pt_for_hittest,
                            flags: TVHITTESTINFO_FLAGS(0),
                            hItem: HTREEITEM(0),
                        };
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

                        if h_item_hit.0 != 0 && (tvht_info.flags.0 & TVHT_ONITEMSTATEICON.0) != 0 {
                            unsafe {
                                PostMessageW(
                                    Some(hwnd),
                                    WM_APP_TREEVIEW_CHECKBOX_CLICKED,
                                    WPARAM(h_item_hit.0 as usize),
                                    LPARAM(0),
                                )
                                .unwrap_or_else(|e| {
                                    log::error!(
                                        "Platform: Failed to post checkbox-clicked msg: {:?}",
                                        e
                                    )
                                });
                            }
                        }
                    }
                }
                return None;
            }
            TVN_ITEMCHANGEDW => {
                return control_treeview::handle_treeview_itemchanged_notification(
                    self, window_id, lparam,
                );
            }
            _ => {}
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
    log::debug!(
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
                        log::error!("DestroyWindow failed: {:?}", err);
                    }
                }
            }
        }
    }
    Ok(())
}

/*
 * Updates the text of the status bar control and triggers a repaint.
 * The actual color change based on severity is handled by `WM_CTLCOLORSTATIC`
 * in `Win32ApiInternalState::handle_window_message`, using the
 * `status_bar_current_severity` stored in `NativeWindowData`.
 * This function assumes the `severity` has already been processed and stored.
 */
pub(crate) fn update_status_bar_text_impl(
    window_data: &mut NativeWindowData,
    text: &str,
    _severity: MessageSeverity,
) -> PlatformResult<()> {
    let hwnd_status_opt = window_data
        .get_control_hwnd(ID_STATUS_BAR_CTRL)
        .or(window_data.get_control_hwnd(ID_STATUS_BAR_CTRL));

    if let Some(hwnd_status) = hwnd_status_opt {
        unsafe {
            if SetWindowTextW(hwnd_status, &HSTRING::from(text)).is_err() {
                return Err(PlatformError::OperationFailed(format!(
                    "SetWindowTextW for status bar failed: {:?}",
                    GetLastError()
                )));
            }
            InvalidateRect(Some(hwnd_status), None, true);
        }
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "Status bar HWND not found for WindowId {:?}",
            window_data.id
        )))
    }
}
