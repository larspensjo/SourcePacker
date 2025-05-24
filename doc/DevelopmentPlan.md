# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

(Earlier steps already completed)

## P2.9: Refresh File List Action
*   Add a "Refresh" button or menu item. `[UiMenuTriggerScanV1]`
*   On action:
    *   Re-run `scan_directory` (P1.2) for the current profile's root. `[FileSystemMonitorTreeViewV1]`
    *   Update the internal `FileNode` tree.
    *   Apply `profile.selected_paths` and `profile.deselected_paths` to the new tree. Newly discovered files (not in profile's selection sets) become `FileState::Unknown`. `[FileStateNewUnknownV2]`
    *   Re-populate the TreeView (P2.1), ensuring new "Unknown" files are visually distinct if possible, or at least correctly reflect their state. `[UiTreeViewVisualSelectionStateV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Run `check_archive_status` (P1.6). `[ArchiveSyncNotifyUserV1]`
    *   Update status bar and any other relevant UI.
*   Integrate basic archive status display into the Status Bar (P3.1 conceptually). `[UiNotificationOutdatedArchiveV1]`
    *   Possible messages: "Archive: Up-to-date", "Archive: Needs Update", "Archive: Not Generated", "Archive: File Missing", "Archive: No Files Selected", "Archive: Error Checking".

## P2.10: AppEvent.Execute Refactoring Discussion
*   Maybe the MyAppLogic::handle_event should call event.Execute(&mut commands, &self)? That would take away almost all code from handle_event.

## P2.11: General Cleanup and Refinements


This section details various cleanup tasks, refactorings, and minor enhancements to improve code quality, user experience, and testability.

Implemented sections have been removed.

### P2.11.1
*   app_logic.on_quit is called directly from platform_layer. That is backwards, I don't likt it.
*   The handle_window_destroyed clears most of the state. Later on, on_quit() tries to save the state that has now been cleared. The saving of the state should maybe be done before handle_window_destroyed?

### P2.11.2: Core Logic Correctness & User Experience (High Priority)
*   **User-Friendly Error Reporting from `MyAppLogic`:** `[TechErrorHandlingGracefulV1]`
    *   **Problem:** Errors in `MyAppLogic` are logged to console (`eprintln!`) but not shown to the user in the UI.
    *   **Action:** Replace `eprintln!` calls in `MyAppLogic` that represent user-facing errors or important warnings with `PlatformCommand::UpdateStatusBarText` (or a future error dialog command). Use appropriate `MessageSeverity`.
    *   **Rationale:** Essential for user feedback and a more polished application.
*   **Accurate Profile State Update:**
    *   **Problem:** `MyAppLogic.current_profile_name` and `current_profile_cache` might be updated prematurely before a profile save operation is fully confirmed.
    *   **Action:** Ensure `self.current_profile_name` and `self.current_profile_cache` in `MyAppLogic` are updated *only after* a successful save operation (e.g., after creating a new profile or saving an existing one under a new name/path).
    *   **Rationale:** Maintains consistent internal state with persisted state, preventing discrepancies.
*   **Correct Application Data Storage Location:** `[ProfileStoreAppdataLocationV1]`
    *   **Problem:** Configuration data might be stored in a non-standard or less appropriate user directory (e.g., Roaming instead of Local).
    *   **Action:** Verify and ensure `CoreConfigManager` and `CoreProfileManager` use the platform-appropriate local application data directory (e.g., `AppData\Local` on Windows via `ProjectDirs::config_local_dir()` or `data_local_dir()`).
    *   **Code Check:** This appears to be correctly implemented.
    *   **Rationale:** Adherence to operating system guidelines for application data storage.
*   **Centralized Profile Name Validation:**
    *   **Problem:** Profile name validation logic might be duplicated between `core::profiles` and `MyAppLogic`.
    *   **Action:** Ensure `MyAppLogic` uses the public `core::profiles::is_valid_profile_name_char` function (or a similar centralized validator) for validating profile names entered by the user.
    *   **Code Check:** This appears to be correctly implemented.
    *   **Rationale:** Code deduplication, consistency in validation rules, and easier maintenance.

### P2.11.3: Code Health & Refactoring (Medium Priority)
*   **Modularity of `MyAppLogic` Handlers:** `[TechModularityLogicalModulesV1]`
    *   **Problem:** Event handlers or helper functions within `app_logic/handler.rs` (like `_activate_profile_and_show_window`) might become too large and complex.
    *   **Action:** Review large methods in `MyAppLogic`. If they exceed reasonable complexity or line count, refactor them into smaller, more focused private helper methods.
    *   **Rationale:** Improves readability, maintainability, and unit testability of the application logic.
*   **Modularity of Platform Message Handlers:**
    *   **Problem:** The main window procedure in `platform_layer/window_common.rs` or `platform_layer/app.rs` (`handle_window_message`) can become a large switch/match statement.
    *   **Action:** Continue the pattern of delegating specific `WM_` message handling to dedicated private methods within `Win32ApiInternalState`. Ensure these individual handlers also remain focused.
    *   **Rationale:** Improves organization and maintainability of platform-specific code.
*   **Consolidate Directory Path Logic:**
    *   **Problem:** `CoreConfigManager::_get_app_config_dir` and `CoreProfileManager::_get_profile_storage_dir` have similar starting points for path derivation.
    *   **Action:** Review these methods. Consider a shared private helper that returns the base application-specific local config directory. Each manager can then append its specific subfolder or filename.
    *   **Rationale:** Minor code deduplication and improved clarity in path construction.
*   **Ergonomic Access to `Win32ApiInternalState.window_map`:**
    *   **Problem:** Direct locking and access to `window_map` can be verbose or potentially error-prone if locks aren't managed carefully.
    *   **Action:** Evaluate if creating specialized helper methods on `Win32ApiInternalState` for common `window_map` access patterns (e.g., retrieving specific window data or tree view state) would improve code clarity and centralize lock management for those operations.
    *   **Rationale:** Can enhance code readability and reduce boilerplate for accessing window-specific data.

### P2.11.4: Testing Improvements (Medium Priority)
*   **Enhance Test Isolation for `on_main_window_created`:**
    *   **Problem:** Tests for `MyAppLogic::on_main_window_created` might not be fully isolated if mock configurations don't cover all interaction paths with `ConfigManagerOperations` and `ProfileManagerOperations`.
    *   **Action:** Review tests in `handler_tests.rs` for `on_main_window_created`. Ensure that `MockConfigManager` and `MockProfileManager` are always configured with specific return values for `load_last_profile_name` and `load_profile` respectively, preventing any passthrough to real file system operations during these unit tests.
    *   **Code Check:** The setup for mocks seems robust, but a targeted review for complete isolation in these specific tests is beneficial.
    *   **Rationale:** Ensures purer and more reliable unit tests for application startup logic.
*   **Simulate Non-Success Paths for Dialog Stubs:**
    *   **Problem:** Current platform layer stub implementations for dialogs (e.g., `_handle_show_profile_selection_dialog_stub_impl`) primarily simulate successful outcomes.
    *   **Action:** Augment these stub implementations in `platform_layer/app.rs` (or introduce a test-specific mechanism) to allow simulation of user cancellations or dialog-induced errors.
    *   **Rationale:** Enables testing `MyAppLogic`'s resilience and error handling for various dialog interaction outcomes.
*   **Integration Testing Considerations for `PlatformInterface::run()`:**
    *   **Problem:** The main event loop (`PlatformInterface::run()`) and its interaction with `MyAppLogic`'s command queue are complex to unit test.
    *   **Action:** Acknowledge this as primarily an integration testing concern. Focus on comprehensive unit tests for `MyAppLogic` and individual `Win32ApiInternalState` command handlers. Defer complex `run()` loop tests unless specific, hard-to-diagnose issues arise.
    *   **Rationale:** Balances testing effort; unit tests provide faster feedback for component logic.

### P2.11.5: Future Enhancements & Nice-to-Haves (Low Priority for P2)
*   **Convenience Macro for Status Bar Updates:**
    *   **Problem:** Enqueuing `PlatformCommand::UpdateStatusBarText` can be verbose in `MyAppLogic`.
    *   **Action:** Explore creating a Rust macro (e.g., `status_update!(app_logic_instance, severity, "message format {}", variable)`) that simplifies the command creation and enqueuing process.
    *   **Rationale:** Improves developer ergonomics and reduces boilerplate code.
*   **Structured Application Settings File:**
    *   **Problem:** The current `last_profile_name.txt` is limited to one piece of startup information.
    *   **Action:** If more application-wide settings are anticipated (e.g., window size/position, UI preferences), plan to migrate `core::config` to use a structured format like JSON (e.g., `app_settings.json`). This would involve expanding `ConfigManagerOperations`.
    *   **Rationale:** Provides a scalable solution for managing a growing set of application preferences.
*   **Modularity of `handler.rs` and `handler_tests.rs`:**
    *   **Problem:** As features are added, `app_logic/handler.rs` and its corresponding test file can become very large.
    *   **Action:** If these files become unwieldy, consider refactoring `MyAppLogic`'s functionality into smaller, more focused sub-modules within `app_logic` (e.g., `profile_flow_logic`, `archive_flow_logic`). Test files could then be co-located or similarly structured.
    *   **Rationale:** Enhances long-term maintainability and navigability of the codebase.
*   **Review UI Element Creation in `WM_CREATE`:**
    *   **Problem:** `handle_wm_create` in `platform_layer/window_common.rs` might become a bottleneck for UI element creation if too many static elements are added there.
    *   **Current Decision:** The existing approach of creating essential, static child controls (like buttons, status bar) in `WM_CREATE` is acceptable. More dynamic content (like TreeView items) is already command-driven. Re-evaluate if this balance becomes problematic.
    *   **Rationale:** Balances initial UI setup efficiency with the flexibility of command-driven UI updates.

## P2.12: Sophisticated Status Bar Control:**
*   For features like multiple panes (e.g., profile name, file count, token count, general status), replace the `STATIC` control with a standard Windows Status Bar control (`STATUSCLASSNAME`). This control supports multiple parts and icons.
*   Centralized Error-to-Status Mapping:
    *   Instead of formatting error strings directly in each error handling site within `MyAppLogic`, you could create helper functions or a dedicated error-handling module that converts specific `Error` types into user-friendly status messages and decides if `is_error` should be true.

# Phase 3: Enhancements & UX Improvements

## P3.0: Deselected files and folders
*   It shall be possible to mark a file as deselected. It should have a visual indicator for this. Typically, when there are new files after a refresh, they will be initiated as neither selected nor deselected.
*   Suggest a way to show this state. Maybe set the checkbox to disabled (not possible to change state). Another idea is to have a combox for each file and folder, with three states.
*   If a checkbox will be used, a mechanism is needed to change it do "deselected". Maybe a separate button can be used to toggle the state of the currently selected file.
*   gitignore should be used to initialize settings to deselected, instead of hiding them completely.

## P3.1: Status Bar Finalization
*   Display current profile name (covered by P2.8, window title).
*   Display number of selected files. `[UiStatusBarSelectedFileCountV1]`
*   Display total size of selected files (optional). `[UiStatusBarSelectedFileSizeV1]`
*   Clearly display archive status (e.g., "Archive: Up-to-date", "Archive: Needs Update", "Archive: Not Generated"). Color-coding or icons could be considered. `[UiNotificationOutdatedArchiveV1]` (Refinement of P2.8).
*   **Usability:** Ensure status bar updates are immediate and clear.

## P3.2: Token Count (Module: `tokenizer_utils`)
*   Integrate a token counting library (e.g., `tiktoken-rs` or a simple word/space counter initially). `[TokenCountEstimateSelectedV1]`
*   Implement `fn estimate_tokens(content: &str) -> usize`.
*   Update status bar with live token count of selected files. `[TokenCountLiveUpdateV1]`, `[UiStatusBarLiveTokenCountV1]`

## P3.3: File Content Viewer
*   Add a read-only text control (e.g., `EDIT` control). `[UiContentViewerPanelReadOnlyV1]`
*   When a file is selected in the `TreeView`, load its content into the viewer.

## P3.4: (REMOVED - Whitelist Pattern Editing) `[UiMenuEditWhitelistV1]` (marks removal)

## P3.5: Handling File State Discrepancies Visually
*   When loading a profile, or after a "Refresh" (P2.9):
    *   Files listed in `profile.selected_paths` or `profile.deselected_paths` but NOT found on disk during scan: Mark visually in the TreeView (e.g., greyed out, special icon, different text color). The profile retains these paths. `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Files found on disk BUT not in `profile.selected_paths` or `profile.deselected_paths`: These are new/unclassified. They appear in the tree with `FileState::Unknown` (and corresponding checkbox state). `[FileStateNewUnknownV2]`, `[UiTreeViewVisualFileStatusV1]`

## P3.6: Full Profile Management UI
*   **Usability:** "File -> Manage Profiles..." opens a dedicated dialog.
*   Dialog to list profiles (P1.3). `[UiMenuProfileManagementV1]`
*   Buttons for "Create New" (uses flow from P2.6.4), "Duplicate", "Delete" profile.
*   "Edit Profile" (if implemented as a distinct feature or part of manage dialog):
    *   Allow changing name (with sanitization and check for conflicts), root folder (triggers rescan), and `archive_path`. `[UiMenuSetRootFolderV1]`
*   "Duplicate Profile" dialog: `[ProfileOpDuplicateExistingV1]`
    *   Prompt for new profile name. Copies settings from selected profile.
    *   `archive_path` for duplicated profile starts as `None` or user is prompted.
*   "Delete Profile": Remove profile file, confirm with user. If deleting active profile, trigger P2.6.2. `[ProfileOpDeleteExistingV1]`

# Phase 4: Advanced Features (Optional / Future)

## P4.1: File Name Search
*   Add a search input field. `[UiSearchFileNameFilterTreeV1]`
*   Filter the `TreeView` to show only matching file/folder names.

## P4.2: Content Search
*   Input field for search string. `[UiSearchFileContentHighlightV1]`
*   Button to "Search in Selected" or "Search in All (visible)".
*   Highlight files in the tree that contain the string.
*   (Optionally) Show occurrences in the File Content Viewer.

## P4.3: Refresh Tree View Button (UI aspect, logic in P2.9)
*   Ensure the "Refresh" button/menu item is clearly accessible. `[UiMenuTriggerScanV1]`

## P4.4: "Clear Selection" / "Select All Files" / "Invert Selection" options.

## P4.5: Better Binary File Detection
*   Implement a more robust check (e.g., percentage of non-printable chars). (Supports `[TextFileFocusUTF8V1]`)
*   Visually indicate binary files or optionally exclude them. `[FutureBinaryFileDetectionSophisticatedV1]`
