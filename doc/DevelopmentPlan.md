# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

(Earlier steps already completed)

## P2.10: AppEvent.Execute Refactoring Discussion
*   Maybe the MyAppLogic::handle_event should call event.Execute(&mut commands, &self)? That would take away almost all code from handle_event.
    *   **Note:** This is an interesting idea for further refactoring *after* the M/V/P separation. The M/V/P changes might make the `handle_event` in `MyAppLogic` simpler by delegating more to `AppSessionData` and `MainWindowUiState` first.

## P2.11: General Cleanup and Refinements

This section details various cleanup tasks, refactorings, and minor enhancements to improve code quality, user experience, and testability.

Implemented sections have been removed.

### P2.11.1: Robust State Saving on Exit
*   **Problem:** (Previously identified) The `handle_window_destroyed` might clear state prematurely before `on_quit()` saves.
*   **Action:** Ensure that application state required for saving (profile name, current file selections, archive path) is reliably preserved until `MyAppLogic::on_quit()` is called. The M/V/P refactoring should address this by separating `AppSessionData` (persistent) from `MainWindowUiState` (transient).
*   **Reference:** Covered by the "M/V/P Separation Plan", specifically how `handle_window_destroyed` will only clear `MainWindowUiState`.

### P2.11.2: Core Logic Correctness & User Experience (High Priority)
*   **User-Friendly Error Reporting from `MyAppLogic`:** `[TechErrorHandlingGracefulV1]`
    *   **Problem:** Errors in `MyAppLogic` are logged to console but not shown to the user in the UI.
    *   **Action:** Replace `eprintln!` calls in `MyAppLogic` that represent user-facing errors or important warnings with `PlatformCommand::UpdateStatusBarText` (or a future error dialog command). Use appropriate `MessageSeverity`. The M/V/P refactoring might centralize status update queuing in `MyAppLogic`.
    *   **Rationale:** Essential for user feedback and a more polished application.
*   **Accurate Profile State Update:**
    *   **Problem:** `MyAppLogic.current_profile_name` and `current_profile_cache` (now fields in `AppSessionData`) might be updated prematurely.
    *   **Action:** Ensure these fields in `AppSessionData` are updated *only after* a successful save operation (e.g., after creating a new profile or saving an existing one under a new name/path). This logic will be managed by `MyAppLogic` when interacting with `AppSessionData`.
    *   **Rationale:** Maintains consistent internal state with persisted state, preventing discrepancies.
*   **Correct Application Data Storage Location:** `[ProfileStoreAppdataLocationV1]`
    *   **Problem:** Configuration data might be stored in a non-standard or less appropriate user directory.
    *   **Action:** Verify and ensure `CoreConfigManager` and `CoreProfileManager` use the platform-appropriate local application data directory.
    *   **Code Check:** This appears to be correctly implemented.
    *   **Rationale:** Adherence to operating system guidelines.
*   **Centralized Profile Name Validation:**
    *   **Problem:** Profile name validation logic might be duplicated.
    *   **Action:** Ensure `MyAppLogic` (when handling user input for profile names) uses the public `core::profiles::is_valid_profile_name_char` function.
    *   **Code Check:** This appears to be correctly implemented.
    *   **Rationale:** Code deduplication, consistency, and easier maintenance.

### P2.11.3: Code Health & Refactoring (Medium Priority)
*   **Modularity of `MyAppLogic` Handlers:** `[TechModularityLogicalModulesV1]`
    *   **Problem:** Event handlers or helper functions within `app_logic/handler.rs` might become too large.
    *   **Action:** Review large methods in `MyAppLogic`. Refactor them into smaller, focused private helpers. The M/V/P refactoring will significantly contribute to this by moving logic to `AppSessionData` and `MainWindowUiState`.
    *   **Reference:** Directly supported by the "M/V/P Separation Plan".
    *   **Rationale:** Improves readability, maintainability, and unit testability.
*   **Modularity of Platform Message Handlers:**
    *   **Problem:** `Win32ApiInternalState::handle_window_message` can become large.
    *   **Action:** Continue delegating specific `WM_` message handling to dedicated private methods within `Win32ApiInternalState`.
    *   **Rationale:** Improves organization of platform-specific code.
*   **Consolidate Directory Path Logic:**
    *   **Problem:** `CoreConfigManager::_get_app_config_dir` and `CoreProfileManager::_get_profile_storage_dir` have similar path derivation.
    *   **Action:** Review. Consider a shared private helper for the base application-specific local config directory.
    *   **Rationale:** Minor code deduplication.
