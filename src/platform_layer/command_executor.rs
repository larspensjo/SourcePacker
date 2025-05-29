/*
 * This module is responsible for executing specific `PlatformCommand`s.
 * It contains functions that take the necessary state (like `Win32ApiInternalState`)
 * and command-specific parameters to perform the requested platform operations.
 * This helps to decouple the command execution logic from the main `app.rs` module.
 */

use super::app::Win32ApiInternalState;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{LayoutRule, WindowId};
use std::sync::Arc;

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

    let mut windows_map_guard = internal_state.window_map.write().map_err(|_| {
        PlatformError::OperationFailed(
            "Failed to lock windows map for execute_define_layout".into(),
        )
    })?;

    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
        window_data.layout_rules = Some(rules);
        log::trace!(
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
