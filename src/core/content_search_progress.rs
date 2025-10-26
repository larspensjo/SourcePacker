/*
 * Defines the data structures exchanged between the asynchronous content-search worker
 * and the application logic. Each `ContentSearchResult` reports whether a specific file
 * matches the active search query, while `ContentSearchProgress` batches those results and
 * tags whether the batch is the final message for the request.
 */
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSearchResult {
    pub path: PathBuf,
    pub matches: bool,
}

#[derive(Debug, Clone)]
pub struct ContentSearchProgress {
    pub is_final: bool,
    pub results: Vec<ContentSearchResult>,
}
