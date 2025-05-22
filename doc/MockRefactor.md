Okay, let's outline the plan for refactoring `MyAppLogic` to use Dependency Injection for its core service interactions, enabling robust unit testing with mocks.

This refactoring is needed to:

*   **Improve Testability:** Allow `MyAppLogic` to be unit-tested in isolation from the file system and other external dependencies. Mocks can simulate various scenarios (e.g., successful operations, errors, specific data returns) without actual I/O.
*   **Enhance Modularity:** Clearly define the contracts (interfaces/traits) between `MyAppLogic` and the `core` services. This makes the system easier to understand, maintain, and extend.
*   **Increase Flexibility:** Make it easier to swap out implementations of core services in the future if needed (e.g., a different profile storage mechanism).

**Goal:** Refactor `MyAppLogic` so that its dependencies on `core` module functionalities (config management, profile management, file system scanning) are expressed through traits, allowing real implementations to be used in production and mock implementations in tests.

Here's the step-by-step refactoring plan:

---

# Refactoring Plan: Dependency Injection for `MyAppLogic` Core Services

This plan details the steps to refactor `MyAppLogic` to use traits for its interactions with core functionalities, facilitating the use of mock objects in unit tests.

## Phase 1: Configuration Management (`core::config`). This phase is largely complete.

**Rationale:** The first dependency to tackle is the loading and saving of the "last used profile name." This is a relatively simple interaction and serves as a good starting point for the DI pattern.

**Steps:**

1.  **Define `ConfigManagerOperations` Trait (in `core/config.rs`):**
    *   Declare a public trait (e.g., `ConfigManagerOperations`) with methods like:
        *   `load_last_profile_name(&self, app_name: &str) -> Result<Option<String>, ConfigError>`
        *   `save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<(), ConfigError>`
    *   Ensure the trait is `Send + Sync` if `MyAppLogic` needs to be thread-safe (which it currently does via `Arc<Mutex<>>`).

2.  **Create `CoreConfigManager` Struct (in `core/config.rs`):**
    *   Create a struct (e.g., `CoreConfigManager`).
    *   Implement the `ConfigManagerOperations` trait for this struct.
    *   Move the existing logic from the free functions `load_last_profile_name` and `save_last_profile_name` (and their helper `get_app_config_dir`) into the methods of `CoreConfigManager`.

3.  **Update `core/mod.rs`:**
    *   Re-export `ConfigManagerOperations` (the trait) and `CoreConfigManager` (the concrete struct).
    *   Stop re-exporting the old free functions for config management if they are no longer intended for direct external use.

4.  **Refactor `MyAppLogic` (in `app_logic/handler.rs`):**
    *   Add a new field: `config_manager: Arc<dyn ConfigManagerOperations>`.
    *   Update `MyAppLogic::new()` to accept an `Arc<dyn ConfigManagerOperations>` as a parameter and store it in the new field.
    *   Modify all internal calls that previously used `core::load_last_profile_name` and `core::save_last_profile_name` to now call `self.config_manager.load_last_profile_name(...)` and `self.config_manager.save_last_profile_name(...)` respectively.
        *   This affects `on_main_window_created`.
        *   This affects `AppEvent::FileOpenDialogCompleted` (for saving after successful load).
        *   This affects `AppEvent::FileSaveDialogCompleted` (for saving after successful profile save).

5.  **Update `main.rs`:**
    *   When creating the `MyAppLogic` instance, instantiate `Arc::new(CoreConfigManager::new())` and pass it to `MyAppLogic::new()`.

6.  **Create `MockConfigManager` for Tests (in `app_logic/handler_tests.rs`):**
    *   Define a new struct `MockConfigManager`.
    *   Implement the `ConfigManagerOperations` trait for `MockConfigManager`.
    *   The mock methods should allow tests to:
        *   Specify the `Result<Option<String>, ConfigError>` to be returned by `load_last_profile_name`.
        *   Record the `app_name` and `profile_name` passed to `save_last_profile_name` for assertion.
    *   Use `std::sync::Mutex` internally within the mock if state needs to be shared or modified by the mock's methods (e.g., to store what was "saved" or the preset return value).

7.  **Update `MyAppLogic` Unit Tests (in `app_logic/handler_tests.rs`):**
    *   Modify test setup functions (e.g., `setup_logic_with_mocks`) to instantiate `MyAppLogic` with an `Arc::new(MockConfigManager::new())`.
    *   Adjust tests that involve loading/saving the last profile name to:
        *   Configure the `MockConfigManager` instance before calling the `MyAppLogic` method under test (e.g., set the expected return value for `load_last_profile_name`).
        *   Assert that the mock's `save_last_profile_name` method was called with the correct parameters, if applicable.
    *   Remove any direct file system setup/teardown related to `last_profile_name.txt` from these unit tests, as the mock now handles this interaction.

