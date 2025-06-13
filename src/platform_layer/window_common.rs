/*
 * This module provides common Win32 windowing functionalities, including
 * window class registration, native window creation, and the main window
 * procedure (WndProc) for message handling. It defines `NativeWindowData`
 * to store per-window native state and helper functions for interacting
 * with the Win32 API.
 *
 * For control-specific message handling (e.g., TreeView notifications,
 * label custom drawing), this module now primarily dispatches to dedicated
 * handlers in the `super::controls` module.
 */
use super::{
    app::Win32ApiInternalState,
    controls::{input_handler, label_handler, treeview_handler},
    error::{PlatformError, Result as PlatformResult},
    types::{
        AppEvent, DockStyle, LayoutRule, MenuAction, MessageSeverity, PlatformEventHandler,
        WindowId,
    },
};

use windows::{
    Win32::{
        Foundation::{
            ERROR_INVALID_WINDOW_HANDLE, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, CreateFontW, DEFAULT_CHARSET,
            DEFAULT_QUALITY, DeleteObject, EndPaint, FF_DONTCARE, FW_NORMAL, FillRect, GetDC,
            GetDeviceCaps, HBRUSH, HDC, HFONT, HGDIOBJ, LOGPIXELSY, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, ReleaseDC,
        },
        System::WindowsProgramming::MulDiv,
        UI::Controls::{NM_CLICK, NM_CUSTOMDRAW, NMHDR, TVN_ITEMCHANGEDW},
        UI::WindowsAndMessaging::*, // This list is massive, just import all of them.
    },
    core::{HSTRING, PCWSTR},
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

// TOOD: Control IDs used by dialog_handler, kept here for visibility if dialog_handler needs them
// but ideally, they should be private to dialog_handler or within a shared constants scope for dialogs.
pub(crate) const ID_DIALOG_INPUT_EDIT: i32 = 3001;
pub(crate) const ID_DIALOG_INPUT_PROMPT_STATIC: i32 = 3002;

// Common control class names
pub(crate) const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");
pub(crate) const WC_STATIC: PCWSTR = windows::core::w!("STATIC");
// Common style constants
pub(crate) const SS_LEFT: WINDOW_STYLE = WINDOW_STYLE(0x00000000_u32);

// Custom application message for TreeView checkbox clicks.
// Defined here as it's part of the window message protocol that window_common handles.
pub(crate) const WM_APP_TREEVIEW_CHECKBOX_CLICKED: u32 = WM_APP + 0x100;
// Custom application message used to defer the MainWindowUISetupComplete event
// until after the Win32 message loop has started. This ensures controls like the
// TreeView have completed their creation and are ready for commands such as
// populating items with checkboxes.
pub(crate) const WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE: u32 = WM_APP + 0x101;

// General UI constants
pub const STATUS_BAR_HEIGHT: i32 = 25; // Example height for status bar
pub const FILTER_DEBOUNCE_MS: u32 = 300;

// Represents an invalid HWND, useful for initialization or checks.
pub(crate) const HWND_INVALID: HWND = HWND(std::ptr::null_mut());

const SUCCESS_CODE: LRESULT = LRESULT(0);
/*
 * Holds native data associated with a specific window managed by the platform layer.
 * This includes the native window handle (`HWND`), a map of control IDs to their
 * `HWND`s, any control-specific states (like for the TreeView),
 * a map for menu item actions (`menu_action_map`),
 * a counter for generating unique menu item IDs (`next_menu_item_id_counter`),
 * a list of layout rules for positioning controls, and
 * severity information for new labels.
 */
#[derive(Debug)]
pub(crate) struct NativeWindowData {
    this_window_hwnd: HWND,
    logical_window_id: WindowId,
    // The specific internal state for the TreeView control if one exists.
    treeview_state: Option<treeview_handler::TreeViewInternalState>,
    // HWNDs for various controls (buttons, status bar, treeview, etc.)
    control_hwnd_map: HashMap<i32, HWND>,
    // Maps dynamically generated `i32` menu item IDs to their semantic `MenuAction`.
    menu_action_map: HashMap<i32, MenuAction>,
    // Counter to generate unique `i32` IDs for menu items that have an action.
    next_menu_item_id_counter: i32,
    // Layout rules for controls within this window.
    layout_rules: Option<Vec<LayoutRule>>,
    /// The current severity for each status label, keyed by its logical ID.
    label_severities: HashMap<i32, MessageSeverity>,
    /// Background color state for input controls keyed by their logical ID.
    input_bg_colors:
        HashMap<i32, crate::platform_layer::controls::input_handler::InputColorState>,
    pub(crate) status_bar_font: Option<HFONT>,
}

impl NativeWindowData {
    pub(crate) fn new(logical_window_id: WindowId) -> Self {
        Self {
            this_window_hwnd: HWND_INVALID,
            logical_window_id,
            treeview_state: None,
            control_hwnd_map: HashMap::new(),
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 30000,
            layout_rules: None,
            label_severities: HashMap::new(),
            input_bg_colors: HashMap::new(),
            status_bar_font: None,
        }
    }

    pub(crate) fn get_hwnd(&self) -> HWND {
        self.this_window_hwnd
    }

    pub(crate) fn set_hwnd(&mut self, hwnd: HWND) {
        self.this_window_hwnd = hwnd;
    }

    pub(crate) fn get_control_hwnd(&self, control_id: i32) -> Option<HWND> {
        self.control_hwnd_map.get(&control_id).copied()
    }

    pub(crate) fn register_control_hwnd(&mut self, control_id: i32, hwnd: HWND) {
        self.control_hwnd_map.insert(control_id, hwnd);
    }

    pub(crate) fn has_control(&self, control_id: i32) -> bool {
        self.control_hwnd_map.contains_key(&control_id)
    }

    pub(crate) fn has_treeview_state(&self) -> bool {
        self.treeview_state.is_some()
    }

    pub(crate) fn init_treeview_state(&mut self) {
        self.treeview_state = Some(treeview_handler::TreeViewInternalState::new());
    }

    pub(crate) fn take_treeview_state(
        &mut self,
    ) -> Option<treeview_handler::TreeViewInternalState> {
        self.treeview_state.take()
    }

    pub(crate) fn set_treeview_state(
        &mut self,
        state: Option<treeview_handler::TreeViewInternalState>,
    ) {
        self.treeview_state = state;
    }

    pub(crate) fn get_treeview_state(&self) -> Option<&treeview_handler::TreeViewInternalState> {
        self.treeview_state.as_ref()
    }

    fn generate_menu_item_id(&mut self) -> i32 {
        let id = self.next_menu_item_id_counter;
        self.next_menu_item_id_counter += 1;
        id
    }

    pub(crate) fn register_menu_action(&mut self, action: MenuAction) -> i32 {
        let id = self.generate_menu_item_id();
        self.menu_action_map.insert(id, action);
        log::debug!(
            "CommandExecutor: Mapping menu action {:?} to ID {} for window {:?}",
            action,
            id,
            self.logical_window_id
        );
        id
    }

    pub(crate) fn get_menu_action(&self, menu_id: i32) -> Option<MenuAction> {
        self.menu_action_map.get(&menu_id).copied()
    }

    pub(crate) fn iter_menu_actions(&self) -> impl Iterator<Item = (&i32, &MenuAction)> {
        self.menu_action_map.iter()
    }

    pub(crate) fn menu_action_count(&self) -> usize {
        self.menu_action_map.len()
    }

    pub(crate) fn get_next_menu_item_id_counter(&self) -> i32 {
        self.next_menu_item_id_counter
    }

    pub(crate) fn define_layout(&mut self, rules: Vec<LayoutRule>) {
        self.layout_rules = Some(rules);
    }

    pub(crate) fn get_layout_rules(&self) -> Option<&Vec<LayoutRule>> {
        self.layout_rules.as_ref()
    }

    pub(crate) fn has_layout_rules(&self) -> bool {
        self.layout_rules.is_some()
    }

    pub(crate) fn set_label_severity(&mut self, label_id: i32, severity: MessageSeverity) {
        self.label_severities.insert(label_id, severity);
    }

    pub(crate) fn get_label_severity(&self, label_id: i32) -> Option<MessageSeverity> {
        self.label_severities.get(&label_id).copied()
    }

    pub(crate) fn set_input_background_color(
        &mut self,
        control_id: i32,
        color: Option<u32>,
    ) -> crate::platform_layer::error::Result<()> {
        use crate::platform_layer::controls::input_handler::InputColorState;
        use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject};
        use windows::Win32::Foundation::{COLORREF, GetLastError};

        if let Some(existing) = self.input_bg_colors.remove(&control_id) {
            unsafe {
                let _ = DeleteObject(existing.brush.into());
            }
        }

        if let Some(c) = color {
            let colorref = COLORREF(c);
            let brush = unsafe { CreateSolidBrush(colorref) };
            if brush.is_invalid() {
                return Err(crate::platform_layer::error::PlatformError::OperationFailed(
                    format!("CreateSolidBrush failed: {:?}", unsafe { GetLastError() }),
                ));
            }
            self.input_bg_colors.insert(
                control_id,
                InputColorState {
                    color: colorref,
                    brush,
                },
            );
        }

        Ok(())
    }

    pub(crate) fn get_input_background_color(
        &self,
        control_id: i32,
    ) -> Option<&crate::platform_layer::controls::input_handler::InputColorState> {
        self.input_bg_colors.get(&control_id)
    }

    pub(crate) fn cleanup_input_background_colors(&mut self) {
        use windows::Win32::Graphics::Gdi::DeleteObject;
        for (_, state) in self.input_bg_colors.drain() {
            unsafe {
                let _ = DeleteObject(state.brush.into());
            }
        }
    }
}

