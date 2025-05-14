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
*   `glob` (for whitelist pattern matching).

## P0.3: Basic `main.rs` and Window
*   Create a minimal Win32 window using `windows-rs`.
*   Implement the basic message loop.
*   This ensures the fundamental UI setup is working.

# Phase 1: Core Logic (Testable Modules)

## P1.1: Data Structures
*   Define `struct FileNode { path: PathBuf, name: String, is_dir: bool, state: FileState, children: Vec<FileNode> }`.
*   Define `enum FileState { Selected, Deselected, Unknown }`.
*   Define `struct Profile { name: String, root_folder: PathBuf, selected_paths: HashSet<PathBuf>, deselected_paths: HashSet<PathBuf>, whitelist_patterns: Vec<String> }`.
    *   *Note: Storing selected/deselected paths explicitly is simpler than trying to store the entire tree state for persistence.*

## P1.2: Directory Scanning & Filtering (Module: `file_system`)
*   Implement function: `scan_directory(root_path: &Path, whitelist_patterns: &[String]) -> Result<Vec<FileNode>, Error>`
    *   Uses `walkdir` to traverse directories.
    *   Uses `glob` to filter files based on whitelist patterns.
    *   Builds the initial `FileNode` tree, all with `FileState::Unknown`.
*   **Unit Tests:** Test with various directory structures and patterns.

## P1.3: Profile Management (Module: `profiles`)
*   Implement `fn save_profile(profile: &Profile, app_name: &str) -> Result<(), Error>`.
*   Implement `fn load_profile(profile_name: &str, app_name: &str) -> Result<Profile, Error>`.
*   Implement `fn list_profiles(app_name: &str) -> Result<Vec<String>, Error>`.
*   Implement `fn get_profile_dir(app_name: &str) -> PathBuf`.
*   **Unit Tests:** Test saving, loading, listing (mock file system or test with actual files in a temp dir).

## P1.4: State Application (Module: `state_manager`)
*   Implement `fn apply_profile_to_tree(tree: &mut Vec<FileNode>, profile: &Profile)`.
    *   Iterates through `tree`, setting `FileState` based on `profile.selected_paths` and `profile.deselected_paths`.
    *   Files not in either set but present in the scanned tree (and matching whitelist) remain `Unknown` or become `Unknown` if newly discovered.
*   Implement `fn update_folder_selection(node: &mut FileNode, select: bool)`.
    *   Recursively sets state of all children.
*   **Unit Tests:** Test application of states to various tree structures.

## P1.5: Archiving Logic (Module: `archiver`)
*   Implement `fn create_archive_content(root_node: &FileNode) -> String` (or takes a list of selected `FileNode`s).
    *   Traverses the `FileNode` tree.
    *   For `Selected` files, reads content (UTF-8) and prepends/appends headers.
*   **Unit Tests:** Test with mock file content and tree structures.

# Phase 2: Basic UI & Interaction

## P2.1: TreeView Population
*   Add a `TreeView` control to the main window.
*   Populate the `TreeView` from a `FileNode` tree (from P1.2).
*   Initially, no selection interaction, just display.

## P2.2: Basic Selection Visualization
*   Use standard checkboxes in the `TreeView`.
*   Implement tristate checkbox logic if `windows-rs` and `TreeView` support it directly, or simulate with custom drawing/icons if necessary (aim for simple first).
*   Link `TreeView` checkbox changes to update the `FileState` in the internal `FileNode` tree and vice-versa.

## P2.3: Folder Selection Propagation
*   When a folder checkbox is changed in the UI, trigger `update_folder_selection` (P1.4) on the corresponding `FileNode` and update the UI for children.

## P2.4: "Generate Archive" Button & Action
*   Add a button.
*   On click, call `create_archive_content` (P1.5) with the current state.
*   Prompt user to save the resulting string to a `.txt` file.

