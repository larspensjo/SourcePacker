# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

# Phase 0: Project Setup & Foundation

## P0.1: Initialize Rust Project
*   `cargo new source_packer --bin`
*   Set up Rust edition (e.g., 2021 or latest).
*   Initialize Git repository.

## P0.2: Add Core Dependencies
*   `windows-rs` (for UI elements and Win32 API).
*   `serde`, `serde_json` (for profile serialization).
*   `directories-rs` (for `%APPDATA%` path).
*   `walkdir` (for efficient directory traversal).
    *   (`glob` removed as whitelists are removed)

## P0.3: Basic `main.rs` and Window
*   Create a minimal Win32 window using `windows-rs`. `[UiMainWindowSingleV1]`
*   Implement the basic message loop.
*   This ensures the fundamental UI setup is working.

# Phase 1: Core Logic (Testable Modules)

## P1.1: Data Structures
*   Define `struct FileNode { path: PathBuf, name: String, is_dir: bool, state: FileState, children: Vec<FileNode> }`. `[FileSelStateSelectedV1]`, `[FileSelStateDeselectedV1]`, `[FileSelStateUnknownV1]`
*   Define `enum FileState { Selected, Deselected, Unknown }`.
*   Modify `struct Profile { name: String, root_folder: PathBuf, selected_paths: HashSet<PathBuf>, deselected_paths: HashSet<PathBuf>, archive_path: Option<PathBuf> }`. (Removed `whitelist_patterns`). `[ProfileDefRootFolderV1]`, `[ProfileDefSelectionStateV1]`, `[ProfileDefAssociatedArchiveV1]`

## P1.2: Directory Scanning (Module: `file_system`)
*   Implement function: `scan_directory(root_path: &Path) -> Result<Vec<FileNode>, Error>` (Removed `whitelist_patterns` parameter). `[FileSystemMonitorTreeViewV1]`
    *   Uses `walkdir` to traverse directories.
    *   Builds the `FileNode` tree for all files and directories.
    *   Initial `FileState` for all nodes will be `Unknown`. `[FileStateNewUnknownV2]`
*   **Unit Tests:** Test with various directory structures.

## P1.3: Profile Management (Module: `profiles`)
*   Implement `fn save_profile(profile: &Profile, app_name: &str) -> Result<(), Error>`. `[ProfileStoreJsonFilesV1]`, `[ProfileStoreAppdataLocationV1]`
*   Implement `fn load_profile(profile_name: &str, app_name: &str) -> Result<Profile, Error>`.
*   Implement `fn list_profiles(app_name: &str) -> Result<Vec<String>, Error>`.
*   Implement `fn get_profile_dir(app_name: &str) -> PathBuf`.
*   **Unit Tests:** Test saving, loading, listing.

## P1.4: State Application (Module: `state_manager`)
*   Implement `fn apply_profile_to_tree(tree: &mut Vec<FileNode>, profile: &Profile)`. `[FileSelStateSelectedV1]`, `[FileSelStateDeselectedV1]`, `[FileSelStateUnknownV1]`
    *   Iterates through `tree`, setting `FileState` based on `profile.selected_paths` and `profile.deselected_paths`.
    *   Files not in either set but present in the scanned tree become `FileState::Unknown`. `[FileStateNewUnknownV2]`
*   Implement `fn update_folder_selection(node: &mut FileNode, new_state: FileState)`. (Review: parameter was `select: bool`). `[FileSelFolderRecursiveStateV1]`
    *   Recursively sets state of all children.
*   **Unit Tests:** Test application of states to various tree structures.

## P1.5: Archiving Logic (Module: `archiver`)
*   Implement `fn create_archive_content(root_node: &FileNode) -> String` (or takes a list of selected `FileNode`s). `[ArchiveGenSingleTxtFileV1]`, `[ArchiveGenConcatenateContentV1]`
    *   Traverses the `FileNode` tree.
    *   For `Selected` files, reads content (UTF-8) and prepends/appends headers. `[ArchiveGenFileHeaderFooterV1]`, `[TextFileFocusUTF8V1]`
*   **Unit Tests:** Test with mock file content and tree structures.

