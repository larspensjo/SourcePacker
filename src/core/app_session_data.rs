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
    TokenCounterOperations, models::FileTokenDetails,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

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

    /*
     * Creates a `Profile` instance from the current session state.
     * This function gathers selected and deselected paths from the `file_nodes_cache`.
     * Crucially, for files that are selected and have a checksum, it reads their content,
     * counts their tokens, and stores this information along with the checksum in the
     * `file_details` map of the returned `Profile`. This is used when saving a profile
     * to persist token count information.
     */
    pub(crate) fn create_profile_from_session_state(
        &self,
        new_profile_name: String,
        token_counter: &dyn TokenCounterOperations,
    ) -> Profile {
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();
        let mut file_details_map = std::collections::HashMap::new();

        Self::gather_selected_deselected_paths_recursive(
            &self.file_nodes_cache,
            &mut selected_paths,
            &mut deselected_paths,
        );

        Self::populate_file_details_recursive(
            &self.file_nodes_cache,
            &mut file_details_map,
            token_counter,
        );

        Profile {
            name: new_profile_name,
            root_folder: self.root_path_for_scan.clone(),
            selected_paths,
            deselected_paths,
            archive_path: self
                .current_profile_cache
                .as_ref()
                .and_then(|p| p.archive_path.clone()),
            file_details: file_details_map,
        }
    }

    /*
     * Recursively populates the `file_details_map` with checksums and token counts
     * for files that are marked as `Selected` in the `nodes` tree.
     * This helper is used by `create_profile_from_session_state` to build the
     * token cache that will be persisted in the profile.
     */
    fn populate_file_details_recursive(
        nodes: &[FileNode],
        file_details_map: &mut std::collections::HashMap<PathBuf, FileTokenDetails>,
        token_counter: &dyn TokenCounterOperations,
    ) {
        for node in nodes {
            if node.is_dir {
                Self::populate_file_details_recursive(
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
                                "AppSessionData (populate_file_details): Cached token count {} for selected file {:?} with checksum {}",
                                token_count,
                                node.path,
                                checksum_val
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "AppSessionData (populate_file_details): Failed to read file {:?} for token caching during profile save: {}",
                                node.path,
                                e
                            );
                        }
                    }
                } else {
                    log::warn!(
                        "AppSessionData (populate_file_details): Selected file {:?} has no checksum; cannot cache token count for profile save.",
                        node.path
                    );
                }
            }
        }
    }

    /*
     * Recalculates the estimated token count for all currently selected files
     * using the `file_details` cache in `current_profile_cache`.
     *
     * It iterates through the `file_nodes_cache`. For selected files, it attempts
     * to use the token count from `current_profile_cache.file_details` if the
     * file's current checksum matches the cached checksum. If there's a mismatch,
     * the file is not in the cache, or the file node has no checksum, it falls
     * back to reading the file and calculating tokens on-the-fly.
     * The result is stored in `self.cached_current_token_count`.
     */
    pub(crate) fn update_token_count(
        &mut self,
        token_counter: &dyn TokenCounterOperations,
    ) -> usize {
        log::debug!("Updating token count using cache for selected files.");
        let mut total_tokens: usize = 0;
        let mut files_processed_from_cache: usize = 0;
        let mut files_processed_fallback: usize = 0;
        let mut files_failed_fallback: usize = 0;

        // Helper function to recursively traverse the file node tree
        fn count_tokens_recursive_cached_inner(
            nodes: &[FileNode],
            profile_cache_opt: Option<&Profile>, // Pass immutable ref to profile_cache
            current_total_tokens: &mut usize,
            processed_cache: &mut usize,
            processed_fallback: &mut usize,
            failed_fallback: &mut usize,
            token_counter_ref: &dyn TokenCounterOperations,
        ) {
            for node in nodes {
                if !node.is_dir && node.state == FileState::Selected {
                    let mut token_value_for_file: Option<usize> = None;

                    if let Some(profile) = profile_cache_opt {
                        if let Some(details) = profile.file_details.get(&node.path) {
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
                                    log::warn!(
                                        "TokenCount (Cache STALE): File {:?} checksum mismatch (disk: {}, cache: {}). Using fallback.",
                                        node.path,
                                        node_checksum,
                                        details.checksum
                                    );
                                }
                            } else {
                                // FileNode has no checksum (e.g., scan error for this file)
                                log::warn!(
                                    "TokenCount (Cache UNAVAILABLE): File {:?} has no disk checksum. Using fallback.",
                                    node.path
                                );
                            }
                        } else {
                            // File not in cache
                            log::warn!(
                                "TokenCount (Cache MISS): File {:?} not found in token cache. Using fallback.",
                                node.path
                            );
                        }
                    } else {
                        // No profile cache available at all
                        log::warn!(
                            "TokenCount: No profile cache available. Using fallback for all selected files."
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
                        profile_cache_opt,
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
            &self.file_nodes_cache,
            self.current_profile_cache.as_ref(), // Pass immutable ref
            &mut total_tokens,
            &mut files_processed_from_cache,
            &mut files_processed_fallback,
            &mut files_failed_fallback,
            token_counter,
        );

        self.cached_current_token_count = total_tokens;
        log::debug!(
            "Token count updated: {}. Cache hits: {}, Fallbacks: {} ({} failed).",
            self.cached_current_token_count,
            files_processed_from_cache,
            files_processed_fallback,
            files_failed_fallback
        );
        self.cached_current_token_count
    }

    /*
     * Recursively iterates through `FileNode`s to update the `file_details` cache.
     * For selected files, it checks if their checksum matches the cached one.
     * If not, or if the file is new, it recalculates and updates the token count.
     * For non-selected files, it removes them from the cache.
     */
    fn update_cached_file_details_recursive(
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
     * Activates the given profile, loads its associated file system data,
     * applies the profile's selection state to the scanned files, updates
     * the profile's internal `file_details` token cache based on current checksums,
     * and finally updates the session's total token count using this cache.
     * This is the primary method for making a profile fully active and ready for use.
     *
     * Returns `Ok(())` on success, or an `Err(String)` containing an error
     * message if file system scanning or processing fails.
     */
    pub fn activate_and_populate_data(
        &mut self,
        profile_to_activate: Profile,
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn StateManagerOperations,
        token_counter: &dyn TokenCounterOperations,
    ) -> Result<(), String> {
        log::debug!(
            "AppSessionData: Activating and populating data for profile '{}'",
            profile_to_activate.name
        );
        self.current_profile_name = Some(profile_to_activate.name.clone());
        self.root_path_for_scan = profile_to_activate.root_folder.clone();
        self.current_profile_cache = Some(profile_to_activate);

        match file_system_scanner.scan_directory(&self.root_path_for_scan) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                log::debug!(
                    "AppSessionData: Scanned {} top-level nodes for profile '{:?}'.",
                    self.file_nodes_cache.len(),
                    self.current_profile_name
                );

                let profile_ref = self.current_profile_cache.as_ref().unwrap();
                state_manager.apply_profile_to_tree(&mut self.file_nodes_cache, profile_ref);
                log::debug!(
                    "AppSessionData: Applied profile selection states from '{:?}' to the scanned tree.",
                    self.current_profile_name
                );

                if let Some(profile_cache_mut) = self.current_profile_cache.as_mut() {
                    log::debug!(
                        "AppSessionData: Updating file_details cache in profile '{}' based on current disk state.",
                        profile_cache_mut.name
                    );
                    Self::update_cached_file_details_recursive(
                        &self.file_nodes_cache,
                        &mut profile_cache_mut.file_details,
                        token_counter,
                    );
                } else {
                    log::error!(
                        "AppSessionData: current_profile_cache was None unexpectedly before updating file_details."
                    );
                }

                self.update_token_count(token_counter);
                Ok(())
            }
            Err(e) => {
                let error_message = format!(
                    "Failed to scan directory {:?} for profile '{:?}': {:?}",
                    self.root_path_for_scan, self.current_profile_name, e
                );
                log::error!("AppSessionData: {}", error_message);
                self.file_nodes_cache.clear();
                self.cached_current_token_count = 0;
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
        apply_profile_to_tree_calls: Mutex<Vec<(Profile, Vec<FileNode>)>>,
    }

    impl MockStateManager {
        fn new() -> Self {
            MockStateManager {
                apply_profile_to_tree_calls: Mutex::new(Vec::new()),
            }
        }

        #[allow(dead_code)]
        fn get_apply_profile_to_tree_calls(&self) -> Vec<(Profile, Vec<FileNode>)> {
            self.apply_profile_to_tree_calls.lock().unwrap().clone()
        }
    }

    impl StateManagerOperations for MockStateManager {
        fn apply_profile_to_tree(&self, tree: &mut Vec<FileNode>, profile: &Profile) {
            self.apply_profile_to_tree_calls
                .lock()
                .unwrap()
                .push((profile.clone(), tree.clone()));
            for node in tree.iter_mut() {
                if profile.selected_paths.contains(&node.path) {
                    node.state = FileState::Selected;
                } else if profile.deselected_paths.contains(&node.path) {
                    node.state = FileState::Deselected;
                } else {
                    if !profile.deselected_paths.contains(&node.path)
                        && !profile.selected_paths.contains(&node.path)
                    {
                        node.state = FileState::Unknown;
                    }
                }
                if node.is_dir {
                    if !node.children.is_empty() {
                        self.apply_profile_to_tree(&mut node.children, profile);
                    }
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
        crate::initialize_logging();
        let session_data = AppSessionData::new();
        assert!(session_data.current_profile_name.is_none());
        assert!(session_data.current_profile_cache.is_none());
        assert!(session_data.file_nodes_cache.is_empty());
        assert_eq!(session_data.root_path_for_scan, PathBuf::from("."));
        assert_eq!(session_data.cached_current_token_count, 0);
    }

    #[test]
    fn test_create_profile_from_session_state_basic() {
        crate::initialize_logging();

        let temp_dir = tempdir().unwrap();
        let file1_content_written = "content one";
        let (file1_path, _g1) =
            create_temp_file_with_content(&temp_dir, "f1", file1_content_written);
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", "content two");

        let mut session_data = AppSessionData {
            current_profile_name: Some("OldProfile".to_string()),
            current_profile_cache: Some(Profile {
                name: "OldProfile".to_string(),
                root_folder: temp_dir.path().join("old_root"),
                selected_paths: HashSet::new(),
                deselected_paths: HashSet::new(),
                archive_path: Some(temp_dir.path().join("old_archive.zip")),
                file_details: HashMap::new(),
            }),
            file_nodes_cache: vec![
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
            root_path_for_scan: temp_dir.path().join("new_root"),
            cached_current_token_count: 0,
        };

        let mut specific_token_counter = MockTokenCounter::new(0);
        let file1_content_as_read = format!("{}\n", file1_content_written);
        specific_token_counter.set_count_for_content(&file1_content_as_read, 10);

        let new_profile = session_data
            .create_profile_from_session_state("NewProfile".to_string(), &specific_token_counter);

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
        crate::initialize_logging();
        let session_data = AppSessionData {
            current_profile_name: None,
            current_profile_cache: None,
            file_nodes_cache: vec![],
            root_path_for_scan: PathBuf::from("/root"),
            cached_current_token_count: 0,
        };
        let mock_token_counter = MockTokenCounter::new(0);
        let new_profile = session_data
            .create_profile_from_session_state("ProfileNoArchive".to_string(), &mock_token_counter);
        assert_eq!(new_profile.archive_path, None);
        assert!(new_profile.file_details.is_empty());
    }

    #[test]
    fn test_create_profile_from_session_state_file_read_error() {
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let non_existent_path = temp_dir.path().join("non_existent.txt");

        let mock_token_counter = MockTokenCounter::new(0);
        let session_data = AppSessionData {
            current_profile_name: None,
            current_profile_cache: None,
            file_nodes_cache: vec![FileNode {
                path: non_existent_path.clone(),
                name: "non_existent.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: Some("cs_non_existent".to_string()),
            }],
            root_path_for_scan: temp_dir.path().to_path_buf(),
            cached_current_token_count: 0,
        };

        let new_profile = session_data.create_profile_from_session_state(
            "ProfileWithErrorFile".to_string(),
            &mock_token_counter,
        );
        assert!(
            new_profile.file_details.is_empty(),
            "File details should be empty if file read failed."
        );
        assert!(new_profile.selected_paths.contains(&non_existent_path));
    }

    #[test]
    fn test_create_profile_from_session_state_no_checksum() {
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let (file_no_cs_path, _g_no_cs) =
            create_temp_file_with_content(&temp_dir, "f_no_cs", "content no cs");

        let mock_token_counter = MockTokenCounter::new(0);
        let session_data = AppSessionData {
            current_profile_name: None,
            current_profile_cache: None,
            file_nodes_cache: vec![FileNode {
                path: file_no_cs_path.clone(),
                name: "f_no_cs.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: None,
            }],
            root_path_for_scan: temp_dir.path().to_path_buf(),
            cached_current_token_count: 0,
        };

        let new_profile = session_data.create_profile_from_session_state(
            "ProfileWithNoCSFile".to_string(),
            &mock_token_counter,
        );
        assert!(
            new_profile.file_details.is_empty(),
            "File details should be empty if file has no checksum."
        );
        assert!(new_profile.selected_paths.contains(&file_no_cs_path));
    }

    #[test]
    fn test_update_token_count_no_files_uses_cache_logic() {
        crate::initialize_logging();
        let mut session_data = AppSessionData::new();
        let mock_token_counter = MockTokenCounter::new(0);
        let count = session_data.update_token_count(&mock_token_counter);
        assert_eq!(count, 0);
        assert_eq!(session_data.cached_current_token_count, 0);
        assert!(
            mock_token_counter.get_call_log().is_empty(),
            "Token counter should not be called for no files"
        );
    }

    #[test]
    fn test_update_token_count_selected_files_cache_hit() {
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let content1 = "hello world";
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1", content1);
        let cs1 = "cs1_match".to_string();

        let content2 = "another example";
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", content2);
        let cs2 = "cs2_match".to_string();

        let mut file_details_cache = HashMap::new();
        file_details_cache.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: cs1.clone(),
                token_count: 10,
            },
        );
        file_details_cache.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: cs2.clone(),
                token_count: 20,
            },
        );

        let mut session_data = AppSessionData {
            current_profile_name: Some("TestProfile".to_string()),
            current_profile_cache: Some(Profile {
                name: "TestProfile".to_string(),
                root_folder: temp_dir.path().to_path_buf(),
                selected_paths: HashSet::new(), // Not directly used by update_token_count, which relies on FileNode.state
                deselected_paths: HashSet::new(),
                archive_path: None,
                file_details: file_details_cache,
            }),
            file_nodes_cache: vec![
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
            root_path_for_scan: temp_dir.path().to_path_buf(),
            cached_current_token_count: 0,
        };

        let mock_token_counter = MockTokenCounter::new(0); // Default, should not be used
        let count = session_data.update_token_count(&mock_token_counter);

        assert_eq!(count, 30, "Expected 10 (f1) + 20 (f2) from cache");
        assert_eq!(session_data.cached_current_token_count, 30);
        assert!(
            mock_token_counter.get_call_log().is_empty(),
            "Token counter should not be called on cache hits"
        );
    }

    #[test]
    fn test_update_token_count_cache_miss_and_stale() {
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();

        // File 1: Cache miss (not in file_details)
        let content1 = "new file content";
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1_new", content1);
        let cs1_disk = "cs1_new_disk".to_string();

        // File 2: Cache stale (checksum mismatch)
        let content2 = "changed file content";
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2_changed", content2);
        let cs2_disk_new = "cs2_changed_disk_new".to_string();
        let cs2_cache_old = "cs2_stale_cache_old".to_string();

        // File 3: Cache hit
        let content3 = "unchanged file content";
        let (file3_path, _g3) = create_temp_file_with_content(&temp_dir, "f3_unchanged", content3);
        let cs3_match = "cs3_unchanged_match".to_string();

        // File 4: Selected, but FileNode has no checksum (e.g. scan error for this file)
        let content4 = "file with no disk checksum";
        let (file4_path, _g4) = create_temp_file_with_content(&temp_dir, "f4_no_cs", content4);

        let mut file_details_initial_cache = HashMap::new();
        // File 1 is missing from initial cache
        file_details_initial_cache.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: cs2_cache_old.clone(),
                token_count: 15,
            },
        ); // Stale entry
        file_details_initial_cache.insert(
            file3_path.clone(),
            FileTokenDetails {
                checksum: cs3_match.clone(),
                token_count: 30,
            },
        ); // Valid entry
        file_details_initial_cache.insert(
            file4_path.clone(),
            FileTokenDetails {
                checksum: "cs4_irrelevant".to_string(),
                token_count: 40,
            },
        ); // Will fallback

        let mut session_data = AppSessionData {
            current_profile_name: Some("TestProfileMixedCache".to_string()),
            current_profile_cache: Some(Profile {
                name: "TestProfileMixedCache".to_string(),
                root_folder: temp_dir.path().to_path_buf(),
                selected_paths: HashSet::new(),
                deselected_paths: HashSet::new(),
                archive_path: None,
                file_details: file_details_initial_cache,
            }),
            file_nodes_cache: vec![
                FileNode {
                    path: file1_path.clone(),
                    name: "f1_new.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs1_disk.clone()),
                },
                FileNode {
                    path: file2_path.clone(),
                    name: "f2_changed.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs2_disk_new.clone()),
                },
                FileNode {
                    path: file3_path.clone(),
                    name: "f3_unchanged.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs3_match.clone()),
                },
                FileNode {
                    path: file4_path.clone(),
                    name: "f4_no_cs.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: None,
                }, // No checksum on FileNode
            ],
            root_path_for_scan: temp_dir.path().to_path_buf(),
            cached_current_token_count: 0,
        };

        let mut mock_token_counter = MockTokenCounter::new(0); // Default, used if specific not set
        mock_token_counter.set_count_for_content(&format!("{}\n", content1), 10); // For file1 fallback
        mock_token_counter.set_count_for_content(&format!("{}\n", content2), 20); // For file2 fallback
        mock_token_counter.set_count_for_content(&format!("{}\n", content4), 5); // For file4 fallback

        let count = session_data.update_token_count(&mock_token_counter);

        // Expected: file1 (10, fallback), file2 (20, fallback), file3 (30, cache hit), file4 (5, fallback)
        // Total = 10 + 20 + 30 + 5 = 65
        assert_eq!(count, 65, "Token count mismatch");
        assert_eq!(session_data.cached_current_token_count, 65);

        let calls = mock_token_counter.get_call_log();
        assert_eq!(
            calls.len(),
            3,
            "Token counter should be called 3 times (for f1, f2, f4)"
        );
        assert!(calls.contains(&format!("{}\n", content1)));
        assert!(calls.contains(&format!("{}\n", content2)));
        assert!(calls.contains(&format!("{}\n", content4)));
        assert!(
            !calls.contains(&format!("{}\n", content3)),
            "Content3 should be a cache hit"
        );
    }

    #[test]
    fn test_update_token_count_handles_read_error_during_fallback() {
        let temp_dir = tempdir().unwrap();
        let (readable_path, _g_readable) =
            create_temp_file_with_content(&temp_dir, "readable", "one two");
        let cs_readable = "cs_readable_match".to_string();
        let non_existent_path = temp_dir.path().join("non_existent_for_fallback.txt"); // Will cause read error in fallback
        let cs_non_existent = "cs_non_existent_stale".to_string(); // Make it seem stale

        let mut file_details_cache = HashMap::new();
        // Readable file is in cache and matches
        file_details_cache.insert(
            readable_path.clone(),
            FileTokenDetails {
                checksum: cs_readable.clone(),
                token_count: 2,
            },
        );
        // Non-existent file also in cache, but will mismatch checksum forcing fallback
        file_details_cache.insert(
            non_existent_path.clone(),
            FileTokenDetails {
                checksum: "cs_old".to_string(),
                token_count: 99,
            },
        );

        let mut session_data = AppSessionData {
            current_profile_cache: Some(Profile {
                name: "TestProfile".to_string(),
                root_folder: temp_dir.path().to_path_buf(),
                selected_paths: HashSet::new(),
                deselected_paths: HashSet::new(),
                archive_path: None,
                file_details: file_details_cache,
            }),
            file_nodes_cache: vec![
                FileNode {
                    path: readable_path.clone(),
                    name: "readable.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs_readable.clone()),
                },
                FileNode {
                    path: non_existent_path.clone(),
                    name: "non_existent.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs_non_existent.clone()),
                }, // Has checksum, will mismatch cache
            ],
            ..AppSessionData::new() // Fill other fields with defaults
        };
        session_data.root_path_for_scan = temp_dir.path().to_path_buf();

        let mock_token_counter = MockTokenCounter::new(0); // Fallback for non-existent should yield 0

        let count = session_data.update_token_count(&mock_token_counter);
        assert_eq!(
            count, 2,
            "Only readable file (from cache) should contribute. Fallback for non-existent failed, contributing 0."
        );
        assert_eq!(session_data.cached_current_token_count, 2);

        // MockTokenCounter should have been called once for non_existent_path during fallback attempt
        let calls = mock_token_counter.get_call_log();
        assert_eq!(
            calls.len(),
            0,
            "Token counter should not be called successfully for a non-existent file during fallback. The read fails first."
        );
        // The log for "TokenCount (FALLBACK FAIL)" should appear for non_existent_path.
    }

    #[test]
    fn test_activate_and_populate_data_success_and_updates_file_details() {
        crate::initialize_logging();
        let mut session_data = AppSessionData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mut mock_token_counter = MockTokenCounter::new(0);

        let profile_name = "TestProfileDetailsUpdate";
        let temp_dir = tempdir().unwrap();
        let root_folder = temp_dir.path().to_path_buf();

        let content1 = "file one content";
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1", content1);
        let file1_checksum_disk = "cs1_disk".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content1), 10);

        let content2 = "file two changed content";
        let (file2_path, _g2) = create_temp_file_with_content(&temp_dir, "f2", content2);
        let file2_checksum_disk = "cs2_disk_new".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content2), 20);

        let content3 = "file three new selected";
        let (file3_path, _g3) = create_temp_file_with_content(&temp_dir, "f3", content3);
        let file3_checksum_disk = "cs3_disk".to_string();
        mock_token_counter.set_count_for_content(&format!("{}\n", content3), 30);

        let content4 = "file four was selected now not";
        let (file4_path, _g4) = create_temp_file_with_content(&temp_dir, "f4", content4);
        let file4_checksum_disk = "cs4_disk".to_string();

        let content5 = "file five selected no checksum";
        let (file5_path, _g5) = create_temp_file_with_content(&temp_dir, "f5", content5);
        mock_token_counter.set_count_for_content(&format!("{}\n", content5), 50); // For fallback in update_token_count

        let mut initial_file_details = HashMap::new();
        initial_file_details.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: file1_checksum_disk.clone(),
                token_count: 10,
            },
        );
        initial_file_details.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: "cs2_disk_old".to_string(),
                token_count: 15,
            },
        );
        initial_file_details.insert(
            file4_path.clone(),
            FileTokenDetails {
                checksum: file4_checksum_disk.clone(),
                token_count: 40,
            },
        );
        initial_file_details.insert(
            file5_path.clone(),
            FileTokenDetails {
                checksum: "cs5_irrelevant".to_string(),
                token_count: 55,
            },
        );

        let mut profile_for_activation = Profile {
            name: profile_name.to_string(),
            root_folder: root_folder.clone(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: None,
            file_details: initial_file_details,
        };
        profile_for_activation
            .selected_paths
            .insert(file1_path.clone());
        profile_for_activation
            .selected_paths
            .insert(file2_path.clone());
        profile_for_activation
            .selected_paths
            .insert(file3_path.clone());
        profile_for_activation
            .selected_paths
            .insert(file5_path.clone());

        let nodes_for_scanner_to_return = vec![
            FileNode {
                path: file1_path.clone(),
                name: "f1.txt".into(),
                is_dir: false,
                state: FileState::Unknown,
                children: Vec::new(),
                checksum: Some(file1_checksum_disk.clone()),
            },
            FileNode {
                path: file2_path.clone(),
                name: "f2.txt".into(),
                is_dir: false,
                state: FileState::Unknown,
                children: Vec::new(),
                checksum: Some(file2_checksum_disk.clone()),
            },
            FileNode {
                path: file3_path.clone(),
                name: "f3.txt".into(),
                is_dir: false,
                state: FileState::Unknown,
                children: Vec::new(),
                checksum: Some(file3_checksum_disk.clone()),
            },
            FileNode {
                path: file4_path.clone(),
                name: "f4.txt".into(),
                is_dir: false,
                state: FileState::Unknown,
                children: Vec::new(),
                checksum: Some(file4_checksum_disk.clone()),
            },
            FileNode {
                path: file5_path.clone(),
                name: "f5.txt".into(),
                is_dir: false,
                state: FileState::Unknown,
                children: Vec::new(),
                checksum: None,
            },
        ];
        mock_scanner
            .set_scan_directory_result(&root_folder, Ok(nodes_for_scanner_to_return.clone()));

        mock_token_counter.clear_call_log(); // Clear before act
        let result = session_data.activate_and_populate_data(
            profile_for_activation.clone(),
            &mock_scanner,
            &mock_state_manager,
            &mock_token_counter,
        );

        assert!(result.is_ok());
        assert_eq!(
            session_data.current_profile_name.as_deref(),
            Some(profile_name)
        );

        let final_profile_cache = session_data.current_profile_cache.as_ref().unwrap();
        let final_details = &final_profile_cache.file_details;

        assert_eq!(final_details.get(&file1_path).unwrap().token_count, 10);
        assert_eq!(
            final_details.get(&file1_path).unwrap().checksum,
            file1_checksum_disk
        );
        assert_eq!(final_details.get(&file2_path).unwrap().token_count, 20);
        assert_eq!(
            final_details.get(&file2_path).unwrap().checksum,
            file2_checksum_disk
        );
        assert_eq!(final_details.get(&file3_path).unwrap().token_count, 30);
        assert_eq!(
            final_details.get(&file3_path).unwrap().checksum,
            file3_checksum_disk
        );
        assert!(final_details.get(&file4_path).is_none());
        assert!(final_details.get(&file5_path).is_none()); // Removed because FileNode had no checksum

        // Verify total token count (sum of selected files with valid token counts from cache or fallback)
        // File1 (10 from cache update) + File2 (20 from cache update) + File3 (30 from cache update)
        // File5 (selected, no checksum in FileNode, so update_cached_file_details_recursive removed it from profile cache.
        // Then update_token_count tries to count it, reads content "file five selected no checksum\n", gets 50 tokens from mock_token_counter)
        // Total = 10 + 20 + 30 + 50 = 110.
        assert_eq!(
            session_data.cached_current_token_count, 110,
            "Total token count mismatch"
        );

        let calls = mock_token_counter.get_call_log();
        // update_cached_file_details_recursive calls:
        // - file2 (stale checksum)
        // - file3 (new to cache)
        // update_token_count calls:
        // - file1 (cache hit in profile_cache)
        // - file2 (cache hit in profile_cache - after update)
        // - file3 (cache hit in profile_cache - after update)
        // - file5 (fallback because not in profile_cache)
        // So, expected calls to token_counter: content2, content3, content5.
        // Content1 should not be re-tokenized by update_token_count if cache is used.
        assert!(
            calls.contains(&format!("{}\n", content2)),
            "Content2 should be tokenized for cache update"
        ); // For update_cached_file_details
        assert!(
            calls.contains(&format!("{}\n", content3)),
            "Content3 should be tokenized for cache update"
        ); // For update_cached_file_details
        assert!(
            calls.contains(&format!("{}\n", content5)),
            "Content5 should be tokenized by update_token_count fallback"
        ); // For update_token_count fallback
        assert!(
            !calls.contains(&format!("{}\n", content1)),
            "Content1 should be a cache hit and not re-tokenized by update_cached_file_details, and also a cache hit for update_token_count"
        );
        assert_eq!(
            calls.len(),
            3,
            "Expected 3 calls to token_counter: content2 (cache update), content3 (cache update), content5 (final count fallback)"
        );
    }

    #[test]
    fn test_activate_and_populate_data_scan_error() {
        let mut session_data = AppSessionData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mock_token_counter = MockTokenCounter::new(0);

        let profile_name = "ErrorProfile";
        let root_folder = PathBuf::from("/error/root");
        let profile = Profile::new(profile_name.to_string(), root_folder.clone());

        mock_scanner.set_scan_directory_result(
            &root_folder,
            Err(FileSystemError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "scan failed",
            ))),
        );

        let result = session_data.activate_and_populate_data(
            profile.clone(),
            &mock_scanner,
            &mock_state_manager,
            &mock_token_counter,
        );

        assert!(result.is_err());
        if let Err(msg) = result {
            assert!(msg.contains("Failed to scan directory"));
            assert!(msg.contains(profile_name));
        }
        assert_eq!(
            session_data.current_profile_name.as_deref(),
            Some(profile_name)
        );
        assert!(
            session_data.file_nodes_cache.is_empty(),
            "Cache should be cleared on scan error"
        );
        assert_eq!(
            session_data.cached_current_token_count, 0,
            "Token count should be 0 on error"
        );

        assert_eq!(mock_scanner.get_scan_directory_calls().len(), 1);
        assert_eq!(
            mock_state_manager.get_apply_profile_to_tree_calls().len(),
            0,
            "Apply profile should not be called if scan fails"
        );
    }
}
