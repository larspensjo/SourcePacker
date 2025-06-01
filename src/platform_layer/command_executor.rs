/*
 * This module is responsible for executing specific `PlatformCommand`s.
 * It contains functions that take the necessary state (like `Win32ApiInternalState`)
 * and command-specific parameters to perform the requested platform operations.
 * This helps to decouple the command execution logic from the main `app.rs` module.
 */

use super::app::Win32ApiInternalState;
use super::control_treeview; // For control_treeview operations
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{
    AppEvent, CheckState, LayoutRule, MenuAction, MenuItemConfig, MessageSeverity, TreeItemId,
    WindowId,
};
use super::window_common; // For window_common operations like set_window_title

use crate::platform_layer::window_common::{
    BUTTON_AREA_HEIGHT, ID_STATUS_BAR_CTRL, SS_LEFT, WC_BUTTON, WC_STATIC,
};
use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND},
        Graphics::Gdi::InvalidateRect,
        UI::{
            Controls::{
                TVS_CHECKBOXES, TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS,
                WC_TREEVIEWW,
            },
            Input::KeyboardAndMouse::EnableWindow,
            WindowsAndMessaging::{
                AppendMenuW, BS_PUSHBUTTON, CreateMenu, CreatePopupMenu, CreateWindowExW,
                DestroyMenu, GetDlgCtrlID, HMENU, MF_POPUP, MF_STRING, PostQuitMessage, SetMenu,
                SetWindowTextW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_BORDER, WS_CHILD, WS_VISIBLE,
            },
        },
    },
    core::HSTRING,
};

/*
 * Executes the `DefineLayout` command.
 * This function retrieves the `NativeWindowData` for the given `window_id`
 * and stores the provided `layout_rules` within it. These rules will later
 * be used by the `WM_SIZE` handler to position controls.
 */
pub(crate) fn execute_define_layout(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    rules: Vec<LayoutRule>,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_define_layout for WinID {:?}, with {} rules.",
        window_id,
        rules.len()
    );

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed(
            "Failed to lock windows map for execute_define_layout".into(),
        )
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        window_data.layout_rules = Some(rules);
        log::debug!(
            "CommandExecutor: Stored layout rules for WinID {:?}",
            window_id
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for execute_define_layout",
            window_id
        )))
    }
}

/*
 * Executes the `UpdateStatusBarText` command.
 * It updates the stored text and severity in `NativeWindowData` and then
 * calls the WinAPI functions to update the visual appearance of the status bar.
 * The lock on `window_map` is released before making WinAPI calls that might
 * trigger synchronous messages (like WM_CTLCOLORSTATIC) to prevent deadlocks.
 */
pub(crate) fn execute_update_status_bar_text(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    text: String,
    severity: MessageSeverity,
) -> PlatformResult<()> {
    // Logging of the status message content is now centralized here.
    match severity {
        MessageSeverity::Error => {
            log::error!("Platform Status (WinID {:?} ERROR): {}", window_id, text)
        }
        MessageSeverity::Warning => {
            log::warn!("Platform Status (WinID {:?} WARN):  {}", window_id, text)
        }
        MessageSeverity::Information => {
            log::info!("Platform Status (WinID {:?} INFO): {}", window_id, text)
        }
        MessageSeverity::Debug => {
            log::debug!("Platform Status (WinID {:?} DEBUG): {}", window_id, text)
        }
        MessageSeverity::None => {
            log::debug!("Platform Status (WinID {:?} CLEAR)", window_id)
        }
    }

    let mut text_to_set_on_control: Option<String> = None;
    let mut hwnd_status_bar_for_api_call: Option<HWND> = None;

    // Scope for the write lock on window_map
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
            PlatformError::OperationFailed("Failed to lock windows map for status update".into())
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            if severity == MessageSeverity::None {
                window_data.status_bar_current_text.clear();
                window_data.status_bar_current_severity = MessageSeverity::None;
                text_to_set_on_control = Some("".to_string());
            } else if severity >= window_data.status_bar_current_severity {
                window_data.status_bar_current_text = text.clone();
                window_data.status_bar_current_severity = severity;
                text_to_set_on_control = Some(text.clone());
            } else {
                log::debug!(
                    "Platform Status (WinID {:?} IGNORED_LOWER_SEVERITY_UI_UPDATE): current severity {:?}, incoming {:?})",
                    window_id,
                    window_data.status_bar_current_severity,
                    severity
                );
                return Ok(());
            }
            hwnd_status_bar_for_api_call = window_data.get_control_hwnd(ID_STATUS_BAR_CTRL);
        } else {
            return Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for status bar update",
                window_id
            )));
        }
    } // Write lock released

    if let Some(hwnd_status) = hwnd_status_bar_for_api_call {
        if let Some(final_text) = text_to_set_on_control {
            unsafe {
                if SetWindowTextW(hwnd_status, &HSTRING::from(final_text)).is_err() {
                    return Err(PlatformError::OperationFailed(format!(
                        "SetWindowTextW for status bar failed: {:?}",
                        GetLastError()
                    )));
                }
                InvalidateRect(Some(hwnd_status), None, true);
            }
            Ok(())
        } else {
            Ok(())
        }
    } else {
        log::error!(
            "Platform Warning: Status bar HWND (ID {}) not found for WindowId {:?} during UI update. Text was: '{}'",
            ID_STATUS_BAR_CTRL,
            window_id,
            text,
        );
        Ok(())
    }
}

