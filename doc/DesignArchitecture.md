# SourcePacker Design and Architecture

## The project shall consist of several small modules, where each is has a full set of tests.

## For every module, add a comment that describes its purpose and key components.
The comment shall emphasize why it is being used, and why it is doing what it does.
The comment shall be succinct no more than two paragraphs.
Don't refer to any internal names in the comment, as these usually get outdated and requires frequent updates.
The comment should strive to be persistent. That is, it shouldn't need to be updated.

## User Interface Architecture: A Two-Layer Approach

The user interface will be structured into two distinct layers: a high-level Application Logic Layer and a low-level Platform Abstraction Layer. This separation aims to isolate platform-specific details from the core application behavior, enhancing testability, maintainability, and potential portability of the application logic.

### 1. Application Logic Layer

#### Purpose

This layer is responsible for the core user interface logic and application state management, entirely independent of any specific GUI toolkit (like Win32). It defines *what* the UI should represent and how it reacts to user interactions in a platform-agnostic manner. Its primary goals are:

- **Platform Independence:** Contains no direct dependencies on native UI libraries (e.g., `windows-rs`).
- **Testability:** Enables UI logic to be unit-tested without a running graphical environment.
- **Clear Semantics:** Focuses on application behavior and data manipulation, rather than rendering details.

#### Key Concepts

-   **Application State Models:** Manages the data that drives the UI (e.g., the file tree structure, current profile, selection states).
-   **Event Handling Logic:** Implements a defined interface (provided by the Platform Abstraction Layer) to process platform-agnostic UI events (e.g., a button click, an item selection).
-   **Command Generation:** Based on its internal state or incoming events, it issues descriptive, platform-agnostic commands to the Platform Abstraction Layer to effect changes in the native UI (e.g., "create a window with this title," "update this item's state").

### 2. Platform Abstraction Layer

#### Purpose

This layer serves as a bridge between the platform-agnostic Application Logic Layer and the native operating system's UI toolkit (e.g., Win32). It provides an idiomatic Rust API that abstracts the complexities of direct native programming. Its primary goals are:

- **Encapsulating Native Code:** Contains all interactions with the specific UI toolkit (e.g., `windows-rs` for Win32), including window creation, message loops, and control management.
- **Providing a Stable Interface:** Offers a well-defined set of types and functions for the Application Logic Layer to consume.
- **Translating Communication:** Converts platform-agnostic commands from the Application Logic Layer into native UI operations, and translates native OS events into platform-agnostic events for the Application Logic Layer.

#### Key Components & Interaction Flow

-   **Platform Interface:** The primary API surface exposed to the Application Logic Layer for creating and managing UI elements and running the application.
-   **Opaque Handles:** Represents native UI elements (like windows or tree view items) with platform-agnostic identifiers. The Application Logic Layer uses these identifiers without needing to know about native handle types (e.g., `HWND`).
-   **Declarative Configuration:** The Application Logic Layer describes UI elements (e.g., a window's properties, a list of tree items) using platform-agnostic data structures when requesting their creation.
-   **Platform-Agnostic Commands:** A defined set of instructions (e.g., an enum) that the Application Logic Layer can send to the Platform Abstraction Layer to manipulate the UI (e.g., `CreateWindow`, `PopulateTreeView`, `SetItemState`).
-   **Platform-Agnostic Events:** A defined set of event types (e.g., an enum) that the Platform Abstraction Layer emits to the Application Logic Layer when user interactions or system events occur (e.g., `WindowCloseRequested`, `TreeItemToggled`).
-   **Event Handler Trait/Callback:** A mechanism (typically a trait implemented by the Application Logic Layer or a closure) through which the Platform Abstraction Layer delivers `AppEvent`s. The handler in the Application Logic Layer processes these events and may return a list of `PlatformCommand`s.
-   **Native Implementation Details:** Internally, this layer manages the native event loop (`WndProc` in Win32), native window and control handles, and the direct calls to the native UI APIs.

### Flow Summary (Revised)

1.  The Application Logic Layer, using the Platform Interface, requests the creation of UI elements (e.g., a main window) by providing platform-agnostic configurations.
2.  The Platform Abstraction Layer translates these requests into native UI toolkit calls, creating the actual OS windows and controls. It returns opaque handles to the Application Logic Layer.
3.  The Platform Abstraction Layer runs the native event loop.
4.  When a native UI event occurs (e.g., mouse click, key press):
    a.  The Platform Abstraction Layer captures it.
    b.  It translates the native event into a platform-agnostic `AppEvent`, often referencing UI elements via their opaque handles.
    c.  It delivers this `AppEvent` to the registered event handler in the Application Logic Layer.
5.  The Application Logic Layer processes the `AppEvent`:
    a.  It may update its internal state models.
    b.  It may decide to issue one or more `PlatformCommand`s back to the Platform Abstraction Layer to change the UI (e.g., update text, change an item's check state).
6.  The Platform Abstraction Layer receives these `PlatformCommand`s and executes them by making the corresponding native UI toolkit calls.
