## Step-by-Step Implementation Plan for a thrtee-state checkbox

**Goal:** Visually indicate "New" files (not in the loaded profile, or no profile loaded) with a small blue circle in the upper-left of their text in the TreeView. Standard checkbox for Selected/Deselected. "New" indicator disappears once the item is explicitly selected/deselected.

**Legend:**
*   **(C):** Core Layer (`src/core/`)
*   **(A):** Application Logic Layer (`src/app_logic/`)
*   **(P):** Platform Layer (`src/platform_layer/`)
*   **(U):** UI Description Layer (`src/ui_description_layer/`)
*   **(M):** Main (`src/main.rs`)

---

**Phase 1: Introduce "New" State in Core & App Logic (Non-Visual)**

**Step 1.1: (C) Extend `FileState` Enum**
*   **File:** `src/core/models.rs`
*   **Action:**
    *   Add the `New` variant to the `FileState` enum.
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum FileState {
        Selected,
        Deselected,
        New, // <-- Add this
        Unknown,
    }
    ```
*   **Goal:** Core data model now supports the "New" state.
*   **Functionality Check:** Application compiles and runs as before. The `New` state is unused.

**Step 1.2: (C) Modify `StateManager` to Initialize to `New` (Potentially)**
*   **File:** `src/core/state_manager.rs`
*   **Action:**
    *   In `CoreStateManager::apply_profile_to_tree`, if a node is *not* in `profile.selected_paths` and *not* in `profile.deselected_paths`, instead of setting its state to `Unknown`, set it to `FileState::New`.
    ```rust
    // In CoreStateManager::apply_profile_to_tree
    // ...
    } else {
        // node.state = FileState::Unknown; // Before
        node.state = FileState::New;      // After
    }
    // ...
    ```
    *   Also, consider how `FileNode::new` should initialize state. If `Unknown` is distinct from `New`, `Unknown` might be the initial state after a raw scan, and `New` is set after profile application for items not covered by the profile. For simplicity, let's assume `New` is the default if not in profile. If `FileNode::default()` (for `state`) sets `Unknown`, `apply_profile_to_tree` will correctly override it to `New`, `Selected`, or `Deselected`.
*   **Goal:** Files not explicitly covered by a loaded profile will internally be marked as `New`.
*   **Functionality Check:** Application compiles and runs. No visual change. Checksum calculations and token counting for "New" files will behave as they did for "Unknown" (likely treated as deselected for counting unless explicitly handled).

**Step 1.3: (A) Update `AppSessionData` for "New" State in Token Counting/Profile Saving**
*   **File:** `src/core/app_session_data.rs`
*   **Action:**
    *   **`update_token_count`:** Decide how "New" files contribute to the token count.
        *   **Option A (Simplest for now):** Treat `FileState::New` like `FileState::Deselected` (i.e., they don't contribute to the token count unless explicitly selected). No change needed if `update_token_count` only sums for `FileState::Selected`.
        *   **Option B (More complex):** Have a separate count for "New" files or include them by default. Let's stick with **Option A** for now to keep it simple.
    *   **`create_profile_from_session_state`:** When saving a profile, decide what happens to files still in `FileState::New`.
        *   **Option A (Simplest):** Treat them as implicitly deselected. They won't be added to `selected_paths` or `deselected_paths` if the gathering logic only looks for explicit `Selected`/`Deselected`.
        *   **Option B:** Explicitly add them to `deselected_paths`.
        Let's stick with **Option A**. The current `gather_selected_deselected_paths_recursive` only adds explicit `Selected` or `Deselected`. This means "New" items won't be in either set in the saved profile. When this profile is reloaded, these items will again be marked as "New" by `apply_profile_to_tree` (Step 1.2), which is reasonable.
    *   **`populate_file_details_recursive`:** Ensure this only caches details for `FileState::Selected`. This is likely already the case.
*   **Goal:** Internal logic for token counting and profile saving handles the `New` state consistently (as "not selected for current operation").
*   **Functionality Check:** Application compiles and runs. Token counts and saved profiles behave as if "New" files are deselected.

**Step 1.4: (A) Modify `MyAppLogic` to Transition "New" State on User Interaction**
*   **File:** `src/app_logic/handler.rs`
*   **Action:**
    *   In `handle_treeview_item_toggled`:
        *   When `AppEvent::TreeViewItemToggledByUser` is received for an item that was in `FileState::New`:
            *   If `new_state` (from UI) is `CheckState::Checked`, the `FileNode`'s model state should become `FileState::Selected`.
            *   If `new_state` (from UI) is `CheckState::Unchecked`, the `FileNode`'s model state should become `FileState::Deselected`.
            *   The `update_folder_selection` in `StateManagerOperations` will be called with the new `FileState::Selected` or `FileState::Deselected`. This part of the logic likely doesn't need to change much, as it already takes the target `FileState`.
    *   The key is that once a "New" item is explicitly toggled, it should *lose* its "New" status.
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
                *   Get `TreeItemId` from `nmcd.nmcd.lItemlParam`.
                *   Call `app_logic.is_tree_item_new(window_id, tree_item_id)`.
                *   If true, return `CDRF_NOTIFYPOSTPAINT` (to draw after default drawing).
                *   Else, return `CDRF_DODEFAULT`.
            *   **`CDDS_ITEMPOSTPAINT`:**
                *   If the item was marked as "New" in `ITEMPREPAINT` (you might need to store this temporarily, e.g., in a flag within `NativeWindowData` or by re-querying `app_logic`):
                    *   Get `HDC` from `nmcd.nmcd.hdc`.
                    *   Get the item's text rectangle: `TreeView_GetItemRect(nmcd.nmcd.hdr.hwndFrom, reinterpret_cast<HTREEITEM>(nmcd.nmcd.dwItemSpec), &item_text_rect, TRUE);` (Note: `dwItemSpec` is the `HTREEITEM` here).
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
            if original_node_state_was_new { // You'll need to fetch the node's state *before* updating it
                self.synchronous_command_queue.push_back(PlatformCommand::RedrawTreeItem {
                    window_id,
                    item_id, // The item_id of the toggled node
                });
            }
            ```
*   **File:** `src/platform_layer/command_executor.rs`
*   **Action:**
    *   Implement `execute_redraw_tree_item`:
        *   Get `hwnd_treeview` and `HTREEITEM` for the `item_id`.
        *   Call `TreeView_GetItemRect` to get the item's bounding box.
        *   Call `InvalidateRect(hwnd_treeview, &item_rect, TRUE)` to force a repaint of just that item area. (Or `TreeView_RedrawItem` if it exists and is simpler).
*   **Goal:** When a "New" item is toggled, the blue circle disappears immediately because the item is redrawn and `is_tree_item_new` now returns false for it.
*   **Functionality Check:** Blue circle appears on new items. When a "New" item's checkbox is clicked, the circle vanishes, and the checkbox state updates.

---

**Phase 3: Refinements & Testing**

**Step 3.1: Testing Edge Cases**
*   Test with no profile loaded (all files should be "New").
*   Test loading a profile with some files selected/deselected, and some new files on disk not in the profile.
*   Test saving a profile: ensure "New" items aren't saved as "Selected" unless explicitly made so.
*   Test toggling folders containing "New" items. The "New" status is per-item, not inherited for the indicator.
*   Test rapid toggling.
*   Check visual appearance of the circle (size, position, color).

**Step 3.2: Code Cleanup & Optimization**
*   Review `NM_CUSTOMDRAW` logic for efficiency.
*   Ensure all state transitions are handled correctly.
