# Token Counting Implementation Plan for SourcePacker

**Goal:** Integrate token counting functionality into SourcePacker, allowing users to see an estimated token count for their selected files. The count should update dynamically and eventually be displayed in a dedicated part of the status bar.

**Core Requirements:**
*   `[TokenCountEstimateSelectedV1]`: Display estimated token count for selected files.
*   `[TokenCountLiveUpdateV1]`: Count updates live as files are selected/deselected.

**Guiding Principle:** The application must remain fully functional after each step.

---

## Phase 1: Basic Tokenizer Module & Core Logic Integration

**Goal:** Create the foundational token counting utility and integrate it into `AppSessionData` to calculate token counts on-the-fly for all selected files (without per-file caching yet). `cached_current_token_count` in `AppSessionData` is updated.

---

## Phase 2: Basic UI Display in Existing Status Bar

**Goal:** Display the `cached_current_token_count` from `AppSessionData` in the main status bar. This will temporarily share space with or overwrite other status messages.

---

## Phase 3: Per-File Token Caching with Checksums for Performance `[NEW MAJOR PHASE]`

**Goal:** Significantly improve token counting performance by caching token counts for individual files and only re-calculating when file content changes (detected by checksums).

**Pre-requisite:** Phases 1 and 2 completed. `AppSessionData` can calculate total tokens on-the-fly, and the UI can display this total.

### Step 3.1: Modify `Profile` Model and Serialization
*   **Action a:** In `src/core/models.rs` (`Profile` struct):
    *   Define `pub struct FileTokenDetails { pub checksum: String, pub token_count: usize }` (ensure it has `Serialize, Deserialize, Clone, Debug, PartialEq`).
    *   Add a new field to `Profile`: `pub file_details: std::collections::HashMap<std::path::PathBuf, FileTokenDetails>`.
*   **Action b:** Update `CoreProfileManager`'s save/load logic (`src/core/profiles.rs`) to handle the new `file_details` field.
    *   Existing profiles without this field should load gracefully (e.g., `file_details` defaults to an empty `HashMap`).
*   **Verification:**
    *   Project compiles.
    *   Application runs. Existing profiles can be loaded (effectively with an empty token cache). New profiles can be saved and reloaded with an empty `file_details` map. No functional change yet in token counting behavior.

### Step 3.2: Checksum Utility and `FileNode` Update
*   **Action a:** Add `sha2 = "0.10"` to `Cargo.toml` dependencies.
*   **Action b:** Create a checksum utility function (e.g., in a new `src/core/checksum_utils.rs` or existing `utils.rs`) that takes a file path and returns `io::Result<String>` (hex-encoded SHA256 checksum).
*   **Action c:** In `src/core/models.rs` (`FileNode` struct):
    *   Add a field: `pub checksum: Option<String>`.
*   **Action d:** In `CoreFileSystemScanner::scan_directory` (`src/core/file_system.rs`):
    *   When creating/populating `FileNode`s for files, calculate their checksum using the new utility and store it in `FileNode.checksum`. Handle potential I/O errors during checksum calculation (e.g., log and leave checksum as `None`).
*   **Verification:**
    *   Checksum utility can be unit tested.
    *   Project compiles.
    *   `FileNode`s for files now have a checksum populated during scanning (can be verified in debugger or logs). No functional change yet in token counting behavior.

### Step 3.3: Populate `file_details` in `Profile` during Profile Save
*   **Action:** Modify `AppSessionData::create_profile_from_session_state` (`src/core/app_session_data.rs`):
    *   When creating a `Profile` instance to be saved:
        *   Initialize an empty `file_details` HashMap for the new `Profile`.
        *   Iterate through `self.file_nodes_cache` (recursively).
        *   For each `FileNode` that is a file and `FileNode.state == FileState::Selected`:
            *   If `FileNode.checksum` is `Some(checksum_val)`:
                *   Read the file content.
                *   Calculate its token count using `self.token_counter_manager.count_tokens(&content)`. Handle read errors.
                *   Insert into the new `Profile`'s `file_details`: `(file_node.path.clone(), FileTokenDetails { checksum: checksum_val.clone(), token_count })`.