*   **Ergonomic Access to `Win32ApiInternalState.active_windows`:**
    *   **Problem:** Direct locking of `active_windows` can be verbose.
    *   **Action:** Evaluate creating specialized helper methods on `Win32ApiInternalState` for common `active_windows` access patterns.
    *   **Rationale:** Can enhance code readability and reduce boilerplate.

### P2.11.4: Testing Improvements (Medium Priority)
*   **Enhance Test Isolation for `_on_ui_setup_complete` (formerly `on_main_window_created`):**
    *   **Problem:** Tests for `MyAppLogic::_on_ui_setup_complete` might not be fully isolated.
    *   **Action:** Review tests. Ensure mocks are configured for `load_last_profile_name` and `load_profile`.
    *   **Code Check:** Mocks seem robust, but targeted review is beneficial.
    *   **Rationale:** Ensures purer unit tests for startup logic.
*   **Simulate Non-Success Paths for Dialog Stubs:**
    *   **Problem:** Dialog stubs primarily simulate success.
    *   **Action:** Augment stubs in `platform_layer/dialog_handler.rs` or use a test mechanism to simulate cancellations/errors.
    *   **Rationale:** Enables testing `MyAppLogic`'s resilience for various dialog outcomes.
*   **Integration Testing Considerations for `PlatformInterface::main_event_loop()`:**
    *   **Problem:** The main event loop is complex to unit test.
    *   **Action:** Acknowledge as integration testing. Focus on unit tests for `MyAppLogic`, `AppSessionData`, `MainWindowUiState`, and command handlers.
    *   **Rationale:** Balances testing effort.

### P2.11.5: Future Enhancements & Nice-to-Haves (Low Priority for P2)
*   **Convenience Macro for Status Bar Updates:**
    *   **Problem:** Enqueuing `PlatformCommand::UpdateStatusBarText` can be verbose.
    *   **Action:** Explore creating a Rust macro or a helper method on `MyAppLogic` (e.g., `_enqueue_status_update`).
    *   **Reference:** The M/V/P plan suggests a helper method `_enqueue_status_update` in "Phase 4: Refine Orchestration".
    *   **Rationale:** Improves developer ergonomics.
*   **Structured Application Settings File:**
    *   **Problem:** `last_profile_name.txt` is limited.
    *   **Action:** Plan to migrate `core::config` to use JSON (e.g., `app_settings.json`) if more settings are anticipated.
    *   **Rationale:** Scalable solution for preferences.
*   **Modularity of `handler.rs` and `handler_tests.rs`:**
    *   **Problem:** Files can become large.
    *   **Action:** If `app_logic/handler.rs` becomes unwieldy *after* the M/V/P refactor, consider further sub-modules for specific flows.
    *   **Reference:** The M/V/P plan (creating `app_session_data.rs` and `main_window_ui_state.rs`) is the first major step in this direction.
    *   **Rationale:** Long-term maintainability.
*   **Review UI Element Creation in `WM_CREATE`:**
    *   **Problem:** `handle_wm_create` might be a bottleneck.
    *   **Current Decision:** Essential static child controls in `WM_CREATE` (now done via commands from `ui_description_layer`) is acceptable. Dynamic content is command-driven.
    *   **Rationale:** Balances initial setup with command-driven flexibility.

### P2.11.6: Clearer State Separation within `MyAppLogic`**
*   **This entire section is now superseded by the "M/V/P Separation Plan".**
    *   **Reference:** This is the core objective of the "M/V/P Separation Plan".

## P2.12: Sophisticated Status Bar Control:
*   For features like multiple panes, replace `STATIC` with `STATUSCLASSNAME`.
*   **Centralized Error-to-Status Mapping:**
    *   Create helpers or a module for converting `Error` types to user-friendly status messages.

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
*   Display total size of selected files (optional). `[UiStatusBarSelectedFileSizeV1]`
*   Clearly display archive status (e.g., "Archive: Up-to-date"). Color-coding or icons. `[UiNotificationOutdatedArchiveV1]`
*   **Usability:** Ensure status bar updates are immediate and clear.

## P3.2: Token Count (Module: `tokenizer_utils`)
*   Integrate a token counting library (e.g., `tiktoken-rs` or improve simple counter). `[TokenCountEstimateSelectedV1]`
    *   **Note:** Current implementation uses `estimate_tokens_simple_whitespace`.
*   Update status bar with live token count. `[TokenCountLiveUpdateV1]`, `[UiStatusBarLiveTokenCountV1]`
    *   **Reference:** Logic for updating `current_token_count` will move to `AppSessionData` as per the M/V/P plan. `MyAppLogic` will request UI updates.

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
