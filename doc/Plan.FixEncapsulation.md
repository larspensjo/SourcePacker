### **Detailed Encapsulation Plan**

This plan proceeds struct by struct, starting from the platform layer and moving up to the application logic. For each struct, we will:
1.  Make all fields private.
2.  Identify where the fields were being accessed directly.
3.  Create new public methods that not only replace that direct access but also centralize the associated logic, thereby enforcing the struct's invariants.
4.  Highlight the new opportunities for unit testing that this refactoring creates.

---

#### **1. `NativeWindowData` - The Foundation of a Window**

**Role:** Manages all native Win32 handles (`HWND`, `HFONT`) and mappings (`control_id -> HWND`) for a single window.
**Core Invariant:** GDI resources like fonts and brushes must be deleted when they are replaced or when the window is destroyed to prevent resource leaks. Control and menu mappings must remain consistent.

##### **Step-by-Step Refactoring (`src/platform_layer/window_common.rs`):**

1.  **Make all fields in `NativeWindowData` private.** Remove `pub(crate)`.

2.  **Replace Direct Access with Methods:** The compiler will now show errors in `command_executor.rs`, `app.rs`, and various control handlers.

3.  **Encapsulate Resource Management (`status_bar_font`, `treeview_new_item_font`):**
    *   The logic in `ensure_status_bar_font()` and `cleanup_status_bar_font()` should be moved *inside* `NativeWindowData`. The `Drop` trait will ensure cleanup is automatic.

    **Before:**
    ```rust
    // in window_common.rs, a free-standing function
    pub(crate) fn cleanup_status_bar_font(&mut self) { /* ... */ }
    ```

    **After (in `impl NativeWindowData`):**
    ```rust
    impl NativeWindowData {
        // ... other methods

        // This method now contains the font creation logic.
        pub(crate) fn ensure_status_bar_font(&mut self) {
            if self.status_bar_font.is_some() { return; }
            // ... (font creation logic from the old global function) ...
            self.status_bar_font = Some(h_font);
        }

        // Getter remains simple.
        pub(crate) fn status_bar_font(&self) -> Option<HFONT> {
            self.status_bar_font
        }

        // Private cleanup helper.
        fn cleanup_status_bar_font(&mut self) {
            if let Some(h_font) = self.status_bar_font.take() {
                // ... (DeleteObject logic) ...
            }
        }
    }

    impl Drop for NativeWindowData {
        fn drop(&mut self) {
            self.cleanup_status_bar_font();
            self.cleanup_treeview_new_item_font();
            log::debug!("NativeWindowData for WinID {:?} dropped, resources cleaned up.", self.logical_window_id);
        }
    }
    ```

4.  **Encapsulate Map-like Fields (`control_hwnd_map`, `menu_action_map`):** Do not expose the `HashMap`s directly. Provide methods for the specific operations needed.

    **Before:**
    ```rust
    // Other modules directly inserting into the map.
    window_data.control_hwnd_map.insert(id, hwnd);
    ```

    **After (in `impl NativeWindowData`):**
    ```rust
    // ...
    pub(crate) fn register_control(&mut self, id: i32, hwnd: HWND) {
        self.control_hwnd_map.insert(id, hwnd);
    }

    pub(crate) fn get_control_hwnd(&self, id: i32) -> Option<HWND> {
        self.control_hwnd_map.get(&id).copied()
    }

    // The logic of generating a new ID now belongs to the struct itself.
    pub(crate) fn register_menu_action(&mut self, action: MenuAction) -> i32 {
        let id = self.next_menu_item_id_counter;
        self.next_menu_item_id_counter += 1;
        self.menu_action_map.insert(id, action);
        id
    }
    // ...
    ```

5.  **Encapsulate Layout Logic (`layout_rules`):** The complex logic for applying layout rules is a perfect candidate to be a method on `NativeWindowData`.

    **Before:**
    ```rust
    // A method on Win32ApiInternalState that reaches into NativeWindowData.
    fn trigger_layout_recalculation(&self, window_id: WindowId) {
        // ... reads NativeWindowData, gets client_rect, then calls...
        window_data.apply_layout_rules_for_children(None, client_rect);
    }
    ```

    **After (in `impl NativeWindowData`):**
    ```rust
    // ...
    pub(crate) fn define_layout(&mut self, rules: Vec<LayoutRule>) {
        self.layout_rules = Some(rules);
    }

    // This is the new public entry point for layout.
    pub(crate) fn recalculate_and_apply_layout(&self) {
        if self.layout_rules.is_none() || self.this_window_hwnd.is_invalid() {
            return;
        }
        let mut client_rect = RECT::default();
        if unsafe { GetClientRect(self.this_window_hwnd, &mut client_rect) }.is_err() {
            return;
        }
        // Calls the now-private helper.
        self.apply_layout_for_children(None, client_rect);
    }

    // The old `apply_layout_rules_for_children` becomes a private helper.
    fn apply_layout_for_children(&self, parent_id: Option<i32>, parent_rect: RECT) {
        // ... (existing layout logic) ...
    }
    // ...
    ```

##### **Testing Opportunities:**

*   The `calculate_layout` function can be made a `pub(crate)` method and unit-tested with various `LayoutRule` combinations without ever creating a real window.

---

#### **2. `Win32ApiInternalState` - The Global Platform State**

