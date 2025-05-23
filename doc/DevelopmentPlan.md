# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

---
## P2.6: **Always Active Profile: Startup Sequence**
**Goal:** Ensure an active profile before the main UI is fully operational. The main window remains hidden or minimally shown until profile activation.

(Earlier steps already completed)

### P2.6.6: Post-Profile Activation: Scan, Apply, Populate, Show
*   This step is reached after a profile is successfully loaded or created.
*   Perform directory scan based on active `profile.root_folder` (P1.2).
*   Apply profile to the scanned tree (P1.4).
*   Populate TreeView (P2.1).
*   Perform initial `check_archive_status` (P1.6) and update UI (P2.8). `[ArchiveSyncUserAcknowledgeV1]`
*   Status bar confirmation: "Profile '[ProfileName]' loaded." or "New profile '[ProfileName]' created and loaded."
*   Issue `PlatformCommand::ShowWindow { window_id }`.

---

## P2.7: "Generate Archive" Button & Action - Enhanced
*   On button click: `[UiMenuGenerateArchiveV1]`, `[ArchiveSyncUserAcknowledgeV1]`
    *   If current `profile.archive_path` is `None`: `[ProfileDefAssociatedArchiveV1]`
        *   Prompt user to "Save Archive As" (standard file save dialog), specifying the output archive file.
        *   If user provides a path, store it in the `current_profile.archive_path`.
        *   Immediately save the updated `Profile` (so `archive_path` is persisted).
    *   If `profile.archive_path` is `Some(path)` (or was just set):
        *   Call `create_archive_content` (P1.5) with the current selected files.
        *   Save the resulting string to `profile.archive_path`.
    *   **Usability:**
        *   On success: Update status to `UpToDate` via `check_archive_status` (P1.6), status bar "Archive saved to '[path]}'".
        *   On error: Status bar error "Failed to save archive: [details]".
        *   Reset busy cursor.

## P2.8: UI for Archive Status (Initial)
*   Integrate basic archive status display into the Status Bar (P3.1 conceptually). `[UiNotificationOutdatedArchiveV1]`
    *   Possible messages: "Archive: Up-to-date", "Archive: Needs Update", "Archive: Not Generated", "Archive: File Missing", "Archive: No Files Selected", "Archive: Error Checking".
*   Update this status display whenever `check_archive_status` is performed (profile load/creation, selection change, refresh, archive generation).
*   The window title should show "SourcePacker - [ProfileName]". `[UiStatusBarProfileNameV1]` (This is a window title, but closely related to profile context).

