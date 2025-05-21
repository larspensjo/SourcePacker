# Improved Plan for Testing MyAppLogic (handler.rs and handler_tests.rs)

This plan outlines strategies to enhance the unit testing of `MyAppLogic` in `handler.rs`. The primary goal is to increase test coverage, reliability, and isolate `MyAppLogic` from external dependencies (especially the file system) by leveraging the Dependency Injection (DI) and mocking approach detailed in `MockRefactor.md`.

## Current State & Core Challenge

While `MockConfigManager` is in place, many existing tests for `MyAppLogic` still interact directly with `core` functions like `core::load_profile`, `core::save_profile`, and `core::scan_directory`. These functions perform real file system I/O, making the tests slower, more brittle, and not true unit tests for `MyAppLogic`'s own logic.

## Overarching Strategy

The key to improving these tests is to systematically replace direct calls to `core` service functions with interactions through traits (`ProfileManagerOperations`, `FileSystemScannerOperations`, etc.), for which mock implementations will be used in tests. This aligns with the phased approach in `MockRefactor.md`.

## Phase 1: Enhancements to Existing Tests (As Mocks Become Available)

1.  **Full Dependency Injection with Mocks:**
    *   **Observation:** Tests rely on real file system operations for profiles and directory scanning.
    *   **Improvement:** As `MockProfileManager` and `MockFileSystemScanner` (and potentially `MockArchiver`, `MockStateManager`) are implemented (Phases 2, 3, and 4 of `MockRefactor.md`), refactor existing tests to:
        *   Inject these mocks into `MyAppLogic` during test setup.
        *   Configure mocks to return specific `Profile` objects, `Vec<FileNode>` trees, `ArchiveStatus` values, or errors, rather than creating temporary files/directories on disk.
        *   Verify that `MyAppLogic` calls the correct methods on these mocks with the expected arguments.
    *   This will make the tests true unit tests for `MyAppLogic`, significantly reducing execution time and flakiness.

2.  **Clarity in Test Setup for `ConfigManagerOperations`:**
    *   **Observation:** Current setup is generally good with `MockConfigManager`.
    *   **Improvement:** Continue ensuring that tests requiring specific config behavior explicitly configure the `MockConfigManager`. For tests not interacting with config, ignoring the mock handle is fine.

3.  **Asserting `PlatformCommand`s More Specifically:**
    *   **Observation:** Current assertions on `PlatformCommand`s are adequate for now.
    *   **Improvement (Future Consideration):** If tests become more complex with many expected commands, consider helper functions or macros for asserting the presence and properties of specific commands. The current approach is acceptable.

4.  **Testing `MyAppLogic::on_quit()`:**
    *   **Observation:** No dedicated test.
    *   **Improvement:** Add a simple test to call `on_quit()` and ensure no panics. If `on_quit` evolves to perform state changes or cleanup, expand the test to verify these actions.

5.  **Testing Error Handling Paths (Enhanced):**
    *   **Observation:** Current tests lean towards success paths.
    *   **Improvement (relies on future mocks):**
        *   Test how `MyAppLogic::on_main_window_created` behaves if `self.config_manager.load_last_profile_name` returns an error.
        *   Test behavior if `self.profile_manager.load_profile` (mocked) returns an error.
        *   Test behavior if `self.file_system_scanner.scan_directory` (mocked) returns an error.
        *   **Crucially, assert that `MyAppLogic` not only enters a sensible default state but also generates appropriate `PlatformCommand`s to inform the user of errors (e.g., updating a status bar message), if this is the intended design (aligns with `TechErrorHandlingGracefulV1`).**

## Phase 2: Implementing New, Comprehensive Unit Tests

This section details new test scenarios focusing on isolated `MyAppLogic` behavior, assuming full DI with mocks.

**Test Data Management Strategy:**

