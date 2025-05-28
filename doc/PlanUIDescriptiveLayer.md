# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step. We want the platform_layer to be independent of the actual application UI. The goal is to eventually break it out into a separate library that any application can use.

Whenever you want to change how your window loks like or population of controls, you should never need to change the platform_layer.

---

Completed changes have been removed.

## Phase 2: Migrating Button Creation

Completed.

## Phase 3: Migrating Status Bar Creation

Completed.

---

## Phase 4: TreeView Creation (Consideration)

Completed
---

## Phase 5: Generalizing Control Storage and Access in `NativeWindowData`

Completed
---

## Phase 6: Cleanup and Review

### Step 6.1: Review `WM_CREATE`

*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_create`)
*   **Action:** Ensure `handle_wm_create` is now very minimal. It should primarily be concerned with setup related to the main window frame itself if anything, not creating child controls.
*   **Rationale:** Confirms the shift of responsibility.
*   **Verification:** Code review.

### Step 6.2: Review `main.rs` Orchestration

*   **Action:** Ensure the sequence of operations in `main.rs` is logical:
    1.  Create `PlatformInterface`, `MyAppLogic`, `UiDescriptionLayer`.
    2.  Call `platform_interface.create_window()` for the main window frame.
    3.  Get UI structure commands from `UiDescriptionLayer`.
    4.  Execute these structure commands via `platform_interface.execute_command()`.
    5.  Call `my_app_logic.on_main_window_created()` (which will now enqueue commands for *data* population and visibility, not structure).
    6.  Start `platform_interface.run()`.
*   **Rationale:** Ensures correct application initialization flow.
*   **Verification:** Code review and functional testing.

### Step 6.3: Update Documentation and Comments

*   **Action:** Update comments in relevant modules (`platform_layer`, `ui_description_layer`, `main.rs`) to reflect the new architecture for UI creation.
*   **Rationale:** Keeps documentation in sync with code.

---

## Phase 7: Abstracting Menu Item Identifiers

**Goal:** Remove the direct dependency on `i32` control IDs for menu items from the `ui_description_layer` and `app_logic`. Instead, use semantic identifiers (e.g., enums or strings) in the UI description, and have the `platform_layer` dynamically assign and manage the native `i32` IDs.

### Step 7.1: Define Semantic Menu Action Identifiers

*   **File:** `src/platform_layer/types.rs` (or a new shared types module if preferred)
*   **Action:**
    *   Define an enum, e.g., `MenuAction`, to represent logical menu actions.
        ```rust
        // Example in types.rs
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum MenuAction {
            LoadProfile,
            SaveProfileAs,
            SetArchivePath,
            RefreshFileList,
            // ... any other distinct menu actions
        }
        ```
    *   Alternatively, decide on a string-based convention (e.g., "file.load_profile"). Enums are generally more type-safe.
*   **Rationale:** Establishes a platform-agnostic way to identify menu actions.
*   **Verification:** Code compiles. No functional change yet.

### Step 7.2: Update `MenuItemConfig` to Use Semantic Identifiers

*   **File:** `src/platform_layer/types.rs`
*   **Action:**
    *   Modify `MenuItemConfig` to use the new `MenuAction` (or string identifier) instead of `id: i32`.
        ```rust
        // In types.rs, MenuItemConfig struct:
        pub struct MenuItemConfig {
            // pub id: i32, // Remove this
            pub action: Option<MenuAction>, // Make it Option an if some menu items (like "&File") don't have actions
            pub text: String,
            pub children: Vec<MenuItemConfig>,
        }
        ```
*   **Rationale:** `MenuItemConfig` now describes menu items semantically.
*   **Verification:** Code compiles. `ui_description_layer` will need updates.

### Step 7.3: Update `ui_description_layer` to Use Semantic Identifiers

*   **File:** `src/ui_description_layer/mod.rs`
*   **Action:**
    *   Modify `describe_main_window_layout` to populate `MenuItemConfig` with `MenuAction` values instead of `i32` IDs.
    *   Remove any local or imported `i32` menu ID constants.
*   **Rationale:** `ui_description_layer` is now free of `i32` menu IDs.
*   **Verification:** `describe_main_window_layout` produces commands with the new `MenuItemConfig`. App will break until platform layer is updated.

### Step 7.4: Modify `platform_layer` to Manage `i32` ID Assignment and Mapping

*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::_handle_create_main_menu_impl`, `add_menu_item_recursive`)
*   **File:** `src/platform_layer/window_common.rs` (`NativeWindowData`)
*   **Action:**
    *   In `NativeWindowData`, add a `HashMap<i32, MenuAction>` to store the mapping from dynamically generated `i32` IDs to `MenuAction`s.
    *   In `_handle_create_main_menu_impl` / `add_menu_item_recursive`:
        *   When creating a menu item, if `MenuItemConfig.action` is `Some(action)`, generate a unique `i32` ID (e.g., from a counter).
        *   Store this mapping (`generated_i32_id -> action`) in `NativeWindowData.menu_action_map`.
        *   Use the `generated_i32_id` when calling `AppendMenuW`.
*   **Rationale:** `platform_layer` now handles the translation from semantic actions to native IDs.
*   **Verification:** Menu is created. `WM_COMMAND` handling for menus will be broken.

### Step 7.5: Update `WM_COMMAND` Handling for Menus

*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_command`)
*   **Action:**
    *   When a `WM_COMMAND` is received for a menu item:
        *   Use the `i32` `control_id` from `wparam` to look up the `MenuAction` in `NativeWindowData.menu_action_map`.
        *   If found, create a new `AppEvent` variant, e.g., `AppEvent::MenuActionClicked { window_id, action: MenuAction }`.
        *   Send this new event to `app_logic`.
*   **File:** `src/platform_layer/types.rs` (for `AppEvent`)
*   **File:** `src/app_logic/handler.rs` (to handle the new `AppEvent`)
*   **Action (`types.rs`):** Add `MenuActionClicked { window_id, action: MenuAction }` to `AppEvent` enum. Remove old menu-specific `AppEvent`s if they become redundant (e.g. `MenuLoadProfileClicked`).
*   **Action (`app_logic/handler.rs`):** Update `handle_event` to match on the new `AppEvent::MenuActionClicked` and dispatch based on the `MenuAction` enum.
*   **Rationale:** Event handling in `app_logic` is now based on semantic actions.
*   **Verification:** Application menu items are functional again, using the new semantic event flow.

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

## Phase A: Clean-ups and additional ideas
*   handle_wm_size seems to be hard coded, knowing what controls there are.
*   There is still considerablel knowlede and dependencies in the platform layer to the UI content. The goal is to remove these, and have them managed by the ui_description_layer.
*   Add support for typical layout controls, based on inspiration from WPS xaml.

---

## Phase B: Change `platform_layer` into a separate crate