*   **Verification:**
    *   Project compiles.
    *   Saving a profile now populates the `file_details` map in the persisted JSON file with checksums and actual token counts for all *selected* files at the time of save.
    *   Loading this profile correctly deserializes `file_details`. Token counting logic in `AppSessionData::update_token_count` still uses the old full scan (it's not using the cache yet).

### Step 3.4: Update `current_profile_cache.file_details` during Profile Activation/Refresh
*   **Action:** In `AppSessionData::activate_and_populate_data` (`src/core/app_session_data.rs`):
    *   This occurs *after* `file_system_scanner.scan_directory` and `state_manager.apply_profile_to_tree`.
    *   The `profile_to_activate` (which becomes `self.current_profile_cache`) already contains `file_details` loaded from disk. We need to update these based on current file checksums.
    *   Iterate through `self.file_nodes_cache` (recursively).
    *   For each `FileNode` that is a file:
        *   Let `current_checksum_on_disk = file_node.checksum.as_ref()`.
        *   Let `cached_details_mut = self.current_profile_cache.as_mut().unwrap().file_details`.
        *   If `file_node.state == FileState::Selected`:
            *   If `current_checksum_on_disk` is `Some(disk_cs)`:
                *   If `cached_details_mut.get(&file_node.path)` has a different checksum than `disk_cs`, or is not present:
                    *   Read file content, calculate its token count using `token_counter.count_tokens(&content)`.
                    *   Update/insert into `cached_details_mut`: `(file_node.path.clone(), FileTokenDetails { checksum: disk_cs.clone(), token_count })`.
            *   Else (no checksum on disk, e.g., read error during scan): Remove from `cached_details_mut` if present, or log.
        *   Else (`file_node.state != FileState::Selected`):
            *   (Optional but good for hygiene) Remove `file_node.path` from `cached_details_mut` if it exists, as its token count is no longer relevant for the current sum.
*   **Note:** This step ensures that `self.current_profile_cache.file_details` is "live" with token counts for currently selected files, respecting their latest content on disk.
*   **Verification:**
    *   After profile activation or a file list refresh, `AppSessionData.current_profile_cache.file_details` contains up-to-date token counts for selected files whose checksums are known. Files that changed content will have their token counts re-calculated and cached here.

### Step 3.5: Modify `AppSessionData::update_token_count` to Use the Populated Cache
*   **Action:** Rewrite `AppSessionData::update_token_count` (`src/core/app_session_data.rs`):
    *   Initialize `total_tokens = 0`.
    *   Iterate `self.file_nodes_cache` (recursively).
    *   For each `FileNode` that is a file and `FileNode.state == FileState::Selected`:
        *   If `self.current_profile_cache` is `Some(profile)` and `profile.file_details.get(&file_node.path)` returns `Some(details)`:
            *   And if `file_node.checksum.as_ref() == Some(&details.checksum)`:
                *   Add `details.token_count` to `total_tokens`.
            *   Else (checksum mismatch or `FileNode` has no checksum):
                *   Fallback: Read file, calculate tokens using `token_counter.count_tokens()`. Add to `total_tokens`. Log a warning that cache was stale or unavailable for this file.
        *   Else (file not in cache):
            *   Fallback: Read file, calculate tokens using `token_counter.count_tokens()`. Add to `total_tokens`. Log a warning.
    *   Set `self.cached_current_token_count = total_tokens`.
*   **Verification:**
    *   Token counting is now significantly faster for selections/deselections if files haven't changed, as it primarily sums pre-calculated values from `current_profile_cache.file_details`.
    *   If a file's content changes on disk, then the user refreshes the file list (triggering Step 3.2d and 3.4), its new token count is used.
    *   The `cached_current_token_count` in `AppSessionData` remains correct.
    *   Status bar updates for token counts reflect the new cached logic.

---

## Phase 4: Advanced Tokenizer Integration (e.g., `tiktoken-rs`)

**Goal:** Replace the simple whitespace tokenizer with a more accurate library like `tiktoken-rs`. The caching mechanism from Phase 3 will now cache `tiktoken-rs` based counts.

**Pre-requisite:** Phase 3 completed.

---

## Phase 5: Integration with Sophisticated Status Bar (Post P2.12)

**Goal:** Display the token count in its own dedicated section of the status bar, assuming the "Sophisticated Status Bar Control" (P2.12 from `DevelopmentPlan.md`) has been implemented. This phase is primarily about UI presentation of the already efficiently-calculated token count.

**Pre-requisite:** Phase 4 completed. P2.12 from `DevelopmentPlan.md` is implemented.

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

## Phase 6: Future Enhancements (Optional)

*   **Asynchronous Token Counting:** For very large selections or complex tokenizers, move token counting to a background thread to prevent UI freezes. This would involve `MyAppLogic` spawning a task, the task reading files and counting tokens, and then sending an `AppEvent` back to `MyAppLogic` with the result.
    *   **Note:** The caching from Phase 3 significantly reduces the *frequency* of full token calculations. Async would primarily benefit the initial cache population (Step 3.3, 3.4) for many new/changed files.
*   **Configurable Tokenizer Model:** Allow users to select different `tiktoken-rs` models (e.g., via a settings dialog). `MyAppLogic` would store the selected model name, and `tokenizer_utils` would need to be adapted to use it.
*   **Display Token Count Per File:**
    *   With Phase 3, `MyAppLogic` can query `AppSessionData` for a specific file's cached token count (from `profile.file_details`) if its current `FileNode.checksum` matches the cached one.
    *   Display this in the "File Content Viewer" panel (P3.3) or as a tooltip.
*   **Error Handling for File Reads in UI:** Improve robustness if many files fail to read during token counting (e.g., display "Tokens: X (Y files failed to read / Z cache misses)" in the status bar part).
*   **Action (If using `tiktoken-rs`):** Modify `tokenizer_utils.rs` to use `lazy_static` or `once_cell` (add to `Cargo.toml`) for the `CoreBPE` instance. This avoids re-initializing the BPE model on every `estimate_tokens_tiktoken` call.
    *   Verification: Project compiles and runs. Performance for token counting should be improved, especially if called frequently.
