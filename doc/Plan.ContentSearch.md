### **Overview of the Content Search Feature Plan**

Here’s a high-level look at the plan for the new "Search by Content" feature we discussed. The goal is to build a powerful, IDE-like search experience directly within SourcePacker, helping users quickly find all files related to a specific topic (like a function name or a variable) without the application ever freezing.

**What the User Will See (The End Result)**

1.  **Seamless Search Mode Switching:** A simple button next to the current search box will let the user toggle between the existing "By Name" filter and the new "By Content" mode.
2.  **Instant & Responsive Filtering:** When the user types a search term and presses Enter in "Content" mode, the app remains perfectly responsive. The TreeView will then update to show only the files that contain the search term, nested within their parent directories.
3.  **Interactive Match Preview:** This is where the feature really shines. When a user clicks on a file in the filtered list:
    *   The preview pane will automatically scroll to the first occurrence of the search term.
    *   Every occurrence of the term in the file will be highlighted with a distinct background color.
    *   Simple "Next" (▼) and "Previous" (▲) buttons will appear, allowing the user to instantly cycle through all the matches within that single file.

**How We'll Build It Smart (The Technical Strategy)**

The plan is designed to be robust, maintainable, and to deliver this feature without sacrificing the application's performance.

1.  **Asynchronous by Design:** To prevent the UI from freezing while searching through potentially thousands of files, the entire search operation will run on a separate background thread. This uses the same successful pattern we established for the asynchronous token counting, ensuring the app is always snappy.
2.  **A Generalized Job Manager:** Instead of building a one-off system for this search, we'll take this opportunity to refactor our background task handling. We will create a central "Job Manager."
    *   This manager will be responsible for running *any* long-running task (token counting, content searching, and future features).
    *   This simplifies the main application logic immensely and makes it incredibly easy to add more background jobs later (like your autocomplete indexing idea) without complicating the core code. It's a key architectural improvement.

**A Step-by-Step, Stable Rollout**

The feature will be built in distinct, stable phases. The application will be fully functional and testable after every single step, which allows for flexibility in planning.

*   **Phase 1: Build the UI Shell.** First, we'll add the new buttons and state-tracking logic to the UI. The feature won't work yet, but the controls will be visible and interactive.
*   **Phase 2: Build the Backend Engine.** Next, we'll implement the asynchronous search logic completely in the background. It will be fully unit-tested but not yet connected to the TreeView.
*   **Phase 3: Connect the Engine to the UI.** In this phase, we'll wire the backend results to the TreeView. After this step, the core feature will be complete and usable: the user will be able to search and see the filtered file list.
*   **Phase 4: Add the Polish.** Finally, we'll implement the advanced preview pane features: highlighting match occurrences and adding the "Next/Previous" navigation buttons.

This phased approach ensures we build on a solid foundation, can test our work thoroughly at each stage, and end up with a high-quality feature that significantly enhances SourcePacker's utility.

### **Phase 1: UI Foundation and State Management**

**Goal:** Add the necessary UI controls and application state for the new feature without implementing the search logic itself. The UI will be interactive, but content search will not yet filter files.

---

**Step 1.1: Add Search Mode Toggle Control**

*   **Goal:** Create a button in the UI that allows the user to switch between "By Name" and "By Content" search modes.
*   **Actions:**
    1.  **UI Constants:** In `src/app_logic/ui_constants.rs`, add a new control ID:
        ```rust
        pub const SEARCH_MODE_TOGGLE_BUTTON_ID: ControlId = ControlId::new(1024);
        ```
    2.  **UI Description:** In `src/ui_description_layer.rs`, update `build_main_window_static_layout`:
        *   Add a `PlatformCommand::CreateButton` for `SEARCH_MODE_TOGGLE_BUTTON_ID`. Set its initial text to "Name".
        *   Add a `PlatformCommand::ApplyStyleToControl` for it using `StyleId::DefaultButton`.
        *   Add a new `LayoutRule` to place it next to the `FILTER_INPUT_ID`, likely docked to the left within `FILTER_PANEL_ID`. Adjust the existing layout rules for the filter input to accommodate it.
