## Step-by-Step Implementation Plan for a Three-State Checkbox

**Goal:** Visually indicate "New" files (not in the loaded profile, or no profile loaded) with a small blue circle in the upper-left of their text in the TreeView. Standard checkbox for Selected/Deselected. "New" indicator disappears once the item is explicitly selected/deselected.

**Legend:**
*   **(C):** Core Layer (`src/core/`)
*   **(A):** Application Logic Layer (`src/app_logic/`)
*   **(P):** Platform Layer (`src/platform_layer/`)
*   **(U):** UI Description Layer (`src/ui_description_layer/`)
*   **(M):** Main (`src/main.rs`)

---

**Phase 1: Introduce "New" State in Core & App Logic (Non-Visual)**

**Step 1.1: (C) Update `FileState` Enum and its Default**
*   **File:** `src/core/models.rs`
*   **Action:**
    *   Ensure the `FileState` enum consists of `Selected`, `Deselected`, and `New`. The `Unknown` variant is removed.
    *   Implement `Default` for `FileState` to return `FileState::New`.
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum FileState {
        Selected,
        Deselected,
        New, // Represents files not in the profile or when no profile is loaded.
    }

    impl Default for FileState {
        fn default() -> Self {
            FileState::New // "New" is the default state.
        }
    }
    ```
*   **File:** `src/core/models.rs` (within `FileNode::new`)
*   **Action:**
    *   Ensure `FileNode::new` initializes the `state` field using `FileState::default()`.
    ```rust
    // In FileNode::new
    // pub fn new(path: PathBuf, name: String, is_dir: bool) -> Self {
    //    FileNode {
    //        // ...
    //        state: FileState::default(), // Ensures state is initialized to FileState::New
    //        // ...
    //    }
    // }
    ```
*   **Goal:** Core data model uses `New` as the default state. `FileNode` instances are initialized with `FileState::New`.
*   **Functionality Check:** Application compiles and runs as before. The `New` state is now the default for newly scanned files.

**Step 1.2: (C) Modify `StateManager` to Handle `New` State Correctly During Profile Application**
*   **File:** `src/core/state_manager.rs`
*   **Action:**
    *   In `CoreStateManager::apply_profile_to_tree`, if a node is *not* in `profile.selected_paths` and *not* in `profile.deselected_paths`, its state should be `FileState::New`. Since `FileNode`s are already initialized to `New` (from Step 1.1), this part of the logic primarily serves to override the state to `Selected` or `Deselected` if specified by the profile. If not in the profile, the node's state effectively remains `New`.
    ```rust
    // In CoreStateManager::apply_profile_to_tree
    // ...
    } else {
        // If FileNode.state was initialized to New, and it's not in selected_paths
        // or deselected_paths, it remains New. This line ensures it's explicitly New
        // if it somehow wasn't already.
        node.state = FileState::New;
    }
    // ...
    ```
*   **Goal:** Files not explicitly covered by a loaded profile will be (or remain) marked as `New`.
*   **Functionality Check:** Application compiles and runs. No visual change. Token counting for "New" files will likely treat them as deselected unless explicitly handled (addressed in Step 1.3).

**Step 1.3: (A) Update `AppSessionData` for "New" State in Token Counting/Profile Saving**
*   **File:** `src/core/app_session_data.rs`
*   **Action:**
    *   **`update_token_count`:** Decide how "New" files contribute to the token count.
        *   **Option A (Chosen):** Treat `FileState::New` like `FileState::Deselected` (i.e., they don't contribute to the token count unless explicitly selected). No change needed if `update_token_count` only sums for `FileState::Selected`.
    *   **`create_profile_from_session_state`:** When saving a profile, what happens to files still in `FileState::New`.
        *   **Option A (Chosen):** Treat them as implicitly deselected. They won't be added to `selected_paths` or `deselected_paths` because the gathering logic in `gather_selected_deselected_paths_recursive` only adds explicit `Selected` or `Deselected` states.
        *   **Implication:** This means "New" items (that haven't been toggled by the user) won't be in either the `selected_paths` or `deselected_paths` set in the saved profile. When this profile is reloaded, these items will again be marked as `FileState::New` by `apply_profile_to_tree` (Step 1.2), fulfilling the requirement that they remain "New" on the next application start if not interacted with.
    *   **`populate_file_details_recursive`:** Ensure this only caches details for `FileState::Selected`. This is likely already the case.
*   **Goal:** Internal logic for token counting and profile saving handles the `New` state consistently (as "not selected for current operation/profile saving"). "New" files are not persisted in profiles and remain "New" across sessions if not interacted with.
*   **Functionality Check:** Application compiles and runs. Token counts and saved profiles behave as if "New" files are deselected. Reloading a profile correctly shows previously "New" (and untoggled) files as "New" again.

**Step 1.4: (A) Modify `MyAppLogic` to Transition "New" State on User Interaction**
*   **File:** `src/app_logic/handler.rs`
*   **Action:**
    *   In `handle_treeview_item_toggled`:
        *   When `AppEvent::TreeViewItemToggledByUser` is received for an item that was in `FileState::New`:
            *   If `new_state` (from UI) is `CheckState::Checked`, the `FileNode`'s model state should become `FileState::Selected`.
            *   If `new_state` (from UI) is `CheckState::Unchecked`, the `FileNode`'s model state should become `FileState::Deselected`.
            *   The `update_folder_selection` in `StateManagerOperations` will be called with the new `FileState::Selected` or `FileState::Deselected`. This part of the logic likely doesn't need to change much, as it already takes the target `FileState`.
    *   The key is that once a "New" item is explicitly toggled, it should *lose* its "New" status for the current session.
*   **Goal:** Interacting with a "New" item changes its state to Selected/Deselected permanently for the session.
*   **Functionality Check:** Application compiles and runs. No visual change for "New" yet. Internally, toggling an item that would have been "New" now correctly sets it to Selected/Deselected.

**Phase 2: Visual Indicator (The Blue Circle)**

**Step 2.1: (P) Prepare `NativeWindowData` to Support "New" State Query for Drawing**
*   **File:** `src/platform_layer/window_common.rs`
*   **Action:** No direct change to `NativeWindowData` is strictly needed for this if the query mechanism goes through `AppEvent` or a new method on `PlatformEventHandler`. However, `handle_wm_notify` for `NM_CUSTOMDRAW` will need to access `app_logic` to determine if an item is "New".

**Step 2.2: (A) Add a Way for Platform Layer to Query "New" Status**
*   **File:** `src/platform_layer/types.rs` (for `PlatformEventHandler` trait) and `src/app_logic/handler.rs` (for `MyAppLogic` impl)
*   **Action:**
    *   Add a new method to the `PlatformEventHandler` trait:
      ```rust
      // In PlatformEventHandler trait
      fn is_tree_item_new(&self, window_id: WindowId, item_id: TreeItemId) -> bool;
      ```
    *   Implement this method in `MyAppLogic`:
      ```rust
      // In MyAppLogic impl PlatformEventHandler
      fn is_tree_item_new(&self, window_id: WindowId, item_id: TreeItemId) -> bool {
          // 1. Find path for item_id from ui_state.path_to_tree_item_id
          // 2. Find FileNode for path from app_session_data.file_nodes_cache
          // 3. Return true if node.state == FileState::New, else false
          // Ensure ui_state and window_id match.
          if let Some(ui_state) = &self.ui_state {
              if ui_state.window_id == window_id {
                  // Find path for item_id
                  let path_opt = ui_state.path_to_tree_item_id.iter()
                      .find(|(_path, id_in_map)| **id_in_map == item_id)
                      .map(|(path_candidate, _id_in_map)| path_candidate);

                  if let Some(path) = path_opt {
                      if let Some(node) = Self::find_filenode_ref(&self.app_session_data.file_nodes_cache, path) {
                          return node.state == FileState::New;
                      }
                  }
              }
          }
          false // Default if not found or state doesn't match
      }
      ```
*   **Goal:** Platform layer can ask app logic if a specific tree item should get the "New" indicator.
*   **Functionality Check:** Application compiles and runs. No visual change yet.

**Step 2.3: (P) Implement `NM_CUSTOMDRAW` for TreeView for "New" Indicator**
*   **File:** `src/platform_layer/window_common.rs` (in `Win32ApiInternalState::handle_window_message` for `WM_NOTIFY`)
*   **Action:**
    *   Modify the `WM_NOTIFY` handler. If `nmhdr.code` is `NM_CUSTOMDRAW` and `nmhdr.idFrom` is `ID_TREEVIEW_CTRL`:
        *   Cast `lparam` to `LPNMTVCUSTOMDRAW` (or `NMTVCUSTOMDRAW*`).
        *   Switch on `nmcd.nmcd.dwDrawStage`:
            *   **`CDDS_PREPAINT`:** Return `CDRF_NOTIFYITEMDRAW`.
            *   **`CDDS_ITEMPREPAINT`:**
                *   Get `TreeItemId` from `nmcd.nmcd.lItemlParam` (this is the `HTREEITEM`, so you'll need to map it back to `TreeItemId` via `NativeWindowData::treeview_state::htreeitem_to_item_id` if querying `app_logic` directly, or if `is_tree_item_new` expects `HTREEITEM` that needs to be documented). A simpler approach if `app_logic.is_tree_item_new` expects `TreeItemId` (as defined) is to call it with the `TreeItemId` associated with `nmcd.nmcd.dwItemSpec` if `dwDrawStage == CDDS_ITEMPREPAINT`. `dwItemSpec` is the `lParam` of the item which we set to `TreeItemId.0` in `add_item_recursive_impl`.
                *   If `nmcd.nmcd.lItemlParam` indeed holds the `TreeItemId.0` (confirm this, as `dwItemSpec` is usually `HTREEITEM` for `CDDS_ITEMPOSTPAINT`, but `lItemlParam` is the application-defined data for `CDDS_ITEMPREPAINT` and `CDDS_ITEMPOSTPAINT` stages for the item). The `TVITEMEX.lParam` is set to `item_desc.id.0`, so `nmcd.nmcd.lItemlParam` should be `TreeItemId.0`.
                *   Call `app_logic.is_tree_item_new(window_id, TreeItemId(nmcd.nmcd.lItemlParam as u64))`.
                *   If true, return `CDRF_NOTIFYPOSTPAINT` (to draw after default drawing).
                *   Else, return `CDRF_DODEFAULT`.
            *   **`CDDS_ITEMPOSTPAINT`:**
                *   If the item was marked as "New" in `ITEMPREPAINT` (you might need to store this temporarily, e.g., in a flag within `NativeWindowData` or by re-querying `app_logic` using `TreeItemId(nmcd.nmcd.lItemlParam as u64)`):
                    *   Get `HDC` from `nmcd.nmcd.hdc`.
                    *   Get the item's text rectangle: `TreeView_GetItemRect(nmcd.nmcd.hdr.hwndFrom, reinterpret_cast<HTREEITEM>(nmcd.dwItemSpec), &item_text_rect, TRUE);` (Note: `dwItemSpec` is the `HTREEITEM` here, not `lItemlParam`).
                    *   Define circle properties (small radius, blue color).
                    *   Create a blue brush: `CreateSolidBrush(RGB(0,0,255))`.
                    *   Select the brush into `HDC`.
                    *   Calculate circle position (e.g., `item_text_rect.left`, `item_text_rect.top`, adjusting slightly if needed).
                    *   Draw the ellipse: `Ellipse(hdc, x, y, x + diameter, y + diameter)`.
                    *   Restore old brush, delete created brush.
                *   Return `CDRF_DODEFAULT`.
*   **Goal:** "New" files now have a blue circle drawn over their text.
*   **Functionality Check:** Compile and run. New files (those not in a loaded profile, or when no profile is active) should show the blue circle. Other files should not. Checkboxes still function for Selected/Deselected.

**Step 2.4: (A) Ensure TreeView Item Redraw When "New" Status Changes**
*   **File:** `src/app_logic/handler.rs`
*   **Action:**
    *   In `handle_treeview_item_toggled`:
        *   After the `FileNode`'s state is updated from `New` to `Selected` or `Deselected`:
        *   The existing call to `self.collect_visual_updates_recursive` *should* update the checkbox.
        *   **Crucially, we also need to tell the platform to redraw the item itself so the custom draw logic (for the blue circle) is re-evaluated.**
        *   Add a new `PlatformCommand`:
            ```rust
            // In src/platform_layer/types.rs (PlatformCommand enum)
            RedrawTreeItem { window_id: WindowId, item_id: TreeItemId },
            ```
        *   Queue this command in `MyAppLogic::handle_treeview_item_toggled` if the item's state *was* `New` and just changed.
            ```rust
            // In MyAppLogic::handle_treeview_item_toggled, after model update and visual_updates_list collection
            // You'll need to fetch the node's state *before* updating it.
            // e.g., let original_node_state_was_new = node_model.state == FileState::New;
            if original_node_state_was_new {
                self.synchronous_command_queue.push_back(PlatformCommand::RedrawTreeItem {
                    window_id,
                    item_id, // The item_id of the toggled node
                });
            }
            ```
*   **File:** `src/platform_layer/command_executor.rs`
*   **Action:**
    *   Implement `execute_redraw_tree_item`:
        *   Retrieve `hwnd_treeview` and `HTREEITEM` for the `item_id` (from `NativeWindowData::treeview_state::item_id_to_htreeitem`).
        *   Call `TreeView_GetItemRect` to get the item's bounding box using the `HTREEITEM`.
        *   Call `InvalidateRect(hwnd_treeview, &item_rect, TRUE)` to force a repaint of just that item area.
*   **Goal:** When a "New" item is toggled, the blue circle disappears immediately because the item is redrawn and `is_tree_item_new` now returns false for it.
*   **Functionality Check:** Blue circle appears on new items. When a "New" item's checkbox is clicked, the circle vanishes, and the checkbox state updates.

---

**Phase 3: Refinements & Testing**

**Step 3.1: Testing Edge Cases**
*   Test with no profile loaded (all files should be "New" and show the circle).
*   Test loading a profile:
    *   Files selected/deselected in profile: No circle.
    *   New files on disk not in profile: Should be "New" and show the circle.
*   Test saving a profile: ensure "New" items aren't saved as "Selected" unless explicitly made so. Confirm they remain "New" on next load if not toggled.
*   Test toggling folders containing "New" items. The "New" status is per-item, not inherited for the indicator.
*   Test rapid toggling of "New" items.
*   Check visual appearance of the circle (size, position, color).

**Step 3.2: Code Cleanup & Optimization**
*   Review `NM_CUSTOMDRAW` logic for efficiency. Ensure correct `lItemlParam` vs `dwItemSpec` usage.
*   Ensure all state transitions (`New` -> `Selected`, `New` -> `Deselected`) are handled correctly and visuals update promptly.
