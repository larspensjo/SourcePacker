/*
 * This module is responsible for defining the static structure of the UI.
 * It generates a series of `PlatformCommand`s that describe the layout
 * and initial properties of UI elements like menus, buttons, status bars, and tree views.
 * This decouples the UI definition from the platform-specific implementation,
 * facilitating a more generic platform layer.
 */
use crate::app_logic::ui_constants;

use crate::platform_layer::{
    styling::StyleId,
    types::{
        DockStyle, LabelClass, LayoutRule, MenuAction, MenuItemConfig, PlatformCommand, WindowId,
    },
};

mod theme;
// Height for the panel containing filter controls.
pub const FILTER_BAR_HEIGHT: i32 = 30;
// Fixed width for the "Expand Filtered/All" button.
pub const FILTER_EXPAND_BUTTON_WIDTH: i32 = 120;
pub const FILTER_CLEAR_BUTTON_WIDTH: i32 = 30;
// Fixed height for the status bar at the bottom of the main window.
pub const STATUS_BAR_HEIGHT: i32 = 25;

/*
 * Generates a list of `PlatformCommand`s that describe the initial static UI layout
 * for the main application window. This includes creating the main menu, TreeView,
 * status bar, filter bar and other foundational UI elements. It also includes
 * `DefineLayout` commands to specify how these controls should be positioned and resized.
 * These commands are processed by the platform layer to construct the native UI.
 * Menu items use `MenuAction` for semantic identification.
 *
 * This function is intended to be called only once per window, during the initial
 * construction of the main window.
 */