*   **Utilize Test Data Builders/Fixtures:** As tests require mock `Profile` objects, `Vec<FileNode>` structures, etc., create helper functions, builder patterns, or constants for this test data. This will:
    *   Keep individual test setup sections concise and readable.
    *   Promote reuse of common test data structures.
    *   Make it easier to create variations for different test scenarios.

**New Test Scenarios:**

1.  **`on_main_window_created` - Profile Load Failure Scenarios:**
    *   **Scenario:** `config_manager.load_last_profile_name` returns `Ok(Some("profile_that_does_not_exist"))`.
        *   **Assert:** `MockProfileManager.load_profile` is called, it returns an error (as configured). `MyAppLogic` proceeds with a default scan (verified via `MockFileSystemScanner.scan_directory` call). `current_profile_name` and `current_profile_cache` are `None`. Assert any `PlatformCommand`s for status updates (e.g., "Failed to load profile 'X', using default").
    *   **Scenario:** `config_manager.load_last_profile_name` returns `Err(ConfigError::Io(...))`.
        *   **Assert:** `MyAppLogic` proceeds with a default scan. No attempt to load a profile via `MockProfileManager`. Assert relevant `PlatformCommand`s.

2.  **`on_main_window_created` - Directory Scan Failure:**
    *   **Scenario:** A profile is successfully loaded (via `MockProfileManager`), but the subsequent `MockFileSystemScanner.scan_directory` for `profile.root_folder` fails.
        *   **Assert:** `file_nodes_cache` reflects the error state (e.g., empty or specific error node). TreeView population command reflects this. `current_archive_status` is appropriate. Assert `PlatformCommand`s for status updates.

3.  **`AppEvent::FileOpenDialogCompleted` (Profile Load) - More Scenarios:**
    *   **Scenario:** User selects a file, but `MockProfileManager.load_profile_from_path` (assuming this method or similar is added to the trait and used) returns `Err(ProfileError::Io)`.
        *   **Assert:** `current_profile_name`/`cache` remain unchanged or are cleared. No call to `MockConfigManager.save_last_profile_name`. Assert error notification `PlatformCommand`s.
    *   **Scenario:** `MockProfileManager.load_profile_from_path` returns `Err(ProfileError::Serde)` (corrupt profile).
        *   **Assert:** Similar to above.
    *   **Scenario:** Successful profile deserialization by `MockProfileManager`, but subsequent `MockFileSystemScanner.scan_directory` fails.
        *   **Assert:** `current_profile_name`/`cache` are set. `file_nodes_cache` reflects scan error. `MockConfigManager.save_last_profile_name` *was* called for the successfully loaded profile. Assert `PlatformCommand`s for partial success/scan failure.

4.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingArchive` - More Scenarios:**
    *   **Scenario:** `fs::write` to save the archive content fails (this might be harder to mock directly if `MyAppLogic` does the `fs::write`; if `ArchiverOperations` trait is introduced for `create_archive_content` *and* saving, this becomes easier).
        *   **If `MyAppLogic` does `fs::write`:** This part remains an integration point.
        *   **If `ArchiverOperations.save_archive_content` is mocked to fail:** Assert `current_profile_cache.archive_path` is *not* updated. `current_archive_status` does not become `UpToDate`. `MockProfileManager.save_profile` (to persist updated `archive_path`) is *not* called. Assert `PlatformCommand`s for error.
    *   **Scenario:** After successful archive write, `MockProfileManager.save_profile` (to update `archive_path` in profile JSON) fails.
        *   **Assert:** In-memory `current_profile_cache.archive_path` might be set, but an error should be signaled. The persisted profile isn't updated. Assert error `PlatformCommand`s.
    *   **Ensure `pending_action` and `pending_archive_content` are cleared** regardless of success or failure of the save operation itself.

5.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingProfile` - More Scenarios:**
    *   **Scenario:** `profile_save_path.file_stem()` returns `None` (e.g., path is just `/` or `.` ).
        *   **Assert:** Graceful handling (e.g., log, error `PlatformCommand`s). No profile saved via `MockProfileManager`. No call to `MockConfigManager.save_last_profile_name`.
    *   **Scenario:** `MockProfileManager.save_profile` fails.
        *   **Assert:** `current_profile_name`/`cache` are not updated to the new profile. No call to `MockConfigManager.save_last_profile_name`. Assert error `PlatformCommand`s.
    *   **Ensure `pending_action` is cleared.**