/*
 * Executes the `QuitApplication` command.
 * Posts a `WM_QUIT` message to the application's message queue, which will
 * eventually cause the main event loop in `PlatformInterface::run` to terminate.
 */
pub(crate) fn execute_quit_application(
    _internal_state: &Arc<Win32ApiInternalState>, // Not strictly needed but passed for consistency
) -> PlatformResult<()> {
    log::debug!("CommandExecutor: execute_quit_application. Posting WM_QUIT.");
    unsafe { PostQuitMessage(0) };
    Ok(())
}

/*
 * Executes the `SignalMainWindowUISetupComplete` command.
 * Retrieves the application's event handler and sends it an
 * `AppEvent::MainWindowUISetupComplete` to signal that `MyAppLogic` can proceed
 * with its data-dependent UI initialization.
 */
pub(crate) fn execute_signal_main_window_ui_setup_complete(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_signal_main_window_ui_setup_complete for window_id: {:?}",
        window_id
    );

    let handler_arc_opt = {
        let event_handler_guard =
            internal_state
                .application_event_handler
                .lock()
                .map_err(|_e| {
                    PlatformError::OperationFailed(
                        "Failed to lock internal event_handler field".into(),
                    )
                })?;
        event_handler_guard
            .as_ref()
            .and_then(|weak_handler| weak_handler.upgrade())
    };

    if let Some(handler_arc) = handler_arc_opt {
        let mut handler_guard = handler_arc.lock().map_err(|_e| {
            PlatformError::OperationFailed(
                "Failed to lock app event handler for MainWindowUISetupComplete".into(),
            )
        })?;
        handler_guard.handle_event(AppEvent::MainWindowUISetupComplete { window_id });
        Ok(())
    } else {
        log::error!(
            "CommandExecutor: Event handler not available to send MainWindowUISetupComplete event."
        );
        Err(PlatformError::OperationFailed(
            "Event handler (MyAppLogic) not available for MainWindowUISetupComplete.".into(),
        ))
    }
}

/*
 * Executes the `CreateMainMenu` command.
 * Creates a native menu structure based on `menu_items` and associates it
 * with the specified window. Menu item actions are mapped to generated IDs.
 */
pub(crate) fn execute_create_main_menu(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    menu_items: Vec<MenuItemConfig>,
) -> PlatformResult<()> {
    let h_main_menu = unsafe { CreateMenu()? };
    let mut hwnd_owner_opt: Option<HWND> = None;

    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to lock windows map for main menu creation (data population)".into(),
            )
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            hwnd_owner_opt = Some(window_data.hwnd);
            if window_data.hwnd.is_invalid() {
                unsafe {
                    DestroyMenu(h_main_menu).unwrap_or_default();
                }
                return Err(PlatformError::InvalidHandle(format!(
                    "HWND not yet valid for WindowId {:?} during menu data population",
                    window_id
                )));
            }
            for item_config in menu_items {
                unsafe {
                    add_menu_item_recursive_impl(h_main_menu, &item_config, window_data)?;
                }
            }
        } else {
            unsafe {
                DestroyMenu(h_main_menu).unwrap_or_default();
            }
            return Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateMainMenu (data population)",
                window_id
            )));
        }
    }

    if let Some(hwnd_owner) = hwnd_owner_opt {
        if unsafe { SetMenu(hwnd_owner, Some(h_main_menu)) }.is_err() {
            unsafe {
                DestroyMenu(h_main_menu).unwrap_or_default();
            }
            return Err(PlatformError::OperationFailed(format!(
                "SetMenu failed for main menu on WindowId {:?}: {:?}",
                window_id,
                unsafe { GetLastError() }
            )));
        }
        log::debug!(
            "CommandExecutor: Main menu created and set for WindowId {:?}",
            window_id
        );
        Ok(())
    } else {
        unsafe {
            DestroyMenu(h_main_menu).unwrap_or_default();
        }
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} became invalid or HWND was not set before SetMenu",
            window_id
        )))
    }
}

