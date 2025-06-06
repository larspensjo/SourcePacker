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
    AppEvent, CheckState, DockStyle, LayoutRule, MenuAction, MessageSeverity, PlatformCommand,
    TreeItemId, WindowId,
};
use crate::app_logic::ui_constants;

use windows::{
    Win32::{
        Foundation::{
            COLORREF, ERROR_INVALID_WINDOW_HANDLE, GetLastError, HWND, LPARAM, LRESULT, POINT,
            RECT, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, COLOR_WINDOWTEXT, CreateFontW,
            CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_QUALITY, DeleteObject, Ellipse, EndPaint,
            FF_DONTCARE, FW_NORMAL, FillRect, GetDC, GetDeviceCaps, GetSysColor, GetSysColorBrush,
            HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect, LOGPIXELSY, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, ReleaseDC, ScreenToClient, SelectObject, SetBkMode, SetTextColor,
            TRANSPARENT,
        },
        System::WindowsProgramming::MulDiv,
        UI::Controls::{
            CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT,
            CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTPAINT, HTREEITEM, NM_CLICK, NM_CUSTOMDRAW, NMHDR,
            NMTREEVIEWW, NMTVCUSTOMDRAW, TVHITTESTINFO, TVHITTESTINFO_FLAGS, TVHT_ONITEMSTATEICON,
            TVI_LAST, TVIF_CHILDREN, TVIF_PARAM, TVIF_STATE, TVIF_TEXT, TVINSERTSTRUCTW,
            TVINSERTSTRUCTW_0, TVIS_STATEIMAGEMASK, TVITEMEXW, TVITEMEXW_CHILDREN, TVM_DELETEITEM,
            TVM_GETITEMRECT, TVM_GETITEMW, TVM_HITTEST, TVM_INSERTITEMW, TVM_SETITEMW,
            TVN_ITEMCHANGEDW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, HSTRING, PCWSTR},
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

// Control IDs
pub(crate) const ID_BUTTON_GENERATE_ARCHIVE: i32 = 1002;

pub(crate) const ID_DIALOG_INPUT_EDIT: i32 = 3001;
pub(crate) const ID_DIALOG_INPUT_PROMPT_STATIC: i32 = 3002;

pub(crate) const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");
pub(crate) const WC_STATIC: PCWSTR = windows::core::w!("STATIC");
pub(crate) const SS_LEFT: WINDOW_STYLE = WINDOW_STYLE(0x00000000_u32);

pub(crate) const WM_APP_TREEVIEW_CHECKBOX_CLICKED: u32 = WM_APP + 0x100;

// pub const BUTTON_AREA_HEIGHT: i32 = 50; // No longer used directly by generic layout
pub const STATUS_BAR_HEIGHT: i32 = 25; // Still used by ui_description_layer for fixed_size
// pub(crate) const BUTTON_X_PADDING: i32 = 10; // No longer used directly
// const BUTTON_Y_PADDING_IN_AREA: i32 = 10; // No longer used by new layout logic
// pub(crate) const BUTTON_WIDTH: i32 = 150; // No longer used directly
// pub(crate) const BUTTON_HEIGHT: i32 = 30; // No longer used directly

// A constant for an invalid HWND, useful for initializing or comparisons.
pub(crate) const HWND_INVALID: HWND = HWND(std::ptr::null_mut());

// Constants for the "New" item indicator circle
const CIRCLE_DIAMETER: i32 = 6;
const CIRCLE_COLOR_BLUE: COLORREF = COLORREF(0x00FF0000); // BGR format for Blue

/*
 * Holds native data associated with a specific window managed by the platform layer.
 * This includes the native window handle (`HWND`), a map of control IDs to their
 * `HWND`s, any control-specific states (like for the TreeView),
 * a map for menu item actions (`menu_action_map`),
 * a counter for generating unique menu item IDs (`next_menu_item_id_counter`),
 * a list of layout rules (`layout_rules`) for positioning controls, and
 * severity information for new labels.
 */
#[derive(Debug)]
pub(crate) struct NativeWindowData {
    pub(crate) hwnd: HWND,
    pub(crate) id: WindowId,
    // The specific internal state for the TreeView control if one exists.
    pub(crate) treeview_state: Option<control_treeview::TreeViewInternalState>,
    // HWNDs for various controls (buttons, status bar, treeview, etc.)
    pub(crate) controls: HashMap<i32, HWND>,
    // Maps dynamically generated `i32` menu item IDs to their semantic `MenuAction`.
    pub(crate) menu_action_map: HashMap<i32, MenuAction>,
    // Counter to generate unique `i32` IDs for menu items that have an action.
    pub(crate) next_menu_item_id_counter: i32,
    // Layout rules for controls within this window.
    pub(crate) layout_rules: Option<Vec<LayoutRule>>,
    /// he current severity for each new status label, keyed by their logical ID.
    pub(crate) label_severities: HashMap<i32, MessageSeverity>,
    pub(crate) status_bar_font: Option<HFONT>,
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
            log::debug!(
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
            .application_event_handler
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
                // TODO: Move this to a separate function
                let nmhdr_ptr = lparam.0 as *const NMHDR;
                if !nmhdr_ptr.is_null() {
                    let nmhdr = unsafe { &*nmhdr_ptr };
                    if nmhdr.idFrom as i32 == control_treeview::ID_TREEVIEW_CTRL
                        && nmhdr.code == NM_CUSTOMDRAW
                    {
                        lresult_override = Some(self.handle_nm_customdraw_treeview(
                            window_id,
                            lparam,
                            event_handler_opt.as_ref(),
                        ));
                    } else {
                        event_to_send =
                            self.handle_wm_notify_general(hwnd, wparam, lparam, window_id, nmhdr);
                    }
                }
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
                // TODO: Move this to a separate function
                let hdc_static_ctrl = HDC(wparam.0 as *mut c_void);
                let hwnd_static_ctrl_from_msg = HWND(lparam.0 as *mut c_void);

                let mut handled = false;
                if let Some(windows_map_guard) = self.active_windows.read().ok() {
                    if let Some(window_data) = windows_map_guard.get(&window_id) {
                        let control_id_of_static =
                            unsafe { GetDlgCtrlID(hwnd_static_ctrl_from_msg) };
                        if let Some(severity) =
                            window_data.label_severities.get(&control_id_of_static)
                        {
                            unsafe {
                                let color = match severity {
                                    MessageSeverity::Error => COLORREF(0x000000FF),
                                    MessageSeverity::Warning => COLORREF(0x0000A5FF),
                                    _ => COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
                                };
                                SetTextColor(hdc_static_ctrl, color);
                                SetBkMode(hdc_static_ctrl, TRANSPARENT);
                                lresult_override =
                                    Some(LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize));
                                handled = true;
                            }
                        }
                    }
                }
                if !handled {
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
                log::warn!(
                    "Platform: Event handler not available in handle_window_message for event {:?}.",
                    event
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
        let windows_guard = self.active_windows.read().ok()?;
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

        // Create and store the status bar font
        if let Ok(mut windows_map_guard) = self.active_windows.write() {
            if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                let font_name_hstring = HSTRING::from("Segoe UI"); // Or "Tahoma"
                let font_point_size = 9;

                let hdc_screen = unsafe { GetDC(None) };
                let logical_font_height = if !hdc_screen.is_invalid() {
                    let height = -unsafe {
                        MulDiv(
                            font_point_size,
                            GetDeviceCaps(Some(hdc_screen), LOGPIXELSY),
                            72,
                        )
                    };
                    unsafe { ReleaseDC(None, hdc_screen) };
                    height
                } else {
                    log::warn!(
                        "Could not get screen DC for font metrics, using direct point size for height."
                    );
                    -font_point_size // Fallback if GetDC fails
                };

                let h_font = unsafe {
                    CreateFontW(
                        logical_font_height,  // nHeight
                        0,                    // nWidth
                        0,                    // nEscapement
                        0,                    // nOrientation
                        FW_NORMAL.0 as i32,   // fnWeight
                        0,                    // fdwItalic
                        0,                    // fdwUnderline
                        0,                    // fdwStrikeOut
                        DEFAULT_CHARSET,      // nCharSet
                        OUT_DEFAULT_PRECIS,   // nOutPrecision
                        CLIP_DEFAULT_PRECIS,  // nClipPrecision
                        DEFAULT_QUALITY,      // nQuality
                        FF_DONTCARE.0 as u32, // nPitchAndFamily (FF_SWISS for Segoe UI/Tahoma often works well)
                        &font_name_hstring,   // lpszFaceName
                    )
                };

                if h_font.is_invalid() {
                    log::error!("Failed to create status bar font: {:?}", unsafe {
                        GetLastError()
                    });
                    window_data.status_bar_font = None;
                } else {
                    log::debug!(
                        "Status bar font created: {:?} for window {:?}",
                        h_font,
                        window_id
                    );
                    window_data.status_bar_font = Some(h_font);
                }
            }
        } else {
            log::error!(
                "Failed to get write lock on active_windows in WM_CREATE for font creation."
            );
        }
    }

    /*
     * Retrieves the HWND of the parent control for a given logical control ID.
     * If parent_control_id is None, it returns the main window's HWND.
     * Otherwise, it looks up the parent control's HWND from the NativeWindowData.controls map.
     */
    fn get_parent_hwnd(
        window_data: &NativeWindowData,
        parent_control_id: Option<i32>,
    ) -> Option<HWND> {
        match parent_control_id {
            None => Some(window_data.hwnd), // Main window is the parent
            Some(id) => window_data.get_control_hwnd(id),
        }
    }

    /*
     * Applies layout rules to child controls within a given parent rectangle.
     * This function is called recursively to handle hierarchical layouts.
     * It filters rules for direct children of the specified parent_id, sorts them,
     * and positions them according to their DockStyle and other properties.
     * For child controls that are themselves panels with layout rules, it recurses.
     */
    fn apply_layout_rules_for_children(
        self: &Arc<Self>,
        window_id: WindowId,
        parent_id_for_layout: Option<i32>, // Logical ID of the parent control, None for main window
        parent_rect: RECT,                 // Client rectangle of the parent HWND
        all_window_rules: &[LayoutRule],
        all_controls_map: &HashMap<i32, HWND>,
    ) {
        log::trace!(
            "Applying layout for parent_id {:?}, rect: {:?}",
            parent_id_for_layout,
            parent_rect
        );

        let mut child_rules: Vec<&LayoutRule> = all_window_rules
            .iter()
            .filter(|r| r.parent_control_id == parent_id_for_layout)
            .collect();
        child_rules.sort_by_key(|r| r.order);

        let mut current_available_rect = parent_rect;
        let mut fill_candidates: Vec<(&LayoutRule, HWND)> = Vec::new();
        let mut proportional_fill_candidates: Vec<(&LayoutRule, HWND)> = Vec::new();

        for rule in &child_rules {
            let control_hwnd_opt = all_controls_map.get(&rule.control_id).copied();
            if control_hwnd_opt.is_none() || control_hwnd_opt == Some(HWND_INVALID) {
                log::warn!(
                    "Layout: HWND for control ID {} not found or invalid, skipping layout.",
                    rule.control_id
                );
                continue;
            }
            let control_hwnd = control_hwnd_opt.unwrap();

            match rule.dock_style {
                DockStyle::Top | DockStyle::Bottom | DockStyle::Left | DockStyle::Right => {
                    let mut item_rect = RECT {
                        left: current_available_rect.left + rule.margin.3,
                        top: current_available_rect.top + rule.margin.0,
                        right: current_available_rect.right - rule.margin.1,
                        bottom: current_available_rect.bottom - rule.margin.2,
                    };

                    let size = rule.fixed_size.unwrap_or(0);

                    match rule.dock_style {
                        DockStyle::Top => {
                            item_rect.bottom = item_rect.top + size;
                            current_available_rect.top = item_rect.bottom + rule.margin.2;
                        }
                        DockStyle::Bottom => {
                            item_rect.top = item_rect.bottom - size;
                            current_available_rect.bottom = item_rect.top - rule.margin.0;
                        }
                        DockStyle::Left => {
                            item_rect.right = item_rect.left + size;
                            current_available_rect.left = item_rect.right + rule.margin.1;
                        }
                        DockStyle::Right => {
                            item_rect.left = item_rect.right - size;
                            current_available_rect.right = item_rect.left - rule.margin.3;
                        }
                        _ => unreachable!(), // Already filtered
                    }
                    unsafe {
                        MoveWindow(
                            control_hwnd,
                            item_rect.left,
                            item_rect.top,
                            (item_rect.right - item_rect.left).max(0),
                            (item_rect.bottom - item_rect.top).max(0),
                            true,
                        );
                    }
                    // If this control is a panel, recursively layout its children
                    if all_window_rules
                        .iter()
                        .any(|r| r.parent_control_id == Some(rule.control_id))
                    {
                        let panel_client_rect = RECT {
                            left: 0,
                            top: 0,
                            right: (item_rect.right - item_rect.left).max(0),
                            bottom: (item_rect.bottom - item_rect.top).max(0),
                        };
                        self.apply_layout_rules_for_children(
                            window_id,
                            Some(rule.control_id),
                            panel_client_rect, // The new client rect for this panel
                            all_window_rules,
                            all_controls_map,
                        );
                    }
                }
                DockStyle::Fill => {
                    fill_candidates.push((rule, control_hwnd));
                }
                DockStyle::ProportionalFill { .. } => {
                    proportional_fill_candidates.push((rule, control_hwnd));
                }
                DockStyle::None => { /* No automated layout */ }
            }
        }

        // Handle ProportionalFill candidates (assuming horizontal layout for now)
        if !proportional_fill_candidates.is_empty() {
            let total_width_for_proportional =
                (current_available_rect.right - current_available_rect.left).max(0);
            let total_height_for_proportional =
                (current_available_rect.bottom - current_available_rect.top).max(0);

            let total_weight: f32 = proportional_fill_candidates
                .iter()
                .map(|(rule, _)| {
                    if let DockStyle::ProportionalFill { weight } = rule.dock_style {
                        weight
                    } else {
                        0.0
                    }
                })
                .sum();

            if total_weight > 0.0 {
                let mut current_x = current_available_rect.left;
                for (rule, control_hwnd) in proportional_fill_candidates {
                    if let DockStyle::ProportionalFill { weight } = rule.dock_style {
                        let proportion = weight / total_weight;
                        let item_width = (total_width_for_proportional as f32 * proportion) as i32;

                        let final_x = current_x + rule.margin.3;
                        let final_y = current_available_rect.top + rule.margin.0;
                        let final_width = (item_width - rule.margin.3 - rule.margin.1).max(0);
                        let final_height =
                            (total_height_for_proportional - rule.margin.0 - rule.margin.2).max(0);

                        unsafe {
                            MoveWindow(
                                control_hwnd,
                                final_x,
                                final_y,
                                final_width,
                                final_height,
                                true,
                            );
                        }
                        current_x += item_width;

                        // If this proportional item is a panel, recursively layout its children
                        if all_window_rules
                            .iter()
                            .any(|r| r.parent_control_id == Some(rule.control_id))
                        {
                            let panel_client_rect_prop = RECT {
                                left: 0,
                                top: 0,
                                right: final_width,
                                bottom: final_height,
                            };
                            self.apply_layout_rules_for_children(
                                window_id,
                                Some(rule.control_id),
                                panel_client_rect_prop,
                                all_window_rules,
                                all_controls_map,
                            );
                        }
                    }
                }
            }
        }

        // Handle Fill candidates (only one Fill per parent is typical)
        if let Some((rule, control_hwnd)) = fill_candidates.first() {
            if fill_candidates.len() > 1 {
                log::warn!(
                    "Layout: Multiple Fill controls for parent_id {:?}. Only the first one (ID {}) will be used.",
                    parent_id_for_layout,
                    rule.control_id
                );
            }
            let fill_rect = RECT {
                left: current_available_rect.left + rule.margin.3,
                top: current_available_rect.top + rule.margin.0,
                right: current_available_rect.right - rule.margin.1,
                bottom: current_available_rect.bottom - rule.margin.2,
            };
            unsafe {
                MoveWindow(
                    *control_hwnd,
                    fill_rect.left,
                    fill_rect.top,
                    (fill_rect.right - fill_rect.left).max(0),
                    (fill_rect.bottom - fill_rect.top).max(0),
                    true,
                );
            }
            // If the Fill control is a panel, recursively layout its children
            if all_window_rules
                .iter()
                .any(|r| r.parent_control_id == Some(rule.control_id))
            {
                let panel_client_rect_fill = RECT {
                    left: 0,
                    top: 0,
                    right: (fill_rect.right - fill_rect.left).max(0),
                    bottom: (fill_rect.bottom - fill_rect.top).max(0),
                };
                self.apply_layout_rules_for_children(
                    window_id,
                    Some(rule.control_id),
                    panel_client_rect_fill, // The new client rect for this panel
                    all_window_rules,
                    all_controls_map,
                );
            }
        }
    }

    /*
     * Triggers a recalculation and application of layout rules for the specified window.
     * This function retrieves the window's current client rectangle and its defined layout rules,
     * then initiates the hierarchical layout process starting from the main window area.
     * It's typically called after layout rules are defined or changed, or when an explicit
     * re-layout is required.
     */
    pub(crate) fn trigger_layout_recalculation(self: &Arc<Self>, window_id: WindowId) {
        log::debug!(
            "Win32ApiInternalState: trigger_layout_recalculation called for WinID {:?}",
            window_id
        );

        let active_windows_guard = match self.active_windows.read() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "Failed to get read lock on active_windows for trigger_layout_recalculation: {:?}",
                    e
                );
                return;
            }
        };

        let window_data = match active_windows_guard.get(&window_id) {
            Some(data) => data,
            None => {
                log::warn!(
                    "trigger_layout_recalculation: WindowData not found for WinID {:?}.",
                    window_id
                );
                return;
            }
        };

        if window_data.hwnd.is_invalid() {
            log::warn!(
                "trigger_layout_recalculation: HWND for WinID {:?} is invalid. Cannot layout.",
                window_id
            );
            return;
        }

        let rules = match &window_data.layout_rules {
            Some(rules) => rules,
            None => {
                log::debug!(
                    "trigger_layout_recalculation: Layout rules are None for WinID {:?}. Cannot layout.",
                    window_id
                );
                return;
            }
        };

        if rules.is_empty() {
            log::debug!(
                "trigger_layout_recalculation: No layout rules to apply for WinID {:?}",
                window_id
            );
            return;
        }

        let mut client_rect = RECT::default();
        if unsafe { GetClientRect(window_data.hwnd, &mut client_rect) }.is_err() {
            log::error!(
                "trigger_layout_recalculation: GetClientRect failed for WinID {:?}. Error: {:?}",
                window_id,
                unsafe { GetLastError() }
            );
            return;
        }

        log::trace!(
            "Win32ApiInternalState: Applying layout with client_rect: {:?}, for WinID {:?}",
            client_rect,
            window_id
        );

        self.apply_layout_rules_for_children(
            window_id,
            None,
            client_rect,
            rules,
            &window_data.controls,
        );
    }

    /*
     * Handles the WM_SIZE message for a window.
     * This is called when the window's size changes.
     * It retrieves the layout rules and control HWNDs from NativeWindowData and
     * initiates the hierarchical layout process by calling `apply_layout_rules_for_children`
     * for the main window (parent_id_for_layout: None).
     * It generates an `AppEvent::WindowResized`.
     */
    fn handle_wm_size(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let client_width = loword_from_lparam(lparam);
        let client_height = hiword_from_lparam(lparam);

        log::debug!(
            "Platform: WM_SIZE for WinID {:?}, HWND {:?}. Client: {}x{}",
            window_id,
            hwnd,
            client_width,
            client_height
        );

        if let Some(windows_guard) = self.active_windows.read().ok() {
            if let Some(window_data) = windows_guard.get(&window_id) {
                if let Some(all_window_rules) = &window_data.layout_rules {
                    if !all_window_rules.is_empty() {
                        let main_window_client_rect = RECT {
                            left: 0,
                            top: 0,
                            right: client_width,
                            bottom: client_height,
                        };
                        // Start recursive layout from the main window (parent_id_for_layout: None)
                        self.apply_layout_rules_for_children(
                            window_id,
                            None,
                            main_window_client_rect,
                            all_window_rules,
                            &window_data.controls,
                        );
                    } else {
                        log::debug!(
                            "Platform: WM_SIZE for WinID {:?} - No layout rules defined.",
                            window_id
                        );
                    }
                } else {
                    log::debug!(
                        "Platform: WM_SIZE for WinID {:?} - Layout rules are None.",
                        window_id
                    );
                }
            } else {
                log::warn!(
                    "Platform: WM_SIZE - WindowData not found for WinID {:?}",
                    window_id
                );
            }
        } else {
            log::error!("Platform: WM_SIZE - Failed to get read lock on active_windows.");
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
        _hwnd_parent: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let command_id = loword_from_wparam(wparam);
        let notification_code_raw = highord_from_wparam(wparam);
        // let hwnd_control_lparam = HWND(lparam.0 as *mut c_void);

        if lparam.0 == 0 && (notification_code_raw == 0 || notification_code_raw == 1) {
            if let Ok(windows_guard) = self.active_windows.read() {
                if let Some(window_data) = windows_guard.get(&window_id) {
                    if let Some(action) = window_data.menu_action_map.get(&command_id) {
                        log::debug!(
                            "Platform: Menu/Accelerator action {:?} (ID {}) triggered for window {:?}.",
                            action,
                            command_id,
                            window_id
                        );
                        return Some(AppEvent::MenuActionClicked {
                            window_id,
                            action: *action,
                        });
                    } else {
                        log::warn!(
                            "Platform: WM_COMMAND (Menu/Accel) for unknown ID {} received for window {:?}.",
                            command_id,
                            window_id
                        );
                    }
                }
            } else {
                log::error!(
                    "Platform: Failed to get read lock for menu_action_map lookup for Menu/Accel command."
                );
            }
        } else if lparam.0 != 0 {
            let control_id_from_wparam = command_id;
            let control_notification_code = notification_code_raw as u32;

            if control_notification_code == BN_CLICKED {
                // Check if this button click has a registered action or needs generic handling
                // For now, only specific known button IDs are handled directly here if needed,
                // otherwise AppEvent::ButtonClicked is sent.
                log::debug!(
                    "Platform: Button ID {} clicked for window {:?}.",
                    control_id_from_wparam,
                    window_id
                );
                return Some(AppEvent::ButtonClicked {
                    window_id,
                    control_id: control_id_from_wparam,
                });
            }
        } else {
            log::warn!(
                "Platform: Unhandled WM_COMMAND variant: command_id={}, notification_code={}, lparam={:?} for window {:?}",
                command_id,
                notification_code_raw,
                lparam,
                window_id
            );
        }
        None
    }

    /*
     * Handles the WM_DESTROY message for a window.
     * This is called when the window is being destroyed (after `WM_CLOSE` but before
     * `WM_NCDESTROY`). It removes the window's data from the internal `active_windows` map.
     * `check_if_should_quit_after_window_close` is then called to determine if `WM_QUIT`
     * should be posted. It generates an `AppEvent::WindowDestroyed`.
     */
    fn handle_wm_destroy(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        log::debug!(
            "Platform: WM_DESTROY for WindowId {:?}. Removing from active_windows.",
            window_id
        );
        if let Ok(mut windows_map_guard) = self.active_windows.write() {
            if let Some(mut window_data_ref) = windows_map_guard.get_mut(&window_id) {
                // Get mutable ref
                // Clean up custom font if it exists
                if let Some(h_font) = window_data_ref.status_bar_font.take() {
                    // .take() to get ownership and remove from Option
                    if !h_font.is_invalid() {
                        log::debug!(
                            "Platform: Deleting status bar font {:?} for WindowId {:?}",
                            h_font,
                            window_id
                        );
                        unsafe { DeleteObject(HGDIOBJ(h_font.0)) };
                    }
                }
            }

            if windows_map_guard.remove(&window_id).is_some() {
                log::debug!(
                    "Platform: Successfully removed WindowId {:?} from active_windows.",
                    window_id
                );
            } else {
                log::warn!(
                    "Platform: WindowId {:?} not found in active_windows during WM_DESTROY.",
                    window_id
                );
            }
        } else {
            log::error!(
                "Platform: Failed to lock active_windows for write during WM_DESTROY for WindowId {:?}.",
                window_id
            );
        }
        self.check_if_should_quit_after_window_close();
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
     * Handles general WM_NOTIFY messages (excluding NM_CUSTOMDRAW for TreeView, which is handled separately).
     * This function specifically looks for notifications from the TreeView control,
     * such as NM_CLICK (for checkbox interactions) or TVN_ITEMCHANGEDW (for other state changes).
     * For NM_CLICK on a state icon (checkbox), it posts a custom `WM_APP_TREEVIEW_CHECKBOX_CLICKED`
     * message to handle the state change logic.
     * It translates TVN_ITEMCHANGEDW into appropriate `AppEvent`s via `control_treeview` module.
     */
    fn handle_wm_notify_general(
        self: &Arc<Self>,
        hwnd_parent: HWND,
        _wparam: WPARAM,
        lparam: LPARAM,
        window_id: WindowId,
        nmhdr: &NMHDR,
    ) -> Option<AppEvent> {
        match nmhdr.code {
            NM_CLICK => {
                // IMPORTANT: NMMOUSE doesn't give a reliable nmtv.pt. Only solution I have found is to manually call GetCursorPos+ScreenToClient.
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

                if let Some(windows_guard) = self.active_windows.read().ok() {
                    if let Some(window_data) = windows_guard.get(&window_id) {
                        if Some(hwnd_tv_from_notify)
                            != window_data.get_control_hwnd(control_treeview::ID_TREEVIEW_CTRL)
                        {
                            log::warn!(
                                "Platform: NM_CLICK from HWND {:?} not registered TreeView for WinID {:?}",
                                hwnd_tv_from_notify,
                                window_id
                            );
                            return None;
                        }
                        if window_data.treeview_state.is_none() {
                            log::warn!(
                                "Platform: NM_CLICK for TreeView, but no treeview_state for WinID {:?}",
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
                                    Some(hwnd_parent),
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

    /*
     * Handles the NM_CUSTOMDRAW notification specifically for the TreeView control.
     * This function orchestrates the custom drawing stages to render a "New" item indicator.
     * It interacts with the `PlatformEventHandler` (application logic) to determine if an
     * item is "New" (now considering files and folders appropriately) and then performs
     * the drawing operations on the item's HDC.
     */
    fn handle_nm_customdraw_treeview(
        self: &Arc<Self>,
        window_id: WindowId,
        lparam: LPARAM,
        event_handler_opt: Option<&Arc<Mutex<dyn super::types::PlatformEventHandler>>>,
    ) -> LRESULT {
        let nmtvcd = unsafe { &*(lparam.0 as *const NMTVCUSTOMDRAW) };

        match nmtvcd.nmcd.dwDrawStage {
            CDDS_PREPAINT => {
                log::trace!(
                    "NM_CUSTOMDRAW TreeView ({:?}): CDDS_PREPAINT. Requesting CDRF_NOTIFYITEMDRAW.",
                    window_id
                );
                return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
            }
            CDDS_ITEMPREPAINT => {
                let tree_item_id_val = nmtvcd.nmcd.lItemlParam;
                let tree_item_id = TreeItemId(tree_item_id_val.0 as u64);
                log::trace!(
                    "NM_CUSTOMDRAW TreeView ({:?}): CDDS_ITEMPREPAINT for TreeItemId {:?}",
                    window_id,
                    tree_item_id
                );

                if let Some(handler_arc) = event_handler_opt {
                    if let Ok(handler_guard) = handler_arc.lock() {
                        if handler_guard.is_tree_item_new(window_id, tree_item_id) {
                            log::debug!(
                                "NM_CUSTOMDRAW TreeView ({:?}): Item {:?} IS NEW (file or folder with new descendants). Requesting CDRF_NOTIFYPOSTPAINT.",
                                window_id,
                                tree_item_id
                            );
                            return LRESULT(CDRF_NOTIFYPOSTPAINT as isize);
                        } else {
                            log::trace!(
                                "NM_CUSTOMDRAW TreeView ({:?}): Item {:?} IS NOT NEW. Requesting CDRF_DODEFAULT.",
                                window_id,
                                tree_item_id
                            );
                        }
                    } else {
                        log::warn!(
                            "NM_CUSTOMDRAW TreeView ({:?}): Failed to lock event handler for ITEMPREPAINT. Defaulting for item {:?}.",
                            window_id,
                            tree_item_id
                        );
                    }
                } else {
                    log::warn!(
                        "NM_CUSTOMDRAW TreeView ({:?}): Event handler not available for ITEMPREPAINT. Defaulting for item {:?}.",
                        window_id,
                        tree_item_id
                    );
                }
                return LRESULT(CDRF_DODEFAULT as isize);
            }
            CDDS_ITEMPOSTPAINT => {
                let tree_item_id_val = nmtvcd.nmcd.lItemlParam;
                let tree_item_id = TreeItemId(tree_item_id_val.0 as u64);

                let hdc = nmtvcd.nmcd.hdc;
                let h_item_native = HTREEITEM(nmtvcd.nmcd.dwItemSpec as isize); // Actual HTREEITEM
                let hwnd_treeview = nmtvcd.nmcd.hdr.hwndFrom;

                let mut item_rect_data = RECT::default();

                unsafe {
                    let p_hitem_in_rect = &mut item_rect_data as *mut RECT as *mut HTREEITEM;
                    *p_hitem_in_rect = h_item_native;
                }

                let lparam_for_getrect = LPARAM(&mut item_rect_data as *mut _ as isize);

                let get_rect_success = unsafe {
                    SendMessageW(
                        hwnd_treeview,
                        TVM_GETITEMRECT,
                        Some(WPARAM(1)), // TRUE (1) for text-only part of the item for positioning circle next to text
                        Some(lparam_for_getrect),
                    )
                };

                if get_rect_success != LRESULT(0) {
                    // Position circle slightly to the left of the text rectangle's left edge
                    let circle_offset_x = -(CIRCLE_DIAMETER + 2); // Offset to the left of text
                    let x1 = item_rect_data.left + circle_offset_x;
                    let y1 = item_rect_data.top
                        + (item_rect_data.bottom - item_rect_data.top - CIRCLE_DIAMETER) / 2; // Vertically center with text
                    let x2 = x1 + CIRCLE_DIAMETER;
                    let y2 = y1 + CIRCLE_DIAMETER;

                    log::debug!(
                        "NM_CUSTOMDRAW TreeView ({:?}): Drawing 'New' indicator for item {:?} (HTREEITEM {:?}) at text_rect: {:?}, circle_coords: ({},{},{},{})",
                        window_id,
                        tree_item_id,
                        h_item_native,
                        item_rect_data,
                        x1,
                        y1,
                        x2,
                        y2
                    );

                    unsafe {
                        let h_brush = CreateSolidBrush(CIRCLE_COLOR_BLUE);
                        if !h_brush.is_invalid() {
                            let brush_obj = HGDIOBJ(h_brush.0);
                            let old_brush_obj = SelectObject(hdc, brush_obj);

                            Ellipse(hdc, x1, y1, x2, y2);

                            SelectObject(hdc, old_brush_obj);
                            DeleteObject(brush_obj);
                        } else {
                            log::error!(
                                "NM_CUSTOMDRAW TreeView ({:?}): Failed to create brush for 'New' indicator. LastError: {:?}",
                                window_id,
                                GetLastError()
                            );
                        }
                    }
                } else {
                    log::error!(
                        "NM_CUSTOMDRAW TreeView ({:?}): TVM_GETITEMRECT FAILED for HTREEITEM {:?}. GetLastError: {:?}",
                        window_id,
                        h_item_native,
                        unsafe { GetLastError() }
                    );
                }
                return LRESULT(CDRF_DODEFAULT as isize);
            }
            _ => {
                log::trace!(
                    "NM_CUSTOMDRAW TreeView ({:?}): Unhandled dwDrawStage: {:?}",
                    window_id,
                    nmtvcd.nmcd.dwDrawStage
                );
            }
        }
        LRESULT(CDRF_DODEFAULT as isize)
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
    if let Some(windows_guard) = internal_state.active_windows.read().ok() {
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
    if let Some(windows_guard) = internal_state.active_windows.read().ok() {
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
        "Platform: send_close_message received for WindowId {:?}, attempting to destroy native window.",
        window_id
    );
    destroy_native_window(internal_state, window_id)
}

/*
 * Attempts to destroy the native window associated with the given `WindowId`.
 * This function retrieves the HWND from the `active_windows` map and calls
 * `DestroyWindow`. Errors are logged. This is typically called in response to a
 * `PlatformCommand::CloseWindow` or when the application is quitting and needs
 * to clean up windows.
 */
pub(crate) fn destroy_native_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    let hwnd_to_destroy: Option<HWND>;
    {
        let windows_read_guard = internal_state.active_windows.read().map_err(|e| {
            log::error!(
                "Platform: Failed to lock active_windows for read (destroy_native_window): {}",
                e
            );
            PlatformError::OperationFailed(format!(
                "Failed to lock active_windows for read (destroy_native_window): {}",
                e
            ))
        })?;

        hwnd_to_destroy = windows_read_guard.get(&window_id).map(|data| data.hwnd);

        if hwnd_to_destroy.is_none() {
            log::warn!(
                "Platform: WindowId {:?} not found in active_windows for destroy_native_window.",
                window_id
            );
            return Ok(());
        }
    }

    if let Some(hwnd) = hwnd_to_destroy {
        if !hwnd.is_invalid() {
            log::debug!(
                "Platform: Calling DestroyWindow for HWND {:?} (WindowId {:?})",
                hwnd,
                window_id
            );
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let last_error = GetLastError();
                    if last_error.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        log::error!(
                            "Platform: DestroyWindow for HWND {:?} (WinID {:?}) failed: {:?}.",
                            hwnd,
                            window_id,
                            last_error
                        );
                    } else {
                        log::debug!(
                            "Platform: DestroyWindow for HWND {:?} (WinID {:?}) reported invalid handle.",
                            hwnd,
                            window_id
                        );
                    }
                } else {
                    log::debug!(
                        "Platform: DestroyWindow call succeeded for HWND {:?} (WinID {:?}).",
                        hwnd,
                        window_id
                    );
                }
            }
        } else {
            log::warn!(
                "Platform: HWND for WindowId {:?} was invalid before DestroyWindow call.",
                window_id
            );
        }
    }
    Ok(())
}
