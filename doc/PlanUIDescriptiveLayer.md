# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step. We want the platform_layer to be independent of the actual application UI. The goal is to eventually break it out into a separate library that any application can use.

Whenever you want to change how your window looks like or population of controls, you should never need to change the platform_layer.

---

## Phase A: MVP Refinements, Platform Layer Restructuring, and Generic Layout

**Goal:** Further refine the separation of concerns according to MVP principles, making the `platform_layer` a truly generic View. This involves:
1.  Ensuring all UI-specific knowledge (including layout definitions and application-specific control IDs) resides in the `ui_description_layer` (View definition) or `app_logic` (Presenter).
2.  Enhancing the `platform_layer`'s layout engine to support dynamic and proportional sizing driven by generic `LayoutRule`s.
3.  Generalizing `platform_layer` commands for controls (like TreeView) to be targetable by logical `control_id`s.
4.  Restructuring `platform_layer` code for better maintainability (e.g., by migrating control-specific logic to dedicated handlers).
The `ui_description_layer` must fully define the UI structure and layout via `PlatformCommand`s, and `platform_layer` must become UI-agnostic, processing these commands generically.

**Sub-Phase A.I: Initial Command-Driven UI Creation (If not fully complete)**
    *   *(Existing steps, ensure `CreateStatusBar` is part of this if it wasn't fully command-driven yet, though it will be replaced soon by panel and labels).*

**Sub-Phase A.II: Implementing Generic Layout, Controls, and Handlers**

**Step A.II.1: Enhance `PlatformCommand::DefineLayout` for Proportional Sizing and Solidify Generic Layout Engine**
    *   **Action a (Platform Layer):**
        1.  Extend `platform_layer::types::LayoutRule` to support proportional sizing. For example, add `weight: Option<f32>` or `size_proportion: Option<f32>` applicable when a control is docked within a parent that supports proportional distribution of space (e.g., a parent panel whose own layout rule allows its children to be distributed).
        2.  Modify `platform_layer::command_executor::execute_define_layout` to store these enhanced rules.
        3.  Significantly refactor `platform_layer::window_common::handle_wm_size` to *exclusively* use the stored `LayoutRule`s.
            *   **Crucially, remove all old hardcoded layout logic and fallback paths from `handle_wm_size`.** The `platform_layer` must assume `ui_description_layer` provides complete and sufficient rules for all managed controls.
            *   The layout engine in `handle_wm_size` must now correctly interpret and apply the new proportional sizing rules (e.g., for children of a panel, based on their weights/proportions relative to sibling controls within that panel), alongside existing `DockStyle` and `fixed_size` rules.
    *   **Action b (UI Description Layer):**
        1.  Modify `ui_description_layer::build_main_window_static_layout` to generate `LayoutRule`s that utilize the new proportional sizing capabilities, especially for the status bar label segments within their parent panel.
        2.  Ensure `ui_description_layer` provides a *complete* set of `LayoutRule`s for all controls it defines. Constants like `STATUS_BAR_HEIGHT` (if used by `ui_description_layer` for a fixed-height status bar *area/panel*) are defined and used solely within `ui_description_layer`.
    *   *Verification:* Compiles. Main window layout is entirely driven by `DefineLayout` commands. `handle_wm_size` in `platform_layer` is generic and contains no application-specific layout code or fallbacks. Status bar segments (labels) are laid out proportionally within their parent panel, according to rules originating from `ui_description_layer`.

**Step A.II.2: Introduce Generic Control Creation (e.g., Static Text/Labels)**
    *   **Action a (Platform Layer - Types):** Define a new `PlatformCommand::CreateControl { window_id, parent_control_id: Option<i32>, control_id: i32, control_type: ControlType, initial_properties: ControlProperties }` or specific commands like `PlatformCommand::CreateLabel { window_id, parent_panel_id: i32, label_id: i32, initial_text: String }`.
    *   **Action b (Platform Layer - Implementation):** Implement the executor function(s) in `command_executor.rs` (e.g., `execute_create_label`) to create the native control (e.g., `WC_STATIC` for labels) and store its `HWND` in `NativeWindowData::controls` mapped by the provided `control_id`.
    *   **Action c (UI Description Layer):** Modify `ui_description_layer::build_main_window_static_layout`:
        *   Remove any old direct status bar creation command (e.g., `PlatformCommand::CreateStatusBar`).
        *   Instead, generate `CreatePanel` for the status bar area, then multiple `CreateLabel` (or generic `CreateControl`) commands for each status bar segment (e.g., "Tokens", "General", "Archive").
        *   Assign unique `control_id`s to each, ensuring these IDs are defined in and imported from `app_logic::ui_constants` (e.g., `ui_constants::STATUS_LABEL_TOKENS_ID`, `ui_constants::STATUS_LABEL_GENERAL_ID`).
    *   **Action d (UI Description Layer):** Update `PlatformCommand::DefineLayout` rules in `ui_description_layer` to position the status bar *panel* and then, using the new proportional layout rules from Step A.II.1, define how the label segments are laid out *within* that panel.
    *   *Verification:* Compiles. The main window appears. The status bar consists of a panel containing multiple label controls. Their initial text is set, and their layout within the panel is governed by `DefineLayout` rules (potentially proportional).

**Step A.II.3: Generic Control Update Mechanism (e.g., UpdateControlText)**
    *   **Action a (Platform Layer - Types):** Define a new `PlatformCommand::UpdateControlText { window_id, control_id, text, severity }`. This will replace any specific status bar update commands.
    *   **Action b (Platform Layer - Implementation):** Implement `command_executor::execute_update_control_text`. This function will:
        1.  Find the `HWND` for `control_id` in `NativeWindowData::controls`.
        2.  Call `SetWindowTextW`.
        3.  Store the `severity` associated with this `control_id` in `NativeWindowData::label_severities` (or similar map).
        4.  Call `InvalidateRect` to trigger a repaint (and thus `WM_CTLCOLORSTATIC`).
    *   **Action c (App Logic):** Modify `app_logic::MyAppLogic` to use `UpdateControlText` for all status updates, targeting the specific segment IDs (from `app_logic::ui_constants`).
    *   **Action d (Platform Layer - WndProc):** Modify `window_common::handle_window_message` (specifically `WM_CTLCOLORSTATIC` handler):
        *   It should use `NativeWindowData::label_severities` to look up the severity for the `hwnd_static_ctrl_from_msg` and set text color accordingly. This logic should be generic.
    *   *Verification:* Compiles. Status bar label segments now update their text and color independently based on generic `UpdateControlText` commands from `MyAppLogic`.

**Step A.II.4: Generic TreeView Control Commands**
    *   **Action a (Platform Layer - Types):**
        1.  Modify `platform_layer::types::PlatformCommand::CreateTreeView` to include `control_id: i32`.
        2.  Modify other TreeView-related commands (`PopulateTreeView`, `UpdateTreeItemVisualState`, `RedrawTreeItem`) to include the target `control_id: i32`.
    *   **Action b (Platform Layer - Implementation):**
        1.  Update `platform_layer::command_executor::execute_create_treeview` to use the provided `control_id` when creating and storing the TreeView's `HWND` in `NativeWindowData::controls`.
        2.  Update functions in `platform_layer::control_treeview` (e.g., `populate_treeview`, `update_treeview_item_visual_state`) to accept `control_id` and use it to retrieve the correct TreeView `HWND`.
    *   **Action c (UI Description Layer):**
        1.  Modify `ui_description_layer::build_main_window_static_layout` to generate `PlatformCommand::CreateTreeView { ..., control_id: ui_constants::MAIN_TREEVIEW_ID }` (where `MAIN_TREEVIEW_ID` is defined in `app_logic::ui_constants`).
    *   **Action d (App Logic):**
        1.  Modify `app_logic::MyAppLogic` to use the `control_id` (e.g., `ui_constants::MAIN_TREEVIEW_ID`) when issuing commands like `PopulateTreeView`.
    *   *Verification:* Compiles. TreeView is created and functions correctly, targeted by its logical ID specified by `ui_description_layer` and used by `app_logic`.

**Step A.II.5: Create `controls` Sub-Module and Handler Skeletons**
    *   **Action a:** Create `src/platform_layer/controls/` directory and `mod.rs`.
    *   **Action b:** Create empty or skeleton `menu_handler.rs`, `button_handler.rs`, `label_handler.rs` (for status segments and other static labels), etc., in the new `controls` module.
    *   **Action c:** Rename `control_treeview.rs` to `src/platform_layer/controls/treeview_handler.rs` and update references.
    *   *Verification:* Compiles. App runs as before. Project structure for control handlers is in place.

**Step A.II.6: Migrate Control-Specific Logic to Handlers (Iteratively)**
    *   **For each control type (e.g., Button, then Menu, then Label, then TreeView):**
        *   **Action a (Command Handling):**
            1.  Identify the function in `command_executor.rs` that handles its creation (e.g., `execute_create_button`, `execute_create_label`).
            2.  Move this function's implementation into the appropriate `*_handler.rs` (e.g., `button_handler::handle_create_button_command`, `label_handler::handle_create_label_command`).
            3.  Update `command_executor.rs` (or `Win32ApiInternalState::_execute_platform_command` directly) to call the new handler function.
            *   *Verification:* Compiles. The specific control is still created correctly. App functional.
        *   **Action b (Notification/Message Handling):**
            1.  Identify the parts of `Win32ApiInternalState::handle_window_message` (e.g., `WM_COMMAND` for buttons, `WM_CTLCOLORSTATIC` for labels, `WM_NOTIFY` for TreeView) in `window_common.rs` that deal with this specific control's events or drawing.
            2.  Move this logic into a new function within the control's `*_handler.rs` (e.g., `button_handler::handle_wm_command(...)`, `label_handler::handle_wm_ctlcolorstatic(...)`). This function will take necessary parameters (like `Win32ApiInternalState`, `HWND` of control, message params).
            3.  Modify the relevant message handlers in `window_common.rs` to call this new handler function from the control's module.
            *   *Verification:* Compiles. Events/drawing for that specific control are still processed correctly by its dedicated handler. App functional.
    *   **Note:** The new `label_handler.rs` will be particularly relevant for the status bar segments, handling their creation and `WM_CTLCOLORSTATIC` logic.

---

**Step A.III.1: Review and Remove Residual UI-Specific Knowledge (A.5)**
    *   **Action:** After all control logic is migrated, perform a thorough review of `platform_layer` (especially `window_common.rs` and `command_executor.rs`) to ensure no SourcePacker-specific UI logic, layout assumptions, or control ID knowledge remains. All such specifics should be driven by `PlatformCommand`s generated by `ui_description_layer` or decisions made in `app_logic`.
    *   *Verification:* Code review confirms increased generality. `platform_layer` acts as a generic command executor and event translator. App functional.

**Step A.III.2: Future Exploration: Advanced Layout and Deeper Decomposition (A.6)**
    *   This remains a longer-term goal. Consider advanced layout managers (e.g., grid, stack panels) as generic offerings within `platform_layer`, configurable by `ui_description_layer` via `PlatformCommand`s.
    *   Evaluate if `NativeWindowData` itself can be made more generic or if control-specific state can be fully encapsulated within their respective handlers.

---

## Phase B: Change `platform_layer` into a separate crate
