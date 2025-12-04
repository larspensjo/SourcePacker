This is a significant and positive architectural change. A detailed, step-by-step plan is crucial to ensure the application remains stable and functional throughout the process.

Here is a comprehensive plan for refactoring SourcePacker to use a project-local `.sourcepacker` directory.

---

### Refactoring Plan: Project-Centric Profiles & Project-Local Config

**Objective:** Transition from a global `AppData` profile storage to a project-local `.sourcepacker` directory, and make SourcePacker fully project-centric. Projects become self-contained and portable. The application remains functional after each step.

---

### Cross-Cutting Concept: ProjectContext

Introduce a small helper to centralize project-related paths:

```rust
struct ProjectContext {
    root: PathBuf,
}

impl ProjectContext {
    fn config_dir(&self) -> PathBuf       { self.root.join(".sourcepacker") }
    fn profile_dir(&self) -> PathBuf      { self.config_dir().join("profiles") }
    fn last_profile_file(&self) -> PathBuf{ self.config_dir().join("last_profile.txt") }
}
````

`MyAppLogic` will hold:

```rust
active_project: Option<ProjectContext>
```

instead of just `Option<PathBuf>`. In places where only a `&Path` is desired, you can pass `&active_project.root`. This keeps `.sourcepacker` path logic out of random call sites and tests.

Decide and document the relationship between:

* **Project root:** the folder the user opens.
* **Profile root:** the scan root in the profile.
* **Archive target path.**

For now, assume the project root is the natural “home” of profiles, but profiles may still reference arbitrary scan roots and archive paths. Document this in `DesignArchitecture.md`.

Also: ensure `.sourcepacker` itself is always excluded from scans (hard-coded ignore), even before `.sourcepackerignore` exists.

---

### Pre-flight Check

* Ensure all existing unit tests pass with `cargo test`.
* Ensure the codebase is clean with `cargo clippy -- -D warnings`.
* Commit all current work to a stable starting point.

---

### Phase 1: Introducing the Project Root Concept

**Goal:** Make the application aware of an “active project” without yet changing how profiles are stored. At the end of this phase, profile management still uses `AppData`, but the application has a project context and behavior is deterministic when switching projects.

#### Step 1.1: Add State for Active Project

* **Action:**

  * In `src/app_logic/handler.rs`, modify `MyAppLogic` to hold:

    * `active_project: Option<ProjectContext>` (or initially `Option<PathBuf>` if you want to phase in `ProjectContext` slightly later).
* **Files to modify:**

  * `src/app_logic/handler.rs`
* **Testing:**

  * Ensure the application still builds and runs as before.

#### Step 1.2: Update UI for Opening a Project Folder

* **Action 1:** In `src/app_logic/ui_constants.rs`:

  * Add `MENU_ACTION_OPEN_FOLDER`.
  * Optionally keep `MENU_ACTION_LOAD_PROFILE` for one transition release; we can remove it later.
* **Action 2:** In `src/ui_description_layer.rs`:

  * In `build_main_window_static_layout`, replace “Load Profile…” with “Open Folder…” (`MENU_ACTION_OPEN_FOLDER`).
* **Files to modify:**

  * `src/app_logic/ui_constants.rs`
  * `src/ui_description_layer.rs`
* **Testing:**

  * Manual: run the app and verify the File menu now contains “Open Folder…”.

#### Step 1.3: Implement the “Open Folder” Logic

* **Action 1:** In `src/app_logic/handler.rs`:

  * Add `PendingAction::OpeningProjectFolder`.
* **Action 2:** In `handle_event`:

  * For `MENU_ACTION_OPEN_FOLDER`, set `pending_action = PendingAction::OpeningProjectFolder` and enqueue `PlatformCommand::ShowFolderPickerDialog`.
* **Action 3:** In `handle_folder_picker_dialog_completed`:

  * If `pending_action == OpeningProjectFolder` and a folder was selected:

    1. Build a `ProjectContext` from the chosen path and assign it to `self.active_project`.
    2. Cancel any ongoing asynchronous work tied to the previous project/profile (token recalculation, content search) via the existing driver cancellation mechanisms.
    3. Call `_update_window_title_with_profile_and_archive` to reflect the new project context (still “No Profile Loaded” for now).
    4. Clear the pending action.
* **Files to modify:**

  * `src/app_logic/handler.rs`
* **Testing:**

  * Add `test_menu_open_folder_sets_project_root_and_updates_title`:

    * Simulate the menu click + folder picker completion.
    * Assert that `active_project` is set and that a `SetWindowTitle` command is generated.
    * Assert that any project-scoped async driver is reset or cancelled.

#### Step 1.4: Define Behavior When Switching Projects

* **Action:**

  * Decide behavior when “Open Folder…” is used while a project is already open:

    * Replace current project and profile (recommended).
    * Ensure:

      * No previous project state is leaked.
      * Any in-flight scans or token jobs are cancelled.
  * Implement this logic in `handle_folder_picker_dialog_completed`.
* **Testing:**

  * `test_open_folder_replaces_existing_project_and_profile`.

#### Checkpoint 1

* Users can select a project folder via the menu.
* The window title reflects the project context.
* Profile storage still uses global `AppData`.
* Switching projects behaves predictably and cancels old async work.

---

### Phase 2: Adapting Profile Management to be Project-Aware

**Goal:** Modify the profile manager to operate on the active project’s `.sourcepacker` directory instead of the global `AppData` location.

#### Step 2.1: Update `ProfileManagerOperations` to Accept a Project Root

* **Action:**

  * In `src/core/profiles.rs`, update `ProfileManagerOperations` to take a project root (or `ProjectContext`) where storage is needed:

    * Option A (simpler, but more plumbing):

      * `save_profile(&self, project_root: &Path, profile: &Profile, app_name: &str)`
      * `load_profile(&self, project_root: &Path, profile_name: &str, app_name: &str)`
      * `list_profiles(&self, project_root: &Path, app_name: &str)`
      * `get_profile_dir_path(&self, project_root: &Path, app_name: &str)`
    * Option B (more ergonomic):

      * Add `fn for_project(&self, project_root: &Path) -> ProjectProfileManager<'_>` wrapper and keep the trait methods project-agnostic. (Optional, can be done later.)
