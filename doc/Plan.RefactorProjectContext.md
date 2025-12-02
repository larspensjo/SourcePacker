# Implementation Plan: Refactoring `ProjectContext` to an Opaque Domain Type

## 1. Rationale and Approach

Current problems:

* `ProjectContext` lives in `app_logic::handler` and implements `AsRef<Path>`, allowing callers to treat a project as a raw path.
* Knowledge about `.sourcepacker`, `profiles/`, `last_profile.txt`, and profile filenames is spread across modules.

Target design:

* `ProjectContext` becomes a domain object in `core`, the single source of truth for project layout.
* `ProjectContext` does not implement `AsRef<Path>`.
* All path topology is accessed through semantic resolvers.
* Only managers in `core` are allowed to use resolvers; `app_logic` sees a higher-level object.

Trusted Consumer pattern:

1. `MyAppLogic` (controller) holds `Option<ProjectContext>`, but never works with raw paths.
2. Core managers (profile/config) are the trusted consumers; they receive `&ProjectContext` and use its resolvers.

Enforcement:

* Resolvers are `pub(super)` and callable only from inside `core`.
* I/O (directory creation, file access) remains in the managers, not in `ProjectContext`.

---

## 2. Step-by-Step Implementation Plan

### Phase 1: Define the Domain Object (COMPLETE)

#### Step 1.1: Create `src/core/project_context.rs`

* Create a new module `src/core/project_context.rs`.
* Move the existing `ProjectContext` definition from `src/app_logic/handler.rs` into this file.
* Define the structure approximately as:

```rust
#[derive(Debug, Clone)]
pub struct ProjectContext {
    root: PathBuf,
}
```

* Implement `ProjectContext::new(root: PathBuf) -> Self` as the only entry point for raw paths.

Optional (if you want stricter behavior):

* Make `new` return `Result<ProjectContext, ProjectContextError>` and validate that `root` exists and is a directory.
* For now, simple `ProjectContext { root }` is acceptable.

#### Step 1.2: Consolidate topology constants

* Choose a canonical home for:

  * `PROJECT_CONFIG_DIR_NAME` (e.g. `.sourcepacker`)
  * `PROFILES_SUBFOLDER_NAME` (e.g. `profiles`)
  * `PROFILE_FILE_EXTENSION` (e.g. `json`)
  * `LAST_PROFILE_FILENAME` (e.g. `last_profile.txt`)
* Prefer placing these in `project_context.rs` and importing them from `profiles.rs` and `file_system.rs`, to avoid divergence.

#### Step 1.3: Implement semantic resolvers with scoped visibility

On `ProjectContext`, implement:

* Public API:

```rust
impl ProjectContext {
    pub fn new(root: PathBuf) -> Self { ... }

    pub fn display_name(&self) -> String {
        // existing UI-friendly name logic
    }
}
```

* Core-only resolvers (`pub(super)`):

```rust
impl ProjectContext {
    pub(super) fn resolve_root_for_serialization(&self) -> &Path {
        &self.root
    }

    pub(super) fn resolve_config_dir(&self) -> PathBuf {
        self.root.join(PROJECT_CONFIG_DIR_NAME)
    }

    pub(super) fn resolve_profiles_dir(&self) -> PathBuf {
        self.resolve_config_dir().join(PROFILES_SUBFOLDER_NAME)
    }

    pub(super) fn resolve_last_profile_pointer_file(&self) -> PathBuf {
        self.resolve_config_dir().join(LAST_PROFILE_FILENAME)
    }

    pub(super) fn resolve_profile_file(&self, profile_name: &str) -> PathBuf {
        let sanitized = sanitize_profile_name(profile_name);
        self.resolve_profiles_dir()
            .join(format!("{sanitized}.{PROFILE_FILE_EXTENSION}"))
    }
}
```

Notes:

* Reuse the existing `sanitize_profile_name` and `PROFILE_FILE_EXTENSION` logic rather than duplicating it.
* Do not implement `AsRef<Path>`.

#### Step 1.4: Expose module from `core`

* In `src/core.rs`:

```rust
mod project_context;
pub use project_context::ProjectContext;
```

* Do not re-export internal constants or helpers outside of `core` unless strictly necessary.

---

### Phase 2: Refactor Core Managers (Trusted Consumers)

#### Step 2.1: Update `ProfileManagerOperations` trait

In `src/core/profiles.rs`, change method signatures to receive `&ProjectContext` instead of `&Path` where the parameter represents a project:

* Update:

```rust
fn load_profile(
    &self,
    project_root: &Path,
    profile_name: &str,
    app_name: &str,
) -> Result<Profile>;
```

