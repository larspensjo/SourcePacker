# Improved Plan for Testing MyAppLogic (handler.rs and handler_tests.rs)

The `MyAppLogic` component now fully utilizes DependencyInjection (DI) for its interactions with core services (`config`, `profiles`, `file_system`, `archiver`, `state_manager`) through their respective traits. This DI foundation allows all external dependencies to be replaced with mock implementations in unit tests, enabling true unit testing of `MyAppLogic`'s internal logic, event handling, state transitions, and command generation.

This plan outlines the next steps to comprehensively leverage this setup to increase test coverage and reliability.

## Step-by-Step Plan for Enhancing `MyAppLogic` Unit Tests

### 1. Ensure Consistent Test Setup
*   **Action:** For all new `MyAppLogic` unit tests, consistently use the `setup_logic_with_mocks()` helper (or similar test fixtures that inject the full suite of mock dependencies: `MockConfigManager`, `MockProfileManager`, `MockFileSystemScanner`, `MockArchiver`, `MockStateManager`).
*   **Action:** In each test, clearly configure the injected mocks to establish the specific preconditions for the scenario under test. For mocks whose behavior is not relevant to a particular test, default mock behavior is acceptable, but explicit setup for relevant interactions is preferred.

### 2. Test `MyAppLogic::on_quit()`
*   **Action:** Add a simple unit test for `MyAppLogic::on_quit()` to ensure it executes without panics. If `on_quit` evolves to perform state changes or cleanup via mocks, expand the test to verify these actions.

### 3. Systematically Test Error Handling Paths for Core Operations
The primary focus for new tests should be on `MyAppLogic`'s response to errors returned by its mocked dependencies.

*   **General Approach for Error Testing:**
    *   For each relevant event handler or internal logic path that interacts with a mocked dependency:
        1.  Configure the relevant mock (e.g., `MockProfileManager.load_profile`, `MockFileSystemScanner.scan_directory`, `MockArchiver.create_archive_content`, etc.) to return an `Err` variant.
        2.  **Assert:**
            *   `MyAppLogic` enters a sensible internal state (e.g., clears pending actions, reverts to a default state if applicable, sets an error status).
            *   Appropriate `PlatformCommand`s are generated to inform the user of the error (e.g., updating a status bar message, showing an error dialog placeholder), aligning with `TechErrorHandlingGracefulV1`.
            *   Subsequent operations are (or are not) attempted based on the error, as expected.

*   **Specific Error Scenarios to Implement:**
    *   **`on_main_window_created` (Initialization):**
        *   Test scenario: `MockConfigManager.load_last_profile_name` returns `Err(ConfigError::Io(...))` or `Err(ConfigError::NoProjectDirectory)`.
            *   *Assert:* No call to `MockProfileManager.load_profile`. Fallback scan initiated. Correct `PlatformCommand`s for status/error.
        *   Test scenario: `MockConfigManager.load_last_profile_name` succeeds, but `MockProfileManager.load_profile` (for the last used profile) returns `Err(ProfileError::ProfileNotFound)` or `Err(ProfileError::Io)`.
            *   *Assert:* Fallback scan initiated. Correct `PlatformCommand`s for status/error.
        *   Test scenario: Profile loading (last used or fallback to default path) succeeds, but the subsequent `MockFileSystemScanner.scan_directory` returns `Err(FileSystemError::Io(...))` or `Err(FileSystemError::InvalidPath(...))`.
            *   *Assert:* `MyAppLogic.file_nodes_cache` reflects the error (e.g., empty or error node). UI population command reflects this. Correct `PlatformCommand`s for status/error.

    *   **`AppEvent::FileOpenDialogCompleted` (Profile Load via Dialog):**
        *   Test scenario: User selects a file, but `MockProfileManager.load_profile_from_path` returns `Err(ProfileError::Io)` or `Err(ProfileError::Serde)`.
            *   *Assert:* `current_profile_name`/`cache` remain unchanged or are cleared. No call to `MockConfigManager.save_last_profile_name`. No scan attempted for the failed profile. Error notification `PlatformCommand`s.
        *   Test scenario: `MockProfileManager.load_profile_from_path` succeeds, but the subsequent `MockFileSystemScanner.scan_directory` for the new profile's root fails.
            *   *Assert:* `current_profile_name`/`cache` are set. `MockConfigManager.save_last_profile_name` *was* called. `file_nodes_cache` reflects scan error. Error notification `PlatformCommand`s.

    *   **`AppEvent::FileSaveDialogCompleted` (Handler for `PendingAction::SavingArchive`):**
        *   Test scenario: `MockArchiver.save_archive_content` (called when `profile.archive_path` was already set or just provided by dialog) returns `Err(io::Error)`.
            *   *Assert:* `pending_action` and `pending_archive_content` cleared. `current_profile_cache.archive_path` is *not* updated (if it was `None` and dialog provided path). `MockProfileManager.save_profile` (to persist an updated `archive_path`) is *not* called. `current_archive_status` does not become `UpToDate`. Error `PlatformCommand`s.
        *   Test scenario: `MockArchiver.save_archive_content` succeeds, but the subsequent `MockProfileManager.save_profile` (to save the profile with the newly set/confirmed `archive_path`) returns `Err(ProfileError::Io)`.
            *   *Assert:* `pending_action` and `pending_archive_content` cleared. In-memory `current_profile_cache.archive_path` *is* set. Error `PlatformCommand` for profile save failure. `MockArchiver.check_archive_status` still called.

    *   **`AppEvent::FileSaveDialogCompleted` (Handler for `PendingAction::SavingProfile`):**
        *   Test scenario: `profile_save_path.file_stem()` (from dialog result) returns `None` (e.g., path is `.` or `..`, or has no filename part).
            *   *Assert:* Graceful handling. No profile saved via `MockProfileManager`. `pending_action` cleared. Error `PlatformCommand`s.
        *   Test scenario: `MockProfileManager.save_profile` returns `Err(ProfileError::Io)` or `Err(ProfileError::InvalidProfileName)`.
            *   *Assert:* `current_profile_name`/`cache` are not updated. No call to `MockConfigManager.save_last_profile_name`. `pending_action` cleared. Error `PlatformCommand`s.

    *   **`ID_BUTTON_GENERATE_ARCHIVE_LOGIC` (Generate Archive Button Click):**
        *   Test scenario: `MockArchiver.create_archive_content` returns `Err(io::Error)`.
            *   *Assert:* `pending_archive_content` and `pending_action` are not set. No `ShowSaveFileDialog` command generated. Error `PlatformCommand` sent to user.

