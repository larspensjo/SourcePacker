# SourcePacker: Performance & Idiomatic-Rust Improvement Plan
**Date:** 2025-08-08

This plan integrates: (1) idiomatic-Rust cleanups, (2) a non-blocking worker for scans, and (3) a **separate, parallel tokenization pass** with caching. It’s written to be implemented in small, verifiable steps.

---

## 1) Objectives & Success Criteria

### Objectives
- Keep the UI/message loop responsive under heavy projects.
- Reduce time-to-first-accurate-token-total by parallelizing and caching.
- Improve code clarity and maintainability with idiomatic Rust patterns.

### Success Criteria (Definition of Done)
- **First open**: UI becomes interactive within ~200 ms after scan begins (no long blocking).
- **Token totals**: a quick on-screen estimate appears immediately; exact counts converge batch-by-batch.
- **Rescan**: Only changed files are tokenized; unchanged files are cheap cache hits.
- **CPU & I/O**: No CPU pegging for extended periods; bounded threads avoid thrashing disks.
- **Clippy clean**: `cargo clippy -- -D warnings` passes; no unnecessary `unsafe` outside wrappers.
- **Unit/integration tests** cover new concurrency paths, cache correctness, and UI updates.

---

## 2) High-Level Approach

1. **Keep scanning lean** (I/O-bound). Compute checksums during the directory walk; record metadata only.
2. **Defer tokenization** to a **separate background pass** that runs **after** the scan (or in parallel once a batch of paths is known).
3. **Cache aggressively** by checksum. Only tokenize on cache miss or checksum mismatch.
4. **Bounded parallelism** for tokenization using a Rayon pool sized to avoid thrashing (`max(2, num_cpus/2)`).
5. **UI progress**: stream periodic progress updates (throttled) and batch-merge results into state.
6. **Idiomatic polish**: reduce `Arc<Mutex<dyn Trait>>` in hot paths, introduce `ControlId` newtype, unify logging/status macros, centralize `unsafe` Win32 wrappers, and add focused tests.

---

## 3) Architecture Changes

### 3.1 New Worker Components
- **TokenizeWorker** (module): launches a bounded Rayon pool, exposes `recalc_tokens_async` with filters (e.g., only selected files).
- **Progress Channel**: internal `crossbeam_channel` or `std::sync::mpsc` to stream `(batch_totals, changed_items)` back to the main thread.
- **Batch Merger**: logic-side function that applies token details in batches (e.g., 64 files) and recomputes derived totals.

### 3.2 API Additions (Logic Layer)
```rust
/// Kick off background tokenization.
pub fn recalc_tokens_async<F>(&self, only_selected: bool, on_progress: F)
where
    F: Fn(TokenProgress) + Send + Sync + 'static;

pub struct TokenProgress {
    pub files_processed: usize,
    pub tokens_added: u64,
    pub changed: Vec<(PathBuf, FileTokenDetails)>,
}
```

### 3.3 Data Model Adjustments
- `ControlId` newtype to replace raw `i32`:
  ```rust
  #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
  pub struct ControlId(pub i32);
  ```
- `cached_file_token_details: HashMap<PathBuf, FileTokenDetails>` unchanged, but **accessed only on the main thread**; worker returns deltas.
- `FileTokenDetails` keeps `{ checksum: u64, token_count: u32 }` (or u64 if needed), plus optional timestamp for diagnostics.

---

## 4) Scanning Pipeline (Non-Blocking)

1. **Directory scan** runs on a **worker thread** (or minimal blocking in main with frequent yielding) and discovers files.
2. For each file:
   - Compute **checksum** (fast hash of content; consider memory-mapped I/O for very large files if needed).
   - Enqueue file metadata into a thread-safe collector (or just return a `Vec<FileNode>` at end of scan).
3. **Do not tokenize here.** Only attach checksums and file sizes.
4. Send initial UI update: “N files discovered. Computing tokens…”

**Optional prefetch**: As soon as a batch of (path, checksum, size) reaches a threshold (e.g., 256 items), start the token worker in parallel.

---

## 5) Tokenization Pipeline (Parallel, Bounded)

### 5.1 Pool Sizing
- Use Rayon with a **custom thread pool** sized to `max(2, num_cpus/2)`.
- Rationale: avoids saturating CPU + disk; leaves headroom for UI/main thread.

