Of course. Adding a profile-specific exclude pattern feature is an excellent idea that aligns well with the tool's purpose. It's very similar to how developers use `.gitignore` to keep archives clean.

Before diving into the detailed implementation plan, let's consider the user experience and a few related aspects you might not have mentioned.

### Suggestions and Considerations for the Feature

1.  **User Interface for Editing:**
    *   **Suggestion:** A straightforward approach is to add a new menu item, such as **"File" -> "Edit Exclude Patterns..."**.
    *   **Implementation:** This menu item would open a simple dialog window containing a title, a brief explanation (e.g., "Enter one pattern per line, like .gitignore"), a large multi-line text box for the patterns, and "OK" and "Cancel" buttons. This is isolated and easy to implement as a first version.

2.  **Applying Changes:**
    *   **Suggestion:** When the user clicks "OK" in the exclude patterns dialog, the application should automatically save the updated profile and trigger a "Refresh" of the file tree.
    *   **Benefit:** This provides immediate feedback, showing the user which files and folders have disappeared from the view as a result of their new patterns.

3.  **Default Exclusions (A "Good First Experience"):**
    *   **Suggestion:** Consider *always* ignoring a hardcoded, common set of patterns in addition to the user's custom list. This would provide a better out-of-the-box experience without requiring every user to manually ignore universal nuisances.
    *   **Examples of hardcoded patterns:**
        *   `.git/`
        *   `node_modules/`
        *   `target/` (for Rust projects)
        *   `.vscode/`, `.idea/`
    *   **Note:** The plan below will focus on implementing the user-configurable patterns, but this is a highly recommended subsequent improvement.

4.  **Visual Feedback:**
    *   **Confirmation:** Your proposal to have matching files simply be ignored (i.e., not appear in the TreeView) is the correct approach. It perfectly mirrors the behavior of `.gitignore`, which is intuitive for the target audience and avoids cluttering the UI with another file state like "Ignored".

Based on these considerations, here is a detailed, step-by-step plan to implement the feature. Each step is designed to leave the application in a compilable and runnable state.

---

### Detailed Implementation Plan: Profile Exclude Patterns

#### Step 1: Update the Core Profile Model and Requirements

First, we'll extend the core data structure to store the patterns and update the project's requirements documentation.

1.  **Modify `Profile` Struct:**
    *   **File:** `src/core/file_node.rs`
    *   **Action:** Add a new field `exclude_patterns` to the `Profile` struct. Use `#[serde(default)]` to ensure backward compatibility when loading older profile files that don't have this field.

    ```rust
    // In src/core/file_node.rs, inside the Profile struct

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Profile {
        pub name: String,
        pub root_folder: PathBuf,
        pub selected_paths: HashSet<PathBuf>,
        pub deselected_paths: HashSet<PathBuf>,
        pub archive_path: Option<PathBuf>,
        #[serde(default)]
        pub file_details: HashMap<PathBuf, FileTokenDetails>,
        #[serde(default)] // Add this line
        pub exclude_patterns: Vec<String>, // And this line
    }

    // Also update Profile::new()
    pub fn new(name: String, root_folder: PathBuf) -> Self {
        Profile {
            name,
            root_folder,
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: None,
            file_details: HashMap::new(),
            exclude_patterns: Vec::new(), // Initialize the new field
        }
    }
    ```

2.  **Update Requirements Document:**
    *   **File:** `doc/UserRequirements.md`
    *   **Action:** Add requirements for the new feature under the "Profile Definition" and "File System Monitoring" sections.

    ```markdown
    // Add to the "Profile Definition" section
    [ProfileDefExcludePatternsV1] * A list of user-defined, gitignore-style exclude patterns.

    // Add a new requirement under "File System Monitoring and Display"
    [FileSystemIgnoreUserPatternsV1] The file system scan must ignore all files and directories that match the exclude patterns defined in the active profile.
    ```

3.  **Verification:**
    *   Run `cargo test`. All existing tests should pass. The application builds and runs, and can load old profiles gracefully.

---

#### Step 2: Integrate Patterns into the File System Scanner

Now, we'll make the core file scanning logic aware of the new patterns.

