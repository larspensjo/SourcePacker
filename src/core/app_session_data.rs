/*
 * This module defines the AppSessionData struct.
 * AppSessionData is responsible for holding and managing the core
 * application's session-specific data, such as the current profile name,
 * root folder, archive path, file cache, scan settings, and token counts,
 * including a cache of file token details. It aims to separate this
 * data from both the UI-specific state and the main application logic handler,
 * acting as a primary model component for session state.
 */
use crate::core::{
    FileNode, FileState, FileSystemScannerOperations, Profile, StateManagerOperations,
    TokenCounterOperations, models::FileTokenDetails,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

/*
 * Holds the core data for an active application session.
 * This includes information about the current profile being worked on (name, root folder,
 * archive path), the cache of scanned file nodes, the root path for file system scans,
 * the estimated token count for selected files, and a cache of token details for individual files.
 * TOOD: Can we make the elements private, to avoid direct access from outside?
 */
pub struct ProfileRuntimeData {
    /* The name of the currently loaded profile, if any. */
    pub profile_name: Option<String>,
    /* The path to the archive file for the current profile, if set. */
    pub archive_path: Option<PathBuf>,
    /* A snapshot of the file and directory nodes scanned from the root_path_for_scan. */
    pub file_system_snapshot_nodes: Vec<FileNode>,
    /* The root directory path from which file system scans are performed (derived from the active profile). */
    pub root_path_for_scan: PathBuf,
    /* The current estimated total token count for all selected files. */
    pub cached_token_count: usize,
    /* Stores cached token counts and checksums for files from the active profile.
     * This cache is updated based on current file checksums during profile activation. */
    pub cached_file_token_details: HashMap<PathBuf, FileTokenDetails>,
}

impl ProfileRuntimeData {
    /*
     * Creates a new `AppSessionData` instance with default values.
     * Initializes with no profile loaded, an empty file cache, a default
     * root scan path (current directory), zero tokens, and an empty token details cache.
     */
    pub fn new() -> Self {
        log::debug!("AppSessionData::new called - initializing default session data.");
        ProfileRuntimeData {
            profile_name: None,
            archive_path: None,
            file_system_snapshot_nodes: Vec::new(),
            root_path_for_scan: PathBuf::from("."), // Default to current directory
            cached_token_count: 0,
            cached_file_token_details: HashMap::new(),
        }
    }

    pub fn get_cached_token_count(&self) -> usize {
        self.cached_token_count
    }

    pub fn get_current_profile_name(&self) -> Option<&str> {
        self.profile_name.as_deref()
    }

    pub fn get_current_archive_path(&self) -> Option<&PathBuf> {
        self.archive_path.as_ref()
    }

    pub fn clear(&mut self) {
        log::debug!("Clearing AppSessionData state.");
        self.profile_name = None;
        self.archive_path = None;
        self.file_system_snapshot_nodes.clear();
        self.root_path_for_scan = PathBuf::from("."); // Reset to default current directory
        self.cached_token_count = 0;
        self.cached_file_token_details.clear();
    }

    // This helper remains static for now.
    // TODO: Shouldn't be made public, we should export a method on AppSessionData instead.
    pub(crate) fn gather_selected_deselected_paths_recursive(
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
                FileState::New => {}
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

    /*
     * Creates a `Profile` instance from the current session state.
     * This function gathers selected and deselected paths from the `file_nodes_cache`.
     * It uses `self.cached_file_token_details` to populate the `file_details` map
     * of the returned `Profile`. This is used when saving a profile.
     * Note: `populate_file_details_recursive_for_save` is called to ensure fresh token
     * counts for selected files are included in the Profile being saved.
     * TODO: Remove call of populate_file_details_recursive_for_save
     */
    pub(crate) fn create_profile_from_session_state(
        &self,
        new_profile_name: String,
        token_counter: &dyn TokenCounterOperations,
    ) -> Profile {
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();
        let mut file_details_for_save = HashMap::new();

        Self::gather_selected_deselected_paths_recursive(
            &self.file_system_snapshot_nodes,
            &mut selected_paths,
            &mut deselected_paths,
        );

        // Populate file_details_for_save with fresh data for currently selected files.
        Self::populate_file_details_recursive_for_save(
            &self.file_system_snapshot_nodes,
            &mut file_details_for_save,
            token_counter,
        );

        Profile {
            name: new_profile_name,
            root_folder: self.root_path_for_scan.clone(),
            selected_paths,
            deselected_paths,
            archive_path: self.archive_path.clone(),
            file_details: file_details_for_save, // Use the freshly populated details for saving
        }
    }

    /*
     * Recursively populates the `file_details_map` with checksums and token counts
     * for files that are marked as `Selected` in the `nodes` tree.
     * This helper is used by `create_profile_from_session_state` to build the
     * token cache that will be persisted in the profile being saved. It reads files
     * to get current token counts.
     */
    fn populate_file_details_recursive_for_save(
        nodes: &[FileNode],
        file_details_map: &mut HashMap<PathBuf, FileTokenDetails>,
        token_counter: &dyn TokenCounterOperations,
    ) {
        for node in nodes {
            if node.is_dir {
                Self::populate_file_details_recursive_for_save(
                    &node.children,
                    file_details_map,
                    token_counter,
                );
            } else if node.state == FileState::Selected {
                if let Some(checksum_val) = &node.checksum {
                    match fs::read_to_string(&node.path) {
                        Ok(content) => {
                            let token_count = token_counter.count_tokens(&content);
                            file_details_map.insert(
                                node.path.clone(),
                                FileTokenDetails {
                                    checksum: checksum_val.clone(),
                                    token_count,
                                },
                            );
                            log::trace!(
                                "AppSessionData (populate_file_details_for_save): Cached token count {} for selected file {:?} with checksum {}",
                                token_count,
                                node.path,
                                checksum_val
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "AppSessionData (populate_file_details_for_save): Failed to read file {:?} for token caching during profile save: {}",
                                node.path,
                                e
                            );
                        }
                    }
                } else {
                    log::warn!(
                        "AppSessionData (populate_file_details_for_save): Selected file {:?} has no checksum; cannot cache token count for profile save.",
                        node.path
                    );
                }
            }
        }
    }

    /*
     * Recalculates the estimated token count for all currently selected files
     * using `self.cached_file_token_details`.
     *
     * It iterates through the `file_nodes_cache`. For selected files, it attempts
     * to use the token count from `self.cached_file_token_details` if the
     * file's current checksum matches the cached checksum. If there's a mismatch,
     * the file is not in the cache, or the file node has no checksum, it falls
     * back to reading the file and calculating tokens on-the-fly.
     * The result is stored in `self.cached_current_token_count`.
     */
    pub(crate) fn update_token_count(
        &mut self,
        token_counter: &dyn TokenCounterOperations,
    ) -> usize {
        log::debug!(
            "Updating token count using session's cached_file_token_details for selected files."
        );
        let mut total_tokens: usize = 0;
        let mut files_processed_from_cache: usize = 0;
        let mut files_processed_fallback: usize = 0;
        let mut files_failed_fallback: usize = 0;

        // Helper function to recursively traverse the file node tree
        // TODO: cached_details_opt shouldn't have to be an Option?
        fn count_tokens_recursive_cached_inner(
            nodes: &[FileNode],
            cached_details_opt: Option<&HashMap<PathBuf, FileTokenDetails>>, // Pass immutable ref to the details map
            current_total_tokens: &mut usize,
            processed_cache: &mut usize,
            processed_fallback: &mut usize,
            failed_fallback: &mut usize,
            token_counter_ref: &dyn TokenCounterOperations,
        ) {
            for node in nodes {
                if !node.is_dir && node.state == FileState::Selected {
                    let mut token_value_for_file: Option<usize> = None;

                    if let Some(cached_details) = cached_details_opt {
                        if let Some(details) = cached_details.get(&node.path) {
                            if let Some(node_checksum) = node.checksum.as_ref() {
                                if *node_checksum == details.checksum {
                                    // Cache hit and checksum match
                                    token_value_for_file = Some(details.token_count);
                                    *processed_cache += 1;
                                    log::trace!(
                                        "TokenCount (Cache HIT): File {:?} - {} tokens (checksum {}).",
                                        node.path,
                                        details.token_count,
                                        details.checksum
                                    );
                                } else {
                                    // Checksum mismatch
                                    log::debug!(
                                        "TokenCount (Cache STALE): File {:?} checksum mismatch (disk: {}, cache: {}). Using fallback.",
                                        node.path,
                                        node_checksum,
                                        details.checksum
                                    );
                                }
                            } else {
                                // FileNode has no checksum (e.g., scan error for this file)
                                log::debug!(
                                    "TokenCount (Cache UNAVAILABLE): File {:?} has no disk checksum. Using fallback.",
                                    node.path
                                );
                            }
                        } else {
                            // File not in cache
                            log::debug!(
                                "TokenCount (Cache MISS): File {:?} not found in token cache. Using fallback.",
                                node.path
                            );
                        }
                    } else {
                        // No cached_file_token_details available at all (e.g., if AppSessionData had None, though it's not Option now)
                        log::warn!(
                            // Changed from error to warn as cached_file_token_details is not Option
                            "TokenCount: No cached_file_token_details map available. Using fallback for all selected files."
                        );
                    }

                    if token_value_for_file.is_none() {
                        // Fallback: read file and count tokens
                        *processed_fallback += 1;
                        log::debug!(
                            "TokenCount (FALLBACK): Processing file {:?} for token counting directly.",
                            node.path
                        );
                        match fs::read_to_string(&node.path) {
                            Ok(content) => {
                                let tokens_in_file = token_counter_ref.count_tokens(&content);
                                token_value_for_file = Some(tokens_in_file);
                            }
                            Err(e) => {
                                *failed_fallback += 1;
                                log::error!(
                                    "TokenCount (FALLBACK FAIL): Failed to read file {:?} for token counting: {}",
                                    node.path,
                                    e
                                );
                                // token_value_for_file remains None, contributing 0
                            }
                        }
                    }
                    *current_total_tokens += token_value_for_file.unwrap_or(0);
                } // end if selected file

                if node.is_dir {
                    count_tokens_recursive_cached_inner(
                        &node.children,
                        cached_details_opt,
                        current_total_tokens,
                        processed_cache,
                        processed_fallback,
                        failed_fallback,
                        token_counter_ref,
                    );
                }
            }
        }

        count_tokens_recursive_cached_inner(
            &self.file_system_snapshot_nodes,
            Some(&self.cached_file_token_details), // Pass immutable ref
            &mut total_tokens,
            &mut files_processed_from_cache,
            &mut files_processed_fallback,
            &mut files_failed_fallback,
            token_counter,
        );

        self.cached_token_count = total_tokens;
        log::debug!(
            "Token count updated: {}. Cache hits: {}, Fallbacks: {} ({} failed).",
            self.cached_token_count,
            files_processed_from_cache,
            files_processed_fallback,
            files_failed_fallback
        );
        self.cached_token_count
    }

    /*
     * Recursively iterates through `FileNode`s to update the `cached_file_token_details` cache.
     * For selected files, it checks if their checksum matches the cached one.
     * If not, or if the file is new to the cache, it recalculates and updates the token count.
     * For non-selected files, it removes them from the cache.
     * This method directly modifies `cached_details_mut` (which will be `self.cached_file_token_details`).
     * TODO: Shouldn't be made public, we should export a method on AppSessionData instead.
     */
    pub(crate) fn update_cached_file_details_recursive(
        nodes: &[FileNode],
        cached_details_mut: &mut HashMap<PathBuf, FileTokenDetails>,
        token_counter: &dyn TokenCounterOperations,
    ) {
        for node in nodes {
            if node.is_dir {
                Self::update_cached_file_details_recursive(
                    &node.children,
                    cached_details_mut,
                    token_counter,
                );
            } else {
                // It's a file
                let file_path = &node.path;
                let current_disk_checksum_opt = node.checksum.as_ref();

                if node.state == FileState::Selected {
                    if let Some(disk_cs) = current_disk_checksum_opt {
                        let needs_recalculation = match cached_details_mut.get(file_path) {
                            Some(cached_entry) => {
                                if cached_entry.checksum != *disk_cs {
                                    log::debug!(
                                        "Token cache for {:?} is stale (disk_cs: {}, cached_cs: {}). Recalculating.",
                                        file_path,
                                        disk_cs,
                                        cached_entry.checksum
                                    );
                                    true
                                } else {
                                    log::trace!(
                                        "Token cache for {:?} is up-to-date (checksum {}).",
                                        file_path,
                                        disk_cs
                                    );
                                    false // Checksum matches, no recalc needed
                                }
                            }
                            None => {
                                log::debug!(
                                    "Token cache miss for selected file {:?} (checksum {}). Calculating.",
                                    file_path,
                                    disk_cs
                                );
                                true // Not in cache, needs calculation
                            }
                        };

                        if needs_recalculation {
                            match fs::read_to_string(file_path) {
                                Ok(content) => {
                                    let token_count = token_counter.count_tokens(&content);
                                    cached_details_mut.insert(
                                        file_path.clone(),
                                        FileTokenDetails {
                                            checksum: disk_cs.clone(),
                                            token_count,
                                        },
                                    );
                                    log::debug!(
                                        "Updated token cache for {:?}: count {}, checksum {}",
                                        file_path,
                                        token_count,
                                        disk_cs
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to read file {:?} to update token cache: {}. Removing old entry if any.",
                                        file_path,
                                        e
                                    );
                                    cached_details_mut.remove(file_path);
                                }
                            }
                        }
                    } else {
                        // Selected file, but no checksum on disk (e.g., read error during scan)
                        log::warn!(
                            "Selected file {:?} has no disk checksum. Removing from token cache if present.",
                            file_path
                        );
                        if cached_details_mut.remove(file_path).is_some() {
                            log::debug!(
                                "Removed token cache entry for {:?} due to missing disk checksum.",
                                file_path
                            );
                        }
                    }
                } else {
                    // File is not selected, remove from cache for hygiene
                    if cached_details_mut.remove(file_path).is_some() {
                        log::debug!(
                            "Removed token cache entry for non-selected file {:?}",
                            file_path
                        );
                    }
                }
            }
        }
    }

    /*
     * Activates the given profile: copies its data into `AppSessionData`,
     * loads its associated file system data, applies the profile's selection
     * state to the scanned files, updates the session's internal `cached_file_token_details`
     * based on current checksums, and finally updates the session's total token count.
     * This is the primary method for making a profile fully active and ready for use.
     * The input `loaded_profile` is consumed as its data is transferred to `AppSessionData`.
     *
     * Returns `Ok(())` on success, or an `Err(String)` containing an error
     * message if file system scanning or processing fails.
     * TODO: Rename to activate_session_from_profile
     * TODO: The call to scan_directory() should be moved upwards. It shouldn't be the responsibility of this function.
     */
    pub fn activate_and_populate_data(
        &mut self,
        loaded_profile: Profile, // Profile object is now transient for loading
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn StateManagerOperations,
        token_counter: &dyn TokenCounterOperations,
    ) -> Result<(), String> {
        log::debug!(
            "AppSessionData: Activating and populating data for profile '{}'",
            loaded_profile.name
        );
        self.profile_name = Some(loaded_profile.name.clone());
        self.root_path_for_scan = loaded_profile.root_folder.clone();
        self.archive_path = loaded_profile.archive_path.clone();
        self.cached_file_token_details = loaded_profile.file_details.clone(); // Initial copy

        match file_system_scanner.scan_directory(&self.root_path_for_scan) {
            Ok(nodes) => {
                self.file_system_snapshot_nodes = nodes;
                log::debug!(
                    "AppSessionData: Scanned {} top-level nodes for profile '{:?}'.",
                    self.file_system_snapshot_nodes.len(),
                    self.profile_name
                );

                // Use selected_paths and deselected_paths from the loaded_profile for state_manager
                state_manager.apply_selection_states_to_nodes(
                    &mut self.file_system_snapshot_nodes,
                    &loaded_profile.selected_paths,
                    &loaded_profile.deselected_paths,
                );
                log::debug!(
                    "AppSessionData: Applied profile selection states from '{:?}' to the scanned tree.",
                    self.profile_name
                );

                // Update self.cached_file_token_details based on current disk state.
                log::debug!(
                    "AppSessionData: Updating session's cached_file_token_details for profile '{}' based on current disk state.",
                    loaded_profile.name
                );
                Self::update_cached_file_details_recursive(
                    &self.file_system_snapshot_nodes,
                    &mut self.cached_file_token_details,
                    token_counter,
                );

                self.update_token_count(token_counter);
                Ok(())
            }
            Err(e) => {
                let error_message = format!(
                    "Failed to scan directory {:?} for profile '{:?}': {:?}",
                    self.root_path_for_scan, self.profile_name, e
                );
                log::error!("AppSessionData: {}", error_message);
                self.file_system_snapshot_nodes.clear();
                self.cached_token_count = 0;
                // Also clear profile-specific data on scan error
                self.profile_name = None;
                self.archive_path = None;
                self.cached_file_token_details.clear();
                Err(error_message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        FileNode, FileState, FileSystemError, FileSystemScannerOperations, Profile,
        StateManagerOperations, TokenCounterOperations,
    };
    use std::collections::{HashMap, HashSet};
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use tempfile::{NamedTempFile, tempdir};

    /*
     * This module contains unit tests for `AppSessionData`.
     * It focuses on testing the logic for managing session state, profile creation from state,
     * token counting, and profile activation. Mocks are used for external dependencies
     * like file system scanning and state management operations.
     */

    // --- Mock Structures for activate_and_populate_data ---
    struct MockFileSystemScanner {
        scan_directory_results: Mutex<HashMap<PathBuf, Result<Vec<FileNode>, FileSystemError>>>,
        scan_directory_calls: Mutex<Vec<PathBuf>>,
    }

    impl MockFileSystemScanner {
        fn new() -> Self {
            MockFileSystemScanner {
                scan_directory_results: Mutex::new(HashMap::new()),
                scan_directory_calls: Mutex::new(Vec::new()),
            }
        }

        fn set_scan_directory_result(
            &self,
            path: &Path,
            result: Result<Vec<FileNode>, FileSystemError>,
        ) {
            self.scan_directory_results
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), result);
        }

        #[allow(dead_code)]
        fn get_scan_directory_calls(&self) -> Vec<PathBuf> {
            self.scan_directory_calls.lock().unwrap().clone()
        }
    }

    impl FileSystemScannerOperations for MockFileSystemScanner {
        fn scan_directory(&self, root_path: &Path) -> Result<Vec<FileNode>, FileSystemError> {
            self.scan_directory_calls
                .lock()
                .unwrap()
                .push(root_path.to_path_buf());
            match self.scan_directory_results.lock().unwrap().get(root_path) {
                Some(Ok(nodes)) => Ok(nodes.clone()),
                Some(Err(e)) => Err(match e {
                    FileSystemError::Io(io_err) => {
                        FileSystemError::Io(io::Error::new(io_err.kind(), "mock io error"))
                    }
                    FileSystemError::IgnoreError(_) => {
                        FileSystemError::IgnoreError(ignore::Error::from(io::Error::new(
                            io::ErrorKind::Other,
                            "mock ignore error",
                        )))
                    }
                    FileSystemError::InvalidPath(p) => FileSystemError::InvalidPath(p.clone()),
                }),
                None => Ok(Vec::new()),
            }
        }
    }

    struct MockStateManager {
        apply_profile_to_tree_calls:
            Mutex<Vec<(HashSet<PathBuf>, HashSet<PathBuf>, Vec<FileNode>)>>,
    }

    impl MockStateManager {
        fn new() -> Self {
            MockStateManager {
                apply_profile_to_tree_calls: Mutex::new(Vec::new()),
            }
        }

        #[allow(dead_code)]
        fn get_apply_profile_to_tree_calls(
            &self,
        ) -> Vec<(HashSet<PathBuf>, HashSet<PathBuf>, Vec<FileNode>)> {
            self.apply_profile_to_tree_calls.lock().unwrap().clone()
        }
    }

    impl StateManagerOperations for MockStateManager {
        fn apply_selection_states_to_nodes(
            &self,
            tree: &mut Vec<FileNode>,
            selected_paths: &HashSet<PathBuf>,
            deselected_paths: &HashSet<PathBuf>,
        ) {
            self.apply_profile_to_tree_calls.lock().unwrap().push((
                selected_paths.clone(),
                deselected_paths.clone(),
                tree.clone(),
            ));
            // Simulate actual behavior for test consistency
            for node in tree.iter_mut() {
                if selected_paths.contains(&node.path) {
                    node.state = FileState::Selected;
                } else if deselected_paths.contains(&node.path) {
                    node.state = FileState::Deselected;
                } else {
                    node.state = FileState::New;
                }
                if node.is_dir && !node.children.is_empty() {
                    self.apply_selection_states_to_nodes(
                        &mut node.children,
                        selected_paths,
                        deselected_paths,
                    );
                }
            }
        }
        fn update_folder_selection(&self, _node: &mut FileNode, _new_state: FileState) {}
    }

    // --- Mock TokenCounter ---
    struct MockTokenCounter {
        default_count: usize,
        counts_for_content: HashMap<String, usize>,
        call_log: Mutex<Vec<String>>,
    }
    impl MockTokenCounter {
        fn new(default_count: usize) -> Self {
            Self {
                default_count,
                counts_for_content: HashMap::new(),
                call_log: Mutex::new(Vec::new()),
            }
        }
        fn set_count_for_content(&mut self, content: &str, count: usize) {
            log::debug!(
                "MockTokenCounter: Setting count {} for content {:?}",
                count,
                content
            );
            self.counts_for_content.insert(content.to_string(), count);
        }

        fn get_call_log(&self) -> Vec<String> {
            self.call_log.lock().unwrap().clone()
        }
        fn clear_call_log(&self) {
            self.call_log.lock().unwrap().clear();
        }
    }
    impl TokenCounterOperations for MockTokenCounter {
        fn count_tokens(&self, text: &str) -> usize {
            log::debug!("MockTokenCounter: Counting tokens for text {:?}", text);
            self.call_log.lock().unwrap().push(text.to_string());
            if let Some(count) = self.counts_for_content.get(text) {
                log::debug!(
                    "MockTokenCounter: Found specific count {} for text {:?}",
                    count,
                    text
                );
                *count
            } else {
                log::debug!(
                    "MockTokenCounter: No specific count for text {:?}, returning default {}",
                    text,
                    self.default_count
                );
                self.default_count
            }
        }
    }

    fn create_temp_file_with_content(
        dir: &tempfile::TempDir,
        filename_prefix: &str,
        content: &str,
    ) -> (PathBuf, NamedTempFile) {
        let mut temp_file = tempfile::Builder::new()
            .prefix(filename_prefix)
            .suffix(".txt")
            .tempfile_in(dir.path())
            .unwrap();
        writeln!(temp_file, "{}", content).unwrap();
        (temp_file.path().to_path_buf(), temp_file)
    }

    #[test]
    fn test_app_session_data_new() {
        // Arrange
        crate::initialize_logging();

        // Act
        let session_data = ProfileRuntimeData::new();

        // Assert
        assert!(session_data.profile_name.is_none());
        assert!(session_data.archive_path.is_none());
        assert!(session_data.cached_file_token_details.is_empty());
        assert!(session_data.file_system_snapshot_nodes.is_empty());
        assert_eq!(session_data.root_path_for_scan, PathBuf::from("."));
        assert_eq!(session_data.cached_token_count, 0);
    }

    #[test]
    fn test_create_profile_from_session_state_basic() {
        // Arrange
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let file1_content_written = "content one";
        let (file1_path, _g1) =
            create_temp_file_with_content(&temp_dir, "f1", file1_content_written);
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", "content two");

        let mut session_data = ProfileRuntimeData {
            profile_name: Some("OldProfile".to_string()),
            root_path_for_scan: temp_dir.path().join("new_root"),
            archive_path: Some(temp_dir.path().join("old_archive.zip")),
            file_system_snapshot_nodes: vec![
                FileNode {
                    path: file1_path.clone(),
                    name: "file1.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some("cs1".to_string()),
                },
                FileNode {
                    path: file2_path.clone(),
                    name: "file2.txt".into(),
                    is_dir: false,
                    state: FileState::Deselected,
                    children: Vec::new(),
                    checksum: Some("cs2".to_string()),
                },
            ],
            cached_token_count: 0, // Not directly used by create_profile_from_session_state
            cached_file_token_details: HashMap::new(), // This will be ignored, new details are populated for save
        };
        let mut specific_token_counter = MockTokenCounter::new(0);
        let file1_content_as_read = format!("{}\n", file1_content_written);
        specific_token_counter.set_count_for_content(&file1_content_as_read, 10);

        // Act
        let new_profile = session_data
            .create_profile_from_session_state("NewProfile".to_string(), &specific_token_counter);

        // Assert
        assert_eq!(new_profile.name, "NewProfile");
        assert_eq!(new_profile.root_folder, temp_dir.path().join("new_root"));
        assert!(new_profile.selected_paths.contains(&file1_path));
        assert!(!new_profile.selected_paths.contains(&file2_path));
        assert!(new_profile.deselected_paths.contains(&file2_path));
        assert_eq!(
            new_profile.archive_path,
            Some(temp_dir.path().join("old_archive.zip"))
        );
        assert_eq!(
            new_profile.file_details.len(),
            1,
            "Only selected file should have details"
        );
        assert!(new_profile.file_details.contains_key(&file1_path));
        let detail1 = new_profile.file_details.get(&file1_path).unwrap();
        assert_eq!(detail1.checksum, "cs1");
        assert_eq!(detail1.token_count, 10);
    }

    #[test]
    fn test_create_profile_from_session_state_no_archive_path() {
        // Arrange
        crate::initialize_logging();
        let session_data = ProfileRuntimeData {
            profile_name: None,
            root_path_for_scan: PathBuf::from("/root"),
            archive_path: None,
            file_system_snapshot_nodes: vec![],
            cached_token_count: 0,
            cached_file_token_details: HashMap::new(),
        };
        let mock_token_counter = MockTokenCounter::new(0);

        // Act
        let new_profile = session_data
            .create_profile_from_session_state("ProfileNoArchive".to_string(), &mock_token_counter);

        // Assert
        assert_eq!(new_profile.archive_path, None);
        assert!(new_profile.file_details.is_empty());
    }

    #[test]
    fn test_update_token_count_selected_files_cache_hit() {
        // Arrange
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let content1 = "hello world";
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1", content1);
        let cs1 = "cs1_match".to_string();

        let content2 = "another example";
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", content2);
        let cs2 = "cs2_match".to_string();

        let mut file_details_cache_for_session = HashMap::new();
        file_details_cache_for_session.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: cs1.clone(),
                token_count: 10,
            },
        );
        file_details_cache_for_session.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: cs2.clone(),
                token_count: 20,
            },
        );

        let mut session_data = ProfileRuntimeData {
            profile_name: Some("TestProfile".to_string()),
            root_path_for_scan: temp_dir.path().to_path_buf(),
            archive_path: None,
            cached_file_token_details: file_details_cache_for_session,
            file_system_snapshot_nodes: vec![
                FileNode {
                    path: file1_path.clone(),
                    name: "f1.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs1.clone()),
                },
                FileNode {
                    path: file2_path.clone(),
                    name: "f2.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs2.clone()),
                },
            ],
            cached_token_count: 0,
        };
        let mock_token_counter = MockTokenCounter::new(0); // Default, should not be used

        // Act
        let count = session_data.update_token_count(&mock_token_counter);

        // Assert
        assert_eq!(count, 30, "Expected 10 (f1) + 20 (f2) from cache");
        assert_eq!(session_data.cached_token_count, 30);
        assert!(
            mock_token_counter.get_call_log().is_empty(),
            "Token counter should not be called on cache hits"
        );
    }

    #[test]
    fn test_activate_and_populate_data_success_and_updates_session_file_details() {
        // Arrange
        crate::initialize_logging();
        let mut session_data = ProfileRuntimeData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mut mock_token_counter = MockTokenCounter::new(0);

        let profile_name = "TestProfileDetailsUpdate";
        let temp_dir = tempdir().unwrap();
        let root_folder = temp_dir.path().to_path_buf();

        let content1 = "file one content"; // 10 tokens
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1", content1);
        let file1_checksum_disk = "cs1_disk_final".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content1), 10);

        let content2 = "file two changed content"; // 20 tokens
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", content2);
        let file2_checksum_disk = "cs2_disk_new_final".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content2), 20);

        let content3 = "file three new selected"; // 30 tokens
        let (file3_path, _g3) = create_temp_file_with_content(&temp_dir, "f3", content3);
        let file3_checksum_disk = "cs3_disk_final".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content3), 30);

        let content4 = "file four was selected now not"; // Not counted
        let (file4_path, _g4) = create_temp_file_with_content(&temp_dir, "f4", content4);
        let file4_checksum_disk = "cs4_disk_final".to_string();

        let content5 = "file five selected no checksum"; // 50 tokens (fallback)
        let (file5_path, _g5) = create_temp_file_with_content(&temp_dir, "f5", content5);
        mock_token_counter.set_count_for_content(&format!("{}\n", content5), 50);

        let mut initial_profile_file_details = HashMap::new();
        // file1: In loaded profile, checksum matches what scan will find.
        initial_profile_file_details.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: file1_checksum_disk.clone(),
                token_count: 10,
            },
        );
        // file2: In loaded profile, but checksum is stale.
        initial_profile_file_details.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: "cs2_disk_old_stale".to_string(),
                token_count: 15,
            },
        );
        // file4: In loaded profile, selected, but will be deselected by apply_profile_to_tree simulation.
        initial_profile_file_details.insert(
            file4_path.clone(),
            FileTokenDetails {
                checksum: file4_checksum_disk.clone(),
                token_count: 40,
            },
        );

        let mut loaded_profile = Profile {
            name: profile_name.to_string(),
            root_folder: root_folder.clone(),
            selected_paths: HashSet::new(), // These will be used by mock_state_manager
            deselected_paths: HashSet::new(),
            archive_path: Some(PathBuf::from("/dummy/archive.txt")),
            file_details: initial_profile_file_details,
        };
        // Simulate profile selections for apply_profile_to_tree
        loaded_profile.selected_paths.insert(file1_path.clone());
        loaded_profile.selected_paths.insert(file2_path.clone());
        loaded_profile.selected_paths.insert(file3_path.clone()); // file3 is newly selected
        loaded_profile.selected_paths.insert(file5_path.clone()); // file5 selected
        // file4 will be deselected by not being in selected_paths (MockStateManager logic)

        let nodes_from_scanner = vec![
            FileNode {
                path: file1_path.clone(),
                name: "f1.txt".into(),
                is_dir: false,
                state: FileState::New,
                children: Vec::new(),
                checksum: Some(file1_checksum_disk.clone()),
            },
            FileNode {
                path: file2_path.clone(),
                name: "f2.txt".into(),
                is_dir: false,
                state: FileState::New,
                children: Vec::new(),
                checksum: Some(file2_checksum_disk.clone()),
            },
            FileNode {
                path: file3_path.clone(),
                name: "f3.txt".into(),
                is_dir: false,
                state: FileState::New,
                children: Vec::new(),
                checksum: Some(file3_checksum_disk.clone()),
            },
            FileNode {
                path: file4_path.clone(),
                name: "f4.txt".into(),
                is_dir: false,
                state: FileState::New,
                children: Vec::new(),
                checksum: Some(file4_checksum_disk.clone()),
            },
            FileNode {
                path: file5_path.clone(),
                name: "f5.txt".into(),
                is_dir: false,
                state: FileState::New,
                children: Vec::new(),
                checksum: None,
            }, // No checksum from scanner
        ];
        mock_scanner.set_scan_directory_result(&root_folder, Ok(nodes_from_scanner.clone()));
        mock_token_counter.clear_call_log();

        // Act
        let result = session_data.activate_and_populate_data(
            loaded_profile.clone(), // Pass the loaded profile
            &mock_scanner,
            &mock_state_manager,
            &mock_token_counter,
        );

        // Assert
        assert!(result.is_ok());
        assert_eq!(session_data.profile_name.as_deref(), Some(profile_name));
        assert_eq!(
            session_data.archive_path.as_deref(),
            Some(Path::new("/dummy/archive.txt"))
        );

        // Check session_data.cached_file_token_details
        let session_details = &session_data.cached_file_token_details;
        // file1: Should remain 10 tokens, checksum matches.
        assert_eq!(session_details.get(&file1_path).unwrap().token_count, 10);
        assert_eq!(
            session_details.get(&file1_path).unwrap().checksum,
            file1_checksum_disk
        );
        // file2: Should be updated to 20 tokens, checksum updated.
        assert_eq!(session_details.get(&file2_path).unwrap().token_count, 20);
        assert_eq!(
            session_details.get(&file2_path).unwrap().checksum,
            file2_checksum_disk
        );
        // file3: Should be added with 30 tokens.
        assert_eq!(session_details.get(&file3_path).unwrap().token_count, 30);
        assert_eq!(
            session_details.get(&file3_path).unwrap().checksum,
            file3_checksum_disk
        );
        // file4: Should be removed as it's not selected after apply_profile_to_tree.
        assert!(session_details.get(&file4_path).is_none());
        // file5: Should be removed from cached_file_token_details because FileNode had no checksum.
        assert!(session_details.get(&file5_path).is_none());

        // Check overall token count (after apply_profile_to_tree and updates)
        // Selected: file1 (10), file2 (20), file3 (30).
        // file5 is selected by profile, but FileNode has no checksum. update_cached_file_details_recursive removes it.
        // update_token_count then does a fallback for file5, reading its content and using mock_token_counter (50).
        // Total = 10 + 20 + 30 + 50 = 110.
        assert_eq!(
            session_data.cached_token_count, 110,
            "Total token count mismatch"
        );

        // Check calls to mock_token_counter
        let calls = mock_token_counter.get_call_log();
        // update_cached_file_details_recursive calls:
        // - file2 (stale checksum in loaded_profile.file_details -> recalculate for session cache)
        // - file3 (new to session cache -> calculate)
        // update_token_count calls:
        // - file1 (cache hit in session_data.cached_file_token_details)
        // - file2 (cache hit in session_data.cached_file_token_details)
        // - file3 (cache hit in session_data.cached_file_token_details)
        // - file5 (fallback as it's not in session_data.cached_file_token_details due to no FileNode checksum)
        assert!(
            calls.contains(&format!("{}\n", content2)),
            "Content2 for cache update"
        );
        assert!(
            calls.contains(&format!("{}\n", content3)),
            "Content3 for cache update"
        );
        assert!(
            calls.contains(&format!("{}\n", content5)),
            "Content5 for final count fallback"
        );
        assert!(
            !calls.contains(&format!("{}\n", content1)),
            "Content1 should be cache hit from loaded_profile, then session_cache"
        );
        assert_eq!(
            calls.len(),
            3,
            "Expected 3 calls to token_counter: content2(cache_update), content3(cache_update), content5(final_count_fallback)"
        );
    }

    #[test]
    fn test_activate_and_populate_data_scan_error() {
        // Arrange
        let mut session_data = ProfileRuntimeData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mock_token_counter = MockTokenCounter::new(0);

        let profile_name = "ErrorProfile";
        let root_folder = PathBuf::from("/error/root");
        let profile = Profile::new(profile_name.to_string(), root_folder.clone()); // Loaded profile

        mock_scanner.set_scan_directory_result(
            &root_folder,
            Err(FileSystemError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "scan failed",
            ))),
        );

        // Act
        let result = session_data.activate_and_populate_data(
            profile.clone(), // Pass the loaded profile
            &mock_scanner,
            &mock_state_manager,
            &mock_token_counter,
        );

        // Assert
        assert!(result.is_err());
        if let Err(msg) = result {
            assert!(msg.contains("Failed to scan directory"));
            assert!(msg.contains(&profile_name)); // Profile name should still be in error msg context
        }
        // Check that AppSessionData fields are reset/cleared
        assert!(session_data.profile_name.is_none());
        assert!(session_data.archive_path.is_none());
        assert!(session_data.cached_file_token_details.is_empty());
        assert!(session_data.file_system_snapshot_nodes.is_empty());
        assert_eq!(session_data.cached_token_count, 0);

        assert_eq!(mock_scanner.get_scan_directory_calls().len(), 1);
        assert_eq!(
            mock_state_manager.get_apply_profile_to_tree_calls().len(),
            0
        );
    }
}
