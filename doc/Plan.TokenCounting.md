# Token Counting Implementation Plan for SourcePacker

**Goal:** Integrate token counting functionality into SourcePacker, allowing users to see an estimated token count for their selected files. The count should update dynamically and eventually be displayed in a dedicated part of the status bar.

**Core Requirements:**
*   `[TokenCountEstimateSelectedV1]`: Display estimated token count for selected files.
*   `[TokenCountLiveUpdateV1]`: Count updates live as files are selected/deselected.

**Guiding Principle:** The application must remain fully functional after each step.

---

## Phase 1: Basic Tokenizer Module & Core Logic Integration

**Goal:** Create the foundational token counting logic and integrate it into `MyAppLogic` to calculate token counts internally, without yet displaying them in the UI.

---

## Phase 2: Basic UI Display in Existing Status Bar

**Goal:** Display the calculated token count in the main status bar. This will temporarily share space with or overwrite other status messages.

---

## Phase 3: Advanced Tokenizer Integration

**Goal:** Replace the simple whitespace tokenizer with a more accurate library like `tiktoken-rs`.

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

*   **Action (Optional but Recommended):** Modify `tokenizer_utils.rs` to use `lazy_static` or `once_cell` (add to `Cargo.toml`) for the `CoreBPE` instance. This avoids re-initializing the BPE model on every `estimate_tokens_tiktoken` call.
*   **Verification:**
    *   Project compiles and runs. Performance for token counting should be improved, especially if called frequently.
