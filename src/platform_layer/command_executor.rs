/*
 * This module is responsible for executing specific `PlatformCommand`s.
 * It contains functions that take the necessary state (like `Win32ApiInternalState`)
 * and command-specific parameters to perform the requested platform operations.
 * This helps to decouple the command execution logic from the main `app.rs` module.
 *
 * For some controls, like TreeView, this module may delegate the actual
 * implementation to more specific handlers within the `super::controls` module
 * (e.g., `treeview_handler`).
 */

use super::app::Win32ApiInternalState;
use super::controls::treeview_handler; // Ensure treeview_handler is used for its functions
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, LayoutRule, MenuItemConfig, TreeItemId, WindowId};

use crate::platform_layer::window_common::WC_BUTTON;
use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND},
        UI::{
            Input::KeyboardAndMouse::EnableWindow,
            WindowsAndMessaging::{
                AppendMenuW, BS_PUSHBUTTON, CreateMenu, CreatePopupMenu, CreateWindowExW,
                DestroyMenu, HMENU, MF_POPUP, MF_STRING, PostQuitMessage, SetMenu, WINDOW_EX_STYLE,
                WINDOW_STYLE, WS_CHILD, WS_VISIBLE,
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

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
        log::error!(
            "CommandExecutor: Failed to lock windows map for execute_define_layout: {:?}",
            e
        );
        PlatformError::OperationFailed(
            "Failed to lock windows map for execute_define_layout".into(),
        )
    })?;

    let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
        log::warn!(
            "CommandExecutor: WindowId {:?} not found for execute_define_layout storage.",
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for execute_define_layout storage",
            window_id
        ))
    })?;

    // Store the rules
    window_data.layout_rules = Some(rules);
    log::debug!(
        "CommandExecutor: Stored layout rules for WinID {:?}",
        window_id
    );

    // Explicitly drop the write guard before calling trigger_layout_recalculation,
    // as trigger_layout_recalculation will take its own read lock.
    drop(windows_map_guard);

    // Now trigger the layout recalculation.
    internal_state.trigger_layout_recalculation(window_id);

    Ok(())
}

/*
 * Executes the `QuitApplication` command.
 * Posts a `WM_QUIT` message to the application's message queue, which will
 * eventually cause the main event loop in `PlatformInterface::run` to terminate.
 */