## P2.9: Refresh File List Action
*   Add a "Refresh" button or menu item. `[UiMenuTriggerScanV1]`
*   On action:
    *   Re-run `scan_directory` (P1.2) for the current profile's root. `[FileSystemMonitorTreeViewV1]`
    *   Update the internal `FileNode` tree.
    *   Apply `profile.selected_paths` and `profile.deselected_paths` to the new tree. Newly discovered files (not in profile's selection sets) become `FileState::Unknown`. `[FileStateNewUnknownV2]`
    *   Re-populate the TreeView (P2.1), ensuring new "Unknown" files are visually distinct if possible, or at least correctly reflect their state. `[UiTreeViewVisualSelectionStateV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Run `check_archive_status` (P1.6). `[ArchiveSyncNotifyUserV1]`
    *   Update status bar and any other relevant UI.

## P2.10: AppEvent.Execute Refactoring Discussion
*   Maybe the MyAppLogic::handle_event should call event.Execute(&mut commands, &self)? That would take away almost all code from handle_event.

## P2.11: General Cleanup and Refinements
*   There is one `_get_app_config_dir` and one `_get_profile_storage_dir`. They have duplicate functionality. Consider consolidating them.
*   Use AppData\Local instead of AppData\Roaming. `[ProfileStoreAppdataLocationV1]` (verify correct path component).
*   Replace `eprintln!` in `MyAppLogic` error paths with `PlatformCommands` to show error messages in a status field or dialog. `[TechErrorHandlingGracefulV1]` User-friendly error messages are key.
*   Break out large message handlers in `Win32ApiInternalState::handle_window_message`.
*   Consolidate Status Messages: The on_main_window_created failure paths now push an error message, and initiate_profile_selection_or_creation also pushes its own message. Review if these should be combined or if the platform layer should only display the latest status message for a given window. The current behavior is that multiple UpdateStatusBarText commands will be processed sequentially.
    * Idea: Add a severity flag to the error (used in UpdateStatusBarText). Any error message with a higher severity will overwrite the previous one. The "current" severity is reset after the error has been shown to the user.
*   Profile Name in `create_profile_from_current_state`: Update `MyAppLogic.current_profile_name` only after successful save.
*   Break out large functions in `app_logic/handler.rs`. `[TechModularityLogicalModulesV1]`
*   Refactor access to `Win32ApiInternalState.window_map` through helper methods if beneficial.
*   The current `last_profile_name.txt` is very simple. If more startup configurations are needed (e.g., window size/position, other UI preferences), migrating to a structured format like JSON for `app_settings.json` might be beneficial. The `core::config` module can be expanded for this.
*   The current tests for `on_main_window_created` in `handler_tests.rs` rely on the actual file system operations of `core::load_last_profile_name` and `core::load_profile`. For more isolated unit tests of `MyAppLogic`, these core functions could be mocked.
*   `handler.rs` and `handler_tests.rs` are big. Is it possible to separate them into smaller modules or files for easier maintenance and testing? If so, the tests should be moved back into the same file, removing the need for `pub(crate)` declarations.
*   It seems to me that `handle_wm_create` is growing with hard coded functionality. Would it be better to control these through the command structure? That is, `window_common.rs` shouldn't manage the complete UI, only manage the individual components. That would be `on_main_window_created`? The question is, will this require a big refactor?
    *   **Decision:** For now, keep `handle_wm_create` for essential child controls like buttons/status bar defined at window creation. More dynamic content (like TreeView items) is already command-driven. Revisit if it becomes too unwieldy.
*  Testing the `PlatformInterface::run()` loop: Testing the interaction between the `run()` loop and `MyAppLogic`'s command queue is more of an integration test. It might be beneficial to have some tests that simulate OS messages and verify that commands are dequeued and "executed" (perhaps via a mock `Win32ApiInternalState` for these tests).
*   `core::profiles::is_valid_profile_name_char`: Make this function public (`pub`) so it can be directly used by `MyAppLogic` for validating profile names instead of duplicating the character check logic.
*   Error Handling in Dialogs: The platform layer's stub dialog handlers (`_handle_show_profile_selection_dialog_impl`, etc.) currently simulate success. For more robust testing, they could be enhanced to simulate cancellation or errors, allowing tests for how `MyAppLogic` handles those scenarios from the platform.
*   Test Isolation for `on_main_window_created`: As noted in P2.11, `on_main_window_created` tests could be more isolated if `ConfigManagerOperations::load_last_profile_name` and `ProfileManagerOperations::load_profile` were fully mocked within `handler_tests.rs` instead of relying on the `MockConfigManager` and `MockProfileManager` which still have some passthrough characteristics to the actual core implementations if not carefully configured. The current mock setup is good, but this is a point for deeper isolation if needed.
*   Refactor `_activate_profile_and_show_window`: This function is getting quite central. Ensure its responsibilities remain clear and it doesn't grow too large. The current size and scope seem acceptable.

## P2.12: Sophisticated Status Bar Control:**
*   For features like multiple panes (e.g., profile name, file count, token count, general status), replace the `STATIC` control with a standard Windows Status Bar control (`STATUSCLASSNAME`). This control supports multiple parts and icons.
*   Centralized Error-to-Status Mapping:
    *   Instead of formatting error strings directly in each error handling site within `MyAppLogic`, you could create helper functions or a dedicated error-handling module that converts specific `Error` types into user-friendly status messages and decides if `is_error` should be true.

# Phase 3: Enhancements & UX Improvements

## P3.0: Blocking folders
*   It shall be possible to mark a folder as blocked (Deselected). That would typically be used for temporary folders. (Covered by P1.4 `update_folder_selection`).
*   `.gitignore` shall automatically be used as a blacklist. These shall be hidden from the user. (This is new, requires parsing .gitignore and modifying `scan_directory` or filtering its results).

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
