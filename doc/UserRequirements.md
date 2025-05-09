# Requirement Specification for SourcePacker

This document outlines the requirements for SourcePacker, a file selection and archive tool designed to package source code for AI prompts.

# Core Functionality

## File System Traversal
[FileSystemScanRootDirTreeViewV1] The application must be able to scan a user-specified root directory and display its file and folder structure in a tree view.

## File Selection
[FileSelStateSelectedV1] *   **Selected:** The item is explicitly included in the archive.
[FileSelStateDeselectedV1] *   **Deselected:** The item is explicitly excluded from the archive.
[FileSelStateUnknownV1] *   **Unknown:** The item's inclusion is not yet determined (initial state).
[FileSelFolderRecursiveStateV1] * Selecting or deselecting a folder shall recursively apply the same state to all its child files and folders.

## File Filtering (Whitelist)
[FileFilterProfileWhitelistGlobV1] * Profiles shall define a list of whitelist glob patterns (e.g., `*.rs`, `src/**/*.toml`).
[FileFilterWhitelistOnlyMatchesV1] * Only files matching at least one whitelist pattern shall be considered for display and selection in the tree view. Files not matching any pattern will not be shown or processed.

## Text File Focus
[TextFileFocusUTF8V1] The application is intended for text-based source code. It should primarily handle files assumed to be UTF-8 encoded.
[TextFileBinaryFutureWarnV1] * (Future Consideration) A simple mechanism to identify and potentially warn about or exclude binary files might be useful, though initially, all files matching whitelist patterns will be treated as text.

## Archive Generation
[ArchiveGenSingleTxtFileV1] * The application shall create a single `.txt` archive file from all "Selected" files.
[ArchiveGenConcatenateContentV1] * The content of selected files shall be concatenated into the archive.
[ArchiveGenFileHeaderFooterV1] * Each file's content in the archive shall be preceded by a simple header (e.g., `--- START FILE: path/to/file.rs ---`) and followed by a simple footer (e.g., `--- END FILE: path/to/file.rs ---`).

## Token Count Estimation
[TokenCountEstimateSelectedV1] * The application shall display an estimated token count for the currently selected files.
[TokenCountLiveUpdateV1] * This count shall update live as files are selected or deselected. (Mechanism for tokenization TBD, e.g., word count or a specific tokenizer).

# Profile Management

## Profile Definition
A profile encapsulates:
[ProfileDefRootFolderV1] * A root folder path.
[ProfileDefSelectionStateV1] * The selection state (Selected/Deselected) of files and folders within that root folder. (Unknown state is not saved; new files in a known folder appear as Unknown).
[ProfileDefWhitelistPatternsV1] * A list of whitelist file patterns.

## Profile Storage
[ProfileStoreJsonFilesV1] * Profiles shall be saved as individual JSON files.
[ProfileStoreAppdataLocationV1] * Profiles shall be stored in `%APPDATA%\SourcePacker\profiles\`.

## Profile Operations
[ProfileOpLoadSwitchV1] * **Load:** Users can switch between different profiles.
[ProfileOpSaveNewOverwriteV1] * **Save:** Users can save the current selection state and whitelist patterns as a new profile or overwrite an existing one.
[ProfileOpCreateNewV1] * **Create New:** Users can create a new, empty profile, specifying a root folder and initial whitelist patterns.
[ProfileOpDuplicateExistingV1] * **Duplicate:** Users can duplicate an existing profile to create a new one based on it.
[ProfileOpDeleteExistingV1] * **Delete:** Users can delete existing profiles.

## Default Profile
[ProfileDefaultLoadRecentV1] * On application start, the most recently used profile shall be loaded by default.
[ProfileDefaultNoPreviousBlankV1] * If no previous profile exists, the application may start with a blank state or prompt the user to create/open a profile.

## Handling Missing Files
[ProfileMissingFileIndicateOrRemoveV1] * When loading a profile, if a previously selected file or folder no longer exists, it should be indicated in the UI (e.g., greyed out, marked with an icon) or silently removed from the selection. The profile itself should persist the path.

# User Interface (Windows Native UI via `windows-rs`)

## Main Window
[UiMainWindowSingleV1] A single main application window.

## Tree View
[UiTreeViewDisplayStructureV1] * Display the file and folder structure starting from the profile's root folder.
[UiTreeViewVisualSelectionStateV1] * Visually indicate the selection state (Selected, Deselected, Unknown) for each item. (Suggestion: Tristate checkboxes).

## File Content Viewer
[UiContentViewerPanelReadOnlyV1] * A panel or separate window to display the content of a selected file from the tree view (read-only).

## Search Functionality
[UiSearchFileNameFilterTreeV1] * **File Name Search:** Allow users to filter the tree view by file/folder names.
[UiSearchFileContentHighlightV1] * **Content Search:** Allow users to search for text strings within the files currently displayed in the tree (or within selected files) and highlight/filter matching files.

## Status Bar
Display relevant information such as:
[UiStatusBarProfileNameV1] * Current profile name.
[UiStatusBarSelectedFileCountV1] * Number of selected files.
[UiStatusBarSelectedFileSizeV1] * Total size of selected files (optional).
[UiStatusBarLiveTokenCountV1] * Live estimated token count.

## Menu/Toolbar
Provide access to functions like:
[UiMenuProfileManagementV1] * Profile management (Open, Save, Save As, New, Duplicate, Delete, Manage Profiles).
[UiMenuSetRootFolderV1] * Set Root Folder (for new profiles).
[UiMenuEditWhitelistV1] * Edit Whitelist Patterns.
[UiMenuGenerateArchiveV1] * Generate Archive.
[UiMenuRefreshTreeViewV1] * Refresh tree view.

# Technical Requirements

## Development Language
[TechLangRustLatestV1] Rust (latest stable version).

## UI Framework
[TechUiFrameworkWindowsRsV1] `windows-rs` for direct Win32/WinRT API interaction.

## Modularity & Testing
[TechModularityLogicalModulesV1] * The codebase shall be organized into logical modules (e.g., UI, core logic, profile management, file operations).
[TechModularityUnitTestsCoreV1] * Core logic components shall have unit tests where feasible.

## Error Handling
[TechErrorHandlingGracefulV1] Graceful error handling for file operations, profile loading/saving, etc.

## Performance
[TechPerfResponsiveUiV1] For typical source code repositories, UI should remain responsive. Asynchronous operations are not an initial requirement but can be considered for future optimization if large directories cause UI lag.

## Dependencies
[TechDepsMinimizeExternalV1] Minimize external dependencies, using well-maintained and popular crates where necessary.

# Future Considerations (Optional/Post-MVP)

[FutureGitignoreIntegrationV1] * Integration with `.gitignore` or similar ignore files.
[FutureBinaryFileDetectionSophisticatedV1] * More sophisticated binary file detection.
[FutureArchiveHeaderFormatConfigurableV1] * Configurable archive header/footer format.
[FutureClipboardCopyArchiveV1] * "Copy to Clipboard" option for the generated archive content.
[FutureEncodingSupportOtherV1] * Support for other character encodings (if UTF-8 proves insufficient).
