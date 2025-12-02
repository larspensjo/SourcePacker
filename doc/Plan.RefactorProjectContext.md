# Implementation Plan: Refactoring ProjectContext to an Opaque Domain Type

## 1. Rationale and Approach

**Why this is being done:**
Currently, `ProjectContext` is a "leaky abstraction." By implementing `AsRef<Path>`, it allows any part of the application to treat a Project as a raw filesystem path. This encourages "Primitive Obsession" (passing paths strings around) and scatters knowledge about the project's internal directory structure (`.sourcepacker`, `profiles/`, etc.) across different modules.

**How it will be done:**
We will move `ProjectContext` into the `core` layer and remove the `AsRef<Path>` implementation. Instead of exposing the raw root path, `ProjectContext` will expose semantic "resolver" methods (e.g., `resolve_profile_file("my_profile")`).

We will adopt the **"Trusted Consumer"** pattern:
1.  **MyAppLogic (The Controller):** Holds the `ProjectContext` but never looks inside it. It simply passes the context object to the Managers.
2.  **Managers (The Consumers):** The `ProfileManager` and `ConfigManager` signatures will be updated to accept `&ProjectContext`. They will call the specific resolver methods on the context to get the paths they need to perform I/O.

---

## 2. Step-by-Step Implementation Plan

### Phase 1: Define the Domain Object

**Step 1.1: Create `src/core/project_context.rs`**
*   Create a new module `src/core/project_context.rs`.
*   Move the `ProjectContext` struct definition from `src/app_logic/handler.rs` to this new file.
*   **Crucial:** Do **not** implement `AsRef<Path>`.

**Step 1.2: Implement Semantic Resolvers**
Implement the following methods on `ProjectContext` to encapsulate the topology:
*   `new(root: PathBuf) -> Self`: The only place a raw path enters.
*   `resolve_root_for_serialization(&self) -> &Path`: Used *only* by `ConfigManager` to save the MRU list.
*   `resolve_config_dir(&self) -> PathBuf`: Returns the `.sourcepacker` path.
*   `resolve_profiles_dir(&self) -> PathBuf`: Returns the `.sourcepacker/profiles` path.
*   `resolve_last_profile_pointer_file(&self) -> PathBuf`: Returns path to `last_profile.txt`.
*   `resolve_profile_file(&self, profile_name: &str) -> PathBuf`: Accepts a profile name, sanitizes it (you may need to move the sanitization logic here or import it), adds the `.json` extension, and returns the full path.
*   `display_name(&self) -> String`: For UI titles.

**Step 1.3: Expose the Module**
*   Update `src/core.rs` to `mod project_context` and `pub use project_context::ProjectContext`.

---

### Phase 2: Refactor Core Managers (The Trusted Consumers)

**Step 2.1: Update `ProfileManagerOperations` Trait**
*   Modify `src/core/profiles.rs`.
*   Change the signature of `load_profile`, `save_profile`, `list_profiles`, `save_last_profile_name_for_project`, and `load_last_profile_name_for_project`.
*   Replace `project_root: &Path` with **`project_context: &ProjectContext`**.

**Step 2.2: Update `CoreProfileManager` Implementation**
*   Refactor `src/core/profiles.rs`.
*   Remove the private helper `get_profile_storage_dir_impl`. Use `project_context.resolve_profiles_dir()` instead.
*   In `load_profile` and `save_profile`, do not manually join paths or strings. Use `project_context.resolve_profile_file(profile_name)`.
*   In `list_profiles`, use `project_context.resolve_profiles_dir()`.

**Step 2.3: Update `ConfigManagerOperations` Trait**
*   Modify `src/core/config.rs`.
*   Change `save_last_project_path` to accept `project: Option<&ProjectContext>` instead of `Option<&Path>`.

**Step 2.4: Update `CoreConfigManager` Implementation**
*   In `save_last_project_path`, map the `Option<&ProjectContext>` using `.map(|ctx| ctx.resolve_root_for_serialization())` to get the path needed for writing to the text file.

---

### Phase 3: Refactor Application Logic (The Controller)

**Step 3.1: Cleanup `handler.rs`**
*   Remove the local definition of `ProjectContext` in `src/app_logic/handler.rs`.
*   Import `crate::core::ProjectContext`.

**Step 3.2: Update Method Calls**
*   The compiler will flag every place where you passed `self.active_project.as_ref()` or `&path`.
*   Refactor `MyAppLogic` methods (like `initiate_profile_selection_or_creation`, `handle_menu_save_profile_as`, etc.) to pass the `&ProjectContext` directly to the manager methods.
*   In `_update_window_title_with_profile_and_archive`, use `ctx.display_name()` (or `ctx.display_full_path()` if you added it) instead of accessing the path directly.

**Step 3.3: Handle Initialization**
*   In `handle_folder_picker_dialog_completed` and startup logic, use `ProjectContext::new(path)` to wrap the raw `PathBuf` coming from the dialog or config file.

---

### Phase 4: Test Suite Updates

**Step 4.1: Fix `handler_tests.rs`**
*   The Mocks (`MockProfileManager`, `MockConfigManager`) implement the traits. You must update the Mock signatures to match the new Trait definitions (accepting `&ProjectContext`).
*   Update the `MockSetup` helper to create a `ProjectContext` when setting up the test state.

**Step 4.2: Fix `profiles.rs` Tests**
*   Update unit tests in `profiles.rs` to instantiate a `ProjectContext` using a temporary directory path before passing it to the manager methods.

### Phase 5: Verification

**Step 5.1: Compilation Check**
*   Run `cargo check`. Ensure no `as_ref()` calls remain on the project context variable.

**Step 5.2: Logic Verification**
*   Run `cargo test` to ensure the mocks and core logic still hold together with the new types.
*   Verify that `CoreProfileManager` tests still pass, ensuring the internal path resolution inside `ProjectContext` correctly maps to the physical disk structure (`.sourcepacker/profiles/`).