* **Files to modify:**

  * `src/core/profiles.rs`
* **Note:**

  * This is a breaking change and will cause compilation errors, guiding the next steps.

#### Step 2.2: Update `CoreProfileManager` Implementation for `.sourcepacker`

* **Action:**

  * In `src/core/profiles.rs`, update `CoreProfileManager`:

    1. `get_profile_storage_dir_impl` should no longer use `path_utils`. Its logic becomes:

       * `Ok(project_root.join(".sourcepacker").join("profiles"))`.
       * Ensure the directory is created if missing.
    2. All profile file I/O functions use this helper.
* **Files to modify:**

  * `src/core/profiles.rs`
* **Testing:**

  * In `profile_tests`:

    * Create a temporary project directory.
    * Pass it to the manager functions.
    * Assert that profiles are created/read under `<project_root>/.sourcepacker/profiles`.

#### Step 2.3: Update `MyAppLogic` to Pass the Active Project

* **Action:**

  * In `src/app_logic/handler.rs`:

    * Replace calls like `self.profile_manager.list_profiles(APP_NAME_FOR_PROFILES)` with versions that:

      * Early-out if `active_project.is_none()`:

        * Log: “Cannot perform profile action: No project folder is open.”
        * Optionally notify the user via status bar.
      * Otherwise, pass the project root (or `ProjectContext`) to the profile manager.
* **Files to modify:**

  * `src/app_logic/handler.rs`
* **Testing:**

  * Update `setup_logic_with_mocks` in `handler_tests.rs` to set a mock `ProjectContext`.
  * Add tests:

    * `test_profile_ops_no_project_root_are_noop`.
    * `test_profile_ops_with_project_root_use_project_local_dir`.

#### Step 2.4: Ensure `.sourcepacker` Is Ignored by the Scanner

* **Action:**

  * In `CoreFileSystemScanner::scan_directory`, ensure the `.sourcepacker` directory is always excluded (hard-coded ignore).
