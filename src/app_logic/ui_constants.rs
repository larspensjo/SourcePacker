/*
 * Defines shared constants for logical UI control identifiers.
 * These IDs are used by the `ui_description_layer` to define the initial UI
 * structure and by the `app_logic` (Presenter) to target specific controls
 * for dynamic updates. The `platform_layer` maps these logical IDs to native
 * UI element handles.
 */

use crate::platform_layer::types::ControlId;

// Logical ID for the main panel that will contain all status bar elements.
pub const STATUS_BAR_PANEL_ID: ControlId = ControlId::new(1010);

// Logical ID for the label displaying general status messages.
pub const STATUS_LABEL_GENERAL_ID: ControlId = ControlId::new(1011);

// Logical ID for the label displaying archive-related status.
pub const STATUS_LABEL_ARCHIVE_ID: ControlId = ControlId::new(1012);

// Logical ID for the label displaying token count information.
pub const STATUS_LABEL_TOKENS_ID: ControlId = ControlId::new(1013);

// --- Panel IDs ---
pub const MAIN_BACKGROUND_PANEL_ID: ControlId = ControlId::new(1000);

// Logical ID for the main TreeView control.
pub const ID_TREEVIEW_CTRL: ControlId = ControlId::new(1001); // Value from platform_layer::control_treeview

// Unicode filled circle appended to "New" tree items to make the state obvious. [FileSelStateNewV2]
pub const NEW_ITEM_INDICATOR_CHAR: char = '●';

// Logical ID for the panel that will contain filter input and buttons.
pub const FILTER_PANEL_ID: ControlId = ControlId::new(1020);

// Logical ID for the text input field used for filtering the TreeView.
pub const FILTER_INPUT_ID: ControlId = ControlId::new(1021);

// Logical ID for the button used to expand filtered or all items in the TreeView.
pub const FILTER_EXPAND_BUTTON_ID: ControlId = ControlId::new(1022);

// Logical ID for the button used to clear the filter input field.
pub const FILTER_CLEAR_BUTTON_ID: ControlId = ControlId::new(1023);
