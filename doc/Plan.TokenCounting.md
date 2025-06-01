# Token Counting Implementation Plan for SourcePacker

**Goal:** Integrate token counting functionality into SourcePacker, allowing users to see an estimated token count for their selected files. The count should update dynamically and eventually be displayed in a dedicated part of the status bar.

**Core Requirements:**
*   `[TokenCountEstimateSelectedV1]`: Display estimated token count for selected files.
*   `[TokenCountLiveUpdateV1]`: Count updates live as files are selected/deselected.

**Guiding Principle:** The application must remain fully functional after each step.

---

# Phase 5: Multi-Element Status Bar with Dedicated Token Count Display (Revised)

**Goal:** Re-architect the status bar to be a composite area containing multiple distinct graphical elements (e.g., text labels for general status, archive status, token count). These elements will be managed by a layout system using generic platform primitives. The application must remain fully functional after each sub-step of this phase.

**Assumptions:**
*   The token count value is correctly calculated and available in `AppSessionData`.
*   `MyAppLogic` (Presenter) can provide the necessary strings for each status bar element.
*   Logical Control IDs for the new status bar elements will be defined (e.g., in a shared constants module or within `app_logic`) and used by the `ui_description_layer` and `app_logic`.

**High-Level Approach:**
The current single `STATIC` control (old `ID_STATUS_BAR_CTRL`) used for the status bar will be replaced. The `ui_description_layer` will define a new "status bar area" by requesting the platform layer to create a generic panel. Within this panel, it will request the creation of generic labels for each piece of information.

**Define Logical Control IDs (e.g., in `src/app_logic/ui_constants.rs` or similar):**
```rust
// Example: src/app_logic/ui_constants.rs
pub const STATUS_BAR_PANEL_ID: i32 = 1010; // Arbitrary, unique logical ID
pub const STATUS_LABEL_GENERAL_ID: i32 = 1011;
pub const STATUS_LABEL_ARCHIVE_ID: i32 = 1012;
pub const STATUS_LABEL_TOKENS_ID: i32 = 1013;
// (Optional) STATUS_SEPARATOR_FOO_ID: i32 = 1014;
```
*(These constants would be used by both `ui_description_layer` and `app_logic::handler`)*

---

**Step 5.A: Define New Generic Commands and Types (Platform Layer & Types)**
*   **Goal:** Introduce new *generic* `PlatformCommand` variants for creating panels and labels, and updating labels, without breaking existing functionality. These commands will use logical `i32` control IDs.
*   **Action:**
    1.  In `platform_layer/types.rs`:
        *   Add new `PlatformCommand` variants:
            *   `CreatePanel { window_id: WindowId, parent_control_id: Option<i32>, panel_id: i32 }`
                *   `parent_control_id`: Logical ID of the parent control (e.g., another panel). `None` means child of the main window's client area.
                *   `panel_id`: Logical ID for this new panel.
            *   `CreateLabel { window_id: WindowId, parent_panel_id: i32, label_id: i32, initial_text: String }`
                *   `parent_panel_id`: Logical ID of the panel this label is a child of.
                *   `label_id`: Logical ID for this new label.
            *   `UpdateLabelText { window_id: WindowId, label_id: i32, text: String, severity: MessageSeverity }`
            *   (Optional) `CreateSeparator { window_id: WindowId, parent_panel_id: i32, separator_id: i32, orientation: SeparatorOrientation }` (assuming `SeparatorOrientation` enum: `Horizontal`, `Vertical`)
    2.  Do **not** remove `PlatformCommand::UpdateStatusBarText` or constants related to the old `ID_STATUS_BAR_CTRL` yet.
*   **Functionality Check:** Application compiles and runs as before. The old status bar (`ID_STATUS_BAR_CTRL`) continues to function normally.

