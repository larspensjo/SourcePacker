# Improved Plan for Testing MyAppLogic (handler.rs and handler_tests.rs)

This plan outlines strategies to enhance the unit testing of `MyAppLogic` in `handler.rs`. The primary goal is to increase test coverage, reliability, and isolate `MyAppLogic` from external dependencies (especially the file system) by leveraging the completed Dependency Injection (DI) and mocking approach.

## Current State & Core Advantage

`MyAppLogic` now exclusively interacts with core services (`config`, `profiles`, `file_system`, `archiver`, `state_manager`) through their respective traits (`ConfigManagerOperations`, `ProfileManagerOperations`, `FileSystemScannerOperations`, `ArchiverOperations`, `StateManagerOperations`). This means all external dependencies can be replaced with mock implementations in unit tests.

The core challenge of direct file system I/O and other side effects during `MyAppLogic` unit tests has been fully mitigated. Tests can now be true unit tests, focusing solely on `MyAppLogic`'s internal logic, event handling, state transitions, and command generation.

## Overarching Strategy: Comprehensive Mock-Based Unit Testing

With the DI foundation in place, the strategy is to:
1.  Develop comprehensive unit tests for each event handler and significant internal logic path in `MyAppLogic`.
2.  Inject mock implementations for all `core` service dependencies (`MockConfigManager`, `MockProfileManager`, `MockFileSystemScanner`, `MockArchiver`, `MockStateManager`).
3.  Configure these mocks to simulate diverse scenarios: successful operations, various error conditions, specific data returns (e.g., different `Profile` objects, `FileNode` trees, `ArchiveStatus` values).
4.  Assert `MyAppLogic`'s internal state changes, the `PlatformCommand`s it generates, and its interactions with the mock objects (i.e., verifying that the correct mock methods are called with the expected arguments).

## Leveraging Full Dependency Injection in Tests

1.  **Consistent Use of All Mocks:**
    *   **Current State:** All core dependencies are mockable.
    *   **Action:** Ensure all `MyAppLogic` unit tests utilize the full suite of mock objects (`MockConfigManager`, `MockProfileManager`, `MockFileSystemScanner`, `MockArchiver`, `MockStateManager`) injected via the `setup_logic_with_mocks()` helper or similar test fixtures.

2.  **Clarity in Mock Configuration:**
    *   **Action:** For each test, clearly configure the injected mocks to establish the specific preconditions for the scenario under test. For mocks whose behavior is not relevant to a particular test, default mock behavior is acceptable, but explicit setup for relevant interactions is preferred.

3.  **Precise Assertion of `PlatformCommand`s:**
    *   **Action:** Continue to assert the exact `PlatformCommand`s generated, including their order and parameters. For complex sequences of commands, consider helper functions or custom matchers if beneficial, though direct `assert_eq!` on the vector of commands is often sufficient.

4.  **Testing `MyAppLogic::on_quit()`:**
    *   **Action:** Add a simple test to call `on_quit()` and ensure no panics. If `on_quit` evolves to perform state changes or cleanup (e.g., saving unsaved state via mocks), expand the test to verify these actions and interactions with mocks.

5.  **Comprehensive Error Handling Path Testing:**
    *   **Action:** Systematically test `MyAppLogic`'s response to errors returned by any of its mocked dependencies. For each relevant event handler:
        *   Configure the relevant mock (e.g., `MockProfileManager.load_profile`, `MockFileSystemScanner.scan_directory`, `MockArchiver.create_archive_content`, `MockArchiver.save_archive_content`, `MockProfileManager.save_profile`) to return an `Err` variant.
        *   **Assert:**
            *   `MyAppLogic` enters a sensible internal state (e.g., clears pending actions, reverts to a default profile, sets an error status).
            *   Appropriate `PlatformCommand`s are generated to inform the user of the error (e.g., updating a status bar message, showing an error dialog placeholder). This aligns with `TechErrorHandlingGracefulV1`.
            *   Subsequent operations are (or are not) attempted based on the error.

## Implementing New, Comprehensive Unit Tests

This section details new test scenarios focusing on isolated `MyAppLogic` behavior, fully utilizing the DI and mock infrastructure.

**Test Data Management Strategy:**

*   **Utilize Test Data Builders/Fixtures:**
    *   **Action:** Create helper functions, builder patterns (e.g., for `FileNode` trees, `Profile` objects), or constants for commonly used test data. This will:
        *   Keep individual test setup sections concise and readable.
        *   Promote reuse of common test data structures.
        *   Make it easier to create variations for different test scenarios (e.g., a `Profile` with/without an `archive_path`, a `FileNode` tree with specific states).

**New Test Scenarios (Illustrative Examples):**