/*
 * Helper function to recursively add menu items.
 * This is an internal implementation detail for `execute_create_main_menu`.
 */
unsafe fn add_menu_item_recursive_impl(
    parent_menu_handle: HMENU,
    item_config: &MenuItemConfig,
    window_data: &mut window_common::NativeWindowData,
) -> PlatformResult<()> {
    if item_config.children.is_empty() {
        if let Some(action) = item_config.action {
            let generated_id = window_data.generate_menu_item_id();
            window_data.menu_action_map.insert(generated_id, action);
            log::debug!(
                "CommandExecutor: Mapping menu action {:?} to ID {} for window {:?}",
                action,
                generated_id,
                window_data.id
            );
            unsafe {
                AppendMenuW(
                    parent_menu_handle,
                    MF_STRING,
                    generated_id as usize,
                    &HSTRING::from(item_config.text.as_str()),
                )?
            };
        } else {
            log::warn!(
                "CommandExecutor: Menu item '{}' has no children and no action. It will be non-functional.",
                item_config.text
            );
        }
    } else {
        let h_submenu = unsafe { CreatePopupMenu()? };
        for child_config in &item_config.children {
            unsafe { add_menu_item_recursive_impl(h_submenu, child_config, window_data)? };
        }
        unsafe {
            AppendMenuW(
                parent_menu_handle,
                MF_POPUP,
                h_submenu.0 as usize,
                &HSTRING::from(item_config.text.as_str()),
            )?
        };
    }
    Ok(())
}

/*
 * Executes the `SetControlEnabled` command.
 * Enables or disables a specific control within a window.
 */
pub(crate) fn execute_set_control_enabled(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    enabled: bool,
) -> PlatformResult<()> {
    let windows_guard = internal_state.active_windows.read().map_err(|_| {
        PlatformError::OperationFailed("Failed to acquire read lock on windows map".into())
    })?;

    if let Some(window_data) = windows_guard.get(&window_id) {
        let hwnd_ctrl = window_data.get_control_hwnd(control_id).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found in window {:?}",
                control_id, window_id
            ))
        })?;
        unsafe {
            EnableWindow(hwnd_ctrl, enabled);
        }
        log::debug!(
            "CommandExecutor: Control ID {} in window {:?} set to enabled: {}",
            control_id,
            window_id,
            enabled
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for SetControlEnabled",
            window_id
        )))
    }
}

/*
 * Executes the `CreateButton` command.
 * Creates a native button control and stores its HWND.
 */
pub(crate) fn execute_create_button(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    text: String,
) -> PlatformResult<()> {
    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to lock windows map for button creation".into())
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        if window_data.controls.contains_key(&control_id) {
            return Err(PlatformError::OperationFailed(format!(
                "Button with ID {} already exists for window {:?}",
                control_id, window_id
            )));
        }
        let hwnd_button = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_BUTTON,
                &HSTRING::from(text.as_str()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                0,
                0,
                10,
                10, // Dummies, WM_SIZE/LayoutRules will adjust
                Some(window_data.hwnd),
                Some(HMENU(control_id as *mut _)),
                Some(internal_state.h_instance),
                None,
            )?
        };
        window_data.controls.insert(control_id, hwnd_button);
        log::debug!(
            "CommandExecutor: Created button '{}' (ID {}) for window {:?} with HWND {:?}",
            text,
            control_id,
            window_id,
            hwnd_button
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateButton",
            window_id
        )))
    }
}

/*
 * Executes the `CreateStatusBar` command.
 * Creates a native status bar control (STATIC) and stores its HWND.
 */
pub(crate) fn execute_create_status_bar(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    initial_text: String,
) -> PlatformResult<()> {
    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to lock windows map for status bar creation".into())
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        if window_data.controls.contains_key(&control_id) {
            return Err(PlatformError::OperationFailed(format!(
                "Status bar with ID {} already present for window {:?}",
                control_id, window_id
            )));
        }
        let hwnd_status_bar = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_STATIC,
                &HSTRING::from(initial_text.as_str()),
                WS_CHILD | WS_VISIBLE | SS_LEFT,
                0,
                0,
                0,
                0, // Dummies, WM_SIZE/LayoutRules will adjust
                Some(window_data.hwnd),
                Some(HMENU(control_id as *mut _)),
                Some(internal_state.h_instance),
                None,
            )?
        };
        window_data.controls.insert(control_id, hwnd_status_bar);
        window_data.status_bar_current_text = initial_text.clone();
        window_data.status_bar_current_severity = MessageSeverity::Information;
        log::debug!(
            "CommandExecutor: Created status bar (ID {}) for window {:?} with HWND {:?}, initial text: '{}'",
            control_id,
            window_id,
            hwnd_status_bar,
            initial_text
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateStatusBar",
            window_id
        )))
    }
}