to:

```rust
fn load_profile(
    &self,
    project: &ProjectContext,
    profile_name: &str,
    app_name: &str,
) -> Result<Profile>;
```

Apply similar changes to:

* `save_profile`
* `list_profiles`
* `get_profile_dir_path`
* `save_last_profile_name_for_project`
* `load_last_profile_name_for_project`

Keep methods that operate purely on explicit paths (e.g. `load_profile_from_path(&self, path: &Path)`) unchanged.

#### Step 2.2: Update `CoreProfileManager` implementation

Refactor `CoreProfileManager` as follows:

1. Replace any usage of `project_root: &Path` with `project: &ProjectContext`.

2. Keep directory-creation helpers, but make them call the resolvers instead of constructing paths manually:

```rust
fn ensure_project_config_dir(project: &ProjectContext) -> Option<PathBuf> {
    let config_dir = project.resolve_config_dir();
    // create_dir_all(config_dir.clone()) + logging
    Some(config_dir)
}

fn get_profile_storage_dir_impl(project: &ProjectContext) -> Option<PathBuf> {
    let config_dir = Self::ensure_project_config_dir(project)?;
    let profiles_dir = config_dir.join(PROFILES_SUBFOLDER_NAME);
    // create_dir_all(profiles_dir.clone()) + logging
    Some(profiles_dir)
}
```

3. In `load_profile` / `save_profile`:

* Use `project.resolve_profile_file(profile_name)` for the target file.
* Do not manually join strings or append `.json`.

4. In `list_profiles`:

* Use `get_profile_storage_dir_impl(project)` to find the directory.
* Enumerate `.json` files there as today.

5. In `save_last_profile_name_for_project` / `load_last_profile_name_for_project`:

* Use `project.resolve_last_profile_pointer_file()` as the location of `last_profile.txt`.

#### Step 2.3: Update `ConfigManagerOperations` trait

In `src/core/config.rs`, update:

```rust
fn save_last_project_path(
    &self,
    app_name: &str,
    project_root: Option<&Path>,
) -> Result<()>;
```

to:

```rust
fn save_last_project_path(
    &self,
    app_name: &str,
    project: Option<&ProjectContext>,
) -> Result<()>;
```

Other methods remain unchanged.

#### Step 2.4: Update `CoreConfigManager` implementation

Adjust `save_last_project_path`:

```rust
fn save_last_project_path(
    &self,
    app_name: &str,
    project: Option<&ProjectContext>,
) -> Result<()> {
    let path_opt = project.map(|ctx| ctx.resolve_root_for_serialization());
    // existing logic: write path_opt (or clear if None)
}
```

All serialization/deserialization logic continues to operate on `&Path`, but only within `core`.

---

### Phase 3: Refactor Application Logic (Controller)

#### Step 3.1: Cleanup `handler.rs`

* Remove the local `ProjectContext` definition from `src/app_logic/handler.rs`.
* Import `crate::core::ProjectContext`.
* Ensure `MyAppLogic`’s field remains:

```rust
active_project: Option<ProjectContext>,
```

#### Step 3.2: Update calls to managers

* Replace all usages of `self.active_project.as_ref().map(|p| p.as_ref())` with passing `&project_context` directly where non-optional, and `Option<&ProjectContext>` where optional.

Examples:

* Before:

```rust
if let Some(project_ctx) = self.active_project.as_ref() {
    self.profile_manager.save_profile(
        project_ctx.as_ref(),
        profile,
        APP_NAME_FOR_PROFILES,
    )?;
}
```

* After:

```rust
if let Some(project_ctx) = self.active_project.as_ref() {
    self.profile_manager.save_profile(
        project_ctx,
        profile,
        APP_NAME_FOR_PROFILES,
    )?;
}
```

* For config manager:

```rust
self.config_manager.save_last_project_path(
    APP_NAME_FOR_CONFIG,
    self.active_project.as_ref().map(|ctx| ctx),
)?;
```

#### Step 3.3: Handle initialization and folder selection

* Wherever a project path is obtained (folder picker dialog, last project from config, etc.), wrap it immediately:

```rust
let project_ctx = ProjectContext::new(selected_path);
self.active_project = Some(project_ctx);
```

* For “restore last project on startup”:

  * If `ConfigManager` returns a path, wrap it with `ProjectContext::new(path)` before assigning to `active_project`.

#### Step 3.4: UI updates

* In `_update_window_title_with_profile_and_archive` (and any similar functions), use `ctx.display_name()` instead of peeking at the root path.
* Remove any direct access to the project’s filesystem layout from `app_logic`.

---

### Phase 4: Test Suite and Invariant Lock-in

