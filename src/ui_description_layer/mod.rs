/*
 * This module is responsible for defining the static structure of the UI.
 * It generates a series of `PlatformCommand`s that describe the layout
 * and initial properties of UI elements like menus, buttons, status bars, and tree views.
 * This decouples the UI definition from the platform-specific implementation,
 * facilitating a more generic platform layer.
 */

use crate::platform_layer::{
    control_treeview::ID_TREEVIEW_CTRL,
    types::{DockStyle, LayoutRule, MenuAction, MenuItemConfig, PlatformCommand, WindowId},
    window_common::{
        BUTTON_AREA_HEIGHT, ID_BUTTON_GENERATE_ARCHIVE, ID_STATUS_BAR_CTRL, STATUS_BAR_HEIGHT,
    },
};

/*
 * Generates a list of `PlatformCommand`s that describe the initial static UI layout
 * for the main application window. This includes creating the main menu, TreeView,
 * status bar, and other foundational UI elements like buttons. It also now includes
 * `DefineLayout` commands to specify how these controls should be positioned and resized.
 * These commands are processed by the platform layer to construct the native UI.
 * Menu items use `MenuAction` for semantic identification.
 */
pub fn describe_main_window_layout(window_id: WindowId) -> Vec<PlatformCommand> {
    log::debug!("ui_description_layer: describe_main_window_layout called.");

    let mut commands = Vec::new();

    // 1. Define the "File" menu structure
    let file_menu_items = vec![
        MenuItemConfig {
            action: Some(MenuAction::LoadProfile),
            text: "Load Profile...".to_string(),
            children: Vec::new(),
        },
        MenuItemConfig {
            action: Some(MenuAction::SaveProfileAs),
            text: "Save Profile As...".to_string(),
            children: Vec::new(),
        },
        MenuItemConfig {
            action: Some(MenuAction::SetArchivePath),
            text: "Set Archive Path...".to_string(),
            children: Vec::new(),
        },
    ];

    let main_menu_command = PlatformCommand::CreateMainMenu {
        window_id,
        menu_items: vec![
            MenuItemConfig {
                action: None, // Top-level "&File" is a popup, no direct action
                text: "&File".to_string(),
                children: file_menu_items,
            },
            MenuItemConfig {
                action: Some(MenuAction::RefreshFileList),
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

    // 5. Define Layout Rules for the controls
    let layout_rules = vec![
        // Status Bar: Docks to the bottom, fixed height. Order 0 (processed first among bottom docks).
        LayoutRule {
            control_id: ID_STATUS_BAR_CTRL,
            dock_style: DockStyle::Bottom,
            order: 0,
            fixed_size: Some(STATUS_BAR_HEIGHT),
            margin: (0, 0, 0, 0),
        },
        // Button Area (conceptually, the button docks within this space):
        // The "Generate Archive" button docks to the bottom of the remaining space AFTER status bar.
        // For simplicity now, we'll treat the button itself as docking.
        // It needs a fixed height area, so its "DockStyle::Bottom" will be relative to space above status bar.
        LayoutRule {
            control_id: ID_BUTTON_GENERATE_ARCHIVE,
            dock_style: DockStyle::Bottom, // It will be placed in the area made available by TreeView Fill
            order: 1, // After status bar, but before TreeView fill for bottom calculation
            fixed_size: Some(BUTTON_AREA_HEIGHT), // This rule implies the button gets this height band
            // The button itself might be smaller, this is the "panel" height it occupies.
            // The generic layout engine will need to interpret this.
            // For now, let's assume this means the button is placed within this band.
            // A simpler interpretation is that the button itself is just placed,
            // and the TreeView fills above it.
            // Let's adjust for a simpler direct control layout:
            // The button will be positioned from the bottom, within the button area space.
            // Actual button height is smaller than BUTTON_AREA_HEIGHT.
            // This simple DockStyle might need refinement or a more complex layout system.
            // For now, we'll use margin to position it within its "conceptual" bottom panel.
            margin: (
                BUTTON_AREA_HEIGHT - crate::platform_layer::window_common::BUTTON_HEIGHT - 5, /* top margin within its allocated space */
                0,                                                      /* right */
                5, /* bottom margin within its allocated space */
                crate::platform_layer::window_common::BUTTON_X_PADDING, /* left */
            ),
        },
        // TreeView: Fills the remaining space. Order 10 (processed last).
        LayoutRule {
            control_id: ID_TREEVIEW_CTRL,
            dock_style: DockStyle::Fill,
            order: 10,
            fixed_size: None,
            margin: (0, 0, 0, 0),
        },
    ];

    commands.push(PlatformCommand::DefineLayout {
        window_id,
        rules: layout_rules,
    });

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_layer::{
        control_treeview::ID_TREEVIEW_CTRL,
        types::{DockStyle, MenuAction},
        window_common::{ID_BUTTON_GENERATE_ARCHIVE, ID_STATUS_BAR_CTRL},
    };

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
        let file_menu_item = menu_items.iter().find(|item| item.text == "&File");
        assert!(file_menu_item.is_some(), "File menu item should exist.");
        assert_eq!(
            file_menu_item.unwrap().action,
            None,
            "File menu item should have no action (it's a popup)."
        );
        assert!(
            !file_menu_item.unwrap().children.is_empty(),
            "File menu item should have children."
        );

        // Check for "Refresh" menu directly
        let refresh_menu_item = menu_items.iter().find(|item| item.text == "&Refresh");
        assert!(
            refresh_menu_item.is_some(),
            "Refresh menu item should exist."
        );
        assert_eq!(
            refresh_menu_item.unwrap().action,
            Some(MenuAction::RefreshFileList),
            "Refresh menu item should have RefreshFileList action."
        );
        assert!(
            refresh_menu_item.unwrap().children.is_empty(),
            "Refresh menu item should have no children."
        );

        // Check that "Refresh" is NOT under "File"
        let file_menu = menu_items.iter().find(|item| item.text == "&File").unwrap();
        assert!(
            !file_menu
                .children
                .iter()
                .any(|sub_item| sub_item.action == Some(MenuAction::RefreshFileList))
        );

        // Check specific actions under "File"
        assert!(
            file_menu
                .children
                .iter()
                .any(|sub_item| sub_item.action == Some(MenuAction::LoadProfile)
                    && sub_item.text == "Load Profile...")
        );
        assert!(file_menu.children.iter().any(|sub_item| sub_item.action
            == Some(MenuAction::SaveProfileAs)
            && sub_item.text == "Save Profile As..."));
        assert!(file_menu.children.iter().any(|sub_item| sub_item.action
            == Some(MenuAction::SetArchivePath)
            && sub_item.text == "Set Archive Path..."));
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

    #[test]
    fn test_describe_main_window_layout_generates_define_layout_command() {
        let dummy_window_id = WindowId(1);
        let commands = describe_main_window_layout(dummy_window_id);

        let define_layout_command = commands.iter().find_map(|cmd| {
            if let PlatformCommand::DefineLayout { window_id, rules } = cmd {
                if *window_id == dummy_window_id {
                    Some(rules.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        assert!(
            define_layout_command.is_some(),
            "Commands should include DefineLayout"
        );
        let rules = define_layout_command.unwrap();
        assert_eq!(rules.len(), 3, "Should define rules for 3 controls");

        // Check status bar rule
        let status_bar_rule = rules
            .iter()
            .find(|r| r.control_id == ID_STATUS_BAR_CTRL)
            .expect("Status bar rule not found");
        assert_eq!(status_bar_rule.dock_style, DockStyle::Bottom);
        assert_eq!(status_bar_rule.order, 0);
        assert_eq!(
            status_bar_rule.fixed_size,
            Some(crate::platform_layer::window_common::STATUS_BAR_HEIGHT)
        );

        // Check button rule
        let button_rule = rules
            .iter()
            .find(|r| r.control_id == ID_BUTTON_GENERATE_ARCHIVE)
            .expect("Button rule not found");
        assert_eq!(button_rule.dock_style, DockStyle::Bottom);
        assert_eq!(button_rule.order, 1);
        assert_eq!(
            button_rule.fixed_size,
            Some(crate::platform_layer::window_common::BUTTON_AREA_HEIGHT)
        ); // This is the conceptual band height

        // Check tree view rule
        let treeview_rule = rules
            .iter()
            .find(|r| r.control_id == ID_TREEVIEW_CTRL)
            .expect("TreeView rule not found");
        assert_eq!(treeview_rule.dock_style, DockStyle::Fill);
        assert_eq!(treeview_rule.order, 10);
        assert_eq!(treeview_rule.fixed_size, None);
    }
}
