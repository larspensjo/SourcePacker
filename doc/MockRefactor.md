Okay, let's outline the plan for refactoring `MyAppLogic` to use Dependency Injection for its core service interactions, enabling robust unit testing with mocks.

Phase 1-4 are complete, and has been removed from this document.

## Phase 5: Future Tasks & Considerations:

1.  **Refactor `FileOpenDialogCompleted` for Profile Loading:**
    *   The `AppEvent::FileOpenDialogCompleted` handler in `MyAppLogic` still directly opens and deserializes profile files (`File::open`, `serde_json::from_reader`).
    *   To fully align with the DI pattern for profile loading, consider adding a method like `load_profile_from_path(&self, path: &Path) -> Result<Profile, ProfileError>` to the `ProfileManagerOperations` trait.
    *   `MyAppLogic` would then call this trait method. `CoreProfileManager` would implement the actual file I/O and deserialization for this new method.
    *   `MockProfileManager` would be updated to mock this new method, allowing tests to directly return a `Profile` or an error for a given path without needing `create_temp_profile_file_for_direct_load`.

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
