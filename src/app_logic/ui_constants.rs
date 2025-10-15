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

// --- Panel IDs ---
pub const MAIN_BACKGROUND_PANEL_ID: i32 = 1000;

// Logical ID for the main TreeView control.
pub const ID_TREEVIEW_CTRL: i32 = 1001; // Value from platform_layer::control_treeview

// Unicode filled circle appended to "New" tree items to make the state obvious. [FileSelStateNewV2]
pub const NEW_ITEM_INDICATOR_CHAR: char = '●';

// Logical ID for the panel that will contain filter input and buttons.
pub const FILTER_PANEL_ID: i32 = 1020;

// Logical ID for the text input field used for filtering the TreeView.
pub const FILTER_INPUT_ID: i32 = 1021;

// Logical ID for the button used to expand filtered or all items in the TreeView.
pub const FILTER_EXPAND_BUTTON_ID: i32 = 1022;

// Logical ID for the button used to clear the filter input field.
pub const FILTER_CLEAR_BUTTON_ID: i32 = 1023;

// Background color for filter input when active (BGR format).
pub const FILTER_COLOR_ACTIVE: u32 = 0x00FFFFE0; // light yellow
// Background color when no matches are found.
pub const FILTER_COLOR_NO_MATCH: u32 = 0x00E0E0FF; // light red/orange