## Phase 2: Profile Management (`core::profiles`)

**Rationale:** Next, decouple the operations related to loading, saving, and listing profiles. These involve more complex data (the `Profile` struct) and file I/O.

**Steps:**

1.  **Define `ProfileManagerOperations` Trait (in `core/profiles.rs`):**
    *   Declare a public trait (e.g., `ProfileManagerOperations`) with methods like:
        *   `load_profile(&self, profile_name: &str, app_name: &str) -> Result<Profile, ProfileError>`
        *   `save_profile(&self, profile: &Profile, app_name: &str) -> Result<(), ProfileError>`
        *   `list_profiles(&self, app_name: &str) -> Result<Vec<String>, ProfileError>`
        *   `get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf>` (renamed from `get_profile_dir` to avoid confusion and clarify it returns a path, not an errorable result directly for this specific method, though `Option` serves a similar purpose).
    *   Ensure `Send + Sync`.

2.  **Create `CoreProfileManager` Struct (in `core/profiles.rs`):**
    *   Create `CoreProfileManager` and implement `ProfileManagerOperations` for it.
    *   Move logic from existing free functions (`load_profile`, `save_profile`, `list_profiles`, `get_profile_dir`) into this struct's methods.

3.  **Update `core/mod.rs`:**
    *   Re-export `ProfileManagerOperations` and `CoreProfileManager`.
    *   Stop re-exporting old free functions for profile management.

4.  **Refactor `MyAppLogic` (in `app_logic/handler.rs`):**
    *   Add field: `profile_manager: Arc<dyn ProfileManagerOperations>`.
    *   Update `MyAppLogic::new()` to accept and store this dependency.
    *   Change calls from `core::load_profile`, `core::save_profile`, `core::profiles::get_profile_dir` to use `self.profile_manager`.
        *   `on_main_window_created` (for loading profile after getting name from config manager).
        *   `AppEvent::MenuLoadProfileClicked` (for `get_profile_dir_path`).
        *   `AppEvent::FileOpenDialogCompleted` (direct file opening should be replaced by `self.profile_manager.load_profile_from_path` if such a method is added to the trait, or adjust how `ShowOpenFileDialog` result is handled to give a name to `load_profile`). For now, loading from the `FileOpenDialogCompleted` still involves direct `File::open` and `serde_json::from_reader`. This part might need a more specific method on the `ProfileManagerOperations` like `load_profile_from_path(path: &PathBuf)` or the dialog needs to return a name that `load_profile` can use. *Self-correction: `FileOpenDialogCompleted` already gives a path to a specific JSON file. The `ProfileManagerOperations` trait could have a `load_profile_from_path(&self, path: &PathBuf) -> Result<Profile, ProfileError>` method. Alternatively, the current design where `MyAppLogic` handles the direct file open and deserialization for `FileOpenDialogCompleted` can remain, but `core::save_profile` should still go through the trait.*
        *   `AppEvent::MenuSaveProfileAsClicked` (for `get_profile_dir_path`).
        *   `AppEvent::FileSaveDialogCompleted` (for `save_profile` when saving a profile or updating it after archive path change).

5.  **Update `main.rs`:**
    *   Inject `Arc::new(CoreProfileManager::new())` into `MyAppLogic`.

6.  **Create `MockProfileManager` for Tests (in `app_logic/handler_tests.rs`):**
    *   Implement `ProfileManagerOperations`.
    *   Allow tests to set mock return values for `load_profile`, `list_profiles`.
    *   Allow tests to inspect what was passed to `save_profile`.
    *   Mock `get_profile_dir_path` to return a controlled path.

7.  **Update `MyAppLogic` Unit Tests:**
    *   Use `MockProfileManager`.
    *   Configure mock behavior for profile operations.
    *   Verify interactions with the mock.
    *   Remove direct file system setup/teardown for profile JSON files.

## Phase 3: File System Scanning (`core::file_system`)

**Rationale:** Decouple the directory scanning logic. This is crucial as `scan_directory` performs significant file system traversal.

**Steps:**

1.  **Define `FileSystemScannerOperations` Trait (in `core/file_system.rs`):**
    *   Declare trait with method:
        *   `scan_directory(&self, root_path: &Path) -> Result<Vec<FileNode>, FileSystemError>`
    *   Ensure `Send + Sync`.

2.  **Create `CoreFileSystemScanner` Struct (in `core/file_system.rs`):**
    *   Implement `FileSystemScannerOperations`.
    *   Move logic from existing `scan_directory` free function.

3.  **Update `core/mod.rs`:**
    *   Re-export `FileSystemScannerOperations` and `CoreFileSystemScanner`.
    *   Stop re-exporting the `scan_directory` free function.

