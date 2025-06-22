# Detailed Development Plan for SourcePacker

This plan breaks down the development of SourcePacker into small, incremental steps. Optional steps or features intended for later are marked.

---

# Phase 2: Basic UI & Interaction - Post Sync

(Earlier steps already completed)

## P2.11: General Cleanup and Refinements

This section details various cleanup tasks, refactorings, and minor enhancements to improve code quality, user experience, and testability.

### P2.11.5: Future Enhancements & Nice-to-Haves (Low Priority for P2)
*   **Structured Application Settings File:**
    *   **Problem:** `last_profile_name.txt` is limited.
    *   **Action:** Plan to migrate `core::config` to use JSON (e.g., `app_settings.json`) if more settings are anticipated.
    *   **Rationale:** Scalable solution for preferences.
    *   **Status:** Not done (low priority).
*   **Modularity of `handler.rs` and `handler_tests.rs`:**
    *   **Problem:** Files can become large.
    *   **Action:** If `app_logic/handler.rs` becomes unwieldy *after* the M/V/P refactor, consider further sub-modules for specific flows.
    *   **Rationale:** Long-term maintainability.
    *   **Status:** M/V/P was the major step. Further sub-moduling can be considered if files grow excessively.

### P2.11.6: Improved disconnect between UI elements and actions
*   When you click on a button or click on a menu item, the execution flow is different. But it should be harmonized. The event should be made independent of the source.
    * For example, when I changed a regular button into a menu item, I had to change code in types.rs, ui_descrptive_layer, handler.rs

### P2.11.7: Should the treeview be created from ui_descriptive_layer?

## P2.13: Improved Profile management
*   Implement a proper dialog for profile selection in the platform layer (not a stub).
*   Implement a user-friendly UI for listing, selecting, creating, and deleting profiles within the application.

# Phase 3: Enhancements & UX Improvements

## P3.2: Quick search visual enhancement
*   When doing a quick search, the text matching should be highlighted.

## P3.3: File Content Viewer (preview)
*   Add a read-only text control. `[UiContentViewerPanelReadOnlyV1]`
*   Load selected file's content into the viewer.

## P3.5: Handling File State Discrepancies Visually
*   When loading a profile or after refresh:
    *   Files in profile but not on disk: Mark visually (e.g., greyed out). `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`
    *   Files on disk but not in profile: Appear as `FileState::Unknown`. `[FileStateNewUnknownV2]`, `[UiTreeViewVisualFileStatusV1]`

## P3.6: Full Profile Management UI
*   "File -> Manage Profiles..." opens a dedicated dialog. `[UiMenuProfileManagementV1]`
*   Dialog with list, "Create New", "Duplicate", "Delete".
*   "Edit Profile": Allow changing name, root folder (triggers rescan), `archive_path`. `[UiMenuSetRootFolderV1]`
*   "Duplicate Profile" dialog. `[ProfileOpDuplicateExistingV1]`
*   "Delete Profile": Confirm, remove file. If active, trigger profile selection. `[ProfileOpDeleteExistingV1]`

## P3.7: But to expand the whole tree
    *   **(Note: This is partially addressed by the "Expand Filtered/All" button in P4.1. This item can be considered superseded or a refinement if P4.1's button doesn't cover all desired "expand all" scenarios.)**

# Phase 4: Advanced Features (Optional / Future)

## P4.1: Quick File & Folder Filter `[UiQuickFileFilterV1]`

**Goal:** Implement an interactive filter for the TreeView, allowing users to quickly find files and folders by name using simple text and glob patterns.

**User Story:** As a user, I want to type text into a filter box to dynamically narrow down the items shown in the file tree, so I can quickly locate specific files or folders in large projects. I also want to easily expand the filtered results or the entire tree.

Implementation complete.

## P4.2: Content Search `[UiSearchFileContentHighlightV1]`
*   Input for search string.
*   Button "Search in Selected".
*   Highlight files in tree.
*   (Optional) Show occurrences in File Content Viewer.

## P4.4: "Clear Selection" / "Select All Files" / "Invert Selection" options.

## P4.5: Better Binary File Detection `[TextFileFocusUTF8V1]`
*   Implement robust check (e.g., percentage of non-printable chars).
*   Visually indicate/exclude binary files. `[FutureBinaryFileDetectionSophisticatedV1]`

## P4.6: Graphical effect on status change
*   When something in the status field changes, I want there to be a visual effect that gradually fades away.

## P4.7: Future Exploration: Advanced Layout and Deeper Decomposition**
*   **(Future)** This remains a longer-term goal. Consider advanced layout managers (e.g., grid, stack panels) as generic offerings within `platform_layer`, configurable by `ui_description_layer`.
*   **(Future)** Evaluate if `NativeWindowData` can be made more generic or if control-specific state can be fully encapsulated within their respective handlers.
