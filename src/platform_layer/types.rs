/*
 * This module defines core data types used for communication between the
 * application logic and the platform layer. It includes identifiers for windows
 * and tree items, configurations for UI elements (windows, menus),
 * platform-agnostic event types (`AppEvent`), commands for the platform layer
 * (`PlatformCommand`), severity levels for messages (`MessageSeverity`),
 * and semantic identifiers for menu actions (`MenuAction`). It also defines the
 * `PlatformEventHandler` trait that the application logic must implement.
 */

use std::path::PathBuf;

// An opaque identifier for a native window, managed by the platform layer.
//
// The application logic layer uses this ID to refer to specific windows
// when sending commands or receiving events, without needing to know about
// native window handles like HWND.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) usize);

// An opaque identifier for an item within a tree-like control (e.g., TreeView).
//
// This ID is generated and managed by the application logic layer and used to
// uniquely identify tree items in commands and events. The platform layer
// maps this to native tree item handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeItemId(pub u64);

// --- Semantic Menu Action Identifiers ---

/*
 * Represents logical menu actions in a platform-agnostic way.
 * This enum is used in `MenuItemConfig` and `AppEvent` to identify menu
 * actions semantically, rather than relying on raw `i32` control IDs.
 * The platform layer will manage the mapping from these actions to
 * dynamically assigned native menu item IDs.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    LoadProfile,
    SaveProfileAs,
    SetArchivePath,
    RefreshFileList,
    GenerateArchive,
}

// --- Data Structures for UI Description (Platform-Agnostic) ---

// Configuration for creating a new native window.
//
// Provided by the application logic to the platform layer, describing
// the desired properties of a window without specifying native details.
#[derive(Debug, Clone)]
pub struct WindowConfig<'a> {
    pub title: &'a str,
    pub width: i32,
    pub height: i32,
}

// Represents the visual check state of an item, typically a checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Checked,
    Unchecked,
}

// Describes a single item to be displayed in a tree-like control.
//
// This structure is used by the application logic to define the content
// and hierarchy of a tree view, which the platform layer then renders.
#[derive(Debug, Clone)]
pub struct TreeItemDescriptor {
    pub id: TreeItemId,
    pub text: String,
    pub is_folder: bool,
    pub state: CheckState,
    pub children: Vec<TreeItemDescriptor>,
}

/*
 * Configuration for a single menu item, used by `PlatformCommand::CreateMainMenu`.
 *
 * Describes the properties of a menu item, including an optional semantic `MenuAction`
 * for event handling, its display text, and any sub-menu items. Menu items that
 * are themselves popups (e.g., a "File" menu that opens a submenu) will have `action: None`.
 */
#[derive(Debug, Clone)]
pub struct MenuItemConfig {
    pub action: Option<MenuAction>,
    pub text: String,
    pub children: Vec<MenuItemConfig>, // For submenus
}

// --- Layout Primitives ---

/*
 * Defines how a control should dock within its parent container.
 * This is a simplified docking model. More advanced anchoring or grid systems
 * could be introduced later.
 * TODO: I think many of these have not been implemented yet.
 */
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DockStyle {
    None,   // No docking, control is positioned manually or by other rules.
    Top,    // Docks to the top edge of the container.
    Bottom, // Docks to the bottom edge of the container.
    Left,   // Docks to the left edge of the container.
    Right,  // Docks to the right edge of the container.
    Fill,   // Fills all remaining space in the container (both axes).
    ProportionalFill { weight: f32 }, // Fills space along main axis proportionally with siblings.
}

/*
 * A rule that associates a control (by its ID) with a specific docking style.
 * The `order` field can be used to determine the sequence in which docking
 * calculations are performed (e.g., top/bottom docks first, then left/right, then fill).
 * Lower order values are typically processed first.
 * The `parent_control_id` specifies the logical ID of the parent control; `None` indicates
 * the main window client area is the parent.
 */
