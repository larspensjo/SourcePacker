# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements.

**Goal:** Decouple UI structure definition from the platform-specific implementation. The `platform_layer` should become a generic, reusable library independent of any specific application's UI. Changes to the application's look or control population should not require changes to the `platform_layer`. New UI features should be added as modular addons rather than expanding the core control mechanisms.

---

## Phase A: MVP Refinements, Platform Layer Restructuring, and Generic Layout

**Goal:** Fully realize the separation of concerns, making the `platform_layer` a truly generic View. This involves ensuring all application-specific UI knowledge resides outside the `platform_layer` and migrating all control-specific logic into dedicated handlers.

### Sub-Phase A.I: Initial Command-Driven UI Creation

*   **(Completed)** UI elements like MainMenu, TreeView, Panels, and Labels are created via `PlatformCommand`s.

### Sub-Phase A.II: Implementing Generic Layout, Controls, and Handlers

**Step A.II.1: Aggressively Implement a Truly Generic Layout Engine**
*   **(Completed)**

**Step A.II.2: Generic Control Creation**
*   **(Completed)**

**Step A.II.3: Generic Control Update Mechanism**
*   **(Completed)**

**Step A.II.4: Ensure All Control Operations Are Generically Targetable**
*   **(Completed)**

**Step A.II.5: Create `controls` Sub-Module and Handler Skeletons**
*   **(Completed)** The `platform_layer/controls` module exists with several handlers.

**Step A.II.6: Migrate Control-Specific Logic to Handlers (Iteratively)**

This is the **current focus**. The goal is to move all logic for a specific control type out of `command_executor.rs` and `window_common.rs` and into its dedicated handler module.

*   **Process for each remaining control type:**
    *   **a. Command Handling:** Move the `execute_create_*` and `execute_update_*` functions from `command_executor.rs` into the control's `*_handler.rs` file. Update `command_executor` to call the new handler function.
    *   **b. Notification/Message Handling:** Move the relevant `WM_*` message processing logic from `window_common.rs` into the control's `*_handler.rs` file. Update `window_common` to call the new handler function.

*   **Priority Order for Migration:**

    1.  **Label (`label_handler.rs`)**
        *   **(Completed)**

    2.  **TreeView (`treeview_handler.rs`)**
        *   **(Completed)**

    3.  **Input (`input_handler.rs`)**
        *   **(Completed)**

    4.  **Button (`button_handler.rs`)**
        *   **(Completed)**

    5.  **Panel (`panel_handler.rs`)**
        *   **(Completed)**

    6.  **Menu (`menu_handler.rs`)**
        *   **Action:** Create `menu_handler.rs`.
        *   **Action:** Move `execute_create_main_menu` and `add_menu_item_recursive_impl` from `command_executor.rs` to `menu_handler.rs`.
        *   **Action:** Extract menu logic (where `control_hwnd.0 == 0`) from `window_common::handle_wm_command` into a new function in `menu_handler.rs`.
        *   **Action:** Update call sites in `command_executor` and `window_common`.

---

### Sub-Phase A.III: Review and Finalize Platform Layer Generality

**Step A.III.1: Review and Remove Residual UI-Specific Knowledge**
*   **Action:** After all control logic is migrated, perform a thorough review of the entire `platform_layer`.
*   **Goal:** Ensure no SourcePacker-specific logic, layout assumptions, or control ID knowledge remains. All such specifics must be driven by `PlatformCommand`s. Constants like `ID_TREEVIEW_CTRL` must only be used in `app_logic` and passed into the `platform_layer`.
*   *Verification:* Code review confirms increased generality. The `platform_layer` acts as a generic command executor and event translator.

**Step A.III.2: Implement Unit Tests for Key Platform Layer Components**
*   **Goal:** Increase the robustness and maintainability of the `platform_layer` by adding unit tests for its pure logic, independent of the Win32 API. This verifies the correctness of data management and complex algorithms like layout calculation.
*   **Strategy:**
    *   **a. Test `NativeWindowData`:**
        *   **Action:** Create a `#[cfg(test)] mod tests` module at the bottom of `window_common.rs`.
        *   **Action:** Write unit tests for `NativeWindowData` methods to verify correct state management. Test cases should cover `register_control_hwnd`, `register_menu_action`, `set_label_severity`, `set_input_background_color`, etc.
        *   *Justification:* These methods are pure data manipulations and are easy to test, forming a solid foundation.
    *   **b. Refactor and Test the Layout Engine:**
        *   **Action:** In `window_common.rs`, refactor `apply_layout_rules_for_children`.
        *   **Sub-Action 1:** Extract the core calculation logic into a new, pure, private function (e.g., `calculate_layout`). This function will not have any Win32 dependencies. It will take a `RECT` and a slice of `LayoutRule`s and return a `HashMap<i32, RECT>` containing the calculated position and size for each child control.
        *   **Sub-Action 2:** Simplify the existing `apply_layout_rules_for_children` function. It will now become a "Humble Object" that calls the pure `calculate_layout` function and then iterates through the results, applying them with the impure `MoveWindow` API call.
        *   **Action:** Write comprehensive unit tests for the new `calculate_layout` function. Test various docking scenarios (`Top`, `Bottom`, `Fill`, `ProportionalFill`), margins, and nested structures to ensure the algorithm is correct.
*   *Verification:* `cargo test` successfully compiles and runs all new tests in `window_common.rs`. The logic of the layout engine is proven correct without requiring an active window.

**Step A.III.3: Implement Unit Tests for `Win32ApiInternalState`**
*   **Goal:** Verify the correctness of the platform layer's central state machine (`Win32ApiInternalState` in `app.rs`) by testing its pure logic components.
*   **Strategy:**
    *   **a. Add a Test Module:**
        *   **Action:** Create a `#[cfg(test)] mod tests` module at the bottom of `app.rs`.
    *   **b. Test State Management Methods:**
        *   **Action:** Write unit tests for methods that manage the internal state of `Win32ApiInternalState`.
        *   **Test Cases:**
            *   `generate_unique_window_id()`: Assert that sequential calls produce unique IDs.
            *   `remove_window_data()`: Assert that a window's data is correctly removed from the `active_windows` map.
            *   `with_treeview_state_mut()`: Write tests to verify that the treeview state is correctly taken, passed to a closure, and returned to the map, both on success and on closure failure (`Err` result). This ensures the state is never lost.
    *   **c. Refactor and Test Quit Logic:**
        *   **Action:** In `app.rs`, refactor `check_if_should_quit_after_window_close`.
        *   **Sub-Action 1:** Extract the pure checking logic into a new private function (e.g., `should_quit_on_last_window_close(&self) -> bool`). This function will only check if the `active_windows` map is empty.
        *   **Sub-Action 2:** Simplify the existing `check_if_should_quit_after_window_close` to call the pure check function and then make the impure `PostQuitMessage` call if it returns `true`.
        *   **Action:** Write unit tests for the new `should_quit_on_last_window_close` function. Test both cases: when windows exist (returns `false`) and when the map is empty (returns `true`).
*   *Verification:* `cargo test` successfully runs new tests in `app.rs`, proving the correctness of the platform's core state management logic.

**Step A.III.4: Future Exploration: Advanced Layout and Deeper Decomposition**
*   **(Future)** This remains a longer-term goal. Consider advanced layout managers (e.g., grid, stack panels) as generic offerings within `platform_layer`, configurable by `ui_description_layer`.
*   **(Future)** Evaluate if `NativeWindowData` can be made more generic or if control-specific state can be fully encapsulated within their respective handlers.
