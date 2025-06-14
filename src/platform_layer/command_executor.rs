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
use super::types::{CheckState, LayoutRule, MenuItemConfig, TreeItemId, WindowId};

use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::InvalidateRect,
        UI::{Controls::WC_EDITW, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
    },
    core::HSTRING,
};

/*
 * Executes the `DefineLayout` command.
 * This function stores the provided `layout_rules` within the specified window's
 * data and then triggers a layout recalculation to apply the new rules.
 */
pub(crate) fn execute_define_layout(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    rules: Vec<LayoutRule>,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: Storing {} layout rules for WinID {:?}.",
        rules.len(),
        window_id
    );

    internal_state.with_window_data_write(window_id, |window_data| {
        window_data.define_layout(rules);
        Ok(())
    })?;

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
 * Instead of invoking the application logic immediately, this function posts a
 * custom window message. The event is then delivered once the Win32 message
 * loop is running, ensuring that controls like the TreeView have completed
 * their internal setup before the application populates them.
 */
pub(crate) fn execute_signal_main_window_ui_setup_complete(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_signal_main_window_ui_setup_complete for window_id: {:?}",
        window_id
    );

    let hwnd_target = internal_state
        .with_window_data_read(window_id, |window_data| Ok(window_data.get_hwnd()))?;

    if hwnd_target.is_invalid() {
        log::warn!(
            "CommandExecutor: Invalid HWND when posting UI setup complete for WindowId {:?}",
            window_id
        );
        return Err(PlatformError::InvalidHandle(format!(
            "Invalid HWND for WindowId {:?} when posting UI setup complete",
            window_id
        )));
    }

    log::debug!(
        "execute_signal_main_window_ui_setup_complete: Post message WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE"
    );
    unsafe {
        if PostMessageW(
            Some(hwnd_target),
            crate::platform_layer::window_common::WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE,
            WPARAM(0),
            LPARAM(0),
        )
        .is_err()
        {
            let err = GetLastError();
            log::error!(
                "CommandExecutor: Failed to post WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE: {:?}",
                err
            );
            return Err(PlatformError::OperationFailed(format!(
                "Failed to post WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE: {:?}",
                err
            )));
        }
    }

    Ok(())
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

    let hwnd_owner = internal_state.with_window_data_write(window_id, |window_data| {
        let hwnd = window_data.get_hwnd();
        if hwnd.is_invalid() {
            log::warn!(
                "CommandExecutor: HWND not yet valid for WindowId {:?} during menu creation.",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "HWND not yet valid for WindowId {:?} during menu creation",
                window_id
            )));
        }
        for item_config in &menu_items {
            // This helper recursively populates the menu and registers actions in window_data
            unsafe { add_menu_item_recursive_impl(h_main_menu, item_config, window_data)? };
        }
        Ok(hwnd)
    })?;

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
            let generated_id = window_data.register_menu_action(action);
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
    let hwnd_ctrl = internal_state.with_window_data_read(window_id, |window_data| {
        window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "CommandExecutor: Control ID {} not found in window {:?} for SetControlEnabled.",
                control_id,
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found in window {:?} for SetControlEnabled",
                control_id, window_id
            ))
        })
    })?;

    if !unsafe { EnableWindow(hwnd_ctrl, enabled) }.as_bool() {
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

pub(crate) fn execute_expand_visible_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_expand_visible_tree_items for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );
    treeview_handler::expand_visible_tree_items(internal_state, window_id, control_id)
}

pub(crate) fn execute_expand_all_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_expand_all_tree_items for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );
    treeview_handler::expand_all_tree_items(internal_state, window_id, control_id)
}

/*
 * Executes the `CreateInput` command.
 * Creates a Win32 EDIT control to be used as a text input field.
 */