**Step 5.B: Implement Generic Panel and Label Creation/Update Logic (Platform Layer)**
*   **Goal:** Add platform-level support for creating and managing generic panels and labels based on the new commands. The platform layer maps the provided logical IDs to `HWND`s.
*   **Action:**
    1.  In `platform_layer/command_executor.rs`:
        *   Implement `execute_create_panel`:
            *   Gets parent `HWND` (either main window's `HWND` if `parent_control_id` is `None`, or `HWND` of `parent_control_id` from `NativeWindowData.controls`).
            *   Creates a `STATIC` control (e.g., with `SS_NOTIFY` if it needs to propagate clicks, or a simple one if just a container) to act as the panel, child to parent `HWND`.
            *   Stores its `HWND` in `NativeWindowData.controls` mapped by the provided `panel_id` (the logical ID).
        *   Implement `execute_create_label`:
            *   Gets parent panel's `HWND` using `parent_panel_id` from `NativeWindowData.controls`.
            *   Creates a `STATIC` control as a child of the parent panel's `HWND`.
            *   Stores its `HWND` in `NativeWindowData.controls` mapped by the provided `label_id`. Sets its initial text.
        *   (Optional) Implement `execute_create_separator`.
    2.  In `platform_layer/window_common.rs` (`NativeWindowData` struct):
        *   Add `label_severities: HashMap<i32, MessageSeverity>` (key is the logical `label_id`).
    3.  In `platform_layer/command_executor.rs`:
        *   Implement `execute_update_label_text`:
            *   Retrieves the label's `HWND` using `label_id` from `NativeWindowData.controls`.
            *   Uses `SetWindowTextW` to update its text.
            *   Updates `NativeWindowData.label_severities[&label_id]` with the new `severity`.
            *   Calls `InvalidateRect` on the label's `HWND` to trigger repaint for `WM_CTLCOLORSTATIC`.
*   **Functionality Check:** Application compiles. No visible changes to the UI yet. The old status bar continues to function normally.

**Step 5.C: Define and Layout New Status Bar Elements (UI Description & Platform Layer - Window Resizing)**
*   **Goal:** Use the new generic commands to define the structure of the new status bar. Instantiate the panel and labels. Implement their layout. The new elements will co-exist with the old status bar.
*   **Action:**
    1.  In `ui_description_layer/mod.rs` (`build_main_window_static_layout` function):
        *   Use the logical IDs defined earlier (e.g., `ui_constants::STATUS_BAR_PANEL_ID`).
        *   Queue `PlatformCommand::CreatePanel` for `STATUS_BAR_PANEL_ID` (e.g., `parent_control_id: None`).
        *   Queue `PlatformCommand::CreateLabel` commands for `STATUS_LABEL_GENERAL_ID`, `STATUS_LABEL_ARCHIVE_ID`, `STATUS_LABEL_TOKENS_ID`. Their `parent_panel_id` will be `STATUS_BAR_PANEL_ID`. Provide brief initial text.
    2.  In `ui_description_layer/mod.rs` (`build_main_window_static_layout`), modify the `PlatformCommand::DefineLayout` command:
        *   Add a `LayoutRule` for `STATUS_BAR_PANEL_ID`. This rule should specify `DockStyle::Bottom`, an `order` (e.g., `-1` if old `ID_STATUS_BAR_CTRL` has order `0`, to place new panel *above* old bar initially, or simply ensure they are distinct using different orders), and `fixed_size: Some(STATUS_BAR_HEIGHT)`.
        *   The existing `LayoutRule` for the old `ID_STATUS_BAR_CTRL` remains unchanged.
    3.  In `platform_layer/window_common.rs` (`handle_wm_size` function):
        *   When `handle_wm_size` processes its `LayoutRule`s, if it resizes the control with logical ID `ui_constants::STATUS_BAR_PANEL_ID`:
            *   Get its `HWND` and client `RECT`.
            *   Iterate through the known new status label logical IDs (`ui_constants::STATUS_LABEL_GENERAL_ID`, etc.).
            *   For each label, get its `HWND` (from `window_data.controls[&label_id]`) and position it using `MoveWindow`.
            *   Implement a simple horizontal flow within the panel's client `RECT` (e.g., general status left, archive middle, tokens right; using percentages like 50%, 25%, 25% of panel width, or calculated fixed widths). **This specific layout logic for the children of `STATUS_BAR_PANEL_ID` will reside in `handle_wm_size` as a special case for this panel ID.**
*   **Functionality Check:** Application compiles. The new status bar panel and its labels should now be visible, likely appearing alongside or slightly overlapping the old status bar (depending on layout order). New labels display initial static text. Old status bar (`ID_STATUS_BAR_CTRL`) continues to update.

**Step 5.D: Implement Styling for New Labels and Transition General Messages (Presenter & Platform Layer - WM_CTLCOLORSTATIC)**
*   **Goal:** Enable text styling for new labels. Start routing general status messages to the new general label, *while also keeping updates to the old status bar*.
*   **Action:**
    1.  In `platform_layer/window_common.rs` (`WndProc` function, `WM_CTLCOLORSTATIC` handler):
        *   Get the control ID of `hwnd_static_ctrl_from_msg` (e.g., using `GetDlgCtrlID`).
        *   If this ID matches one of the new logical label IDs (e.g., `ui_constants::STATUS_LABEL_GENERAL_ID`):
            *   Look up its severity from `NativeWindowData.label_severities[&id]`.
            *   Set text color via `SetTextColor` based on severity.
            *   The old logic for `ID_STATUS_BAR_CTRL` remains for now.
    2.  In `app_logic/handler.rs`:
        *   Modify `status_message!` macro:
            *   Queue `PlatformCommand::UpdateLabelText` for `ui_constants::STATUS_LABEL_GENERAL_ID` with text and severity.
            *   *Still* queue old `PlatformCommand::UpdateStatusBarText` for `ID_STATUS_BAR_CTRL`.
*   **Functionality Check:** General status messages appear in the new general label with correct color. They also appear in the old status bar.

**Step 5.E: Transition Archive and Token Messages to New Labels (Presenter)**
*   **Goal:** Route archive and token updates to new labels, *while also keeping updates to the old status bar*.
*   **Action:**
    1.  In `app_logic/handler.rs` (e.g., `update_current_archive_status`):
        *   In addition to old status bar logic, queue `PlatformCommand::UpdateLabelText` for `ui_constants::STATUS_LABEL_ARCHIVE_ID`.
    2.  In `app_logic/handler.rs` (e.g., `_update_token_count_and_request_display`):
        *   In addition to old status bar logic, queue `PlatformCommand::UpdateLabelText` for `ui_constants::STATUS_LABEL_TOKENS_ID`.
*   **Functionality Check:** Archive/token updates appear in new labels *and* old status bar. General messages continue to update both.

**Step 5.F: Finalize New System and Remove Old Status Bar (Presenter, Platform Layer, UI Description)**
*   **Goal:** Remove all code related to the old single-string status bar (`ID_STATUS_BAR_CTRL`), making the new multi-element status bar the sole provider.
*   **Action:**
    1.  In `app_logic/handler.rs`:
        *   `status_message!`: Remove queuing of `UpdateStatusBarText`. Only queue `UpdateLabelText` for `ui_constants::STATUS_LABEL_GENERAL_ID`.
        *   Archive/Token updates: Remove logic for old status bar. Only queue `UpdateLabelText` for their respective new label IDs.
    2.  In `platform_layer/types.rs`:
        *   Remove `PlatformCommand::UpdateStatusBarText`.
        *   Remove `PlatformCommand::CreateStatusBar` (the one that took `ID_STATUS_BAR_CTRL`).
    3.  In `platform_layer/command_executor.rs`:
        *   Remove `execute_update_status_bar_text`.
        *   Remove `execute_create_status_bar`.
    4.  In `platform_layer/window_common.rs` (`NativeWindowData` struct):
        *   Remove `status_bar_current_text` and `status_bar_current_severity` fields (related to the old single status bar). The `label_severities` map remains.
    5.  In `platform_layer/window_common.rs` (`WndProc`, `WM_CTLCOLORSTATIC` handler):
        *   Remove the specific logic block for `ID_STATUS_BAR_CTRL`. Logic for new labels (using `label_severities` map and their logical IDs) remains.
    6.  In `ui_description_layer/mod.rs` (`build_main_window_static_layout`):
        *   Remove the `PlatformCommand::CreateStatusBar` that created the old status bar (`ID_STATUS_BAR_CTRL`).
        *   Remove the `LayoutRule` for `ID_STATUS_BAR_CTRL`.
        *   Adjust `LayoutRule` for `ui_constants::STATUS_BAR_PANEL_ID`'s `order` if needed (e.g., ensure it's `order: 0` or appropriate for the primary bottom-docked item).
