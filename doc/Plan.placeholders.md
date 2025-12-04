Below is a concrete, incremental implementation plan that keeps the app compiling and runnable after every step, uses a `SummaryMode` enum instead of a bare `HashSet<PathBuf>`, and avoids leaking internal struct state.

I assume we stick with the overall design from `doc\Plan.placeholders.md` but switch to “Option B with SummaryMode”.

---

## Phase 0 – Update documentation (no code impact)

**Step 0.1 – Requirements + plan docs**

* Update `doc\Requirements.md` (or corresponding doc) with new requirement tags, for example:

  * `[PlaceholderFolderSummaryV1]` – A folder can be exported as a placeholder summary instead of full contents.
  * `[PlaceholderSummaryStorageProjectLocalV1]` – Summary markdown stored under `.sourcepacker\summaries\...` mirroring the project tree.
  * `[UiTreeViewSummaryVisualV1]` – Summary folders are visually distinguishable in the TreeView.
* Update `doc\Plan.placeholders.md`:

  * Explicitly say we choose the “SummaryMode / Option B” design instead of a `HashSet<PathBuf>`.
  * Describe semantics:

    * `SummaryMode::Full` – normal export.
    * `SummaryMode::SummaryOnly` – export only the placeholder markdown for that folder; children are not exported.
  * Keep the description of how summaries are stored under `.sourcepacker\summaries\...`.

Application behavior is unchanged.

---

## Phase 1 – Domain model: SummaryMode and profile persistence

### Step 1.1 – Introduce `SummaryMode` enum (core only)

**Goal:** Introduce a small, encapsulated domain type.

**Changes**