### 5.2 Work Selection
- **Mode A (initial run)**: all files, except those with a **cache hit** (checksum unchanged).
- **Mode B (refresh/“director scan”)**: only files whose checksum changed since last run.
- **Mode C (user-selected)**: only currently selected files for faster perceived responsiveness on large trees.

### 5.3 Execution
- Partition files into **batches** (e.g., 64). For each batch:
  - In parallel: for each file, if `cache_hit(checksum) => skip`, else `tokenize(file)`.
  - Collect `changed: Vec<(PathBuf, FileTokenDetails)>` for misses.
  - Aggregate a `batch_token_total` for quick progress increments.
- Send `TokenProgress` back to the main thread at batch completion (throttle sends to ~150–250 ms).

### 5.4 Applying Results
- On the **main thread**:
  - Merge `changed` into `cached_file_token_details`.
  - Recompute derived totals for **selected files** and overall profile.
  - Update token labels/status (`Tokens: {exact_so_far}`).
  - After large chunks, **persist profile** so subsequent launches are instant for unchanged files.

---

## 6) Instant Token Estimates (UX)

- Show an immediate estimate per file: `approx_tokens = bytes / 4` (tune factor by corpus).
- Maintain **two counters** in the UI:
  - **Estimated Tokens**: shown immediately after scan.
  - **Exact Tokens**: increments as batches finish; once equal to estimated (or close), hide the estimate badge.

This preserves a “snappy” feel while exact values arrive asynchronously.

---

## 7) Idiomatic Rust Improvements

### 7.1 Reduce `Arc<Mutex<dyn Trait>>` in Hot Paths
- Prefer **static dispatch** (`impl Trait` / generics) in core logic where types are known at compile time.
- If shared state is read-heavy: switch to `Arc<RwLock<T>>`.
- Keep trait objects for plugin-like/late-bound components (UI abstraction, platform bridge).

### 7.2 Newtype for Control IDs
- Replace `i32` with `ControlId` throughout UI code to prevent accidental misuse and self-document intent.

### 7.3 Error Handling
- Use `thiserror` for crate-level domain errors.
- Use `anyhow` at the binary boundary (top-level `main`) for rapid context chaining (`with_context(|| "...")`).

### 7.4 Logging/Status Macros
- Replace multiple severity-specific macros with a single macro that takes a **level** parameter:
  ```rust
  macro_rules! app_log {
      ($lvl:expr, $($arg:tt)*) => { ::log::$lvl!($($arg)*); }
  }
  // app_log!(info, "Scanning {} files", n);
  ```
- Or just rely on `log` facade + `tracing` with structured fields if you want spans around token batches.

### 7.5 Centralize `unsafe` Win32
- Wrap each unsafe call in a minimal safe function in a `platform::win32::ffi` module.
- Audit: ensure all UI code calls the safe wrappers; no stray `unsafe` in app logic.

### 7.6 Small State Helpers
- Add `validate_ui_state(window_id: ControlId) -> Result<&UiState>` to cut repeated `Option` unwrapping.
- Narrow public surface of UI-state; keep mutation sites limited and obvious.

---

## 8) Persistence & Caching

- Persist `cached_file_token_details` **after each sizable batch** (e.g., every 1k changed files) and at graceful shutdown.
- Key cache entries by **path + checksum**; if checksum matches, skip tokenization.
- Consider a **lightweight LRU** for very large repos if memory rises; but start simple (HashMap is fine).

---

## 9) Concurrency Details

- **Work Queue**: use `rayon::ThreadPool::install` + `par_iter()` over batches.
- **Data Sharing**: workers do not mutate shared maps; they **return deltas**.
- **Main-Thread Merge**: single-threaded merge ensures no lock contention on the cache HashMap.
- **Throttling**: gate UI updates by time (≥150 ms) and/or minimum increment (≥100 files).

---

## 10) Testing Strategy

### 10.1 Unit Tests
- `cache_hit` / `checksum_match` correctness.
- Tokenizer returns stable counts for a set of golden files.
- Batch-merging logic updates totals correctly (including subtract/add for replaces).

### 10.2 Concurrency Tests
- Simulate large file sets (1k–10k) with randomized “changed” subsets.
- Verify no deadlocks; main thread stays responsive (mock progress channel).

### 10.3 Integration Tests
- Full pipeline: scan → tokenize (parallel) → UI progress events.
- Persistence: run twice; second run should skip most tokenization if unchanged.

