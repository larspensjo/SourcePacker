# Refactoring Plan: Enhancing M/V/P Separation in SourcePacker

**Goal:** Improve the architectural separation, particularly between the Model, View, and Presenter/Controller components, by restructuring `MyAppLogic`'s state and responsibilities. This involves introducing `AppSessionData` (for core application session state) and `MainWindowUiState` (for UI-specific window state), and moving relevant logic into these new components. The application should remain functional after each major step.

**Legend for Changes:**
*   **[M]**: Relates to Model components (primarily `core` directory, including the new `AppSessionData`).
*   **[P]**: Relates to Presenter/Controller components (primarily `app_logic::handler::MyAppLogic`).
*   **[V]**: Relates to View components (primarily `platform_layer` and `ui_description_layer`, but also how Presenter interacts with `MainWindowUiState` to command the View).
*   **[S]**: Structural changes (file/module organization).

---

## Phase 1: Introduce `AppSessionData` and `MainWindowUiState` Structs (Minimal Logic)

**Objective:** Define the new state-holding structs and move the relevant fields from `MyAppLogic` into them. `MyAppLogic` will hold instances of these new structs. Functionality remains largely within `MyAppLogic` methods for now, but they will access data through the new structs.

1.  **[S] Create new files:**
    *   Create `src/core/app_session_data.rs`.
    *   Create `src/app_logic/main_window_ui_state.rs`.

2.  **[M] Define `AppSessionData` struct in `src/core/app_session_data.rs`:**
    *   Move fields from `MyAppLogic` that represent the core application's working data set:
        *   `current_profile_name: Option<String>`
        *   `current_profile_cache: Option<Profile>`
        *   `file_nodes_cache: Vec<FileNode>`
        *   `root_path_for_scan: PathBuf`
        *   `current_token_count: usize`
    *   Add `pub(crate)` or `pub` visibility as needed for fields if `MyAppLogic` will access them directly initially.
    *   Create a basic `new()` constructor for `AppSessionData`.
    *   Update `src/core/mod.rs` to declare `mod app_session_data;` and `pub use app_session_data::AppSessionData;`.

3.  **[V] Define `MainWindowUiState` struct in `src/app_logic/main_window_ui_state.rs`:**
    *   Move fields from `MyAppLogic` specific to the main window's UI state:
        *   `window_id: Option<WindowId>` (no longer `main_window_id` in `MyAppLogic`)
        *   `path_to_tree_item_id: PathToTreeItemIdMap`
        *   `next_tree_item_id_counter: u64`
        *   `current_archive_status: Option<ArchiveStatus>` (as `current_archive_status_for_ui`)
        *   `pending_action: Option<PendingAction>`
        *   `pending_new_profile_name: Option<String>`
    *   Add `pub(crate)` or `pub` visibility.
    *   Create a basic `new(window_id: WindowId)` constructor.
    *   Update `src/app_logic/mod.rs` to declare `mod main_window_ui_state;` and `pub use main_window_ui_state::MainWindowUiState;`.

4.  **[P] Modify `MyAppLogic` in `src/app_logic/handler.rs`:**
    *   Remove the fields that were moved to `AppSessionData` and `MainWindowUiState`.
    *   Add new fields:
        *   `app_session_data: AppSessionData`
        *   `ui_state: Option<MainWindowUiState>` (Option because the window might not exist yet at `MyAppLogic` creation).
    *   Update `MyAppLogic::new()` to initialize `app_session_data` (e.g., with default `root_path_for_scan`) and `ui_state` to `None`.
    *   In `_on_ui_setup_complete`, instantiate `MainWindowUiState` and assign it to `self.ui_state`.
    *   Go through all methods in `MyAppLogic` and change direct field access (e.g., `self.current_profile_name`) to access via the new structs (e.g., `self.app_session_data.current_profile_name`, `self.ui_state.as_ref().unwrap().window_id`). Use `as_mut()` where modification is needed.
        *   Focus on just getting the field access right. Logic movement comes later.
        *   Example: `self.main_window_id` becomes `self.ui_state.as_ref().and_then(|ui| ui.window_id)`.
        *   Example: `self.file_nodes_cache` becomes `self.app_session_data.file_nodes_cache`.

