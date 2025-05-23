# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 0': Code Synchronization (Post-Whitelist Removal)

**Goal:** Modify the existing codebase to remove all whitelist functionality and align it with the updated requirements for features up to the original P2.5. This phase ensures the application is in a clean state before proceeding with P2.6.

## P0'.2: Modify Core Data Structures (`core/models.rs`)
*   In `struct Profile`:
    *   Remove the `whitelist_patterns: Vec<String>` field. `[ProfileDefWhitelistPatternsV1]` (marks removal)
*   Adjust `Profile::new()` if it explicitly handled `whitelist_patterns` (it likely just relied on struct definition).
*   **Note:** Existing profile JSON files will become incompatible. For this sync, assume new profiles will be created or existing ones manually edited.

## P0'.3: Modify Directory Scanning Logic (`core/file_system.rs`)
*   Change `scan_directory` function signature to:
    `pub fn scan_directory(root_path: &Path) -> Result<Vec<FileNode>, FileSystemError>` (remove `whitelist_patterns_str` parameter). `[FileFilterWhitelistOnlyMatchesV1]` (marks removal)
*   Remove all internal logic related to compiling glob patterns and matching files against them.
*   The function should now list *all* files and directories found by `WalkDir` under `root_path`.
*   Update unit tests for `scan_directory` to reflect that no filtering occurs and all files/directories are returned.

## P0'.4: Update Application Logic (`app_logic/handler.rs`)
*   In `struct MyAppLogic`:
    *   Remove the `current_whitelist_patterns: Vec<String>` field.
    *   Remove its initialization in `MyAppLogic::new()`.
*   In `MyAppLogic::on_main_window_created`:
    *   Modify the call to `core::scan_directory` to no longer pass whitelist patterns.
*   In `MyAppLogic::create_profile_from_current_state`:
    *   Remove the assignment of `self.current_whitelist_patterns` to the new `Profile` object.
*   In `AppEvent::FileOpenDialogCompleted` handler (profile loading):
    *   When a `Profile` is loaded, it will no longer contain `whitelist_patterns`. Remove any logic that attempts to read this from the loaded profile and store it in `MyAppLogic`.
    *   Ensure the subsequent call to `core::scan_directory` (if present here or triggered by a refresh) uses the no-whitelist version.
*   Search for any other uses of `current_whitelist_patterns` or whitelist-related logic in `MyAppLogic` and remove them.

## P0'.5: Test Basic Functionality
*   Perform a full `cargo build` and `cargo test --all-features`.
*   Run the application:
    *   Verify that the TreeView now displays all files and folders from the scanned directory (no filtering). `[UiTreeViewDisplayStructureV2]`
    *   Verify that profiles can be saved and loaded correctly (they will now lack whitelist data). `[ProfileOpSaveNewOverwriteV3]`
    *   Verify basic file/folder selection in the TreeView works. `[FileSelStateSelectedV1]`, `[FileSelStateDeselectedV1]`, `[FileSelStateUnknownV1]`
    *   Verify basic archive generation still functions with selected files. `[ArchiveGenSingleTxtFileV1]`
*   This confirms the codebase is stable and reflects the no-whitelist requirement for features analogous to the old P0.1-P2.5.

---

