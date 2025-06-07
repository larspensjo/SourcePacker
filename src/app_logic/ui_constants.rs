/*
 * Defines shared constants for logical UI control identifiers.
 * These IDs are used by the `ui_description_layer` to define the initial UI
 * structure and by the `app_logic` (Presenter) to target specific controls
 * for dynamic updates. The `platform_layer` maps these logical IDs to native
 * UI element handles.
 */

// Logical ID for the main panel that will contain all status bar elements.
pub const STATUS_BAR_PANEL_ID: i32 = 1010;

// Logical ID for the label displaying general status messages.
pub const STATUS_LABEL_GENERAL_ID: i32 = 1011;

// Logical ID for the label displaying archive-related status.
pub const STATUS_LABEL_ARCHIVE_ID: i32 = 1012;

// Logical ID for the label displaying token count information.
pub const STATUS_LABEL_TOKENS_ID: i32 = 1013;

// Logical ID for the main TreeView control.
pub const ID_TREEVIEW_CTRL: i32 = 1001; // Value from platform_layer::control_treeview