// Context passed during window creation to associate Win32ApiInternalState with HWND.
struct WindowCreationContext {
    internal_state_arc: Arc<Win32ApiInternalState>,
    window_id: WindowId,
}

/*
 * Registers the main window class for the application if not already registered.
 * This function defines the common properties for all windows created by this
 * platform layer, including the window procedure (`facade_wnd_proc_router`).
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
            log::error!("Platform: RegisterClassExW failed: {:?}", error);
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
 * Uses `CreateWindowExW` and passes `WindowCreationContext` via `lpCreateParams`.
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
            WINDOW_EX_STYLE::default(),          // Optional extended window styles
            &class_name_hstring,                 // Window class name
            &HSTRING::from(title),               // Window title
            WS_OVERLAPPEDWINDOW,                 // Common window style
            CW_USEDEFAULT,                       // Default X position
            CW_USEDEFAULT,                       // Default Y position
            width,                               // Width
            height,                              // Height
            None,                                // Parent window (None for top-level)
            None,                                // Menu (None for no default menu)
            Some(internal_state_arc.h_instance), // Application instance
            Some(Box::into_raw(creation_context) as *mut c_void), // lParam for WM_CREATE/WM_NCCREATE
        )?; // Returns Result<HWND, Error>, so ? operator handles error conversion

        Ok(hwnd)
    }
}

/*
 * Main window procedure router. Retrieves `WindowCreationContext` and calls
 * `handle_window_message` on `Win32ApiInternalState`.
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
     * relevant messages. It translates them into `AppEvent`s to be sent to the
     * application logic or performs direct actions by dispatching to control handlers.
     * It may also override the default message result (`lresult_override`).
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
            WM_TIMER => {
                event_to_send = self.handle_wm_timer(hwnd, wparam, lparam, window_id);
            }
            WM_CLOSE => {
                log::debug!(
                    "WM_CLOSE received for WinID {:?}. Generating WindowCloseRequestedByUser.",
                    window_id
                );
                event_to_send = Some(AppEvent::WindowCloseRequestedByUser { window_id });
                lresult_override = Some(SUCCESS_CODE);
            }
            WM_DESTROY => {
                event_to_send = self.handle_wm_destroy(hwnd, wparam, lparam, window_id);
            }
            WM_NCDESTROY => {}
            WM_PAINT => {
                lresult_override = Some(self.handle_wm_paint(hwnd, wparam, lparam, window_id));
            }
            WM_NOTIFY => {
                (event_to_send, lresult_override) = self._handle_wm_notify_dispatch(
                    hwnd,
                    wparam,
                    lparam,
                    window_id,
                    event_handler_opt.as_ref(),
                );
            }
            WM_APP_TREEVIEW_CHECKBOX_CLICKED => {
                event_to_send = treeview_handler::handle_wm_app_treeview_checkbox_clicked(
                    self, hwnd, window_id, wparam, lparam,
                );
            }
            WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE => {
                log::debug!(
                    "handle_window_message: Received message WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE"
                );
                event_to_send = Some(AppEvent::MainWindowUISetupComplete { window_id });
            }
            WM_GETMINMAXINFO => {
                lresult_override =
                    Some(self.handle_wm_getminmaxinfo(hwnd, wparam, lparam, window_id));
            }
            WM_CTLCOLORSTATIC => {
                let hdc_static_ctrl = HDC(wparam.0 as *mut c_void);
                let hwnd_static_ctrl = HWND(lparam.0 as *mut c_void);
                lresult_override = label_handler::handle_wm_ctlcolorstatic(
                    self,
                    window_id,
                    hdc_static_ctrl,
                    hwnd_static_ctrl,
                );
            }
            WM_CTLCOLOREDIT => {
                let hdc_edit = HDC(wparam.0 as *mut c_void);
                let hwnd_edit = HWND(lparam.0 as *mut c_void);
                lresult_override =
                    input_handler::handle_wm_ctlcoloredit(self, window_id, hdc_edit, hwnd_edit);
            }
            _ => {}
        }

        if let Some(event) = event_to_send {
            if let Some(handler_arc) = event_handler_opt {
                if let Ok(mut handler_guard) = handler_arc.lock() {
                    handler_guard.handle_event(event);
                } else {
                    log::error!(
                        "Platform: Failed to lock event handler for event {:?}.",
                        event
                    );
                }
            } else {
                log::warn!(
                    "Platform: Event handler not available for event {:?}.",
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
     * Dispatches WM_NOTIFY messages to appropriate control handlers.
     * It inspects the NMHDR code to determine the type of notification and the
     * control that sent it, then routes to specific handlers (e.g., for TreeView
     * custom draw or general notifications).
     */
    fn _handle_wm_notify_dispatch(
        self: &Arc<Self>,
        hwnd_parent_window: HWND,
        _wparam_original: WPARAM,
        lparam_original: LPARAM,
        window_id: WindowId,
        event_handler_opt: Option<&Arc<Mutex<dyn PlatformEventHandler>>>,
    ) -> (Option<AppEvent>, Option<LRESULT>) {
        let nmhdr_ptr = lparam_original.0 as *const NMHDR;
        if nmhdr_ptr.is_null() {
            log::warn!("WM_NOTIFY received with null NMHDR pointer. Ignoring.");
            return (None, None);
        }
        let nmhdr = unsafe { &*nmhdr_ptr };
        let control_id_from_notify = nmhdr.idFrom as i32;

        // Check if this notification is from a TreeView control associated with this window.
        // This requires looking up the control by its ID and checking its class or type if needed,
        // or simply assuming based on notification codes like NM_CUSTOMDRAW if they are unique enough.
        // For now, we'll rely on treeview_state existing for the window and the notification code.
        let is_treeview_notification;
        {
            let windows_guard = match self.active_windows.read() {
                Ok(g) => g,
                Err(e) => {
                    log::error!(
                        "Platform: Failed to get read lock in _handle_wm_notify_dispatch: {:?}",
                        e
                    );
                    return (None, None);
                }
            };
            let window_data = match windows_guard.get(&window_id) {
                Some(wd) => wd,
                None => {
                    log::warn!(
                        "Platform: WindowData not found for WinID {:?} in _handle_wm_notify_dispatch.",
                        window_id
                    );
                    return (None, None);
                }
            };
            // A simple check: does this window have treeview_state?
            // And is the notification coming from the control ID stored for that treeview?
            // For now, assume if treeview_state exists, this NM_CUSTOMDRAW or NM_CLICK could be for it.
            // A more robust check would be to see if nmhdr.hwndFrom matches the stored TreeView HWND.
            is_treeview_notification = window_data.has_treeview_state()
                && window_data.get_control_hwnd(control_id_from_notify) == Some(nmhdr.hwndFrom);
        }

        if is_treeview_notification {
            match nmhdr.code {
                NM_CUSTOMDRAW => {
                    log::trace!(
                        "Routing NM_CUSTOMDRAW from ControlID {} to treeview_handler.",
                        control_id_from_notify
                    );
                    let lresult = treeview_handler::handle_nm_customdraw(
                        self,
                        window_id,
                        lparam_original,
                        event_handler_opt,
                        control_id_from_notify,
                    );
                    return (None, Some(lresult));
                }
                NM_CLICK => {
                    log::trace!(
                        "Routing NM_CLICK from ControlID {} to treeview_handler.",
                        control_id_from_notify
                    );
                    treeview_handler::handle_nm_click(self, hwnd_parent_window, window_id, nmhdr);
                    return (None, None);
                }
                TVN_ITEMCHANGEDW => {
                    log::trace!(
                        "Routing TVN_ITEMCHANGEDW from ControlID {} to treeview_handler.",
                        control_id_from_notify
                    );
                    let event = treeview_handler::handle_treeview_itemchanged_notification(
                        self,
                        window_id,
                        lparam_original,
                        control_id_from_notify,
                    );
                    return (event, None);
                }
                _ => {
                    log::trace!(
                        "Unhandled WM_NOTIFY code {} from known TreeView ControlID {}.",
                        nmhdr.code,
                        control_id_from_notify
                    );
                }
            }
        } else {
            log::trace!(
                "WM_NOTIFY code {} from ControlID {} is not identified as a TreeView notification.",
                nmhdr.code,
                control_id_from_notify
            );
        }
        (None, None)
    }

    /*
     * Handles the WM_CREATE message for a window.
     * Minimal responsibilities now, mainly for setting up things like custom fonts
     * if they are window-wide and not control-specific.
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
        if let Ok(mut windows_map_guard) = self.active_windows.write() {
            if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                if window_data.status_bar_font.is_none() {
                    let font_name_hstring = HSTRING::from("Segoe UI");
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
                        -font_point_size // Fallback: direct point size as negative height
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
                            DEFAULT_CHARSET,      // fdwCharSet
                            OUT_DEFAULT_PRECIS,   // fdwOutputPrecision
                            CLIP_DEFAULT_PRECIS,  // fdwClipPrecision
                            DEFAULT_QUALITY,      // fdwQuality
                            FF_DONTCARE.0 as u32, // fdwPitchAndFamily
                            &font_name_hstring,   // lpszFace
                        )
                    };
                    if h_font.is_invalid() {
                        log::error!("Platform: Failed to create status bar font: {:?}", unsafe {
                            GetLastError()
                        });
                        window_data.status_bar_font = None;
                    } else {
                        log::debug!(
                            "Platform: Status bar font created: {:?} for window {:?}",
                            h_font,
                            window_id
                        );
                        window_data.status_bar_font = Some(h_font);
                    }
                }
            }
        }
    }

    /*
     * Applies layout rules recursively for a parent and its children.
     */
    fn apply_layout_rules_for_children(
        self: &Arc<Self>,
        window_id: WindowId,
        parent_id_for_layout: Option<i32>,
        parent_rect: RECT,
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
                    "Layout: HWND for control ID {} not found or invalid.",
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
                        _ => unreachable!(),
                    }
                    let item_width = (item_rect.right - item_rect.left).max(0);
                    let item_height = (item_rect.bottom - item_rect.top).max(0);
                    unsafe {
                        _ = MoveWindow(
                            control_hwnd,
                            item_rect.left,
                            item_rect.top,
                            item_width,
                            item_height,
                            true,
                        );
                    }
                    if all_window_rules
                        .iter()
                        .any(|r_child| r_child.parent_control_id == Some(rule.control_id))
                    {
                        let panel_client_rect = RECT {
                            left: 0,
                            top: 0,
                            right: item_width,
                            bottom: item_height,
                        };
                        self.apply_layout_rules_for_children(
                            window_id,
                            Some(rule.control_id),
                            panel_client_rect,
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
                DockStyle::None => {}
            }
        }
        if !proportional_fill_candidates.is_empty() {
            let total_width_for_proportional =
                (current_available_rect.right - current_available_rect.left).max(0);
            let total_height_for_proportional =
                (current_available_rect.bottom - current_available_rect.top).max(0);
            let total_weight: f32 = proportional_fill_candidates
                .iter()
                .map(|(r, _)| {
                    if let DockStyle::ProportionalFill { weight } = r.dock_style {
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
                        let item_width_allocation =
                            (total_width_for_proportional as f32 * proportion) as i32;
                        let final_x = current_x + rule.margin.3;
                        let final_y = current_available_rect.top + rule.margin.0;
                        let final_width =
                            (item_width_allocation - rule.margin.3 - rule.margin.1).max(0);
                        let final_height =
                            (total_height_for_proportional - rule.margin.0 - rule.margin.2).max(0);
                        unsafe {
                            _ = MoveWindow(
                                control_hwnd,
                                final_x,
                                final_y,
                                final_width,
                                final_height,
                                true,
                            );
                        }
                        current_x += item_width_allocation;
                        if all_window_rules
                            .iter()
                            .any(|r_child| r_child.parent_control_id == Some(rule.control_id))
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
        if let Some((rule, control_hwnd)) = fill_candidates.first() {
            if fill_candidates.len() > 1 {
                log::warn!(
                    "Layout: Multiple Fill controls for parent_id {:?}. Using first (ID {}).",
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
            let fill_width = (fill_rect.right - fill_rect.left).max(0);
            let fill_height = (fill_rect.bottom - fill_rect.top).max(0);
            unsafe {
                _ = MoveWindow(
                    *control_hwnd,
                    fill_rect.left,
                    fill_rect.top,
                    fill_width,
                    fill_height,
                    true,
                );
            }
            if all_window_rules
                .iter()
                .any(|r_child| r_child.parent_control_id == Some(rule.control_id))
            {
                let panel_client_rect_fill = RECT {
                    left: 0,
                    top: 0,
                    right: fill_width,
                    bottom: fill_height,
                };
                self.apply_layout_rules_for_children(
                    window_id,
                    Some(rule.control_id),
                    panel_client_rect_fill,
                    all_window_rules,
                    all_controls_map,
                );
            }
        }
    }

    /*
     * Triggers layout recalculation for the specified window.
     */
    pub(crate) fn trigger_layout_recalculation(self: &Arc<Self>, window_id: WindowId) {
        log::debug!(
            "trigger_layout_recalculation called for WinID {:?}",
            window_id
        );
        let active_windows_guard = match self.active_windows.read() {
            Ok(g) => g,
            Err(e) => {
                log::error!("Failed to get read lock for layout: {:?}", e);
                return;
            }
        };
        let window_data = match active_windows_guard.get(&window_id) {
            Some(d) => d,
            None => {
                log::warn!("WindowData not found for layout: {:?}", window_id);
                return;
            }
        };
        if window_data.this_window_hwnd.is_invalid() {
            log::warn!("HWND invalid for layout: {:?}", window_id);
            return;
        }
        let rules = match window_data.get_layout_rules() {
            Some(r) => r,
            None => {
                log::debug!("No layout rules for WinID {:?}", window_id);
                return;
            }
        };
        if rules.is_empty() {
            log::debug!("Empty layout rules for WinID {:?}", window_id);
            return;
        }
        let mut client_rect = RECT::default();
        if unsafe { GetClientRect(window_data.this_window_hwnd, &mut client_rect) }.is_err() {
            log::error!("GetClientRect failed for layout: {:?}", unsafe {
                GetLastError()
            });
            return;
        }
        log::trace!(
            "Applying layout with client_rect: {:?}, for WinID {:?}",
            client_rect,
            window_id
        );
        self.apply_layout_rules_for_children(
            window_id,
            None,
            client_rect,
            rules,
            &window_data.control_hwnd_map,
        );
    }

    /*
     * Handles WM_SIZE: Triggers layout recalculation.
     */
    fn handle_wm_size(
        self: &Arc<Self>,
        hwnd: HWND,
        _wparam: WPARAM,
        width_height: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let client_width = loword_from_lparam(width_height);
        let client_height = hiword_from_lparam(width_height);
        log::debug!(
            "Platform: WM_SIZE for WinID {:?}, HWND {:?}. Client: {}x{}",
            window_id,
            hwnd,
            client_width,
            client_height
        );
        self.trigger_layout_recalculation(window_id);
        Some(AppEvent::WindowResized {
            window_id,
            width: client_width,
            height: client_height,
        })
    }

    /*
     * Handles WM_COMMAND: Dispatches menu actions or button clicks.
     */
    fn handle_wm_command(
        self: &Arc<Self>,
        _hwnd_parent: HWND,
        wparam: WPARAM,
        control_hwnd: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        let command_id = loword_from_wparam(wparam);
        let notification_code = highord_from_wparam(wparam);
        if control_hwnd.0 == 0 {
            // Menu or accelerator
            if let Ok(windows_guard) = self.active_windows.read() {
                if let Some(window_data) = windows_guard.get(&window_id) {
                    if let Some(action) = window_data.get_menu_action(command_id) {
                        log::debug!(
                            "Menu action {:?} (ID {}) for WinID {:?}.",
                            action,
                            command_id,
                            window_id
                        );
                        return Some(AppEvent::MenuActionClicked { action });
                    } else {
                        log::warn!(
                            "WM_COMMAND (Menu/Accel) for unknown ID {} in WinID {:?}.",
                            command_id,
                            window_id
                        );
                    }
                } else {
                    log::warn!(
                        "WindowData not found for WinID {:?} in WM_COMMAND (Menu/Accel).",
                        window_id
                    );
                }
            } else {
                log::error!("Failed read lock for menu_action_map in WM_COMMAND (Menu/Accel).");
            }
        } else {
            // Control
            let hwnd_control = HWND(control_hwnd.0 as *mut std::ffi::c_void);
            if notification_code == BN_CLICKED as i32 {
                log::debug!(
                    "Button ID {} clicked (HWND {:?}) for WinID {:?}.",
                    command_id,
                    hwnd_control,
                    window_id
                );
                return Some(AppEvent::ButtonClicked {
                    window_id,
                    control_id: command_id,
                });
            } else if notification_code == EN_CHANGE as i32 {
                log::trace!(
                    "Edit control ID {} changed, starting debounce timer",
                    command_id
                );
                unsafe {
                    SetTimer(
                        Some(_hwnd_parent),
                        command_id as usize,
                        FILTER_DEBOUNCE_MS,
                        None,
                    );
                }
            } else {
                log::trace!(
                    "Unhandled WM_COMMAND from control: ID {}, NotifyCode {}, HWND {:?}, WinID {:?}",
                    command_id,
                    notification_code,
                    hwnd_control,
                    window_id
                );
            }
        }
        None
    }

    fn handle_wm_timer(
        self: &Arc<Self>,
        hwnd: HWND,
        timer_id: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        unsafe {
            _ = KillTimer(Some(hwnd), timer_id.0 as usize);
        }
        let control_id = timer_id.0 as i32;
        let hwnd_edit_opt = self.active_windows.read().ok().and_then(|g| {
            g.get(&window_id)
                .and_then(|wd| wd.get_control_hwnd(control_id))
        });
        if let Some(hwnd_edit) = hwnd_edit_opt {
            let mut buf: [u16; 256] = [0; 256];
            let len = unsafe { GetWindowTextW(hwnd_edit, &mut buf) } as usize;
            let text = String::from_utf16_lossy(&buf[..len]);
            return Some(AppEvent::FilterTextSubmitted { window_id, text });
        }
        None
    }

    /*
     * Handles WM_DESTROY: Cleans up resources and removes window data.
     */
    fn handle_wm_destroy(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        log::debug!(
            "WM_DESTROY for WinID {:?}. Removing data and cleaning resources.",
            window_id
        );
        if let Ok(mut windows_map_guard) = self.active_windows.write() {
            if let Some(mut window_data_entry) = windows_map_guard.remove(&window_id) {
                if let Some(h_font) = window_data_entry.status_bar_font.take() {
                    if !h_font.is_invalid() {
                        log::debug!(
                            "Deleting status bar font {:?} for WinID {:?}",
                            h_font,
                            window_id
                        );
                        unsafe {
                            _ = DeleteObject(HGDIOBJ(h_font.0 as *mut c_void));
                        }
                    }
                }
                window_data_entry.cleanup_input_background_colors();
                log::debug!("Removed WindowId {:?} from active_windows.", window_id);
            } else {
                log::warn!(
                    "WindowId {:?} not found in active_windows during WM_DESTROY.",
                    window_id
                );
            }
        } else {
            log::error!(
                "Failed write lock for active_windows in WM_DESTROY for WinID {:?}.",
                window_id
            );
        }
        self.check_if_should_quit_after_window_close();
        Some(AppEvent::WindowDestroyed { window_id })
    }

    /*
     * Handles WM_PAINT: Fills background. Control custom drawing is separate.
     */
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
                _ = EndPaint(hwnd, &ps);
            }
        }
        SUCCESS_CODE
    }

    /*
     * Handles WM_GETMINMAXINFO: Sets minimum window tracking size.
     */
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
        SUCCESS_CODE
    }
}

