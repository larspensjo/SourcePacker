# SourcePacker Design and Architecture

## UI Facade for Win32 API Interaction

### Purpose

To simplify interaction with the low-level Win32 API (via `windows-rs`) for creating and managing the user interface. The facade aims to:

*   **Abstract Boilerplate:** Hide repetitive and verbose Win32 setup code (e.g., window class registration, window creation, message loops).
*   **Enhance Safety:** Encapsulate `unsafe` Win32 API calls, exposing a safer, more Rust-idiomatic interface to the rest of the application.
*   **Improve Readability & Maintainability:** Make the main application logic cleaner by separating UI implementation details.
*   **Promote Modularity:** Allow for a more organized structure of UI components.
*   **Manage UI State:** Provide a structured way to associate Rust data and logic with Windows UI elements (windows and controls).

### Core Components of the Facade

The UI facade will be organized around a few key structs and patterns:

1.  **`Application` (or `App`) Struct:**
    *   **Responsibilities:** Manages application-level concerns like instance handles and the main event message loop.
    *   **Key Goal:** Centralize application startup and the core event processing mechanism.

2.  **`WindowBuilder` (e.g., `MainWindowBuilder`):**
    *   **Responsibilities:** Provides a fluent, declarative API for configuring window properties (title, size, position, initial styles) before creation.
    *   **Key Goal:** Simplify the complex process of window creation and make it less error-prone.

3.  **`Window` (e.g., `MainWindow`) Struct:**
    *   **Responsibilities:** Represents an application window, holding its handle (`HWND`) and managing its lifecycle, state, and event handling (via callbacks or methods). It will internally bridge the static Win32 `WndProc` to instance-specific Rust logic using techniques like `GWLP_USERDATA`.
    *   **Key Goal:** Enable an object-oriented approach to window management, allowing window-specific data and behavior to be encapsulated.

4.  **`Control` Abstractions (e.g., `Button`, `TreeViewWrapper`):**
    *   **Responsibilities:** Represent individual UI controls within a window. They will provide methods for control-specific operations (e.g., adding items to a TreeView, getting/setting text) and for attaching event handlers (e.g., button clicks, selection changes).
    *   **Key Goal:** Simplify the creation, configuration, and event handling for common UI controls.

5.  **Event Handling Mechanism:**
    *   **Responsibilities:** Define a system (likely using closures or trait objects) for application code to respond to UI events (e.g., window close, button click, paint requests) without directly writing a monolithic `WndProc`.
    *   **Key Goal:** Decouple event detection (in the facade's `WndProc`) from event handling logic (in the main application code or specific UI component structs).

### Interaction Flow

1.  The main application initializes the `Application` facade component.
2.  It then uses a `WindowBuilder` to configure and create the main window, resulting in a `Window` facade object.
3.  The `Window` object is used to add `Control` facade objects (buttons, tree views, etc.).
4.  Event handlers (callbacks) are registered with the `Window` and `Control` objects.
5.  The `Application` facade component runs the main event loop.
6.  When UI events occur, the facade's internal `WndProc` dispatches these events to the appropriate registered callbacks on the `Window` or `Control` facade objects.

By implementing this facade, the core application logic in `main.rs` and other modules can focus on the application's features rather than the intricacies of direct Win32 API programming.