* **Files to modify:**

  * `src/core/file_system.rs` (or equivalent scanner module).
* **Testing:**

  * Add a test that:

    * Creates `.sourcepacker` under a scanned root.
    * Verifies no entries from that folder appear in the file tree.

#### Checkpoint 2

* With a project open, all profile operations now target `project_root/.sourcepacker/profiles`.
* When no project is open, profile menu items effectively do nothing (with clear logs / optional UI hint).
* `.sourcepacker` is never scanned as content.

---

### Phase 3: Refactoring Startup Flow and “No Project” State

**Goal:** Change startup to be project-centric and handle the “no project” state gracefully.

#### Step 3.1: Update Global Config to Store Last Project Path

* **Action:**

  * In `src/core/config.rs`:

    * Rename `load_last_profile_name` → `load_last_project_path`.
    * Rename `save_last_profile_name` → `save_last_project_path`.
    * Store a file path, e.g. in `last_project_path.txt`.
  * Consider structuring the config so it can later hold multiple recent projects (`Vec<PathBuf>`) even if only the first one is used initially.
* **Files to modify:**

  * `src/core/config.rs`
* **Testing:**

  * Update / add unit tests to verify saving/loading a project path string.

#### Step 3.2: Implement Project-Local “Last Profile” Tracking

* **Action:**

  * In `src/core/profiles.rs` (`ProfileManagerOperations` + `CoreProfileManager`):

    * Add:

      * `save_last_profile_name_for_project(project_root: &Path, profile_name: &str)`
      * `load_last_profile_name_for_project(project_root: &Path) -> Result<Option<String>>`
    * Implement via `<project_root>/.sourcepacker/last_profile.txt`.
* **Files to modify:**

  * `src/core/profiles.rs`
* **Testing:**

  * Add unit tests for these two functions using a temp project directory.

#### Step 3.3: Rewrite Application Startup Logic

* **Action:**

  * In `src/app_logic/handler.rs`, refactor `_on_ui_setup_complete`:

    1. Call `config_manager.load_last_project_path()`.
    2. If a valid and existing path is found:

       * Create `ProjectContext` and set `self.active_project`.
       * Call `profile_manager.load_last_profile_name_for_project()`.
       * If a last profile exists:

         * Load that profile and call `_activate_profile_and_show_window`.
       * Else:

         * Call `initiate_profile_selection_or_creation` (now listing project-local profiles).
    3. If the stored path is missing or does not exist anymore:

       * Clear the stored last project path in config.
       * Do not show the main window yet.
       * Immediately issue `ShowFolderPickerDialog` to let the user select a project folder.
* **Action 2:**

  * Update `_activate_profile_and_show_window` and any profile-switch operations to call `save_last_profile_name_for_project` whenever a profile is loaded or switched successfully.
* **Files to modify:**

  * `src/app_logic/handler.rs`
* **Testing:**

  * Add tests:

    * `test_startup_with_no_last_project_prompts_for_folder`
    * `test_startup_with_last_project_and_last_profile_loads_correctly`
    * `test_startup_with_last_project_but_no_last_profile_shows_selection_dialog`
    * `test_startup_with_missing_last_project_path_falls_back_to_prompt`

#### Step 3.4: Future “Open Recent” Foundations

* **Action:**

  * In `CoreConfigManager`, consider storing a list of recent projects (e.g. last 5–10 paths), even if the UI initially only uses the first one.
  * This will later drive a `File → Open Recent` submenu without another config schema change.

#### Checkpoint 3

* Startup is now project-centric:

  * Last project is used if available and valid.
  * Otherwise, the user is prompted to choose a project folder.
* Last profile is tracked per project.
* The “no project” state is explicit and handled gracefully.

---

### Phase 4: Documentation and Final Polish

**Goal:** Update all documentation to reflect the new architecture and flows.

#### Step 4.1: Update `Readme.md`

* **Action:**

  * Describe the project-based workflow:

    * “Open Folder…” to select a project.
    * Project-local `.sourcepacker` folder with profiles and state.
  * Remove or relegate references to `AppData`.
  * Add a small section on recommended `.gitignore` configuration:

    * `/.sourcepacker/` (or at least `/profiles` and `last_profile.txt` if you want some parts tracked).

