# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step.

---

## Phase 1: Migrating Menu Creation

Completed changes have been removed.

### Step 1.3: Integrate Menu Creation via Command in `main.rs`

*   **File:** `src/main.rs`
*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_create`)
*   **Action (`main.rs`):**
    1.  After `platform_interface.create_window()` successfully returns a `main_window_id`.
    2.  Call `ui_description_layer::describe_main_window_layout(main_window_id)`.
    3.  Iterate through the *menu-related commands only* (for now) from the returned Vec.
    4.  For each, call `platform_interface.execute_command()`.
*   **Action (`window_common.rs`):**
    1.  In `Win32ApiInternalState::handle_wm_create`, **remove** the direct call to `create_app_menu(hwnd)`.
*   **Rationale:** Shifts menu creation from implicit `WM_CREATE` to explicit command execution driven by `main.rs` using the `ui_description_layer`.
*   **Verification:** Application runs, and the main menu is present and functional, created via the new command flow.

---

## Phase 2: Migrating Button Creation

### Step 2.1: Implement `CreateButton` Command Handler in `platform_layer`

*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::_execute_platform_command`)
*   **Action:**
    *   Add a match arm for `PlatformCommand::CreateButton`.
    *   The handler logic will use `CreateWindowExW` with `WC_BUTTON`.
    *   It will take `window_id`, `control_id` (logical), `text`, and potentially initial rect from the command.
    *   Store the created button's `HWND` in `NativeWindowData` (e.g., in a `HashMap<i32, HWND>` for generic controls, or update `hwnd_button_generate` if keeping specific field).
    *   The existing `WM_SIZE` logic will need to be adapted to find the button's HWND from `NativeWindowData` rather than assuming `hwnd_button_generate` is set.
*   **Rationale:** Enables platform layer to create buttons via commands.
*   **Verification:** New command handler compiles. App still uses old button creation.

### Step 2.2: Update `ui_description_layer` for Button

*   **File:** `src/ui_description_layer/mod.rs`
*   **Action:**
    *   Modify `describe_main_window_layout` to also generate a `PlatformCommand::CreateButton` for the "Generate Archive" button, using `ID_BUTTON_GENERATE_ARCHIVE`.
*   **Rationale:** New layer describes the button.
*   **Verification:** `describe_main_window_layout` produces the button command. App still uses old button creation.

### Step 2.3: Integrate Button Creation via Command

*   **File:** `src/main.rs`
*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_create`)
*   **Action (`main.rs`):**
    *   In the loop after `create_window`, also process the `CreateButton` command.
*   **Action (`window_common.rs`):**
    *   In `Win32ApiInternalState::handle_wm_create`, **remove** the direct `CreateWindowExW` call for the "Generate Archive" button.
*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_size`)
*   **Action (`window_common.rs` - WM_SIZE):**
    *   Modify `handle_wm_size` to get the button's HWND from `NativeWindowData` (e.g., `window_data.get_control_hwnd(ID_BUTTON_GENERATE_ARCHIVE)`) instead of directly from `window_data.hwnd_button_generate`. This might involve adding a small helper or changing how `hwnd_button_generate` is accessed/populated.
*   **Rationale:** Shifts button creation to be command-driven.
*   **Verification:** Application runs, "Generate Archive" button is present, correctly positioned by `WM_SIZE`, and functional.

---

## Phase 3: Migrating Status Bar Creation

### Step 3.1: Implement `CreateStatusBar` Command Handler in `platform_layer`