## P2.5: Profile Loading/Saving UI
*   Add basic menu items: "Load Profile", "Save Profile As".
*   "Load Profile": Show a dialog to pick a profile (from `list_profiles`), load it (P1.3), rescan directory, apply profile (P1.4), update TreeView.
*   "Save Profile As": Prompt for profile name, create `Profile` object from current state (root dir, selection, whitelist), save it (P1.3).

## P2.6: Initial Profile Load on Startup
*   Implement logic to store/retrieve the last used profile name (e.g., in a simple config file or registry).
*   On startup, attempt to load this profile.

## P2.7: Maybe the MyAppLogic::handle_event should call event.Execute(&mut commands, &self)?
*   That would take away almost all code from handle_event.

## P2.8: Some cleanup
*   Use AppData\Local instead of AppData\Roaming.
*   The error paths in MyAppLogic mostly eprintln!. You'll want to replace these with PlatformCommands to show error messages in a status field.
*   The Win32ApiInternalState::handle_window_message is getting large. Some message handlers could be broken out into separate functions within that impl block for better organization if they grow more complex.
*   Profile Name in create_profile_from_current_state: When saving, create_profile_from_current_state takes the new profile name. The original profile.name in MyAppLogic should perhaps be updated only after a successful save.
*   `Win32ApiInternalState::process_commands_from_event_handler` and `PlatformInterface::execute_command` seem to do the same thing. That can't be right?
*   There are some big functions in app.rs. I think it is possible to break out parts of them, and possibly finding common parts that can be re-used.
*   MyAppLogic.handle_event() should be possible to split up. Either as member functions, or as separate functions.
*   Access all over the place of Win32ApiInternalState.window_map could probably go through Win32ApiInternalState.get_hwnd_owner?

# Phase 3: Enhancements & UX Improvements

## P3.1: Status Bar
*   Add a status bar to the window.
*   Display current profile name (if loaded).
*   Display number of selected files.

## P3.2: Token Count (Module: `tokenizer_utils`)
*   Integrate a token counting library (e.g., `tiktoken-rs` or a simple word/space counter initially).
*   Implement `fn estimate_tokens(content: &str) -> usize`.
*   Update status bar with live token count of selected files. This might require recalculating on every selection change.

## P3.3: File Content Viewer
*   Add a read-only text control (e.g., `EDIT` control).
*   When a file is selected in the `TreeView`, load its content into the viewer.

## P3.4: Whitelist Pattern Editing
*   Provide a dialog or input field to view/edit the `whitelist_patterns` for the current profile.
*   Re-scan/re-filter the tree when patterns change.

## P3.5: Handling Missing Files Visually
*   When loading a profile, if `FileNode`s corresponding to `selected_paths` or `deselected_paths` are not found after scanning, mark them visually in the tree (e.g., greyed out, special icon). *This might require keeping the full tree structure from the profile, or just the paths.*

## P3.6: Full Profile Management UI
*   Dialog to list profiles (P1.3).
*   Buttons for "New", "Duplicate", "Delete" profile.
    *   New: Prompt for root, name, initial patterns.
    *   Duplicate: Prompt for new name, copy existing profile.
    *   Delete: Remove profile file.

# Phase 4: Advanced Features (Optional / Future)

## P4.1: File Name Search
*   Add a search input field.
*   Filter the `TreeView` to show only matching file/folder names.

## P4.2: Content Search
*   Input field for search string.
*   Button to "Search in Selected" or "Search in All (visible)".
*   Highlight files in the tree that contain the string.
*   (Optionally) Show occurrences in the File Content Viewer.

## P4.3: Refresh Tree View Button
*   Manually trigger a re-scan of the root directory and update the tree.

## P4.4: "Clear Selection" / "Select All Whitelisted" / "Invert Selection" options.

## P4.5: Better Binary File Detection
*   Implement a more robust check (e.g., percentage of non-printable chars).
*   Visually indicate binary files or optionally exclude them.