4.  **Refactor `MyAppLogic` (in `app_logic/handler.rs`):**
    *   Add field: `file_system_scanner: Arc<dyn FileSystemScannerOperations>`.
    *   Update `MyAppLogic::new()` to accept and store it.
    *   Change calls from `core::scan_directory` to `self.file_system_scanner.scan_directory(...)`.
        *   `on_main_window_created`.
        *   `AppEvent::FileOpenDialogCompleted` (after loading a profile, a rescan is triggered).
        *   Any "Refresh" action (e.g., P2.9).

5.  **Update `main.rs`:**
    *   Inject `Arc::new(CoreFileSystemScanner::new())` into `MyAppLogic`.

6.  **Create `MockFileSystemScanner` for Tests (in `app_logic/handler_tests.rs`):**
    *   Implement `FileSystemScannerOperations`.
    *   Allow tests to set mock `Result<Vec<FileNode>, FileSystemError>` to be returned by `scan_directory` for a given path.
    *   Allow tests to verify which `root_path` was passed to `scan_directory`.

7.  **Update `MyAppLogic` Unit Tests:**
    *   Use `MockFileSystemScanner`.
    *   Configure the mock to return specific `FileNode` trees or errors.
    *   Verify interactions.
    *   Remove any test dependency on actual directory structures for scanning logic within `MyAppLogic` tests.

## Phase 4: Other Core Interactions (as needed)

**Rationale:** Review `MyAppLogic` for any other direct calls to `core` module free functions that involve side effects or complex logic (e.g., `core::archiver::create_archive_content`, `core::archiver::check_archive_status`, `core::state_manager` functions if they were to become more complex or have side effects).

**General Steps for each remaining interaction:**

1.  Define a new trait in the relevant `core` submodule (e.g., `ArchiverOperations`, `StateManagerOperations`).
2.  Create a `Core...` struct implementing the trait.
3.  Update `core/mod.rs` to export the new trait and struct.
4.  Add the new trait object as a dependency to `MyAppLogic` and update `MyAppLogic::new()`.
5.  Update `main.rs` to inject the concrete `Core...` implementation.
6.  Create a `Mock...` struct for tests.
7.  Update `MyAppLogic` unit tests to use the mock.

**Specific Considerations for `FileOpenDialogCompleted` in `MyAppLogic`:**
The current logic in `FileOpenDialogCompleted` directly opens and deserializes the profile JSON file:
```rust
// Inside FileOpenDialogCompleted:
match File::open(&profile_file_path) {
    Ok(file) => {
        let reader = std::io::BufReader::new(file);
        match serde_json::from_reader(reader) {
            Ok(loaded_profile) => { /* ... */ }
            // ...
        }
    }
    // ...
}
```
This direct file operation will remain until `ProfileManagerOperations` is fully integrated. When `ProfileManagerOperations` is introduced, you could add a method like `load_profile_from_path(&self, path: &Path) -> Result<Profile, ProfileError>` to the trait. `MyAppLogic` would then call this method, and `CoreProfileManager` would implement the file opening and deserialization. `MockProfileManager` could then directly return a `Profile` object or an error for a given path.

## Phase 5: Future Tasks & Considerations:

0.1.  **Cleanup Deprecated Free Functions:**
    - Now that all `core` module interactions in `MyAppLogic` are through traits, the `#[deprecated]` free functions in `core/archiver.rs`, `core/state_manager.rs`, `core/file_system.rs`, `core/profiles.rs`, and `core/config.rs` can be fully removed along with their re-exports in `core/mod.rs` if they are not used anywhere else. This will clean up the `core` API.

0.2  **Comprehensive Tests for `MyAppLogic` with `MockStateManager`:**
    *   Specifically, test how `MyAppLogic` behaves when `MockStateManager.apply_profile_to_tree` or `MockStateManager.update_folder_selection` are called, and verify the resulting `PlatformCommand`s and internal state of `MyAppLogic` (e.g., `current_archive_status` changes based on new selection states).
    *   Since the `MockStateManager` currently also performs the state modification, tests implicitly cover some of this. However, if the mock were to *only* record calls (without modifying the tree), tests would need to ensure `MyAppLogic` still behaves correctly based on the *assumption* that the state manager did its job.

0.3.  **Review `ImproveTestingHandle.md`:**
    - With all core dependencies now mockable, revisit `ImproveTestingHandle.md` to implement more comprehensive unit tests for `MyAppLogic`, especially focusing on error paths and complex interaction scenarios as outlined in "Phase 2: Implementing New, Comprehensive Unit Tests" of that document. For example:
        - Test `ID_BUTTON_GENERATE_ARCHIVE_LOGIC` when `mock_archiver.create_archive_content` returns an `Err`.
        - Test `FileSaveDialogCompleted` for `PendingAction::SavingArchive` when `mock_archiver.save_archive_content` returns an `Err`.
        - Test scenarios where `mock_archiver.check_archive_status` returns different `ArchiveStatus` values and verify `MyAppLogic`'s internal state and generated `PlatformCommand`s.

