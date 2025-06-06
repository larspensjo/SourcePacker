# Requirement Specification for SourcePacker

This document outlines the requirements for SourcePacker, a file selection and archive tool designed to package source code for AI prompts. The tool actively monitors source code hierarchies, helps manage subsets of files through profiles, and ensures archives reflect the latest selected changes.

# Core Functionality

## File System Monitoring and Display
[FileSystemMonitorTreeViewV1] The application must be able to scan a user-specified root directory (defined by the active profile), detect file/folder additions, removals, and modifications (primarily via a manual "Refresh" action initially), and display its relevant file and folder structure in a tree view.
**[FileStateNewDetectedV2] New files detected within the monitored directory (e.g., after a "Refresh," or when no profile is loaded) that are not already part of the current profile's explicit selection state shall initially be presented in a distinct "New" state, requiring user classification.**

## File Selection
The application shall support three distinct states for files and folders within the tree view regarding their inclusion in an archive:
[FileSelStateSelectedV2] *   **Selected:** The item is explicitly included in the profile's archive. This state must be clearly visually indicated (e.g., a checked checkbox).
[FileSelStateDeselectedV2] *   **Deselected:** The item is explicitly excluded from the profile's archive. This state must be clearly visually indicated (e.g., an unchecked checkbox).
**[FileSelStateNewV1] *   **New:** The item's inclusion in the profile's archive is not yet determined. This applies to files newly detected on disk that are not part of an active profile's saved selections, or all files when no profile is loaded. This state must have its own distinct visual indicator, separate from "Selected" or "Deselected" (e.g., a special icon, text styling, or overlay).**
[FileSelFolderRecursiveStateV2] * Selecting or deselecting a folder shall recursively apply the same state (Selected or Deselected) to all its child files and folders within the current view. Items previously in a "New" state will transition to "Selected" or "Deselected" accordingly.
**[FileSelTransitionFromNewV1] * When a user explicitly interacts with an item in the "New" state to select or deselect it, the item shall transition to the "Selected" or "Deselected" state respectively, and its "New" state indicator shall be removed.**

## Text File Focus
[TextFileFocusUTF8V1] The application is intended for text-based source code. It should primarily handle files assumed to be UTF-8 encoded.

## Archive Generation
[ArchiveGenSingleTxtFileV1] * The application shall create a single `.txt` archive file from all files marked "Selected" *for the currently active profile*.
[ArchiveGenConcatenateContentV1] * The content of selected files shall be concatenated into the archive.
[ArchiveGenFileHeaderFooterV1] * Each file's content in the archive shall be preceded by a simple header (e.g., `--- START FILE: "path/to/file.rs" ---`) and followed by a simple footer (e.g., `--- END FILE: "path/to/file.rs" ---`).

## Archive Synchronization and Integrity
[ArchiveSyncTimestampV1] The application must compare the last modification timestamp of a profile's associated archive file with the last modification timestamps of its "Selected" source files.
[ArchiveSyncNotifyUserV1] If any "Selected" source files for the current profile are newer than its associated archive, or if the set of "Selected" files has changed since the last archive generation, the user shall be clearly notified that the archive is outdated and requires regeneration.
[ArchiveSyncUserAcknowledgeV1] Users must be able to acknowledge the need for an archive update, typically by triggering the "Generate Archive" action.

## Token Count Estimation
[TokenCountEstimateSelectedV1] * The application shall display an estimated token count for the files currently marked "Selected" in the active profile.
[TokenCountLiveUpdateV1] * This count shall update live as files are selected or deselected. (Mechanism for tokenization TBD, e.g., word count or a specific tokenizer).

# Profile Management

## Profile Definition
A profile encapsulates:
[ProfileDefRootFolderV2] * A root folder path to be monitored. This is set during profile creation or loaded from an existing profile.
**[ProfileDefSelectionStateV3] * The selection state (Selected/Deselected) of files and folders within that root folder for that specific profile. "New" state items are not explicitly persisted as "New" in the profile; upon next load, they would re-evaluate to "New" if not explicitly selected/deselected in the saved profile.**
[ProfileDefAssociatedArchiveV2] * Each profile shall be associated with its own specific output archive file. The path/name of this archive is set when the user first saves an archive for the profile and is then persisted with the profile.