*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::_execute_platform_command`)
*   **Action:**
    *   Add a match arm for `PlatformCommand::CreateStatusBar`.
    *   Handler uses `CreateWindowExW` with `WC_STATIC`.
    *   Takes `window_id`, `control_id` (logical `ID_STATUS_BAR_CTRL`), `initial_text`.
    *   Stores `HWND` in `NativeWindowData` (e.g., update `hwnd_status_bar`).
    *   `WM_SIZE` logic will need adaptation similar to the button.
    *   `WM_CTLCOLORSTATIC` will also need to find the status bar HWND from `NativeWindowData`.
*   **Rationale:** Enables platform layer to create status bar via command.
*   **Verification:** New command handler compiles. App still uses old status bar creation.

### Step 3.2: Update `ui_description_layer` for Status Bar

*   **File:** `src/ui_description_layer/mod.rs`
*   **Action:**
    *   Modify `describe_main_window_layout` to also generate a `PlatformCommand::CreateStatusBar` using `ID_STATUS_BAR_CTRL` and "Ready" as initial text.
*   **Rationale:** New layer describes the status bar.
*   **Verification:** `describe_main_window_layout` produces the status bar command. App still uses old status bar creation.

### Step 3.3: Integrate Status Bar Creation via Command

*   **File:** `src/main.rs`
*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_create`)
*   **Action (`main.rs`):**
    *   Process the `CreateStatusBar` command.
*   **Action (`window_common.rs` - WM_CREATE):**
    *   In `Win32ApiInternalState::handle_wm_create`, **remove** the direct `CreateWindowExW` call for the status bar.
*   **File:** `src/platform_layer/window_common.rs` (`Win32ApiInternalState::handle_wm_size`)
*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::handle_window_message` for `WM_CTLCOLORSTATIC`)
*   **Action (`window_common.rs` / `app.rs`):**
    *   Modify `handle_wm_size` and the `WM_CTLCOLORSTATIC` handler to get the status bar's HWND from `NativeWindowData` instead of `window_data.hwnd_status_bar` directly.
*   **Rationale:** Shifts status bar creation to be command-driven.
*   **Verification:** Application runs, status bar is present, correctly positioned, displays initial text, and updates colors correctly.

---

## Phase 4: TreeView Creation (Consideration)

*   **Current State:** The TreeView is created on-demand by `control_treeview::ensure_treeview_exists_and_get_state` when `PlatformCommand::PopulateTreeView` is first processed. This is already somewhat command-driven.
*   **Optional Step 4.1: Explicit `CreateTreeView` Command**
    *   **Action:**
        1.  Define `PlatformCommand::CreateTreeView { window_id, control_id /* ...other configs if needed */ }`.
        2.  `ui_description_layer` generates this command.
        3.  `main.rs` executes it *before* any `PopulateTreeView` can occur.
        4.  The handler for `CreateTreeView` in `platform_layer` would explicitly create the TreeView (similar to `ensure_treeview_exists_and_get_state` but without the "ensure" part, just "create"). It would store its HWND and `TreeViewInternalState` in `NativeWindowData`.
        5.  `PopulateTreeView` would then *assume* the TreeView HWND exists.
    *   **Rationale:** Makes the creation of *all* primary UI elements explicit and command-driven from the start. Useful if you need to configure TreeView styles or other properties *before* it's populated.
    *   **Verification:** TreeView is created and populated correctly.
*   **Alternative:** Keep the current on-demand creation via `PopulateTreeView` if explicit pre-population configuration isn't immediately needed. The current approach is already quite decoupled.

---

## Phase 5: Generalizing Control Storage and Access in `NativeWindowData`

### Step 5.1: Refactor `NativeWindowData` for Generic Control Storage

*   **File:** `src/platform_layer/window_common.rs`
*   **Action:**
    *   Change `NativeWindowData` to store control HWNDs in a `HashMap<i32, HWND>` where the key is the logical `control_id`.
    *   Remove specific fields like `hwnd_button_generate`, `hwnd_status_bar`.
    *   Update all places that accessed these specific fields (e.g., `WM_SIZE`, `WM_COMMAND`, `WM_CTLCOLORSTATIC`, status bar updates) to use the HashMap lookup: `window_data.controls.get(&ID_BUTTON_GENERATE_ARCHIVE)`.
*   **Rationale:** Makes the `platform_layer` more generic, as it no longer has hardcoded knowledge of specific button or status bar fields. It just knows about controls identified by an `i32` ID.
*   **Verification:** All UI elements (button, status bar) still function correctly and are laid out properly.

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

## Phase 8: Change `platform_layer` into a separate crate
