/*
 * Unit tests for the command_executor module.
 * These tests verify the correct behavior of functions that execute PlatformCommands,
 * particularly focusing on state changes within NativeWindowData or interactions
 * with mockable dependencies if those were involved (though most here are direct WinAPI).
 * TODO: These tests aren't detected.
 */

use crate::platform_layer::{
    app::Win32ApiInternalState,
    command_executor, // The module we are testing (now a sibling)
    types::{MenuAction, MenuItemConfig, MessageSeverity, WindowId},
    window_common::{self, NativeWindowData},
};
use std::collections::HashMap;
use std::sync::Arc;
use windows::Win32::UI::WindowsAndMessaging::{CreateMenu, DestroyMenu, HMENU};

// Helper to set up a basic Win32ApiInternalState and NativeWindowData for tests
fn setup_test_env() -> (Arc<Win32ApiInternalState>, WindowId, NativeWindowData) {
    // Ensure logging is initialized for tests if it relies on it.
    // If your main `initialize_logging` sets up a global logger, it might affect tests.
    // For focused unit tests, it's often better if they don't rely on global state like logging.
    // However, if `Win32ApiInternalState::new` logs critical info that you want to see during tests,
    // you might need a test-specific logging setup or ensure the main one is test-friendly.
    // For now, we assume that logging calls within the tested functions won't cause test failures
    // if no logger is explicitly initialized for the test environment.
    // Example: crate::initialize_logging(); // If you have a central logging init

    let internal_state_arc = Win32ApiInternalState::new("TestAppForExecutor".to_string()).unwrap();
    let window_id = internal_state_arc.generate_window_id();

    let native_window_data = NativeWindowData::new(window_id);
    (internal_state_arc, window_id, native_window_data)
}
#[cfg(test)]
mod tests {

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
            // Call the function from the command_executor module directly
            for item_config in &menu_items {
                command_executor::add_menu_item_recursive_impl(
                    // No super:: needed if it's a sibling
                    h_main_menu,
                    item_config,
                    &mut native_window_data,
                )
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

        let result = command_executor::execute_expand_visible_tree_items(
            &internal_state,
            window_id,
            999,
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

        let result = command_executor::execute_expand_all_tree_items(
            &internal_state,
            window_id,
            999,
        );
        assert!(result.is_err());
    }
}
