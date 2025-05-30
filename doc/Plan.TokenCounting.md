# Token Counting Implementation Plan for SourcePacker

**Goal:** Integrate token counting functionality into SourcePacker, allowing users to see an estimated token count for their selected files. The count should update dynamically and eventually be displayed in a dedicated part of the status bar.

**Core Requirements:**
*   `[TokenCountEstimateSelectedV1]`: Display estimated token count for selected files.
*   `[TokenCountLiveUpdateV1]`: Count updates live as files are selected/deselected.

**Guiding Principle:** The application must remain fully functional after each step.

---

## Phase 1: Basic Tokenizer Module & Core Logic Integration

**Goal:** Create the foundational token counting logic and integrate it into `MyAppLogic` to calculate token counts internally, without yet displaying them in the UI.

### Step 1.1: Create `tokenizer_utils` Module and Simple Estimator
*   **Action a:** Create a new module file: `src/core/tokenizer_utils.rs`.
*   **Action b:** In `src/core/mod.rs`, add `pub mod tokenizer_utils;` and re-export relevant items if any (likely just the function initially).
*   **Action c:** In `tokenizer_utils.rs`, define a public function `estimate_tokens_simple_whitespace(content: &str) -> usize` that counts words based on whitespace separation.
*   **Action d:** Add basic unit tests in `tokenizer_utils.rs` for `estimate_tokens_simple_whitespace`.
*   **Verification:**
    *   Project compiles.
    *   Unit tests for `tokenizer_utils` pass.
    *   Application runs as before (no visible changes).

### Step 1.2: Integrate Token Calculation into `MyAppLogic`
*   **Action a:** In `src/app_logic/handler.rs` (`MyAppLogic` struct):
    *   Add a new field: `current_token_count: usize`. Initialize to `0`.
*   **Action b:** Create a new private helper method in `MyAppLogic`, for example, `_recalculate_and_log_token_count`. This method will:
    *   Iterate through `self.file_nodes_cache`.
    *   For each `FileNode` that is a file and has `FileState::Selected`:
        *   Read its content using `std::fs::read_to_string`.
        *   Use `crate::core::tokenizer_utils::estimate_tokens_simple_whitespace` to count tokens in the content.
        *   Sum these counts.
        *   Log any file read errors but continue counting for other files.
    *   Store the total sum in `self.current_token_count`.
    *   Log the final calculated token count, number of files processed, and number of failed reads using `log::debug!`.
*   **Action c:** Call this new token calculation method at the end of:
    *   `handle_treeview_item_toggled` (after `update_current_archive_status`).
    *   `_activate_profile_and_show_window` (after scan and state application).
    *   `handle_menu_refresh_file_list_clicked` (after scan and state application).
*   **Verification:**
    *   Project compiles.
    *   Application runs. When selecting/deselecting files, or loading/refreshing profiles, debug logs show the updated token count. No UI changes yet.

### Step 1.3: Unit Test `MyAppLogic` Token Calculation
*   **Action:** In `src/app_logic/handler_tests.rs`:
    *   Add tests for `MyAppLogic` to verify `current_token_count` is updated correctly.
    *   These tests will need to:
        *   Set up `MyAppLogic` with mock dependencies.
        *   Populate `file_nodes_cache` with `FileNode`s, some marked as `Selected`.
        *   Use `tempfile` to create actual temporary files with known content for selected nodes in the test setup to allow `std::fs::read_to_string` to work.
        *   Simulate events like `TreeViewItemToggledByUser`.
        *   Assert `logic.current_token_count` has the expected value.
        *   Assert no *token-count-specific* UI `PlatformCommand`s are generated yet.
*   **Verification:**
    *   New unit tests pass.
    *   Application runs as before.

---

## Phase 2: Basic UI Display in Existing Status Bar

**Goal:** Display the calculated token count in the main status bar. This will temporarily share space with or overwrite other status messages.

### Step 2.1: Update `MyAppLogic` to Command Status Bar Update
*   **Action a:** Modify the token calculation method in `MyAppLogic` (e.g., `_recalculate_and_log_token_count`).
    *   After `self.current_token_count` is set, if `self.main_window_id` is `Some`, create and enqueue a `PlatformCommand::UpdateStatusBarText`.
    *   The text for this command should be formatted (e.g., "Tokens: X").
    *   Use `MessageSeverity::Information` or `MessageSeverity::Debug` for now.
    *   Consider renaming the method to reflect its new responsibility of also requesting UI updates (e.g., `_update_token_count_state_and_request_display`).
*   **Verification:**
    *   Project compiles.
    *   Application runs. The status bar now displays "Tokens: X" when selections change or profiles load/refresh. This message might conflict with other status updates.