### 4. Test Edge Cases for Event Handlers
*   **`AppEvent::TreeViewItemToggled`:**
    *   Test scenario: The `item_id` received from the platform event does not correspond to any known path in `MyAppLogic.path_to_tree_item_id` map.
        *   *Assert:* Graceful handling (e.g., error logged internally, no panic). No calls to `MockStateManager` or `MockArchiver`. Possibly an error `PlatformCommand` if user feedback is desired for such an inconsistency.

### 5. Expand Coverage for `update_current_archive_status()`
*   **Action:** While many event handlers already trigger `update_current_archive_status()`, systematically verify that tests cover scenarios where `MockArchiver.check_archive_status` is configured to return *each* variant of the `ArchiveStatus` enum (`UpToDate`, `OutdatedRequiresUpdate`, `NotYetGenerated`, `ArchiveFileMissing`, `NoFilesSelected`, `ErrorChecking`).
*   **Assert:**
    *   The value of `MyAppLogic.current_archive_status` is correctly updated.
    *   Relevant `PlatformCommand`s for status bar updates are generated (matching the UI requirements like `UiNotificationOutdatedArchiveV1`).

### 6. Adopt Best Practices for All New Tests
*   **Structure:** Follow the "Arrange, Act, Assert" (AAA) pattern for clarity.
*   **Naming:** Use descriptive test names that indicate the scenario and expected behavior (e.g., `test_event_X_when_condition_Y_should_produce_Z`).
*   **Test Data:** Leverage and expand test data builders/helper functions (e.g., for `FileNode` trees, `Profile` objects) to keep individual test setup sections concise, readable, and to promote reuse.
*   **Assertions:**
    *   Verify the exact `Vec<PlatformCommand>` returned by `handle_event` or `on_main_window_created` (order and content).
    *   Assert the state of relevant `pub(crate)` fields in `MyAppLogic` (e.g., `current_profile_name`, `current_profile_cache` attributes, `file_nodes_cache` state after mock interactions, `current_archive_status`, and crucially, `pending_action` to ensure it's correctly set/cleared).
    *   Confirm interactions with all relevant mock objects (e.g., `mock_profile_manager.save_profile_called_with(...)`, `mock_state_manager.apply_profile_to_tree_called_with(...)`, `mock_archiver.check_archive_status_called_with(...)`).

By systematically implementing these steps, the unit tests for `MyAppLogic` will become highly effective at verifying its correctness, improving maintainability, and providing confidence during future development and refactoring.
