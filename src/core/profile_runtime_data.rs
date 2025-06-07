/*
 * This module defines the ProfileRuntimeData struct and the ProfileRuntimeDataOperations trait.
 * ProfileRuntimeData holds the core, mutable data for an active application session,
 * such as current profile details, scanned file nodes, and token caches.
 * The ProfileRuntimeDataOperations trait provides an abstraction for interacting with
 * this session data, facilitating dependency injection and testing.
 */
use crate::core::{
    FileNode, FileSystemScannerOperations, NodeStateApplicatorOperations, Profile, SelectionState,
    TokenCounterOperations, file_node::FileTokenDetails,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/*
 * Defines the operations for managing the runtime data of an active profile session.
 * This trait abstracts access and manipulation of session-specific information like
 * profile identifiers, file system snapshots, selection states, and token counts.
 * It is designed to be implemented by `ProfileRuntimeData` and mocked for testing.
 */
pub trait ProfileRuntimeDataOperations: Send + Sync {
    // Profile identification
    fn get_profile_name(&self) -> Option<String>;
    fn set_profile_name(&mut self, name: Option<String>);

    // Archive path
    fn get_archive_path(&self) -> Option<PathBuf>;
    fn set_archive_path(&mut self, path: Option<PathBuf>);

    // Root scan path
    fn get_root_path_for_scan(&self) -> PathBuf;

    // File system snapshot (nodes)
    fn get_snapshot_nodes(&self) -> &Vec<FileNode>;
    fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>);
    fn apply_selection_states_to_snapshot(
        &mut self,
        state_manager: &dyn NodeStateApplicatorOperations,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    );
    fn get_node_attributes_for_path(&self, path: &Path) -> Option<(SelectionState, bool)>; // (state, is_dir)
    fn update_node_state_and_collect_changes(
        &mut self,
        path: &Path,
        new_state: SelectionState,
        state_manager: &dyn NodeStateApplicatorOperations,
    ) -> Vec<(PathBuf, SelectionState)>;
    /*
     * Checks if the file or folder at the given path, or any of its descendants
     * (if it's a folder), contains any file in the 'New' state.
     */
    fn does_path_or_descendants_contain_new_file(&self, path: &Path) -> bool;

    // Token related data
    fn update_total_token_count_for_selected_files(
        &mut self,
        token_counter: &dyn TokenCounterOperations,
    ) -> usize;

    // General session management
    fn clear(&mut self);
    fn create_profile_snapshot(&self) -> Profile;
    fn load_profile_into_session(
        &mut self,
        loaded_profile: Profile,
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn NodeStateApplicatorOperations,
        token_counter: &dyn TokenCounterOperations,
    ) -> Result<(), String>; // String is error message
    fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>);
}

/*
 * Holds the core data for an active application session.
 * This includes information about the current profile being worked on (name, root folder,
 * archive path), the cache of scanned file nodes, the root path for file system scans,
 * the estimated token count for selected files, and a cache of token details for individual files.
 */
pub struct ProfileRuntimeData {
    profile_name: Option<String>,
    archive_path: Option<PathBuf>,
    file_system_snapshot_nodes: Vec<FileNode>,
    root_path_for_scan: PathBuf,
    cached_token_count: usize,
    cached_file_token_details: HashMap<PathBuf, FileTokenDetails>,
}

impl ProfileRuntimeData {
    /*
     * Creates a new `ProfileRuntimeData` instance with default values.
     * Initializes with no profile loaded, an empty file cache, a default
     * root scan path (current directory), zero tokens, and an empty token details cache.
     */
    pub fn new() -> Self {
        log::debug!("ProfileRuntimeData::new called - initializing default session data.");
        ProfileRuntimeData {
            profile_name: None,
            archive_path: None,
            file_system_snapshot_nodes: Vec::new(),
            root_path_for_scan: PathBuf::from("."), // Default to current directory
            cached_token_count: 0,
            cached_file_token_details: HashMap::new(),
        }
    }

    /*
     * Try the cache first. If not found or stale, read the file, count tokens, and update the cache.
     * Asserts that node.checksum is Some, as this function is only valid in that context.
     * Returns None if the file cannot be read.
     */
    fn get_token_count_with_cache(
        token_counter_service: &dyn TokenCounterOperations,
        node: &FileNode,
        cache: &mut HashMap<PathBuf, FileTokenDetails>,
    ) -> Option<usize> {
        // PRECONDITION: The node MUST have a checksum to use this function.
        // The checksum is essential for cache validity and identifying the file's state.
        let node_checksum = node.checksum.as_ref()
            .expect("get_token_count_with_cache called on a FileNode without a checksum. Contract violation.");

        if let Some(details) = cache.get(&node.path) {
            if *node_checksum == details.checksum {
                return Some(details.token_count);
            }
        }

        let content = match fs::read_to_string(&node.path) {
            Ok(c) => c,
            Err(_e) => {
                // Remove any stale checksum from the cache
                cache.remove(&node.path);
                return None;
            }
        };

        let tokens_in_file = token_counter_service.count_tokens(&content);

        cache.insert(
            node.path.clone(),
            FileTokenDetails {
                checksum: node_checksum.clone(),
                token_count: tokens_in_file,
            },
        );

        Some(tokens_in_file)
    }

