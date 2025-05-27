/*
 * This module is responsible for defining the static structure of the UI.
 * It generates a series of `PlatformCommand`s that describe the layout
 * and initial properties of UI elements like menus, buttons, status bars, and tree views.
 * This decouples the UI definition from the platform-specific implementation,
 * facilitating a more generic platform layer.
 */

use crate::platform_layer::{
    control_treeview::ID_TREEVIEW_CTRL,
    types::{MenuItemConfig, PlatformCommand, WindowId},
    window_common::{
        ID_BUTTON_GENERATE_ARCHIVE, ID_MENU_FILE_LOAD_PROFILE, ID_MENU_FILE_REFRESH,
        ID_MENU_FILE_SAVE_PROFILE_AS, ID_MENU_FILE_SET_ARCHIVE, ID_STATUS_BAR_CTRL,
    },
};

/*
 * Generates a list of `PlatformCommand`s that describe the initial static UI layout
 * for the main application window. This includes creating the main menu, TreeView,
 * status bar, and other foundational UI elements like buttons.
 * These commands are processed by the platform layer to construct the native UI.
 */
pub fn describe_main_window_layout(window_id: WindowId) -> Vec<PlatformCommand> {
    log::debug!("ui_description_layer: describe_main_window_layout called.");

    let mut commands = Vec::new();

    // 1. Define the "File" menu structure
    let file_menu_items = vec![
        MenuItemConfig {
            id: ID_MENU_FILE_LOAD_PROFILE,
            text: "Load Profile...".to_string(),
            children: Vec::new(),
        },
        MenuItemConfig {
            id: ID_MENU_FILE_SAVE_PROFILE_AS,
            text: "Save Profile As...".to_string(),
            children: Vec::new(),
        },
        MenuItemConfig {
            id: ID_MENU_FILE_SET_ARCHIVE,
            text: "Set Archive Path...".to_string(),
            children: Vec::new(),
        },
    ];

    let main_menu_command = PlatformCommand::CreateMainMenu {
        window_id,
        menu_items: vec![
            MenuItemConfig {
                id: 0, // Top-level menu items like "&File" that are popups don't usually have command IDs themselves
                text: "&File".to_string(),
                children: file_menu_items,
            },
            MenuItemConfig {
                id: ID_MENU_FILE_REFRESH,
                text: "&Refresh".to_string(),
                children: Vec::new(),
            },
        ],
    };
    commands.push(main_menu_command);

    // 2. Create TreeView
    commands.push(PlatformCommand::CreateTreeView {
        window_id,
        control_id: ID_TREEVIEW_CTRL,
    });

    // 3. Create "Generate Archive" Button
    commands.push(PlatformCommand::CreateButton {
        window_id,
        control_id: ID_BUTTON_GENERATE_ARCHIVE,
        text: "Generate Archive".to_string(),
    });

    // 4. Create Status Bar
    commands.push(PlatformCommand::CreateStatusBar {
        window_id,
        control_id: ID_STATUS_BAR_CTRL,
        initial_text: "Ready".to_string(),
    });

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_layer::{WindowId, control_treeview::ID_TREEVIEW_CTRL};

    #[test]
    fn test_describe_main_window_layout_generates_create_main_menu_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);
        let main_menu_cmd = commands.iter().find_map(|cmd| {
            if let PlatformCommand::CreateMainMenu {
                window_id: _,
                menu_items,
            } = cmd
            {
                Some(menu_items)
            } else {
                None
            }
        });
        assert!(
            main_menu_cmd.is_some(),
            "Should generate CreateMainMenu command."
        );

        let menu_items = main_menu_cmd.unwrap();
        // Check for "File" menu
        assert!(
            menu_items
                .iter()
                .any(|item| item.text == "&File" && !item.children.is_empty())
        );
        // Check for "Refresh" menu directly
        assert!(menu_items.iter().any(|item| item.text == "&Refresh"
            && item.id == ID_MENU_FILE_REFRESH
            && item.children.is_empty()));

        // Check that "Refresh" is NOT under "File"
        let file_menu = menu_items.iter().find(|item| item.text == "&File").unwrap();
        assert!(
            !file_menu
                .children
                .iter()
                .any(|sub_item| sub_item.id == ID_MENU_FILE_REFRESH)
        );
    }

    #[test]
    fn test_describe_main_window_layout_generates_create_treeview_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);
        assert!(
            commands.iter().any(|cmd| matches!(
                cmd,
                PlatformCommand::CreateTreeView { window_id, control_id }
                if *window_id == dummy_window_id && *control_id == ID_TREEVIEW_CTRL
            )),
            "describe_main_window_layout should generate a CreateTreeView command."
        );
    }

    #[test]
    fn test_describe_main_window_layout_generates_create_button_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);

        let create_button_command = commands.iter().find_map(|cmd| {
            if let PlatformCommand::CreateButton {
                window_id,
                control_id,
                text,
            } = cmd
            {
                if *window_id == dummy_window_id && *control_id == ID_BUTTON_GENERATE_ARCHIVE {
                    Some(text.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert!(
            create_button_command.is_some(),
            "Commands should include CreateButton for the generate archive button"
        );
        assert_eq!(
            create_button_command.unwrap(),
            "Generate Archive",
            "CreateButton command should have the correct text"
        );
    }

    #[test]
    fn test_describe_main_window_layout_generates_create_status_bar_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);

        let create_status_bar_command = commands.iter().find_map(|cmd| {
            if let PlatformCommand::CreateStatusBar {
                window_id,
                control_id,
                initial_text,
            } = cmd
            {
                if *window_id == dummy_window_id && *control_id == ID_STATUS_BAR_CTRL {
                    Some(initial_text.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert!(
            create_status_bar_command.is_some(),
            "Commands should include CreateStatusBar for the status bar"
        );
        assert_eq!(
            create_status_bar_command.unwrap(),
            "Ready",
            "CreateStatusBar command should have the correct initial text"
        );
    }
}