**Role:** The central owner of all windows, styles, and the application event handler.
**Core Invariant:** The `active_windows` map must be the single source of truth for window state. The `defined_styles` map manages GDI resources that must be cleaned up properly.

##### **Step-by-Step Refactoring (`src/platform_layer/app.rs`):**

1.  **Make all fields in `Win32ApiInternalState` private.**
2.  **Encapsulate `active_windows`:** The existing `with_window_data_read` and `with_window_data_write` methods are the perfect pattern. Make them the *only* way to access window data. Add methods for creating and destroying windows that manage the map internally.

    **After (in `impl Win32ApiInternalState`):**
    ```rust
    // The `with_...` methods remain the primary access pattern.

    // New method to abstract away the preliminary insertion logic.
    pub(crate) fn prepare_new_window(&self) -> PlatformResult<WindowId> {
        let window_id = self.generate_unique_window_id();
        let data = NativeWindowData::new(window_id);
        self.active_windows.write().unwrap().insert(window_id, data);
        Ok(window_id)
    }

    // The remove logic is already mostly encapsulated.
    pub(crate) fn remove_window_data(&self, window_id: WindowId) { /* ... */ }
    ```

3.  **Encapsulate Style Management (`defined_styles`):** Move the style parsing logic from `styling_handler.rs` directly into a method on `Win32ApiInternalState`.

    **Before:**
    ```rust
    // in command_executor.rs
    fn execute_define_style(...) {
        let parsed_style = styling_handler::parse_style(style)?;
        internal_state.defined_styles.write().unwrap().insert(style_id, Arc::new(parsed_style));
    }
    ```

    **After (in `impl Win32ApiInternalState`):**
    ```rust
    // This method now owns the entire process of defining a style.
    pub(crate) fn define_style(&self, style_id: StyleId, style: ControlStyle) -> PlatformResult<()> {
        // The parsing logic from styling_handler::parse_style() is moved here.
        // ... create HFONT, HBRUSH ...
        let parsed_style = ParsedControlStyle { /* ... */ };

        self.defined_styles.write().unwrap().insert(style_id, Arc::new(parsed_style));
        Ok(())
    }

    // The getter remains simple.
    pub(crate) fn get_parsed_style(&self, style_id: StyleId) -> Option<Arc<ParsedControlStyle>> {
        self.defined_styles.read().unwrap().get(&style_id).cloned()
    }
    ```

---

#### **3. `MainWindowUiState` & `ProfileRuntimeData` - The Application's Brain**

**Role:** These hold the core application state (`ProfileRuntimeData`) and the UI-specific state (`MainWindowUiState`).
**Core Invariants:** The data across these structs must be consistent. For example, `filter_text` in `MainWindowUiState` should align with the `last_successful_filter_result`. The `cached_token_count` in `ProfileRuntimeData` must accurately reflect the selected files in its `file_system_snapshot_nodes`.

##### **Step-by-Step Refactoring (`src/app_logic/main_window_ui_state.rs` & `profile_runtime_data.rs`):**

1.  **Make fields private in both `MainWindowUiState` and `ProfileRuntimeData`.**
2.  **Move Logic into Methods:** This is where the biggest gains are made.

    **Example 1: Filtering Logic in `MainWindowUiState`**
    **Before:** `MyAppLogic` would set `ui_state.filter_text`, then call `FileNode::build_tree_item_descriptors_filtered`, then check if the result was empty to set `ui_state.filter_no_match`.

    **After (in `impl MainWindowUiState`):**
    ```rust
    // This method encapsulates the entire filtering operation.
    pub(crate) fn apply_filter(&mut self, text: &str, all_nodes: &[FileNode]) {
        if text.is_empty() {
            self.filter_text = None;
            self.filter_no_match = false;
        } else {
            self.filter_text = Some(text.to_string());
        }

        let descriptors = if let Some(filter) = &self.filter_text {
            // All the logic for building descriptors is now here.
            FileNode::build_tree_item_descriptors_filtered(
                all_nodes,
                filter,
                &mut self.path_to_tree_item_id,
                &mut self.next_tree_item_id_counter,
            )
        } else {
            // ... logic for unfiltered descriptors ...
        };

        if self.filter_text.is_some() && descriptors.is_empty() {
            self.filter_no_match = true;
            // Don't update last_successful_filter_result if there's no match
        } else {
            self.filter_no_match = false;
            self.last_successful_filter_result = descriptors;
        }
    }
    ```
    Now, `MyAppLogic` just calls `ui_state.apply_filter(...)`.

    **Example 2: Token Counting in `ProfileRuntimeData`**
    The `ProfileRuntimeDataOperations` trait already provides a great API. We just need to ensure the implementation in `profile_runtime_data.rs` uses private fields. The logic for `update_total_token_count_for_selected_files` already lives in the `impl`, so this is mostly about changing `pub` to private on the fields themselves.

3.  **Provide Necessary Getters:** Add simple getters for data that needs to be read externally (e.g., `profile_name()`, `archive_path()`).

##### **Testing Opportunities:**

*   `MainWindowUiState::apply_filter` can be unit-tested by feeding it a `Vec<FileNode>` and a filter string, then asserting the state of `filter_no_match` and `last_successful_filter_result`.
*   The methods on `ProfileRuntimeData` (like `apply_token_progress`) can be tested by creating an instance, calling the method with test data, and then using a new getter (e.g., `total_token_count()`) to check the result.
