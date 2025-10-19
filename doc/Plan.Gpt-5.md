Of course. Based on the provided source code, I've analyzed the progress made on the tasks outlined in `Plan.Gpt-5.md`. Many of the core architectural changes, like splitting the scan and tokenization phases and implementing a parallel background worker, have been completed.

Here is a rewritten version of the plan that removes completed tasks, updates the overall objectives, and refocuses the milestones on the remaining work.

***

# SourcePacker: Performance & Idiomatic-Rust Improvement Plan (Revised)
**Date:** 2025-10-19

### 1. Status Update

The initial plan to improve performance by refactoring the file scanning and tokenization process has been largely implemented. The core architectural changes are now in place:

*   **Scan/Tokenization Split:** The initial file system scan is now a lightweight operation that only gathers file metadata and checksums, fully separating it from the CPU-intensive tokenization process.
*   **Asynchronous Tokenization:** Token counting has been moved to a background worker that runs on a bounded Rayon thread pool, preventing the UI from freezing during large calculations.
*   **Progress Streaming:** A progress channel (`TokenProgressChannel`) has been established to stream batched results from the worker back to the main thread for incremental UI updates.
*   **Checksum-Based Caching:** The tokenization pipeline now leverages checksums to skip re-processing files whose content has not changed.
*   **Persistence on Exit:** The application state, including the token cache, is persisted when the application quits gracefully.

This revised plan outlines the remaining work, focusing on further improving UI responsiveness, code safety, and data durability.

---

### 2. Remaining Objectives & Success Criteria

#### Objectives
*   Improve the user's perception of "snapiness" by providing immediate feedback on token counts, even before exact calculations are complete.
*   Enhance code clarity, safety, and maintainability by adopting more idiomatic Rust patterns, especially around UI identifiers and `unsafe` code.
*   Increase data durability by persisting the token cache more frequently during long operations.

#### Success Criteria (Definition of Done)
*   **Instant Feedback:** An estimated token count appears in the UI immediately after a file scan, which is then replaced by the exact count as it's calculated.
*   **Code Safety:** Raw `i32` control identifiers are replaced with a type-safe `ControlId` newtype. All `unsafe` Win32 API calls are contained within minimal, safe wrapper functions.
*   **Data Durability:** The token cache is saved periodically during a large recalculation, ensuring progress isn't lost if the application is unexpectedly closed.
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
    *   **Rationale:** Replaces raw `i32` constants for control IDs with a type-safe struct to prevent accidental misuse and improve API clarity, as recommended in other planning documents.
    *   **Actions:**
        1.  Define `pub struct ControlId(pub i32);` in the UI type system (`platform_layer/types.rs`).
        2.  Migrate all `PlatformCommand` and `AppEvent` variants that use `control_id: i32` to use `control_id: ControlId`.
        3.  Update all application logic and UI description code to use the new `ControlId` type. This aligns with the goals of `Plan.CommanDuctUI.md`.

*   **Task: Centralize `unsafe` Win32 API Calls**
    *   **Rationale:** Reduce the surface area of `unsafe` code by wrapping each Win32 call in a minimal, safe Rust function.
    *   **Actions:**
        1.  Create a dedicated module (e.g., `platform_layer::win32_ffi`) for these wrappers.
        2.  Audit the `platform_layer` and move each `unsafe` block into a safe function within the new module.
        3.  Update the rest of the UI code to call these new safe wrappers instead of using `unsafe` blocks directly.

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