pub(crate) fn set_window_title(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
) -> PlatformResult<()> {
    log::debug!("Setting title for WinID {:?} to '{}'", window_id, title);
    let windows_guard = internal_state.active_windows.read().map_err(|_| {
        PlatformError::OperationFailed("Failed read lock for set_window_title".into())
    })?;
    let window_data = windows_guard.get(&window_id).ok_or_else(|| {
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for set_window_title",
            window_id
        ))
    })?;
    if window_data.this_window_hwnd.is_invalid() {
        return Err(PlatformError::InvalidHandle(format!(
            "HWND for WinID {:?} invalid in set_window_title",
            window_id
        )));
    }
    unsafe { SetWindowTextW(window_data.this_window_hwnd, &HSTRING::from(title))? };
    Ok(())
}

pub(crate) fn show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    log::debug!("Setting visibility for WinID {:?} to {}", window_id, show);
    let windows_guard = internal_state
        .active_windows
        .read()
        .map_err(|_| PlatformError::OperationFailed("Failed read lock for show_window".into()))?;
    let window_data = windows_guard.get(&window_id).ok_or_else(|| {
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for show_window",
            window_id
        ))
    })?;
    if window_data.this_window_hwnd.is_invalid() {
        return Err(PlatformError::InvalidHandle(format!(
            "HWND for WinID {:?} invalid in show_window",
            window_id
        )));
    }
    let cmd = if show { SW_SHOW } else { SW_HIDE };
    unsafe { _ = ShowWindow(window_data.this_window_hwnd, cmd) };
    Ok(())
}