pub(crate) fn execute_quit_application() -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_quit_application. Setting quit flag and Posting WM_QUIT."
    );
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
        let event_handler_guard = internal_state
            .application_event_handler
            .lock()
            .map_err(|e| {
                log::error!(
                    "CommandExecutor: Failed to lock internal event_handler field: {:?}",
                    e
                );
                PlatformError::OperationFailed("Failed to lock internal event_handler field".into())
            })?;
        event_handler_guard
            .as_ref()
            .and_then(|weak_handler| weak_handler.upgrade())
    };

    if let Some(handler_arc) = handler_arc_opt {
        let mut handler_guard = handler_arc.lock().map_err(|e| {
            log::error!("CommandExecutor: Failed to lock app event handler for MainWindowUISetupComplete: {:?}", e);
            PlatformError::OperationFailed("Failed to lock app event handler for MainWindowUISetupComplete".into())
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
    log::debug!(
        "CommandExecutor: execute_create_main_menu for WinID {:?}",
        window_id
    );
    let h_main_menu = unsafe { CreateMenu()? };
    let hwnd_owner_opt: Option<HWND>;

    {
        // Scope for write lock
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|e|{
            log::error!("CommandExecutor: Failed to lock windows map for main menu creation (data population): {:?}", e);
            PlatformError::OperationFailed("Failed to lock windows map for main menu creation (data population)".into())
        })?;

        let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
            unsafe {
                DestroyMenu(h_main_menu).unwrap_or_default();
            }
            log::warn!(
                "CommandExecutor: WindowId {:?} not found for CreateMainMenu (data population).",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateMainMenu (data population)",
                window_id
            ))
        })?;

        hwnd_owner_opt = Some(window_data.this_window_hwnd);
        if window_data.this_window_hwnd.is_invalid() {
            unsafe {
                DestroyMenu(h_main_menu).unwrap_or_default();
            }
            log::warn!(
                "CommandExecutor: HWND not yet valid for WindowId {:?} during menu data population.",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "HWND not yet valid for WindowId {:?} during menu data population",
                window_id
            )));
        }
        for item_config in menu_items {
            // This helper function is now part of menu_handler or command_executor itself
            // For now, let's assume it's still here or move it if refactoring menu_handler next.
            // Keeping it here for now for minimal change to this file beyond TreeView.
            unsafe { add_menu_item_recursive_impl(h_main_menu, &item_config, window_data)? };
        }
    } // Write lock released

    if let Some(hwnd_owner) = hwnd_owner_opt {
        if !hwnd_owner.is_invalid() {
            if unsafe { SetMenu(hwnd_owner, Some(h_main_menu)) }.is_err() {
                let last_error = unsafe { GetLastError() };
                unsafe {
                    DestroyMenu(h_main_menu).unwrap_or_default();
                }
                log::error!(
                    "CommandExecutor: SetMenu failed for main menu on WindowId {:?}: {:?}",
                    window_id,
                    last_error
                );
                return Err(PlatformError::OperationFailed(format!(
                    "SetMenu failed for main menu on WindowId {:?}: {:?}",
                    window_id, last_error
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
            log::warn!(
                "CommandExecutor: Owner HWND was invalid before SetMenu for WinID {:?}",
                window_id
            );
            Err(PlatformError::InvalidHandle(format!(
                "Owner HWND was invalid before SetMenu for WinID {:?}",
                window_id
            )))
        }
    } else {
        // Should not happen if window_data was found and HWND was valid
        unsafe {
            DestroyMenu(h_main_menu).unwrap_or_default();
        }
        log::error!(
            "CommandExecutor: hwnd_owner_opt was None after lock release for WinID {:?}",
            window_id
        );
        Err(PlatformError::OperationFailed(format!(
            "hwnd_owner_opt was None after lock release for WinID {:?}",
            window_id
        )))
    }
}

/*
 * Helper function to recursively add menu items.
 * This is an internal implementation detail for `execute_create_main_menu`.
 */
pub(crate) unsafe fn add_menu_item_recursive_impl(
    parent_menu_handle: HMENU,
    item_config: &MenuItemConfig,
    window_data: &mut super::window_common::NativeWindowData, // Needs full path if moved
) -> PlatformResult<()> {
    if item_config.children.is_empty() {
        // This is a command item
        if let Some(action) = item_config.action {
            let generated_id = window_data.generate_menu_item_id();
            window_data.menu_action_map.insert(generated_id, action);
            log::debug!(
                "CommandExecutor: Mapping menu action {:?} to ID {} for window {:?}",
                action,
                generated_id,
                window_data.logical_window_id
            );
            unsafe {
                AppendMenuW(
                    parent_menu_handle,
                    MF_STRING,
                    generated_id as usize, // WinAPI uses usize for item ID
                    &HSTRING::from(item_config.text.as_str()),
                )?
            };
        } else {
            // Item has no children and no action - could be a separator or an oversight
            log::warn!(
                "CommandExecutor: Menu item '{}' has no children and no action. It will be non-functional unless it's a separator (not yet supported).",
                item_config.text
            );
            // Potentially handle separators here if MF_SEPARATOR is to be supported
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
                h_submenu.0 as usize, // h_submenu is the handle for the popup
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
    log::debug!(
        "CommandExecutor: execute_set_control_enabled for WinID {:?}, ControlID {}, Enabled: {}",
        window_id,
        control_id,
        enabled
    );
    let windows_guard = internal_state.active_windows.read().map_err(|e|{
        log::error!("CommandExecutor: Failed to acquire read lock on windows map for SetControlEnabled: {:?}", e);
        PlatformError::OperationFailed("Failed to acquire read lock on windows map for SetControlEnabled".into())
    })?;

    let window_data = windows_guard.get(&window_id).ok_or_else(|| {
        log::warn!(
            "CommandExecutor: WindowId {:?} not found for SetControlEnabled.",
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for SetControlEnabled",
            window_id
        ))
    })?;

    let hwnd_ctrl = window_data.get_control_hwnd(control_id).ok_or_else(|| {
        log::warn!(
            "CommandExecutor: Control ID {} not found in window {:?} for SetControlEnabled.",
            control_id,
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "Control ID {} not found in window {:?} for SetControlEnabled",
            control_id, window_id
        ))
    })?;

    if unsafe { EnableWindow(hwnd_ctrl, enabled) }.as_bool() == false {
        // EnableWindow returns non-zero if previously disabled, zero if previously enabled.
        // It doesn't directly indicate error unless GetLastError is checked,
        // but for this operation, we usually assume it succeeds if HWND is valid.
        // We can log if we want to be more verbose.
        log::trace!(
            "CommandExecutor: EnableWindow call for Control ID {} in window {:?} (enabled: {}).",
            control_id,
            window_id,
            enabled
        );
    }
    Ok(())
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
    log::debug!(
        "CommandExecutor: execute_create_button for WinID {:?}, ControlID {}, Text: '{}'",
        window_id,
        control_id,
        text
    );
    let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
        log::error!(
            "CommandExecutor: Failed to lock windows map for button creation: {:?}",
            e
        );
        PlatformError::OperationFailed("Failed to lock windows map for button creation".into())
    })?;

    let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
        log::warn!(
            "CommandExecutor: WindowId {:?} not found for CreateButton.",
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateButton",
            window_id
        ))
    })?;

    if window_data.control_hwnd_map.contains_key(&control_id) {
        log::warn!(
            "CommandExecutor: Button with ID {} already exists for window {:?}.",
            control_id,
            window_id
        );
        return Err(PlatformError::OperationFailed(format!(
            "Button with ID {} already exists for window {:?}",
            control_id, window_id
        )));
    }

    if window_data.this_window_hwnd.is_invalid() {
        log::error!(
            "CommandExecutor: Parent HWND for CreateButton is invalid (WinID: {:?})",
            window_id
        );
        return Err(PlatformError::InvalidHandle(format!(
            "Parent HWND for CreateButton is invalid (WinID: {:?})",
            window_id
        )));
    }

    let hwnd_button = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WC_BUTTON, // Standard class name for Button
            &HSTRING::from(text.as_str()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            0,
            0,
            10,
            10, // Dummies, WM_SIZE/LayoutRules will adjust
            Some(window_data.this_window_hwnd),
            Some(HMENU(control_id as *mut _)), // Use logical ID for HMENU
            Some(internal_state.h_instance),
            None,
        )?
    };
    window_data.control_hwnd_map.insert(control_id, hwnd_button);
    log::debug!(
        "CommandExecutor: Created button '{}' (ID {}) for window {:?} with HWND {:?}",
        text,
        control_id,
        window_id,
        hwnd_button
    );
    Ok(())
}

