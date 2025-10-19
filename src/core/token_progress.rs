/*
 * Defines shared data structures used to shuttle token recalculation progress between
 * background workers and the main application logic. The module keeps the transport
 * types lightweight and serializable so that UI-facing code can remain decoupled from
 * the worker implementation details.
 *
 * Each structure focuses on representing immutable snapshots of work completed so far,
 * ensuring that consumers can merge updates deterministically while maintaining cache
 * correctness and real-time progress indicators.
 */
use crate::core::file_node::FileTokenDetails;
use std::path::PathBuf;

/*
 * Encapsulates progress reporting data emitted by asynchronous token recalculation.
 * Each entry tracks the token information for a single file, allowing the main thread
 * to merge cache mutations and accumulate totals without tight coupling to worker internals.
 */
#[derive(Debug, Clone)]
pub struct TokenProgressEntry {
    pub path: PathBuf,
    pub token_count: usize,
    pub is_selected: bool,
    pub details: Option<FileTokenDetails>,
    pub invalidate_cache: bool,
}

/*
 * Summarizes a batch of token recalculation updates flowing from the worker thread.
 * The `entries` vector carries detailed per-file results while the counters help the
 * UI layer surface coarse progress indicators. A final message is flagged via `is_final`.
 */
#[derive(Debug, Clone)]
pub struct TokenProgress {
    pub entries: Vec<TokenProgressEntry>,
    pub files_processed: usize,
    pub total_files: usize,
    pub is_final: bool,
}
