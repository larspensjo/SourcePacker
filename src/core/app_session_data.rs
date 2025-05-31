/*
 * This module defines the AppSessionData struct.
 * AppSessionData is responsible for holding and managing the core
 * application's session-specific data, such as the current profile,
 * file cache, scan settings, and token counts. It aims to separate this
 * data from both the UI-specific state and the main application logic handler,
 * acting as a primary model component for session state.
 */
use crate::core::{
    FileNode, FileState, FileSystemScannerOperations, Profile, StateManagerOperations,
}; // Import necessary types
use std::collections::HashSet;
use std::fs;
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
    pub cached_current_token_count: usize,
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
            cached_current_token_count: 0,
        }
    }

    // This helper remains static for now.
    fn gather_selected_deselected_paths_recursive(
        nodes: &[FileNode],
        selected: &mut HashSet<PathBuf>,
        deselected: &mut HashSet<PathBuf>,
    ) {
        for node in nodes {
            match node.state {
                FileState::Selected => {
                    selected.insert(node.path.clone());
                }
                FileState::Deselected => {
                    deselected.insert(node.path.clone());
                }
                FileState::Unknown => {}
            }
            if node.is_dir && !node.children.is_empty() {
                Self::gather_selected_deselected_paths_recursive(
                    &node.children,
                    selected,
                    deselected,
                );
            }
        }
    }

    pub(crate) fn create_profile_from_session_state(&self, new_profile_name: String) -> Profile {
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();

        Self::gather_selected_deselected_paths_recursive(
            &self.file_nodes_cache, // Use app_session_data
            &mut selected_paths,
            &mut deselected_paths,
        );

        Profile {
            name: new_profile_name,
            root_folder: self.root_path_for_scan.clone(), // Use app_session_data
            selected_paths,
            deselected_paths,
            archive_path: self
                .current_profile_cache // Use app_session_data
                .as_ref()
                .and_then(|p| p.archive_path.clone()),
        }
    }

    /*
     * Recalculates the estimated token count for all currently selected files.
     * Data is read from `file_nodes_cache` and result cached
     * in `current_token_count`. UI update is requested if UI state exists.
     */
    pub(crate) fn update_token_count(&mut self) -> usize {
        log::debug!("Recalculating token count for selected files and requesting display.");
        let mut total_tokens: usize = 0;
        let mut files_processed_for_tokens: usize = 0;
        let mut files_failed_to_read_for_tokens: usize = 0;

        // Helper function to recursively traverse the file node tree
        fn count_tokens_recursive_inner(
            nodes: &[FileNode], // Operates on FileNode slice
            current_total_tokens: &mut usize,
            files_processed: &mut usize,
            files_failed: &mut usize,
        ) {
            for node in nodes {
                if !node.is_dir && node.state == FileState::Selected {
                    *files_processed += 1;
                    match fs::read_to_string(&node.path) {
                        Ok(content) => {
                            let tokens_in_file = crate::core::estimate_tokens_tiktoken(&content);
                            *current_total_tokens += tokens_in_file;
                        }
                        Err(e) => {
                            *files_failed += 1;
                            log::warn!(
                                "TokenCount: Failed to read file {:?} for token counting: {}",
                                node.path,
                                e
                            );
                        }
                    }
                }
                if node.is_dir {
                    count_tokens_recursive_inner(
                        &node.children,
                        current_total_tokens,
                        files_processed,
                        files_failed,
                    );
                }
            }
        }

        count_tokens_recursive_inner(
            &self.file_nodes_cache, // Use app_session_data
            &mut total_tokens,
            &mut files_processed_for_tokens,
            &mut files_failed_to_read_for_tokens,
        );

        self.cached_current_token_count = total_tokens; // Store in app_session_data
        log::debug!(
            "Token count updated internally: {} tokens from {} selected files ({} files failed to read).",
            self.cached_current_token_count,
            files_processed_for_tokens,
            files_failed_to_read_for_tokens
        );

        // Status message macro will use ui_state to get window_id if available
        // app_info!(self, "Tokens: {}", self.current_token_count);
        self.cached_current_token_count
    }

    /*
     * Activates the given profile, loads its associated file system data,
     * applies the profile's selection state to the scanned files, and updates
     * the token count. This is the primary method for making a profile fully
     * active and ready for use in the session.
     *
     * Returns `Ok(())` on success, or an `Err(String)` containing an error
     * message if file system scanning or processing fails.
     */
    pub fn activate_and_populate_data(
        &mut self,
        profile_to_activate: Profile, // Takes ownership
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn StateManagerOperations,
    ) -> Result<(), String> {
        log::debug!(
            "AppSessionData: Activating and populating data for profile '{}'",
            profile_to_activate.name
        );
        self.current_profile_name = Some(profile_to_activate.name.clone());
        self.root_path_for_scan = profile_to_activate.root_folder.clone();
        self.current_profile_cache = Some(profile_to_activate.clone());

        match file_system_scanner.scan_directory(&self.root_path_for_scan) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                log::debug!(
                    "AppSessionData: Scanned {} top-level nodes for profile '{:?}'.",
                    self.file_nodes_cache.len(),
                    self.current_profile_name
                );
                state_manager.apply_profile_to_tree(
                    &mut self.file_nodes_cache,
                    self.current_profile_cache.as_ref().unwrap(),
                );
                log::debug!(
                    "AppSessionData: Applied profile '{:?}' to the scanned tree.",
                    self.current_profile_name
                );
                self.update_token_count(); // Update token count after successful scan and state application
                Ok(())
            }
            Err(e) => {
                let error_message = format!(
                    "Failed to scan directory {:?} for profile '{:?}': {:?}",
                    self.root_path_for_scan,
                    self.current_profile_name, // Use the name now stored in self
                    e
                );
                log::error!("AppSessionData: {}", error_message);
                self.file_nodes_cache.clear(); // Ensure cache is clear on error
                Err(error_message)
            }
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
        assert_eq!(session_data.cached_current_token_count, 0);
    }
}