6.  **`AppEvent::TreeViewItemToggled` - Complex Interactions:**
    *   **Scenario:** Toggling a folder that has children with mixed states (requires `MockStateManager` or careful `FileNode` setup if `core::state_manager` functions are still called directly but with testable inputs).
        *   **Assert:** Verify the recursive state update logic and ensure all necessary `UpdateTreeItemVisualState` commands are generated for the parent and all children.
    *   **Scenario:** Toggling an item when `path_to_tree_item_id` somehow doesn't contain the `item_id` (edge case for defensive programming).
        *   **Assert:** Graceful handling (e.g., error log, no panic, possibly error `PlatformCommand`).

7.  **`update_current_archive_status()` Logic (Indirectly):**
    *   **Improvement Idea:** Ensure tests for event handlers that call `update_current_archive_status()` cover scenarios leading to different `ArchiveStatus` outcomes.
        *   After loading a profile with no `archive_path` (mock `Profile` return).
        *   After loading a profile where `archive_path` exists but `MockArchiver.check_archive_status` (or similar) indicates file is missing or returns specific status.
        *   After selecting/deselecting files to make the archive outdated/up-to-date based on mock file timestamps/states.
        *   When no files are selected.
    *   **Assert:** The value of `logic.current_archive_status` and any `PlatformCommand`s related to status bar updates.

8.  **`ID_BUTTON_GENERATE_ARCHIVE_LOGIC` - `core::create_archive_content` Fails:**
    *   **Scenario:** (Requires `ArchiverOperations` trait and mock) Simulate `MockArchiver.create_archive_content` returning an `Err`.
        *   **Assert:** `pending_archive_content` and `pending_action` are not set. No `ShowSaveFileDialog` command. An error is communicated to the user (e.g., via status bar `PlatformCommand`).

## General Approach for Adding New Tests

*   **Follow "Arrange, Act, Assert" (AAA):** Clearly structure each test.
*   **Use Descriptive Test Names:** Employ clear names that indicate the scenario and expected behavior (e.g., `test_event_X_when_condition_Y_should_produce_Z`).
*   **Setup with Mocks:** Use helper functions like `setup_logic_with_mock_config_manager()` and extend them to inject `MockProfileManager`, `MockFileSystemScanner`, etc.
*   **Configure Mocks:** Set up mock objects to return specific data or simulate error conditions relevant to the test case.
*   **Leverage Test Data Builders:** Use helper functions/builders to create complex input data (e.g., `FileNode` trees, `Profile` instances) for mocks or direct input.
*   **Act:** Call the `MyAppLogic` method under test (usually `handle_event` or `on_main_window_created`).
*   **Assert:**
    *   The `Vec<PlatformCommand>` returned (order and content).
    *   The state of `pub(crate)` fields in `MyAppLogic` (e.g., `current_profile_name`, `file_nodes_cache`, `current_archive_status`, and crucially, `pending_action` to ensure it's correctly set/cleared).
    *   Interactions with mocks (e.g., `mock_profile_manager.save_profile_called_with(...)`).
    *   Any `PlatformCommand`s generated for user feedback, especially for error conditions.
*   **Consider User Interaction Story:** While testing individual handlers, keep in mind the broader sequence of user actions (state transitions) to ensure robust coverage.

## Conclusion

Implementing this plan, especially in conjunction with the `MockRefactor.md` strategy, will significantly elevate the quality and reliability of `MyAppLogic`. The focus on DI, comprehensive mocking, and testing of failure/edge cases is crucial for a robust application.