#### Step 4.1: New `ProjectContext` unit tests

In `project_context.rs` (or a dedicated `project_context_tests` module):

1. `test_resolvers_from_simple_root`:

   * Create `ProjectContext::new(PathBuf::from("/project"))`.
   * Assert:

     * `resolve_config_dir() == "/project/.sourcepacker"`.
     * `resolve_profiles_dir() == "/project/.sourcepacker/profiles"`.
     * `resolve_last_profile_pointer_file() == "/project/.sourcepacker/last_profile.txt"`.

2. `test_resolve_profile_file_uses_sanitization`:

   * Use a name like `"My Profile!"`.
   * Assert that the final path is in `profiles` under `.sourcepacker` and the filename equals `sanitize_profile_name("My Profile!") + ".json"`.

3. `test_display_name_returns_human_readable_name`:

   * Ensure `display_name()` matches the existing user-facing convention.

These tests lock in the topology and naming rules.

#### Step 4.2: Update `ProfileManager` tests

* Replace usages of `temp_dir.path()` with a `ProjectContext` built from that path:

```rust
let ctx = ProjectContext::new(temp_dir.path().to_path_buf());
```

* Pass `&ctx` to all manager methods under test.

Add or strengthen:

1. `test_core_profile_manager_uses_project_context_layout`:

   * Use `ctx.resolve_profile_file("My Profile")` to determine the expected file path.
   * Call `save_profile(&ctx, ...)`.
   * Assert that the file exists exactly at `ctx.resolve_profile_file("My Profile")`.

2. Ensure existing tests for:

   * Creating profile directories if missing.
   * Handling missing/empty `last_profile.txt`.

still pass after the refactor.

#### Step 4.3: Update `ConfigManager` tests

* Update tests that previously used `Option<&Path>` to use `Option<&ProjectContext>`.

Add:

* `test_save_last_project_path_serializes_context_root`:

  * Use a temp directory, build `ProjectContext`.
  * Call `save_last_project_path(app_name, Some(&ctx))`.
  * Read the stored config and assert it equals `ctx.resolve_root_for_serialization()`.

#### Step 4.4: Update `MyAppLogic` / handler tests

* Update mock `ProfileManager` and `ConfigManager` traits to accept `&ProjectContext`.
* Update helpers to construct a `ProjectContext` for tests that require an active project.

Add or adapt tests:

1. `test_profile_actions_no_active_project_do_not_call_managers`:

   * With `active_project = None`, invoke actions that should require a project.
   * Assert mocks for profile/config managers are not called.

2. `test_open_folder_sets_project_context_and_persists_last_project`:

   * Simulate folder selection.
   * Assert:

     * `active_project` is `Some(ProjectContext)` with the expected root.
     * `config_manager.save_last_project_path(...)` is called with that context.

#### Step 4.5: Clippy/quality checks

* Run `cargo test` and `cargo clippy`.
* Optionally, add a unit test or static assertion to ensure `ProjectContext` does not implement `AsRef<Path>` (e.g. by relying on compilation errors if someone tries to call `ctx.as_ref()`).

---

### Phase 5: Verification and Manual QA

#### Step 5.1: Compilation and code search

* Run `cargo check`.
* Search the codebase for:

  * `ProjectContext as_ref` or `project_ctx.as_ref()` and remove/replace all.
  * Direct usages of `.sourcepacker` and `profiles` outside of `core` and scanner-specific code, and consolidate them into `ProjectContext` if appropriate.

#### Step 5.2: Manual scenarios

Manually verify:

* Opening an existing project:

  * `.sourcepacker` and `profiles/` directories are created as needed.
  * Profiles load and save correctly.
* Creating a new project via folder picker:

  * `last_profile.txt` is maintained correctly.
* Switching between multiple projects:

  * Each project’s profiles and last profile pointer are isolated under that project’s `.sourcepacker`.

---

## 6. Future Extensions (Optional)

These are not part of the immediate refactor, but are enabled by this design:

1. Stronger domain types:

   * Introduce a `ProfileName` newtype with validation done once, instead of passing `&str` everywhere.
   * Introduce wrappers for `ProjectRelativePath` or similar to avoid mixing project-root-relative and absolute paths.

2. Recent projects / MRU list:

   * Extend `ConfigManager` to store a short MRU list of project roots.
   * Use `ProjectContext::new` on restore; drop entries that no longer exist.

3. Scanner integration:

   * If more logic around internal project directories emerges, consider exposing helpers on `ProjectContext` (e.g. “is internal project root child”) and use them in the scanner.

This completes the updated plan with the additional constraints and test hooks to keep the abstraction tight over time.
