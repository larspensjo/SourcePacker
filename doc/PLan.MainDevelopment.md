# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

(Earlier steps already completed)

## P2.11: General Cleanup and Refinements

This section details various cleanup tasks, refactorings, and minor enhancements to improve code quality, user experience, and testability.

### P2.11.5: Future Enhancements & Nice-to-Haves (Low Priority for P2)
*   **Structured Application Settings File:**
    *   **Problem:** `last_profile_name.txt` is limited.
    *   **Action:** Plan to migrate `core::config` to use JSON (e.g., `app_settings.json`) if more settings are anticipated.
    *   **Rationale:** Scalable solution for preferences.
    *   **Status:** Not done (low priority).
*   **Modularity of `handler.rs` and `handler_tests.rs`:**
    *   **Problem:** Files can become large.
    *   **Action:** If `app_logic/handler.rs` becomes unwieldy *after* the M/V/P refactor, consider further sub-modules for specific flows.
    *   **Rationale:** Long-term maintainability.
    *   **Status:** M/V/P was the major step. Further sub-moduling can be considered if files grow excessively.

### P2.11.6: Improved disconnect between UI elements and actions
*   When you click on a button or click on a menu item, the execution flow is different. But it should be harmonized. The event should be made independent of the source.
    * For example, when I changed a regular button into a menu item, I had to change code in types.rs, ui_descrptive_layer, handler.rs

### P2.11.7: Should the treeview be createed from ui_descriptive_layer?

## P2.13: Improved Profile management
*   Implement a proper dialog for profile selection in the platform layer (not a stub).
*   Implement a user-friendly UI for listing, selecting, creating, and deleting profiles within the application.

# Phase 3: Enhancements & UX Improvements

## P3.0: Deselected files and folders
*   Allow marking files as deselected with a visual indicator. New files default to an "unknown" state.
*   Consider UI for this: disabled checkbox, tri-state checkbox (if platform supports easily), or a separate button/context menu to toggle between selected/deselected/unknown.
*   Use `.gitignore` to initialize files to "deselected" rather than hiding them.

## P3.1: Status Bar Finalization
*   Display number of selected files. `[UiStatusBarSelectedFileCountV1]`
*   Display total token count of selected files (optional).
*   Clearly display archive status (e.g., "Archive: Up-to-date"). Color-coding or icons. `[UiNotificationOutdatedArchiveV1]`
*   **Usability:** Ensure status bar updates are immediate and clear.