5.  **[P] Update `handle_window_destroyed` in `MyAppLogic`:**
    *   This method should now primarily set `self.ui_state = None;` if the destroyed window was the main one.

6.  **Build and Test:**
    *   Ensure the application compiles and all existing tests pass.
    *   Manually test core functionalities (profile loading, saving, file selection, archive generation) to ensure no regressions due to field access changes.

---

## Phase 2: Move Simple Data-Centric Logic to `AppSessionData`

**Objective:** Move methods from `MyAppLogic` that primarily operate on `AppSessionData`'s fields into `AppSessionData` itself.

1.  **[M] Identify and move methods to `AppSessionData`:**
    *   `create_profile_from_current_state` (rename to `create_profile_from_session_state` or similar in `AppSessionData`):
        *   This method reads `file_nodes_cache`, `root_path_for_scan`, `current_profile_cache.archive_path`.
        *   The helper `gather_selected_deselected_paths_recursive` might need to be made a public static helper in `MyAppLogic` or moved/replicated into `AppSessionData` if it's simple enough and only uses `FileNode` types.
    *   The core logic of `_update_token_count_and_request_display` (the part that *calculates* `current_token_count`):
        *   Move the calculation logic into a new method in `AppSessionData`, e.g., `fn update_token_count(&mut self)`. This method will update `self.current_token_count`.
        *   `_update_token_count_and_request_display` in `MyAppLogic` will then call `self.app_session_data.update_token_count()` and then queue the `UpdateStatusBarText` command.

2.  **[P] Update `MyAppLogic` to call new `AppSessionData` methods:**
    *   In `on_quit`, call `self.app_session_data.create_profile_from_session_state(...)`.
    *   In places where token count needs updating, call `self.app_session_data.update_token_count()` and then the separate `MyAppLogic` helper to queue the display command (e.g., `self._request_token_count_display()`).

3.  **[M] Consider `StateManagerOperations` interaction:**
    *   Methods like `apply_profile_to_tree` (from `StateManagerOperations`) operate on `file_nodes_cache`.
    *   You could create wrapper methods in `AppSessionData`:
        ```rust
        // In AppSessionData
        pub fn apply_profile(&mut self, profile: &Profile, state_manager: &dyn StateManagerOperations) {
            state_manager.apply_profile_to_tree(&mut self.file_nodes_cache, profile);
        }
        ```
    *   `MyAppLogic` would then call `self.app_session_data.apply_profile(&profile, &*self.state_manager);`.

4.  **Build and Test:**
    *   Compile and run all tests.
    *   Manually verify profile saving on quit and token count updates.

---

## Phase 3: Move UI-Specific Logic to `MainWindowUiState`

**Objective:** Move methods from `MyAppLogic` that primarily manage or derive data for the UI window's state into `MainWindowUiState`.

1.  **[V] Identify and move methods to `MainWindowUiState`:**
    *   `generate_tree_item_id`: This uses `next_tree_item_id_counter`. Move it to `MainWindowUiState`.
    *   The core logic of `build_tree_item_descriptors_recursive` (the part that *builds* descriptors and updates `path_to_tree_item_id` and `next_tree_item_id_counter`):
        *   Move this into a method in `MainWindowUiState`, e.g., `fn build_tree_item_descriptors(&mut self, nodes: &[FileNode]) -> Vec<TreeItemDescriptor>`.
        *   The helper `build_tree_item_descriptors_recursive_internal` would either be moved into `MainWindowUiState` as a private helper or become a public static method on `MyAppLogic` if it's generic enough. Given it mutates `MainWindowUiState`'s fields, making it a private method within `MainWindowUiState` is cleaner.
    *   Methods for managing `pending_action` and `pending_new_profile_name` (setters, getters, clearers) can be moved to `MainWindowUiState`.

2.  **[P] Update `MyAppLogic` to call new `MainWindowUiState` methods:**
    *   When populating the tree view (e.g., in `_activate_profile_and_show_window` or `handle_menu_refresh_file_list_clicked`), `MyAppLogic` would call `self.ui_state.as_mut().unwrap().build_tree_item_descriptors(&self.app_session_data.file_nodes_cache)`.
    *   When starting dialog flows, call methods on `self.ui_state.as_mut().unwrap()` to set pending actions/names.

