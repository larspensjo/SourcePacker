/*
 * Unit tests for the command_executor module.
 * These tests verify the correct behavior of functions that execute PlatformCommands,
 * particularly focusing on state changes within NativeWindowData or interactions
 * with mockable dependencies if those were involved (though most here are direct WinAPI).
 */

#[cfg(test)]
mod tests {
    use crate::platform_layer::{
        app::Win32ApiInternalState, // For creating internal_state_arc
        command_executor,           // The module we are testing
        types::{MenuAction, MenuItemConfig, MessageSeverity, WindowId},
        window_common::{self, NativeWindowData}, // For NativeWindowData
    };
    use std::collections::HashMap;
    use std::ptr;
    use std::sync::Arc;
    use windows::Win32::UI::WindowsAndMessaging::{CreateMenu, DestroyMenu, HMENU};

    // Helper to set up a basic Win32ApiInternalState and NativeWindowData for tests
    fn setup_test_env() -> (Arc<Win32ApiInternalState>, WindowId, NativeWindowData) {
        // Ensure logging is initialized for tests if it relies on it.
        // crate::initialize_logging(); // If your main has this, test setup might need it too.
        // For now, assuming logs in command_executor don't critically fail tests if logger not fully set.

        let internal_state_arc =
            Win32ApiInternalState::new("TestAppForExecutor".to_string()).unwrap();
        let window_id = internal_state_arc.generate_window_id();

        // Create a dummy NativeWindowData. In a real scenario, this would be
        // more complexly managed by Win32ApiInternalState's window_map.
        // For focused testing of command_executor functions, we often need to
        // provide a relevant NativeWindowData instance.
        let native_window_data = NativeWindowData {
            hwnd: window_common::HWND_INVALID, // Dummy HWND for this test; real HWND needed for SetMenu
            id: window_id,
            treeview_state: None,
            controls: HashMap::new(),
            status_bar_current_text: String::new(),
            status_bar_current_severity: MessageSeverity::None,
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 30000,
            layout_rules: None,
        };
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
                action: None, // This is a popup
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
            // We need a valid HMENU for AppendMenuW calls, even if we don't SetMenu on a window.
            let h_main_menu = CreateMenu().expect("Failed to create dummy menu for test");
            for item_config in &menu_items {
                command_executor::add_menu_item_recursive_impl(
                    h_main_menu,
                    item_config,
                    &mut native_window_data,
                )
                .unwrap();
            }
            DestroyMenu(h_main_menu).expect("Failed to destroy dummy menu for test");
        }

        assert_eq!(
            native_window_data.menu_action_map.len(),
            3,
            "Expected 3 actions in map: Load, Save As, Refresh"
        );
        assert_eq!(
            native_window_data.next_menu_item_id_counter, 30003,
            "Menu item ID counter should advance by 3"
        );

        let mut found_load = false;
        let mut found_save_as = false;
        let mut found_refresh = false;

        for (id, action) in &native_window_data.menu_action_map {
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

    // Future tests for other command_executor functions can go here.
    // For example, testing `execute_update_status_bar_text` would involve:
    // 1. Setting up `internal_state_arc` and adding a `NativeWindowData` with a dummy status bar HWND to its `window_map`.
    // 2. Calling `execute_update_status_bar_text`.
    // 3. Verifying that `NativeWindowData.status_bar_current_text` and `status_bar_current_severity` are updated.
    // (Directly testing the WinAPI call `SetWindowTextW` is harder without a real window).
}
