This is a significant and positive architectural change. A detailed, step-by-step plan is crucial to ensure the application remains stable and functional throughout the process.

Here is a comprehensive plan for refactoring SourcePacker to use a project-local `.sourcepacker` directory.

---

### **Refactoring Plan: Project-Centric Profiles**

**Objective:** Transition from a global `AppData` profile storage to a project-local `.sourcepacker` directory. This will make projects self-contained and portable. The application will remain functional after each step.

#### **Pre-flight Check**

*   Ensure all existing unit tests pass with `cargo test`.
*   Ensure the codebase is clean with `cargo clippy -- -D warnings`.
*   Commit all current work to a stable starting point.

---

### **Phase 1: Introducing the "Project Root" Concept**

**Goal:** Make the application aware of an "active project folder" without yet changing how profiles are stored. At the end of this phase, profile management will still use `AppData`, but the application will have a concept of a current project context.

*   **Step 1.1: Add State for Active Project**
    *   **Action:** In `src/app_logic/handler.rs`, modify the `MyAppLogic` struct to include a field for the current project root.
        ```rust
        // In MyAppLogic struct
        active_project_root: Option<PathBuf>,
        ```
    *   **Files to Modify:** `src/app_logic/handler.rs`.
    *   **Testing:** No new tests needed for adding a field. The application will build and run as before.

*   **Step 1.2: Update UI for Opening a Project Folder**
    *   **Action 1:** In `src/app_logic/ui_constants.rs`, add a new menu action constant `MENU_ACTION_OPEN_FOLDER` and remove the old `MENU_ACTION_LOAD_PROFILE`.
    *   **Action 2:** In `src/ui_description_layer.rs`, update `build_main_window_static_layout`. Replace the "Load Profile..." menu item with a new "**Open Folder...**" menu item that uses `MENU_ACTION_OPEN_FOLDER`.
    *   **Files to Modify:** `src/app_logic/ui_constants.rs`, `src/ui_description_layer.rs`.
    *   **Testing:** Manually run the application to confirm the "File" menu now shows "Open Folder...". Clicking it will do nothing yet.

*   **Step 1.3: Implement the "Open Folder" Logic**
    *   **Action 1:** In `src/app_logic/handler.rs`, add a new `PendingAction` variant: `OpeningProjectFolder`.
    *   **Action 2:** In `handle_event`, add a match arm for `MENU_ACTION_OPEN_FOLDER`. This handler will set the pending action to `OpeningProjectFolder` and command the platform layer to `ShowFolderPickerDialog`.
    *   **Action 3:** Modify `handle_folder_picker_dialog_completed`. Add logic to check if the pending action is `OpeningProjectFolder`. If it is, and a folder was selected:
        1.  Set `self.active_project_root` to the chosen path.
        2.  Call `_update_window_title_with_profile_and_archive` to reflect the new project context (the title will still show "No Profile Loaded" for now, which is correct).
        3.  Clear the pending action.
    *   **Files to Modify:** `src/app_logic/handler.rs`.
    *   **Testing:**
        *   Add a new unit test in `handler_tests.rs`: `test_menu_open_folder_sets_project_root`.
        *   This test should simulate the `MenuActionClicked` event for opening a folder, followed by a `FolderPickerDialogCompleted` event.
        *   Assert that `MyAppLogic::active_project_root` is updated correctly and that a `PlatformCommand::SetWindowTitle` is generated.

*   **Checkpoint 1:** The application is fully functional. Users can now select a project folder via the menu, and the window title will update to reflect this. Profile loading and saving still use the global `AppData` directory.

---

### **Phase 2: Adapting Profile Management to be Project-Aware**

**Goal:** Modify the profile manager to operate on the active project's `.sourcepacker` directory instead of the global `AppData` location.

*   **Step 2.1: Modify the `ProfileManagerOperations` Trait**
    *   **Action:** Update the signatures of `save_profile`, `load_profile`, `list_profiles`, and `get_profile_dir_path` in the `ProfileManagerOperations` trait to accept a `project_root: &Path` parameter.
    *   **Files to Modify:** `src/core/profiles.rs`.
    *   **Note:** This is a breaking change and will cause compilation errors, which will guide the next steps.

*   **Step 2.2: Update `CoreProfileManager` Implementation**
    *   **Action:** In `src/core/profiles.rs`, modify the `CoreProfileManager` implementation.
        1.  The `get_profile_storage_dir_impl` function should no longer use `path_utils`. Its logic should be: `Ok(project_root.join(".sourcepacker").join("profiles"))`. Ensure this function also creates the directory structure if it doesn't exist.
        2.  Update the `save_profile`, `load_profile`, etc., methods to use the new `project_root` parameter when calling the internal directory helper.
    *   **Files to Modify:** `src/core/profiles.rs`.
    *   **Testing:** Update the unit tests in `profile_tests` to create a temporary project directory, pass it to the manager functions, and assert that profile files are correctly created and read from the `.sourcepacker/profiles` subdirectory.

*   **Step 2.3: Update `MyAppLogic` to Pass the Active Project Root**
    *   **Action:** In `src/app_logic/handler.rs`, search for all calls to `self.profile_manager`. Update each call to pass `&self.active_project_root`.
    *   **Action 2:** Add guard clauses at the beginning of any function that uses the profile manager. If `self.active_project_root` is `None`, log a warning and return early (e.g., `app_warn!(self, "Cannot perform profile action: No project folder is open.");`).
    *   **Files to Modify:** `src/app_logic/handler.rs`.
    *   **Testing:** Update the `setup_logic_with_mocks` test helper in `handler_tests.rs` to set a mock project root in `MyAppLogic`. Rerun tests to ensure they pass with the new parameter. Add a test case to verify that profile operations do nothing if no project root is set.

