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
    controls::{button_handler, input_handler, label_handler, treeview_handler},
    error::{PlatformError, Result as PlatformResult},
    types::{AppEvent, DockStyle, LayoutRule, MenuAction, MessageSeverity, WindowId},
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
use std::sync::Arc;

// TOOD: Control IDs used by dialog_handler, kept here for visibility if dialog_handler needs them
// but ideally, they should be private to dialog_handler or within a shared constants scope for dialogs.
pub(crate) const ID_DIALOG_INPUT_EDIT: i32 = 3001;
pub(crate) const ID_DIALOG_INPUT_PROMPT_STATIC: i32 = 3002;

// Common control class names
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
/// Default debounce delay for edit controls in milliseconds.
pub const INPUT_DEBOUNCE_MS: u32 = 300;

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
    input_bg_colors: HashMap<i32, crate::platform_layer::controls::input_handler::InputColorState>,
    status_bar_font: Option<HFONT>,
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

    /*
     * Pure layout calculation for a group of child controls. Returns the
     * rectangle for each control without calling any Win32 APIs. The
     * algorithm mirrors the runtime layout engine and is recursively
     * applied by `apply_layout_rules_for_children`.
     */
    fn calculate_layout(parent_rect: RECT, rules: &[LayoutRule]) -> HashMap<i32, RECT> {
        let mut sorted = rules.to_vec();
        sorted.sort_by_key(|r| r.order);

        let mut result = HashMap::new();
        let mut current_available_rect = parent_rect;
        let mut fill_candidate: Option<&LayoutRule> = None;
        let mut proportional_fill_candidates: Vec<&LayoutRule> = Vec::new();

        for rule in &sorted {
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
                    result.insert(rule.control_id, item_rect);
                }
                DockStyle::Fill => {
                    if fill_candidate.is_none() {
                        fill_candidate = Some(rule);
                    }
                }
                DockStyle::ProportionalFill { .. } => {
                    proportional_fill_candidates.push(rule);
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
                .map(|r| match r.dock_style {
                    DockStyle::ProportionalFill { weight } => weight,
                    _ => 0.0,
                })
                .sum();
            if total_weight > 0.0 {
                let mut current_x = current_available_rect.left;
                for rule in proportional_fill_candidates {
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
                        result.insert(
                            rule.control_id,
                            RECT {
                                left: final_x,
                                top: final_y,
                                right: final_x + final_width,
                                bottom: final_y + final_height,
                            },
                        );
                        current_x += item_width_allocation;
                    }
                }
            }
        }

        if let Some(rule) = fill_candidate {
            let fill_rect = RECT {
                left: current_available_rect.left + rule.margin.3,
                top: current_available_rect.top + rule.margin.0,
                right: current_available_rect.right - rule.margin.1,
                bottom: current_available_rect.bottom - rule.margin.2,
            };
            result.insert(rule.control_id, fill_rect);
        }

        result
    }

    /*
     * Applies layout rules recursively for a parent and its children.
     * The heavy lifting is done by `calculate_layout`, which returns the
     * desired rectangles for each child. This function merely calls the
     * Win32 API to move the windows and recurses for nested containers.
     */
    fn apply_layout_rules_for_children(
        &self,
        parent_id_for_layout: Option<i32>,
        parent_rect: RECT,
    ) {
        log::trace!(
            "Applying layout for parent_id {:?}, rect: {:?}",
            parent_id_for_layout,
            parent_rect
        );

        let all_window_rules = match &self.layout_rules {
            Some(rules) => rules,
            None => return, // No rules to apply
        };

        let mut child_rules: Vec<LayoutRule> = all_window_rules
            .iter()
            .filter(|r| r.parent_control_id == parent_id_for_layout)
            .cloned()
            .collect();
        if child_rules.is_empty() {
            return;
        }
        child_rules.sort_by_key(|r| r.order);

        if child_rules
            .iter()
            .filter(|r| r.dock_style == DockStyle::Fill)
            .count()
            > 1
        {
            log::warn!(
                "Layout: Multiple Fill controls for parent_id {:?}. Using first.",
                parent_id_for_layout
            );
        }

        let layout_map = NativeWindowData::calculate_layout(parent_rect, &child_rules);

        for rule in &child_rules {
            let rect = match layout_map.get(&rule.control_id) {
                Some(r) => r,
                None => continue,
            };
            let control_hwnd_opt = self.control_hwnd_map.get(&rule.control_id).copied();
            if control_hwnd_opt.is_none() || control_hwnd_opt == Some(HWND_INVALID) {
                log::warn!(
                    "Layout: HWND for control ID {} not found or invalid.",
                    rule.control_id
                );
                continue;
            }
            let hwnd = control_hwnd_opt.unwrap();
            let width = (rect.right - rect.left).max(0);
            let height = (rect.bottom - rect.top).max(0);
            unsafe {
                _ = MoveWindow(hwnd, rect.left, rect.top, width, height, true);
            }
            if all_window_rules
                .iter()
                .any(|r_child| r_child.parent_control_id == Some(rule.control_id))
            {
                let panel_client_rect = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };
                self.apply_layout_rules_for_children(Some(rule.control_id), panel_client_rect);
            }
        }
    }

    pub(crate) fn get_menu_action(&self, menu_id: i32) -> Option<MenuAction> {
        self.menu_action_map.get(&menu_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn iter_menu_actions(&self) -> impl Iterator<Item = (&i32, &MenuAction)> {
        self.menu_action_map.iter()
    }

    #[cfg(test)]
    pub(crate) fn menu_action_count(&self) -> usize {
        self.menu_action_map.len()
    }

    #[cfg(test)]
    pub(crate) fn get_next_menu_item_id_counter(&self) -> i32 {
        self.next_menu_item_id_counter
    }

    pub(crate) fn define_layout(&mut self, rules: Vec<LayoutRule>) {
        self.layout_rules = Some(rules);
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
        use windows::Win32::Foundation::{COLORREF, GetLastError};
        use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject};

        if let Some(existing) = self.input_bg_colors.remove(&control_id) {
            unsafe {
                let _ = DeleteObject(existing.brush.into());
            }
        }

        if let Some(c) = color {
            let colorref = COLORREF(c);
            let brush = unsafe { CreateSolidBrush(colorref) };
            if brush.is_invalid() {
                return Err(
                    crate::platform_layer::error::PlatformError::OperationFailed(format!(
                        "CreateSolidBrush failed: {:?}",
                        unsafe { GetLastError() }
                    )),
                );
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

    pub(crate) fn ensure_status_bar_font(&mut self) {
        if self.status_bar_font.is_some() {
            return;
        }

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
            -font_point_size
        };

        let h_font = unsafe {
            CreateFontW(
                logical_font_height,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                FF_DONTCARE.0 as u32,
                &font_name_hstring,
            )
        };

        if h_font.is_invalid() {
            log::error!("Platform: Failed to create status bar font: {:?}", unsafe {
                GetLastError()
            });
            self.status_bar_font = None;
        } else {
            log::debug!(
                "Platform: Status bar font created: {:?} for window {:?}",
                h_font,
                self.logical_window_id
            );
            self.status_bar_font = Some(h_font);
        }
    }

    pub(crate) fn get_status_bar_font(&self) -> Option<HFONT> {
        self.status_bar_font
    }

    pub(crate) fn cleanup_status_bar_font(&mut self) {
        if let Some(h_font) = self.status_bar_font.take() {
            if !h_font.is_invalid() {
                log::debug!(
                    "Deleting status bar font {:?} for WinID {:?}",
                    h_font,
                    self.logical_window_id
                );
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(h_font.0));
                }
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
        internal_state.app_name_for_class()
    ));
    let class_name_pcwstr = PCWSTR(class_name_hstring.as_ptr());

    unsafe {
        let mut wc_test = WNDCLASSEXW::default();
        if GetClassInfoExW(
            Some(internal_state.h_instance()),
            class_name_pcwstr,
            &mut wc_test,
        )
        .is_ok()
        {
            log::debug!(
                "Platform: Window class '{}' already registered.",
                internal_state.app_name_for_class()
            );
            return Ok(());
        }

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(facade_wnd_proc_router),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: internal_state.h_instance(),
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
                internal_state.app_name_for_class()
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
        internal_state_arc.app_name_for_class()
    ));

    let creation_context = Box::new(WindowCreationContext {
        internal_state_arc: Arc::clone(internal_state_arc),
        window_id,
    });

    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),            // Optional extended window styles
            &class_name_hstring,                   // Window class name
            &HSTRING::from(title),                 // Window title
            WS_OVERLAPPEDWINDOW,                   // Common window style
            CW_USEDEFAULT,                         // Default X position
            CW_USEDEFAULT,                         // Default Y position
            width,                                 // Width
            height,                                // Height
            None,                                  // Parent window (None for top-level)
            None,                                  // Menu (None for no default menu)
            Some(internal_state_arc.h_instance()), // Application instance
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
                (event_to_send, lresult_override) =
                    self._handle_wm_notify_dispatch(hwnd, wparam, lparam, window_id);
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
            self.send_event(event);
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
    ) -> (Option<AppEvent>, Option<LRESULT>) {
        let nmhdr_ptr = lparam_original.0 as *const NMHDR;
        if nmhdr_ptr.is_null() {
            log::warn!("WM_NOTIFY received with null NMHDR pointer. Ignoring.");
            return (None, None);
        }
        let nmhdr = unsafe { &*nmhdr_ptr };
        let control_id_from_notify = nmhdr.idFrom as i32;

        let is_treeview_notification = self.with_window_data_read(window_id, |window_data| {
            Ok(window_data.has_treeview_state()
                && window_data.get_control_hwnd(control_id_from_notify) == Some(nmhdr.hwndFrom))
        });

        if let Ok(true) = is_treeview_notification {
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
        } else if is_treeview_notification.is_err() {
            log::error!(
                "Failed to access window data for WM_NOTIFY in WinID {:?}: {:?}",
                window_id,
                is_treeview_notification.unwrap_err()
            );
        }
        (None, None)
    }

    /*
     * Handles the WM_CREATE message for a window.
     * Ensures window-wide resources like custom fonts are created.
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
        if let Err(e) = self.with_window_data_write(window_id, |window_data| {
            window_data.ensure_status_bar_font();
            Ok(())
        }) {
            log::error!(
                "Failed to access window data during WM_CREATE for WinID {:?}: {:?}",
                window_id,
                e
            );
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

        if let Err(e) = self.with_window_data_read(window_id, |window_data| {
            if window_data.get_hwnd().is_invalid() {
                log::warn!("HWND invalid for layout: {:?}", window_id);
                return Ok(()); // Not an error, just can't do anything.
            }
            if window_data.layout_rules.is_none() {
                log::debug!("No layout rules for WinID {:?}", window_id);
                return Ok(());
            }

            let mut client_rect = RECT::default();
            if unsafe { GetClientRect(window_data.get_hwnd(), &mut client_rect) }.is_err() {
                log::error!("GetClientRect failed for layout: {:?}", unsafe {
                    GetLastError()
                });
                return Ok(());
            }

            log::trace!(
                "Applying layout with client_rect: {:?}, for WinID {:?}",
                client_rect,
                window_id
            );
            window_data.apply_layout_rules_for_children(None, client_rect);
            Ok(())
        }) {
            log::error!(
                "Failed to access window data for layout recalculation of WinID {:?}: {:?}",
                window_id,
                e
            );
        }
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
            return super::controls::menu_handler::handle_wm_command_for_menu(
                window_id,
                command_id,
                _hwnd_parent,
                self,
            );
        } else {
            // Control notification
            let hwnd_control = HWND(control_hwnd.0 as *mut std::ffi::c_void);
            if notification_code == BN_CLICKED as i32 {
                return Some(button_handler::handle_bn_clicked(
                    window_id,
                    command_id,
                    hwnd_control,
                ));
            } else if notification_code == EN_CHANGE as i32 {
                log::trace!(
                    "Edit control ID {} changed, starting debounce timer",
                    command_id
                );
                unsafe {
                    SetTimer(
                        Some(_hwnd_parent),
                        command_id as usize,
                        INPUT_DEBOUNCE_MS,
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
            _ = KillTimer(Some(hwnd), timer_id.0);
        }
        let control_id = timer_id.0 as i32;

        let hwnd_edit_result = self.with_window_data_read(window_id, |window_data| {
            window_data.get_control_hwnd(control_id).ok_or_else(|| {
                log::warn!("Control not found for timer ID {}", control_id);
                PlatformError::InvalidHandle("Control not found for timer".into())
            })
        });

        if let Ok(hwnd_edit) = hwnd_edit_result {
            let mut buf: [u16; 256] = [0; 256];
            let len = unsafe { GetWindowTextW(hwnd_edit, &mut buf) } as usize;
            let text = String::from_utf16_lossy(&buf[..len]);
            return Some(AppEvent::InputTextChanged {
                window_id,
                control_id,
                text,
            });
        }
        None
    }

    /*
     * Handles WM_DESTROY: Delegates to a helper for resource cleanup and data
     * removal, checks if the application should quit, and generates the final
     * WindowDestroyed event.
     */
    fn handle_wm_destroy(
        self: &Arc<Self>,
        _hwnd: HWND,
        _wparam: WPARAM,
        _lparam: LPARAM,
        window_id: WindowId,
    ) -> Option<AppEvent> {
        log::debug!(
            "WM_DESTROY received for WinID {:?}. Initiating cleanup.",
            window_id
        );

        // Delegate the complex task of locking, removing, and cleaning up GDI
        // resources to the dedicated helper method. This keeps the window
        // procedure clean and focused on message flow.
        self.remove_window_data(window_id);

        // After removing the window, check if it was the last one.
        self.check_if_should_quit_after_window_close();

        // Notify the application logic that the window is gone.
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
    internal_state.with_window_data_read(window_id, |window_data| {
        let hwnd = window_data.get_hwnd();
        if hwnd.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "HWND for WinID {:?} is invalid in set_window_title",
                window_id
            )));
        }
        unsafe { SetWindowTextW(hwnd, &HSTRING::from(title))? };
        Ok(())
    })
}