/*
 * Executes the `CreateTreeView` command.
 * Creates a native TreeView control, stores its HWND, and initializes its internal state.
 */
pub(crate) fn execute_create_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to lock windows map for TreeView creation".into())
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        if window_data.controls.contains_key(&control_id) || window_data.treeview_state.is_some() {
            return Err(PlatformError::ControlCreationFailed(format!(
                "TreeView with ID {} or existing TreeView state already present for window {:?}",
                control_id, window_id
            )));
        }
        let tvs_style = WINDOW_STYLE(
            TVS_HASLINES | TVS_LINESATROOT | TVS_HASBUTTONS | TVS_SHOWSELALWAYS | TVS_CHECKBOXES,
        );
        let combined_style = WS_CHILD | WS_VISIBLE | WS_BORDER | tvs_style;
        let hwnd_tv = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_TREEVIEWW,
                None,
                combined_style,
                0,
                0,
                10,
                10, // Dummies, WM_SIZE/LayoutRules will adjust
                Some(window_data.hwnd),
                Some(HMENU(control_id as *mut _)),
                Some(internal_state.h_instance),
                None,
            )?
        };
        window_data.controls.insert(control_id, hwnd_tv);
        window_data.treeview_state = Some(control_treeview::TreeViewInternalState::new());
        log::debug!(
            "CommandExecutor: Created TreeView (ID {}) for window {:?} with HWND {:?}",
            control_id,
            window_id,
            hwnd_tv
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateTreeView",
            window_id
        )))
    }
}

// Functions for commands that are still handled directly by Win32ApiInternalState
// or will be moved to dialog_handler.rs. These are just stubs for now if they were
// in Win32ApiInternalState, or direct calls if they were simple enough.
// For this step, we are only moving the non-dialog ones listed above.

// Example of a command that would eventually go to dialog_handler.rs
// pub(crate) fn execute_show_save_file_dialog(...) -> PlatformResult<()> { ... }

// Commands that are delegated to other modules (like control_treeview) remain as direct calls
// for now in `_execute_platform_command`.
pub(crate) fn execute_populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    items: Vec<super::types::TreeItemDescriptor>,
) -> PlatformResult<()> {
    control_treeview::populate_treeview(internal_state, window_id, items)
}

pub(crate) fn execute_update_tree_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    item_id: TreeItemId,
    new_state: CheckState,
) -> PlatformResult<()> {
    control_treeview::update_treeview_item_visual_state(
        internal_state,
        window_id,
        item_id,
        new_state,
    )
}

// Commands that call simple window_common functions
pub(crate) fn execute_set_window_title(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
) -> PlatformResult<()> {
    window_common::set_window_title(internal_state, window_id, title)
}

pub(crate) fn execute_show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    window_common::show_window(internal_state, window_id, show)
}

pub(crate) fn execute_close_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    window_common::send_close_message(internal_state, window_id)
}

/*
 * Executes the `CreatePanel` command.
 * Creates a generic STATIC control to act as a panel. The panel can be a child
 * of the main window or another control (identified by `parent_control_id`).
 * The new panel's HWND is stored in `NativeWindowData.controls` mapped by `panel_id`.
 */
pub(crate) fn execute_create_panel(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    parent_control_id: Option<i32>,
    panel_id: i32, // Logical ID for this new panel
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_create_panel for WinID {:?}, PanelID: {}, ParentControlID: {:?}",
        window_id,
        panel_id,
        parent_control_id
    );

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to lock windows map for CreatePanel".into())
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        if window_data.controls.contains_key(&panel_id) {
            return Err(PlatformError::OperationFailed(format!(
                "Panel with logical ID {} already exists for window {:?}",
                panel_id, window_id
            )));
        }

        let hwnd_parent = match parent_control_id {
            Some(id) => window_data.get_control_hwnd(id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "Parent control with logical ID {} not found for CreatePanel in WinID {:?}",
                    id, window_id
                ))
            })?,
            None => window_data.hwnd, // Parent is the main window
        };

        if hwnd_parent.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND for CreatePanel is invalid (WinID: {:?}, ParentControlID: {:?})",
                window_id, parent_control_id
            )));
        }

        let hwnd_panel = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0), // Or WS_EX_CONTROLPARENT if it should manage tab order for children
                WC_STATIC,          // Using a STATIC control as a simple panel container
                None,               // No text for a simple panel
                WS_CHILD | WS_VISIBLE, // Basic styles for a panel
                0,
                0,
                10,
                10, // Dummy position/size, layout rules will adjust
                Some(hwnd_parent),
                Some(HMENU(panel_id as *mut _)), // Use logical ID for the HMENU
                Some(internal_state.h_instance),
                None,
            )?
        };
        window_data.controls.insert(panel_id, hwnd_panel);
        log::debug!(
            "CommandExecutor: Created panel (LogicalID {}) for WinID {:?} with HWND {:?}",
            panel_id,
            window_id,
            hwnd_panel
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreatePanel",
            window_id
        )))
    }
}

