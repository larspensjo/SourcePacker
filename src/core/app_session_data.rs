/*
 * This module defines the AppSessionData struct.
 * AppSessionData is responsible for holding and managing the core
 * application's session-specific data, such as the current profile,
 * file cache, scan settings, and token counts. It aims to separate this
 * data from both the UI-specific state and the main application logic handler,
 * acting as a primary model component for session state.
 */
use crate::core::{FileNode, Profile}; // Import necessary types
use std::path::PathBuf;

/*
 * Holds the core data for an active application session.
 * This includes information about the current profile being worked on,
 * the cache of scanned file nodes, the root path for file system scans,
 * and the estimated token count for selected files.
 */
pub struct AppSessionData {
    /* The name of the currently loaded profile, if any. */
    pub current_profile_name: Option<String>,
    /* The cached data of the currently loaded profile, if any. */
    pub current_profile_cache: Option<Profile>,
    /* A cache of the file and directory nodes scanned from the root_path_for_scan. */
    pub file_nodes_cache: Vec<FileNode>,
    /* The root directory path from which file system scans are performed. */
    pub root_path_for_scan: PathBuf,
    /* The current estimated total token count for all selected files. */
    pub current_token_count: usize,
}

impl AppSessionData {
    /*
     * Creates a new `AppSessionData` instance with default values.
     * Initializes with no profile loaded, an empty file cache, a default
     * root scan path (current directory), and zero tokens.
     */
    pub fn new() -> Self {
        log::debug!("AppSessionData::new called - initializing default session data.");
        AppSessionData {
            current_profile_name: None,
            current_profile_cache: None,
            file_nodes_cache: Vec::new(),
            root_path_for_scan: PathBuf::from("."), // Default to current directory
            current_token_count: 0,
        }
    }
}

// Minimal unit test for the constructor
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_session_data_new() {
        let session_data = AppSessionData::new();
        assert!(session_data.current_profile_name.is_none());
        assert!(session_data.current_profile_cache.is_none());
        assert!(session_data.file_nodes_cache.is_empty());
        assert_eq!(session_data.root_path_for_scan, PathBuf::from("."));
        assert_eq!(session_data.current_token_count, 0);
    }
}