*   **Requirements:**
    *   `[UiContentSearchModeToggleV1]` The UI must provide a control to switch the filter mode between "By Name" and "By Content".
*   **Testing:**
    *   No unit tests are needed for this step. Manual verification: run the application and confirm the "Name" button appears next to the filter input box.
*   **Result:** The application runs and displays the new button. Clicking it does nothing yet.

---

**Step 1.2: Implement State Management for Search Mode**

*   **Goal:** Allow the application logic to track the current search mode.
*   **Actions:**
    1.  **State Enum:** In `src/app_logic/main_window_ui_state.rs`, define the `SearchMode` enum and add it to the `MainWindowUiState` struct.
        ```rust
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum SearchMode { ByName, ByContent }

        pub struct MainWindowUiState {
            // ... existing fields
            search_mode: SearchMode,
        }
        // In MainWindowUiState::new(), initialize search_mode: SearchMode::ByName.
        ```    2.  **Event Handling:** In `src/app_logic/handler.rs`, modify `handle_button_clicked` to handle clicks on `SEARCH_MODE_TOGGLE_BUTTON_ID`.
        *   When clicked, toggle the `search_mode` in `MainWindowUiState`.
        *   Enqueue a `PlatformCommand::SetWindowTextW` (you'll need a new `PlatformCommand` variant like `SetButtonText` or reuse a general one) to update the button's label to "Name" or "Content" based on the new state.
*   **Requirements:** (Updates) `[UiContentSearchModeToggleV1]`
*   **Testing:**
    *   **Unit Test:** In `handler_tests.rs`, add a test that sends an `AppEvent::ButtonClicked` for `SEARCH_MODE_TOGGLE_BUTTON_ID`. Assert that the correct `PlatformCommand` to change the button's text is generated and that the internal state flips.
*   **Result:** The application runs, and clicking the search mode button now correctly toggles its text between "Name" and "Content". The filter's *behavior* is still unchanged.

### **Phase 2: Asynchronous Backend Search**

**Goal:** Implement the background file-content search mechanism. This phase involves no changes to the UI's visible output but makes the search functionality work under the hood.

---

**Step 2.1: Define the Search Progress Data Structures**

*   **Goal:** Create the data types for communicating search results from the worker thread.
*   **Actions:**
    1.  Create a new file: `src/core/content_search_progress.rs`.
    2.  Define the `ContentSearchResult` and `ContentSearchProgress` structs as discussed. For this initial version, the result only needs the path and a boolean `matches` flag.
        ```rust
        pub struct ContentSearchResult {
            pub path: PathBuf,
            pub matches: bool,
        }
        // ... and ContentSearchProgress
        ```
*   **Requirements:** (Internal) `[TechContentSearchAsyncV1]` Content search must be performed on a background thread to keep the UI responsive.
*   **Testing:** No specific tests for this step, as it only defines data structures.

---

**Step 2.2: Implement the Asynchronous Search Function**

*   **Goal:** Create the function that performs the search in the background using `rayon`.
*   **Actions:**
    1.  **Trait Update:** In `src/core/profile_runtime_data.rs`, add `search_content_async(&self, search_term: String) -> Option<mpsc::Receiver<ContentSearchProgress>>` to the `ProfileRuntimeDataOperations` trait.
    2.  **Implementation:** Implement the function in `ProfileRuntimeData`. Use `rayon::prelude::*` to parallelize the search over the file snapshot. For now, a simple case-insensitive `contains` check on the file content is sufficient. Send all results back in a single `is_final: true` message.
*   **Requirements:** `[TechContentSearchAsyncV1]`, `[UiSearchFileContentV1]` The application must allow users to filter files based on a search string matching their content.
*   **Testing:**
    *   **Unit Test:** In `profile_runtime_data.rs`'s test module, create a test that:
        *   Sets up a `ProfileRuntimeData` with a few `FileNode`s pointing to temporary files with known content.
        *   Calls `search_content_async`.
        *   Receives the result from the channel and asserts that the correct files were identified as matches.
*   **Result:** The core logic now has a fully functional, asynchronous content search capability, but it is not yet connected to the UI filtering.

---

**Step 2.3: Integrate Backend Search into Application Logic**

*   **Goal:** Trigger the async search from `MyAppLogic` and store the results.
*   **Actions:**
    1.  **Driver Struct:** In `src/app_logic/handler.rs`, define a `ContentSearchDriver` struct to hold the `mpsc::Receiver`. Add an `Option<ContentSearchDriver>` field to `MyAppLogic`.
    2.  **State Storage:** In `src/app_logic/main_window_ui_state.rs`, add `content_search_matches: Option<HashSet<PathBuf>>` to `MainWindowUiState`. This will store the final results.
    3.  **Modify `handle_filter_text_submitted`:**
        *   If the `search_mode` is `ByContent`, call `app_session_data_ops.search_content_async(...)`.
        *   Store the returned receiver in a new `ContentSearchDriver` instance in `MyAppLogic`.
        *   Set `content_search_matches` in `MainWindowUiState` to `None` to indicate a search is in progress.
    4.  **Create Polling Function:** Create `poll_content_search_progress` in `MyAppLogic`. Call it at the start of `try_dequeue_command`.
        *   This function will check the receiver. When the final results arrive, it will populate the `content_search_matches` `HashSet` in `MainWindowUiState` and then drop the driver.
*   **Requirements:** (Updates) `[TechContentSearchAsyncV1]`
*   **Testing:**
    *   **Unit Test:** In `handler_tests.rs`, create a test for `handle_filter_text_submitted` in "By Content" mode.
        *   Use a mock `ProfileRuntimeData` that returns a mock channel.
        *   Assert that `MyAppLogic` enters a state where it's waiting for results.
        *   Simulate receiving results and assert that the `content_search_matches` in the mock UI state is correctly populated.
*   **Result:** The application now runs the content search in the background when requested. The results are stored internally but the TreeView does not yet update.

### **Phase 3: Displaying the Filtered Results**

**Goal:** Connect the backend search results to the frontend TreeView, making the filter functional for the user.

---

**Step 3.1: Implement the Content-Filtered Descriptor Builder**

*   **Goal:** Create the logic that translates a set of matching file paths into a visible tree structure.
*   **Actions:**
    1.  **New Function:** In `src/core/file_node.rs`, implement the `build_tree_item_descriptors_from_matches` function as planned. This function's logic is to include a node only if it is in the `matches` set *or* if it is an ancestor of a node in the `matches` set.
*   **Requirements:** `[UiSearchFileContentV1]`
*   **Testing:**
    *   **Unit Test:** In `file_node.rs`'s test module, create a test for this new function.
        *   Build a sample `FileNode` tree.
        *   Create a `HashSet` containing paths for a few nested files.
        *   Call the function and assert that the returned `Vec<TreeItemDescriptor>` correctly includes only the matching files and their parent directories, and nothing else.
*   **Result:** A pure, testable function for generating the filtered view now exists.

---

**Step 3.2: Update the Live TreeView**

*   **Goal:** Make the TreeView visually update based on the results of a content search.
*   **Actions:**
    1.  **Modify `rebuild_tree_descriptors`:** In `src/app_logic/main_window_ui_state.rs`, update `rebuild_tree_descriptors`.
        *   It should now check if `self.content_search_matches` is `Some`.
        *   If it is, it should call the new `build_tree_item_descriptors_from_matches` function, passing in the set of matches.
        *   If it's `None`, it should fall back to the existing name-filter logic.
    2.  **Trigger Repopulation:** In `src/app_logic/handler.rs`, at the end of `poll_content_search_progress` (when the final search result is received), call `repopulate_tree_view`. This will trigger the updated `rebuild_tree_descriptors` logic and send the `PopulateTreeView` command.
*   **Requirements:** (Updates) `[UiSearchFileContentV1]`
*   **Testing:**
    *   **Integration Test:** The existing unit tests for `MyAppLogic` that cover filtering should be expanded. A new test should simulate a complete content search flow, from submitting the text to polling and finally asserting that a `PopulateTreeView` command is generated.
*   **Result:** **The feature is now functionally complete!** The user can switch to "By Content" mode, type a search term, and see the TreeView filter to show only matching files.

### **Phase 4: Advanced Preview Pane Functionality**

**Goal:** Enhance the user experience by upgrading the preview pane to support highlighting and navigating through matches.

---

**Step 4.1: Upgrade Viewer to a RichEdit Control**

*   **Goal:** Replace the standard `EDIT` control with a `RichEdit` control capable of formatted text.
*   **Modern API Research:** The modern approach for high-performance text rendering is DirectWrite. However, for a control that needs to handle text selection, scrolling, and formatting, the `RichEdit` control is the most practical and powerful component. It uses modern rendering internally and provides a rich message-based API (`EM_` messages).
*   **Actions:**
    1.  **Library Loading:** In `src/platform_layer/app.rs` (`PlatformInterface::new`), you may need to explicitly load the RichEdit library: `unsafe { LoadLibraryW(&HSTRING::from("msftedit.dll")) };`.
    2.  **Control Creation:** In `src/platform_layer/command_executor.rs` (`execute_create_input`), when creating the viewer control (`ID_VIEWER_EDIT_CTRL`), use `RICHEDIT_CLASSW` instead of `WC_EDITW`. You may need to add this class constant.
*   **Requirements:**
    *   `[UiMatchHighlightingV1]` The preview pane must visually highlight occurrences of the active search term within a file's content.
*   **Testing:** Manual verification. The viewer should still function as a read-only text box.

---

**Step 4.2: Enhance Backend and Communication**

*   **Goal:** Modify the search worker to report not just *if* a file matches, but *where* it matches.
*   **Actions:**
    1.  **Data Structures:** Update `src/core/content_search_progress.rs`:
        ```rust
        pub struct MatchLocation { pub line: usize, pub start: usize, pub end: usize }
        pub struct ContentSearchResult {
            pub path: PathBuf,
            pub matches: Vec<MatchLocation>, // Changed from bool
        }
        ```
    2.  **Worker Logic:** Update the search worker in `search_content_async` to find the line number and column offsets of all matches within a file and return them.
    3.  **State Storage:** In `MainWindowUiState`, change `content_search_matches` to `Option<HashMap<PathBuf, Vec<MatchLocation>>>` to store the detailed match info.
*   **Requirements:** (Updates) `[UiMatchHighlightingV1]`
*   **Testing:**
    *   **Unit Test:** Update the backend search test to assert that the `Vec<MatchLocation>` returned for a matching file is correct.

---

**Step 4.3: Implement Highlighting**

*   **Goal:** Make the RichEdit control visually highlight the search matches.
*   **Actions:**
    1.  **New Platform Command:** In `src/platform_layer/types.rs`, modify `SetViewerContent` or create a new command:
        ```rust
        PlatformCommand::SetViewerContentWithHighlights {
            window_id: WindowId,
            control_id: ControlId,
            text: String,
            highlights: Vec<(usize, usize)>, // (start_char_index, end_char_index)
        }
        ```
    2.  **Executor Logic:** In `command_executor.rs`, implement the handler for this new command. It will use `RichEdit` messages (`EM_SETCHARFORMAT`, `EM_EXSETSEL`) to apply a background color format to the specified character ranges.
    3.  **`MyAppLogic`:** When a user selects a file from the content-filtered tree, `MyAppLogic` will now use this new command, converting the line/column match locations into absolute character indices.
*   **Requirements:** (Updates) `[UiMatchHighlightingV1]`
*   **Testing:** This is difficult to unit test. Manual verification is the most practical approach.
