Okay, let's outline the plan for refactoring `MyAppLogic` to use Dependency Injection for its core service interactions, enabling robust unit testing with mocks.

Phase 1-4 are complete, and has been removed from this document.

## Phase 5: Future Tasks & Considerations:

Completed steps have been removed.

3.  **Comprehensive Error Handling Tests:**
    *   With mocks for all major `core` dependencies, expand tests in `handler_tests.rs` to cover various error scenarios:
        *   `MockFileSystemScanner` returns `Err(FileSystemError::Io(...))` or `Err(FileSystemError::InvalidPath(...))`.
        *   Test how `MyAppLogic` behaves and what `PlatformCommand`s (e.g., status updates, error dialogs) are generated in these error cases. (As outlined in `ImproveTestingHandle.md`).

4.  **Implement "Refresh" Action (P2.9):**
    *   The "Refresh" action would involve calling `self.file_system_scanner.scan_directory(...)`. Ensure this new functionality correctly uses the injected scanner.

7.  **True Mock for `TestProfileManager::load_profile_from_path`:**
    *   The `TestProfileManager` in `core/profiles/profile_tests.rs` (which is different from `MockProfileManager` in `handler_tests.rs`) currently has a `load_profile_from_path` that still performs actual file I/O. For more isolated unit tests of `CoreProfileManager` itself (if desired through `TestProfileManager`), this mock could be enhanced to not rely on the file system, perhaps by pre-configuring expected path-to-profile mappings. However, the primary `MockProfileManager` used for `MyAppLogic` tests *is* now correctly mocking this behavior.