#[derive(Debug, Clone)]
pub struct LayoutRule {
    pub control_id: i32,                // The ID of the control this rule applies to.
    pub parent_control_id: Option<i32>, // ID of the parent control, None for main window.
    pub dock_style: DockStyle,
    pub order: u32, // Order of application (e.g., 0 for top, 1 for bottom, 10 for fill)
    pub fixed_size: Option<i32>, // For Top/Bottom, this is height. For Left/Right, this is width. Not used for Fill/None.
    pub margin: (i32, i32, i32, i32), // (top, right, bottom, left) margins around the control.
}

// --- Events from Platform to App Logic ---

/*
 * Represents platform-agnostic UI events generated by the native toolkit.
 *
 * The platform layer translates native OS events into these types and
 * sends them to the application logic layer for handling. Menu item clicks
 * are now generalized into `MenuActionClicked`.
 */
#[derive(Debug)]
pub enum AppEvent {
    WindowCloseRequestedByUser {
        window_id: WindowId,
    },
    // Signals that a window has been resized.
    WindowResized {
        window_id: WindowId,
        width: i32,
        height: i32,
    },
    // Signals that a window and its native resources have been destroyed.
    // The `WindowId` should be considered invalid after this event.
    WindowDestroyed {
        window_id: WindowId,
    },
    TreeViewItemToggledByUser {
        window_id: WindowId,
        item_id: TreeItemId,
        new_state: CheckState,
    },
    // Signals that a button was clicked.
    ButtonClicked {
        window_id: WindowId,
        control_id: i32,
    },
    // Signals that a menu item was clicked, identified by its semantic `MenuAction`.
    MenuActionClicked {
        action: MenuAction,
    },
    // Signals the result of a "Save File" dialog.
    FileSaveDialogCompleted {
        window_id: WindowId,
        result: Option<std::path::PathBuf>,
    },
    FileOpenProfileDialogCompleted {
        window_id: WindowId,
        result: Option<PathBuf>,
    },
    ProfileSelectionDialogCompleted {
        window_id: WindowId,
        chosen_profile_name: Option<String>,
        create_new_requested: bool,
        user_cancelled: bool,
    },
    GenericInputDialogCompleted {
        window_id: WindowId,
        text: Option<String>,
        context_tag: Option<String>,
    },
    FolderPickerDialogCompleted {
        window_id: WindowId,
        path: Option<PathBuf>,
    },
    // Signals that the initial static UI setup for the main window is complete.
    MainWindowUISetupComplete {
        window_id: WindowId,
    },
    // Signals that the user has submitted text in a filter input field. TODO: This should be generalized, for any input field.
    FilterTextSubmitted {
        window_id: WindowId,
        text: String,
    },
}

// Defines the severity of a message to be displayed, e.g., in the status bar.
// Ordered from least to most severe for comparison. `None` clears.
// TODO: 'None' isn't used, is it needed?
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageSeverity {
    None,        // Clears the status, or lowest priority if not explicitly clearing
    Information, // Neutral information
    Warning,     // A warning to the user
    Error,       // An error has occurred
}

// --- Label Classification ---
// TODO: Only 'StatusBar' is currently used, is it needed?
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelClass {
    Default,
    StatusBar,
}