*   **Checkpoint 2:** The application is functional. When a project folder is open, all profile operations (Load, Save As, Switch) now correctly target the local `.sourcepacker` directory. If no project is open, these actions do nothing. Startup still uses the old logic.

---

### **Phase 3: Refactoring the Startup Flow and "No Project" State**

**Goal:** Change the application's startup sequence to be project-centric and gracefully handle the state where no project is open.

*   **Step 3.1: Update Global Config to Store Project Path**
    *   **Action:** In `src/core/config.rs`, rename `load_last_profile_name` to `load_last_project_path` and `save_last_profile_name` to `save_last_project_path`. Update the implementation to read/write a file path from a file like `last_project_path.txt`.
    *   **Files to Modify:** `src/core/config.rs`.
    *   **Testing:** Update the unit tests in `config.rs` to verify the correct saving and loading of a file path string.

*   **Step 3.2: Implement Project-Local "Last Profile" Tracking**
    *   **Action:** In `src/core/profiles.rs`, add two new methods to `ProfileManagerOperations` and `CoreProfileManager`:
        *   `save_last_profile_name_for_project(project_root: &Path, profile_name: &str)`
        *   `load_last_profile_name_for_project(project_root: &Path) -> Result<Option<String>>`
    *   **Implementation:** These methods will write to and read from `<project_root>/.sourcepacker/last_profile.txt`.
    *   **Files to Modify:** `src/core/profiles.rs`.
    *   **Testing:** Add new unit tests in `profile_tests` for these two new methods.

*   **Step 3.3: Rewrite the Application Startup Logic**
    *   **Action:** In `src/app_logic/handler.rs`, completely refactor the `_on_ui_setup_complete` method.
        1.  Call `config_manager.load_last_project_path()`.
        2.  **If a valid path is found:**
            *   Set `self.active_project_root`.
            *   Call `profile_manager.load_last_profile_name_for_project()`.
            *   If a last profile name is found, load that profile and call `_activate_profile_and_show_window`.
            *   If no last profile name is found, call `initiate_profile_selection_or_creation` (which will now list profiles from the project).
        3.  **If NO valid path is found:**
            *   Do not show the main window.
            *   Immediately call `ShowFolderPickerDialog` to prompt the user to select their first project folder.
    *   **Action 2:** Update `_activate_profile_and_show_window` and any other relevant places to call `save_last_profile_name_for_project` whenever a profile is successfully loaded or switched.
    *   **Files to Modify:** `src/app_logic/handler.rs`.
    *   **Testing:** Add new unit tests in `handler_tests.rs` to cover the main startup scenarios:
        *   `test_startup_with_no_last_project_prompts_for_folder`
        *   `test_startup_with_last_project_and_last_profile_loads_correctly`
        *   `test_startup_with_last_project_but_no_last_profile_shows_selection_dialog`

*   **Checkpoint 3:** The refactor is functionally complete. The application now starts up based on the last project, manages profiles within that project, and provides a clear onboarding flow for new users.

---

### **Phase 4: Documentation and Final Polish**

**Goal:** Update all project documentation to reflect the new architecture and user flow.

*   **Step 4.1: Update `Readme.md`**
    *   **Action:** Revise the main project `Readme.md` to explain the new project-based workflow. Remove references to `AppData`.
    *   **Files to Modify:** `Readme.md`.

*   **Step 4.2: Update `DesignArchitecture.md`**
    *   **Action:** Update the architecture document to describe the project-centric model. Explain the role of the `active_project_root` and how the `CoreProfileManager` now operates within its context. Document the new roles of the global config (`AppData`) vs. the local project config (`.sourcepacker`).
    *   **Files to Modify:** `doc/DesignArchitecture.md`.

*   **Step 4.3: Update `Requirements.md`**
    *   **Action:** Review and update requirements. For example, `[ProfileStoreAppdataLocationV2]` is now obsolete and should be replaced with a requirement specifying the `.sourcepacker` folder. Update UI requirements to reflect the "Open Folder" flow.
    *   **Files to Modify:** `doc/Requirements.md`.

---

### **Future Considerations (Optional Follow-Up Tasks)**

Once the core refactor is complete, these ideas can be implemented as separate features, building on the new foundation.

*   **Task A: Implement "Open Recent" Menu**
    *   **Action:** Modify `CoreConfigManager` to store a list of the last 5-10 project paths. Create a `File -> Open Recent` submenu in `ui_description_layer.rs` and the corresponding event handling in `MyAppLogic`.

*   **Task B: Implement Project-Level `.sourcepackerignore` File**
    *   **Action:** Modify `CoreFileSystemScanner::scan_directory` to look for a `.sourcepackerignore` file in the `project_root`. If found, its patterns would be added to the `WalkBuilder` as global ignore rules for that scan.

*   **Task C: Profile Import/Export**
    *   **Action:** Add menu items and logic to allow users to export a profile from one project's `.sourcepacker` folder and import it into another. This could be useful for migrating configurations or sharing a single profile without sharing the whole project.