pub(crate) fn execute_create_input(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    parent_control_id: Option<i32>,
    control_id: i32,
    initial_text: String,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_create_input for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    internal_state.with_window_data_write(window_id, |window_data| {
        if window_data.has_control(control_id) {
            log::warn!(
                "CommandExecutor: Input with logical ID {} already exists for window {:?}",
                control_id,
                window_id
            );
            return Err(PlatformError::OperationFailed(format!(
                "Input with logical ID {} already exists for window {:?}",
                control_id, window_id
            )));
        }

        let hwnd_parent = match parent_control_id {
            Some(id) => window_data.get_control_hwnd(id).ok_or_else(|| {
                log::warn!(
                "CommandExecutor: Parent control with ID {} not found for CreateInput in WinID {:?}",
                id, window_id
            );
                PlatformError::InvalidHandle(format!(
                    "Parent control with ID {} not found for CreateInput in WinID {:?}",
                    id, window_id
                ))
            })?,
            None => window_data.get_hwnd(),
        };

        if hwnd_parent.is_invalid() {
            log::error!(
                "CommandExecutor: Parent HWND invalid for CreateInput (WinID {:?})",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND invalid for CreateInput (WinID {:?})",
                window_id
            )));
        }

        let h_instance = internal_state.h_instance();
        let hwnd_edit = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WC_EDITW,
                &HSTRING::from(initial_text.as_str()),
                WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                0,
                0,
                10,
                10,
                Some(hwnd_parent),
                Some(HMENU(control_id as usize as *mut std::ffi::c_void)),
                Some(h_instance),
                None,
            )?
        };

        window_data.register_control_hwnd(control_id, hwnd_edit);
        log::debug!(
            "CommandExecutor: Created input field (ID {}) for WinID {:?} with HWND {:?}",
            control_id,
            window_id,
            hwnd_edit
        );
        Ok(())
    })
}

/*
 * Executes the `SetInputText` command to update an EDIT control's content.
 */
pub(crate) fn execute_set_input_text(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    text: String,
) -> PlatformResult<()> {
    let hwnd_edit = internal_state.with_window_data_read(window_id, |window_data| {
        window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "CommandExecutor: Control ID {} not found for SetInputText in WinID {:?}",
                control_id,
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found for SetInputText in WinID {:?}",
                control_id, window_id
            ))
        })
    })?;

    unsafe {
        SetWindowTextW(hwnd_edit, &HSTRING::from(text.as_str())).map_err(|e| {
            log::error!(
                "CommandExecutor: SetWindowTextW failed for input ID {}: {:?}",
                control_id,
                e
            );
            PlatformError::OperationFailed(format!("SetWindowText failed: {:?}", e))
        })?;
    }
    Ok(())
}

/*
 * Executes the `SetInputBackgroundColor` command. The desired color is stored
 * in `NativeWindowData` and applied during WM_CTLCOLOREDIT handling. This
 * avoids reliance on EM_SETBKGNDCOLOR which is not supported for plain EDIT
 * controls on all Windows versions.
 */
pub(crate) fn execute_set_input_background_color(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    color: Option<u32>,
) -> PlatformResult<()> {
    let hwnd_edit = internal_state.with_window_data_write(window_id, |window_data| {
        // Store the new color state, this also handles cleanup of old GDI objects.
        window_data.set_input_background_color(control_id, color)?;

        // Return the HWND for invalidation.
        window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "CommandExecutor: Control ID {} not found for SetInputBackgroundColor in WinID {:?}",
                control_id,
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found for SetInputBackgroundColor in WinID {:?}",
                control_id, window_id
            ))
        })
    })?;

    // Trigger a repaint for the new color to take effect.
    unsafe {
        _ = InvalidateRect(Some(hwnd_edit), None, true);
    }
    Ok(())
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
 * Custom window procedure for panel controls.
 * It forwards important messages like WM_COMMAND and WM_CTLCOLOR* to the panel's parent
 * so that controls embedded within the panel generate events and can be custom-drawn
 * at the main window level.
 */
