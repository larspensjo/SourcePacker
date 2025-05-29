# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step. We want the platform_layer to be independent of the actual application UI. The goal is to eventually break it out into a separate library that any application can use.

Whenever you want to change how your window loks like or population of controls, you should never need to change the platform_layer.

---

Completed changes have been removed.

---
## Phase 8: Centralizing Initial UI Command Execution in `run()`

**Goal:** Modify the application initialization flow so that initial UI structural commands (from `ui_description_layer`) are executed by the `platform_layer::run()` method, rather than directly in `main.rs`. This involves introducing a mechanism to signal `MyAppLogic` once this initial static UI setup is complete.

### Step 8.1: Define New `PlatformCommand` and `AppEvent` for UI Setup Completion

*   **File:** `src/platform_layer/types.rs`
*   **Action:**
    *   Add a new `PlatformCommand` variant:
        ```rust
        // In PlatformCommand enum:
        // ...
        SignalMainWindowUISetupComplete { window_id: WindowId },
        ```
    *   Add a new `AppEvent` variant:
        ```rust
        // In AppEvent enum:
        // ...
        MainWindowUISetupComplete { window_id: WindowId },
        ```
*   **Rationale:** Provides a dedicated command to signal the end of initial UI description processing and a corresponding event for `MyAppLogic` to react to.
*   **Verification:** Code compiles. No functional change yet.

### Step 8.2: Modify `main.rs` to Prepare and Forward Initial Commands to `run()`

*   **File:** `src/main.rs`
*   **Action:**
    1.  After `platform_interface.create_window()`, call `ui_description_layer::describe_main_window_layout()` to get the `initial_ui_commands`.
    2.  Append the `PlatformCommand::SignalMainWindowUISetupComplete { window_id: main_window_id }` to this list of commands.
    3.  Modify the call to `platform_interface.run()` to pass this combined list of initial commands.
    4.  Remove the direct loop that executes UI commands in `main.rs`.
    5.  Remove the direct call to `my_app_logic.on_main_window_created()` from `main.rs` (this logic will be moved).
*   **Rationale:** `main.rs` now gathers all initial setup instructions and delegates their execution to `platform_layer::run()`.
*   **Verification:** Code compiles. Application will likely not fully initialize its UI or app logic state correctly until subsequent steps are done.

### Step 8.3: Modify `PlatformInterface::run()` to Process Initial Commands

*   **File:** `src/platform_layer/app.rs` (`PlatformInterface::run`)
*   **Action:**
    1.  Change the signature of `run()` to accept `initial_commands_to_execute: Vec<PlatformCommand>`.
    2.  Before starting the main event loop (`GetMessageW` loop):
        *   Iterate through `initial_commands_to_execute`.
        *   For each command, call `self.internal_state._execute_platform_command(command)`.
        *   Handle any errors during this initial command execution.
*   **Rationale:** `run()` now orchestrates the execution of initial static UI setup commands before entering the main event processing loop.
*   **Verification:** Initial UI commands (menu, button, status bar) are executed. The `SignalMainWindowUISetupComplete` command will be processed, but the event won't be handled by `MyAppLogic` yet.

### Step 8.4: Implement Handler for `SignalMainWindowUISetupComplete` Command