## P1.6: Timestamp & Archive Status Utilities (Module: `file_system` or `archive_utils`)
*   Implement `fn get_file_timestamp(path: &Path) -> Result<SystemTime, Error>`. `[ArchiveSyncTimestampV1]`
*   Define `enum ArchiveStatus { UpToDate, OutdatedRequiresUpdate, NotYetGenerated, ArchiveFileMissing, ErrorChecking }`.
*   Implement `fn check_archive_status(profile: &Profile, selected_file_nodes: &[FileNode]) -> ArchiveStatus`. `[ArchiveSyncTimestampV1]`, `[ArchiveSyncNotifyUserV1]`
    *   Checks if `profile.archive_path` exists.
    *   If archive exists, compares its timestamp with the newest timestamp among `selected_file_nodes`.
    *   If selected files are newer, or `profile.archive_path` is None, or archive file is missing, return appropriate status.
*   **Unit Tests:** Test with mock files, timestamps, and profile states.

# Phase 2: Basic UI & Interaction

## P2.1: TreeView Population
*   Add a `TreeView` control to the main window. `[UiTreeViewDisplayStructureV2]`
*   Populate the `TreeView` from a `FileNode` tree (from P1.2).
*   Initially, no selection interaction, just display.

## P2.2: Basic Selection Visualization
*   Use standard checkboxes in the `TreeView`. `[UiTreeViewVisualSelectionStateV1]`
*   Implement tristate checkbox logic if `windows-rs` and `TreeView` support it directly, or simulate with custom drawing/icons if necessary (aim for simple first).
*   Link `TreeView` checkbox changes to update the `FileState` in the internal `FileNode` tree and vice-versa.

## P2.3: Folder Selection Propagation
*   When a folder checkbox is changed in the UI, trigger `update_folder_selection` (P1.4) on the corresponding `FileNode` and update the UI for children. `[FileSelFolderRecursiveStateV1]`

## P2.4: "Generate Archive" Button & Action - Basic
*   Add a button. `[UiMenuGenerateArchiveV1]`
*   On click, call `create_archive_content` (P1.5) with the current state of selected files. `[ArchiveGenSingleTxtFileV1]`
*   Initially, prompt user to save the resulting string to a `.txt` file each time. (This will be enhanced in P2.7).

## P2.5: Profile Loading/Saving UI
*   Add basic menu items: "Load Profile", "Save Profile As". `[UiMenuProfileManagementV1]`
*   "Load Profile": Show a dialog to pick a profile (from `list_profiles`), load it (P1.3), rescan directory (P1.2), apply profile (P1.4), update TreeView. `[ProfileOpLoadSwitchV1]`
    *   After loading and scanning, perform initial `check_archive_status` (P1.6) and update UI (e.g., status bar placeholder for now). `[ArchiveSyncUserAcknowledgeV1]`
*   "Save Profile As": Prompt for profile name, create `Profile` object from current state (root dir, selection, current `archive_path` if any from loaded profile), save it (P1.3). `[ProfileOpSaveNewOverwriteV3]`

## P2.6: Initial Profile Load on Startup
*   Implement logic to store/retrieve the last used profile name (e.g., in a simple config file or registry). `[ProfileDefaultLoadRecentV1]`
*   On startup, attempt to load this profile. `[ProfileDefaultNoPreviousBlankV1]`
*   After successful load, perform initial scan, apply profile state, then `check_archive_status` (P1.6) and update relevant UI (e.g. status bar). `[ArchiveSyncUserAcknowledgeV1]`

## P2.7: "Generate Archive" Button & Action - Enhanced
*   On button click: `[UiMenuGenerateArchiveV1]`, `[ArchiveSyncUserAcknowledgeV1]`
    *   If current `profile.archive_path` is `None`: `[ProfileDefAssociatedArchiveV1]`
        *   Prompt user to "Save Archive As" (standard file save dialog), specifying the output archive file.
        *   If user provides a path, store it in the `current_profile.archive_path`.
        *   Immediately save the updated `Profile` (so `archive_path` is persisted).
    *   If `profile.archive_path` is `Some(path)` (or was just set):
        *   Call `create_archive_content` (P1.5) with the current selected files.
        *   Save the resulting string to `profile.archive_path`.
        *   After successful save, update internal archive status to `UpToDate` via `check_archive_status` (P1.6).
        *   Update UI (e.g., status bar).