3.  **[V] Consider UI Command Queuing:**
    *   Methods in `MainWindowUiState` should *not* directly access `MyAppLogic`'s `synchronous_command_queue`.
    *   Instead, methods like `build_tree_item_descriptors` return the data (`Vec<TreeItemDescriptor>`).
    *   `MyAppLogic` then takes this returned data and creates/enqueues the `PlatformCommand`.
    *   Helper methods in `MyAppLogic` like `_refresh_tree_view_from_cache`, `_update_window_title_with_profile_and_archive`, `_update_save_to_archive_button_state` will remain in `MyAppLogic` as they are about *commanding the platform*. They will use data from `app_session_data` and `ui_state` to form these commands.

4.  **Build and Test:**
    *   Compile and run all tests.
    *   Manually verify tree view population, dialog flows, and UI updates.

---

## Phase 4: Refine Orchestration in `MyAppLogic`

**Objective:** Ensure `MyAppLogic` methods are now primarily orchestrators, coordinating between `AppSessionData`, `MainWindowUiState`, service managers, and the command queue.

1.  **[P] Review all methods in `MyAppLogic`:**
    *   Ensure that methods clearly show their dependencies on `app_session_data` and `ui_state`.
    *   Break down larger event handlers or methods into smaller, more focused private helpers if they become too complex.
    *   Example: `_activate_profile_and_show_window` would:
        1.  Update `app_session_data` with the new profile.
        2.  Call `file_system_scanner` (using `app_session_data.root_path_for_scan`).
        3.  Update `app_session_data.file_nodes_cache`.
        4.  Call `app_session_data.apply_profile(...)`.
        5.  Call `app_session_data.update_token_count()`.
        6.  If `ui_state` exists:
            *   Queue `SetWindowTitle` (using `app_session_data` and `ui_state`).
            *   Queue `PopulateTreeView` (using `ui_state.build_tree_item_descriptors(&app_session_data.file_nodes_cache)`).
            *   Queue `UpdateStatusBarText` for tokens (using `app_session_data.current_token_count`).
            *   Update and queue `SetControlEnabled` for save button (using `app_session_data.current_profile_cache.archive_path`).
            *   Queue `ShowWindow`.

2.  **[P] Refine Status Message Macros/Logic:**
    *   The `status_message!` macro uses `self.main_window_id` (now in `ui_state`) and `self.synchronous_command_queue`.
    *   Consider if these macros need adjustment or if status updates become more explicit calls like `self.queue_status_update(severity, message)`. For now, direct access via `self.ui_state.as_ref().and_then(|ui| ui.window_id)` within the macro might still work, but it's getting complex.
    *   A helper method on `MyAppLogic` like `fn _enqueue_status_update(&mut self, severity: MessageSeverity, text: String)` could encapsulate this.

3.  **[S] Static Helpers Cleanup:**
    *   Re-evaluate static helper functions like `find_filenode_mut`, `gather_selected_deselected_paths_recursive`, etc.
    *   Should they be public static methods of `MyAppLogic`?
    *   Should they be free functions in a new `src/app_logic/utils.rs` module?
    *   If `gather_selected_deselected_paths_recursive` is only used by `AppSessionData::create_profile_from_session_state`, it could become a private helper within that method or `AppSessionData`.

4.  **Build and Test:**
    *   Thoroughly test all application flows.
    *   Pay attention to edge cases and error handling.

---
## Phase 5: Fix all tests in handler_tests.rs

Many of these should be moved to AppSessionData or MainWindowUiState.

Evaluate whether the injected services should also be moved. This is probably necessary, to be able to move the tests effectively.
---

## Phase 6: Documentation and Review

**Objective:** Document the new architecture and review the changes for clarity and correctness.

1.  **[S] Update module-level documentation:**
    *   Explain the roles of `app_session_data.rs`, `main_window_ui_state.rs`, and `handler.rs`.
    *   Describe the responsibilities of `AppSessionData`, `MainWindowUiState`, and `MyAppLogic`.
2.  **Code Review:**
    *   Review the changes with a focus on separation of concerns, clarity, and robustness.
    *   Ensure that `AppSessionData` is mostly independent of UI specifics and `MainWindowUiState` is mostly independent of core data manipulation logic (it might *read* core data to build UI descriptions, but not *change* it).
