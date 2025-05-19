# Streamlined Plan: Dependency Inversion for AppEvents

This is an idea, not a decision. Maybe I will go through it later.

**Goal:** Refactor event handling so event logic resides with event objects, using an `AppLogicInterface` to break direct dependencies.

---

**Phase 1: Setup & First Event (e.g., `WindowCloseRequested`)**

1.  **Define `AppLogicInterface` Trait (Minimal):** Create `src/app_logic_interface/mod.rs`. Define the trait with only the methods needed by the first event (e.g., `get_main_window_id()`).
2.  **Define `ExecutableAppEvent` Trait:** In `platform_layer`, create a trait with an `execute(&self, app_logic: &mut dyn AppLogicInterface)` method.
3.  **Create Concrete Event Struct:** In `platform_layer`, define a struct for the first event (e.g., `WindowCloseRequestedEvent`), implementing `ExecutableAppEvent`. Its `execute` method will contain the original logic for this event, using the `AppLogicInterface`.
4.  **Adapt `PlatformEventHandler`:** Modify this trait (implemented by `MyAppLogic`) to have a new method like `process_executable_event(Box<dyn ExecutableAppEvent>)`.
5.  **Implement in `MyAppLogic`:**
    *   Implement the `AppLogicInterface` (with the minimal methods).
    *   Implement `process_executable_event` to call `event.execute(self)`.
6.  **Update Platform Layer:** Modify the platform code (e.g., WndProc) to:
    *   Instantiate the new concrete event struct (e.g., `Box::new(WindowCloseRequestedEvent { ... })`).
    *   Pass this `Box<dyn ExecutableAppEvent>` to `MyAppLogic` via the `process_executable_event` method.
7.  **Test:** Verify the first refactored event works correctly.

---

**Phase 2: Iteratively Refactor Remaining Events**

For each subsequent `AppEvent` variant:

1.  **Extend `AppLogicInterface`:** Add any new methods to the interface that the current event's logic requires from `MyAppLogic`.
2.  **Create Concrete Event Struct:** Define a new struct in `platform_layer` for this event, implementing `ExecutableAppEvent`. Port the event's original handling logic into its `execute` method, using the `AppLogicInterface`.
3.  **Implement in `MyAppLogic`:** Add implementations for any new `AppLogicInterface` methods.
4.  **Update Platform Layer:** Modify the platform code to instantiate and dispatch this new event struct.
5.  **Test:** Verify the newly refactored event.

*   **Handle Shared State:** For events whose behavior depends on shared state within `MyAppLogic` (like `pending_action`), ensure `AppLogicInterface` provides methods to manage that state (e.g., `pending_action_take()`, `set_pending_action()`).
*   **Constants:** Make necessary constants (like control IDs or `APP_NAME_FOR_PROFILES`) accessible to event structs, either by passing them into the event struct or by defining them in the `app_logic_interface` module.

---

**Phase 3: Final Cleanup**

1.  Once all events are refactored, remove the old `AppEvent` enum and the original `MyAppLogic::handle_event` method.
2.  Review and refine the `AppLogicInterface` for clarity and conciseness.
