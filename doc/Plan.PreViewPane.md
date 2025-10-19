# Preview pane

### Pre-computation: Updated and New Requirements

First, let's define the new requirements and update existing ones that will guide our implementation.

**New Requirement:**

*   `[UiContentViewerPanelReadOnlyV1]` A read-only panel shall be present in the main window to display the content of the currently selected file from the tree view.

**Updated Requirements:**

*   `[UiTreeViewDisplayStructureV2]` -> `[UiTreeViewDisplayStructureV3]`
    *   (Add) The TreeView must visually indicate the single item currently selected for viewing, distinct from its checkbox state. This is typically done with a row highlight.
*   `[FileSelStateSelectedV2]` -> `[FileSelStateSelectedV3]`
    *   (Clarify) Clicking on an item's **checkbox** exclusively toggles its "Selected" or "Deselected" state for archive inclusion.
    *   (Add) Clicking on an item's **text label** exclusively selects it for viewing in the content panel and does not affect its checkbox state.

---

Here is the detailed, step-by-step plan:

### Step 1: Add the Viewer UI Shell

**Goal:** Add the new, empty viewer panel to the main window layout. The application will look different, but its existing functionality will be unchanged.

1.  **Create a New Control ID:**
    *   In `src/app_logic/ui_constants.rs`, add a new constant for the viewer panel. An `EDIT` control is suitable as it's designed for displaying multi-line text.
    ```rust
    // Logical ID for the read-only file content viewer.
    pub const ID_VIEWER_EDIT_CTRL: ControlId = ControlId::new(1030);
    ```

2.  **Update the UI Layout:**
    *   In `src/ui_description_layer.rs`, modify `build_main_window_static_layout`:
    *   Add a `PlatformCommand::CreateInput` command to create the viewer. You'll need to extend the `CreateInput` command or create a new one (e.g., `CreateReadOnlyViewer`) to support the necessary styles for a multi-line, read-only edit control (`ES_MULTILINE`, `ES_READONLY`, `WS_VSCROLL`).
    *   Update the `DefineLayout` command. Change the layout rules to create a two-pane view. A good approach is to make the `ID_TREEVIEW_CTRL` dock to the left with a fixed or proportional width, and the new `ID_VIEWER_EDIT_CTRL` dock to fill the remaining space.

    **Example Layout Rule Change:**
    ```rust
    // In the layout_rules vector:
    // Change TreeView from DockStyle::Fill to DockStyle::Left
    LayoutRule {
        control_id: ui_constants::ID_TREEVIEW_CTRL,
        parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
        dock_style: DockStyle::Left, // Was Fill
        order: 10,
        fixed_size: Some(300), // Give it a starting width of 300 pixels
        margin: (0, 2, 0, 0),
    },
    // Add a new rule for the viewer
    LayoutRule {
        control_id: ui_constants::ID_VIEWER_EDIT_CTRL,
        parent_control_id: Some(ui_constants::MAIN_BACKGROUND_PANEL_ID),
        dock_style: DockStyle::Fill, // Fills the rest of the space
        order: 11,
        fixed_size: None,
        margin: (0, 0, 0, 0),
    },
    ```

**Result After Step 1:** The application will launch and run normally. The main window will now have a TreeView on the left and a new, empty, non-functional text area on the right.

---

### Step 2: Implement TreeView Selection and Highlighting

**Goal:** Make the TreeView respond to clicks on item labels by highlighting the row, without changing the checkbox.

1.  **Define New Event and Command:**
    *   In `src/platform_layer/types.rs`:
        *   Add a new event: `AppEvent::TreeViewItemSelectionChanged { window_id: WindowId, item_id: TreeItemId }`.
        *   Add a new command: `PlatformCommand::SetTreeViewSelection { window_id: WindowId, control_id: ControlId, item_id: TreeItemId }`.

2.  **Enhance Platform Layer (TreeView Handler):**
    *   In `src/platform_layer/controls/treeview_handler.rs`, modify `handle_nm_click`.
    *   The existing logic already checks if a click was on a checkbox (`TVHT_ONITEMSTATEICON`). Extend it: if a click occurred on an item but **not** on the state icon, it means the label was clicked.
    *   When a label click is detected, get the `TreeItemId` and send the new `AppEvent::TreeViewItemSelectionChanged` to the application logic.
    *   Implement the handler for the new `PlatformCommand::SetTreeViewSelection`. It will use the `TVM_SELECTITEM` message with the `TVGN_CARET` flag to programmatically set the TreeView's built-in highlight.