*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::_execute_platform_command`)
*   **Action:**
    *   Add a match arm for `PlatformCommand::SignalMainWindowUISetupComplete`.
    *   The handler for this command will retrieve the `event_handler` (MyAppLogic) and call `handler_guard.handle_event(AppEvent::MainWindowUISetupComplete { window_id })`.
*   **Rationale:** Enables the platform layer to translate the signal command into an application event.
*   **Verification:** The `AppEvent::MainWindowUISetupComplete` is now generated and sent to `MyAppLogic`.

### Step 8.5: Update `MyAppLogic` to Handle `MainWindowUISetupComplete` Event

*   **File:** `src/app_logic/handler.rs`
*   **Action:**
    1.  Add a match arm in `MyAppLogic::handle_event` for `AppEvent::MainWindowUISetupComplete { window_id }`.
    2.  Move the logic currently in `MyAppLogic::on_main_window_created()` into a new method, e.g., `on_ui_setup_complete(window_id: WindowId)`.
    3.  Call this new method from the `MainWindowUISetupComplete` event handler.
    4.  The original `MyAppLogic::on_main_window_created()` method can be removed or repurposed if it served any other pre-UI-setup role (unlikely in the current context).
*   **Rationale:** `MyAppLogic` now performs its initial data loading and dynamic UI setup in response to the `MainWindowUISetupComplete` event, ensuring the static UI is ready.
*   **Verification:** Application initializes correctly, with static UI created first, followed by `MyAppLogic`'s initialization logic (profile loading, tree population, etc.). The overall application behavior should be the same as before this phase, but the initialization flow is different.

## Phase A: MVP Refinements & Platform Layer Generalization

**Goal:** Further refine the separation of concerns according to MVP principles, focusing on making the `platform_layer` a truly generic View. This involves ensuring all UI-specific knowledge (beyond what's needed to render generic controls and translate events) resides in the `ui_description_layer` (View definition) or `app_logic` (Presenter). This phase also considers initial steps towards more advanced layout management.

### Step A.1: Relocate Control-Specific Layout Logic from `platform_layer` (View)

*   **File:** `src/platform_layer/window_common.rs` (primarily `Win32ApiInternalState::handle_wm_size`)
*   **Current Issue:** `handle_wm_size` currently has hardcoded knowledge of specific controls (TreeView, Button, Status Bar) and their intended layout relationships (e.g., TreeView above Button area, Button area above Status Bar).
*   **Action:**
    1.  **Define Layout Commands/Descriptions:** Introduce new `PlatformCommand` variants or augment existing ones to allow the `ui_description_layer` to describe the *layout relationships* or *anchoring* of controls, rather than just their existence. Examples:
        *   `PlatformCommand::DefineLayout { window_id, layout_rules: Vec<LayoutRule> }`
        *   `LayoutRule { control_id, anchor_top: Option<ControlIdOrEdge>, anchor_bottom: ..., size_policy_h: ..., size_policy_v: ... }`
        *   Alternatively, enhance `CreateButton`, `CreateTreeView`, etc., commands with optional layout parameters (e.g., `dock: DockStyle`, `margin: Rect`, `size_percentage: Option<f32>`).
    2.  **Update `ui_description_layer`:** Modify `describe_main_window_layout` to generate these new layout commands/parameters. It will now define not just *that* a button exists, but *where* it should generally be and how it should behave on resize (e.g., "button X is docked to the bottom-left of the button panel area").
    3.  **Generic `handle_wm_size`:** Refactor `handle_wm_size` in the `platform_layer` to be a generic layout engine. It will iterate through the controls registered for the window and apply the layout rules/parameters defined by the `ui_description_layer` for each control. It should no longer contain direct references to `BUTTON_AREA_HEIGHT`, specific control IDs for positioning, or fixed pixel calculations for relative placement.
    4.  **Control "Panels" (Optional but Recommended):** Consider introducing a concept of "Panels" or "Containers" as describable UI elements (`PlatformCommand::CreatePanel`). Other controls could then be parented to these panels, and layout rules applied within panels. This simplifies complex layouts.
*   **Rationale:**
    *   Moves layout policy out of the `platform_layer` (View) and into the `ui_description_layer` (View Definition).
    *   Makes the `platform_layer`'s `handle_wm_size` truly generic and driven by descriptive data.
    *   Aligns with MVP by having the View (`platform_layer`) responsible for rendering based on instructions, not deciding layout policy itself.
*   **Verification:**
    *   UI resizes correctly according to the new descriptive layout rules.
    *   `handle_wm_size` in `platform_layer` is significantly simplified and generic.
    *   Code review confirms no application-specific layout logic remains in `platform_layer`.

### Step A.2: Review and Remove Residual UI-Specific Knowledge from `platform_layer` (View)

*   **Files:** Primarily `src/platform_layer/app.rs` and `src/platform_layer/window_common.rs`.
*   **Action:**
    *   Conduct a thorough review of the entire `platform_layer` for any remaining hardcoded assumptions about specific UI elements or application behavior that should ideally be driven by `PlatformCommand`s from the `ui_description_layer` or state changes from `app_logic` (Presenter).
    *   Examples:
        *   Are there any control IDs (other than for generic dialog components like `IDOK`) still hardcoded for specific behaviors within the platform layer?
        *   Does the platform layer make assumptions about which controls *must* exist?
        *   Any styling or default text/behavior not set via a command? (Status bar initial text is a good example of what *was* moved).
    *   For each identified instance, determine if it can be parameterized or controlled via a new/modified `PlatformCommand` or if it's a truly generic platform behavior.
*   **Rationale:** Ensures the `platform_layer` becomes as generic and reusable as possible, a core tenet of making it a "pure" View in MVP. The `platform_layer` should only know *how* to draw/manage generic UI components, not *what* specific application components exist or how they relate beyond structural descriptions.
*   **Verification:** Code review and testing to confirm that changes maintain functionality while improving generality.

### Step A.3: Explore Advanced Layout Controls (Inspiration from XAML/WPF)

*   **Goal:** Investigate and potentially implement foundational support for more declarative and flexible layout management, drawing inspiration from systems like WPF's XAML (e.g., `Grid`, `StackPanel`, `DockPanel`). This is a longer-term extension of Step A.1.
*   **Action (High-Level Ideas):**
    1.  **Define Layout Panel Types:** Introduce `PlatformCommand`s to create different types of layout panels (e.g., `CreateStackPanelCommand`, `CreateGridCommand`). These panels would be UI elements themselves.
    2.  **Panel-Specific Properties:** Allow these panel commands to take properties (e.g., `Orientation` for `StackPanel`, `RowDefinitions`/`ColumnDefinitions` for `Grid`).
    3.  **Child Control Attachment:** Allow other controls to be "children" of these panels, with panel-specific attached properties (e.g., `Grid.Row`, `Grid.Column`, `DockPanel.DockEdge`).
    4.  **Layout Engine Enhancement:** The `platform_layer`'s layout engine (within `handle_wm_size` or a dedicated layout manager) would need to understand how to interpret these panel types and their children's layout properties.
*   **Rationale:**
    *   Provides a much more powerful and flexible way to define UIs compared to manual coordinate calculations or simple docking.
    *   Further decouples UI design from imperative code.
    *   Paves the way for potentially loading UI descriptions from external files (like XAML) in the distant future.
*   **Verification:** Implementation of one or two simple layout panel types (e.g., a basic `StackPanel` or `DockPanel`) and demonstration that child controls are arranged correctly within them and respond to window resizing.
*   **Note:** This is an ambitious step and might be broken down into smaller sub-phases. The initial focus should be on the descriptive capabilities needed for the current SourcePacker UI.

---

## Phase B: Change `platform_layer` into a separate crate
