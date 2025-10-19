# SourcePacker: Performance & Idiomatic-Rust Improvement Plan (Revised)
**Date:** 2025-10-19

### 1. Status Update

The initial plan to refactor the file scanning and tokenization process has been largely implemented. The core architectural changes are now in place, including the scan/tokenization split, asynchronous tokenization, progress streaming, and checksum-based caching.

This revised plan outlines the remaining work, focusing on further improving UI responsiveness, code safety, and data durability.

---

### 2. Remaining Objectives & Success Criteria

#### Objectives
*   Improve the user's perception of "snapiness" by providing immediate feedback on token counts.
*   Enhance code clarity, safety, and maintainability by adopting more idiomatic Rust patterns.
*   Increase data durability by persisting the token cache more frequently.

#### Success Criteria (Definition of Done)
*   **Instant Feedback:** An estimated token count appears in the UI immediately after a file scan.
*   **Code Safety:** Raw `i32` control identifiers are replaced with a type-safe `ControlId` newtype. All `unsafe` Win32 API calls are contained within minimal, safe wrapper functions.
*   **Data Durability:** The token cache is saved periodically during a large recalculation.
*   **Code Quality:** `cargo clippy -- -D warnings` passes, and all new logic is covered by unit tests.

---

### 3. Revised Work Breakdown & Milestones

#### Milestone A — UI/UX Enhancement (High Priority)

*   **Task: Implement Instant Token Estimates**
    *   **Rationale:** The backend is now asynchronous, but the UI still waits for the first batch of exact results. Showing an immediate, approximate total will make the application feel instantaneous.
    *   **Actions:**
        1.  Immediately after a file scan completes, calculate a rough token estimate for each file (e.g., `approx_tokens = file_bytes / 4`).
        2.  Update the UI to display two counters: one for the initial estimate (e.g., "Estimated Tokens: ~150,000") and one for the exact count that updates as batches arrive (e.g., "Exact Tokens: 25,342/148,991").
        3.  Once the exact tokenization is complete, hide the "Estimated" counter and show only the final, exact total.

#### Milestone B — Idiomatic Rust & Code Safety

*   **Task: Introduce `ControlId` Newtype for UI Identifiers**
    *   **Rationale:** Replaces raw `i32` constants for control IDs with a type-safe struct to prevent accidental misuse and improve API clarity.
    *   **Actions:**
        1.  Define `pub struct ControlId(pub i32);` in `platform_layer/types.rs`.
        2.  Migrate all `PlatformCommand` and `AppEvent` variants to use `ControlId`.
        3.  Update application logic and UI description code to use the new type.

*   **Task: Centralize `unsafe` Win32 API Calls**
    *   **Rationale:** Drastically reduce the surface area of `unsafe` code by wrapping each Win32 call in a minimal, safe Rust function that handles errors idiomatically. This improves safety, ergonomics, and auditability.
    *   **Actions:**
        1.  **Create a dedicated FFI module:** Create a new file at `src/platform_layer/win32_ffi.rs` and declare it as a private module in `src/platform_layer.rs` (`mod win32_ffi;`).
        2.  **Create Safe Wrappers:** For each `unsafe` call to a Win32 function in the `platform_layer`, create a corresponding safe wrapper function inside `win32_ffi.rs`. This wrapper should:
            *   Accept idiomatic Rust types (e.g., `&str` instead of `PCWSTR`).
            *   Contain the `unsafe` block.
            *   Handle the conversion from Rust types to Win32 types (e.g., `&str` to `HSTRING`).
            *   Check the return value of the Win32 function for errors.
            *   Return a `PlatformResult<T>` to propagate errors correctly.
        3.  **Refactor Existing Code:** Systematically replace every `unsafe` block throughout the `platform_layer` (in modules like `command_executor.rs`, `window_common.rs`, and the `controls` handlers) with a call to the corresponding new safe wrapper function from the `win32_ffi` module.
        4.  **Audit and Repeat:** Continue this process until no `unsafe` blocks remain outside of the `win32_ffi.rs` module. Good candidates for wrapping include: `CreateWindowExW`, `DestroyWindow`, `SetWindowTextW`, `MoveWindow`, `EnableWindow`, `SendMessageW`, `PostMessageW`, and all GDI functions (`CreateFontW`, `CreateSolidBrush`, etc.).

#### Milestone C — Durability & Diagnostics

*   **Task: Implement Periodic Cache Persistence**
    *   **Rationale:** Persisting the token cache only on graceful shutdown risks losing significant progress if a large project is being processed and the application closes unexpectedly.
    *   **Actions:**
        1.  In the main thread's progress-handling logic (`poll_token_recalc_progress`), track how many new file details have been merged into the cache.
        2.  After a significant number of updates (e.g., every 500-1,000 changed files), trigger a profile save to persist the `cached_file_token_details` map to disk.

*   **Task (Optional): Add Telemetry & Diagnostics**
    *   **Rationale:** Adding simple metrics can help tune performance and verify cache effectiveness.
    *   **Actions:**
        1.  Log the total time spent in each stage: scan, tokenization, and merging.
        2.  Log the count of files that were tokenized versus those that were skipped (cache hits) during a `recalc_tokens_async` call.

#### Milestone D — Final Polish & Testing

*   **Task: Improve Error Handling and Logging**
    *   **Rationale:** Standardize error types for better context and use a more structured logging approach.
    *   **Actions:**
        1.  Consider using `thiserror` for library-level domain errors and `anyhow` at the top-level binary for context chaining.
        2.  If diagnostics become more complex, consider replacing the `log` macros with the `tracing` crate to add spans around tokenization batches.

*   **Task: Expand Test Coverage**
    *   **Rationale:** Ensure the new UI states and error paths from the remaining tasks are fully tested.
    *   **Actions:**
        1.  Add unit tests for the "Instant Token Estimate" logic.
        2.  Verify that all tests pass after the `ControlId` migration.
        3.  Add tests to confirm that profile saves are triggered correctly during periodic persistence.