/*
 * Delegates to treeview_handler::populate_treeview.
 * This function remains in command_executor as it's directly executing a command,
 * but the core logic is in the treeview_handler.
 */
pub(crate) fn execute_populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    items: Vec<super::types::TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_populate_treeview for WinID {:?}, ControlID {}, delegating to treeview_handler.",
        window_id,
        control_id
    );
    treeview_handler::populate_treeview(internal_state, window_id, control_id, items)
}

/*
 * Delegates to treeview_handler::update_treeview_item_visual_state.
 * Similar to populate_treeview, this executes the command by calling the handler.
 */
pub(crate) fn execute_update_tree_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    item_id: TreeItemId,
    new_state: CheckState,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_update_tree_item_visual_state for WinID {:?}, ControlID {}, ItemID {:?}, delegating.",
        window_id,
        control_id,
        item_id
    );
    treeview_handler::update_treeview_item_visual_state(
        internal_state,
        window_id,
        control_id,
        item_id,
        new_state,
    )
}

// Commands that call simple window_common functions (or could be moved to window_common if preferred)
pub(crate) fn execute_set_window_title(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: &str,
) -> PlatformResult<()> {
    super::window_common::set_window_title(internal_state, window_id, title)
}

pub(crate) fn execute_show_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    show: bool,
) -> PlatformResult<()> {
    super::window_common::show_window(internal_state, window_id, show)
}

pub(crate) fn execute_close_window(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    super::window_common::send_close_message(internal_state, window_id)
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

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
        log::error!(
            "CommandExecutor: Failed to lock windows map for CreatePanel: {:?}",
            e
        );
        PlatformError::OperationFailed("Failed to lock windows map for CreatePanel".into())
    })?;

    let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
        log::warn!(
            "CommandExecutor: WindowId {:?} not found for CreatePanel.",
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreatePanel",
            window_id
        ))
    })?;

    if window_data.control_hwnd_map.contains_key(&panel_id) {
        log::warn!(
            "CommandExecutor: Panel with logical ID {} already exists for window {:?}.",
            panel_id,
            window_id
        );
        return Err(PlatformError::OperationFailed(format!(
            "Panel with logical ID {} already exists for window {:?}",
            panel_id, window_id
        )));
    }

    let hwnd_parent = match parent_control_id {
        Some(id) => window_data.get_control_hwnd(id).ok_or_else(|| {
            log::warn!("CommandExecutor: Parent control with logical ID {} not found for CreatePanel in WinID {:?}", id, window_id);
            PlatformError::InvalidHandle(format!(
                "Parent control with logical ID {} not found for CreatePanel in WinID {:?}",
                id, window_id
            ))
        })?,
        None => window_data.this_window_hwnd, // Parent is the main window
    };

    if hwnd_parent.is_invalid() {
        log::error!(
            "CommandExecutor: Parent HWND for CreatePanel is invalid (WinID: {:?}, ParentControlID: {:?})",
            window_id,
            parent_control_id
        );
        return Err(PlatformError::InvalidHandle(format!(
            "Parent HWND for CreatePanel is invalid (WinID: {:?}, ParentControlID: {:?})",
            window_id, parent_control_id
        )));
    }

    let hwnd_panel = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0), // Or WS_EX_CONTROLPARENT if it should manage tab order for children
            super::window_common::WC_STATIC, // Using a STATIC control as a simple panel container
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
    window_data.control_hwnd_map.insert(panel_id, hwnd_panel);
    log::debug!(
        "CommandExecutor: Created panel (LogicalID {}) for WinID {:?} with HWND {:?}",
        panel_id,
        window_id,
        hwnd_panel
    );
    Ok(())
}