* New file `src/core/summary_mode.rs` (or add near `SelectionState` in `file_node.rs`):

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
  pub enum SummaryMode {
      Full,
      SummaryOnly,
  }
  ```

* Export from `core` module (`lib.rs` or `mod core`) so other core types can use it.

**Tests**

* Simple unit test for `SummaryMode` serialization/deserialization round-trip.

No other code uses it yet; the application still runs as today.

---

### Step 1.2 – Add summary modes to `Profile` with strong encapsulation

**Goal:** Persist per-folder summary modes without leaking internal representation.

**Changes**

* In `src/core/profile.rs` (where `Profile` is defined ):

  * Add a private field:

    ```rust
    use crate::core::summary_mode::SummaryMode;
    use std::collections::HashMap;
    use std::path::PathBuf;

    pub struct Profile {
        pub name: ProfileName,
        pub root_folder: PathBuf,
        pub selected_paths: HashSet<PathBuf>,
        pub deselected_paths: HashSet<PathBuf>,
        pub archive_path: Option<PathBuf>,
        #[serde(default)]
        pub file_details: HashMap<PathBuf, FileTokenDetails>,
        #[serde(default)]
        pub exclude_patterns: Vec<String>,

        #[serde(default)]
        #[serde(skip_serializing_if = "HashMap::is_empty")]
        summary_modes: HashMap<PathBuf, SummaryMode>,
    }
    ```

    Note: `summary_modes` stays private; only methods expose it.

* In `impl Profile`:

  ```rust
  impl Profile {
      pub fn summary_mode_for(&self, path: &Path) -> SummaryMode {
          self.summary_modes
              .get(path)
              .copied()
              .unwrap_or(SummaryMode::Full)
      }

      pub fn set_summary_mode(&mut self, path: PathBuf, mode: SummaryMode) {
          match mode {
              SummaryMode::Full => {
                  self.summary_modes.remove(&path);
              }
              _ => {
                  self.summary_modes.insert(path, mode);
              }
          }
      }

      pub fn clear_all_summary_modes(&mut self) {
          self.summary_modes.clear();
      }

      #[cfg(test)]
      pub(crate) fn summary_modes_for_test(&self) -> &HashMap<PathBuf, SummaryMode> {
          &self.summary_modes
      }
  }
  ```

* Update `Profile::new` to initialize `summary_modes` as `HashMap::new()`.

**Tests**

* Extend existing `Profile` tests (if any) to assert a new profile has no summary modes.
* Add a small test verifying:

  * `summary_mode_for(path)` returns `Full` by default.
  * After `set_summary_mode(path, SummaryOnly)`, the getter returns `SummaryOnly`.
  * After `set_summary_mode(path, Full)`, the entry is removed.

Behavior: still no effect on packing; nothing yet reads `summary_modes`.

---

### Step 1.3 – Mirror summary modes in `ProfileRuntimeData`

**Goal:** Keep runtime/session state and persisted profiles consistent, still encapsulated.

**Changes**

* In `src/core/profile_runtime_data.rs` add:

  ```rust
  use crate::core::summary_mode::SummaryMode;
  use std::collections::HashMap;

  pub struct ProfileRuntimeData {
      // existing fields...
      summary_modes: HashMap<PathBuf, SummaryMode>,
  }
  ```

* In `impl ProfileRuntimeData::new` initialize `summary_modes` to `HashMap::new()`.

* In `ProfileRuntimeDataOperations` trait, add:

  ```rust
  fn get_summary_mode_for_path(&self, path: &Path) -> SummaryMode;
  fn set_summary_mode_for_path(&mut self, path: PathBuf, mode: SummaryMode);
  fn get_all_summary_modes(&self) -> HashMap<PathBuf, SummaryMode>; // for snapshot
  ```

* Implement these in `ProfileRuntimeData` using the private `summary_modes` map, with same “remove on Full” semantics as `Profile`.

* Update `create_profile_snapshot` to pull summary modes from runtime:

  ```rust
  let summary_modes = self.summary_modes.clone();
  let mut profile = Profile::new(name, root_folder);
  // ... existing fields ...
  for (path, mode) in summary_modes {
      profile.set_summary_mode(path, mode);
  }
  ```

* Update `load_profile_into_session` to copy summary modes from `Profile` into `ProfileRuntimeData`.

* Update `MockProfileRuntimeDataOps` implementations (used in UI tests) to either:

  * Implement the new trait methods with simple in-memory maps, or
  * Panic with `unimplemented!()` if they are never called by current tests (simpler for now).

**Tests**

* Extend existing `ProfileRuntimeData` tests to verify round-trip:

  * Set a `SummaryOnly` mode in runtime.
  * Create snapshot.
  * Ensure `Profile::summary_mode_for(path)` returns `SummaryOnly`.

Still no change to archive behavior or UI.

---

## Phase 2 – Summary file storage and access

### Step 2.1 – Extend `ProjectContext` with summaries directory

**Goal:** Centralize the path logic for `.sourcepacker\summaries`.

**Changes**

* In `src/core/project_context.rs` add:

  ```rust
  impl ProjectContext {
      pub fn summaries_dir(&self) -> PathBuf {
          self.root_path()
              .join(".sourcepacker")
              .join("summaries")
      }
  }
  ```

* Optionally a helper:

  ```rust
      pub fn ensure_summaries_dir_exists(&self) -> std::io::Result<PathBuf> {
          let dir = self.summaries_dir();
          std::fs::create_dir_all(&dir)?;
          Ok(dir)
      }
  ```

**Tests**

* Add unit test (using temp dir) verifying the computed path and creation behavior.

---

### Step 2.2 – Introduce `SummaryManager` (core service)

**Goal:** Encapsulate how folder paths map to markdown files and how they are read.

**Changes**

* New file `src/core/summary_manager.rs`:

  ```rust
  pub trait SummaryManagerOperations: Send + Sync {
      fn resolve_summary_path(
          &self,
          project: &ProjectContext,
          folder: &Path,
      ) -> Option<PathBuf>;

      fn load_summary_text(
          &self,
          project: &ProjectContext,
          folder: &Path,
      ) -> std::io::Result<Option<String>>;
  }

  pub struct CoreSummaryManager;

  impl SummaryManagerOperations for CoreSummaryManager {
      fn resolve_summary_path(
          &self,
          project: &ProjectContext,
          folder: &Path,
      ) -> Option<PathBuf> {
          // MVP: only support folders under project root
          let rel = folder.strip_prefix(project.root_path()).ok()?;
          let mut path = project.summaries_dir();
          for comp in rel.components() {
              path.push(comp);
          }
          path.set_extension("md");
          Some(path)
      }

      fn load_summary_text(
          &self,
          project: &ProjectContext,
          folder: &Path,
      ) -> std::io::Result<Option<String>> {
          if let Some(summary_path) = self.resolve_summary_path(project, folder) {
              match std::fs::read_to_string(&summary_path) {
                  Ok(text) => Ok(Some(text)),
                  Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                  Err(e) => Err(e),
              }
          } else {
              Ok(None)
          }
      }
  }
  ```

* Wire `CoreSummaryManager` into your DI container (where you construct other core services), but don’t use it yet.

**Tests**

* Unit tests for `CoreSummaryManager::resolve_summary_path` and `load_summary_text` using a temporary project root and synthetic folder tree.
* Confirm that folders outside project root simply return `None`.

App still behaves as before.

---

## Phase 3 – Archiver: support SummaryMode, no UI yet

### Step 3.1 – Extend Archiver operations to consider SummaryMode

**Goal:** Implement the core behavior: when a folder is `SummaryOnly`, include only its markdown and skip descendants.

**Changes**

* In `src/core/archiver.rs` (where `CoreArchiver` is) :

  * Inject `SummaryManagerOperations` and `ProfileRuntimeDataOperations` (which now know `SummaryMode`):

    ```rust
    pub struct CoreArchiver<'a> {
        project: &'a ProjectContext,
        summary_manager: &'a dyn SummaryManagerOperations,
        // existing fields...
    }
    ```

    Or keep the existing struct and pass `summary_manager` & `runtime_data` into `create_content` as parameters if that fits your current design better.

  * In the recursion that walks `FileNode`s to emit archive content:

    ```rust
    fn add_node_to_archive(
        &mut self,
        runtime: &dyn ProfileRuntimeDataOperations,
        node: &FileNode,
        /* ...whatever you already pass... */
    ) -> Result<(), Error> {
        let mode = runtime.get_summary_mode_for_path(node.path());
        let state = node.selection_state();

        if !state.is_selected() {
            return Ok(());
        }

        if node.is_dir() && matches!(mode, SummaryMode::SummaryOnly) {
            // Folder is selected and summarized: emit summary markdown only
            if let Some(summary_text) = self
                .summary_manager
                .load_summary_text(self.project, node.path())?
            {
                self.write_summary_entry(node.path(), &summary_text)?;
            } else {
                // MVP: log warning and skip completely, or fall back to full content
                log::warn!(
                    "No summary markdown found for summarized folder {:?}. Skipping.",
                    node.path()
                );
            }
            // Critical: DO NOT recurse into children
            return Ok(());
        }

        // Existing handling for normal files and directories (recursing into children).
    }
    ```

  * Implement `write_summary_entry(...)` to add an entry into the archive that corresponds to that folder (e.g., using same relative path but `.md` file name).

**Tests**

* New unit test in archiver module:

  * Build a small `FileNode` tree with a root folder and a child file.
  * Mock `ProfileRuntimeDataOperations` to return `SummaryMode::SummaryOnly` for the folder.
  * Mock `SummaryManagerOperations::load_summary_text` to return known markdown text.
  * Verify the archive contains exactly:

    * The summary markdown.
    * No entry for the child file.
* Existing archiver tests must still pass when `SummaryMode::Full` (default) is used.

Even now there is no UI to change `SummaryMode`; everything runs like before.

---

## Phase 4 – UI support: visualizing summaries

### Step 4.1 – Platform layer: allow per-item style override

**Goal:** TreeView items can be visually marked as “summary folders” without leaking core state.

**Changes**

* Locate `TreeItemDescriptor` in `platform_layer::types` and add a new field `style_override: Option<StyleId>`, keeping it private or at least not mutated elsewhere.

  ```rust
  pub struct TreeItemDescriptor {
      pub id: TreeItemId,
      pub is_folder: bool,
      pub children: Vec<TreeItemDescriptor>,
      pub text: String,
      pub state: CheckState,
      pub style_override: Option<StyleId>, // new
  }
  ```

* Update any `TreeItemDescriptor` constructors (e.g., in `FileNode::new_tree_item_descriptor`) to set `style_override: None` for now.

* Extend `PlatformCommand::PopulateTreeView` handling so the platform layer applies `style_override` if present; otherwise it uses default TreeView styling.

**Tests**

* Existing tests that construct `TreeItemDescriptor` are updated to set `style_override: None`.
* Add a small platform-layer test (if applicable) to ensure a descriptor with `Some(StyleId::...)` uses the override.

Application behavior still the same; all descriptors use `None`.

---

### Step 4.2 – Define a style for summary folders

**Goal:** Give summary folders a distinct, but non-intrusive, look.

**Changes**

* Extend `StyleId` enum with a new value like `SummaryFolderText` (or similar).
* In `MyAppLogic::define_styles`, define a style for it (e.g. same font, slightly different text color).

  ```rust
  self.synchronous_command_queue.push_back(PlatformCommand::DefineStyle {
      style_id: StyleId::SummaryFolderText,
      style: ControlStyle {
          text_color: Some(text_warning), // or a subtle variant
          font: Some(default_font.clone()),
          background_color: None,
      },
  });
  ```

No summary items use this yet; application still behaves normally.

---

### Step 4.3 – Mark TreeView descriptors based on SummaryMode

**Goal:** Summary folders become visible in the TreeView as special items.

**Changes**

* `MainWindowUiState::rebuild_tree_descriptors` currently just uses pure `FileNode` data.

  * Here, we keep it pure and unaware of `SummaryMode` to maintain separation of concerns.

* Instead, adjust `FileNode::build_tree_item_descriptors_recursive`/`new_tree_item_descriptor` to accept an optional callback that can compute the style override:

  ```rust
  pub fn build_tree_item_descriptors_recursive_with_style<F>(
      nodes: &[FileNode],
      path_to_id: &mut PathToTreeItemIdMap,
      next_id: &mut u64,
      style_for_path: &F,
  ) -> Vec<TreeItemDescriptor>
  where
      F: Fn(&Path) -> Option<StyleId>,
  {
      // In the recursion, when constructing a descriptor:
      let style_override = style_for_path(node.path());
      // pass style_override into new_tree_item_descriptor
  }
  ```

* Modify `MainWindowUiState::rebuild_tree_descriptors` to call this overload, but it does not know `SummaryMode` itself. Instead, it receives a closure or adapter from `MyAppLogic`:

  * Add a new method in `MainWindowUiState`:

    ```rust
    pub fn rebuild_tree_descriptors_with_style<F>(
        &mut self,
        snapshot_nodes: &[FileNode],
        style_for_path: &F,
    ) -> Vec<TreeItemDescriptor>
    where
        F: Fn(&Path) -> Option<StyleId>,
    {
        // Same logic as rebuild_tree_descriptors, but where it eventually
        // calls `build_tree_item_descriptors_recursive_with_style`.
    }
    ```

* In `MyAppLogic::repopulate_tree_view` :

  ```rust
  fn repopulate_tree_view(&mut self, window_id: WindowId) {
      let ui_state = ...;
      let snapshot_nodes = ...;

      // We have app_session_data_ops (ProfileRuntimeDataOperations)
      let summary_style = |path: &Path| {
          let app_data = self.app_session_data_ops.lock().unwrap();
          match app_data.get_summary_mode_for_path(path) {
              SummaryMode::SummaryOnly => Some(StyleId::SummaryFolderText),
              SummaryMode::Full => None,
          }
      };

      let items_to_use = ui_state.rebuild_tree_descriptors_with_style(&snapshot_nodes, &summary_style);

      self.synchronous_command_queue.push_back(PlatformCommand::PopulateTreeView { ... });
  }
  ```

**Tests**

* New test for `rebuild_tree_descriptors_with_style` using a closure that returns `Some(StyleId::SummaryFolderText)` for one path; assert the corresponding descriptor has `style_override` set.
* New integration-style test for `MyAppLogic::repopulate_tree_view` with a mock `ProfileRuntimeDataOps` returning `SummaryMode::SummaryOnly` for one folder, asserting the generated `PopulateTreeView` command carries a descriptor with the overridden style.

This makes summary folders visually distinct, even before we expose a UX for changing the mode.

---

## Phase 5 – UX: mark/unmark summary folders

### Step 5.1 – Add UI affordance (context menu or button)

**Goal:** Let the user choose “Replace this folder with placeholder” and “Back to full folder.”

**Changes**

* Add new menu action IDs in `ui_constants.rs`, e.g.:

  ```rust
  pub const MENU_ACTION_MARK_SUMMARY: MenuActionId = MenuActionId::new(3001);
  pub const MENU_ACTION_CLEAR_SUMMARY: MenuActionId = MenuActionId::new(3002);
  ```

* Update the appropriate menu-building code (likely in `ui_description_layer`) to:

  * Add a “Mark as placeholder summary” and “Clear placeholder summary” entries in the context menu for TreeView.

* In `MyAppLogic::handle_event`, handle these actions:

  * Resolve the currently selected `TreeItemId` to a path via `MainWindowUiState::path_for_tree_item`.
  * Confirm that path corresponds to a directory (using `get_node_attributes_for_path`).
  * Call `set_summary_mode_for_path(path.clone(), SummaryMode::SummaryOnly)` or `SummaryMode::Full` on `ProfileRuntimeDataOperations`.
  * Trigger:

    * `repopulate_tree_view(window_id);`
    * `update_current_archive_status();`
    * `_update_token_count_and_request_display();` (optional but nice for immediate feedback).

**Tests**

* Add test in `handler_tests.rs` or equivalent:

  * Simulate a context menu “Mark as placeholder” click on a folder.
  * Use a mock `ProfileRuntimeDataOps` to assert `set_summary_mode_for_path` was called with the correct path and `SummaryMode::SummaryOnly`.
  * Assert a `PopulateTreeView` command is enqueued (TreeView updates).
* Similar for “Clear placeholder summary”.

---

### Step 5.2 – Persistence round-trip with profiles

**Goal:** Ensure summary modes survive save/load.

**Changes**

* Existing profile save/load code (`ProfileManager`) already serializes/deserializes the new `summary_modes` field through the `Profile` struct; no further changes needed if you did Step 1.2 correctly.

**Tests**

* End-to-end test:

  * Create a `ProfileRuntimeData` with a `SummaryOnly` mode on a folder.
  * Snapshot to a `Profile`.
  * Save the profile with `ProfileManager` to disk.
  * Load the profile again.
  * Load the profile into runtime and check `get_summary_mode_for_path` returns `SummaryOnly`.

At this point, the full feature works end-to-end for existing placeholder files.

---

## Phase 6 – Diagnostics and future extensions

These are optional but useful steps once the MVP is in place.

### Step 6.1 – Diagnostics in status bar

* Extend archive status or a separate label to show:

  * Number of folders currently summarized.
  * Warnings when a `SummaryOnly` folder has no `.md` summary file (detected in archiver via `SummaryManager`).

### Step 6.2 – Future `SummaryMode` variants

Once the basic flow is stable, consider adding more modes:

```rust
pub enum SummaryMode {
    Full,
    SummaryOnly,
    SummaryWithPinnedChildren, // future
}
```

Where `SummaryWithPinnedChildren` could later be extended to carry a list of pinned child paths; when that happens, add a small struct to store both mode and pinned children and keep it encapsulated (never expose raw maps).

---

This sequence keeps the application buildable and runnable after every step, introduces `SummaryMode` as a clean domain concept instead of a bare `HashSet<PathBuf>`, keeps internal maps and structs encapsulated, and gradually wires summary placeholders through core, storage, archiver, and UI.