1.  **`on_main_window_created` - Initialization Scenarios:**
    *   **Scenario:** `MockConfigManager.load_last_profile_name` returns `Ok(Some("profile_that_does_not_exist"))`.
        *   **Configure:** `MockProfileManager.load_profile` to return `Err(ProfileError::ProfileNotFound)`. `MockFileSystemScanner.scan_directory` to return a default tree for the fallback path.
        *   **Assert:** `MockProfileManager.load_profile` called. `MockFileSystemScanner.scan_directory` called for fallback. `current_profile_name` and `current_profile_cache` are `None`. `MockStateManager.apply_profile_to_tree` is *not* called with a loaded profile. `MockArchiver.check_archive_status` reflects no profile. Assert `PlatformCommand`s for status updates (e.g., "Failed to load profile 'X', using default").
    *   **Scenario:** `MockConfigManager.load_last_profile_name` returns `Err(ConfigError::Io(...))`.
        *   **Assert:** No call to `MockProfileManager.load_profile`. `MockFileSystemScanner.scan_directory` called for fallback. Assert relevant `PlatformCommand`s.
    *   **Scenario:** Successful profile load by `MockConfigManager` and `MockProfileManager`, but `MockFileSystemScanner.scan_directory` for `profile.root_folder` fails.
        *   **Assert:** `current_profile_name` and `current_profile_cache` are set. `file_nodes_cache` reflects the error (e.g., empty or specific error node). `MockStateManager.apply_profile_to_tree` is *not* called or called with an empty tree. TreeView population command reflects this. `current_archive_status` is appropriate. Assert `PlatformCommand`s for status updates.
    *   **Scenario:** Successful profile load and scan.
        *   **Assert:** `MockStateManager.apply_profile_to_tree` is called with the scanned `file_nodes_cache` and the loaded `Profile`. `MockArchiver.check_archive_status` is called with the (potentially modified by mock state manager) `file_nodes_cache` and profile. Assert UI population commands.

2.  **`AppEvent::FileOpenDialogCompleted` (Profile Load) - Scenarios:**
    *   **Scenario:** User selects a file, but `MockProfileManager.load_profile_from_path` returns `Err(ProfileError::Io)` or `Err(ProfileError::Serde)`.
        *   **Assert:** `current_profile_name`/`cache` remain unchanged or are cleared. No call to `MockConfigManager.save_last_profile_name`. No call to `MockFileSystemScanner.scan_directory` for the failed profile's root. No call to `MockStateManager.apply_profile_to_tree`. Assert error notification `PlatformCommand`s.
    *   **Scenario:** Successful `MockProfileManager.load_profile_from_path`, but subsequent `MockFileSystemScanner.scan_directory` fails.
        *   **Assert:** `current_profile_name`/`cache` are set. `MockConfigManager.save_last_profile_name` *was* called. `file_nodes_cache` reflects scan error. No call to `MockStateManager.apply_profile_to_tree` or called with empty/error tree. Assert `PlatformCommand`s for partial success/scan failure.
    *   **Scenario:** Successful profile load and scan via `FileOpenDialogCompleted`.
        *   **Assert:** `MockConfigManager.save_last_profile_name` called. `MockFileSystemScanner.scan_directory` called. `MockStateManager.apply_profile_to_tree` called with new scan and profile. `MockArchiver.check_archive_status` called. UI update commands.