## Profile Storage
[ProfileStoreJsonFilesV1] * Profiles shall be saved as individual JSON files.
[ProfileStoreAppdataLocationV2] * Profiles shall be stored in a local application-specific directory (e.g., `%LOCALAPPDATA%\SourcePacker\profiles\` on Windows).

## Profile Operations
[ProfileOpLoadSwitchV2] * **Load/Switch:** Users can switch between different profiles (e.g., via a "Switch Profile..." menu or initial selection dialog). Loading a profile will apply its settings (root folder, persisted selections, archive path) to the view and scan its root folder.
[ProfileOpSaveNewOverwriteV4] * **Save As:** Users can save the current root folder, selection state (derived from the live UI), and associated archive configuration as a new profile or overwrite an existing profile file (by choosing an existing name in the save dialog).
[ProfileOpCreateNewV4] * **Create New:** Users can create a new profile. This involves:
    1.  Prompting for a profile name.
    2.  Prompting for a root folder to associate with the new profile.
    3.  The new profile starts with no files explicitly selected or deselected (all files in the root folder will initially appear as "New") and no associated archive file path.
[ProfileOpDuplicateExistingV1] * **Duplicate:** Users can duplicate an existing profile to create a new one based on it.
[ProfileOpDeleteExistingV1] * **Delete:** Users can delete existing profiles.

## Startup and Profile State
[ProfileDefaultLoadRecentV2] * On application start, the most recently used profile (name stored in application configuration) shall be loaded by default.
[ProfileDefaultNoPreviousBlankV2] * If no previous profile exists or the last used profile cannot be loaded, the application will guide the user to select an existing profile or create a new one before the main UI is fully shown. The main window remains hidden or minimally functional until a profile is active.
[ProfileSaveOnExplicitActionV2] * The selection state of files within a profile is persisted to its file when the user explicitly saves the profile (e.g., "Save Profile As") or when the associated archive path is set/updated (which also triggers a profile save). There is no automatic save of selection changes on application exit without an explicit save action during the session.
[ProfileConfigSaveOnExitV2] * The name of the currently active profile is saved to the application's configuration on exit, so it can be loaded by [ProfileDefaultLoadRecentV2].

## Handling Missing Files
[ProfileMissingFileIndicateOrRemoveV1] * When loading a profile, if a previously selected/deselected file or folder no longer exists in the monitored directory, it should be indicated in the UI (e.g., greyed out, marked with an icon) or silently removed from the profile's selection set. The profile itself should persist the path until explicitly removed by the user.

# User Interface (Windows Native UI via `windows-rs`)

## Main Window
[UiMainWindowSingleV1] A single main application window.

## Tree View
[UiTreeViewDisplayStructureV2] * Display the file and folder structure starting from the active profile's root folder.
**[UiTreeViewVisualSelectionStateV2] * Visually indicate the selection state (Selected, Deselected, New) for each item. "Selected" and "Deselected" will typically use checkbox states. "New" items will have an additional distinct visual indicator.**
[UiTreeViewVisualFileStatusV1] * Visually indicate the status of files relative to the profile's selection and archive state (e.g., new, modified since last archive, included, excluded).

## File Content Viewer
[UiContentViewerPanelReadOnlyV1] * A panel or separate window to display the content of a selected file from the tree view (read-only).

## Search Functionality
[UiSearchFileNameFilterTreeV1] * **File Name Search:** Allow users to filter the tree view by file/folder names.
[UiSearchFileContentHighlightV1] * **Content Search:** Allow users to search for text strings within the files currently displayed in the tree (or within selected files) and highlight/filter matching files.

## Status Bar
Display relevant information such as:
[UiStatusBarProfileNameV2] * Current active profile name (could be part of window title or status bar).
[UiStatusBarSelectedFileCountV1] * Number of selected files for the current profile.
[UiStatusBarSelectedFileSizeV1] * Total size of selected files (optional).
[UiStatusBarLiveTokenCountV1] * Live estimated token count for selected files.
[UiNotificationOutdatedArchiveV2] * A clear visual indicator (e.g., text, icon) when the current profile's archive is outdated or its status (e.g., "Archive: Up-to-date", "Archive: Needs Update").

## Menu/Toolbar
Provide access to functions like:
[UiMenuProfileManagementV2] * Profile management (Switch Profile..., Save Profile As..., New Profile flow initiated from startup dialog).
[UiMenuSetRootFolderV2] * (Future/Part of Edit Profile) Set/Change Root Folder for existing profiles; for new profiles, this is part of the creation flow.
[UiMenuGenerateArchiveV1] * Generate/Update Archive for the current profile.
[UiMenuTriggerScanV1] * Manually trigger a re-scan/re-evaluation of the monitored directory ("Refresh").

# Technical Requirements

## Development Language
[TechLangRustLatestV1] Rust (latest stable version).

## UI Framework
[TechUiFrameworkWindowsRsV1] `windows-rs` for direct Win32/WinRT API interaction.

## Modularity & Testing
[TechModularityLogicalModulesV1] * The codebase shall be organized into logical modules (e.g., UI, core logic, profile management, file operations, monitoring).
[TechModularityUnitTestsCoreV1] * Core logic components shall have unit tests where feasible.

## Error Handling
[TechErrorHandlingGracefulV2] Graceful error handling for file operations, profile loading/saving, monitoring, etc., with user-facing messages in the UI.

## Performance
[TechPerfResponsiveUiV1] For typical source code repositories, UI should remain responsive. File system monitoring and processing should be efficient to avoid UI lag. Asynchronous operations may be needed for monitoring.

## Dependencies
[TechDepsMinimizeExternalV1] Minimize external dependencies, using well-maintained and popular crates where necessary.

# Future Considerations (Optional/Post-MVP)

[FutureGitignoreIntegrationV1] * Integration with `.gitignore` or similar ignore files.
[FutureBinaryFileDetectionSophisticatedV1] * More sophisticated binary file detection.
[FutureArchiveHeaderFormatConfigurableV1] * Configurable archive header/footer format.
[FutureClipboardCopyArchiveV1] * "Copy to Clipboard" option for the generated archive content.
[FutureEncodingSupportOtherV1] * Support for other character encodings (if UTF-8 proves insufficient).
[FutureAutomatedArchivingV1] * Option for automated archive regeneration upon detecting changes (with user consent).
[FutureProfileEditDialogV1] * A dedicated "Edit Profile" or "Manage Profiles" dialog allowing changes to name, root folder, and archive path of existing profiles.
[FutureSaveSelectionsOnExitV1] * Option to automatically save selection changes to the active profile on application exit, or prompt the user if unsaved changes exist.