*   **Functionality Check:** Application compiles and runs. Only the new multi-element status bar (panel with labels) is visible. General, archive, and token messages update correctly in their respective labels with appropriate styling. The old status bar is gone.

**Step 5.G: Testing (Ongoing throughout the phase)**
*   **Platform Layer:**
    *   After 5.B: Test processing of `CreatePanel`, `CreateLabel`, `UpdateLabelText`.
    *   After 5.C: Verify new panel and labels are created and laid out correctly (alongside old bar). Test resizing and internal layout of panel children.
    *   After 5.D: Test `UpdateLabelText` changes text and `WM_CTLCOLORSTATIC` applies styling to new labels.
    *   After 5.F: Retest layout of elements within the panel during resize, ensuring only new system is active.
*   **Application Logic (`handler_tests.rs`):**
    *   After 5.D: Verify `status_message!` queues `UpdateLabelText` for the general label (with its logical ID) *and still* queues old `UpdateStatusBarText`.
    *   After 5.E: Verify archive/token updates queue `UpdateLabelText` for their respective labels (with logical IDs) *and still* contribute to old `UpdateStatusBarText`.
    *   After 5.F: Verify `MyAppLogic` now *only* queues the correct `UpdateLabelText` commands with the appropriate logical `label_id`s, text, and severity.
