# SourcePacker Design and Architecture

## Core Principles

This project emphasizes modularity, with each module aimed at a specific responsibility and accompanied by comprehensive tests. Module-level documentation will explain its purpose and rationale, focusing on why it exists and its role, rather than volatile internal details, to ensure comments remain relevant over time.

## Project Context

* **Project root:** The folder the user opens via **File → Open Folder**. It is the anchor for all project state and never inferred implicitly.
* **Profile scan root:** Each profile still declares its own scan root. It often matches the project root but can point elsewhere when needed.
* **Archive target path:** The destination for the generated archive. It may live inside or outside the project tree.

The `ProjectContext` helper centralizes project-local paths so UI and core code do not hand-craft `.sourcepacker` locations. It resolves:

* `.sourcepacker/` – project-local configuration directory.
* `.sourcepacker/profiles/` – storage for profile JSON files.
* `.sourcepacker/last_profile.txt` – remembers the last profile used for this project.

The file system scanner hard-ignores `.sourcepacker` so SourcePacker never surfaces its own metadata in the tree view.

## Project-Centric Startup Flow

1. On UI setup completion, the application loads the last project path from global config (AppData). If the path exists, it becomes the active project.
2. For the active project, the application loads the project-local `last_profile.txt` to restore the last profile, if present.
3. If the stored project path is missing or invalid, the entry is cleared and the user is immediately prompted to pick a project folder before profile actions become available.
4. When no project is open, profile-related operations no-op with a clear log/status message; the main window prompts for folder selection.

## Configuration Boundaries

* **Global (AppData):** Cross-project data such as the last project path (and future recent-project lists) remain in AppData so they are available before any project is chosen.
* **Project-local (`.sourcepacker`):** Profiles, last-profile tracking, and any future project-scoped configuration live under the opened project's `.sourcepacker` directory, keeping projects portable.

## User Interface Architecture: Model-View-Presenter (MVP)

SourcePacker's UI architecture is structured around the Model-View-Presenter (MVP) pattern. This pattern enhances testability, maintainability, and separates concerns effectively.

Whenever new features or functionality is added to the UI system, they should preferably be added as addons.
That is, don't expand the control mechanisms making them very big with lots of values that need to be
configured. The basic function should be as simple as possible. Instead, use addon mechanisms that attaches
functionality as a separate feature.

### 1. Model

*   **Purpose:** Manages the application's data, core business logic, and rules. It is the source of truth for the application's state and is entirely UI-agnostic.
*   **SourcePacker Context:** Includes `core` modules (e.g., `CoreProfileManager`, `CoreFileSystemScanner`) and the data-holding aspects of `app_logic` (e.g., `current_profile_cache`, `file_nodes_cache`).

### 2. View (`platform_layer` & `ui_description_layer`)

*   **Purpose:** Presents the Model's data to the user and routes user interactions (as raw input) to the Presenter. It is a passive interface, responsible for rendering and abstracting platform-specific UI details.
*   **Key Components & Responsibilities (`platform_layer`):**
    *   Encapsulates all interactions with the native UI toolkit (e.g., Win32).
    *   Manages native window and control handles, **and maps logical control IDs (received in `PlatformCommand`s) to these native handles.**
    *   Executes `PlatformCommand`s received from the Presenter to create or modify UI elements.
    *   Translates native OS events into platform-agnostic `AppEvent`s.
*   **View Definition (`ui_description_layer`):**
    *   Defines the **initial static structure and layout** of the UI by generating `PlatformCommand`s.
    *   It uses **pre-defined logical control IDs** (e.g., `i32` constants shared with `app_logic`) when instructing the `platform_layer` to create these elements.
    *   This component tells the View *what* to display initially. Once these initial commands are issued, its primary role for that window instance is complete.

### 3. Presenter (`app_logic::handler`)

*   **Purpose:** Acts as the intermediary between the Model and the View. It retrieves data from the Model, formats it for display (by deciding what `PlatformCommand`s to send, **referencing UI elements by their logical control IDs**), and handles user input (`AppEvent`s) from the View to update the Model and instruct the View.
*   **Key Responsibilities:**
    *   Implements the `PlatformEventHandler` trait to receive `AppEvent`s from the View.
    *   Interacts with the Model to fetch data or trigger business logic.
    *   Contains UI-specific logic that doesn't belong in the View or Model (e.g., orchestrating dialog flows, deciding when to enable/disable controls based on application state).
    *   Generates `PlatformCommand`s (which include logical control IDs for targeting specific elements) to instruct the View on how to update.

