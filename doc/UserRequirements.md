# Requirement Specification for SourcePacker

This document outlines the requirements for SourcePacker, a file selection and archive tool designed to package source code for AI prompts.

# Core Functionality

## File System Traversal
The application must be able to scan a user-specified root directory and display its file and folder structure in a tree view.

## File Selection
* Each file and folder in the tree view shall have one of three selection states:
    *   **Selected:** The item is explicitly included in the archive.
    *   **Deselected:** The item is explicitly excluded from the archive.
    *   **Unknown:** The item's inclusion is not yet determined (initial state).
* Selecting or deselecting a folder shall recursively apply the same state to all its child files and folders.

## File Filtering (Whitelist)
* Profiles shall define a list of whitelist glob patterns (e.g., `*.rs`, `src/**/*.toml`).
* Only files matching at least one whitelist pattern shall be considered for display and selection in the tree view. Files not matching any pattern will not be shown or processed.

## Text File Focus
The application is intended for text-based source code. It should primarily handle files assumed to be UTF-8 encoded.
* (Future Consideration) A simple mechanism to identify and potentially warn about or exclude binary files might be useful, though initially, all files matching whitelist patterns will be treated as text.

## Archive Generation
* The application shall create a single `.txt` archive file from all "Selected" files.
* The content of selected files shall be concatenated into the archive.
* Each file's content in the archive shall be preceded by a simple header (e.g., `--- START FILE: path/to/file.rs ---`) and followed by a simple footer (e.g., `--- END FILE: path/to/file.rs ---`).

## Token Count Estimation
* The application shall display an estimated token count for the currently selected files.
* This count shall update live as files are selected or deselected. (Mechanism for tokenization TBD, e.g., word count or a specific tokenizer).

# Profile Management

## Profile Definition
A profile encapsulates:
* A root folder path.
* The selection state (Selected/Deselected) of files and folders within that root folder. (Unknown state is not saved; new files in a known folder appear as Unknown).
* A list of whitelist file patterns.

## Profile Storage
* Profiles shall be saved as individual JSON files.
* Profiles shall be stored in `%APPDATA%\SourcePacker\profiles\`.

## Profile Operations
* **Load:** Users can switch between different profiles.
* **Save:** Users can save the current selection state and whitelist patterns as a new profile or overwrite an existing one.
* **Create New:** Users can create a new, empty profile, specifying a root folder and initial whitelist patterns.
* **Duplicate:** Users can duplicate an existing profile to create a new one based on it.
* **Delete:** Users can delete existing profiles.

## Default Profile
* On application start, the most recently used profile shall be loaded by default.
* If no previous profile exists, the application may start with a blank state or prompt the user to create/open a profile.

## Handling Missing Files
* When loading a profile, if a previously selected file or folder no longer exists, it should be indicated in the UI (e.g., greyed out, marked with an icon) or silently removed from the selection. The profile itself should persist the path.

# User Interface (Windows Native UI via `windows-rs`)

## Main Window
A single main application window.

## Tree View
* Display the file and folder structure starting from the profile's root folder.
* Visually indicate the selection state (Selected, Deselected, Unknown) for each item. (Suggestion: Tristate checkboxes).

## File Content Viewer
* A panel or separate window to display the content of a selected file from the tree view (read-only).

## Search Functionality
* **File Name Search:** Allow users to filter the tree view by file/folder names.
* **Content Search:** Allow users to search for text strings within the files currently displayed in the tree (or within selected files) and highlight/filter matching files.

## Status Bar
Display relevant information such as:
* Current profile name.
* Number of selected files.
* Total size of selected files (optional).
* Live estimated token count.

## Menu/Toolbar
Provide access to functions like:
* Profile management (Open, Save, Save As, New, Duplicate, Delete, Manage Profiles).
* Set Root Folder (for new profiles).
* Edit Whitelist Patterns.
* Generate Archive.
* Refresh tree view.

# Technical Requirements

## Development Language
Rust (latest stable version).

## UI Framework
`windows-rs` for direct Win32/WinRT API interaction.

## Modularity & Testing
* The codebase shall be organized into logical modules (e.g., UI, core logic, profile management, file operations).
* Core logic components shall have unit tests where feasible.

## Error Handling
Graceful error handling for file operations, profile loading/saving, etc.

## Performance
For typical source code repositories, UI should remain responsive. Asynchronous operations are not an initial requirement but can be considered for future optimization if large directories cause UI lag.

## Dependencies
Minimize external dependencies, using well-maintained and popular crates where necessary.

# Future Considerations (Optional/Post-MVP)

* Integration with `.gitignore` or similar ignore files.
* More sophisticated binary file detection.
* Configurable archive header/footer format.
* "Copy to Clipboard" option for the generated archive content.
* Support for other character encodings (if UTF-8 proves insufficient).