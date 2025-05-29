# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step. We want the platform_layer to be independent of the actual application UI. The goal is to eventually break it out into a separate library that any application can use.

Whenever you want to change how your window looks like or population of controls, you should never need to change the platform_layer.

---

## Phase A: MVP Refinements, Platform Layer Restructuring, and Generic Layout

**Goal:** Further refine the separation of concerns according to MVP principles, focusing on making the `platform_layer` a truly generic View. This involves ensuring all UI-specific knowledge (beyond what's needed to render generic controls and translate events) resides in the `ui_description_layer` (View definition) or `app_logic` (Presenter). This phase also focuses on restructuring the `platform_layer` code (primarily `app.rs` and `window_common.rs`) for better maintainability and implements a generic layout mechanism.

**Sub-Phase A.I: Laying Groundwork for Generic Layout & Initial Restructuring**

**Step A.I.3: Implement `DefineLayout` Command Handler (Storing Rules)**
    *   **Action a:** Create the basic structure of `src/platform_layer/command_executor.rs`.
    *   **Action b:** In `command_executor.rs`, implement a function `execute_define_layout(state: &Arc<Win32ApiInternalState>, window_id: WindowId, rules: Vec<LayoutRule>)`. This function will lock `Win32ApiInternalState::window_map`, get the `NativeWindowData` for `window_id`, and store the `rules` in the new `layout_rules` field.
    *   **Action c:** In `Win32ApiInternalState::_execute_platform_command` (in `app.rs`), add a match arm for `PlatformCommand::DefineLayout` that calls the new `execute_define_layout` function.
    *   *Verification:* Compiles. App runs as before. The new command can be processed, but `WM_SIZE` doesn't use the rules yet.

**Step A.I.4: Update `ui_description_layer` to Send `DefineLayout` Commands**
    *   **Action:** Modify `ui_description_layer::describe_main_window_layout` to generate and include `PlatformCommand::DefineLayout` commands for the TreeView, Button, and Status Bar, *alongside* the existing `CreateButton`, `CreateTreeView`, `CreateStatusBar` commands.
    *   *Verification:* Compiles. App runs. The `DefineLayout` commands are sent and processed, storing rules in `NativeWindowData`. Layout still handled by old `WM_SIZE`.

**Step A.I.5: Move Non-Dialog Command Implementations to `command_executor.rs`**
    *   **Action (iterative, one command type at a time):**
        *   For each non-dialog `_handle_..._impl` method in `Win32ApiInternalState` (e.g., `_handle_create_button_impl`, `_handle_create_main_menu_impl`, etc., but *not* `_handle_show_save_file_dialog_impl` yet):
            1.  Move its implementation to a corresponding free function in `command_executor.rs` (e.g., `execute_create_button(...)`).
            2.  Update the `_execute_platform_command` match arm in `app.rs` to call this new function.
    *   *Verification (after each command type moved):* Compiles. App runs, and the specific functionality related to that command still works.

**Step A.I.6: Create `dialog_handler.rs` and Move Dialog Command Implementations**
    *   **Action a:** Create the basic structure of `src/platform_layer/dialog_handler.rs`.
    *   **Action b (iterative, one dialog type at a time):**
        *   For each dialog-related `_handle_..._impl` method in `Win32ApiInternalState` (or if already moved, in `command_executor.rs`):
            1.  Move its implementation (and any helper structs/functions like `_show_common_file_dialog`, `InputDialogData`, etc.) to a corresponding free function in `dialog_handler.rs`.
            2.  Update the `_execute_platform_command` match arm (or the call in `command_executor.rs`) to call this new function from `dialog_handler.rs`.
    *   *Verification (after each dialog type moved):* Compiles. App runs, and dialog functionality still works.

---

**Sub-Phase A.II: Implementing Generic Layout & Control Handler Modules (Steps A.3 & A.4 from previous plan)**

**Step A.II.1: Implement Generic `handle_wm_size` (Side-by-Side or Flagged)**
    *   **Action a:** In `Win32ApiInternalState::handle_wm_size` (`window_common.rs`), add logic to check if `layout_rules` are present in `NativeWindowData`.
    *   **Action b:** If rules are present, implement the new generic layout logic that iterates through controls and applies these rules.
    *   **Action c (Crucial for stability):** Initially, the *old* hardcoded layout logic in `handle_wm_size` should *still run*. You might:
        *   Have the new logic run *after* the old one (it might override positions if controls are the same). This is simpler but could be messy.
        *   Or, introduce a temporary flag (e.g., in `NativeWindowData` or even a global atomic for initial testing) to switch between old and new `WM_SIZE` logic. This is safer.
        *   Or, if the layout rules only target the existing known controls (TreeView, Button, Statusbar), you can carefully replace the old logic for *just those controls* with the new rule-based calculations one by one, ensuring the constants like `BUTTON_AREA_HEIGHT` are only used by the `ui_description_layer` when it generates the rules.
    *   **Goal:** Transition to the new `WM_SIZE` behavior for the existing controls *without breaking the layout*.
    *   *Verification:* Compiles. App runs. The layout of TreeView, Button area, and Status Bar is now determined by the rules sent from `ui_description_layer` and applied by the new generic part of `handle_wm_size`. The old hardcoded positioning for these specific elements is no longer active.

**Step A.II.2: Remove Old Hardcoded Layout from `handle_wm_size`**
    *   **Action:** Once Step A.II.1 is verified, remove the old, hardcoded layout logic for TreeView, Button area, and Status Bar from `handle_wm_size`. The constants like `BUTTON_AREA_HEIGHT` should only be referenced (if needed) by the `ui_description_layer` when it defines the layout rules.
    *   *Verification:* Compiles. App runs. Layout is correct and entirely driven by `PlatformCommand::DefineLayout`. `handle_wm_size` is now generic.

**Step A.II.3: Create `controls` Sub-Module and `*_handler.rs` Skeletons**
    *   **Action a:** Create `src/platform_layer/controls/` directory and `mod.rs`.
    *   **Action b:** Create empty or skeleton `menu_handler.rs`, `button_handler.rs`, `statusbar_handler.rs`.
    *   **Action c:** Rename `control_treeview.rs` to `src/platform_layer/controls/treeview_handler.rs` and update references.
    *   *Verification:* Compiles. App runs as before.

**Step A.II.4: Migrate Control Logic to Handlers (Iteratively, one control type at a time)**
    *   **For each control type (e.g., Button, then Menu, then StatusBar, then TreeView):**
        *   **Action a (Command Handling):**
            1.  Identify the function in `command_executor.rs` that handles its creation (e.g., `execute_create_button`).
            2.  Move this function's implementation into the appropriate `*_handler.rs` (e.g., `button_handler::handle_create_button_command`).
            3.  Update `command_executor.rs` (or `_execute_platform_command` directly) to call the new handler function.
            *   *Verification:* Compiles. The specific control is still created correctly. App functional.
        *   **Action b (Notification/Message Handling):**
            1.  Identify the parts of `Win32ApiInternalState::handle_wm_command` or `handle_wm_notify` (in `window_common.rs`) that deal with this specific control's events.
            2.  Move this logic into a new function within the control's `*_handler.rs` (e.g., `button_handler::handle_wm_command(...)`, `menu_handler::handle_wm_command(...)`). This function will take necessary parameters (like `wparam`, `lparam`, `window_id`, a reference to `Arc<Win32ApiInternalState>`, and a mutable ref to `NativeWindowData`).
            3.  Modify `handle_wm_command`/`handle_wm_notify` in `window_common.rs` to call this new handler function from the control's module if the message pertains to that control.
            *   *Verification:* Compiles. Events for that specific control (e.g., button clicks, menu selections) are still processed correctly. App functional.
    *   **Note:** `NativeWindowData` fields like `menu_action_map`, `treeview_state` will now be primarily accessed and modified by their respective control handlers, though `NativeWindowData` itself still resides in `window_common.rs`.

---

**Sub-Phase A.III: Review and Future Exploration (Step A.5 & A.6 from previous plan)**

**Step A.III.1: Review and Remove Residual UI-Specific Knowledge (A.5)**
    *   **Action:** After all control logic is migrated, perform the thorough review as described previously.
    *   *Verification:* Code review confirms increased generality. App functional.

**Step A.III.2: Future Exploration: Advanced Layout and Deeper Decomposition (A.6)**
    *   This remains a longer-term goal, to be approached after the core refactoring is stable. The "fully functional after each minor step" applies less directly here as it's about new features or major architectural shifts.


---

## Phase B: Change `platform_layer` into a separate crate