### Communication Flow (MVP)

1.  **Initialization:**
    *   The `ui_description_layer` provides initial `PlatformCommand`s to the `platform_layer` (View) to build the static UI structure, **referencing elements by logical control IDs.**
    *   The `platform_layer` creates native UI elements **and internally maps their logical IDs to native handles.**
2.  **User Interaction:**
    *   The user interacts with a native UI element (e.g., clicks a button).
    *   The `platform_layer` (View) captures this native event, translates it into a platform-agnostic `AppEvent` (e.g., `AppEvent::MenuActionClicked { action: MenuAction::LoadProfile }`), and sends it to `app_logic` (Presenter).
3.  **Presenter Logic:**
    *   `app_logic` (Presenter) receives the `AppEvent`.
    *   It may query or update the Model (e.g., load a profile, scan files).
    *   Based on the event and Model state, it decides how the UI should change.
4.  **View Update:**
    *   `app_logic` (Presenter) issues one or more `PlatformCommand`s (e.g., `PlatformCommand::UpdateLabelText { label_id: YOUR_LOGICAL_ID, text: "...", severity: ... }`, `PlatformCommand::SetControlEnabled { control_id: ANOTHER_LOGICAL_ID, enabled: true }`) to the `platform_layer` (View), **using logical control IDs to target specific UI elements.**
5.  **Rendering:**
    *   The `platform_layer` (View) receives these commands, **uses the logical ID to look up the corresponding native UI handle**, and executes them by making the appropriate native UI toolkit calls, updating what the user sees.

This MVP approach ensures the `app_logic` (Presenter) and `core` (Model) can be tested independently of the `platform_layer` (View), and the `platform_layer` can be developed as a reusable, generic UI toolkit abstraction.

## Key Abstractions for MVP

*   **`PlatformCommand` (Presenter -> View):** An enum defining all operations the Presenter can request the View to perform (e.g., create control, update text, show dialog). **Commands that target specific controls use logical control IDs.**
*   **`AppEvent` (View -> Presenter):** An enum defining all UI interactions or platform events the View can report to the Presenter (e.g., button clicked, menu action triggered, window closed). **Events originating from specific controls also use logical control IDs.**
*   **`MenuAction`:** A semantic enum used by the Presenter and View Definition to refer to menu operations, abstracted from native IDs.
*   **Logical Control IDs (`i32`):** Pre-defined `i32` constants that serve as stable, platform-agnostic identifiers for UI controls.
    *   The `ui_description_layer` uses these IDs when generating `PlatformCommand`s for initial control creation.
    *   The `app_logic` (Presenter) uses these same IDs in `PlatformCommand`s to target specific controls for dynamic updates (e.g., changing text, enabling/disabling).
    *   The `platform_layer` is responsible for creating native controls based on these logical IDs and maintaining an internal mapping from these logical IDs to the actual native control handles (e.g., `HWND`s).
*   **Opaque Handles (`WindowId`, `TreeItemId`):** Used by the Presenter to refer to UI elements like windows or specific tree view nodes without needing knowledge of native handles. These are distinct from logical control IDs, which identify controls like buttons or labels.

## Lock Strategy Summary

*   **`MyAppLogic` (Presenter):** The entire instance is protected by a `std::sync::Mutex` (via `Arc<Mutex<dyn PlatformEventHandler>>`). This serializes all event handling and command generation within the Presenter, ensuring its internal state is consistent. Operations under this lock should be brief.
*   **`Win32ApiInternalState::window_map` (View State):** Protected by `std::sync::RwLock`. This allows concurrent reads of window data (e.g., by `WndProc` for different messages or by command handlers needing to look up `HWND`s) but serializes writes (e.g., adding/removing windows, modifying `NativeWindowData` like the `menu_action_map`). Locks are kept for minimal duration, especially avoiding calls to external/OS functions that might re-enter or send synchronous messages while a write lock is held (e.g., `SetMenu`).
*   **Mock Objects (Tests):** Utilize `std::sync::Mutex` for thread-safe testing of mock components.
