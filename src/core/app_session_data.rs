/*
 * This module defines the ProfileRuntimeData struct and the ProfileRuntimeDataOperations trait.
 * ProfileRuntimeData holds the core, mutable data for an active application session,
 * such as current profile details, scanned file nodes, and token caches.
 * The ProfileRuntimeDataOperations trait provides an abstraction for interacting with
 * this session data, facilitating dependency injection and testing.
 */
use crate::core::{
    FileNode, FileState, FileSystemScannerOperations, Profile, StateManagerOperations,
    TokenCounterOperations, models::FileTokenDetails,
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
    fn set_root_path_for_scan(&mut self, path: PathBuf);

    // File system snapshot (nodes)
    fn get_snapshot_nodes(&self) -> &Vec<FileNode>;
    fn clear_snapshot_nodes(&mut self);
    fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>);
    fn apply_selection_states_to_snapshot(
        &mut self,
        state_manager: &dyn StateManagerOperations,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    );
    fn get_node_attributes_for_path(&self, path: &Path) -> Option<(FileState, bool)>; // (state, is_dir)
    fn update_node_state_and_collect_changes(
        &mut self,
        path: &Path,
        new_state: FileState,
        state_manager: &dyn StateManagerOperations,
    ) -> Vec<(PathBuf, FileState)>;

    // Token related data
    fn get_cached_file_token_details(&self) -> HashMap<PathBuf, FileTokenDetails>;
    fn set_cached_file_token_details(&mut self, details: HashMap<PathBuf, FileTokenDetails>);
    fn get_cached_total_token_count(&self) -> usize;
    fn update_total_token_count(&mut self, token_counter: &dyn TokenCounterOperations) -> usize;

    // General session management
    fn clear(&mut self);
    fn create_profile_snapshot(
        &self,
        new_profile_name: String,
        token_counter: &dyn TokenCounterOperations,
    ) -> Profile;
    fn load_profile_into_session(
        &mut self,
        loaded_profile: Profile,
        file_system_scanner: &dyn FileSystemScannerOperations,
        state_manager: &dyn StateManagerOperations,
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
                FileState::Selected => {
                    selected.insert(node.path.clone());
                }
                FileState::Deselected => {
                    deselected.insert(node.path.clone());
                }
                FileState::New => {}
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

    // Helper: Populates file details map for saving, based on selected files.
    fn populate_file_details_recursive_for_save_internal(
        nodes: &[FileNode],
        file_details_map: &mut HashMap<PathBuf, FileTokenDetails>,
        token_counter: &dyn TokenCounterOperations,
    ) {
        for node in nodes {
            if node.is_dir {
                Self::populate_file_details_recursive_for_save_internal(
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
                                "ProfileRuntimeData (populate_file_details_for_save): Cached token count {} for selected file {:?} with checksum {}",
                                token_count,
                                node.path,
                                checksum_val
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "ProfileRuntimeData (populate_file_details_for_save): Failed to read file {:?} for token caching during profile save: {}",
                                node.path,
                                e
                            );
                        }
                    }
                } else {
                    log::warn!(
                        "ProfileRuntimeData (populate_file_details_for_save): Selected file {:?} has no checksum; cannot cache token count for profile save.",
                        node.path
                    );
                }
            }
        }
    }

    // Helper: Updates cached file details based on current disk state.
    fn update_cached_file_details_recursive_internal(
        nodes: &[FileNode],
        cached_details_mut: &mut HashMap<PathBuf, FileTokenDetails>,
        token_counter: &dyn TokenCounterOperations,
    ) {
        for node in nodes {
            if node.is_dir {
                Self::update_cached_file_details_recursive_internal(
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

    // Helper: Collects (PathBuf, FileState) for a node and its children.
    fn collect_node_states_recursive(node: &FileNode, updates: &mut Vec<(PathBuf, FileState)>) {
        updates.push((node.path.clone(), node.state));
        if node.is_dir {
            for child in &node.children {
                Self::collect_node_states_recursive(child, updates);
            }
        }
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

    fn set_root_path_for_scan(&mut self, path: PathBuf) {
        self.root_path_for_scan = path;
    }

    fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
        &self.file_system_snapshot_nodes
    }

    fn clear_snapshot_nodes(&mut self) {
        self.file_system_snapshot_nodes.clear();
    }

    fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>) {
        self.file_system_snapshot_nodes = nodes;
    }

    fn apply_selection_states_to_snapshot(
        &mut self,
        state_manager: &dyn StateManagerOperations,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    ) {
        state_manager.apply_selection_states_to_nodes(
            &mut self.file_system_snapshot_nodes,
            selected_paths,
            deselected_paths,
        );
    }

    fn get_node_attributes_for_path(&self, path: &Path) -> Option<(FileState, bool)> {
        Self::find_node_recursive_ref(&self.file_system_snapshot_nodes, path)
            .map(|node| (node.state, node.is_dir))
    }

    fn update_node_state_and_collect_changes(
        &mut self,
        path: &Path,
        new_state: FileState,
        state_manager: &dyn StateManagerOperations,
    ) -> Vec<(PathBuf, FileState)> {
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

    fn get_cached_file_token_details(&self) -> HashMap<PathBuf, FileTokenDetails> {
        self.cached_file_token_details.clone()
    }

    fn set_cached_file_token_details(&mut self, details: HashMap<PathBuf, FileTokenDetails>) {
        self.cached_file_token_details = details;
    }

    fn get_cached_total_token_count(&self) -> usize {
        self.cached_token_count
    }

    /*
     * Recalculates the estimated token count for all currently selected files
     * using `self.cached_file_token_details`. The result is stored internally and returned.
     */
    fn update_total_token_count(&mut self, token_counter: &dyn TokenCounterOperations) -> usize {
        log::debug!(
            "Updating token count using session's cached_file_token_details for selected files."
        );
        let mut total_tokens: usize = 0;
        let mut files_processed_from_cache: usize = 0;
        let mut files_processed_fallback: usize = 0;
        let mut files_failed_fallback: usize = 0;

        // Helper function to recursively traverse the file node tree
        fn count_tokens_recursive_cached_inner(
            nodes: &[FileNode],
            cached_details_map: &HashMap<PathBuf, FileTokenDetails>,
            current_total_tokens: &mut usize,
            processed_cache: &mut usize,
            processed_fallback: &mut usize,
            failed_fallback: &mut usize,
            token_counter_ref: &dyn TokenCounterOperations,
        ) {
            for node in nodes {
                if !node.is_dir && node.state == FileState::Selected {
                    let mut token_value_for_file: Option<usize> = None;

                    if let Some(details) = cached_details_map.get(&node.path) {
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
                        cached_details_map,
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
            &self.cached_file_token_details,
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
     * TODO: Shouldn't call populate_file_details_recursive_for_save_internal?
     */
    fn create_profile_snapshot(
        &self,
        new_profile_name: String,
        token_counter: &dyn TokenCounterOperations,
    ) -> Profile {
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();
        let mut file_details_for_save = HashMap::new();

        Self::gather_selected_deselected_paths_recursive_internal(
            &self.file_system_snapshot_nodes,
            &mut selected_paths,
            &mut deselected_paths,
        );

        Self::populate_file_details_recursive_for_save_internal(
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
            file_details: file_details_for_save,
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
        state_manager: &dyn StateManagerOperations,
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

                Self::update_cached_file_details_recursive_internal(
                    &self.file_system_snapshot_nodes,
                    &mut self.cached_file_token_details,
                    token_counter,
                );

                self.update_total_token_count(token_counter);
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
        update_folder_selection_calls: Mutex<Vec<(PathBuf, FileState)>>,
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
        fn get_update_folder_selection_calls(&self) -> Vec<(PathBuf, FileState)> {
            self.update_folder_selection_calls.lock().unwrap().clone()
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
        fn update_folder_selection(&self, node: &mut FileNode, new_state: FileState) {
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
            cached_token_count: 0,
            cached_file_token_details: HashMap::new(),
        };
        let mut specific_token_counter = MockTokenCounter::new(0);
        let file1_content_as_read = format!("{}\n", file1_content_written);
        specific_token_counter.set_count_for_content(&file1_content_as_read, 10);

        // Act
        let new_profile =
            session_data.create_profile_snapshot("NewProfile".to_string(), &specific_token_counter);

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
        let count = session_data.update_total_token_count(&mock_token_counter);

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
        let mut session_data: Box<dyn ProfileRuntimeDataOperations> =
            Box::new(ProfileRuntimeData::new());
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
                checksum: "cs2_disk_old_stale".to_string(),
                token_count: 15,
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

        let session_details = session_data.get_cached_file_token_details();
        assert_eq!(session_details.get(&file1_path).unwrap().token_count, 10);
        assert_eq!(
            session_details.get(&file1_path).unwrap().checksum,
            file1_checksum_disk
        );
        assert_eq!(session_details.get(&file2_path).unwrap().token_count, 20);
        assert_eq!(
            session_details.get(&file2_path).unwrap().checksum,
            file2_checksum_disk
        );
        assert_eq!(session_data.get_cached_total_token_count(), 30);
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
                state: FileState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                path: dir1_path.clone(),
                name: "dir1".into(),
                is_dir: true,
                state: FileState::New,
                children: vec![FileNode {
                    path: file2_path.clone(),
                    name: "file2.txt".into(),
                    is_dir: false,
                    state: FileState::New,
                    children: vec![],
                    checksum: None,
                }],
                checksum: None,
            },
        ];

        // Act: Select dir1 and its children
        let changes = session_data.update_node_state_and_collect_changes(
            &dir1_path,
            FileState::Selected,
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
                .any(|(p, s)| p == &dir1_path && *s == FileState::Selected)
        );
        assert!(
            sm_calls
                .iter()
                .any(|(p, s)| p == &file2_path && *s == FileState::Selected)
        );

        // Check collected changes
        assert_eq!(changes.len(), 2, "Expected 2 changes collected");
        assert!(changes.contains(&(dir1_path.clone(), FileState::Selected)));
        assert!(changes.contains(&(file2_path.clone(), FileState::Selected)));

        // Verify internal state of ProfileRuntimeData reflects the change
        let dir1_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &dir1_path,
        )
        .unwrap();
        assert_eq!(dir1_node.state, FileState::Selected);
        let file2_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &file2_path,
        )
        .unwrap();
        assert_eq!(file2_node.state, FileState::Selected);
        let file1_node = ProfileRuntimeData::find_node_recursive_ref(
            &session_data.file_system_snapshot_nodes,
            &file1_path,
        )
        .unwrap();
        assert_eq!(file1_node.state, FileState::New); // Should be unchanged
    }
}
