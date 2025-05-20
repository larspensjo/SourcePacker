# Plan to improve testing of handle.rs and handle_test.rs

## Potential Improvements to Existing Tests:

1.  **Reduce File System Dependency Further (Beyond Config):**
    *   **Observation:** While `ConfigManagerOperations` is mocked, tests still interact with `core::load_profile`, `core::save_profile`, and `core::scan_directory` which hit the actual file system for profile JSON files and directory scanning.
    *   **Improvement Idea (aligns with future MockRefactor phases):** As you implement mocks for `ProfileManagerOperations` and `FileSystemScannerOperations` (Phases 2 and 3 of `MockRefactor.md`), update these tests to use those mocks. This will make them true unit tests for `MyAppLogic` by removing almost all file system interactions.
    *   **Example:**
        *   Instead of `create_temp_profile_file_in_profile_subdir` and then `core::load_profile`, a `MockProfileManager` would be configured to return a specific `Profile` object or `ProfileError`.
        *   Instead of `core::scan_directory` hitting the disk, a `MockFileSystemScanner` would return a predefined `Vec<FileNode>` or `FileSystemError`.

2.  **Clarity in Test Setup for Config Manager:**
    *   **Observation:** Some tests use `setup_logic_with_mock_config_manager()` and then immediately configure the returned `mock_config_manager`. Others use `setup_logic_with_window()` (which now also uses a mock) but ignore the mock handle because the test doesn't touch config.
    *   **Improvement Idea:**
        *   For tests that *do* care about config manager behavior, ensure they explicitly get the `mock_config_manager` handle and use it (as is done in `test_on_main_window_created_loads_last_profile_with_mock`).
        *   For tests that *don't* care about config manager behavior, using `let (mut logic, _mock_config_manager) = setup_logic_with_window();` is fine. This is already the case for many.

3.  **Asserting `PlatformCommand`s More Specifically:**
    *   **Observation:** Some tests check `cmds.len()` and then `match &cmds[0]` to assert the type of command.
    *   **Improvement Idea:** For more complex scenarios or when multiple commands are expected, consider helper functions or macros to assert the presence and properties of specific commands in the returned `Vec<PlatformCommand>`. This can make tests cleaner if there's a common pattern of command checking. For now, the current approach is okay for the number of commands being checked.

4.  **Testing `MyAppLogic::on_quit()`:**
    *   **Observation:** There's no test for the `on_quit` method.
    *   **Improvement Idea:** While `on_quit` currently only prints a message, if it were to do any cleanup or state changes in the future, a test should verify that. For now, a simple test that calls it and ensures no panic occurs might be a placeholder.

5.  **Error Handling Paths in `MyAppLogic`:**
    *   **Observation:** Current tests primarily focus on success paths.
    *   **Improvement Idea (relies on future mocks):**
        *   Test how `MyAppLogic::on_main_window_created` behaves if `self.config_manager.load_last_profile_name` returns an error.
        *   Test how it behaves if `core::load_profile` returns an error (will require `MockProfileManager`).
        *   Test how it behaves if `core::scan_directory` returns an error (will require `MockFileSystemScanner`).
        *   Assert that appropriate logging occurs (if possible to capture/mock) or that `MyAppLogic` enters a sensible default state.

## Suggestions for New Tests:

Here are areas where new tests could add significant value:

1.  **`on_main_window_created` - Profile Load Failure Scenarios:**
    *   **Scenario:** `config_manager.load_last_profile_name` returns `Ok(Some("profile_that_does_not_exist"))`.
        *   **Assert:** `MyAppLogic` should attempt `core::load_profile`, it should fail, and `MyAppLogic` should then proceed with a default scan/state. `current_profile_name` and `current_profile_cache` should be `None`.
    *   **Scenario:** `config_manager.load_last_profile_name` returns `Err(ConfigError::Io(...))`.
        *   **Assert:** `MyAppLogic` should proceed with a default scan/state.

2.  **`on_main_window_created` - Directory Scan Failure:**
    *   **Scenario:** A profile is successfully loaded, but the subsequent `core::scan_directory` for `profile.root_folder` fails. (Requires `MockFileSystemScanner` for easy simulation).
        *   **Assert:** `file_nodes_cache` should perhaps contain an error node (as currently implemented) or be empty. TreeView population command should reflect this. `current_archive_status` might be affected.

