# SourcePacker Design and Architecture

## Core Principles

This project emphasizes modularity, with each module aimed at a specific responsibility and accompanied by comprehensive tests. Module-level documentation will explain its purpose and rationale, focusing on why it exists and its role, rather than volatile internal details, to ensure comments remain relevant over time.

## User Interface Architecture: Model-View-Presenter (MVP)

SourcePacker's UI architecture is structured around the Model-View-Presenter (MVP) pattern. This pattern enhances testability, maintainability, and separates concerns effectively.

### 1. Model

*   **Purpose:** Manages the application's data, core business logic, and rules. It is the source of truth for the application's state and is entirely UI-agnostic.
*   **SourcePacker Context:** Includes `core` modules (e.g., `CoreProfileManager`, `CoreFileSystemScanner`) and the data-holding aspects of `app_logic` (e.g., `current_profile_cache`, `file_nodes_cache`).

### 2. View (`platform_layer` & `ui_description_layer`)

*   **Purpose:** Presents the Model's data to the user and routes user interactions (as raw input) to the Presenter. It is a passive interface, responsible for rendering and abstracting platform-specific UI details.
*   **Key Components & Responsibilities (`platform_layer`):**
    *   Encapsulates all interactions with the native UI toolkit (e.g., Win32).
    *   Manages native window and control handles, and the native event loop.
    *   Executes `PlatformCommand`s received from the Presenter to create or modify UI elements.
    *   Translates native OS events into platform-agnostic `AppEvent`s.
*   **View Definition (`ui_description_layer`):**
    *   Defines the static structure and layout of the UI by generating initial `PlatformCommand`s. This component tells the View *what* to display initially.

### 3. Presenter (`app_logic::handler`)

*   **Purpose:** Acts as the intermediary between the Model and the View. It retrieves data from the Model, formats it for display (by deciding what `PlatformCommand`s to send), and handles user input (`AppEvent`s) from the View to update the Model and instruct the View.
*   **Key Responsibilities:**
    *   Implements the `PlatformEventHandler` trait to receive `AppEvent`s from the View.
    *   Interacts with the Model to fetch data or trigger business logic.
    *   Contains UI-specific logic that doesn't belong in the View or Model (e.g., orchestrating dialog flows, deciding when to enable/disable controls based on application state).
    *   Generates `PlatformCommand`s to instruct the View on how to update.

### Communication Flow (MVP)

1.  **Initialization:**
    *   The `ui_description_layer` provides initial `PlatformCommand`s to the `platform_layer` (View) to build the static UI structure.
    *   The `platform_layer` creates native UI elements.
2.  **User Interaction:**
    *   The user interacts with a native UI element (e.g., clicks a button).
    *   The `platform_layer` (View) captures this native event, translates it into a platform-agnostic `AppEvent` (e.g., `AppEvent::MenuActionClicked { action: MenuAction::LoadProfile }`), and sends it to `app_logic` (Presenter).
3.  **Presenter Logic:**
    *   `app_logic` (Presenter) receives the `AppEvent`.
    *   It may query or update the Model (e.g., load a profile, scan files).
    *   Based on the event and Model state, it decides how the UI should change.
4.  **View Update:**
    *   `app_logic` (Presenter) issues one or more `PlatformCommand`s (e.g., `PlatformCommand::PopulateTreeView`, `PlatformCommand::UpdateStatusBarText`) to the `platform_layer` (View).
5.  **Rendering:**
    *   The `platform_layer` (View) receives these commands and executes them by making the corresponding native UI toolkit calls, updating what the user sees.

This MVP approach ensures the `app_logic` (Presenter) and `core` (Model) can be tested independently of the `platform_layer` (View), and the `platform_layer` can be developed as a reusable, generic UI toolkit abstraction.

## Key Abstractions for MVP

*   **`PlatformCommand` (Presenter -> View):** An enum defining all operations the Presenter can request the View to perform (e.g., create control, update text, show dialog).
*   **`AppEvent` (View -> Presenter):** An enum defining all UI interactions or platform events the View can report to the Presenter (e.g., button clicked, menu action triggered, window closed).
*   **`MenuAction`:** A semantic enum used by the Presenter and View Definition to refer to menu operations, abstracted from native IDs.
*   **Opaque Handles (`WindowId`, `TreeItemId`):** Used by the Presenter to refer to UI elements without needing knowledge of native handles.

## Lock Strategy Summary

*   **`MyAppLogic` (Presenter):** The entire instance is protected by a `std::sync::Mutex` (via `Arc<Mutex<dyn PlatformEventHandler>>`). This serializes all event handling and command generation within the Presenter, ensuring its internal state is consistent. Operations under this lock should be brief.
*   **`Win32ApiInternalState::window_map` (View State):** Protected by `std::sync::RwLock`. This allows concurrent reads of window data (e.g., by `WndProc` for different messages or by command handlers needing to look up `HWND`s) but serializes writes (e.g., adding/removing windows, modifying `NativeWindowData` like the `menu_action_map`). Locks are kept for minimal duration, especially avoiding calls to external/OS functions that might re-enter or send synchronous messages while a write lock is held (e.g., `SetMenu`).
*   **Mock Objects (Tests):** Utilize `std::sync::Mutex` for thread-safe testing of mock components.