// Represents platform-agnostic commands sent from the application logic to the platform layer.
//
// These commands instruct the platform layer to perform specific actions on
// native UI elements.
// TODO: All commands that create controls should use the same name for this ID. E.g. "control_id".
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlatformCommand {
    SetWindowTitle {
        window_id: WindowId,
        title: String,
    },
    ShowWindow {
        window_id: WindowId,
    },
    CloseWindow {
        window_id: WindowId,
    },
    PopulateTreeView {
        window_id: WindowId,
        control_id: i32, /* New: Logical ID of the TreeView to populate */
        items: Vec<TreeItemDescriptor>,
    },
    UpdateTreeItemVisualState {
        window_id: WindowId,
        control_id: i32, /* New: Logical ID of the TreeView containing the item */
        item_id: TreeItemId,
        new_state: CheckState,
    },
    ShowSaveFileDialog {
        window_id: WindowId,
        title: String,
        default_filename: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    },
    ShowOpenFileDialog {
        window_id: WindowId,
        title: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    },
    ShowProfileSelectionDialog {
        window_id: WindowId,
        available_profiles: Vec<String>,
        title: String,
        prompt: String,
        emphasize_create_new: bool,
    },
    ShowInputDialog {
        window_id: WindowId,
        title: String,
        prompt: String,
        default_text: Option<String>,
        context_tag: Option<String>,
    },
    ShowFolderPickerDialog {
        window_id: WindowId,
        title: String,
        initial_dir: Option<PathBuf>,
    },
    SetControlEnabled {
        window_id: WindowId,
        control_id: i32,
        enabled: bool,
    },
    QuitApplication,

    CreateMainMenu {
        window_id: WindowId,
        menu_items: Vec<MenuItemConfig>,
    },
    CreateButton {
        window_id: WindowId,
        control_id: i32, // The existing logical ID (e.g., ID_BUTTON_GENERATE_ARCHIVE)
        text: String,
        // Position/size will be managed by DefineLayout command.
    },
    CreateTreeView {
        window_id: WindowId,
        control_id: i32, // The logical ID for the TreeView
                         // Position/size will be managed by DefineLayout command.
    },
    // Signals to the platform layer that all initial UI description commands
    // for the main window have been enqueued and processed.
    SignalMainWindowUISetupComplete {
        window_id: WindowId,
    },
    // New command to define layout rules for controls within a window.
    DefineLayout {
        window_id: WindowId,
        rules: Vec<LayoutRule>,
    },
    CreatePanel {
        window_id: WindowId,
        parent_control_id: Option<i32>, // None means child of main window's client area
        control_id: i32,                // Logical ID for this new panel
    },
    CreateLabel {
        window_id: WindowId,
        parent_panel_id: i32,
        control_id: i32,
        initial_text: String,
        class: LabelClass, // Classify labels for potential specific styling
    },
    CreateInput {
        window_id: WindowId,
        parent_control_id: Option<i32>,
        control_id: i32,
        initial_text: String,
    },
    SetInputText {
        window_id: WindowId,
        control_id: i32,
        text: String,
    },
    /// Sets the background color of an input control. None resets to system default.
    SetInputBackgroundColor {
        window_id: WindowId,
        control_id: i32,
        color: Option<u32>,
    },
    UpdateLabelText {
        window_id: WindowId,
        control_id: i32,
        text: String,
        severity: MessageSeverity,
    },
    /// Expands only the currently visible items in a TreeView. Used when a filter is active.
    ExpandVisibleTreeItems {
        window_id: WindowId,
        control_id: i32,
    },
    /// Expands all items in a TreeView regardless of visibility.
    ExpandAllTreeItems {
        window_id: WindowId,
        control_id: i32,
    },
    RedrawTreeItem {
        window_id: WindowId,
        control_id: i32, /* New: Logical ID of the TreeView containing the item */
        item_id: TreeItemId,
    },
}

// --- Trait for App Logic to Handle Events ---

// A trait to be implemented by the application logic layer to handle UI events.
//
// The platform layer calls methods on this trait to notify the application
// logic about user interactions or system events.
pub trait PlatformEventHandler: Send + Sync + 'static {
    // Called by the platform layer when a native UI event has been processed.
    // The implementor should handle the event and enqueue `PlatformCommand`s
    // for the platform layer to execute.
    fn handle_event(&mut self, event: AppEvent);

    // Called by the platform layer when the application is about to exit its main loop.
    // This allows the application logic to perform any necessary cleanup.
    fn on_quit(&mut self) {}

    // Attempts to dequeue a single `PlatformCommand` from the internal queue.
    // This is called by the platform layer's run loop.
    fn try_dequeue_command(&mut self) -> Option<PlatformCommand>;

    /*
     * Queries if a specific tree item is currently in the "New" state.
     * The platform layer uses this during custom drawing to determine if the
     * "New" visual indicator (e.g., a blue circle) should be rendered for the item.
     */
    fn is_tree_item_new(&self, window_id: WindowId, item_id: TreeItemId) -> bool;
}