## P2.8: UI for Archive Status (Initial)
*   Integrate basic archive status display into the Status Bar (P3.1 conceptually). `[UiNotificationOutdatedArchiveV1]`
*   Update this status display whenever `check_archive_status` is performed (profile load, selection change, refresh, archive generation). `[UiStatusBarProfileNameV1]` (profile name influences archive context)

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
*   Use AppData\Local instead of AppData\Roaming.
*   Replace `eprintln!` in `MyAppLogic` error paths with `PlatformCommands` to show error messages in a status field or dialog. `[TechErrorHandlingGracefulV1]`
*   Break out large message handlers in `Win32ApiInternalState::handle_window_message`.
*   Profile Name in `create_profile_from_current_state`: Update `MyAppLogic.current_profile_name` only after successful save.
*   Review `Win32ApiInternalState::process_commands_from_event_handler` vs `PlatformInterface::execute_command`.
*   Break out large functions in `app_logic/handler.rs`. `[TechModularityLogicalModulesV1]`
*   Refactor access to `Win32ApiInternalState.window_map` through helper methods if beneficial.

# Phase 3: Enhancements & UX Improvements

## P3.1: Status Bar Finalization
*   Display current profile name. `[UiStatusBarProfileNameV1]`
*   Display number of selected files. `[UiStatusBarSelectedFileCountV1]`
*   Display total size of selected files (optional). `[UiStatusBarSelectedFileSizeV1]`
*   Clearly display archive status (e.g., "Archive: Up-to-date", "Archive: Needs Update", "Archive: Not Generated"). Color-coding or icons could be considered. `[UiNotificationOutdatedArchiveV1]`

## P3.2: Token Count (Module: `tokenizer_utils`)
*   Integrate a token counting library (e.g., `tiktoken-rs` or a simple word/space counter initially). `[TokenCountEstimateSelectedV1]`
*   Implement `fn estimate_tokens(content: &str) -> usize`.
*   Update status bar with live token count of selected files. `[TokenCountLiveUpdateV1]`, `[UiStatusBarLiveTokenCountV1]`

## P3.3: File Content Viewer
*   Add a read-only text control (e.g., `EDIT` control). `[UiContentViewerPanelReadOnlyV1]`
*   When a file is selected in the `TreeView`, load its content into the viewer.

## P3.4: (REMOVED - Whitelist Pattern Editing)

## P3.5: Handling File State Discrepancies Visually
*   When loading a profile, or after a "Refresh" (P2.9):
    *   Files listed in `profile.selected_paths` or `profile.deselected_paths` but NOT found on disk during scan: Mark visually in the TreeView (e.g., greyed out, special icon, different text color). The profile retains these paths. `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Files found on disk BUT not in `profile.selected_paths` or `profile.deselected_paths`: These are new/unclassified. They appear in the tree with `FileState::Unknown` (and corresponding checkbox state). `[FileStateNewUnknownV2]`, `[UiTreeViewVisualFileStatusV1]`

## P3.6: Full Profile Management UI
*   Dialog to list profiles (P1.3). `[UiMenuProfileManagementV1]`
*   Buttons for "New", "Duplicate", "Delete" profile.
*   "New Profile" dialog: `[ProfileOpCreateNewV2]`
    *   Input for profile name, root folder.
    *   Input/Selector for associated archive file path (can be initially empty; "Generate Archive" will prompt). `[ProfileDefAssociatedArchiveV1]`
*   "Edit Profile" (if implemented as a distinct feature):
    *   Allow changing name, root folder, and `archive_path`. `[UiMenuSetRootFolderV1]`
*   "Duplicate Profile" dialog: `[ProfileOpDuplicateExistingV1]`
    *   Prompt for new profile name.
    *   Prompt for new archive file path for the duplicated profile (or it starts as `None`).
*   "Delete Profile": Remove profile file. `[ProfileOpDeleteExistingV1]`

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
    (Changed "Select All Whitelisted" to "Select All Files")

## P4.5: Better Binary File Detection
*   Implement a more robust check (e.g., percentage of non-printable chars). (Supports `[TextFileFocusUTF8V1]`)
*   Visually indicate binary files or optionally exclude them. `[FutureBinaryFileDetectionSophisticatedV1]`

## P4.6: Background File System Monitoring
*   Integrate OS-level file system notifications (e.g., using `notify` crate or direct Win32 APIs like `ReadDirectoryChangesW`). `[FileSystemMonitorTreeViewV1]`
*   Automatically trigger update logic (similar to P2.9, but more targeted) when relevant file changes are detected in the background.
*   Provide UI feedback that monitoring is active and when changes are processed.