/*
 * Initiates the closing of a specified window by calling DestroyWindow directly.
 * The actual destruction sequence (WM_DESTROY, etc.) will follow.
 */
pub(crate) fn send_close_message(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    log::debug!(
        "Platform: send_close_message received for WindowId {:?}, attempting to destroy native window directly.",
        window_id
    );
    // This function will get the HWND and call DestroyWindow.
    // If successful, WM_DESTROY will be posted to the window's queue,
    // and our handle_wm_destroy will eventually be called.
    destroy_native_window(internal_state, window_id)
}

/*
 * Attempts to destroy the native window associated with the given `WindowId`.
 * This is called by `send_close_message` or can be used for more direct cleanup.
 */
pub(crate) fn destroy_native_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    log::debug!(
        "Attempting to destroy native window for WinID {:?}",
        window_id
    );
    let hwnd_to_destroy: Option<HWND>;
    {
        let windows_read_guard = internal_state.active_windows.read().map_err(|e| {
            PlatformError::OperationFailed(format!(
                "Failed read lock (destroy_native_window): {}",
                e
            ))
        })?;
        hwnd_to_destroy = windows_read_guard
            .get(&window_id)
            .map(|data| data.this_window_hwnd);
    }

    if let Some(hwnd) = hwnd_to_destroy {
        if !hwnd.is_invalid() {
            log::debug!(
                "Calling DestroyWindow for HWND {:?} (WinID {:?})",
                hwnd,
                window_id
            );
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let last_error = GetLastError();
                    if last_error.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        log::error!("DestroyWindow for HWND {:?} failed: {:?}", hwnd, last_error);
                        // Optionally return error: PlatformError::OperationFailed(format!("DestroyWindow failed: {:?}", last_error))
                    } else {
                        log::debug!(
                            "DestroyWindow for HWND {:?} reported invalid handle (already destroyed?).",
                            hwnd
                        );
                    }
                } else {
                    log::debug!(
                        "DestroyWindow call initiated for HWND {:?}. WM_DESTROY will follow.",
                        hwnd
                    );
                }
            }
        } else {
            log::warn!(
                "HWND for WinID {:?} was invalid before DestroyWindow call.",
                window_id
            );
        }
    } else {
        log::warn!("WinID {:?} not found for destroy_native_window.", window_id);
    }
    Ok(())
}