3.  **`AppEvent::FileOpenDialogCompleted` - More Scenarios:**
    *   **Scenario:** `File::open` fails for the selected profile path.
        *   **Assert:** `current_profile_name`/`cache` should be cleared or remain unchanged (depending on desired behavior for partial failure). No call to `save_last_profile_name` on the mock.
    *   **Scenario:** `serde_json::from_reader` fails (corrupt profile JSON).
        *   **Assert:** Similar to above.
    *   **Scenario:** Subsequent `core::scan_directory` fails after successful profile deserialization. (Requires `MockFileSystemScanner`).
        *   **Assert:** `current_profile_name`/`cache` might be set, but `file_nodes_cache` reflects scan error. `save_last_profile_name` on mock should have been called for the successfully deserialized profile.

4.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingArchive` - More Scenarios:**
    *   **Scenario:** `fs::write` to save the archive content fails.
        *   **Assert:** `current_profile_cache.archive_path` should *not* be updated. `current_archive_status` should not become `UpToDate`. Check if `core::save_profile` (to persist the archive path) is *not* called.
    *   **Scenario:** After successful `fs::write`, `core::save_profile` (to update `archive_path` in the profile JSON) fails. (Requires `MockProfileManager`).
        *   **Assert:** The archive file is written, but the in-memory `current_profile_cache.archive_path` might be set, but the persisted profile isn't updated. This is a tricky state; the test would highlight how `MyAppLogic` handles it.

5.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingProfile` - More Scenarios:**
    *   **Scenario:** `profile_save_path.file_stem()` returns `None` (e.g., path is just `/` or `.` ).
        *   **Assert:** `MyAppLogic` should handle this gracefully (e.g., log error, no profile saved, no call to `save_last_profile_name` on mock).
    *   **Scenario:** `core::save_profile` fails. (Requires `MockProfileManager`).
        *   **Assert:** `current_profile_name`/`cache` should not be updated to the new profile. No call to `save_last_profile_name` on mock.

6.  **`AppEvent::TreeViewItemToggled` - Complex Interactions:**
    *   **Scenario:** Toggling a folder that has children with mixed states.
        *   **Assert:** Verify the recursive state update logic via `core::state_manager::update_folder_selection` and ensure all `UpdateTreeItemVisualState` commands are generated for the parent and all children.
    *   **Scenario:** Toggling an item when `path_to_tree_item_id` somehow doesn't contain the `item_id` (edge case, defensive programming check).
        *   **Assert:** Graceful handling (e.g., error log, no panic).

7.  **`update_current_archive_status()` Logic (Indirectly):**
    *   **Observation:** This is a private method but called by many event handlers.
    *   **Improvement Idea:** Ensure tests for those event handlers cover scenarios that lead to different `ArchiveStatus` outcomes, and then assert `logic.current_archive_status`. For example:
        *   After loading a profile with no `archive_path`.
        *   After loading a profile where `archive_path` exists but the file is missing.
        *   After selecting/deselecting files to make the archive outdated/up-to-date.
        *   When no files are selected.

8.  **`ID_BUTTON_GENERATE_ARCHIVE_LOGIC` - `core::create_archive_content` Fails:**
    *   **Scenario:** Simulate `core::create_archive_content` returning an `Err`. (May require an `ArchiverOperations` trait and mock later).
        *   **Assert:** `pending_archive_content` and `pending_action` should not be set. No `ShowSaveFileDialog` command should be issued. An error should ideally be communicated to the user (future: via a status bar command).

**General Approach for Adding New Tests:**

*   **Follow the "Arrange, Act, Assert" (AAA) pattern.**
*   **Use `setup_logic_with_mock_config_manager()`** (and later, helpers that inject more mocks).
*   **Configure mocks** to simulate specific conditions or errors.
*   **Call the `MyAppLogic` method** being tested (usually `handle_event` or `on_main_window_created`).
*   **Assert:**
    *   The `Vec<PlatformCommand>` returned.
    *   The state of `pub(crate)` fields in `MyAppLogic` (e.g., `current_profile_name`, `file_nodes_cache`, `current_archive_status`).
    *   Interactions with mocks (e.g., `mock_config_manager.get_saved_profile_name()`).

Implementing all these would significantly increase test coverage and confidence. The key is to do it incrementally, especially as you introduce more mocks for other `core` services.

For now, without mocks for `ProfileManager` or `FileSystemScanner`, some of these suggestions are harder to implement in a pure unit-test style for `MyAppLogic`. However, identifying these scenarios is good preparation for when those mocks are available.
