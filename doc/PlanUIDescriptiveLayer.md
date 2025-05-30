# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step. We want the platform_layer to be independent of the actual application UI. The goal is to eventually break it out into a separate library that any application can use.

Whenever you want to change how your window looks like or population of controls, you should never need to change the platform_layer.

---

## Phase A: MVP Refinements, Platform Layer Restructuring, and Generic Layout

**Goal:** Further refine the separation of concerns according to MVP principles, focusing on making the `platform_layer` a truly generic View. This involves ensuring all UI-specific knowledge (beyond what's needed to render generic controls and translate events) resides in the `ui_description_layer` (View definition) or `app_logic` (Presenter). This phase also focuses on restructuring the `platform_layer` code (primarily `app.rs` and `window_common.rs`) for better maintainability and implements a generic layout mechanism.

**Sub-Phase A.I: Initial Command-Driven UI Creation (If not fully complete)**
    *   *(Existing steps, ensure `CreateStatusBar` is part of this if it wasn't fully command-driven yet, though it will be replaced soon).*

**Sub-Phase A.II: Implementing Generic Layout & Control Handler Modules**

**Step A.II.1: Implement Generic Layout Command `PlatformCommand::DefineLayout`**
    *   *(Existing step)*
    *   **Addition:** When designing the `DefineLayout` executor (`execute_define_layout` in `command_executor.rs` and its use in `handle_wm_size`), consider how it will arrange multiple horizontal segments for the new status bar. You might need:
        *   A way to specify a fixed width for some segments and have one segment "fill" the rest.
        *   Or, allow `LayoutRule` to specify a `width_proportion` or `weight` for horizontal distribution within a parent area (the status bar's bottom-docked region).
        *   A simple start: Dock segments left, use `fixed_size` for width, and `margin.left` to position them sequentially. One segment can `DockStyle::Fill` to take remaining space.

**Step A.II.1.5: Introduce Generic Static Text Control Creation**
    *   **Action a:** Define a new `PlatformCommand::CreateStaticText { window_id, control_id, initial_text, ... }` (or a more generic `PlatformCommand::CreateControl { ..., control_type: ControlType::StaticText, ... }`).
    *   **Action b:** Implement `command_executor::execute_create_static_text` to create a `WC_STATIC` control. This will be used for the status bar segments.
    *   **Action c:** Modify `ui_description_layer::describe_main_window_layout`:
        *   Remove the old `PlatformCommand::CreateStatusBar`.
        *   Instead, generate multiple `CreateStaticText` commands for each status bar segment (e.g., "Tokens", "Files", "Operation Result", and separators if they are also static text controls). Assign unique `control_id`s to each (e.g., `ID_STATUS_SEGMENT_TOKENS`, `ID_STATUS_SEGMENT_SEPARATOR_1`, etc. - define these constants).
    *   **Action d:** Update `PlatformCommand::DefineLayout` rules in `ui_description_layer` to position these new static text segments horizontally within the status bar area at the bottom of the window.
    *   *Verification:* Compiles. The main window appears. Instead of one status bar, you see multiple static text controls at the bottom, likely unstyled and un-updated initially. Layout might be preliminary.

**Step A.II.1.6: Generic Control Update Mechanism**
    *   **Action a:** Define a new `PlatformCommand::UpdateControlText { window_id, control_id, text, severity }`. This will replace the specific `UpdateStatusBarText`.
    *   **Action b:** Implement `command_executor::execute_update_control_text`. This function will:
        1.  Find the `HWND` for `control_id` in `NativeWindowData::controls`.
        2.  Call `SetWindowTextW`.
        3.  Store the `severity` associated with this `control_id` (perhaps in a new `HashMap<i32, MessageSeverity>` in `NativeWindowData` or by extending `NativeWindowData::controls` to store more than just `HWND`). This is for `WM_CTLCOLORSTATIC`.
        4.  Call `InvalidateRect` to trigger a repaint (and thus `WM_CTLCOLORSTATIC`).
    *   **Action c:** Modify `app_logic::MyAppLogic` to use `UpdateControlText` for status updates, targeting the specific segment IDs.
    *   **Action d:** Modify `window_common::handle_window_message` (specifically `WM_CTLCOLORSTATIC` handler):
        *   It should now iterate through the `controls` or the new severity map in `NativeWindowData`.
        *   If the `hwnd_static_ctrl_from_msg` matches a known status segment's `HWND`, use its stored severity to set the text color.
    *   *Verification:* Compiles. Status bar segments now update their text and color independently based on commands from `MyAppLogic`. `MyAppLogic` can now, for example, show "Tokens: 123" in normal color and "Error: File not found" in red, simultaneously in different segments.

**Step A.II.2: Remove Old Hardcoded Layout from `handle_wm_size`**
    *   **Action:** Once Step A.II.1.6 is verified (meaning the new status segments are laid out by `DefineLayout` and update correctly), proceed with removing old hardcoded layout logic. The constant `STATUS_BAR_HEIGHT` will still be used by `ui_description_layer` when defining the `fixed_size` for the status bar *area* or for individual segments if they have fixed heights.
    *   *Verification:* Compiles. App runs. Layout is correct and entirely driven by `PlatformCommand::DefineLayout`. `handle_wm_size` is now generic. The status bar consists of multiple segments correctly positioned.

**Step A.II.3: Create `controls` Sub-Module and `*_handler.rs` Skeletons**
    *   **Action a:** Create `src/platform_layer/controls/` directory and `mod.rs`.
    *   **Action b:** Create empty or skeleton `menu_handler.rs`, `button_handler.rs`, `static_text_handler.rs` (for status segments and other static labels), etc.
    *   **Action c:** Rename `control_treeview.rs` to `src/platform_layer/controls/treeview_handler.rs` and update references.
    *   *Verification:* Compiles. App runs as before.

**Step A.II.4: Migrate Control Logic to Handlers (Iteratively, one control type at a time)**
    *   **For each control type (e.g., Button, then Menu, then StaticText, then TreeView):**
        *   **Action a (Command Handling):**
            1.  Identify the function in `command_executor.rs` that handles its creation (e.g., `execute_create_button`, `execute_create_static_text`).
            2.  Move this function's implementation into the appropriate `*_handler.rs` (e.g., `button_handler::handle_create_button_command`, `static_text_handler::handle_create_static_text_command`).
            3.  Update `command_executor.rs` (or `_execute_platform_command` directly) to call the new handler function.
            *   *Verification:* Compiles. The specific control is still created correctly. App functional.
        *   **Action b (Notification/Message Handling):**
            1.  Identify the parts of `Win32ApiInternalState::handle_wm_command`, `handle_wm_notify`, or `handle_wm_ctlcolorstatic` (in `window_common.rs`) that deal with this specific control's events/drawing.
            2.  Move this logic into a new function within the control's `*_handler.rs` (e.g., `button_handler::handle_wm_command(...)`, `static_text_handler::handle_wm_ctlcolorstatic(...)`). This function will take necessary parameters.
            3.  Modify the relevant message handlers in `window_common.rs` to call this new handler function from the control's module.
            *   *Verification:* Compiles. Events/drawing for that specific control are still processed correctly. App functional.
    *   **Note:** The new `static_text_handler.rs` will be particularly relevant for the status bar segments. It would handle their creation and potentially parts of `WM_CTLCOLORSTATIC` if you choose to centralize that logic per control type.

---

**Step A.III.1: Review and Remove Residual UI-Specific Knowledge (A.5)**
    *   **Action:** After all control logic is migrated, perform the thorough review as described previously.
    *   *Verification:* Code review confirms increased generality. App functional.

**Step A.III.2: Future Exploration: Advanced Layout and Deeper Decomposition (A.6)**
    *   This remains a longer-term goal, to be approached after the core refactoring is stable. The "fully functional after each minor step" applies less directly here as it's about new features or major architectural shifts.


---

## Phase B: Change `platform_layer` into a separate crate