1.  **Update `FileSystemScannerOperations` Trait:**
    *   **File:** `src/core/file_system.rs`
    *   **Action:** Modify the `scan_directory` method signature in the trait to accept the exclude patterns.

    ```rust
    // In src/core/file_system.rs
    pub trait FileSystemScannerOperations: Send + Sync {
        fn scan_directory(&self, root_path: &Path, exclude_patterns: &[String]) -> Result<Vec<FileNode>>;
    }
    ```

2.  **Implement Pattern Matching in `CoreFileSystemScanner`:**
    *   **File:** `src/core/file_system.rs`
    *   **Action:** Use the `ignore::overrides::OverrideBuilder` to add the custom patterns to the `WalkBuilder`.

    ```rust
    // In src/core/file_system.rs, inside CoreFileSystemScanner::scan_directory

    fn scan_directory(&self, root_path: &Path, exclude_patterns: &[String]) -> Result<Vec<FileNode>> {
        if !root_path.is_dir() {
            return Err(FileSystemError::InvalidPath(root_path.to_path_buf()));
        }
        log::debug!(/* ... */);

        // --- ADD THIS BLOCK ---
        let mut override_builder = ignore::overrides::OverrideBuilder::new(root_path);
        for pattern in exclude_patterns {
            // The '!' prefix in gitignore means "don't ignore".
            // We'll treat all patterns as ignore patterns.
            let pattern_to_add = if pattern.starts_with('!') {
                &pattern[1..]
            } else {
                pattern
            };
            if let Err(e) = override_builder.add(&format!("!{}", pattern_to_add)) {
                log::warn!("Invalid exclude pattern '{}': {}", pattern, e);
            }
        }
        let overrides = override_builder.build()?;
        // --- END BLOCK ---

        // Use WalkBuilder from the 'ignore' crate.
        let walker = WalkBuilder::new(root_path)
            .standard_filters(true)
            .overrides(overrides) // --- ADD THIS LINE ---
            /* ... rest of the builder config ... */
            .build();

        // ... rest of the function remains the same ...
    }
    ```

3.  **Update Call Sites of `scan_directory`:**
    *   **File:** `src/core/profile_runtime_data.rs`
        *   In `load_profile_into_session`, change the call to `file_system_scanner.scan_directory(...)` to pass the patterns from the profile being loaded.
        ```rust
        // In ProfileRuntimeData::load_profile_into_session
        match file_system_scanner.scan_directory(&self.root_path_for_scan, &loaded_profile.exclude_patterns) {
            // ...
        }
        ```
    *   **File:** `src/app_logic/handler.rs`
        *   In `handle_menu_refresh_file_list_clicked`, retrieve the exclude patterns from `app_session_data_ops` along with the other data and pass them to the scanner.
    *   **File:** `src/app_logic/handler_tests.rs` and `src/core/file_system.rs`
        *   Update all mock implementations and test calls to match the new `scan_directory` signature. This will cause compilation errors that guide you. For tests not concerned with this feature, passing an empty slice `&[]` is sufficient.

4.  **Add Unit Test for Scanner:**
    *   **File:** `src/core/file_system.rs` (in the `tests` module)
    *   **Action:** Create a new test that sets up a temporary directory with files like `test.log`, `build/output.bin`, and `src/main.rs`. Call `scan_directory` with patterns like `*.log` and `build/`. Assert that the returned `FileNode` tree does *not* contain the log file or the build directory.

5.  **Verification:**
    *   Run `cargo test`. After fixing the compile errors from the signature change and adding the new test, all tests should pass. The application now correctly applies exclude patterns if they are manually added to a profile's JSON file.

---

#### Step 3: Implement the User Interface for Editing

Now, let's build the dialog for the user to edit the patterns.

1.  **Update `PlatformCommand`, `AppEvent`, and `MenuAction`:**
    *   **File:** `src/platform_layer/types.rs`
    *   **Action:** Add the new menu action and the command/event pair for the dialog.

    ```rust
    // In MenuAction enum
    EditExcludePatterns,

    // In PlatformCommand enum
    ShowExcludePatternsDialog {
        window_id: WindowId,
        title: String,
        patterns: String, // A single string with newlines
    },

    // In AppEvent enum
    ExcludePatternsDialogCompleted {
        window_id: WindowId,
        saved: bool, // True if OK was clicked, false for Cancel
        patterns: String,
    },
    ```

