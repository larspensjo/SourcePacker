# SourcePacker Design and Architecture

## The project shall consist of several small modules, where each is has a full set of tests.

## For every module, add a comment that describes its purpose and key components.
The comment shall emphasize why it is being used, and why it is doing what it does.
The comment shall be succinct no more than two paragraphs.
Don't refer to any internal names in the comment, as these usually get outdated and requires frequent updates.
The comment should strive to be persistent. That is, it shouldn't need to be updated.

## UI Facade for Win32 API Interaction

### Purpose

Provide a safe, organized layer over the low-level Win32 API to make building and managing the user interface in Rust simpler and safer. This includes:

- **Hiding Boilerplate:** Simplify window creation and message loop management.
- **Encapsulating Unsafe Code:** Offer a safer, Rust-friendly API.
- **Improving Readability:** Separate UI concerns from application logic.
- **Supporting UI State:** Allow associating data and logic with windows and controls.

### Key Components

- **Application Struct:** Manages app startup and the main event loop.
- **WindowBuilder:** Simplifies window configuration and creation.
- **Window Struct:** Represents a window, managing its handle and events.
- **Control Wrappers (e.g., Button):** Simplify control creation and event handling.
- **Event System:** Lets application code respond to UI events without touching raw `WndProc`.

### Flow Summary

1. Create the `Application` and main window via `WindowBuilder`.
2. Add controls and register event handlers.
3. Run the main event loop.
4. The facade routes system events to your registered callbacks.