### Step 2.2: Update `MyAppLogic` Unit Tests
*   **Action:** In `src/app_logic/handler_tests.rs`:
    *   Modify existing tests (or add new ones) to assert that `PlatformCommand::UpdateStatusBarText` is generated with the correct token count message when the token calculation/update method is called.
*   **Verification:**
    *   Unit tests pass.
    *   Application runs as in Step 2.1.

---

## Phase 3: Advanced Tokenizer Integration

**Goal:** Replace the simple whitespace tokenizer with a more accurate library like `tiktoken-rs`.

### Step 3.1: Add `tiktoken-rs` Dependency
*   **Action:** Add `tiktoken-rs = "0.5"` (or latest version) to `Cargo.toml` under `[dependencies]`.
*   **Verification:**
    *   `cargo build` completes successfully, downloading and compiling the new dependency.
    *   Application runs as before.

### Step 3.2: Update `tokenizer_utils.rs`
*   **Action a:** Modify `src/core/tokenizer_utils.rs`:
    *   Import necessary items from `tiktoken_rs` (e.g., `cl100k_base`, `CoreBPE`).
    *   Define a new public function, e.g., `estimate_tokens_tiktoken(content: &str) -> usize`.
    *   Inside this function, get a `CoreBPE` instance (e.g., using `cl100k_base()`).
    *   Use the `bpe.encode_with_special_tokens(content).len()` to get the token count.
    *   Include error handling for BPE initialization, potentially logging an error and returning 0 or panicking.
    *   Add unit tests for this new `estimate_tokens_tiktoken` function.
*   **Action b:** In `MyAppLogic`'s token calculation method, change the call from `estimate_tokens_simple_whitespace` to the new `estimate_tokens_tiktoken`.
*   **Verification:**
    *   Project compiles.
    *   Unit tests for `tokenizer_utils` (including new `tiktoken` tests) pass.
    *   Application runs. Token counts displayed in the status bar should now be based on `tiktoken-rs`.

### Step 3.3: Performance Consideration (Lazy BPE Initialization)
*   **Action (Optional but Recommended):** Modify `tokenizer_utils.rs` to use `lazy_static` or `once_cell` (add to `Cargo.toml`) for the `CoreBPE` instance. This avoids re-initializing the BPE model on every `estimate_tokens_tiktoken` call.
*   **Verification:**
    *   Project compiles and runs. Performance for token counting should be improved, especially if called frequently.

---

## Phase 4: Integration with Sophisticated Status Bar (Post P2.12)

**Goal:** Display the token count in its own dedicated section of the status bar, assuming the "Sophisticated Status Bar Control" (P2.12 from `DevelopmentPlan.md`) has been implemented.

**Pre-requisite:** P2.12 from `DevelopmentPlan.md` is implemented. This implies the existence of a multi-part status bar and mechanisms to update individual parts.

### Step 4.1: Adapt `MyAppLogic` to Use New Status Bar Command
*   **Action a:** If P2.12 introduced specific identifiers for status bar parts (e.g., an enum or constants for indices), ensure these are accessible or define one for the token count part (e.g., `STATUS_PART_TOKENS`).
*   **Action b:** In `MyAppLogic`'s token update method:
    *   Remove the old `PlatformCommand::UpdateStatusBarText` command previously used for the token count.
    *   Add a new command to update the specific status bar part dedicated to tokens. This command would be something like `PlatformCommand::UpdateStatusBarPart { window_id, part_index: STATUS_PART_TOKENS, text: token_status_text }`. The exact command structure depends on the P2.12 implementation.
*   **Verification:**
    *   Project compiles.
    *   Application runs. The token count now appears in its dedicated section of the status bar, no longer conflicting with general status messages.

### Step 4.2: Update `MyAppLogic` Unit Tests for New Command
*   **Action:** In `src/app_logic/handler_tests.rs`:
    *   Modify tests to assert that the new platform command for updating a status bar part is generated correctly with the appropriate part identifier and token count message.
*   **Verification:**
    *   Unit tests pass.

---

## Phase 5: Future Enhancements (Optional)

*   **Asynchronous Token Counting:** For very large selections or complex tokenizers, move token counting to a background thread to prevent UI freezes. This would involve `MyAppLogic` spawning a task, the task reading files and counting tokens, and then sending an `AppEvent` back to `MyAppLogic` with the result.
*   **Configurable Tokenizer Model:** Allow users to select different `tiktoken-rs` models (e.g., via a settings dialog). `MyAppLogic` would store the selected model name, and `tokenizer_utils` would need to be adapted to use it.
*   **Display Token Count Per File:**
    *   Show token count for the currently selected file in the "File Content Viewer" panel (P3.3).
    *   Potentially add a column to the TreeView or a tooltip.
*   **Error Handling for File Reads in UI:** Improve robustness if many files fail to read during token counting (e.g., display "Tokens: X (Y files failed to read)" in the status bar part).
