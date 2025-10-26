# SourcePacker - Master Development Plan

This document outlines the prioritized, sequential development plan for SourcePacker. It serves as the master roadmap, consolidating goals from other planning files.

## **Phase 1: UI Architectural Refinement (High Priority)**

**Objective:** Transform the `platform_layer` into a reusable library, improve type safety, and formalize the command-driven UI pattern. This is a critical step for enhancing code quality and maintainability.

*   **Task 1.1: Refactor `platform_layer` into the `CommanDuctUI` Library**
    *   **Goal:** Create a high-quality, reusable Rust library for command-driven native Windows UI.
    *   **Status:** **Not Started.**
    *   **Action:** Follow the detailed steps outlined in `Plan.CommanDuctUI.md`. Key steps include:
        1.  Create the new `CommanDuctUI` library crate.
        2.  Migrate the existing `platform_layer` code.
        3.  **Implement the type-safe `ControlId` wrapper** to replace raw `i32` IDs.
        4.  Integrate the new library back into SourcePacker as a crate dependency.
        5.  Set up `build.rs` in SourcePacker to auto-generate `ControlId` constants.
    *   **Justification:** This addresses the need for type safety ("Newtypes for IDs") and improves the "disconnect between UI elements and actions" by solidifying the command pattern.

## **Phase 2: Backend Architectural Refinement (High Priority)**

**Objective:** Unify the handling of asynchronous background tasks to simplify the core application logic and improve scalability for future features.

*   **Task 2.1: Implement Generalized Job Manager**
    *   **Goal:** Refactor the separate `TokenRecalcDriver` and `ContentSearchDriver` into a single, unified job management system.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  Define a generic `Job` trait and `JobKind` enum to represent different types of background work (e.g., `TokenRecalculation`, `ContentSearch`).
        2.  Create a `JobManager` struct within `MyAppLogic` to replace the individual `Option<...Driver>` fields.
        3.  The `JobManager` will be responsible for spawning threads, managing communication channels (`mpsc::Receiver`), and holding `JoinHandle`s for all active jobs.
        4.  Refactor `MyAppLogic::try_dequeue_command` to call a single `job_manager.poll_all_jobs()` method instead of multiple `poll_...()` functions.
        5.  The `JobManager`'s poll method will check all active channels and return a `Vec` of completed job results (e.g., `JobResult::TokenProgress`, `JobResult::ContentSearchComplete`) for `MyAppLogic` to process.
    *   **Justification:** This is a key architectural improvement. It drastically simplifies `MyAppLogic` by centralizing asynchronous task management. It makes the system more robust and scalable, making it trivial to add future background jobs (like autocomplete indexing) without increasing the complexity of the main event loop.

## **Phase 3: Core UI Feature Implementation**

**Objective:** Add essential, user-facing features that improve the core usability of the application. These tasks can begin once the architectural refactoring is complete.

*   **Task 3.1: Implement Advanced Content Search Preview**
    *   **Goal:** Enhance the file content viewer to highlight search matches and allow cycling between them, turning the preview pane into an interactive tool.
    *   **Status:** **Not Started.**
    *   **Action:** Follow the detailed steps in **Phase 4** of `Plan.ContentSearch.md`. Key steps include:
        1.  Upgrade the viewer control to a `RichEdit` control.
        2.  Enhance the `search_content_async` worker to return the line and column number of each match.
        3.  Implement a new `PlatformCommand` to set viewer content *with* a list of ranges to highlight.
        4.  Add "Next/Previous" buttons to the UI to navigate between the highlighted matches within the selected file.
    *   **Requirement:** `[UiSearchFileContentHighlightV1]` (This will be a new version of the requirement).

*   **Task 3.2: Implement Visual Handling for Missing Files**
    *   **Goal:** Visually distinguish files that are part of a profile but are missing from the disk.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  During profile load, identify paths from the profile that do not exist in the file system scan results.
        2.  Create `FileNode`s for these missing paths with a distinct state (e.g., `SelectionState::Missing`).
        3.  Modify the platform layer's TreeView custom draw logic (`handle_nm_customdraw`) to render items with this state differently (e.g., greyed-out text).
    *   **Requirement:** `[ProfileMissingFileIndicateOrRemoveV1]`, `[UiTreeViewVisualFileStatusV1]`

## **Phase 4: Theming and UX Enhancements**

**Objective:** Improve the visual appeal and user experience of the application.

*   **Task 4.1: Implement Advanced Styling (Borders & Focus)**
    *   **Goal:** Enhance the "Neon Night" theme with custom borders and a visual indicator for focused controls.
    *   **Status:** **Not Started.**
    *   **Action:** Follow **Phase 5** of the `Plan.StyleGuides.md`. This involves:
        1.  Extending `ControlStyle` to include border properties.
        2.  Implementing `WM_NCPAINT`, `WM_SETFOCUS`, and `WM_KILLFOCUS` handlers to draw custom borders and change their color on focus.
        3.  Updating the theme definition in `theme.rs` with the new border styles.

*   **Task 4.2: Enhance Quick Search Visuals**
    *   **Goal:** Highlight matching text within the TreeView when a quick search is active.
    *   **Status:** **Not Started.**
    *   **Action:** This is a more advanced custom draw task. It would likely require modifying the `NM_CUSTOMDRAW` handler to parse the item text and render the matching substring with a different background color.

## **Phase 5: Full Profile Management**

**Objective:** Provide a complete user interface for managing profiles beyond loading and saving.

*   **Task 5.1: Build Profile Management Dialog**
    *   **Goal:** Create a dedicated dialog for users to create, duplicate, and delete profiles.
    *   **Status:** **Not Started.**
    *   **Action:**
        1.  Add a "Manage Profiles..." `MenuAction`.
        2.  Design and implement a new dialog using platform commands. This dialog should list existing profiles and have "Create New", "Duplicate", and "Delete" buttons.
        3.  Implement the corresponding `AppEvent` to handle the dialog's results.
        4.  Implement the backend logic in `CoreProfileManager` for deleting and duplicating profile files.
    *   **Requirements:** `[UiMenuProfileManagementV1]`, `[ProfileOpDuplicateExistingV1]`, `[ProfileOpDeleteExistingV1]`

## **Future Considerations (Post-Phase 5)**

*   **Autocomplete Search:** Implement autocomplete suggestions for the content search input, backed by a one-time indexing job run by the new `JobManager`.
*   **Advanced Selection:** Add "Clear Selection", "Select All", and "Invert Selection" options.
*   **Improved Binary File Detection:** Implement a more robust check for binary files to avoid errors during content search and tokenization. (`[FutureBinaryFileDetectionSophisticatedV1]`)
*   **Filesystem Watcher:** Use the `notify` crate to replace manual refresh with real-time file system monitoring.
