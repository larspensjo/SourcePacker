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
        *   **(Completed)**

---

### Sub-Phase A.III: Review and Finalize Platform Layer Generality

**Step A.III.1: Review and Remove Residual UI-Specific Knowledge**
        *   **(Completed)**

**Step A.III.2: Implement Unit Tests for Key Platform Layer Components**
        *   **(Completed)**

**Step A.III.3: Implement Unit Tests for `Win32ApiInternalState`**
        *   **(Completed)**

**Step A.III.4: Future Exploration: Advanced Layout and Deeper Decomposition**
*   **(Future)** This remains a longer-term goal. Consider advanced layout managers (e.g., grid, stack panels) as generic offerings within `platform_layer`, configurable by `ui_description_layer`.
*   **(Future)** Evaluate if `NativeWindowData` can be made more generic or if control-specific state can be fully encapsulated within their respective handlers.
