# Refactoring Plan: UI Construction via `ui_description_layer`

This plan outlines the steps to refactor SourcePacker so that the main UI structure is defined by a new `ui_description_layer` which generates `PlatformCommand`s. The `platform_layer` will then execute these commands to create UI elements, rather than creating them implicitly (e.g., in `WM_CREATE`).

**Goal:** Decouple UI structure definition from the platform-specific implementation, improve testability, and pave the way for a more reusable `platform_layer`. The application should remain functional after each major step.

---

## Phase 0: Preparation & Setup

### Step 0.1: Define New `PlatformCommand`s for UI Element Creation

*   **File:** `src/platform_layer/types.rs`
*   **Action:**
    *   Add new variants to the `PlatformCommand` enum for creating basic UI elements.
    *   Define associated structs for configuration if needed (e.g., `ButtonConfig`, `MenuConfig`). Initially, these can be simple.
*   **New Commands (Examples):**
    ```rust
    // In PlatformCommand enum:
    // ...
    CreateMainMenu { // Or just CreateMenu, if only one main menu is typical
        window_id: WindowId,
        menu_items: Vec<MenuItemConfig>, // MenuItemConfig would define text, ID, sub_items
    },
    CreateButton {
        window_id: WindowId,
        control_id: i32, // The existing logical ID
        text: String,
        // Initial position/size can be optional for now, relying on WM_SIZE
        // Or add: x: i32, y: i32, width: i32, height: i32,
    },
    CreateStatusBar { // Or CreateStaticText if it's a generic status display
        window_id: WindowId,
        control_id: i32, // The existing logical ID (ID_STATUS_BAR_CTRL)
        initial_text: String,
        // Initial position/size
    },
    // TreeView is already created on-demand by PopulateTreeView, might not need a separate CreateTreeView initially
    // unless you want to configure its style/position explicitly before first population.
    ```
*   **New Config Structs (Examples):**
    ```rust
    // In types.rs
    #[derive(Debug, Clone)]
    pub struct MenuItemConfig {
        pub id: i32, // Menu item ID
        pub text: String,
        pub children: Vec<MenuItemConfig>, // For submenus
    }
    // ButtonConfig, StatusBarConfig can be added if more complex setup is needed than simple params.
    ```
*   **Rationale:** Establishes the new communication contract between the description layer and the platform layer.
*   **Verification:** Code compiles. No functional change yet.

### Step 0.2: Create the `ui_description_layer` Module

*   **Action:**
    *   Create a new directory `src/ui_description_layer`.
    *   Add `mod.rs` to it.
    *   In `src/main.rs`, add `mod ui_description_layer;`.
    *   In `src/ui_description_layer/mod.rs`, create a public function, e.g., `pub fn describe_main_window_layout(window_id: WindowId) -> Vec<PlatformCommand>`.
    *   Initially, this function can return an empty `Vec`.
*   **Rationale:** Sets up the new module structure.
*   **Verification:** Code compiles. No functional change.

---

## Phase 1: Migrating Menu Creation

### Step 1.1: Implement `CreateMainMenu` Command Handler in `platform_layer`

*   **File:** `src/platform_layer/app.rs` (`Win32ApiInternalState::_execute_platform_command`)
*   **Action:**
    *   Add a match arm for `PlatformCommand::CreateMainMenu`.
    *   The handler logic will be similar to the current `create_app_menu` function in `window_common.rs`. It will use `CreateMenu`, `AppendMenuW`, `SetMenu`.
    *   The `MenuItemConfig` will be used to recursively build the menu.
    *   Store the `HMENU` in `NativeWindowData` if needed for future modifications (though less common for main menus).
*   **Rationale:** Enables the platform layer to create menus based on commands.
*   **Verification:** New command handler compiles. App still uses old menu creation.

### Step 1.2: Modify `ui_description_layer` to Describe the Menu

*   **File:** `src/ui_description_layer/mod.rs`
*   **Action:**
    *   Implement `describe_main_window_layout` to generate a `PlatformCommand::CreateMainMenu` command with the existing menu structure (File -> Load, Save As, Refresh). Use the existing `ID_MENU_...` constants.
*   **Rationale:** The new layer now knows how to define the menu.
*   **Verification:** `describe_main_window_layout` produces the correct command. App still uses old menu creation.

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

### Step 7: Change platform_layer into a separate crate
