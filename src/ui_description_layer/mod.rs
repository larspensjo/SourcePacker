/*
 * This module is responsible for defining the static structure of the UI.
 * It generates a series of `PlatformCommand`s that describe the layout
 * and initial properties of UI elements like menus, buttons, status bars, and tree views.
 * This decouples the UI definition from the platform-specific implementation,
 * facilitating a more generic platform layer.
 */
use crate::app_logic::ui_constants;

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
 * status bar, and other foundational UI elements like buttons. It also includes
 * `DefineLayout` commands to specify how these controls should be positioned and resized.
 * These commands are processed by the platform layer to construct the native UI.
 * Menu items use `MenuAction` for semantic identification.
 *
 * This function is intended to be called only once per window, during the initial
 * construction of the main window.
 */
pub fn build_main_window_static_layout(window_id: WindowId) -> Vec<PlatformCommand> {
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

    // 4. Create *Old* Status Bar (will be removed in a later step)
    commands.push(PlatformCommand::CreateStatusBar {
        window_id,
        control_id: ID_STATUS_BAR_CTRL, // The old, single status bar
        initial_text: "Ready (Old)".to_string(), // Indicate it's the old one
    });

    // --- New Status Bar Elements (Phase 5) ---
    // 4.a Create the Status Bar Panel (child of main window)
    commands.push(PlatformCommand::CreatePanel {
        window_id,
        parent_control_id: None, // Child of the main window's client area
        panel_id: ui_constants::STATUS_BAR_PANEL_ID,
    });

    // 4.b Create Labels within the Status Bar Panel
    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        label_id: ui_constants::STATUS_LABEL_GENERAL_ID,
        initial_text: "Status: Initial".to_string(),
    });
    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
        initial_text: "Archive: Initial".to_string(),
    });
    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
        initial_text: "Tokens: Initial".to_string(),
    });

    // 5. Define Layout Rules for the controls
    let layout_rules = vec![
        // Old Status Bar: Docks to the bottom, fixed height. Order 0 (processed first among bottom docks).
        LayoutRule {
            control_id: ID_STATUS_BAR_CTRL,
            dock_style: DockStyle::Bottom,
            order: 0, // At the very bottom
            fixed_size: Some(STATUS_BAR_HEIGHT),
            margin: (0, 0, 0, 0),
        },
        // New Status Bar Panel: Docks to the bottom, above the old one.
        // Order 1, so it's processed after the old status bar when docking from bottom.
        LayoutRule {
            control_id: ui_constants::STATUS_BAR_PANEL_ID,
            dock_style: DockStyle::Bottom,
            order: 1, // Just above the old status bar
            fixed_size: Some(STATUS_BAR_HEIGHT),
            margin: (0, 0, 0, 0), // Panel itself has no margin against window edges here
        },
        // Button Area / "Generate Archive" Button:
        // Docks to the bottom of the remaining space AFTER both status bars.
        LayoutRule {
            control_id: ID_BUTTON_GENERATE_ARCHIVE,
            dock_style: DockStyle::Bottom,
            order: 2,                             // After both status bars are placed
            fixed_size: Some(BUTTON_AREA_HEIGHT), // This is the height of the conceptual "band" for the button
            // Margins position the button within this band:
            margin: (
                5, // Top margin from the top of its allocated band
                crate::platform_layer::window_common::BUTTON_X_PADDING, // Right margin (or use for centering)
                5, // Bottom margin from the bottom of its allocated band
                crate::platform_layer::window_common::BUTTON_X_PADDING, // Left margin
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