3.  **Update Application Logic (`MyAppLogic`):**
    *   In `src/app_logic/main_window_ui_state.rs`, add a new field to `MainWindowUiState` to track the selected item: `active_viewer_item_id: Option<TreeItemId>`.
    *   In `src/app_logic/handler.rs`, implement the handler for `AppEvent::TreeViewItemSelectionChanged`.
    *   In this handler:
        *   Update the `active_viewer_item_id` in `MainWindowUiState`.
        *   Issue the `PlatformCommand::SetTreeViewSelection` command to visually update the highlight in the UI.

**Result After Step 2:** The application is still fully functional. Now, when you click on a file or folder name in the TreeView, the row will become highlighted. Clicking the checkbox still only toggles the check state. The right-hand pane is still empty.

**Unit Test to Add:**

*   In `src/app_logic/handler_tests.rs`, create a new test: `test_treeview_item_selection_changed_updates_state_and_issues_command`.
    *   **Arrange:** Set up `MyAppLogic` and its UI state.
    *   **Act:** Send a mock `AppEvent::TreeViewItemSelectionChanged` with a test `TreeItemId`.
    *   **Assert:**
        1.  Verify that `ui_state.active_viewer_item_id` has been updated to the test ID.
        2.  Verify that a `PlatformCommand::SetTreeViewSelection` was enqueued with the correct `item_id`.

---

### Step 3: Load and Display File Content

**Goal:** Connect the selection event to the viewer pane, making it display the content of the selected file.

1.  **Define New Command:**
    *   In `src/platform_layer/types.rs`, add: `PlatformCommand::SetViewerContent { window_id: WindowId, control_id: ControlId, text: String }`.

2.  **Implement Command in Platform Layer:**
    *   The `SetViewerContent` command will be handled by sending a `WM_SETTEXT` message to the `HWND` of the viewer control (`ID_VIEWER_EDIT_CTRL`).

3.  **Enhance Application Logic:**
    *   In `src/app_logic/handler.rs`, expand the `handle_event` for `TreeViewItemSelectionChanged`.
    *   After updating the state and issuing the highlight command (from Step 2), add this logic:
        1.  Use the `item_id` to find the corresponding `PathBuf` from the `path_to_tree_item_id` map in `MainWindowUiState`.
        2.  Check the corresponding `FileNode` in your `file_system_snapshot_nodes` to see if it's a file (not a directory).
        3.  If it's a file, use `std::fs::read_to_string()` to read its content.
        4.  Issue the new `PlatformCommand::SetViewerContent` with the file's content.

**Result After Step 3:** The feature is now functional for text files. Clicking a file name in the TreeView displays its content in the right-hand pane. Clicking a folder does nothing to the pane yet.

**Unit Test to Add:**

*   Extend the test from Step 2, or create a new one: `test_select_text_file_loads_content_into_viewer`.
    *   **Arrange:** Create a temporary text file with known content. Set up the mocks so that the `TreeItemId` from the event maps to this temporary file's path.
    *   **Act:** Send the `AppEvent::TreeViewItemSelectionChanged` event.
    *   **Assert:** Verify that a `PlatformCommand::SetViewerContent` command was enqueued and that its `text` payload matches the known content of the temporary file.

---

### Step 4: Handle Edge Cases (Folders & Binary Files)

**Goal:** Make the viewer behave gracefully when a folder or a non-text file is selected.

1.  **Update Application Logic (`MyAppLogic`):**
    *   In `src/app_logic/handler.rs`, further enhance the `TreeViewItemSelectionChanged` handler:
    *   **Folder Handling:** Before trying to read a file, check `FileNode.is_dir()`. If `true`, issue a `SetViewerContent` command with an empty string or a helpful placeholder message (e.g., "Select a file to view its content.").
    *   **Binary File Handling:** When you call `std::fs::read_to_string()`, it will return an `Err` for non-UTF-8 files. Catch this error. When it occurs, issue a `SetViewerContent` command with a message like "Cannot display binary file content."

**Result After Step 4:** The feature is now robust. The viewer provides appropriate feedback for all item types in the TreeView.

**Unit Tests to Add:**

*   `test_select_folder_clears_viewer_and_shows_placeholder`:
    *   **Arrange:** Mock a selection event for a `TreeItemId` that maps to a directory.
    *   **Assert:** Verify a `SetViewerContent` command is sent with the placeholder text.
*   `test_select_binary_file_shows_binary_message`:
    *   **Arrange:** Create a temporary file with non-UTF-8 bytes. Mock a selection event for it.
    *   **Assert:** Verify a `SetViewerContent` command is sent with the "Cannot display binary file" message.

This incremental plan builds the feature layer by layer, keeping the application stable at each stage and incorporating testing along the way.