pub fn build_main_window_static_layout(window_id: WindowId) -> Vec<PlatformCommand> {
    log::debug!("ui_description_layer: describe_main_window_layout called.");

    // Start by defining the entire theme. This ensures all StyleIds are defined
    // before they are applied to any controls.
    let mut commands = theme::define_neon_night_theme();

    // --- Menu Bar ---
    // 1. Define the "File" menu structure
    let file_menu_items = vec![
        MenuItemConfig {
            action: Some(MenuAction::LoadProfile),
            text: "Load Profile...".to_string(),
            children: Vec::new(),
        },
        MenuItemConfig {
            action: Some(MenuAction::NewProfile),
            text: "New Profile...".to_string(),
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
        MenuItemConfig {
            action: Some(MenuAction::EditExcludePatterns),
            text: "Edit Exclude Patterns...".to_string(),
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
            MenuItemConfig {
                action: Some(MenuAction::GenerateArchive),
                text: "&Generate Archive".to_string(),
                children: Vec::new(),
            },
        ],
    };
    commands.push(main_menu_command);

    // --- Main Content Area Panels ---

    // Create a main background panel that everything else will sit on.
    // This allows us to apply a background color to the entire window client area.
    commands.push(PlatformCommand::CreatePanel {
        window_id,
        parent_control_id: None,
        control_id: ui_constants::MAIN_BACKGROUND_PANEL_ID,
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::MAIN_BACKGROUND_PANEL_ID,
        style_id: StyleId::MainWindowBackground,
    });

    // Create Filter Bar Panel (now a child of the main background panel)
    commands.push(PlatformCommand::CreatePanel {
        window_id,
        parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
        control_id: ui_constants::FILTER_PANEL_ID,
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::FILTER_PANEL_ID,
        style_id: StyleId::PanelBackground,
    });

    // Create Status Bar Panel (now a child of the main background panel)
    commands.push(PlatformCommand::CreatePanel {
        window_id,
        parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
        control_id: ui_constants::STATUS_BAR_PANEL_ID,
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::STATUS_BAR_PANEL_ID,
        style_id: StyleId::StatusBarBackground,
    });

    // --- Controls within Panels ---

    // Create Filter Input field within the Filter Panel
    commands.push(PlatformCommand::CreateInput {
        window_id,
        parent_control_id: Some(ui_constants::FILTER_PANEL_ID),
        control_id: ui_constants::FILTER_INPUT_ID,
        initial_text: "".to_string(), // Placeholder text can be set here if desired
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::FILTER_INPUT_ID,
        style_id: StyleId::DefaultInput,
    });

    // Create Clear Filter button
    commands.push(PlatformCommand::CreateButton {
        window_id,
        control_id: ui_constants::FILTER_CLEAR_BUTTON_ID,
        text: "X".to_string(),
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::FILTER_CLEAR_BUTTON_ID,
        style_id: StyleId::DefaultButton,
    });

    // Create "Expand Filtered/All" Button within the Filter Panel
    commands.push(PlatformCommand::CreateButton {
        window_id,
        control_id: ui_constants::FILTER_EXPAND_BUTTON_ID,
        text: "Expand All".to_string(), // Initial text, might change based on filter state
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::FILTER_EXPAND_BUTTON_ID,
        style_id: StyleId::DefaultButton,
    });

    // Create TreeView (now a child of the main background panel)
    commands.push(PlatformCommand::CreateTreeView {
        window_id,
        parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
        control_id: ui_constants::ID_TREEVIEW_CTRL,
    });
    // TreeView styling is handled by its custom draw logic, but we could apply a base style if needed.

    // Create Labels within the Status Bar Panel and apply styles
    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        control_id: ui_constants::STATUS_LABEL_GENERAL_ID,
        initial_text: "Status: Initial".to_string(),
        class: LabelClass::StatusBar, // This class is for font, colors are now from style
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::STATUS_LABEL_GENERAL_ID,
        style_id: StyleId::StatusLabelNormal,
    });

    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        control_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
        initial_text: "Archive: Initial".to_string(),
        class: LabelClass::StatusBar,
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
        style_id: StyleId::StatusLabelNormal,
    });

    commands.push(PlatformCommand::CreateLabel {
        window_id,
        parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
        control_id: ui_constants::STATUS_LABEL_TOKENS_ID,
        initial_text: "Tokens: Initial".to_string(),
        class: LabelClass::StatusBar,
    });
    commands.push(PlatformCommand::ApplyStyleToControl {
        window_id,
        control_id: ui_constants::STATUS_LABEL_TOKENS_ID,
        style_id: StyleId::StatusLabelNormal,
    });

    // --- Layout Rules ---
    let layout_rules = vec![
        // The main background panel fills the entire window.
        LayoutRule {
            control_id: ui_constants::MAIN_BACKGROUND_PANEL_ID,
            parent_control_id: None,
            dock_style: DockStyle::Fill,
            order: 0,
            fixed_size: None,
            margin: (0, 0, 0, 0),
        },
        // Filter Bar Panel: Docks to the top of the main background panel.
        LayoutRule {
            control_id: ui_constants::FILTER_PANEL_ID,
            parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
            dock_style: DockStyle::Top,
            order: 0, // Process first
            fixed_size: Some(FILTER_BAR_HEIGHT),
            margin: (2, 2, 2, 2), // Small margin around the panel
        },
        // Status Bar Panel: Docks to the bottom of the main background panel.
        LayoutRule {
            control_id: ui_constants::STATUS_BAR_PANEL_ID,
            parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
            dock_style: DockStyle::Bottom,
            order: 1, // Process after top-docked items
            fixed_size: Some(STATUS_BAR_HEIGHT),
            margin: (0, 0, 0, 0),
        },
        // TreeView: Fills the remaining space in the main background panel.
        LayoutRule {
            control_id: ui_constants::ID_TREEVIEW_CTRL,
            parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
            dock_style: DockStyle::Fill,
            order: 10, // Process after fixed-size items
            fixed_size: None,
            margin: (0, 0, 0, 0),
        },
        // Layout Rules for controls WITHIN the Filter Panel (parent_control_id = FILTER_PANEL_ID)
        LayoutRule {
            control_id: ui_constants::FILTER_EXPAND_BUTTON_ID,
            parent_control_id: Some(ui_constants::FILTER_PANEL_ID),
            dock_style: DockStyle::Right, // Button on the right
            order: 0,                     // Process first within its parent
            fixed_size: Some(FILTER_EXPAND_BUTTON_WIDTH),
            margin: (2, 2, 2, 2), // Small margin for the button
        },
        LayoutRule {
            control_id: ui_constants::FILTER_CLEAR_BUTTON_ID,
            parent_control_id: Some(ui_constants::FILTER_PANEL_ID),
            dock_style: DockStyle::Right,
            order: 0,
            fixed_size: Some(FILTER_CLEAR_BUTTON_WIDTH),
            margin: (2, 2, 2, 2),
        },
        LayoutRule {
            control_id: ui_constants::FILTER_INPUT_ID,
            parent_control_id: Some(ui_constants::FILTER_PANEL_ID),
            dock_style: DockStyle::Fill, // Input field takes remaining space
            order: 1,                    // Process after the button
            fixed_size: None,
            margin: (2, 2, 2, 2), // Small margin for the input field
        },
        // Layout Rules for labels WITHIN the status bar panel
        LayoutRule {
            control_id: ui_constants::STATUS_LABEL_GENERAL_ID,
            parent_control_id: Some(ui_constants::STATUS_BAR_PANEL_ID),
            dock_style: DockStyle::ProportionalFill { weight: 2.0 },
            order: 1,
            fixed_size: None,
            margin: (0, 1, 0, 1),
        },
        LayoutRule {
            control_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
            parent_control_id: Some(ui_constants::STATUS_BAR_PANEL_ID),
            dock_style: DockStyle::ProportionalFill { weight: 1.0 },
            order: 1,
            fixed_size: None,
            margin: (0, 1, 0, 1),
        },
        LayoutRule {
            control_id: ui_constants::STATUS_LABEL_TOKENS_ID,
            parent_control_id: Some(ui_constants::STATUS_BAR_PANEL_ID),
            dock_style: DockStyle::ProportionalFill { weight: 1.0 },
            order: 1,
            fixed_size: None,
            margin: (0, 1, 0, 0),
        },
    ];

    commands.push(PlatformCommand::DefineLayout {
        window_id,
        rules: layout_rules,
    });

    commands
}
