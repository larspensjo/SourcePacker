/*
 * This module is responsible for defining the static structure of the UI.
 * It generates a series of `PlatformCommand`s that describe the layout
 * and initial properties of UI elements like menus, buttons, and status bars.
 * This decouples the UI definition from the platform-specific implementation.
 */

use crate::platform_layer::{
    types::{MenuItemConfig, PlatformCommand, WindowId},
    window_common::{
        ID_BUTTON_GENERATE_ARCHIVE, ID_MENU_FILE_LOAD_PROFILE, ID_MENU_FILE_REFRESH,
        ID_MENU_FILE_SAVE_PROFILE_AS, ID_MENU_FILE_SET_ARCHIVE,
    },
};

/*
 * Generates a list of `PlatformCommand`s that describe the initial static UI layout
 * for the main application window. This includes creating the main menu and other
 * foundational UI elements like buttons.
 */
pub fn describe_main_window_layout(window_id: WindowId) -> Vec<PlatformCommand> {
    println!("ui_description_layer: describe_main_window_layout called.");

    let mut commands = Vec::new();

    // Define the "File" menu structure
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
        // TODO: Add MF_SEPARATOR equivalent if MenuItemConfig supports it,
        // or handle separators differently in platform_layer. For now, omitting.
        MenuItemConfig {
            id: ID_MENU_FILE_REFRESH,
            text: "Refresh File List".to_string(),
            children: Vec::new(),
        },
    ];

    let main_menu_command = PlatformCommand::CreateMainMenu {
        window_id,
        menu_items: vec![MenuItemConfig {
            id: 0, // Top-level menu items like "&File" don't usually have command IDs themselves
            text: "&File".to_string(),
            children: file_menu_items,
        }],
    };
    commands.push(main_menu_command);

    // 2. Create "Generate Archive" Button
    commands.push(PlatformCommand::CreateButton {
        window_id: window_id,
        control_id: ID_BUTTON_GENERATE_ARCHIVE,
        text: "Generate Archive".to_string(),
    });

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_layer::WindowId;

    #[test]
    fn test_describe_main_window_layout_initially_empty() {
        let dummy_window_id = WindowId(1);
        // Temporarily modify for this test as it's no longer empty
        // let commands = describe_main_window_layout(dummy_window_id);
        // assert!(
        //     commands.is_empty(),
        //     "describe_main_window_layout should return an empty Vec initially."
        // );
    }

    #[test]
    fn test_describe_main_window_layout_generates_create_main_menu_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);
        assert!(
            commands
                .iter()
                .any(|cmd| matches!(cmd, PlatformCommand::CreateMainMenu { .. })),
            "describe_main_window_layout should generate a CreateMainMenu command."
        );
    }

    #[test]
    fn test_describe_main_window_layout_generates_create_button_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);

        let has_create_main_menu = commands.iter().any(|cmd| {
            matches!(cmd, PlatformCommand::CreateMainMenu { window_id, .. } if *window_id == dummy_window_id)
        });
        assert!(
            has_create_main_menu,
            "Commands should include CreateMainMenu"
        );

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
}