### 10.4 Property Tests (optional)
- QuickCheck/proptest for idempotence: merging the same delta twice produces same totals.

---

## 11) Telemetry & Diagnostics (Optional but Useful)
- Count of files tokenized vs skipped (cache hits).
- Time spent per stage: scan, batch tokenize, merge, persist.
- Max queue depth and average batch time to tune pool size.

---

## 12) Work Breakdown & Milestones

### Milestone A — Plumbing & Safety (0.5–1 day)
- [ ] Introduce `ControlId` newtype and migrate UI APIs.
- [ ] Create `platform::win32::ffi` safe wrappers; remove stray `unsafe` from logic.
- [ ] Unify logging/status macros or add `tracing` spans.

### Milestone B — Scan/Token Split (1–2 days)
- [ ] Ensure scan computes and records checksums only; remove tokenization from scan.
- [ ] Add `recalc_tokens_async(only_selected, on_progress)` entry point.
- [ ] Create progress channel type and main-thread merger.

### Milestone C — Parallel Token Pass (1–2 days)
- [ ] Implement Rayon pool with bounded threads.
- [ ] Implement batch processing (64 files/batch) and throttled UI updates.
- [ ] Add **instant estimate** UI + transition to exact totals.

### Milestone D — Cache & Persistence (0.5–1 day)
- [ ] Integrate checksum-based cache lookups.
- [ ] Persist profile after large batches and on shutdown.
- [ ] Add metrics counters (tokenized vs skipped; timings).

### Milestone E — Tests & Polish (1–2 days)
- [ ] Unit tests for cache/tokenizer/merger.
- [ ] Concurrency/integration tests for progress flow.
- [ ] Clippy + docs + final cleanup.

---

## 13) Example Skeletons

### 13.1 Token Worker Kickoff
```rust
pub fn recalc_tokens_async<F>(&self, only_selected: bool, on_progress: F)
where
    F: Fn(TokenProgress) + Send + Sync + 'static,
{
    let files = self.collect_targets(only_selected); // (PathBuf, checksum, size)
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(std::cmp::max(2, num_cpus::get() / 2))
        .build()
        .expect("build pool");

    let batch_size = 64;
    let (tx, rx) = crossbeam_channel::unbounded::<TokenProgress>();

    std::thread::spawn(move || {
        pool.install(|| {
            for chunk in files.chunks(batch_size) {
                let changed: Vec<_> = chunk.par_iter()
                    .filter_map(|f| tokenize_if_needed(f)) // returns Option<(PathBuf, FileTokenDetails)>
                    .collect();

                let tokens_added: u64 = changed.iter().map(|(_, d)| d.token_count as u64).sum();
                tx.send(TokenProgress {
                    files_processed: chunk.len(),
                    tokens_added,
                    changed,
                }).ok();
            }
        });
    });

    // Main-thread: drain rx in an event loop / dispatcher and call on_progress
    std::thread::spawn(move || {
        for prog in rx {
            on_progress(prog);
        }
    });
}
```

### 13.2 Main-Thread Merge (Pseudo)
```rust
fn apply_progress(&mut self, p: TokenProgress) {
    for (path, details) in p.changed {
        self.cached_file_token_details.insert(path, details);
    }
    self.recompute_totals_selected();
    self.update_token_labels(); // UI command
}
```

---

## 14) Rollout Plan

1. Ship **Milestones A–C** behind a feature flag (e.g., `--token-worker`).
2. Enable by default after tests are green and telemetry shows improved first-run times.
3. Keep the old path for one version as fallback; remove after bake-in.

---

## 15) Risks & Mitigations

- **Disk Thrash on HDDs**: Bound thread count; consider sorting files by directory to improve locality.
- **Large Files**: Cap parallel reads; optionally defer tokenization until selection.
- **UI Spam**: Throttle progress updates; coalesce label updates.
- **Cache Corruption**: Only mutate cache on the main thread; persist atomically (write-temp + rename).

---

## 16) Final Checklist

- [ ] UI remains smooth during initial scan & token pass.
- [ ] Token totals converge quickly; estimates disappear when exact complete.
- [ ] Rescan only processes changed files.
- [ ] Clippy/rustfmt clean; minimal `unsafe` contained in wrappers.
- [ ] Tests passing (unit + integration + concurrency).

---

**Ready to implement.** Ping me when you want a PR-style patch for Milestone B/C.