pub(crate) fn show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    log::debug!("Setting visibility for WinID {:?} to {}", window_id, show);
    internal_state.with_window_data_read(window_id, |window_data| {
        let hwnd = window_data.get_hwnd();
        if hwnd.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "HWND for WinID {:?} is invalid in show_window",
                window_id
            )));
        }
        let cmd = if show { SW_SHOW } else { SW_HIDE };
        unsafe { _ = ShowWindow(hwnd, cmd) };
        Ok(())
    })
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

    let hwnd_to_destroy =
        internal_state.with_window_data_read(window_id, |window_data| Ok(window_data.get_hwnd()));

    match hwnd_to_destroy {
        Ok(hwnd) if !hwnd.is_invalid() => {
            log::debug!(
                "Calling DestroyWindow for HWND {:?} (WinID {:?})",
                hwnd,
                window_id
            );
            unsafe {
                if DestroyWindow(hwnd).is_err() {
                    let last_error = GetLastError();
                    // Don't error out if the handle is already invalid (e.g., already destroyed).
                    if last_error.0 != ERROR_INVALID_WINDOW_HANDLE.0 {
                        log::error!("DestroyWindow for HWND {:?} failed: {:?}", hwnd, last_error);
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
        }
        Ok(_) => {
            // HWND is invalid
            log::warn!(
                "HWND for WinID {:?} was invalid before DestroyWindow call.",
                window_id
            );
        }
        Err(_) => {
            // WindowId not found
            log::warn!("WinID {:?} not found for destroy_native_window.", window_id);
        }
    };
    // This function's purpose is to *try* to destroy, so don't bubble up "not found" as an error.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::HWND;

    /*
     * Unit tests for NativeWindowData. These tests verify basic state
     * management without invoking Win32 APIs, using dummy HWND values.
     */

    #[test]
    fn test_register_control_hwnd_lookup() {
        // Arrange
        let mut data = NativeWindowData::new(WindowId(1));
        let hwnd = HWND(0x1234 as *mut std::ffi::c_void);
        // Act
        data.register_control_hwnd(42, hwnd);
        // Assert
        assert_eq!(data.get_control_hwnd(42), Some(hwnd));
        assert!(data.has_control(42));
    }

    #[test]
    fn test_register_menu_action_increments_counter() {
        // Arrange
        let mut data = NativeWindowData::new(WindowId(2));
        let start = data.get_next_menu_item_id_counter();
        // Act
        let id1 = data.register_menu_action(MenuAction::RefreshFileList);
        let id2 = data.register_menu_action(MenuAction::RefreshFileList);
        // Assert
        assert_eq!(data.menu_action_count(), 2);
        assert_eq!(id1, start);
        assert_eq!(id2, start + 1);
        assert_eq!(data.get_next_menu_item_id_counter(), start + 2);
        assert_eq!(data.get_menu_action(id1), Some(MenuAction::RefreshFileList));
    }

    #[test]
    fn test_set_and_get_label_severity() {
        // Arrange
        let mut data = NativeWindowData::new(WindowId(3));
        // Act
        data.set_label_severity(7, MessageSeverity::Warning);
        // Assert
        assert_eq!(data.get_label_severity(7), Some(MessageSeverity::Warning));
    }

    #[test]
    fn test_set_input_background_color_none() {
        // Arrange
        let mut data = NativeWindowData::new(WindowId(4));
        // Act
        let result = data.set_input_background_color(5, None);
        // Assert
        assert!(result.is_ok());
        assert!(data.get_input_background_color(5).is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_set_input_background_color_some() {
        // Arrange
        let mut data = NativeWindowData::new(WindowId(5));
        // Act
        let result = data.set_input_background_color(6, Some(0x00FF00));
        // Assert
        assert!(result.is_ok());
        let state = data.get_input_background_color(6).expect("color state");
        assert_eq!(state.color.0, 0x00FF00);
        assert!(!state.brush.is_invalid());
    }

    /*
     * Unit tests for the pure layout calculation. These tests ensure the
     * geometry is computed correctly without creating any native windows.
     */

    #[test]
    fn test_calculate_layout_top_and_fill() {
        // Arrange
        let rules = vec![
            LayoutRule {
                control_id: 1,
                parent_control_id: None,
                dock_style: DockStyle::Top,
                order: 0,
                fixed_size: Some(20),
                margin: (0, 0, 0, 0),
            },
            LayoutRule {
                control_id: 2,
                parent_control_id: None,
                dock_style: DockStyle::Fill,
                order: 1,
                fixed_size: None,
                margin: (0, 0, 0, 0),
            },
        ];
        let parent_rect = RECT {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        // Act
        let map = NativeWindowData::calculate_layout(parent_rect, &rules);
        // Assert
        assert_eq!(map.get(&1).unwrap().bottom, 20);
        assert_eq!(map.get(&2).unwrap().top, 20);
        assert_eq!(map.get(&2).unwrap().bottom, 100);
    }

    #[test]
    fn test_calculate_layout_proportional_fill() {
        // Arrange
        let rules = vec![
            LayoutRule {
                control_id: 1,
                parent_control_id: None,
                dock_style: DockStyle::ProportionalFill { weight: 1.0 },
                order: 0,
                fixed_size: None,
                margin: (0, 0, 0, 0),
            },
            LayoutRule {
                control_id: 2,
                parent_control_id: None,
                dock_style: DockStyle::ProportionalFill { weight: 2.0 },
                order: 1,
                fixed_size: None,
                margin: (0, 0, 0, 0),
            },
        ];
        let parent_rect = RECT {
            left: 0,
            top: 0,
            right: 100,
            bottom: 20,
        };
        // Act
        let map = NativeWindowData::calculate_layout(parent_rect, &rules);
        // Assert
        let rect1 = map.get(&1).unwrap();
        let rect2 = map.get(&2).unwrap();
        assert_eq!(rect1.right - rect1.left, 33);
        assert_eq!(rect2.left, 33);
        assert_eq!(rect2.right - rect2.left, 66);
    }

    #[test]
    fn test_calculate_layout_nested_panels() {
        // Arrange
        let outer_rule = LayoutRule {
            control_id: 1,
            parent_control_id: None,
            dock_style: DockStyle::Fill,
            order: 0,
            fixed_size: None,
            margin: (0, 0, 0, 0),
        };
        let inner_rules = vec![
            LayoutRule {
                control_id: 2,
                parent_control_id: Some(1),
                dock_style: DockStyle::Top,
                order: 0,
                fixed_size: Some(10),
                margin: (0, 0, 0, 0),
            },
            LayoutRule {
                control_id: 3,
                parent_control_id: Some(1),
                dock_style: DockStyle::Fill,
                order: 1,
                fixed_size: None,
                margin: (0, 0, 0, 0),
            },
        ];
        let parent_rect = RECT {
            left: 0,
            top: 0,
            right: 50,
            bottom: 50,
        };
        // Act
        let outer_map = NativeWindowData::calculate_layout(parent_rect, &[outer_rule.clone()]);
        let outer_rect = outer_map.get(&1).unwrap();
        let inner_map = NativeWindowData::calculate_layout(
            RECT {
                left: 0,
                top: 0,
                right: outer_rect.right - outer_rect.left,
                bottom: outer_rect.bottom - outer_rect.top,
            },
            &inner_rules,
        );
        // Assert
        assert_eq!(outer_rect.right - outer_rect.left, 50);
        assert_eq!(inner_map.get(&2).unwrap().bottom, 10);
        assert_eq!(inner_map.get(&3).unwrap().top, 10);
    }
}
