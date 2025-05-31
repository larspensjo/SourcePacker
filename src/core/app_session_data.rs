/*
 * This module defines the AppSessionData struct.
 * AppSessionData is responsible for holding and managing the core
 * application's session-specific data, such as the current profile,
 * file cache, scan settings, and token counts. It aims to separate this
 * data from both the UI-specific state and the main application logic handler,
 * acting as a primary model component for session state.
 */
use crate::core::models::FileTokenDetails;
use crate::core::{
    FileNode, FileState, FileSystemScannerOperations, Profile, StateManagerOperations,
    TokenCounterOperations,
};
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
                            log::debug!(
                                "AppSessionData: Cached token count {} for selected file {:?} with checksum {}",
                                token_count,
                                node.path,
                                checksum_val
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "AppSessionData: Failed to read file {:?} for token caching during profile save: {}",
                                node.path,
                                e
                            );
                        }
                    }
                } else {
                    log::warn!(
                        "AppSessionData: Selected file {:?} has no checksum; cannot cache token count for profile save.",
                        node.path
                    );
                }
            }
        }
    }

    /*
     * Recalculates the estimated token count for all currently selected files.
     * Data is read from `file_nodes_cache` and result cached
     * in `current_token_count`.
     */
    pub(crate) fn update_token_count(
        &mut self,
        token_counter: &dyn TokenCounterOperations,
    ) -> usize {
        log::debug!("Recalculating token count for selected files.");
        let mut total_tokens: usize = 0;
        let mut files_processed_for_tokens: usize = 0;
        let mut files_failed_to_read_for_tokens: usize = 0;

        // Helper function to recursively traverse the file node tree
        fn count_tokens_recursive_inner(
            nodes: &[FileNode], // Operates on FileNode slice
            current_total_tokens: &mut usize,
            files_processed: &mut usize,
            files_failed: &mut usize,
            token_counter_ref: &dyn TokenCounterOperations,
        ) {
            for node in nodes {
                if !node.is_dir && node.state == FileState::Selected {
                    *files_processed += 1;
                    log::debug!(
                        "TokenCount: Processing file {:?} for token counting.",
                        node.path
                    );
                    match fs::read_to_string(&node.path) {
                        Ok(content) => {
                            let tokens_in_file = token_counter_ref.count_tokens(&content);
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
                        token_counter_ref,
                    );
                }
            }
        }

        count_tokens_recursive_inner(
            &self.file_nodes_cache, // Use app_session_data
            &mut total_tokens,
            &mut files_processed_for_tokens,
            &mut files_failed_to_read_for_tokens,
            token_counter,
        );

        self.cached_current_token_count = total_tokens; // Store in app_session_data
        log::debug!(
            "Token count updated internally: {} tokens from {} selected files ({} files failed to read).",
            self.cached_current_token_count,
            files_processed_for_tokens,
            files_failed_to_read_for_tokens
        );
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
        token_counter: &dyn TokenCounterOperations,
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
                self.update_token_count(token_counter); // Update token count after successful scan and state application
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
                self.cached_current_token_count = 0; // Also reset token count
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

        #[allow(dead_code)] // Potentially useful for more detailed assertions
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
                    // Basic cloning for test purposes
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
                None => Ok(Vec::new()), // Default to empty if no specific result is set
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

        #[allow(dead_code)] // Potentially useful for more detailed assertions
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
            // Minimal simulation of apply_profile_to_tree for testing post-conditions
            for node in tree.iter_mut() {
                if profile.selected_paths.contains(&node.path) {
                    node.state = FileState::Selected;
                } else if profile.deselected_paths.contains(&node.path) {
                    node.state = FileState::Deselected;
                } else {
                    node.state = FileState::Unknown;
                }
            }
        }

        fn update_folder_selection(&self, _node: &mut FileNode, _new_state: FileState) {
            // Not directly used by AppSessionData::activate_and_populate_data, but part of trait.
        }
    }

    // --- Mock TokenCounter ---
    struct MockTokenCounter {
        default_count: usize,
        counts_for_content: HashMap<String, usize>, // For more specific mocking if needed
    }
    impl MockTokenCounter {
        fn new(default_count: usize) -> Self {
            Self {
                default_count,
                counts_for_content: HashMap::new(),
            }
        }
        #[allow(dead_code)]
        fn set_count_for_content(&mut self, content: &str, count: usize) {
            log::debug!(
                "MockTokenCounter: Setting count {} for content '{}'",
                count,
                content
            );
            self.counts_for_content.insert(content.to_string(), count);
        }
    }
    impl TokenCounterOperations for MockTokenCounter {
        fn count_tokens(&self, text: &str) -> usize {
            log::debug!("MockTokenCounter: Counting tokens for text '{}'", text);
            if let Some(count) = self.counts_for_content.get(text) {
                *count
            } else {
                self.default_count
            }
        }
    }
    // Helper to create temporary files for token counting tests
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
        let session_data = AppSessionData::new();
        assert!(session_data.current_profile_name.is_none());
        assert!(session_data.current_profile_cache.is_none());
        assert!(session_data.file_nodes_cache.is_empty());
        assert_eq!(session_data.root_path_for_scan, PathBuf::from("."));
        assert_eq!(session_data.cached_current_token_count, 0);
    }

    #[test]
    fn test_create_profile_from_session_state_basic() {
        let temp_dir = tempdir().unwrap();
        let (file1_path, _g1) = create_temp_file_with_content(&temp_dir, "f1", "content one");
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
                    state: FileState::Deselected, // This one won't be in file_details
                    children: Vec::new(),
                    checksum: Some("cs2".to_string()),
                },
            ],
            root_path_for_scan: temp_dir.path().join("new_root"),
            cached_current_token_count: 0,
        };
        // Setup mock token counter for specific content
        let mut specific_token_counter = MockTokenCounter::new(0);
        specific_token_counter.set_count_for_content("content one", 10);

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
        assert_eq!(detail1.token_count, 10); // As per mock
    }

    #[test]
    fn test_create_profile_from_session_state_no_archive_path() {
        let session_data = AppSessionData {
            current_profile_name: None,
            current_profile_cache: None, // No old profile, so no archive path to inherit
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
        let temp_dir = tempdir().unwrap();
        let non_existent_path = temp_dir.path().join("non_existent.txt"); // Will cause read error

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
        assert!(new_profile.selected_paths.contains(&non_existent_path)); // Still marked as selected path
    }

    #[test]
    fn test_create_profile_from_session_state_no_checksum() {
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
                checksum: None, // No checksum
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
    fn test_update_token_count_no_files() {
        let mut session_data = AppSessionData::new();
        let mock_token_counter = MockTokenCounter::new(0); // Irrelevant count for no files
        let count = session_data.update_token_count(&mock_token_counter);
        assert_eq!(count, 0);
        assert_eq!(session_data.cached_current_token_count, 0);
    }

    #[test]
    fn test_update_token_count_selected_files() {
        let mut session_data = AppSessionData::new();
        let mock_token_counter = MockTokenCounter::new(5); // Each "file" has 5 tokens
        let temp_dir = tempdir().unwrap();
        let (file1_path, _guard1) = create_temp_file_with_content(&temp_dir, "f1", "hello world");
        let (file2_path, _guard2) =
            create_temp_file_with_content(&temp_dir, "f2", "another example");
        let (file3_path, _guard3) = create_temp_file_with_content(&temp_dir, "f3", "skip this one");

        session_data.file_nodes_cache = vec![
            FileNode {
                path: file1_path.clone(),
                name: "f1.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: None,
            },
            FileNode {
                path: file2_path.clone(),
                name: "f2.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: None,
            },
            FileNode {
                path: file3_path.clone(),
                name: "f3.txt".into(),
                is_dir: false,
                state: FileState::Deselected,
                children: Vec::new(),
                checksum: None,
            },
            FileNode {
                // Folder with a selected child
                path: temp_dir.path().join("folder"),
                name: "folder".into(),
                is_dir: true,
                state: FileState::Unknown, // Folder state itself doesn't matter for token sum
                children: vec![FileNode {
                    path: create_temp_file_with_content(&temp_dir, "child", "child content").0,
                    name: "child.txt".into(),
                    is_dir: false,
                    state: FileState::Selected,
                    children: Vec::new(),
                    checksum: None,
                }],
                checksum: None,
            },
        ];
        let count = session_data.update_token_count(&mock_token_counter);
        // 3 selected files (f1, f2, child.txt), each mocked to 5 tokens
        assert_eq!(count, 5 * 3, "Expected 3 files * 5 tokens each");
        assert_eq!(session_data.cached_current_token_count, 15);
    }

    #[test]
    fn test_update_token_count_handles_read_error() {
        let mut session_data = AppSessionData::new();
        let mock_token_counter = MockTokenCounter::new(2); // Readable file has 2 tokens
        let temp_dir = tempdir().unwrap();
        let (readable_path, _guard_readable) =
            create_temp_file_with_content(&temp_dir, "readable", "one two");
        let non_existent_path = temp_dir.path().join("non_existent.txt");

        session_data.file_nodes_cache = vec![
            FileNode {
                path: readable_path.clone(),
                name: "readable.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: None,
            },
            FileNode {
                path: non_existent_path.clone(),
                name: "non_existent.txt".into(),
                is_dir: false,
                state: FileState::Selected,
                children: Vec::new(),
                checksum: None,
            },
        ];
        let count = session_data.update_token_count(&mock_token_counter);
        assert_eq!(count, 2, "Only readable file should contribute");
        assert_eq!(session_data.cached_current_token_count, 2);
    }

    #[test]
    fn test_activate_and_populate_data_success() {
        let mut session_data = AppSessionData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mock_token_counter = MockTokenCounter::new(7); // Each selected file will contribute 7 tokens

        let profile_name = "TestProfile";
        let root_folder = PathBuf::from("/test/root"); // Initial conceptual root

        let temp_dir = tempdir().unwrap(); // Actual root for scanned files and token counting
        let (file_for_token_path, _g) =
            create_temp_file_with_content(&temp_dir, "tok", "token count this");

        let mut profile_for_activation =
            Profile::new(profile_name.to_string(), temp_dir.path().to_path_buf());
        profile_for_activation
            .selected_paths
            .insert(file_for_token_path.clone());

        let nodes_for_scanner_to_return = vec![FileNode {
            path: file_for_token_path.clone(),
            name: "tok.txt".into(),
            is_dir: false,
            state: FileState::Unknown, // State manager will update this
            children: Vec::new(),
            checksum: None,
        }];
        mock_scanner
            .set_scan_directory_result(temp_dir.path(), Ok(nodes_for_scanner_to_return.clone()));

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
        assert_eq!(
            session_data.current_profile_cache.as_ref().unwrap().name,
            profile_name
        );
        assert_eq!(session_data.root_path_for_scan, temp_dir.path()); // Should be the actual scan root
        assert_eq!(session_data.file_nodes_cache.len(), 1);
        assert_eq!(session_data.file_nodes_cache[0].path, file_for_token_path);
        assert_eq!(
            session_data.file_nodes_cache[0].state,
            FileState::Selected,
            "State manager should have set it to selected"
        );
        assert_eq!(session_data.cached_current_token_count, 7); // From MockTokenCounter

        assert_eq!(mock_scanner.get_scan_directory_calls().len(), 1);
        assert_eq!(mock_scanner.get_scan_directory_calls()[0], temp_dir.path());
        assert_eq!(
            mock_state_manager.get_apply_profile_to_tree_calls().len(),
            1
        );
        assert_eq!(
            mock_state_manager.get_apply_profile_to_tree_calls()[0]
                .0
                .name,
            profile_name
        );
    }

    #[test]
    fn test_activate_and_populate_data_scan_error() {
        let mut session_data = AppSessionData::new();
        let mock_scanner = MockFileSystemScanner::new();
        let mock_state_manager = MockStateManager::new();
        let mock_token_counter = MockTokenCounter::new(0); // Irrelevant as scan fails

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
        ); // Profile details are set before scan
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
