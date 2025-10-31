# Refactor Plan: Decoupling Menu Actions with the Newtype ID Pattern

**Goal:** Refactor `CommanDuctUI` to remove the application-specific `MenuAction` enum and replace it with a generic `MenuActionId` newtype. This will decouple the library from its consumers, allowing any application to define its own menu actions.

---

### **Phase 1: Update the `CommanDuctUI` Library**

**Goal:** Modify the public API of `CommanDuctUI` to use the new ID-based pattern. The library will not compile until `SourcePacker` is updated in Phase 2.

*   **Step 1.1: Define `MenuActionId` and Update Core Types**
    *   **File:** `src/CommanDuctUI/src/types.rs`
    *   **Action:**
        1.  Create the new `MenuActionId` newtype struct.
            ```rust
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct MenuActionId(pub u32);
            ```
        2.  Modify the `MenuItemConfig` struct to use `Option<MenuActionId>` instead of `Option<MenuAction>`.
            ```rust
            #[derive(Debug, Clone)]
            pub struct MenuItemConfig {
                pub action: Option<MenuActionId>, // CHANGED
                pub text: String,
                pub children: Vec<MenuItemConfig>,
            }
            ```
        3.  Modify the `AppEvent` enum. The `MenuActionClicked` variant should now carry a `MenuActionId`.
            ```rust
            #[derive(Debug)]
            pub enum AppEvent {
                // ... other events
                MenuActionClicked {
                    action_id: MenuActionId, // CHANGED
                },
                // ...
            }
            ```

*   **Step 1.2: Update Menu Creation Logic**
    *   **File:** `src/CommanDuctUI/src/controls/menu_handler.rs`
    *   **Action:**
        1.  In `add_menu_item_recursive_impl`, the `item_config.action` will now be an `Option<MenuActionId>`.
        2.  The `window_data.register_menu_action` method will need to be updated to accept a `MenuActionId` instead of a `MenuAction`.

*   **Step 1.3: Update Event Generation Logic**
    *   **File:** `src/CommanDuctUI/src/controls/menu_handler.rs`
    *   **Action:**
        1.  In `handle_wm_command_for_menu`, the `window_data.get_menu_action` method will now return an `Option<MenuActionId>`.
        2.  When an action is found, construct the `AppEvent::MenuActionClicked { action_id: ... }` variant with the retrieved ID.

*   **Step 1.4: Update `NativeWindowData`**
    *   **File:** `src/CommanDuctUI/src/window_common.rs`
    *   **Action:**
        1.  Change the `menu_action_map` from `HashMap<i32, MenuAction>` to `HashMap<i32, MenuActionId>`.
        2.  Update the signatures of `register_menu_action` and `get_menu_action` accordingly.

*   **Step 1.5: Remove the Old `MenuAction` Enum**
    *   **File:** `src/CommanDuctUI/src/types.rs`
    *   **Action:** Delete the entire `pub enum MenuAction { ... }` block.

*   **Checkpoint:** At this point, `CommanDuctUI` is internally consistent but will fail to build because `SourcePacker` still refers to the old types.

---

### **Phase 2: Update the `SourcePacker` Application**

**Goal:** Adapt the `SourcePacker` codebase to the new API provided by `CommanDuctUI`.

*   **Step 2.1: Define Application-Specific Menu Action IDs**
    *   **File:** `src/app_logic/ui_constants.rs` (A good central place for them)
    *   **Action:**
        1.  Add a `use` statement for the new type: `use commanductui::MenuActionId;`.
        2.  Define your application's menu actions as `pub const` values. Start numbering from 1.
            ```rust
            pub const MENU_ACTION_LOAD_PROFILE: MenuActionId = MenuActionId(1);
            pub const MENU_ACTION_NEW_PROFILE: MenuActionId = MenuActionId(2);
            pub const MENU_ACTION_SAVE_PROFILE_AS: MenuActionId = MenuActionId(3);
            pub const MENU_ACTION_SET_ARCHIVE_PATH: MenuActionId = MenuActionId(4);
            pub const MENU_ACTION_EDIT_EXCLUDE_PATTERNS: MenuActionId = MenuActionId(5);
            pub const MENU_ACTION_REFRESH_FILE_LIST: MenuActionId = MenuActionId(6);
            pub const MENU_ACTION_GENERATE_ARCHIVE: MenuActionId = MenuActionId(7);
            ```

*   **Step 2.2: Update UI Description Layer**
    *   **File:** `src/ui_description_layer.rs`
    *   **Action:**
        1.  In `build_main_window_static_layout`, find where `MenuItemConfig` is created.
        2.  Replace all `MenuAction::*` enum variants with your new `ui_constants::MENU_ACTION_*` constants.
            ```rust
            // Before
            // action: Some(MenuAction::LoadProfile),

            // After
            action: Some(ui_constants::MENU_ACTION_LOAD_PROFILE),
            ```

*   **Step 2.3: Update Event Handling Logic**
    *   **File:** `src/app_logic/handler.rs`
    *   **Action:**
        1.  In the main `handle_event` method, find the `match` arm for `AppEvent::MenuActionClicked`.
        2.  Update its signature to destructure `action_id`: `AppEvent::MenuActionClicked { action_id }`.
        3.  Change the inner `match` to compare against your `ui_constants`.
            ```rust
            // In handle_event...
            AppEvent::MenuActionClicked { action_id } => match action_id {
                ui_constants::MENU_ACTION_LOAD_PROFILE => self.handle_menu_load_profile_clicked(),
                ui_constants::MENU_ACTION_NEW_PROFILE => self.handle_menu_new_profile_clicked(),
                // ... and so on for all other actions
                _ => log::warn!("Received unhandled menu action ID: {:?}", action_id),
            },
            ```

---

### **Phase 3: Verification and Commit**

**Goal:** Ensure the entire project builds, runs, and passes tests, then commit the changes correctly across both repositories.

*   **Step 3.1: Full Project Build and Test**
    *   **Action:** From the root of the `source_packer` repository, run:
        ```bash
        cargo check
        cargo test
        cargo clippy -- -D warnings
        ```    *   **Expected:** The entire project should now compile successfully and all tests should pass.
    *   **Action:** Run the application (`cargo run`) and manually verify that all menu items function as they did before the refactor.

*   **Step 3.2: Commit Changes in the Submodule**
    *   **Action:** Navigate to the submodule directory, commit, and push the changes.
        ```bash
        cd src/CommanDuctUI
        git add .
        git commit -m "refactor(api): Replace MenuAction enum with MenuActionId newtype"
        git push
        cd ../..
        ```

*   **Step 3.3: Commit Changes in the Main Project**
    *   **Action:** The main `source_packer` repository will now see the code changes in its own files and the updated commit pointer for the submodule. Commit everything.
        ```bash
        git add .
        git commit -m "refactor: Adapt to CommanDuctUI's new MenuActionId API"
        git push
        ```
