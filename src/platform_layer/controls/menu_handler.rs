/*
 * Encapsulates Win32-specific menu creation and command routing.
 * This handler isolates menu logic so that other platform components
 * remain decoupled from raw Win32 menu APIs.
 *
 * The concrete implementations will be added as menu functionality
 * is migrated from `command_executor` and `window_common` in later steps.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{AppEvent, MenuAction, MenuItemConfig, WindowId};
use crate::platform_layer::window_common::NativeWindowData;

use std::sync::Arc;
use windows::Win32::UI::WindowsAndMessaging::{HMENU, HWND};

/*
 * Handles the `CreateMainMenu` command by constructing the native menu
 * structure for the given window. Logic will be moved from
 * `command_executor` in a future step.
 */
pub(crate) fn handle_create_main_menu_command(
    _internal_state: &Arc<Win32ApiInternalState>,
    _window_id: WindowId,
    _menu_items: Vec<MenuItemConfig>,
) -> PlatformResult<()> {
    unimplemented!("handle_create_main_menu_command")
}

/*
 * Internal helper for recursively adding menu items to a parent menu.
 * This will also register semantic actions with the window's data
 * structure. The real implementation is pending migration.
 */
pub(crate) unsafe fn add_menu_item_recursive_impl(
    _parent_menu_handle: HMENU,
    _item_config: &MenuItemConfig,
    _window_data: &mut NativeWindowData,
) -> PlatformResult<()> {
    unimplemented!("add_menu_item_recursive_impl")
}

/*
 * Extracted handling of menu-triggered `WM_COMMAND` messages.
 * When invoked, it should translate the menu action into an
 * `AppEvent::MenuActionClicked`.
 */
pub(crate) fn handle_wm_command_for_menu(
    _window_id: WindowId,
    _command_id: i32,
    _hwnd_menu_owner: HWND,
    _internal_state: &Arc<Win32ApiInternalState>,
) -> Option<AppEvent> {
    unimplemented!("handle_wm_command_for_menu")
}