1.  **Refactor `FileOpenDialogCompleted` for Profile Loading:**
    *   The `AppEvent::FileOpenDialogCompleted` handler in `MyAppLogic` still directly opens and deserializes profile files (`File::open`, `serde_json::from_reader`).
    *   To fully align with the DI pattern for profile loading, consider adding a method like `load_profile_from_path(&self, path: &Path) -> Result<Profile, ProfileError>` to the `ProfileManagerOperations` trait.
    *   `MyAppLogic` would then call this trait method. `CoreProfileManager` would implement the actual file I/O and deserialization for this new method.
    *   `MockProfileManager` would be updated to mock this new method, allowing tests to directly return a `Profile` or an error for a given path without needing `create_temp_profile_file_for_direct_load`.

2.  **Refactor `core::archiver` and `core::state_manager`:**
    *   Review if `core::archiver` functions (like `create_archive_content`, `check_archive_status`) and `core::state_manager` functions (like `apply_profile_to_tree`, `update_folder_selection`) should also be refactored to use traits and dependency injection if `MyAppLogic`'s direct calls to them make testing complex or involve significant side effects (though currently they seem mostly pure or use already-mockable inputs like `FileNode` trees). This is Phase 4 of `MockRefactor.md`.

3.  **Comprehensive Error Handling Tests:**
    *   With mocks for all major `core` dependencies, expand tests in `handler_tests.rs` to cover various error scenarios:
        *   `MockFileSystemScanner` returns `Err(FileSystemError::Io(...))` or `Err(FileSystemError::InvalidPath(...))`.
        *   Test how `MyAppLogic` behaves and what `PlatformCommand`s (e.g., status updates, error dialogs) are generated in these error cases. (As outlined in `ImproveTestingHandle.md`).

4.  **Implement "Refresh" Action (P2.9):**
    *   The "Refresh" action would involve calling `self.file_system_scanner.scan_directory(...)`. Ensure this new functionality correctly uses the injected scanner.

5.  **Cleanup Deprecated Free Functions:**
    *   Once all direct calls to deprecated free functions (like `core::scan_directory`, `core::profiles::load_profile`, etc.) are removed from `MyAppLogic` and any other internal `core` usage (if applicable), remove their re-exports from `core/mod.rs` and potentially the functions themselves from their respective modules if they are no longer needed. The `#[deprecated]` attribute is a good first step.

6.  **Isolate `MyAppLogic` Tests Further:**
    *   Some tests, like `test_handle_treeview_item_toggled_updates_model_visuals_and_archive_status` and `test_profile_load_updates_archive_status_direct_load`, still perform some real file system operations (e.g., creating temporary files for `core::get_file_timestamp` or the archive file itself).
    *   If these become problematic or if purer unit tests are desired, the `core::archiver::get_file_timestamp` and `core::archiver::check_archive_status` functions might need to be abstracted via an `ArchiverOperations` trait as per Phase 4 of `MockRefactor.md`. This would allow mocking timestamp reads and archive status checks.
    *   Similarly, the direct `fs::write` in the `FileSaveDialogCompleted` handler for saving archive content could be moved into an `ArchiverOperations` method.

7.  **True Mock for `TestProfileManager::load_profile_from_path`:**
    *   The `TestProfileManager` in `core/profiles/profile_tests.rs` (which is different from `MockProfileManager` in `handler_tests.rs`) currently has a `load_profile_from_path` that still performs actual file I/O. For more isolated unit tests of `CoreProfileManager` itself (if desired through `TestProfileManager`), this mock could be enhanced to not rely on the file system, perhaps by pre-configuring expected path-to-profile mappings. However, the primary `MockProfileManager` used for `MyAppLogic` tests *is* now correctly mocking this behavior.

8.  **Abstract `core::archiver`:**
    *   Functions like `core::create_archive_content` and `core::check_archive_status` are still called directly by `MyAppLogic`.
    *   Introduce an `ArchiverOperations` trait (and `CoreArchiver`, `MockArchiver`) to decouple these. This would allow:
        *   Mocking archive content generation (e.g., to simulate errors or specific content).
        *   Mocking `check_archive_status` to return specific `ArchiveStatus` values without needing real files or timestamps, further isolating `MyAppLogic` tests (like `test_profile_load_updates_archive_status_via_manager` which currently relies on the real `check_archive_status` behavior for a missing archive file).
        *   Moving the `fs::write` call for saving archive content (currently in `MyAppLogic::handle_event` for `FileSaveDialogCompleted` with `PendingAction::SavingArchive`) into this new `ArchiverOperations` trait (e.g., `save_archive_content(&self, path: &Path, content: &str)`).