#### Step 4.2: Update `DesignArchitecture.md`

* **Action:**

  * Add a “Project Context” section:

    * Definition of project root vs. profile’s scan root vs. archive path.
    * Role of `ProjectContext`.
  * Document the new project-centric startup:

    * Last project, last profile, “no project” state, and folder picker.
  * Document the split between:

    * Global config (AppData) for cross-project data like “recent projects”.
    * Local project config (`.sourcepacker`) for profiles and last profile.

#### Step 4.3: Update `Requirements.md`

* **Action:**

  * Replace obsolete requirements (e.g. `[ProfileStoreAppdataLocationV2]`) with:

    * `[ProfileStoreProjectLocalV1]` – Profiles are stored under `<project_root>/.sourcepacker/profiles`.
    * `[ProjectFolderSelectionOnStartupV1]` – Application must require a project folder to be selected to perform profile operations.
    * `[ProjectLocalLastProfileTrackingV1]` – Per-project last profile is persisted in `.sourcepacker/last_profile.txt`.
    * `[ProjectScannerIgnoreToolConfigDirV1]` – `.sourcepacker` is always ignored by the scanner.
  * Update UI requirements to reflect:

    * “Open Folder…” flow.
    * Optional future “Open Recent” submenu.

#### Checkpoint 4

* Code and documentation reflect the same architecture.
* Requirements clearly describe behavior for:

  * Project selection.
  * Project-local profile storage.
  * Startup and “no project” state.

---

### Future Considerations (Optional Follow-Up Tasks)

These tasks build on the new foundation and can be implemented independently.

#### Task A: Implement “Open Recent” Menu

* **Action:**

  * Extend `CoreConfigManager` to store a list (e.g. 5–10) of recent project paths.
  * In `ui_description_layer.rs`, add a `File → Open Recent` submenu.
  * In `MyAppLogic`, handle selection of a recent project:

    * Set `active_project`.
    * Cancel any in-flight async work.
    * Load last profile for that project if available.

#### Task B: Implement Project-Level `.sourcepackerignore`

* **Action:**

  * Add support in `CoreFileSystemScanner` for a project-local `.sourcepackerignore` file.
  * Patterns from this file are applied as ignore rules for scans.
* **Note:**

  * `.sourcepacker` itself remains hard-ignored regardless of this file.

#### Task C: Legacy AppData Profile Import

* **Action:**

  * On first run after the upgrade:

    * Detect existing profiles in the legacy `AppData` location.
    * Offer to import them into the current project’s `.sourcepacker/profiles` directory.
  * Provide a manual “Import Legacy Profile…” command as well.
* **Goal:**

  * Avoid silently abandoning old profiles.

#### Task D: Profile Import/Export Between Projects

* **Action:**

  * Add menu items:

    * “Export Profile…”
    * “Import Profile…”
  * Implementation:

    * Export: write a profile JSON file to user-selected location.
    * Import: load a profile JSON and save it under the current project’s `.sourcepacker/profiles`.
* **Goal:**

  * Allow configuration sharing without sharing the entire project.

#### Task E: CLI Project Support

* **Action:**

  * Allow `SourcePacker.exe --project "C:\my_project"`:

    * Starts the app directly on that project.
    * If `.sourcepacker` doesn’t exist, optionally prompt to initialize.

#### Task F: Project Initialization Helper

* **Action:**

  * Add “Initialize SourcePacker in This Folder”:

    * Creates `.sourcepacker/`.
    * Optionally creates a default profile and default `.sourcepackerignore`.
* **Goal:**

  * Smooth onboarding for new projects.

#### Task G: Project Diagnostics

* **Action:**

  * Add “Project Diagnostics…”:

    * Scans `.sourcepacker` for missing/corrupt profiles and other inconsistencies.
    * Helpful when projects are shared across machines via git.

#### Task H: Implement a "Generate All" button.

This will generate all archives from the current project.

#### Task I: The struct ProjectContext usage

It should be used everywhere, instead of extracting the path and forward it to sub functions.
That means several member functions will be needed.