/*
 * Executes the `CreateLabel` command.
 * Creates a STATIC control (label) as a child of the specified parent panel.
 * The label's HWND is stored in `NativeWindowData.controls` mapped by `label_id`.
 * Its initial severity is set to Information in `label_severities`.
 */
pub(crate) fn execute_create_label(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    parent_panel_id: i32, // Logical ID of the parent panel
    label_id: i32,        // Logical ID for this new label
    initial_text: String,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_create_label for WinID {:?}, LabelID: {}, ParentPanelID: {}, Text: '{}'",
        window_id,
        label_id,
        parent_panel_id,
        initial_text
    );

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to lock windows map for CreateLabel".into())
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        if window_data.controls.contains_key(&label_id) {
            return Err(PlatformError::OperationFailed(format!(
                "Label with logical ID {} already exists for window {:?}",
                label_id, window_id
            )));
        }

        let hwnd_parent_panel = window_data
            .get_control_hwnd(parent_panel_id)
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "Parent panel with logical ID {} not found for CreateLabel in WinID {:?}",
                    parent_panel_id, window_id
                ))
            })?;

        let hwnd_label = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_STATIC,
                &HSTRING::from(initial_text.as_str()),
                WS_CHILD | WS_VISIBLE | SS_LEFT, // Basic label styles
                0,
                0,
                10,
                10, // Dummy position/size, layout rules will adjust
                Some(hwnd_parent_panel),
                Some(HMENU(label_id as *mut _)), // Use logical ID for the HMENU
                Some(internal_state.h_instance),
                None,
            )?
        };
        window_data.controls.insert(label_id, hwnd_label);
        window_data
            .label_severities
            .insert(label_id, MessageSeverity::Information); // Default to Information
        log::debug!(
            "CommandExecutor: Created label '{}' (LogicalID {}) for WinID {:?} with HWND {:?}",
            initial_text,
            label_id,
            window_id,
            hwnd_label
        );
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateLabel",
            window_id
        )))
    }
}

/*
 * Executes the `UpdateLabelText` command.
 * Updates the text and severity of a generic label control.
 * The label is identified by its logical `label_id`.
 */
pub(crate) fn execute_update_label_text(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    label_id: i32, // Logical ID of the label
    text: String,
    severity: MessageSeverity,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_update_label_text for WinID {:?}, LabelID: {}, Text: '{}', Severity: {:?}",
        window_id,
        label_id,
        text,
        severity
    );

    let hwnd_label_for_api_call: Option<HWND>;

    // Scope for the write lock on window_map to update label_severities
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
            PlatformError::OperationFailed("Failed to lock windows map for UpdateLabelText".into())
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            hwnd_label_for_api_call = window_data.get_control_hwnd(label_id);
            if hwnd_label_for_api_call.is_none() {
                return Err(PlatformError::InvalidHandle(format!(
                    "Label with logical ID {} not found for UpdateLabelText in WinID {:?}",
                    label_id, window_id
                )));
            }
            window_data.label_severities.insert(label_id, severity);
        } else {
            return Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for UpdateLabelText",
                window_id
            )));
        }
    } // Write lock released

    // Now make WinAPI calls without holding the lock
    if let Some(hwnd_label) = hwnd_label_for_api_call {
        unsafe {
            if SetWindowTextW(hwnd_label, &HSTRING::from(text)).is_err() {
                return Err(PlatformError::OperationFailed(format!(
                    "SetWindowTextW for label ID {} failed: {:?}",
                    label_id,
                    GetLastError()
                )));
            }
            InvalidateRect(Some(hwnd_label), None, true); // Trigger repaint for WM_CTLCOLORSTATIC
        }
        Ok(())
    } else {
        // This case should have been caught above, but as a safeguard:
        Err(PlatformError::InvalidHandle(format!(
            "Label HWND for logical ID {} became invalid before API call in WinID {:?}",
            label_id, window_id
        )))
    }
}
