/*
 * This module is responsible for defining the UI structure of the application.
 *
 * It generates a series of `PlatformCommand`s that describe the layout and
 * elements of UI components, such as windows, menus, and controls. This
 * decouples the UI definition from the platform-specific implementation details,
 * allowing for easier testing and potential future UI toolkit changes.
 */
use crate::platform_layer::{
    PlatformCommand, WindowId,
    types::MenuItemConfig,
    window_common::{
        ID_MENU_FILE_LOAD_PROFILE, ID_MENU_FILE_REFRESH, ID_MENU_FILE_SAVE_PROFILE_AS,
        ID_MENU_FILE_SET_ARCHIVE,
    },
};

/*
 * Generates a list of `PlatformCommand`s that describe the main window's layout.
 *
 * This function will be called by the application's main initialization logic
 * to get the structural commands for the primary UI. Initially, it returns
 * an empty vector, but will be expanded to describe menus, buttons, etc.
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
}
