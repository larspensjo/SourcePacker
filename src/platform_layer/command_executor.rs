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
use super::types::{CheckState, ControlId, LayoutRule, MenuItemConfig, TreeItemId, WindowId};

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
        "CommandExecutor: execute_signal_main_window_ui_setup_complete for window_id: {window_id:?}"
    );

    let hwnd_target = internal_state
        .with_window_data_read(window_id, |window_data| Ok(window_data.get_hwnd()))?;

    if hwnd_target.is_invalid() {
        log::warn!(
            "CommandExecutor: Invalid HWND when posting UI setup complete for WindowId {window_id:?}"
        );
        return Err(PlatformError::InvalidHandle(format!(
            "Invalid HWND for WindowId {window_id:?} when posting UI setup complete"
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
                "CommandExecutor: Failed to post WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE: {err:?}"
            );
            return Err(PlatformError::OperationFailed(format!(
                "Failed to post WM_APP_MAIN_WINDOW_UI_SETUP_COMPLETE: {err:?}"
            )));
        }
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
    control_id: ControlId,
    enabled: bool,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_set_control_enabled for WinID {window_id:?}, ControlID {}, Enabled: {enabled}",
        control_id.raw()
    );
    let hwnd_ctrl = internal_state.with_window_data_read(window_id, |window_data| {
        window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "CommandExecutor: Control ID {} not found in window {window_id:?} for SetControlEnabled.",
                control_id.raw()
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found in window {window_id:?} for SetControlEnabled",
                control_id.raw()
            ))
        })
    })?;

    if unsafe { !EnableWindow(hwnd_ctrl, enabled) }.as_bool() {
        // EnableWindow returns non-zero if previously disabled, zero if previously enabled.
        // It doesn't directly indicate error unless GetLastError is checked,
        // but for this operation, we usually assume it succeeds if HWND is valid.
        // We can log if we want to be more verbose.
        log::trace!(
            "CommandExecutor: EnableWindow call for Control ID {} in window {window_id:?} (enabled: {enabled}).",
            control_id.raw()
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
    control_id: ControlId,
    items: Vec<super::types::TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_populate_treeview for WinID {window_id:?}, ControlID {}, delegating to treeview_handler.",
        control_id.raw()
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
    control_id: ControlId,
    item_id: TreeItemId,
    new_state: CheckState,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_update_tree_item_visual_state for WinID {window_id:?}, ControlID {}, ItemID {item_id:?}, delegating.",
        control_id.raw()
    );
    treeview_handler::update_treeview_item_visual_state(
        internal_state,
        window_id,
        control_id,
        item_id,
        new_state,
    )
}

pub(crate) fn execute_update_tree_item_text(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: ControlId,
    item_id: TreeItemId,
    text: String,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_update_tree_item_text for WinID {window_id:?}, ControlID {}, ItemID {item_id:?}",
        control_id.raw()
    );
    treeview_handler::update_treeview_item_text(
        internal_state,
        window_id,
        control_id,
        item_id,
        text,
    )
}

pub(crate) fn execute_expand_visible_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: ControlId,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_expand_visible_tree_items for WinID {window_id:?}, ControlID {}",
        control_id.raw()
    );
    treeview_handler::expand_visible_tree_items(internal_state, window_id, control_id)
}

pub(crate) fn execute_expand_all_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: ControlId,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_expand_all_tree_items for WinID {window_id:?}, ControlID {}",
        control_id.raw()
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
    parent_control_id: Option<ControlId>,
    control_id: ControlId,
    initial_text: String,
) -> PlatformResult<()> {
    log::debug!(
        "CommandExecutor: execute_create_input for WinID {window_id:?}, ControlID {}",
        control_id.raw()
    );

    internal_state.with_window_data_write(window_id, |window_data| {
        if window_data.has_control(control_id) {
            log::warn!(
                "CommandExecutor: Input with logical ID {} already exists for window {window_id:?}",
                control_id.raw()
            );
            return Err(PlatformError::OperationFailed(format!(
                "Input with logical ID {} already exists for window {window_id:?}",
                control_id.raw()
            )));
        }

        let hwnd_parent = match parent_control_id {
            Some(id) => window_data.get_control_hwnd(id).ok_or_else(|| {
                log::warn!(
                "CommandExecutor: Parent control with ID {} not found for CreateInput in WinID {window_id:?}",
                id.raw()
            );
                PlatformError::InvalidHandle(format!(
                    "Parent control with ID {} not found for CreateInput in WinID {window_id:?}",
                    id.raw()
                ))
            })?,
            None => window_data.get_hwnd(),
        };

        if hwnd_parent.is_invalid() {
            log::error!(
                "CommandExecutor: Parent HWND invalid for CreateInput control ID {} (WinID {window_id:?})",
                control_id.raw()
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND invalid for CreateInput control ID {} (WinID {window_id:?})",
                control_id.raw()
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
                Some(HMENU(control_id.raw() as usize as *mut std::ffi::c_void)),
                Some(h_instance),
                None,
            )?
        };

        window_data.register_control_hwnd(control_id, hwnd_edit);
        log::debug!(
            "CommandExecutor: Created input field (ID {}) for WinID {window_id:?} with HWND {hwnd_edit:?}",
            control_id.raw()
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
    control_id: ControlId,
    text: String,
) -> PlatformResult<()> {
    let hwnd_edit = internal_state.with_window_data_read(window_id, |window_data| {
        window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "CommandExecutor: Control ID {} not found for SetInputText in WinID {window_id:?}",
                control_id.raw()
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found for SetInputText in WinID {window_id:?}",
                control_id.raw()
            ))
        })
    })?;

    unsafe {
        SetWindowTextW(hwnd_edit, &HSTRING::from(text.as_str())).map_err(|e| {
            log::error!(
                "CommandExecutor: SetWindowTextW failed for input ID {}: {e:?}",
                control_id.raw()
            );
            PlatformError::OperationFailed(format!("SetWindowText failed: {e:?}"))
        })?;
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

#[cfg(test)]
mod tests {
    use super::*; // Import functions from command_executor like execute_expand_all_tree_items
    use crate::platform_layer::{
        WindowId,
        app::Win32ApiInternalState,
        types::{ControlId, MenuAction, MenuItemConfig},
        window_common::NativeWindowData,
    };
    use std::sync::Arc;

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
    fn test_expand_visible_tree_items_returns_error() {
        let (internal_state, window_id, native_window_data) = setup_test_env();
        {
            let mut guard = internal_state.active_windows().write().unwrap();
            guard.insert(window_id, native_window_data);
        }

        let result = execute_expand_visible_tree_items(
            &internal_state,
            window_id,
            ControlId::new(999), // A non-existent control ID
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
            ControlId::new(999), // A non-existent control ID
        );
        assert!(result.is_err());
    }
}