*   **Manual UI Testing (after each relevant step):**
    *   5.C: Confirm new panel and labels are visible. Old bar still updates.
    *   5.D: Confirm general messages update in new general label (with color) AND old bar.
    *   5.E: Confirm archive/token messages update in new labels AND old bar.
    *   5.F: Confirm only new status bar elements are visible and correctly positioned/laid out. Confirm they update independently and correctly. Confirm error/warning styling applies. Check window resize behavior.

---

## Phase 6: Future Enhancements (Optional)

*   **Asynchronous Token Counting:** For very large selections or complex tokenizers, move token counting to a background thread to prevent UI freezes. This would involve `MyAppLogic` spawning a task, the task reading files and counting tokens, and then sending an `AppEvent` back to `MyAppLogic` with the result.
    *   **Note:** The caching from Phase 3 significantly reduces the *frequency* of full token calculations. Async would primarily benefit the initial cache population (Step 3.3, 3.4) for many new/changed files.
*   **Configurable Tokenizer Model:** Allow users to select different `tiktoken-rs` models (e.g., via a settings dialog). `MyAppLogic` would store the selected model name, and `tokenizer_utils` would need to be adapted to use it.
*   **Display Token Count Per File:**
    *   With Phase 3, `MyAppLogic` can query `AppSessionData` for a specific file's cached token count (from `profile.file_details`) if its current `FileNode.checksum` matches the cached one.
    *   Display this in the "File Content Viewer" panel (P3.3) or as a tooltip.
*   **Error Handling for File Reads in UI:** Improve robustness if many files fail to read during token counting (e.g., display "Tokens: X (Y files failed to read / Z cache misses)" in the status bar part).
*   **Action (If using `tiktoken-rs`):** Modify `tokenizer_utils.rs` to use `lazy_static` or `once_cell` (add to `Cargo.toml`) for the `CoreBPE` instance. This avoids re-initializing the BPE model on every `estimate_tokens_tiktoken` call.
    *   Verification: Project compiles and runs. Performance for token counting should be improved, especially if called frequently.