    // Helper: Recursively finds a reference to a FileNode within a slice of nodes.
    fn find_node_recursive_ref<'a>(
        nodes: &'a [FileNode],
        path_to_find: &Path,
    ) -> Option<&'a FileNode> {
        for node in nodes.iter() {
            if node.path == path_to_find {
                return Some(node);
            }
            if node.is_dir && !node.children.is_empty() {
                if let Some(found_in_child) =
                    Self::find_node_recursive_ref(&node.children, path_to_find)
                {
                    return Some(found_in_child);
                }
            }
        }
        None
    }

    // Helper: Recursively finds a mutable reference to a FileNode within a slice of nodes.
    fn find_node_recursive_mut<'a>(
        nodes: &'a mut [FileNode],
        path_to_find: &Path,
    ) -> Option<&'a mut FileNode> {
        for node in nodes.iter_mut() {
            if node.path == path_to_find {
                return Some(node);
            }
            if node.is_dir && !node.children.is_empty() {
                if let Some(found_in_child) =
                    Self::find_node_recursive_mut(&mut node.children, path_to_find)
                {
                    return Some(found_in_child);
                }
            }
        }
        None
    }

    // Helper: Gathers selected and deselected paths from a node tree.
    fn gather_selected_deselected_paths_recursive_internal(
        nodes: &[FileNode],
        selected: &mut HashSet<PathBuf>,
        deselected: &mut HashSet<PathBuf>,
    ) {
        for node in nodes {
            match node.state {
                SelectionState::Selected => {
                    selected.insert(node.path.clone());
                }
                SelectionState::Deselected => {
                    deselected.insert(node.path.clone());
                }
                SelectionState::New => {}
            }
            if node.is_dir && !node.children.is_empty() {
                Self::gather_selected_deselected_paths_recursive_internal(
                    &node.children,
                    selected,
                    deselected,
                );
            }
        }
    }

    // Helper: Collects (PathBuf, FileState) for a node and its children.
    fn collect_node_states_recursive(
        node: &FileNode,
        updates: &mut Vec<(PathBuf, SelectionState)>,
    ) {
        updates.push((node.path.clone(), node.state));
        if node.is_dir {
            for child in &node.children {
                Self::collect_node_states_recursive(child, updates);
            }
        }
    }

    /*
     * Recursively checks if the given node or any of its descendants is a file
     * in the 'New' state.
     */
    fn does_node_contain_new_file_recursive(node: &FileNode) -> bool {
        if !node.is_dir {
            // It's a file
            return node.state == SelectionState::New;
        }

        // It's a directory, check its children
        for child in &node.children {
            if Self::does_node_contain_new_file_recursive(child) {
                return true; // Found a new file in a descendant
            }
        }
        false // No new file found in this directory or its descendants
    }

    #[cfg(test)]
    fn get_cached_file_token_details(&self) -> HashMap<PathBuf, FileTokenDetails> {
        self.cached_file_token_details.clone()
    }

    #[cfg(test)]
    fn get_cached_total_token_count(&self) -> usize {
        self.cached_token_count
    }
}

impl ProfileRuntimeDataOperations for ProfileRuntimeData {
    fn get_profile_name(&self) -> Option<String> {
        self.profile_name.clone()
    }

    fn set_profile_name(&mut self, name: Option<String>) {
        self.profile_name = name;
    }

    fn get_archive_path(&self) -> Option<PathBuf> {
        self.archive_path.clone()
    }

    fn set_archive_path(&mut self, path: Option<PathBuf>) {
        self.archive_path = path;
    }

    fn get_root_path_for_scan(&self) -> PathBuf {
        self.root_path_for_scan.clone()
    }

    fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
        &self.file_system_snapshot_nodes
    }

    fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>) {
        self.file_system_snapshot_nodes = nodes;
    }

    fn apply_selection_states_to_snapshot(
        &mut self,
        state_manager: &dyn NodeStateApplicatorOperations,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    ) {
        state_manager.apply_selection_states_to_nodes(
            &mut self.file_system_snapshot_nodes,
            selected_paths,
            deselected_paths,
        );
    }

    fn get_node_attributes_for_path(&self, path: &Path) -> Option<(SelectionState, bool)> {
        Self::find_node_recursive_ref(&self.file_system_snapshot_nodes, path)
            .map(|node| (node.state, node.is_dir))
    }

    fn update_node_state_and_collect_changes(
        &mut self,
        path: &Path,
        new_state: SelectionState,
        state_manager: &dyn NodeStateApplicatorOperations,
    ) -> Vec<(PathBuf, SelectionState)> {
        let mut collected_changes = Vec::new();
        if let Some(node_to_update) =
            Self::find_node_recursive_mut(&mut self.file_system_snapshot_nodes, path)
        {
            state_manager.update_folder_selection(node_to_update, new_state);
            // After updating, collect states from this node downwards
            Self::collect_node_states_recursive(node_to_update, &mut collected_changes);
        } else {
            log::error!(
                "ProfileRuntimeData: Node not found for path {:?} to update state and collect changes.",
                path
            );
        }
        collected_changes
    }

    /*
     * Checks if the file or folder at the given path, or any of its descendants
     * (if it's a folder), contains any file in the 'New' state.
     * This is used to determine if a folder node in the UI should display the "New" indicator.
     */
    fn does_path_or_descendants_contain_new_file(&self, path: &Path) -> bool {
        log::trace!(
            "ProfileRuntimeData: Checking if path or descendants contain new file for: {:?}",
            path
        );
        match Self::find_node_recursive_ref(&self.file_system_snapshot_nodes, path) {
            Some(node) => Self::does_node_contain_new_file_recursive(node),
            None => {
                log::warn!(
                    "ProfileRuntimeData: Path {:?} not found in snapshot for new file check.",
                    path
                );
                false
            }
        }
    }

    /*
     * Use `get_token_count` for each selected file, which handles cache lookups,
     * file reads on miss/stale, and cache updates. The result is stored internally and returned.
     */
    fn update_total_token_count_for_selected_files(
        &mut self,
        token_counter: &dyn TokenCounterOperations,
    ) -> usize {
        log::debug!("ProfileRuntimeData: Starting update_total_token_count_for_selected_files.");

        let mut total_tokens: usize = 0;
        let mut files_considered_for_total: usize = 0;
        let mut files_failed_to_get_count: usize = 0;

        // Recursive helper function.
        fn sum_tokens_recursive(
            nodes_to_scan: &[FileNode],
            token_counter_service: &dyn TokenCounterOperations,
            cache: &mut HashMap<PathBuf, FileTokenDetails>,
            current_total_tokens: &mut usize,
            processed_count: &mut usize,
            failed_count: &mut usize,
        ) {
            for node in nodes_to_scan {
                if node.is_dir {
                    sum_tokens_recursive(
                        &node.children,
                        token_counter_service,
                        cache,
                        current_total_tokens,
                        processed_count,
                        failed_count,
                    );
                } else if node.state == SelectionState::Selected {
                    *processed_count += 1;
                    if node.checksum.is_some() {
                        if let Some(count) = ProfileRuntimeData::get_token_count_with_cache(
                            token_counter_service,
                            node,
                            cache,
                        ) {
                            *current_total_tokens += count;
                        } else {
                            *failed_count += 1;
                            log::warn!(
                                "ProfileRuntimeData (sum_tokens_recursive): Failed to get token count for selected file {:?}",
                                node.path
                            );
                        }
                    } else {
                        *failed_count += 1;
                        log::error!(
                            "ProfileRuntimeData (sum_tokens_recursive): Selected file {:?} has no checksum. Cannot count tokens.",
                            node.path
                        );
                    }
                }
            }
        }

        sum_tokens_recursive(
            &self.file_system_snapshot_nodes,
            token_counter,
            &mut self.cached_file_token_details, // Pass mutable cache
            &mut total_tokens,
            &mut files_considered_for_total,
            &mut files_failed_to_get_count,
        );

        self.cached_token_count = total_tokens;
        log::debug!(
            "ProfileRuntimeData: update_total_token_count_for_selected_files complete. Total: {}. Files processed for sum: {} ({} failed to get count).",
            self.cached_token_count,
            files_considered_for_total,
            files_failed_to_get_count
        );
        self.cached_token_count
    }

    fn clear(&mut self) {
        log::debug!("Clearing ProfileRuntimeData state.");
        self.profile_name = None;
        self.archive_path = None;
        self.file_system_snapshot_nodes.clear();
        self.root_path_for_scan = PathBuf::from("."); // Reset to default current directory
        self.cached_token_count = 0;
        self.cached_file_token_details.clear();
    }

    /*
     * Creates a `Profile` instance (a snapshot) from the current session state.
     * This is used when saving the current working state as a named profile.
     * It iterates through selected files in the current snapshot and uses
     * `self.cached_file_token_details` to populate the profile's `file_details`.
     * The `_token_counter` argument is not used if relying solely on the cache.
     */
    fn create_profile_snapshot(&self) -> Profile {
        log::debug!(
            "ProfileRuntimeData: Creating profile snapshot '{:?}' using cached details for selected files.",
            self.profile_name
        );
        let mut selected_paths_for_profile = HashSet::new();
        let mut deselected_paths_for_profile = HashSet::new();
        let mut file_details_for_save = HashMap::new(); // This will be populated

        // Recursive helper to gather selection states and populate file_details_for_save
        // from self.cached_file_token_details for selected files.
        fn gather_states_and_cached_details_recursive(
            nodes: &[FileNode],
            cached_details: &HashMap<PathBuf, FileTokenDetails>, // Read-only access to the current cache
            selected_paths_out: &mut HashSet<PathBuf>,
            deselected_paths_out: &mut HashSet<PathBuf>,
            file_details_out: &mut HashMap<PathBuf, FileTokenDetails>, // Populate this
        ) {
            for node in nodes {
                if node.is_dir {
                    // If the directory itself is marked selected/deselected, record its path.
                    // Children's states will be handled individually.
                    match node.state {
                        SelectionState::Selected => {
                            selected_paths_out.insert(node.path.clone());
                        }
                        SelectionState::Deselected => {
                            deselected_paths_out.insert(node.path.clone());
                        }
                        SelectionState::New => {}
                    }
                    if !node.children.is_empty() {
                        gather_states_and_cached_details_recursive(
                            &node.children,
                            cached_details,
                            selected_paths_out,
                            deselected_paths_out,
                            file_details_out,
                        );
                    }
                } else {
                    // It's a file
                    match node.state {
                        SelectionState::Selected => {
                            selected_paths_out.insert(node.path.clone());
                            // Only add details for selected files to file_details_out
                            if let Some(detail) = cached_details.get(&node.path) {
                                // We trust the cache. The checksum in `detail` is what we save.
                                // `node.checksum` is the latest from disk, but we're saving the cached state.
                                file_details_out.insert(node.path.clone(), detail.clone());
                                log::trace!(
                                    "Snapshot: Using cached detail for selected file {:?}: (cs: {}, count: {})",
                                    node.path,
                                    detail.checksum,
                                    detail.token_count
                                );
                            } else {
                                // If a selected file isn't in the cache, its details won't be saved.
                                // This implies it might have been recently selected and not yet processed by
                                // update_total_token_count, or get_token_count_with_cache failed for it.
                                log::warn!(
                                    "Snapshot: Selected file {:?} not found in cache. Its details will not be saved in profile.",
                                    node.path
                                );
                            }
                        }
                        SelectionState::Deselected => {
                            deselected_paths_out.insert(node.path.clone());
                        }
                        SelectionState::New => {}
                    }
                }
            }
        }

        gather_states_and_cached_details_recursive(
            &self.file_system_snapshot_nodes,
            &self.cached_file_token_details, // Provide read-only access to the current cache
            &mut selected_paths_for_profile,
            &mut deselected_paths_for_profile,
            &mut file_details_for_save,
        );

        Profile {
            name: self.profile_name.clone().unwrap_or_else(String::new),
            root_folder: self.root_path_for_scan.clone(),
            selected_paths: selected_paths_for_profile,
            deselected_paths: deselected_paths_for_profile,
            archive_path: self.archive_path.clone(),
            file_details: file_details_for_save, // Use the selectively populated map
        }
    }

    /*
     * Activates the given profile: copies its data into `ProfileRuntimeData`,
     * loads its associated file system data, applies the profile's selection
     * state, updates token caches, and the total token count.
     * Returns `Ok(())` on success, or an `Err(String)` with an error message on failure.
     * TODO: The call to scan_directory() should be moved upwards.
     */
    fn load_profile_into_session(
        &mut self,
        loaded_profile: Profile,
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn NodeStateApplicatorOperations,
        token_counter: &dyn TokenCounterOperations,
    ) -> Result<(), String> {
        log::debug!(
            "ProfileRuntimeData: Loading profile '{}' into session.",
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
                    "ProfileRuntimeData: Scanned {} top-level nodes for profile '{:?}'.",
                    self.file_system_snapshot_nodes.len(),
                    self.profile_name
                );

                state_manager.apply_selection_states_to_nodes(
                    &mut self.file_system_snapshot_nodes,
                    &loaded_profile.selected_paths,
                    &loaded_profile.deselected_paths,
                );
                log::debug!(
                    "ProfileRuntimeData: Applied profile selection states from '{:?}' to the scanned tree.",
                    self.profile_name
                );

                self.update_total_token_count_for_selected_files(token_counter);
                Ok(())
            }
            Err(e) => {
                let error_message = format!(
                    "Failed to scan directory {:?} for profile '{:?}': {:?}",
                    self.root_path_for_scan, self.profile_name, e
                );
                log::error!("ProfileRuntimeData: {}", error_message);
                self.clear(); // Clear all session data on scan failure
                Err(error_message)
            }
        }
    }

    fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
        let mut selected = HashSet::new();
        let mut deselected = HashSet::new();
        // Use your existing internal helper
        Self::gather_selected_deselected_paths_recursive_internal(
            &self.file_system_snapshot_nodes,
            &mut selected,
            &mut deselected,
        );
        (selected, deselected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::checksum_utils;
    use crate::core::{
        FileNode, FileSystemError, FileSystemScannerOperations, NodeStateApplicatorOperations,
        Profile, SelectionState, TokenCounterOperations,
    };
    use std::collections::{HashMap, HashSet};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::{NamedTempFile, tempdir};

    /*
     * This module contains unit tests for `ProfileRuntimeData` and its implementation
     * of `ProfileRuntimeDataOperations`. It focuses on testing session state management,
     * profile snapshot creation, token counting logic, and profile activation, using
     * mocks for external dependencies.
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
        update_folder_selection_calls: Mutex<Vec<(PathBuf, SelectionState)>>,
    }

    impl MockStateManager {
        fn new() -> Self {
            MockStateManager {
                apply_profile_to_tree_calls: Mutex::new(Vec::new()),
                update_folder_selection_calls: Mutex::new(Vec::new()),
            }
        }

        fn get_apply_profile_to_tree_calls(
            &self,
        ) -> Vec<(HashSet<PathBuf>, HashSet<PathBuf>, Vec<FileNode>)> {
            self.apply_profile_to_tree_calls.lock().unwrap().clone()
        }
        fn get_update_folder_selection_calls(&self) -> Vec<(PathBuf, SelectionState)> {
            self.update_folder_selection_calls.lock().unwrap().clone()
        }
    }

    impl NodeStateApplicatorOperations for MockStateManager {
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
                    node.state = SelectionState::Selected;
                } else if deselected_paths.contains(&node.path) {
                    node.state = SelectionState::Deselected;
                } else {
                    node.state = SelectionState::New;
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
        fn update_folder_selection(&self, node: &mut FileNode, new_state: SelectionState) {
            self.update_folder_selection_calls
                .lock()
                .unwrap()
                .push((node.path.clone(), new_state));
            node.state = new_state;
            if node.is_dir {
                for child in node.children.iter_mut() {
                    self.update_folder_selection(child, new_state);
                }
            }
        }
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
    fn test_profileruntimedata_new() {
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
    fn test_create_profile_snapshot_basic() {
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
                    state: SelectionState::Selected,
                    children: Vec::new(),
                    checksum: Some("cs1".to_string()),
                },
                FileNode {
                    path: file2_path.clone(),
                    name: "file2.txt".into(),
                    is_dir: false,
                    state: SelectionState::Deselected,
                    children: Vec::new(),
                    checksum: Some("cs2".to_string()),
                },
            ],
            cached_token_count: 0, // Not directly used by create_profile_snapshot itself
            cached_file_token_details: HashMap::new(),
        };
        // Populate cached_file_token_details as update_total_token_count_for_selected_files would
        session_data.cached_file_token_details.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: "cs1".to_string(), // Ensure this matches the FileNode's checksum if it's to be used
                token_count: 10,
            },
        );

        // Act
        let mut new_profile = session_data.create_profile_snapshot();
        new_profile.name = "NewProfile".to_string(); // Simulate renaming on save as

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
    fn test_update_total_token_count_selected_files_cache_hit() {
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
                    state: SelectionState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs1.clone()),
                },
                FileNode {
                    path: file2_path.clone(),
                    name: "f2.txt".into(),
                    is_dir: false,
                    state: SelectionState::Selected,
                    children: Vec::new(),
                    checksum: Some(cs2.clone()),
                },
            ],
            cached_token_count: 0,
        };
        let mock_token_counter = MockTokenCounter::new(0); // Default, should not be used

        // Act
        let count = session_data.update_total_token_count_for_selected_files(&mock_token_counter);

        // Assert
        assert_eq!(count, 30, "Expected 10 (f1) + 20 (f2) from cache");
        assert_eq!(session_data.cached_token_count, 30);
        assert!(
            mock_token_counter.get_call_log().is_empty(),
            "Token counter should not be called on cache hits"
        );
    }

    #[test]
    fn test_load_profile_into_session_success_and_updates_session_file_details() {
        // Arrange
        crate::initialize_logging();
        let mut session_data = Box::new(ProfileRuntimeData::new());
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

        let mut initial_profile_file_details = HashMap::new();
        initial_profile_file_details.insert(
            file1_path.clone(),
            FileTokenDetails {
                checksum: file1_checksum_disk.clone(),
                token_count: 10,
            },
        );
        initial_profile_file_details.insert(
            file2_path.clone(),
            FileTokenDetails {
                checksum: "cs2_disk_old_stale".to_string(), // Stale checksum
                token_count: 15,                            // Old token count
            },
        );

        let mut loaded_profile = Profile {
            name: profile_name.to_string(),
            root_folder: root_folder.clone(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: Some(PathBuf::from("/dummy/archive.txt")),
            file_details: initial_profile_file_details,
        };
        loaded_profile.selected_paths.insert(file1_path.clone());
        loaded_profile.selected_paths.insert(file2_path.clone());

        let nodes_from_scanner = vec![
            FileNode {
                path: file1_path.clone(),
                name: "f1.txt".into(),
                is_dir: false,
                state: SelectionState::New, // Will be updated by apply_selection_states_to_nodes
                children: Vec::new(),
                checksum: Some(file1_checksum_disk.clone()),
            },
            FileNode {
                path: file2_path.clone(),
                name: "f2.txt".into(),
                is_dir: false,
                state: SelectionState::New, // Will be updated
                children: Vec::new(),
                checksum: Some(file2_checksum_disk.clone()), // New checksum on disk
            },
        ];
        mock_scanner.set_scan_directory_result(&root_folder, Ok(nodes_from_scanner.clone()));
        mock_token_counter.clear_call_log();

        // Act
        let result = session_data.load_profile_into_session(
            loaded_profile.clone(),
            &mock_scanner,
            &mock_state_manager,
            &mock_token_counter,
        );

        // Assert
        assert!(result.is_ok());
        assert_eq!(
            session_data.get_profile_name().as_deref(),
            Some(profile_name)
        );

        // Verify that apply_selection_states_to_nodes was called and updated the states in snapshot_nodes
        let apply_calls = mock_state_manager.get_apply_profile_to_tree_calls();
        assert_eq!(apply_calls.len(), 1);
        let (selected_in_call, _, _) = &apply_calls[0];
        assert!(selected_in_call.contains(&file1_path));
        assert!(selected_in_call.contains(&file2_path));

        // After load_profile_into_session, the session_data.file_system_snapshot_nodes
        // should have their states (Selected/Deselected/New) correctly set by apply_selection_states_to_nodes.
        // And then update_total_token_count_for_selected_files uses these states.

        let session_details = session_data.get_cached_file_token_details();
        assert_eq!(session_details.get(&file1_path).unwrap().token_count, 10);
        assert_eq!(
            session_details.get(&file1_path).unwrap().checksum,
            file1_checksum_disk
        );
        // For file2, since disk checksum changed, its token count should be re-calculated (20)
        // and cache updated.
        assert_eq!(session_details.get(&file2_path).unwrap().token_count, 20);
        assert_eq!(
            session_details.get(&file2_path).unwrap().checksum,
            file2_checksum_disk
        );
        assert_eq!(session_data.get_cached_total_token_count(), 30); // 10 (f1) + 20 (f2)
    }

    #[test]
    fn test_update_node_state_and_collect_changes_updates_and_collects() {
        // Arrange
        let mut session_data: ProfileRuntimeData = ProfileRuntimeData::new();
        let mock_state_manager = MockStateManager::new();
        let root_path = PathBuf::from("/root");
        let file1_path = root_path.join("file1.txt");
        let dir1_path = root_path.join("dir1");
        let file2_path = dir1_path.join("file2.txt");

        session_data.file_system_snapshot_nodes = vec![
            FileNode {
                path: file1_path.clone(),
                name: "file1.txt".into(),
                is_dir: false,
                state: SelectionState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                path: dir1_path.clone(),
                name: "dir1".into(),
                is_dir: true,
                state: SelectionState::New,
                children: vec![FileNode {
                    path: file2_path.clone(),
                    name: "file2.txt".into(),
                    is_dir: false,
                    state: SelectionState::New,
                    children: vec![],
                    checksum: None,
                }],
                checksum: None,
            },
        ];

        // Act: Select dir1 and its children
        let changes = session_data.update_node_state_and_collect_changes(
            &dir1_path,
            SelectionState::Selected,
            &mock_state_manager,
        );

        // Assert
        // Check MockStateManager calls
        let sm_calls = mock_state_manager.get_update_folder_selection_calls();
        assert_eq!(
            sm_calls.len(),
            2,
            "Expected StateManager to be called for dir1 and file2"
        );
        assert!(
            sm_calls
                .iter()
                .any(|(p, s)| p == &dir1_path && *s == SelectionState::Selected)
        );
        assert!(
            sm_calls
                .iter()
                .any(|(p, s)| p == &file2_path && *s == SelectionState::Selected)
        );

        // Check collected changes
        assert_eq!(changes.len(), 2, "Expected 2 changes collected");
        assert!(changes.contains(&(dir1_path.clone(), SelectionState::Selected)));
        assert!(changes.contains(&(file2_path.clone(), SelectionState::Selected)));

        // Verify internal state of ProfileRuntimeData reflects the change
        let dir1_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &dir1_path,
        )
        .unwrap();
        assert_eq!(dir1_node.state, SelectionState::Selected);
        let file2_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &file2_path,
        )
        .unwrap();
        assert_eq!(file2_node.state, SelectionState::Selected);
        let file1_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &file1_path,
        )
        .unwrap();
        assert_eq!(file1_node.state, SelectionState::New); // Should be unchanged
    }

    #[test]
    fn test_update_total_token_count_cache_miss_or_stale_updates_cache_and_total() {
        // Arrange
        crate::initialize_logging();
        let temp_dir = tempdir().unwrap();
        let content_v1 = "version one content"; // 10 tokens (mocked)
        let content_v2 = "version two new content"; // 20 tokens (mocked)

        let (file_path, _g1) = create_temp_file_with_content(&temp_dir, "f_cache_test", content_v2);
        let checksum_v2 = checksum_utils::calculate_sha256_checksum(&file_path).unwrap();

        let mut mock_token_counter = MockTokenCounter::new(0);
        mock_token_counter.set_count_for_content(&format!("{}\n", content_v1), 10); // For stale case
        mock_token_counter.set_count_for_content(&format!("{}\n", content_v2), 20); // For miss/stale update

        // Case 1: Cache Miss
        let mut session_data_miss = ProfileRuntimeData {
            profile_name: Some("TestProfileMiss".to_string()),
            root_path_for_scan: temp_dir.path().to_path_buf(),
            archive_path: None,
            cached_file_token_details: HashMap::new(), // Empty cache -> miss
            file_system_snapshot_nodes: vec![FileNode {
                path: file_path.clone(),
                name: "f_cache_test.txt".into(),
                is_dir: false,
                state: SelectionState::Selected,
                children: Vec::new(),
                checksum: Some(checksum_v2.clone()), // Current checksum on disk
            }],
            cached_token_count: 0,
        };
        mock_token_counter.clear_call_log();

        // Act (Cache Miss)
        let total_miss =
            session_data_miss.update_total_token_count_for_selected_files(&mock_token_counter);

        // Assert (Cache Miss)
        assert_eq!(
            total_miss, 20,
            "Total count should be 20 for v2 content after miss"
        );
        assert_eq!(session_data_miss.cached_token_count, 20);
        assert_eq!(
            mock_token_counter.get_call_log().len(),
            1,
            "Token counter should be called once on cache miss"
        );
        assert!(
            mock_token_counter.get_call_log()[0].contains(content_v2),
            "Token counter should have processed v2 content"
        );
        let details_after_miss = session_data_miss.get_cached_file_token_details();
        assert!(
            details_after_miss.contains_key(&file_path),
            "Cache should now contain the file after miss"
        );
        let entry_miss = details_after_miss.get(&file_path).unwrap();
        assert_eq!(
            entry_miss.checksum, checksum_v2,
            "Cache checksum should be v2 after miss"
        );
        assert_eq!(
            entry_miss.token_count, 20,
            "Cache token count should be 20 after miss"
        );

        // Case 2: Stale Checksum
        let mut initial_stale_cache = HashMap::new();
        initial_stale_cache.insert(
            file_path.clone(),
            FileTokenDetails {
                checksum: "checksum_v1_stale".to_string(), // Stale checksum
                token_count: 10,                           // Old token count for v1
            },
        );
        let mut session_data_stale = ProfileRuntimeData {
            profile_name: Some("TestProfileStale".to_string()),
            root_path_for_scan: temp_dir.path().to_path_buf(),
            archive_path: None,
            cached_file_token_details: initial_stale_cache, // Cache has stale entry
            file_system_snapshot_nodes: vec![FileNode {
                path: file_path.clone(),
                name: "f_cache_test.txt".into(),
                is_dir: false,
                state: SelectionState::Selected,
                children: Vec::new(),
                checksum: Some(checksum_v2.clone()), // Current checksum on disk is v2
            }],
            cached_token_count: 0,
        };
        mock_token_counter.clear_call_log();

        // Act (Stale Checksum)
        let total_stale =
            session_data_stale.update_total_token_count_for_selected_files(&mock_token_counter);

        // Assert (Stale Checksum)
        assert_eq!(
            total_stale, 20,
            "Total count should be 20 for v2 content after stale"
        );
        assert_eq!(session_data_stale.cached_token_count, 20);
        assert_eq!(
            mock_token_counter.get_call_log().len(),
            1,
            "Token counter should be called once on stale checksum"
        );
        assert!(
            mock_token_counter.get_call_log()[0].contains(content_v2),
            "Token counter should have processed v2 content on stale"
        );
        let details_after_stale = session_data_stale.get_cached_file_token_details();
        assert!(
            details_after_stale.contains_key(&file_path),
            "Cache should still contain the file after stale update"
        );
        let entry_stale = details_after_stale.get(&file_path).unwrap();
        assert_eq!(
            entry_stale.checksum, checksum_v2,
            "Cache checksum should be updated to v2 after stale"
        );
        assert_eq!(
            entry_stale.token_count, 20,
            "Cache token count should be updated to 20 after stale"
        );
    }

    #[test]
    fn test_does_path_or_descendants_contain_new_file() {
        // Arrange
        let root_path = PathBuf::from("/root");
        let file1_new_path = root_path.join("file1_new.txt");
        let file2_selected_path = root_path.join("file2_selected.txt");
        let dir1_path = root_path.join("dir1");
        let file3_in_dir1_new_path = dir1_path.join("file3_new.txt");
        let dir2_path = root_path.join("dir2");
        let file4_in_dir2_selected_path = dir2_path.join("file4_selected.txt");
        let dir3_empty_path = root_path.join("dir3_empty");
        let dir4_deep_new_path = root_path.join("dir4_deep_new");
        let dir4_1_path = dir4_deep_new_path.join("subdir1");
        let dir4_2_path = dir4_1_path.join("subdir2");
        let file5_deep_new_path = dir4_2_path.join("file5_deep_new.txt");

        let mut data = ProfileRuntimeData::new();
        data.file_system_snapshot_nodes = vec![
            FileNode {
                // /root/file1_new.txt
                path: file1_new_path.clone(),
                name: "file1_new.txt".into(),
                is_dir: false,
                state: SelectionState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                // /root/file2_selected.txt
                path: file2_selected_path.clone(),
                name: "file2_selected.txt".into(),
                is_dir: false,
                state: SelectionState::Selected,
                children: vec![],
                checksum: None,
            },
            FileNode {
                // /root/dir1
                path: dir1_path.clone(),
                name: "dir1".into(),
                is_dir: true,
                state: SelectionState::New,
                children: vec![FileNode {
                    // /root/dir1/file3_new.txt
                    path: file3_in_dir1_new_path.clone(),
                    name: "file3_new.txt".into(),
                    is_dir: false,
                    state: SelectionState::New,
                    children: vec![],
                    checksum: None,
                }],
                checksum: None,
            },
            FileNode {
                // /root/dir2
                path: dir2_path.clone(),
                name: "dir2".into(),
                is_dir: true,
                state: SelectionState::New,
                children: vec![FileNode {
                    // /root/dir2/file4_selected.txt
                    path: file4_in_dir2_selected_path.clone(),
                    name: "file4_selected.txt".into(),
                    is_dir: false,
                    state: SelectionState::Selected,
                    children: vec![],
                    checksum: None,
                }],
                checksum: None,
            },
            FileNode {
                // /root/dir3_empty
                path: dir3_empty_path.clone(),
                name: "dir3_empty".into(),
                is_dir: true,
                state: SelectionState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                // /root/dir4_deep_new
                path: dir4_deep_new_path.clone(),
                name: "dir4_deep_new".into(),
                is_dir: true,
                state: SelectionState::Selected,
                children: vec![FileNode {
                    // /root/dir4_deep_new/subdir1
                    path: dir4_1_path.clone(),
                    name: "subdir1".into(),
                    is_dir: true,
                    state: SelectionState::Selected,
                    children: vec![FileNode {
                        // /root/dir4_deep_new/subdir1/subdir2
                        path: dir4_2_path.clone(),
                        name: "subdir2".into(),
                        is_dir: true,
                        state: SelectionState::New,
                        children: vec![FileNode {
                            // /root/dir4_deep_new/subdir1/subdir2/file5_deep_new.txt
                            path: file5_deep_new_path.clone(),
                            name: "file5_deep_new.txt".into(),
                            is_dir: false,
                            state: SelectionState::New,
                            children: vec![],
                            checksum: None,
                        }],
                        checksum: None,
                    }],
                    checksum: None,
                }],
                checksum: None,
            },
        ];

        // Act & Assert
        assert!(
            data.does_path_or_descendants_contain_new_file(&file1_new_path),
            "File 1 (New) should be true"
        );
        assert!(
            !data.does_path_or_descendants_contain_new_file(&file2_selected_path),
            "File 2 (Selected) should be false"
        );

        assert!(
            data.does_path_or_descendants_contain_new_file(&dir1_path),
            "Dir 1 (contains new file3) should be true"
        );
        assert!(
            data.does_path_or_descendants_contain_new_file(&file3_in_dir1_new_path),
            "File 3 (New, in dir1) should be true"
        );

        assert!(
            !data.does_path_or_descendants_contain_new_file(&dir2_path),
            "Dir 2 (contains selected file4) should be false"
        );
        assert!(
            !data.does_path_or_descendants_contain_new_file(&file4_in_dir2_selected_path),
            "File 4 (Selected, in dir2) should be false"
        );

        assert!(
            !data.does_path_or_descendants_contain_new_file(&dir3_empty_path),
            "Dir 3 (empty) should be false"
        );

        assert!(
            data.does_path_or_descendants_contain_new_file(&dir4_deep_new_path),
            "Dir 4 (contains deep new file5) should be true"
        );
        assert!(
            data.does_path_or_descendants_contain_new_file(&dir4_1_path),
            "Dir 4_1 (descendant contains new file5) should be true"
        );
        assert!(
            data.does_path_or_descendants_contain_new_file(&dir4_2_path),
            "Dir 4_2 (contains new file5) should be true"
        );
        assert!(
            data.does_path_or_descendants_contain_new_file(&file5_deep_new_path),
            "File 5 (New, deep) should be true"
        );

        assert!(
            !data.does_path_or_descendants_contain_new_file(&PathBuf::from("/non_existent_path")),
            "Non-existent path should be false"
        );
    }
}
