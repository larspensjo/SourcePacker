# SourcePacker - Master Development Plan

This document outlines the prioritized, sequential development plan for SourcePacker. It serves as the master roadmap, consolidating goals from other planning files.

## **Phase 1: Architectural Refinement (High Priority)**

**Objective:** Transform the `platform_layer` into a reusable library, improve type safety, and formalize the command-driven UI pattern. This is the most critical next step for enhancing code quality and maintainability.

*   **Task 1.1: Refactor `platform_layer` into the `CommanDuctUI` Library**
    *   **Goal:** Create a high-quality, reusable Rust library for command-driven native Windows UI.
    *   **Status:** **Not Started.**
    *   **Action:** Follow the detailed steps outlined in `Plan.CommanDuctUI.md`. Key steps include:
        1.  Create the new `CommanDuctUI` library crate.
        2.  Migrate the existing `platform_layer` code.
        3.  **Implement the type-safe `ControlId` wrapper** to replace raw `i32` IDs.
        4.  Integrate the new library back into SourcePacker as a crate dependency.
        5.  Set up `build.rs` in SourcePacker to auto-generate `ControlId` constants.
    *   **Justification:** This addresses the need for type safety ("Newtypes for IDs") and improves the "disconnect between UI elements and actions" (`P2.11.6`) by solidifying the command pattern.

## **Phase 2: Core UI Feature Implementation**

**Objective:** Add essential, user-facing features that improve the core usability of the application. These tasks can begin once the `CommanDuctUI` refactor is complete.

*   **Task 2.1: Implement File Content Viewer**
    *   **Goal:** Add a read-only panel to display the contents of the selected file.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  Add a read-only, multi-line `EDIT` control to the UI layout in `ui_description_layer`.
        2.  Define a new `PlatformCommand` to set its content (e.g., `SetViewerContent`).
        3.  Add a `TreeViewItemSelectionChanged` event to the platform layer.
        4.  In `MyAppLogic`, handle the selection change event to read the file's content and issue the `SetViewerContent` command.
    *   **Requirement:** `[UiContentViewerPanelReadOnlyV1]`

*   **Task 2.2: Implement Visual Handling for Missing Files**
    *   **Goal:** Visually distinguish files that are part of a profile but are missing from the disk.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  During profile load, identify paths from the profile that do not exist in the file system scan results.
        2.  Create `FileNode`s for these missing paths with a distinct state (e.g., `SelectionState::Missing`).
        3.  Modify the platform layer's TreeView custom draw logic (`handle_nm_customdraw`) to render items with this state differently (e.g., greyed-out text).
    *   **Requirement:** `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`

## **Phase 3: Theming and UX Enhancements**

**Objective:** Improve the visual appeal and user experience of the application.

*   **Task 3.1: Implement Advanced Styling (Borders & Focus)**
    *   **Goal:** Enhance the "Neon Night" theme with custom borders and a visual indicator for focused controls.
    *   **Status:** **Not Started.**
    *   **Action:** Follow **Phase 5** of the `Plan.StyleGuides.md`. This involves:
        1.  Extending `ControlStyle` to include border properties.
        2.  Implementing `WM_NCPAINT`, `WM_SETFOCUS`, and `WM_KILLFOCUS` handlers to draw custom borders and change their color on focus.
        3.  Updating the theme definition in `theme.rs` with the new border styles.

*   **Task 3.2: Enhance Quick Search Visuals**
    *   **Goal:** Highlight matching text within the TreeView when a quick search is active.
    *   **Status:** **Not Started.**
    *   **Action:** This is a more advanced custom draw task. It would likely require modifying the `NM_CUSTOMDRAW` handler to parse the item text and render the matching substring with a different background color.

## **Phase 4: Full Profile Management**

**Objective:** Provide a complete user interface for managing profiles beyond loading and saving.

*   **Task 4.1: Build Profile Management Dialog**
    *   **Goal:** Create a dedicated dialog for users to create, duplicate, and delete profiles.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  Add a "Manage Profiles..." `MenuAction`.
        2.  Design and implement a new dialog using platform commands. This dialog should list existing profiles and have "Create New", "Duplicate", and "Delete" buttons.
        3.  Implement the corresponding `AppEvent` to handle the dialog's results.
        4.  Implement the backend logic in `CoreProfileManager` for deleting and duplicating profile files.
    *   **Requirements:** `[UiMenuProfileManagementV1]`, `[ProfileOpDuplicateExistingV1]`, `[ProfileOpDeleteExistingV1]`

## **Future Considerations (Post-Phase 4)**

*   **Concurrency:** Offload long-running tasks (scanning, archiving) to background threads to ensure UI responsiveness.
*   **Content Search:** Implement search within file contents. (`[UiSearchFileContentHighlightV1]`)
*   **Advanced Selection:** Add "Clear Selection", "Select All", and "Invert Selection" options.
*   **Improved Binary File Detection:** Implement a more robust check for binary files. (`[FutureBinaryFileDetectionSophisticatedV1]`)
*   **Filesystem Watcher:** Use the `notify` crate to replace manual refresh with real-time file system monitoring.