3.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingArchive` - Scenarios:**
    *   **Scenario:** `MockArchiver.save_archive_content` fails.
        *   **Assert:** `pending_action` and `pending_archive_content` cleared. `current_profile_cache.archive_path` is *not* updated (if it was `None`). `MockProfileManager.save_profile` (to persist updated `archive_path`) is *not* called. `current_archive_status` does not become `UpToDate`. Assert error `PlatformCommand`s.
    *   **Scenario:** `MockArchiver.save_archive_content` succeeds, but subsequent `MockProfileManager.save_profile` (to update `archive_path` in profile JSON) fails.
        *   **Assert:** `pending_action` and `pending_archive_content` cleared. In-memory `current_profile_cache.archive_path` *is* set. Error `PlatformCommand` signaled for profile save failure. `MockArchiver.check_archive_status` still called to reflect new archive file.
    *   **Scenario:** Full success saving archive and updating profile.
        *   **Assert:** `pending_action` and `pending_archive_content` cleared. `MockArchiver.save_archive_content` called. `current_profile_cache.archive_path` updated. `MockProfileManager.save_profile` called with updated profile. `MockArchiver.check_archive_status` called (should result in `UpToDate` if mock configured so).

4.  **`AppEvent::FileSaveDialogCompleted` - `PendingAction::SavingProfile` - Scenarios:**
    *   **Scenario:** `profile_save_path.file_stem()` returns `None`.
        *   **Assert:** Graceful handling. No profile saved via `MockProfileManager`. No call to `MockConfigManager.save_last_profile_name`. `pending_action` cleared. Error `PlatformCommand`s.
    *   **Scenario:** `MockProfileManager.save_profile` fails.
        *   **Assert:** `current_profile_name`/`cache` are not updated. No call to `MockConfigManager.save_last_profile_name`. `pending_action` cleared. Error `PlatformCommand`s.
    *   **Scenario:** Full success saving profile.
        *   **Assert:** `MockProfileManager.save_profile` called. `current_profile_name`/`cache` updated. `MockConfigManager.save_last_profile_name` called. `MockArchiver.check_archive_status` called. `pending_action` cleared.

5.  **`AppEvent::TreeViewItemToggled` - Scenarios:**
    *   **Scenario:** Toggling an item.
        *   **Setup:** Pre-populate `file_nodes_cache` and `path_to_tree_item_id`.
        *   **Assert:** `MyAppLogic::find_filenode_mut` logic correctly identifies the node. `MockStateManager.update_folder_selection` is called with the correct node reference and the new `FileState`. `collect_visual_updates_recursive` generates correct `UpdateTreeItemVisualState` commands based on the (mock-modified) `file_nodes_cache`. `MockArchiver.check_archive_status` is called with the updated `file_nodes_cache`.
    *   **Scenario:** Toggling an item when `path_to_tree_item_id` somehow doesn't contain the `item_id` (edge case).
        *   **Assert:** Graceful handling (e.g., error log, no panic, possibly error `PlatformCommand`). No call to `MockStateManager` or `MockArchiver`.

6.  **`ID_BUTTON_GENERATE_ARCHIVE_LOGIC` (Generate Archive Button) - Scenarios:**
    *   **Scenario:** `MockArchiver.create_archive_content` returns an `Err`.
        *   **Assert:** `pending_archive_content` and `pending_action` are not set. No `ShowSaveFileDialog` command. Error `PlatformCommand` communicated to user.
    *   **Scenario:** `MockArchiver.create_archive_content` succeeds.
        *   **Assert:** `pending_archive_content` set with content from mock. `pending_action` set to `SavingArchive`. `ShowSaveFileDialog` command generated.

7.  **`update_current_archive_status()` Logic (Tested via events like profile load, item toggle, archive save):**
    *   **Action:** Ensure tests for event handlers that trigger `update_current_archive_status()` cover scenarios where `MockArchiver.check_archive_status` is configured to return different `ArchiveStatus` values (e.g., `UpToDate`, `OutdatedRequiresUpdate`, `NotYetGenerated`, `ArchiveFileMissing`, `NoFilesSelected`, `ErrorChecking`).
    *   **Assert:** The value of `logic.current_archive_status` is correctly updated. Relevant `PlatformCommand`s for status bar updates are generated (if this UI feedback is implemented).

## General Approach for Adding New Tests

*   **Follow "Arrange, Act, Assert" (AAA):** Clearly structure each test.
*   **Use Descriptive Test Names:** Employ clear names that indicate the scenario and expected behavior (e.g., `test_event_X_when_condition_Y_should_produce_Z_and_call_mock_A`).
*   **Setup with Mocks:** Use and extend `setup_logic_with_mocks()` to inject all necessary mock dependencies.
*   **Configure Mocks:** For each test, meticulously set up mock objects to return specific data or simulate error conditions relevant to the test case.
*   **Leverage Test Data Builders:** Use helper functions/builders to create complex input data (e.g., `FileNode` trees, `Profile` instances) for mocks or direct input to `MyAppLogic` test setup.
*   **Act:** Call the `MyAppLogic` method under test (usually `handle_event` or `on_main_window_created`).
*   **Assert:**
    *   The `Vec<PlatformCommand>` returned (order and content).
    *   The state of `pub(crate)` fields in `MyAppLogic` (e.g., `current_profile_name`, `file_nodes_cache` state after mock interactions, `current_archive_status`, and crucially, `pending_action` to ensure it's correctly set/cleared).
    *   Interactions with all relevant mocks (e.g., `mock_profile_manager.save_profile_called_with(...)`, `mock_state_manager.apply_profile_to_tree_called_with(...)`, `mock_archiver.check_archive_status_called_with(...)`).
    *   Any `PlatformCommand`s generated for user feedback, especially for error conditions.
*   **Consider User Interaction Story:** While testing individual handlers, keep in mind the broader sequence of user actions and application state transitions to ensure robust coverage.

## Conclusion

By consistently applying these strategies, the unit tests for `MyAppLogic` will become highly effective at verifying its correctness, improving maintainability, and providing confidence during future development and refactoring. The full DI and mocking infrastructure is now a powerful tool to achieve this.
