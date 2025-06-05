# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

(Earlier steps already completed)

## P2.11: General Cleanup and Refinements

This section details various cleanup tasks, refactorings, and minor enhancements to improve code quality, user experience, and testability.

### P2.11.3: Code Health & Refactoring (Medium Priority)
*   **Consolidate Directory Path Logic:**
    *   **Problem:** `CoreConfigManager::_get_app_config_dir` and `CoreProfileManager::_get_profile_storage_dir` have similar path derivation.
    *   **Action:** Review. Consider a shared private helper for the base application-specific local config directory.
    *   **Rationale:** Minor code deduplication.
    *   **Status:** Potential minor improvement. A shared helper could be introduced.
*   **Ergonomic Access to `Win32ApiInternalState.active_windows`:**
    *   **Problem:** Direct locking of `active_windows` can be verbose.
    *   **Action:** Evaluate creating specialized helper methods on `Win32ApiInternalState` for common `active_windows` access patterns. For example:
        ```rust
        // In Win32ApiInternalState
        pub(crate) fn with_window_data<F, R>(&self, window_id: WindowId, op: F) -> PlatformResult<R>
        where
            F: FnOnce(&window_common::NativeWindowData) -> R,
        { /* ... */ }

        pub(crate) fn with_window_data_mut<F, R>(&self, window_id: WindowId, op: F) -> PlatformResult<R>
        where
            F: FnOnce(&mut window_common::NativeWindowData) -> R,
        { /* ... */ }
        ```
    *   **Rationale:** Can enhance code readability and reduce boilerplate.
    *   **Status:** Potential improvement.

### P2.11.4: Testing Improvements (Medium Priority)
*   **Integration Testing Considerations for `PlatformInterface::main_event_loop()`:**
    *   **Problem:** The main event loop is complex to unit test.
    *   **Action:** Acknowledge as integration testing. Focus on unit tests for `MyAppLogic`, `AppSessionData`, `MainWindowUiState`, and command handlers.
    *   **Rationale:** Balances testing effort.
    *   **Status:** Ongoing strategy.
*   The ui_descrptive_layer builds the UI. There should be some test to assert that the same ID isn't re-used.

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

### P2.11.7: SHould the treeview be createed from ui_descriptive_layer?

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
    1.  **Handle `TVM_GETITEMRECT` Return:** Modify `handle_nm_customdraw_treeview` in `platform_layer/window_common.rs` to treat a 0 return from `SendMessageW` with `TVM_GETITEMRECT` as a failure. Log this appropriately (e.g., DEBUG if expected for non-visible items) and skip drawing the indicator for that item in that pass.
    2.  **Investigate Rectangle & Positioning:**
        *   Thoroughly investigate why `TVM_GETITEMRECT` (with `wParam = FALSE`) sometimes returns a narrow rectangle. This might involve checking the item's state, visibility, and if its parent is expanded.
        *   Adjust the drawing coordinates for the indicator circle to be correctly positioned relative to the item's text. This might involve using `TVM_GETITEMRECT` with `wParam = TRUE` (for text-only rectangle) and calculating an offset, or deriving a suitable offset from the full item rectangle if it can be reliably obtained.
        *   Ensure the indicator is drawn only when the item is actually visible on screen and its state is "New".
*   **Rationale:** Fixes visual glitches with the "New" item indicator, improves UI accuracy by correctly highlighting new files, and removes misleading error messages from the logs. This is crucial for the `[UiTreeViewVisualFileStatusV1]` feature.
*   **Priority:** High
*   **Tag:** `[BugFixTreeViewCustomDrawV1]`

## P3.3: File Content Viewer
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

# Phase 4: Advanced Features (Optional / Future)

## P4.1: File Name Search
*   Add search input. `[UiSearchFileNameFilterTreeV1]`
*   Filter `TreeView`.

## P4.2: Content Search
*   Input for search string. `[UiSearchFileContentHighlightV1]`
*   Button "Search in Selected".
*   Highlight files in tree.
*   (Optional) Show occurrences in File Content Viewer.

## P4.3: Refresh Tree View Button (UI aspect)
*   Ensure "Refresh" button/menu item is accessible. `[UiMenuTriggerScanV1]`
    *   **Note:** Logic is present; this is about UI prominence if needed beyond menu.

## P4.4: "Clear Selection" / "Select All Files" / "Invert Selection" options.

## P4.5: Better Binary File Detection
*   Implement robust check (e.g., percentage of non-printable chars). `[TextFileFocusUTF8V1]`
*   Visually indicate/exclude binary files. `[FutureBinaryFileDetectionSophisticatedV1]`