2.  **Add New Menu Item:**
    *   **File:** `src/ui_description_layer.rs`
    *   **Action:** Add the "Edit Exclude Patterns..." item to the "File" menu. Place it after "Set Archive Path...".

    ```rust
    // In build_main_window_static_layout(), inside the file_menu_items vec
    MenuItemConfig {
        action: Some(MenuAction::SetArchivePath),
        text: "Set Archive Path...".to_string(),
        children: Vec::new(),
    },
    // Add this item:
    MenuItemConfig {
        action: Some(MenuAction::EditExcludePatterns),
        text: "Edit Exclude Patterns...".to_string(),
        children: Vec::new(),
    },
    ```

3.  **Implement the Dialog in the Platform Layer:**
    *   **File:** `src/platform_layer/controls/dialog_handler.rs`
    *   **Action:** This is the most complex part. You will need to create a new `handle_show_exclude_patterns_dialog_command` function. This will be very similar to your existing `handle_show_input_dialog_command`.
        *   Create a new dialog procedure (`exclude_patterns_dialog_proc`).
        *   Create a new `build_exclude_patterns_dialog_template` function that constructs a template with a large, multi-line `EDIT` control (`ES_MULTILINE | ES_WANTRETURN | WS_VSCROLL`).
        *   The dialog proc on `WM_INITDIALOG` will populate the edit control with the patterns string.
        *   On `WM_COMMAND` for `IDOK`, it will read the text from the edit control and end the dialog.

4.  **Verification:**
    *   Run `cargo run`. The "Edit Exclude Patterns..." menu item should appear. Clicking it should open a dialog box. This dialog won't be wired up to the application logic yet, but the UI will be present.

---

#### Step 4: Wire up the Application Logic

Finally, let's connect the UI to the core logic.

1.  **Update `ProfileRuntimeData` to hold live patterns:**
    *   **File:** `src/core/profile_runtime_data.rs`
    *   **Action:** Add `exclude_patterns: Vec<String>` to the `ProfileRuntimeData` struct and `set_exclude_patterns` to the `ProfileRuntimeDataOperations` trait. Update `load_profile_into_session` to populate this field from the loaded profile.

2.  **Implement Logic in `MyAppLogic`:**
    *   **File:** `src/app_logic/handler.rs`
    *   **Action:**
        1.  Add a `handle_menu_edit_exclude_patterns_clicked` method. In the main `handle_event` match, call this for `MenuAction::EditExcludePatterns`.
        2.  Inside this new method, get the current profile's patterns from `app_session_data_ops`, join them into a newline-separated string, and enqueue the `PlatformCommand::ShowExcludePatternsDialog`.
        3.  Add a `handle_exclude_patterns_dialog_completed` method. In the `handle_event` match, call this for `AppEvent::ExcludePatternsDialogCompleted`.
        4.  Inside this new method:
            *   If the dialog was cancelled (`saved: false`), do nothing.
            *   If saved, parse the incoming string into a `Vec<String>`.
            *   Create a profile snapshot from the current session data.
            *   Update the `exclude_patterns` on the snapshot.
            *   Call `self.profile_manager.save_profile(...)` with the updated snapshot.
            *   On successful save, update the in-memory patterns in `app_session_data_ops` by calling the new `set_exclude_patterns` method.
            *   Finally, call `self.handle_menu_refresh_file_list_clicked()` to trigger a re-scan with the new rules.

3.  **Add Unit Test for App Logic:**
    *   **File:** `src/app_logic/handler_tests.rs`
    *   **Action:** Create a test `test_exclude_patterns_dialog_saved_updates_profile_and_refreshes`.
        *   **Arrange:** Set up `MyAppLogic` with mocks and an active profile.
        *   **Act:** Send an `AppEvent::ExcludePatternsDialogCompleted` with `saved: true` and a new pattern string.
        *   **Assert:** Verify that `mock_profile_manager.save_profile` was called and that the profile passed to it contains the new patterns. Also, assert that `mock_file_system_scanner.scan_directory` was called, indicating a refresh was triggered.

4.  **Verification:**
    *   Run `cargo test` to ensure the new logic is covered.
    *   Run `cargo run` and test the full flow: open the dialog, add a pattern (e.g., `*.md`), save, and verify that the corresponding files disappear from the TreeView.

This completes the implementation. This plan breaks the feature into manageable, testable pieces and ensures the application remains stable throughout the development process.