unsafe extern "system" fn forwarding_panel_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Forward key messages to the parent window (the main window).
        // This allows the main window's WndProc to handle notifications from controls inside the panel.
        if msg == WM_COMMAND
            || msg == WM_CTLCOLOREDIT
            || msg == WM_CTLCOLORSTATIC
            || msg == WM_NOTIFY
        {
            if let Ok(parent) = GetParent(hwnd) {
                if !parent.is_invalid() {
                    // Use SendMessageW to synchronously send the message and return the result.
                    // This is crucial for messages like WM_CTLCOLOREDIT that expect a return value (HBRUSH).
                    return SendMessageW(parent, msg, Some(wparam), Some(lparam));
                }
            }
        }

        let prev = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if prev != 0 {
            let prev_proc: WNDPROC = std::mem::transmute(prev);
            return CallWindowProcW(prev_proc, hwnd, msg, wparam, lparam);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
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

    internal_state.with_window_data_write(window_id, |window_data| {
        if window_data.has_control(panel_id) {
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
            None => window_data.get_hwnd(), // Parent is the main window
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

        let h_instance = internal_state.h_instance();
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
                Some(h_instance),
                None,
            )?
        };
        unsafe {
            let prev = SetWindowLongPtrW(hwnd_panel, GWLP_WNDPROC, forwarding_panel_proc as usize);
            SetWindowLongPtrW(hwnd_panel, GWLP_USERDATA, prev);
        }
        window_data.register_control_hwnd(panel_id, hwnd_panel);
        log::debug!(
            "CommandExecutor: Created panel (LogicalID {}) for WinID {:?} with HWND {:?}",
            panel_id,
            window_id,
            hwnd_panel
        );
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*; // Import functions from command_executor like execute_expand_all_tree_items
    use crate::platform_layer::{
        WindowId,
        app::Win32ApiInternalState,
        types::{MenuAction, MenuItemConfig},
        window_common::NativeWindowData,
    };
    use std::sync::Arc;
    use windows::Win32::UI::WindowsAndMessaging::{CreateMenu, DestroyMenu};

    // Helper to set up a basic Win32ApiInternalState and NativeWindowData for tests
    // This helper function is now local to the tests module.
    fn setup_test_env() -> (Arc<Win32ApiInternalState>, WindowId, NativeWindowData) {
        let internal_state_arc =
            Win32ApiInternalState::new("TestAppForExecutor".to_string()).unwrap();
        // WindowId now needs to be generated from the state.
        let window_id = internal_state_arc.generate_unique_window_id();
        let native_window_data = NativeWindowData::new(window_id);
        (internal_state_arc, window_id, native_window_data)
    }

    #[test]
    fn test_add_menu_item_recursive_impl_builds_map_and_ids() {
        let (_internal_state_arc, _window_id, mut native_window_data) = setup_test_env();

        let menu_items = vec![
            MenuItemConfig {
                action: Some(MenuAction::LoadProfile),
                text: "Load".to_string(),
                children: vec![],
            },
            MenuItemConfig {
                action: None,
                text: "File".to_string(),
                children: vec![MenuItemConfig {
                    action: Some(MenuAction::SaveProfileAs),
                    text: "Save As".to_string(),
                    children: vec![],
                }],
            },
            MenuItemConfig {
                action: Some(MenuAction::RefreshFileList),
                text: "Refresh".to_string(),
                children: vec![],
            },
        ];

        unsafe {
            let h_main_menu = CreateMenu().expect("Failed to create dummy menu for test");
            // Call the function directly from the parent module
            for item_config in &menu_items {
                add_menu_item_recursive_impl(h_main_menu, item_config, &mut native_window_data)
                    .unwrap();
            }
            DestroyMenu(h_main_menu).expect("Failed to destroy dummy menu for test");
        }

        assert_eq!(
            native_window_data.menu_action_count(),
            3,
            "Expected 3 actions in map: Load, Save As, Refresh"
        );
        assert_eq!(
            native_window_data.get_next_menu_item_id_counter(),
            30003,
            "Menu item ID counter should advance by 3"
        );

        let mut found_load = false;
        let mut found_save_as = false;
        let mut found_refresh = false;

        for (id, action) in native_window_data.iter_menu_actions() {
            assert!(
                *id >= 30000 && *id < 30003,
                "Generated menu IDs should be in the expected range"
            );
            match action {
                MenuAction::LoadProfile => found_load = true,
                MenuAction::SaveProfileAs => found_save_as = true,
                MenuAction::RefreshFileList => found_refresh = true,
                _ => panic!("Unexpected action {:?} found in menu_action_map", action),
            }
        }
        assert!(found_load, "MenuAction::LoadProfile not found in map");
        assert!(found_save_as, "MenuAction::SaveProfileAs not found in map");
        assert!(
            found_refresh,
            "MenuAction::RefreshFileList not found in map"
        );
    }

    #[test]
    fn test_expand_visible_tree_items_returns_error() {
        let (internal_state, window_id, native_window_data) = setup_test_env();
        {
            let mut guard = internal_state.active_windows().write().unwrap();
            guard.insert(window_id, native_window_data);
        }

        let result = execute_expand_visible_tree_items(
            &internal_state,
            window_id,
            999, // A non-existent control ID
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_all_tree_items_returns_error() {
        let (internal_state, window_id, native_window_data) = setup_test_env();
        {
            let mut guard = internal_state.active_windows().write().unwrap();
            guard.insert(window_id, native_window_data);
        }

        let result = execute_expand_all_tree_items(
            &internal_state,
            window_id,
            999, // A non-existent control ID
        );
        assert!(result.is_err());
    }
}