**## P3.2: Correct TreeView "New" Item Indicator Drawing**
*   **Problem:** The custom drawing logic for indicating "New" files in the TreeView (blue circle) experiences issues:
    *   `TVM_GETITEMRECT` frequently "fails" (returns 0) but `GetLastError()` also returns 0, particularly for items not currently visible or when parents are collapsed. This leads to error logs and the indicator not being drawn.
    *   When `TVM_GETITEMRECT` "succeeds" for some items, it returns a very narrow rectangle, causing the indicator to be drawn incorrectly (e.g., over the item's icon/checkbox area instead of next to the text).
*   **Action:**
    1.  **Handle `TVM_GETITEMRECT` Return:** Modify `handle_nm_customdraw_treeview` in `platform_layer/window_common.rs` (or its new location in `treeview_handler.rs`) to treat a 0 return from `SendMessageW` with `TVM_GETITEMRECT` as a failure. Log this appropriately (e.g., DEBUG if expected for non-visible items) and skip drawing the indicator for that item in that pass.
    2.  **Investigate Rectangle & Positioning:**
        *   Thoroughly investigate why `TVM_GETITEMRECT` (with `wParam = FALSE`) sometimes returns a narrow rectangle. This might involve checking the item's state, visibility, and if its parent is expanded.
        *   Adjust the drawing coordinates for the indicator circle to be correctly positioned relative to the item's text. This might involve using `TVM_GETITEMRECT` with `wParam = TRUE` (for text-only rectangle) and calculating an offset, or deriving a suitable offset from the full item rectangle if it can be reliably obtained.
        *   Ensure the indicator is drawn only when the item is actually visible on screen and its state is "New".
*   **Rationale:** Fixes visual glitches with the "New" item indicator, improves UI accuracy by correctly highlighting new files, and removes misleading error messages from the logs. This is crucial for the `[UiTreeViewVisualFileStatusV1]` feature.
*   **Priority:** High
*   **Tag:** `[BugFixTreeViewCustomDrawV1]`

## P3.3: File Content Viewer (preview)
*   Add a read-only text control. `[UiContentViewerPanelReadOnlyV1]`
*   Load selected file's content into the viewer.

## P3.5: Handling File State Discrepancies Visually
*   When loading a profile or after refresh:
    *   Files in profile but not on disk: Mark visually (e.g., greyed out). `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Files on disk but not in profile: Appear as `FileState::Unknown`. `[FileStateNewUnknownV2]`, `[UiTreeViewVisualFileStatusV1]`

## P3.6: Full Profile Management UI
*   "File -> Manage Profiles..." opens a dedicated dialog. `[UiMenuProfileManagementV1]`
*   Dialog with list, "Create New", "Duplicate", "Delete".
*   "Edit Profile": Allow changing name, root folder (triggers rescan), `archive_path`. `[UiMenuSetRootFolderV1]`
*   "Duplicate Profile" dialog. `[ProfileOpDuplicateExistingV1]`
*   "Delete Profile": Confirm, remove file. If active, trigger profile selection. `[ProfileOpDeleteExistingV1]`

## P3.7: But to expand the whole tree
    *   **(Note: This is partially addressed by the "Expand Filtered/All" button in P4.1. This item can be considered superseded or a refinement if P4.1's button doesn't cover all desired "expand all" scenarios.)**

# Phase 4: Advanced Features (Optional / Future)

## P4.1: Quick File & Folder Filter `[UiQuickFileFilterV1]`

**Goal:** Implement an interactive filter for the TreeView, allowing users to quickly find files and folders by name using simple text and glob patterns.

**User Story:** As a user, I want to type text into a filter box to dynamically narrow down the items shown in the file tree, so I can quickly locate specific files or folders in large projects. I also want to easily expand the filtered results or the entire tree.

**Sub-Phase 4.1.A: Basic UI Elements & Manual Filter Application**
    *   **Action 1.1 (UI Elements):**
        *   Define and create a text input field for filter input. `[UiFilterInputFieldV1]`
            *   **`Plan.UIDescriptiveLayer.md` Ref:** Add `PlatformCommand::CreateInput` (or similar, if a generic text input creation command is needed beyond `CreateLabel`) to `ui_description_layer`.
        *   Define and create an "Expand Filtered/All" button. `[UiFilterExpandButtonV1]`
            *   **`Plan.UIDescriptiveLayer.md` Ref:** Use `PlatformCommand::CreateButton` from `ui_description_layer`.
        *   **Placement:** Input field above the TreeView. Button near the input field or in a small toolbar.
        *   **`Plan.UIDescriptiveLayer.md` Ref:** Define `LayoutRule`s for these new controls in `ui_description_layer`.
    *   **Action 1.2 (App Logic - Filter Text Handling):**
        *   `app_logic` to store the current filter text.
        *   Handle `AppEvent` from the input field (e.g., `FilterTextEntered` if using Enter key, or an initial `TextChanged` if not yet live).
    *   **Action 1.3 (App Logic - Basic Filtering):**
        *   Modify `app_logic`'s TreeView population logic (e.g., `build_tree_item_descriptors_recursive_internal` or equivalent).
        *   When filter text is present:
            *   Iterate through `FileNode`s.
            *   Match filter text exactly (case-insensitive for now) against file/folder names.
            *   If a node matches, include it.
            *   If a node matches, ensure all its parent folders up to the root are also included in the `TreeItemDescriptor` list.
        *   Send updated `TreeItemDescriptor`s via `PlatformCommand::PopulateTreeView`.
    *   **Action 1.4 (App Logic - Expand Button):**
        *   Handle `AppEvent` for the "Expand Filtered/All" button click.
        *   If filter text is active: Implement logic to expand all currently *visible* (filtered) nodes in the TreeView. This might require a new `PlatformCommand::ExpandAllVisibleItems { window_id, control_id }` or iterating through visible items and sending individual `PlatformCommand::ExpandItem` if that exists.
        *   If no filter text: Implement logic to expand the *entire* TreeView (all nodes). (Partially addresses P3.7).
    *   **Verification:** Application is functional. User can type text, (manually trigger filter if not live yet), and see a filtered tree. Expand button works for filtered and full tree.

**Sub-Phase 4.1.B: Live Filtering with Debounce & Clearing Mechanism**
    *   **Action 2.1 (App Logic - Live Filtering & Debounce):**
        *   Modify `app_logic` to apply the filter live as the user types in the filter input field.
        *   Implement a debounce mechanism (e.g., 200-500ms) to prevent excessive `PopulateTreeView` calls during rapid typing.
        *   `AppEvent` for text changes should be frequent; debouncer in `app_logic` controls actual filter application.
    *   **Action 2.2 (UI - Clear Button):**
        *   Define and create an "X" (clear) button, typically positioned inside or next to the filter input field. `[UiFilterClearButtonV1]`
        *   **`Plan.UIDescriptiveLayer.md` Ref:** Use `PlatformCommand::CreateButton` (or a specialized small icon button type if available) and `LayoutRule`s.
    *   **Action 2.3 (App Logic - Clear Functionality):**
        *   Handle `AppEvent` for the "X" button click: Clear filter text in `app_logic`, clear filter text in the UI input field, and re-populate the TreeView with the unfiltered view.
        *   Handle `Esc` key press when the filter input field has focus: Perform the same clear action.
            *   **`Plan.UIDescriptiveLayer.md` Ref:** This might require `platform_layer` to detect focus on the input field and specific key presses, generating a suitable `AppEvent`.
    *   **Verification:** Application is functional. Filtering is live and responsive. "X" button and `Esc` key clear the filter and restore the full tree.

**Sub-Phase 4.1.C: Enhanced Pattern Matching**
    *   **Action 3.1 (App Logic - Implicit Wildcards):**
        *   Modify filter matching logic: If the filter text does *not* contain explicit glob characters (`*`, `?`), treat it as a substring match (e.g., `text` matches `*text*`). Case-insensitivity remains.
    *   **Action 3.2 (App Logic - Explicit Glob Patterns):**
        *   Integrate a glob matching library (like the `glob` crate if suitable, or a simpler custom implementation for `*` and `?`).
        *   If filter text *does* contain glob characters, use glob matching against file/folder names. Case-insensitivity remains.
    *   **Verification:** Application is functional. Users can filter by simple substrings or use `*` and `?` for more complex patterns.

**Sub-Phase 4.1.D: "No Match" Behavior & Visual Cues (Nice-to-Haves)**
    *   **Action 4.1 (App Logic - State for "No Match"):**
        *   When a filter is applied, if it results in at least one match, `app_logic` caches the generated `TreeItemDescriptor`s (the "last successful filter result").
        *   If a subsequent filter application yields *no* matches:
            *   `app_logic` sends the *cached* `TreeItemDescriptor`s from the last successful filter to `PopulateTreeView`.
            *   `app_logic` also signals (e.g., via a new state flag or specific `PlatformCommand`) that the current text is a "no match" situation.
    *   **Action 4.2 (UI - Visual Cue for "No Match"):**
        *   When in a "no match" state:
            *   Change the background color of the filter input field (e.g., light red/orange).
            *   Optionally, display a small, temporary text message near the filter (e.g., "No matches for '[filter text]'").
            *   **`Plan.UIDescriptiveLayer.md` Ref:** May need `PlatformCommand::SetControlAppearance { control_id, property, value }` for input field color. The message could be an existing label updated or a new temporary one.
    *   **Action 4.3 (UI - Visual Cue for "Filter Active"):**
        *   When the filter input field contains text and a filter is actively applied (and it's *not* a "no match" situation):
            *   Change the background color of the filter input field to a subtle indicator (e.g., light yellow) or change its border.
            *   Alternatively, or in addition, show a small persistent label "Filter active".
    *   **Action 4.4 (App Logic - Clearing Cues):**
        *   Ensure all "no match" and "filter active" visual cues are reset when the filter is cleared or when a new filter yields matches.
    *   **Verification:** Application is functional. "No match" situations are handled gracefully by showing last good results with clear indication. "Filter active" state is visually distinct.

## P4.2: Content Search `[UiSearchFileContentHighlightV1]`
*   Input for search string.
*   Button "Search in Selected".
*   Highlight files in tree.
*   (Optional) Show occurrences in File Content Viewer.

## P4.3: Refresh Tree View Button (UI aspect) `[UiMenuTriggerScanV1]`
*   Ensure "Refresh" button/menu item is accessible.
    *   **Note:** Logic is present; this is about UI prominence if needed beyond menu.

## P4.4: "Clear Selection" / "Select All Files" / "Invert Selection" options.

## P4.5: Better Binary File Detection `[TextFileFocusUTF8V1]`
*   Implement robust check (e.g., percentage of non-printable chars).
*   Visually indicate/exclude binary files. `[FutureBinaryFileDetectionSophisticatedV1]`

## P4.6: Graphical effect on status change
*   When something in the status field changes, I want there to be a visual effect that gradually fades away.