# Phase 1: Core Logic (Testable Modules) - Post Sync
*(This section now describes the target state AFTER Phase 0' is complete)*

## P1.1: Data Structures
*   Define `struct FileNode { path: PathBuf, name: String, is_dir: bool, state: FileState, children: Vec<FileNode> }`. `[FileSelStateSelectedV1]`, `[FileSelStateDeselectedV1]`, `[FileSelStateUnknownV1]`
*   Define `enum FileState { Selected, Deselected, Unknown }`.
*   Define `struct Profile { name: String, root_folder: PathBuf, selected_paths: HashSet<PathBuf>, deselected_paths: HashSet<PathBuf>, archive_path: Option<PathBuf> }`. `[ProfileDefRootFolderV1]`, `[ProfileDefSelectionStateV1]`, `[ProfileDefAssociatedArchiveV1]`

## P1.2: Directory Scanning (Module: `file_system`)
*   Implement function: `scan_directory(root_path: &Path) -> Result<Vec<FileNode>, Error>`. `[FileSystemMonitorTreeViewV1]`
    *   Uses `walkdir` to traverse directories.
    *   Builds the `FileNode` tree for all files and directories.
    *   Initial `FileState` for all nodes will be `Unknown`. `[FileStateNewUnknownV2]`
*   **Unit Tests:** Test with various directory structures.

## P1.3: Profile Management (Module: `profiles`)
*   Implement `fn save_profile(profile: &Profile, app_name: &str) -> Result<(), Error>`. `[ProfileStoreJsonFilesV1]`, `[ProfileStoreAppdataLocationV1]`
*   Implement `fn load_profile(profile_name: &str, app_name: &str) -> Result<Profile, Error>`.
*   Implement `fn list_profiles(app_name: &str) -> Result<Vec<String>, Error>`. (Critical for P2.6.2)
*   Implement `fn get_profile_dir_path(app_name: &str) -> Option<PathBuf>`. (Used to be `get_profile_dir`)
*   **Unit Tests:** Test saving, loading, listing.

## P1.4: State Application (Module: `state_manager`)
*   Implement `fn apply_profile_to_tree(tree: &mut Vec<FileNode>, profile: &Profile)`. `[FileSelStateSelectedV1]`, `[FileSelStateDeselectedV1]`, `[FileSelStateUnknownV1]`
    *   Iterates through `tree`, setting `FileState` based on `profile.selected_paths` and `profile.deselected_paths`.
    *   Files not in either set but present in the scanned tree become `FileState::Unknown`. `[FileStateNewUnknownV2]`
*   Implement `fn update_folder_selection(node: &mut FileNode, new_state: FileState)`. `[FileSelFolderRecursiveStateV1]`
    *   Recursively sets state of all children.
*   **Unit Tests:** Test application of states to various tree structures.

## P1.5: Archiving Logic (Module: `archiver`)
*   Implement `fn create_archive_content(root_node: &FileNode) -> String` (or takes a list of selected `FileNode`s). `[ArchiveGenSingleTxtFileV1]`, `[ArchiveGenConcatenateContentV1]`
    *   Traverses the `FileNode` tree.
    *   For `Selected` files, reads content (UTF-8) and prepends/appends headers. `[ArchiveGenFileHeaderFooterV1]`, `[TextFileFocusUTF8V1]`
*   **Unit Tests:** Test with mock file content and tree structures.

## P1.6: Timestamp & Archive Status Utilities (Module: `file_system` or `archive_utils`)
*   Implement `fn get_file_timestamp(path: &Path) -> Result<SystemTime, Error>`. `[ArchiveSyncTimestampV1]`
*   Define `enum ArchiveStatus { UpToDate, OutdatedRequiresUpdate, NotYetGenerated, ArchiveFileMissing, NoFilesSelected, ErrorChecking }`.
*   Implement `fn check_archive_status(profile: &Profile, selected_file_nodes: &[FileNode]) -> ArchiveStatus`. `[ArchiveSyncTimestampV1]`, `[ArchiveSyncNotifyUserV1]`
    *   Checks if `profile.archive_path` exists.
    *   If archive exists, compares its timestamp with the newest timestamp among `selected_file_nodes`.
    *   If selected files are newer, or `profile.archive_path` is None, or archive file is missing, return appropriate status.
    *   If no files are selected, return `NoFilesSelected`.
*   **Unit Tests:** Test with mock files, timestamps, and profile states.

# Phase 2: Basic UI & Interaction - Post Sync
*(This section now describes the target state AFTER Phase 0' is complete and incorporates the "Always Active Profile" flow)*

## P2.1: TreeView Population
*   Add a `TreeView` control to the main window. `[UiTreeViewDisplayStructureV2]`
*   Populate the `TreeView` from a `FileNode` tree (from P1.2) *after a profile is active*.

## P2.2: Basic Selection Visualization
*   Use standard checkboxes in the `TreeView`. `[UiTreeViewVisualSelectionStateV1]`
*   Link `TreeView` checkbox changes to update the `FileState` in the internal `FileNode` tree and vice-versa.

## P2.3: Folder Selection Propagation
*   When a folder checkbox is changed in the UI, trigger `update_folder_selection` (P1.4) on the corresponding `FileNode` and update the UI for children. `[FileSelFolderRecursiveStateV1]`

## P2.4: "Generate Archive" Button & Action - Basic
*   Add a button. `[UiMenuGenerateArchiveV1]`
*   On click, call `create_archive_content` (P1.5) with the current state of selected files. `[ArchiveGenSingleTxtFileV1]`
*   Initially, prompt user to save the resulting string to a `.txt` file each time. (This will be enhanced in P2.7).
*   **Usability:**
    *   Provide feedback during generation (busy cursor, status bar text: "Generating archive...").
    *   Provide confirmation on save: "Archive 'X' saved." or error message.

## P2.5: Profile Management UI (Post-Startup)
*   Add menu items: "Switch Profile..." (replaces "Load Profile"), "Save Profile As". `[UiMenuProfileManagementV1]`
*   "Switch Profile...":
    *   Re-uses the `ShowProfileSelectionDialog` (from P2.6.2.1) but without quitting on cancel.
    *   On successful selection: Load profile (P1.3), update `current_profile` in `MyAppLogic`, rescan directory (P1.2), apply profile (P1.4), update TreeView. `[ProfileOpLoadSwitchV1]`
    *   Update window title and status bar.
    *   Perform `check_archive_status` (P1.6) and update UI. `[ArchiveSyncUserAcknowledgeV1]`
*   "Save Profile As": Prompt for profile name, create `Profile` object from current state (root dir, selection, current `archive_path` if any), save it (P1.3). Update `current_profile` and window title. `[ProfileOpSaveNewOverwriteV3]`

---
## P2.6: **Always Active Profile: Startup Sequence**
**Goal:** Ensure an active profile before the main UI is fully operational. The main window remains hidden or minimally shown until profile activation.

### P2.6.1: `MyAppLogic::on_main_window_created` Refactor
*   Attempt to load the last used profile name (`config_manager.load_last_profile_name`) and then the profile itself (`profile_manager.load_profile`).
*   **If successful:**
    *   Set profile as active (`current_profile_cache`, `current_profile_name`).
    *   Update window title to "SourcePacker - [ProfileName]".
    *   Proceed to P2.6.5 (Scan, Apply, Populate, Show).
*   **If loading fails (or no last profile):**
    *   Main window remains hidden.
    *   Call new internal method `MyAppLogic::initiate_profile_selection_or_creation(window_id)`.

### P2.6.2: `MyAppLogic::initiate_profile_selection_or_creation(window_id)`
*   Get `available_profiles` using `profile_manager.list_profiles()` (P1.3).
*   **Determine Dialog Texts:**
    *   If `available_profiles` is empty: Title "Welcome!", Prompt "No profiles found. Please create a new profile to get started." Emphasize "Create New".
    *   Else: Title "Select or Create Profile", Prompt "Please select a profile to continue, or create a new one."
*   Issue `PlatformCommand::ShowProfileSelectionDialog` (New Command) with `window_id`, `available_profiles`, determined texts, and `emphasize_create_new` flag.

### P2.6.3: Implement New Platform Commands and Events
*   **Platform Commands:**
    *   `ShowProfileSelectionDialog { window_id, available_profiles, title, prompt, emphasize_create_new }`
    *   `ShowInputDialog { window_id, title, prompt, default_text, context_tag }`
    *   `ShowFolderPickerDialog { window_id, title, initial_dir }`
    *   `QuitApplication`
    *   `SetWindowBusyCursor { window_id, is_busy }` (Optional)
*   **AppEvents:**
    *   `ProfileSelectionDialogCompleted { window_id, chosen_profile_name: Option<String>, create_new_requested: bool, cancelled: bool }`
    *   `InputDialogCompleted { window_id, text: Option<String>, context_tag: Option<String> }`
    *   `FolderPickerDialogCompleted { window_id, path: Option<PathBuf> }`
*   **Platform Layer Implementation:**
    *   Implement native dialogs for these commands, sending back the corresponding `AppEvent`.
    *   `ShowProfileSelectionDialog`: Modal, list box, "Select", "Create New", "Quit Application" buttons.
    *   `ShowInputDialog`: Modal, text input, "OK", "Cancel".
    *   `ShowFolderPickerDialog`: Native folder browser, use `initial_dir` (e.g., Documents, last valid path).
    *   `QuitApplication`: Posts `WM_QUIT`.
    *   `SetWindowBusyCursor`: Changes mouse cursor.

### P2.6.4: `MyAppLogic` Event Handling for Profile Flow
*   **Handle `AppEvent::ProfileSelectionDialogCompleted`:**
    *   If `cancelled`: Issue `PlatformCommand::QuitApplication`.
    *   If `chosen_profile_name`: Attempt to load.
        *   On success: Set active, save as last used. Proceed to P2.6.5.
        *   On failure: Status bar error (e.g., "Could not load profile '[X]'. May be corrupt."), then `initiate_profile_selection_or_creation(window_id)`.
    *   If `create_new_requested`: Call `MyAppLogic::start_new_profile_creation_flow(window_id)`.
*   **`MyAppLogic::start_new_profile_creation_flow(window_id)`:**
    *   **Step 1 (Name):** Issue `ShowInputDialog` (title: "New Profile (1/2): Name", prompt: "Enter profile name:", context: "NewProfileName").
*   **Handle `AppEvent::InputDialogCompleted` (for "NewProfileName"):**
    *   If `text` is valid: Store name. Issue `ShowFolderPickerDialog` (title: "New Profile (2/2): Select Root Folder").
    *   If `text` is None (cancelled): Call `initiate_profile_selection_or_creation(window_id)`.
    *   If `text` invalid: Status bar error ("Invalid name..."). Re-issue `ShowInputDialog` for name.
*   **Handle `AppEvent::FolderPickerDialogCompleted`:**
    *   If `path` is Some: Create `Profile`, save it (P1.3). Set active, save as last used. Proceed to P2.6.5.
    *   If `path` is None (cancelled): Call `initiate_profile_selection_or_creation(window_id)`.

### P2.6.5: Post-Profile Activation: Scan, Apply, Populate, Show
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
*   Profile Name in `create_profile_from_current_state`: Update `MyAppLogic.current_profile_name` only after successful save.
*   Review `Win32ApiInternalState::process_commands_from_event_handler` vs `PlatformInterface::execute_command`.
*   Break out large functions in `app_logic/handler.rs`. `[TechModularityLogicalModulesV1]`
*   Refactor access to `Win32ApiInternalState.window_map` through helper methods if beneficial.
*   The current `last_profile_name.txt` is very simple. If more startup configurations are needed (e.g., window size/position, other UI preferences), migrating to a structured format like JSON for `app_settings.json` might be beneficial. The `core::config` module can be expanded for this.
*   The current tests for `on_main_window_created` in `handler_tests.rs` rely on the actual file system operations of `core::load_last_profile_name` and `core::load_profile`. For more isolated unit tests of `MyAppLogic`, these core functions could be mocked.
*   `handler.rs` and `handler_tests.rs` are big. Is it possible to separate them into smaller modules or files for easier maintenance and testing? If so, the tests should be moved back into the same file, removing the need for `pub(crate)` declarations.
*   It seems to me that `handle_wm_create` is growing with hard coded functionality. Would it be better to control these through the command structure? That is, `window_common.rs` shouldn't manage the complete UI, only manage the individual components. That would be `on_main_window_created`? The question is, will this require a big refactor?
    *   **Decision:** For now, keep `handle_wm_create` for essential child controls like buttons/status bar defined at window creation. More dynamic content (like TreeView items) is already command-driven. Revisit if it becomes too unwieldy.

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
