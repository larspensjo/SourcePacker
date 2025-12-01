#[cfg(test)]
mod tests {
    use crate::app_logic::{SearchMode, handler::*, ui_constants};

    use crate::core::token_progress::TokenProgressEntry;
    use crate::core::{
        ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations,
        ContentSearchProgress, ContentSearchResult, FileNode, FileSystemError,
        FileSystemScannerOperations, NodeStateApplicatorOperations, Profile, ProfileError,
        ProfileManagerOperations, ProfileRuntimeDataOperations, SelectionState,
        TokenCounterOperations, TokenProgress, TokenProgressChannel, file_node::FileTokenDetails,
    };
    use crate::platform_layer::{
        AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler, StyleId,
        TreeItemId, UiStateProvider, WindowId,
    };

    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::io::{self};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread;
    use std::time::SystemTime;
    use tempfile::tempdir;

    type SelectionSnapshotLog = (Vec<FileNode>, HashSet<PathBuf>, HashSet<PathBuf>);
    type MockSetupResult = (
        MyAppLogic,
        Arc<Mutex<MockProfileRuntimeData>>,
        Arc<MockConfigManager>,
        Arc<MockProfileManager>,
        Arc<MockFileSystemScanner>,
        Arc<MockArchiver>,
        Arc<MockStateManager>,
        Arc<MockTokenCounter>,
    );

    /*
     * This module contains unit tests for `MyAppLogic` from the `super::handler` module.
     * It utilizes mock implementations of core dependencies, including a new
     * `MockProfileRuntimeData` for `ProfileRuntimeDataOperations`, to isolate
     * `MyAppLogic`'s behavior for testing. Tests focus on event handling, state
     * transitions, command generation, and error paths, adapting to trait-based
     * dependency injection for session data.
     */

    // --- MockProfileRuntimeData ---
    #[derive(Debug)]
    struct MockProfileRuntimeData {
        profile_name: Option<String>,
        archive_path: Option<PathBuf>,
        root_path_for_scan: PathBuf,
        snapshot_nodes: Vec<FileNode>,
        cached_file_token_details: HashMap<PathBuf, FileTokenDetails>,
        cached_total_token_count: usize,
        exclude_patterns: Vec<String>,

        // Call counters for &self methods using AtomicUsize
        get_profile_name_calls: AtomicUsize,
        get_archive_path_calls: AtomicUsize,
        get_snapshot_nodes_calls: AtomicUsize,
        get_root_path_for_scan_calls: AtomicUsize,
        create_profile_snapshot_calls: AtomicUsize,
        get_node_attributes_for_path_calls: AtomicUsize,
        does_path_or_descendants_contain_new_file_calls: AtomicUsize,

        // Call logs/trackers for &mut self methods (plain types, as they are called on &mut MockProfileRuntimeData)
        _set_profile_name_log: Mutex<Vec<Option<String>>>,
        _set_archive_path_log: Mutex<Vec<Option<PathBuf>>>,
        _set_root_path_for_scan_log: Mutex<Vec<PathBuf>>,
        _set_snapshot_nodes_log: Mutex<Vec<Vec<FileNode>>>,
        _apply_selection_states_to_snapshot_log: Mutex<Vec<(HashSet<PathBuf>, HashSet<PathBuf>)>>,
        _update_node_state_and_collect_changes_log: Mutex<Vec<(PathBuf, SelectionState)>>,
        _set_cached_file_token_details_log: Mutex<Vec<HashMap<PathBuf, FileTokenDetails>>>,
        _update_total_token_count_calls: AtomicUsize,
        _clear_calls: AtomicUsize,
        _load_profile_into_session_log: Mutex<Vec<Profile>>,
        _does_path_or_descendants_contain_new_file_log: Mutex<Vec<PathBuf>>,
        _get_current_selection_paths_calls: AtomicUsize,
        recalc_tokens_async_calls: AtomicUsize,
        apply_token_progress_calls: AtomicUsize,
        search_content_async_calls: AtomicUsize,
        content_search_terms: Mutex<Vec<String>>,
        content_search_receiver: Mutex<Option<mpsc::Receiver<ContentSearchProgress>>>,

        // Mock results
        // get_node_attributes_for_path_result: Option<(SelectionState, bool)>, <- now derived from snapshot_nodes
        update_node_state_and_collect_changes_result: Mutex<Vec<(PathBuf, SelectionState)>>,
        load_profile_into_session_result: Mutex<Result<(), String>>,
        does_path_or_descendants_contain_new_file_results: Mutex<HashMap<PathBuf, bool>>,
        update_total_token_count_for_selected_files_result: AtomicUsize,
    }

    impl MockProfileRuntimeData {
        fn new() -> Self {
            MockProfileRuntimeData {
                profile_name: None,
                archive_path: None,
                root_path_for_scan: PathBuf::from("/mock/default_root"),
                snapshot_nodes: Vec::new(),
                cached_file_token_details: HashMap::new(),
                cached_total_token_count: 0,
                exclude_patterns: Vec::new(),

                get_profile_name_calls: AtomicUsize::new(0),
                get_archive_path_calls: AtomicUsize::new(0),
                get_snapshot_nodes_calls: AtomicUsize::new(0),
                get_root_path_for_scan_calls: AtomicUsize::new(0),
                create_profile_snapshot_calls: AtomicUsize::new(0),
                get_node_attributes_for_path_calls: AtomicUsize::new(0),
                does_path_or_descendants_contain_new_file_calls: AtomicUsize::new(0),

                _set_profile_name_log: Mutex::new(Vec::new()),
                _set_archive_path_log: Mutex::new(Vec::new()),
                _set_root_path_for_scan_log: Mutex::new(Vec::new()),
                _set_snapshot_nodes_log: Mutex::new(Vec::new()),
                _apply_selection_states_to_snapshot_log: Mutex::new(Vec::new()),
                _update_node_state_and_collect_changes_log: Mutex::new(Vec::new()),
                _set_cached_file_token_details_log: Mutex::new(Vec::new()),
                _update_total_token_count_calls: AtomicUsize::new(0),
                _clear_calls: AtomicUsize::new(0),
                _load_profile_into_session_log: Mutex::new(Vec::new()),
                _does_path_or_descendants_contain_new_file_log: Mutex::new(Vec::new()),
                _get_current_selection_paths_calls: AtomicUsize::new(0),
                recalc_tokens_async_calls: AtomicUsize::new(0),
                apply_token_progress_calls: AtomicUsize::new(0),
                search_content_async_calls: AtomicUsize::new(0),
                content_search_terms: Mutex::new(Vec::new()),
                content_search_receiver: Mutex::new(None),

                update_node_state_and_collect_changes_result: Mutex::new(Vec::new()),
                load_profile_into_session_result: Mutex::new(Ok(())),
                does_path_or_descendants_contain_new_file_results: Mutex::new(HashMap::new()),
                update_total_token_count_for_selected_files_result: AtomicUsize::new(0),
            }
        }

        // Test setters for mock's internal data (called on &mut MockProfileRuntimeData)
        #[allow(dead_code)]
        fn set_profile_name_for_mock(&mut self, name: Option<String>) {
            self.profile_name = name;
        }
        #[allow(dead_code)]
        fn set_archive_path_for_mock(&mut self, path: Option<PathBuf>) {
            self.archive_path = path;
        }
        #[allow(dead_code)]
        fn set_root_path_for_scan_for_mock(&mut self, path: PathBuf) {
            self.root_path_for_scan = path;
        }
        #[allow(dead_code)]
        fn set_snapshot_nodes_for_mock(&mut self, nodes: Vec<FileNode>) {
            self.snapshot_nodes = nodes;
        }
        #[allow(dead_code)]
        fn set_cached_total_token_count_for_mock(&mut self, count: usize) {
            self.cached_total_token_count = count;
            self.update_total_token_count_for_selected_files_result
                .store(count, Ordering::Relaxed);
        }
        #[allow(dead_code)]
        fn set_cached_file_token_details_for_mock(
            &mut self,
            details: HashMap<PathBuf, FileTokenDetails>,
        ) {
            self.cached_file_token_details = details;
        }
        #[allow(dead_code)]
        fn set_exclude_patterns_for_mock(&mut self, patterns: Vec<String>) {
            self.exclude_patterns = patterns;
        }
        #[allow(dead_code)]
        fn get_exclude_patterns_for_mock(&self) -> Vec<String> {
            self.exclude_patterns.clone()
        }
        #[allow(dead_code)]
        fn set_update_node_state_and_collect_changes_result(
            &self, // Note: &self because Mutex
            result: Vec<(PathBuf, SelectionState)>,
        ) {
            *self
                .update_node_state_and_collect_changes_result
                .lock()
                .unwrap() = result;
        }
        #[allow(dead_code)]
        fn set_load_profile_into_session_result(&self, result: Result<(), String>) {
            // Note: &self
            *self.load_profile_into_session_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn set_does_path_or_descendants_contain_new_file_result(&self, path: &Path, result: bool) {
            self.does_path_or_descendants_contain_new_file_results
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), result);
        }

        fn set_content_search_receiver_for_mock(
            &self,
            receiver: Option<mpsc::Receiver<ContentSearchProgress>>,
        ) {
            *self.content_search_receiver.lock().unwrap() = receiver;
        }

        fn get_search_content_async_call_count(&self) -> usize {
            self.search_content_async_calls.load(Ordering::Relaxed)
        }

        // Test getters for call logs/counters
        #[allow(dead_code)]
        fn get_load_profile_into_session_log(&self) -> Vec<Profile> {
            self._load_profile_into_session_log.lock().unwrap().clone()
        }
        #[allow(dead_code)]
        fn get_set_archive_path_calls_log(&self) -> Vec<Option<PathBuf>> {
            self._set_archive_path_log.lock().unwrap().clone()
        }

        #[allow(dead_code)]
        fn get_set_profile_name_calls_log(&self) -> Vec<Option<String>> {
            self._set_profile_name_log.lock().unwrap().clone()
        }

        #[allow(dead_code)]
        fn get_update_node_state_and_collect_changes_log(&self) -> Vec<(PathBuf, SelectionState)> {
            self._update_node_state_and_collect_changes_log
                .lock()
                .unwrap()
                .clone()
        }

        #[allow(dead_code)]
        fn get_save_profile_calls_log(&self) -> Vec<Profile> {
            self.create_profile_snapshot_calls.load(Ordering::Relaxed); // This seems wrong, should be related to save.
            // This should likely be tracked by MockProfileManager instead
            vec![] // Placeholder
        }

        #[allow(dead_code)]
        fn get_does_path_or_descendants_contain_new_file_log(&self) -> Vec<PathBuf> {
            self._does_path_or_descendants_contain_new_file_log
                .lock()
                .unwrap()
                .clone()
        }
    }

    impl ProfileRuntimeDataOperations for MockProfileRuntimeData {
        fn get_profile_name(&self) -> Option<String> {
            self.get_profile_name_calls.fetch_add(1, Ordering::Relaxed);
            self.profile_name.clone()
        }
        fn set_profile_name(&mut self, name: Option<String>) {
            self._set_profile_name_log
                .lock()
                .unwrap()
                .push(name.clone());
            self.profile_name = name;
        }
        fn get_archive_path(&self) -> Option<PathBuf> {
            self.get_archive_path_calls.fetch_add(1, Ordering::Relaxed);
            self.archive_path.clone()
        }
        fn set_archive_path(&mut self, path: Option<PathBuf>) {
            self._set_archive_path_log
                .lock()
                .unwrap()
                .push(path.clone());
            self.archive_path = path;
        }
        fn get_root_path_for_scan(&self) -> PathBuf {
            self.get_root_path_for_scan_calls
                .fetch_add(1, Ordering::Relaxed);
            self.root_path_for_scan.clone()
        }
        fn get_exclude_patterns(&self) -> Vec<String> {
            self.exclude_patterns.clone()
        }
        fn set_exclude_patterns(&mut self, patterns: Vec<String>) {
            self.exclude_patterns = patterns;
        }
        fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
            self.get_snapshot_nodes_calls
                .fetch_add(1, Ordering::Relaxed);
            &self.snapshot_nodes
        }
        fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>) {
            self._set_snapshot_nodes_log
                .lock()
                .unwrap()
                .push(nodes.clone());
            self.snapshot_nodes = nodes;
        }
        fn apply_selection_states_to_snapshot(
            &mut self,
            _state_manager: &dyn NodeStateApplicatorOperations,
            selected_paths: &HashSet<PathBuf>,
            deselected_paths: &HashSet<PathBuf>,
        ) {
            self._apply_selection_states_to_snapshot_log
                .lock()
                .unwrap()
                .push((selected_paths.clone(), deselected_paths.clone()));
            // Basic simulation for mock:
            fn apply_recursive(
                nodes: &mut [FileNode],
                selected: &HashSet<PathBuf>,
                deselected: &HashSet<PathBuf>,
            ) {
                for node in nodes.iter_mut() {
                    if selected.contains(node.path()) {
                        node.set_state(SelectionState::Selected);
                    } else if deselected.contains(node.path()) {
                        node.set_state(SelectionState::Deselected);
                    } else {
                        // If not explicitly selected or deselected, assume it's New for this mock.
                        // A more sophisticated mock might preserve original states or handle Unknown.
                        node.set_state(SelectionState::New);
                    }
                    if node.is_dir() {
                        apply_recursive(node.children.as_mut_slice(), selected, deselected);
                    }
                }
            }
            apply_recursive(
                self.snapshot_nodes.as_mut_slice(),
                selected_paths,
                deselected_paths,
            );
        }
        fn get_node_attributes_for_path(
            &self,
            path_to_find: &Path,
        ) -> Option<(SelectionState, bool)> {
            self.get_node_attributes_for_path_calls
                .fetch_add(1, Ordering::Relaxed);
            fn find_node_attrs_recursive(
                nodes: &[FileNode],
                path: &Path,
            ) -> Option<(SelectionState, bool)> {
                for node in nodes {
                    if node.path() == path {
                        return Some((node.state(), node.is_dir()));
                    }
                    if node.is_dir() && path.starts_with(node.path()) {
                        // Optimization: only search children if path could be inside
                        if let Some(attrs) = find_node_attrs_recursive(&node.children, path) {
                            return Some(attrs);
                        }
                    }
                }
                None
            }
            find_node_attrs_recursive(&self.snapshot_nodes, path_to_find)
        }
        fn update_node_state_and_collect_changes(
            &mut self,
            path: &Path,
            new_state: SelectionState,
            _state_manager: &dyn NodeStateApplicatorOperations,
        ) -> Vec<(PathBuf, SelectionState)> {
            self._update_node_state_and_collect_changes_log
                .lock()
                .unwrap()
                .push((path.to_path_buf(), new_state));

            let mut actual_changes = Vec::new();
            fn update_recursive(
                nodes: &mut [FileNode],
                target_path: &Path,
                new_sel_state: SelectionState,
                changes: &mut Vec<(PathBuf, SelectionState)>,
            ) -> bool {
                // Returns true if target_path was found and processed
                let mut found_target = false;
                for node in nodes.iter_mut() {
                    if node.path() == target_path {
                        node.set_state(new_sel_state);
                        changes.push((node.path().to_path_buf(), node.state()));
                        if node.is_dir() {
                            // Apply to all children recursively if a directory is toggled
                            for child in node.children.iter_mut() {
                                update_recursive_children(child, new_sel_state, changes);
                            }
                        }
                        found_target = true;
                        break;
                    }
                    // If the target_path is a descendant of the current node.is_dir, recurse
                    if node.is_dir()
                        && target_path.starts_with(node.path())
                        && update_recursive(
                            node.children.as_mut_slice(),
                            target_path,
                            new_sel_state,
                            changes,
                        )
                    {
                        found_target = true; // Found in children
                        // Potentially update parent's state based on children if needed by NodeStateApplicator logic
                        // For this mock, we'll skip complex parent state updates.
                        break;
                    }
                }
                found_target
            }
            fn update_recursive_children(
                node: &mut FileNode,
                new_sel_state: SelectionState,
                changes: &mut Vec<(PathBuf, SelectionState)>,
            ) {
                node.set_state(new_sel_state);
                changes.push((node.path().to_path_buf(), node.state()));
                if node.is_dir() {
                    for child in node.children.iter_mut() {
                        update_recursive_children(child, new_sel_state, changes);
                    }
                }
            }
            update_recursive(
                self.snapshot_nodes.as_mut_slice(),
                path,
                new_state,
                &mut actual_changes,
            );

            if !actual_changes.is_empty() {
                actual_changes
            } else {
                // Fallback to preset result if simulation didn't produce changes (e.g., path not found)
                self.update_node_state_and_collect_changes_result
                    .lock()
                    .unwrap()
                    .clone()
            }
        }

        fn does_path_or_descendants_contain_new_file(&self, path: &Path) -> bool {
            self.does_path_or_descendants_contain_new_file_calls
                .fetch_add(1, Ordering::Relaxed);
            self._does_path_or_descendants_contain_new_file_log
                .lock()
                .unwrap()
                .push(path.to_path_buf());

            if let Some(result) = self
                .does_path_or_descendants_contain_new_file_results
                .lock()
                .unwrap()
                .get(path)
            {
                return *result;
            }
            fn check_recursive(nodes: &[FileNode], target_path: &Path) -> Option<bool> {
                for node in nodes {
                    if node.path() == target_path {
                        return Some(check_node_itself_or_descendants(node));
                    }
                    if node.is_dir() && target_path.starts_with(node.path()) {
                        if let Some(found_in_child) = check_recursive(&node.children, target_path) {
                            if found_in_child {
                                return Some(true);
                            }
                        }
                    }
                }
                None
            }
            fn check_node_itself_or_descendants(node: &FileNode) -> bool {
                if !node.is_dir() {
                    return node.state() == SelectionState::New;
                }
                // For a directory, check if any of its children (recursively) are New.
                // The directory's own state doesn't make it "contain a new file" for this method's purpose.
                for child in &node.children {
                    if check_node_itself_or_descendants(child) {
                        return true;
                    }
                }
                false
            }
            check_recursive(&self.snapshot_nodes, path).unwrap_or(false)
        }

        fn update_total_token_count_for_selected_files(
            &mut self,
            __token_counter: &dyn TokenCounterOperations,
        ) -> usize {
            self._update_total_token_count_calls
                .fetch_add(1, Ordering::Relaxed);
            // Simulate if needed or just return preset
            self.update_total_token_count_for_selected_files_result
                .load(Ordering::Relaxed)
        }
        fn recalc_tokens_async(
            &mut self,
            _token_counter: Arc<dyn TokenCounterOperations>,
            _only_selected: bool,
        ) -> Option<TokenProgressChannel> {
            self.cached_total_token_count = 0;
            self.recalc_tokens_async_calls
                .fetch_add(1, Ordering::Relaxed);
            let preset_total = self
                .update_total_token_count_for_selected_files_result
                .load(Ordering::Relaxed);
            let (tx, rx) = mpsc::channel();
            let handle = thread::spawn(move || {
                if preset_total > 0 {
                    let entry = TokenProgressEntry {
                        path: PathBuf::from("mock_selected.txt"),
                        token_count: preset_total,
                        is_selected: true,
                        details: None,
                        invalidate_cache: false,
                    };
                    let progress = TokenProgress {
                        entries: vec![entry],
                        files_processed: 1,
                        total_files: 1,
                        is_final: false,
                    };
                    let _ = tx.send(progress);
                    let _ = tx.send(TokenProgress {
                        entries: Vec::new(),
                        files_processed: 1,
                        total_files: 1,
                        is_final: true,
                    });
                } else {
                    let _ = tx.send(TokenProgress {
                        entries: Vec::new(),
                        files_processed: 0,
                        total_files: 0,
                        is_final: true,
                    });
                }
            });
            Some(TokenProgressChannel {
                receiver: rx,
                worker_handle: Some(handle),
                total_files: if preset_total > 0 { 1 } else { 0 },
            })
        }
        fn apply_token_progress(&mut self, progress: TokenProgress) -> usize {
            self.apply_token_progress_calls
                .fetch_add(1, Ordering::Relaxed);
            for entry in progress.entries {
                let TokenProgressEntry {
                    path,
                    token_count,
                    is_selected,
                    details,
                    invalidate_cache,
                } = entry;

                if invalidate_cache {
                    self.cached_file_token_details.remove(&path);
                }

                if let Some(details) = details {
                    self.cached_file_token_details.insert(path.clone(), details);
                }

                if is_selected {
                    self.cached_total_token_count =
                        self.cached_total_token_count.saturating_add(token_count);
                }
            }
            self.cached_total_token_count
        }
        fn search_content_async(
            &self,
            search_term: String,
        ) -> Option<mpsc::Receiver<ContentSearchProgress>> {
            self.search_content_async_calls
                .fetch_add(1, Ordering::Relaxed);
            self.content_search_terms.lock().unwrap().push(search_term);
            self.content_search_receiver.lock().unwrap().take()
        }
        fn clear(&mut self) {
            self._clear_calls.fetch_add(1, Ordering::Relaxed);
            self.profile_name = None;
            self.archive_path = None;
            self.snapshot_nodes.clear();
            self.root_path_for_scan = PathBuf::from("."); // Or some default
            self.cached_total_token_count = 0;
            self.update_total_token_count_for_selected_files_result
                .store(0, Ordering::Relaxed);
            self.cached_file_token_details.clear();
            self.exclude_patterns.clear();
        }
        fn create_profile_snapshot(&self) -> Profile {
            self.create_profile_snapshot_calls
                .fetch_add(1, Ordering::Relaxed);
            let mut profile = Profile::new(
                self.profile_name.clone().unwrap_or_default(),
                self.root_path_for_scan.clone(),
            );
            profile.archive_path = self.archive_path.clone();
            profile.file_details = self.cached_file_token_details.clone();

            fn gather_paths_recursive(
                nodes: &[FileNode],
                selected: &mut HashSet<PathBuf>,
                deselected: &mut HashSet<PathBuf>,
            ) {
                for node in nodes {
                    match node.state() {
                        SelectionState::Selected => {
                            selected.insert(node.path().to_path_buf());
                        }
                        SelectionState::Deselected => {
                            deselected.insert(node.path().to_path_buf());
                        }
                        _ => {} // New/Unknown states are not explicitly stored in profile's selected/deselected sets
                    }
                    if node.is_dir() {
                        gather_paths_recursive(&node.children, selected, deselected);
                    }
                }
            }
            gather_paths_recursive(
                &self.snapshot_nodes,
                &mut profile.selected_paths,
                &mut profile.deselected_paths,
            );
            profile.exclude_patterns = self.exclude_patterns.clone();
            profile
        }

        fn load_profile_into_session(
            &mut self,
            loaded_profile: Profile,
            file_system_scanner: &dyn FileSystemScannerOperations, // Added underscore as it's now used
            state_manager: &dyn NodeStateApplicatorOperations,     // Added underscore
            _token_counter: &dyn TokenCounterOperations,           // Added underscore
        ) -> Result<(), String> {
            self._load_profile_into_session_log
                .lock()
                .unwrap()
                .push(loaded_profile.clone());

            // Simulate the behavior of the real ProfileRuntimeData::load_profile_into_session
            match file_system_scanner.scan_directory(
                &loaded_profile.root_folder,
                &loaded_profile.exclude_patterns,
            ) {
                Ok(scanned_nodes) => {
                    self.profile_name = Some(loaded_profile.name.clone());
                    self.archive_path = loaded_profile.archive_path.clone();
                    self.root_path_for_scan = loaded_profile.root_folder.clone();
                    self.snapshot_nodes = scanned_nodes; // <<< KEY CHANGE: Populate snapshot_nodes from scanner
                    self.cached_file_token_details = loaded_profile.file_details.clone();
                    self.exclude_patterns = loaded_profile.exclude_patterns.clone();

                    // Simulate applying selection states (simplified for mock)
                    self.apply_selection_states_to_snapshot(
                        state_manager,
                        &loaded_profile.selected_paths,
                        &loaded_profile.deselected_paths,
                    );

                    // Respect the pre-set mock result if it was an error, otherwise Ok
                    let preset_result = self
                        .load_profile_into_session_result
                        .lock()
                        .unwrap()
                        .clone();
                    if preset_result.is_err() {
                        self.clear(); // Simulate clearing data on explicit mock error
                        preset_result
                    } else {
                        Ok(())
                    }
                }
                Err(scan_error) => {
                    self.clear(); // Clear data on scan failure
                    log::error!(
                        "MockProfileRuntimeData: Scan failed during load_profile_into_session: {scan_error:?}"
                    );
                    Err(format!(
                        "Mock scan failed for root {:?}: {:?}",
                        loaded_profile.root_folder, scan_error
                    ))
                }
            }
        }

        fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
            self._get_current_selection_paths_calls
                .fetch_add(1, Ordering::Relaxed);
            let mut selected = HashSet::new();
            let mut deselected = HashSet::new();
            fn gather_paths_recursive(
                nodes: &[FileNode],
                selected: &mut HashSet<PathBuf>,
                deselected: &mut HashSet<PathBuf>,
            ) {
                for node in nodes {
                    match node.state() {
                        SelectionState::Selected => {
                            selected.insert(node.path().to_path_buf());
                        }
                        SelectionState::Deselected => {
                            deselected.insert(node.path().to_path_buf());
                        }
                        _ => {}
                    }
                    if node.is_dir() {
                        gather_paths_recursive(&node.children, selected, deselected);
                    }
                }
            }
            gather_paths_recursive(&self.snapshot_nodes, &mut selected, &mut deselected);
            (selected, deselected)
        }
    }
    // --- End MockProfileRuntimeData ---

    // --- Mock Structures (ConfigManager, ProfileManager, FileSystemScanner, Archiver, StateManager) ---
    // These are assumed to be correct from previous steps.
    struct MockConfigManager {
        load_last_project_path_result: Mutex<Result<Option<PathBuf>, ConfigError>>,
        saved_project_path: Mutex<Option<(String, Option<PathBuf>)>>,
        save_last_project_path_calls: AtomicUsize,
    }
    impl MockConfigManager {
        fn new() -> Self {
            MockConfigManager {
                load_last_project_path_result: Mutex::new(Ok(None)),
                saved_project_path: Mutex::new(None),
                save_last_project_path_calls: AtomicUsize::new(0),
            }
        }
        fn set_load_last_project_path_result(&self, result: Result<Option<PathBuf>, ConfigError>) {
            *self.load_last_project_path_result.lock().unwrap() = result;
        }
    }
    impl ConfigManagerOperations for MockConfigManager {
        fn load_last_project_path(&self, _app_name: &str) -> Result<Option<PathBuf>, ConfigError> {
            self.load_last_project_path_result
                .lock()
                .unwrap()
                .as_ref()
                .map(|opt_s| opt_s.clone())
                .map_err(|e| match e {
                    ConfigError::Io(io_err) => {
                        ConfigError::Io(io::Error::new(io_err.kind(), "mocked io error"))
                    }
                    ConfigError::NoProjectDirectory => ConfigError::NoProjectDirectory,
                    ConfigError::Utf8Error(utf8_err) => ConfigError::Utf8Error(
                        String::from_utf8(utf8_err.as_bytes().to_vec()).unwrap_err(),
                    ),
                })
        }
        fn save_last_project_path(
            &self,
            app_name: &str,
            project_path: Option<&Path>,
        ) -> Result<(), ConfigError> {
            self.save_last_project_path_calls
                .fetch_add(1, Ordering::Relaxed);
            *self.saved_project_path.lock().unwrap() =
                Some((app_name.to_string(), project_path.map(|p| p.to_path_buf())));
            Ok(())
        }
    }

    struct MockProfileManager {
        load_profile_results: Mutex<HashMap<String, Result<Profile, ProfileError>>>,
        load_profile_from_path_results: Mutex<HashMap<PathBuf, Result<Profile, ProfileError>>>,
        load_profile_calls: Mutex<Vec<(PathBuf, String)>>,
        save_profile_calls: Mutex<Vec<(PathBuf, Profile, String)>>,
        save_profile_result: Mutex<Result<(), ProfileError>>,
        list_profiles_result: Mutex<Result<Vec<String>, ProfileError>>,
        list_profiles_calls: Mutex<Vec<PathBuf>>,
        get_profile_dir_path_result: Mutex<Option<PathBuf>>,
        profile_dir_path_calls: Mutex<Vec<PathBuf>>,
        load_last_profile_name_for_project_result: Mutex<Result<Option<String>, ProfileError>>,
        save_last_profile_name_for_project_calls: Mutex<Vec<(PathBuf, String)>>,
    }
    impl MockProfileManager {
        fn new() -> Self {
            MockProfileManager {
                load_profile_results: Mutex::new(HashMap::new()),
                load_profile_from_path_results: Mutex::new(HashMap::new()),
                load_profile_calls: Mutex::new(Vec::new()),
                save_profile_calls: Mutex::new(Vec::new()),
                save_profile_result: Mutex::new(Ok(())),
                list_profiles_result: Mutex::new(Ok(Vec::new())),
                list_profiles_calls: Mutex::new(Vec::new()),
                get_profile_dir_path_result: Mutex::new(Some(PathBuf::from("/mock/profiles"))),
                profile_dir_path_calls: Mutex::new(Vec::new()),
                load_last_profile_name_for_project_result: Mutex::new(Ok(None)),
                save_last_profile_name_for_project_calls: Mutex::new(Vec::new()),
            }
        }
        fn set_load_profile_result(
            &self,
            profile_name: &str,
            result: Result<Profile, ProfileError>,
        ) {
            self.load_profile_results
                .lock()
                .unwrap()
                .insert(profile_name.to_string(), result);
        }
        fn set_load_profile_from_path_result(
            &self,
            path: &Path,
            result: Result<Profile, ProfileError>,
        ) {
            self.load_profile_from_path_results
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), result);
        }
        #[allow(dead_code)]
        fn set_save_profile_result(&self, result: Result<(), ProfileError>) {
            *self.save_profile_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn set_list_profiles_result(&self, result: Result<Vec<String>, ProfileError>) {
            *self.list_profiles_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn set_get_profile_dir_path_result(&self, result: Option<PathBuf>) {
            *self.get_profile_dir_path_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn get_save_profile_calls(&self) -> Vec<(PathBuf, Profile, String)> {
            self.save_profile_calls.lock().unwrap().clone()
        }
        #[allow(dead_code)]
        fn get_load_profile_calls(&self) -> Vec<(PathBuf, String)> {
            self.load_profile_calls.lock().unwrap().clone()
        }
        #[allow(dead_code)]
        fn get_list_profiles_calls(&self) -> Vec<PathBuf> {
            self.list_profiles_calls.lock().unwrap().clone()
        }
        #[allow(dead_code)]
        fn get_profile_dir_path_calls(&self) -> Vec<PathBuf> {
            self.profile_dir_path_calls.lock().unwrap().clone()
        }
        fn set_load_last_profile_name_for_project_result(
            &self,
            result: Result<Option<String>, ProfileError>,
        ) {
            *self
                .load_last_profile_name_for_project_result
                .lock()
                .unwrap() = result;
        }
        fn get_save_last_profile_name_for_project_calls(&self) -> Vec<(PathBuf, String)> {
            self.save_last_profile_name_for_project_calls
                .lock()
                .unwrap()
                .clone()
        }
    }
    impl ProfileManagerOperations for MockProfileManager {
        fn load_profile(
            &self,
            project_root: &Path,
            profile_name: &str,
            _app_name: &str,
        ) -> Result<Profile, ProfileError> {
            self.load_profile_calls
                .lock()
                .unwrap()
                .push((project_root.to_path_buf(), profile_name.to_string()));
            let map = self.load_profile_results.lock().unwrap();
            match map.get(profile_name) {
                Some(Ok(profile)) => Ok(profile.clone()),
                Some(Err(e)) => Err(clone_profile_error(e)),
                None => Err(ProfileError::ProfileNotFound(profile_name.to_string())),
            }
        }
        fn load_profile_from_path(&self, path: &Path) -> Result<Profile, ProfileError> {
            let map = self.load_profile_from_path_results.lock().unwrap();
            match map.get(path) {
                Some(Ok(profile)) => Ok(profile.clone()),
                Some(Err(e)) => Err(clone_profile_error(e)),
                None => Err(ProfileError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("MockProfileManager: No result set for path {path:?}"),
                ))),
            }
        }
        fn save_profile(
            &self,
            project_root: &Path,
            profile: &Profile,
            app_name: &str,
        ) -> Result<(), ProfileError> {
            let result_to_return = match *self.save_profile_result.lock().unwrap() {
                Ok(_) => Ok(()),
                Err(ref e) => Err(clone_profile_error(e)),
            };
            if result_to_return.is_ok() {
                self.save_profile_calls.lock().unwrap().push((
                    project_root.to_path_buf(),
                    profile.clone(),
                    app_name.to_string(),
                ));
            }
            result_to_return
        }
        fn list_profiles(
            &self,
            project_root: &Path,
            _app_name: &str,
        ) -> Result<Vec<String>, ProfileError> {
            self.list_profiles_calls
                .lock()
                .unwrap()
                .push(project_root.to_path_buf());
            match *self.list_profiles_result.lock().unwrap() {
                Ok(ref names) => Ok(names.clone()),
                Err(ref e) => Err(clone_profile_error(e)),
            }
        }
        fn get_profile_dir_path(&self, project_root: &Path, _app_name: &str) -> Option<PathBuf> {
            self.profile_dir_path_calls
                .lock()
                .unwrap()
                .push(project_root.to_path_buf());
            self.get_profile_dir_path_result.lock().unwrap().clone()
        }
        fn save_last_profile_name_for_project(
            &self,
            project_root: &Path,
            profile_name: &str,
        ) -> Result<(), ProfileError> {
            self.save_last_profile_name_for_project_calls
                .lock()
                .unwrap()
                .push((project_root.to_path_buf(), profile_name.to_string()));
            Ok(())
        }
        fn load_last_profile_name_for_project(
            &self,
            _project_root: &Path,
        ) -> Result<Option<String>, ProfileError> {
            let guard = self
                .load_last_profile_name_for_project_result
                .lock()
                .unwrap();
            match guard.as_ref() {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(clone_profile_error(e)),
            }
        }
    }
    fn clone_profile_error(error: &ProfileError) -> ProfileError {
        match error {
            ProfileError::Io(e) => ProfileError::Io(io::Error::new(e.kind(), format!("{e}"))),
            ProfileError::Serde(_e) => {
                let representative_json_error = serde_json::from_reader::<_, serde_json::Value>(
                    std::io::Cursor::new(b"invalid json {"),
                )
                .unwrap_err();
                ProfileError::Serde(representative_json_error)
            }
            ProfileError::NoProjectDirectory => ProfileError::NoProjectDirectory,
            ProfileError::ProfileNotFound(s) => ProfileError::ProfileNotFound(s.clone()),
            ProfileError::InvalidProfileName(s) => ProfileError::InvalidProfileName(s.clone()),
        }
    }

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
        fn scan_directory(
            &self,
            root_path: &Path,
            exclude_patterns: &[String],
        ) -> Result<Vec<FileNode>, FileSystemError> {
            let _ = exclude_patterns;
            self.scan_directory_calls
                .lock()
                .unwrap()
                .push(root_path.to_path_buf());
            let map = self.scan_directory_results.lock().unwrap();
            match map.get(root_path) {
                Some(Ok(nodes)) => Ok(nodes.clone()),
                Some(Err(e)) => Err(clone_file_system_error(e)),
                None => Ok(Vec::new()), // Default to empty vec if no result is set for path
            }
        }
    }
    fn clone_file_system_error(error: &FileSystemError) -> FileSystemError {
        match error {
            FileSystemError::Io(e) => FileSystemError::Io(io::Error::new(e.kind(), format!("{e}"))),
            FileSystemError::IgnoreError(original_ignore_error) => {
                let error_message = format!("Mocked IgnoreError: {original_ignore_error:?}");
                let mock_io_err = io::Error::other(error_message);
                FileSystemError::IgnoreError(ignore::Error::from(mock_io_err))
            }
            FileSystemError::InvalidPath(p) => FileSystemError::InvalidPath(p.clone()),
        }
    }

    struct MockArchiver {
        create_archive_content_result: Mutex<io::Result<String>>,
        create_archive_content_calls: Mutex<Vec<(Vec<FileNode>, PathBuf)>>,
        check_archive_status_result: Mutex<ArchiveStatus>,
        check_archive_status_calls: Mutex<Vec<(Option<PathBuf>, Vec<FileNode>)>>,
        save_archive_content_result: Mutex<io::Result<()>>,
        save_archive_content_calls: Mutex<Vec<(PathBuf, String)>>,
        get_file_timestamp_results: Mutex<HashMap<PathBuf, io::Result<SystemTime>>>,
        get_file_timestamp_calls: Mutex<Vec<PathBuf>>,
    }
    impl MockArchiver {
        fn new() -> Self {
            MockArchiver {
                create_archive_content_result: Mutex::new(Ok("mocked_archive_content".to_string())),
                create_archive_content_calls: Mutex::new(Vec::new()),
                check_archive_status_result: Mutex::new(ArchiveStatus::UpToDate),
                check_archive_status_calls: Mutex::new(Vec::new()),
                save_archive_content_result: Mutex::new(Ok(())),
                save_archive_content_calls: Mutex::new(Vec::new()),
                get_file_timestamp_results: Mutex::new(HashMap::new()),
                get_file_timestamp_calls: Mutex::new(Vec::new()),
            }
        }
        fn set_create_archive_content_result(&self, result: io::Result<String>) {
            *self.create_archive_content_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn get_create_archive_content_calls(&self) -> Vec<(Vec<FileNode>, PathBuf)> {
            self.create_archive_content_calls.lock().unwrap().clone()
        }
        fn set_check_archive_status_result(&self, result: ArchiveStatus) {
            *self.check_archive_status_result.lock().unwrap() = result;
        }
        #[allow(dead_code)]
        fn get_check_archive_status_calls(&self) -> Vec<(Option<PathBuf>, Vec<FileNode>)> {
            self.check_archive_status_calls.lock().unwrap().clone()
        }
        fn set_save_archive_content_result(&self, result: io::Result<()>) {
            *self.save_archive_content_result.lock().unwrap() = result;
        }
        fn get_save_archive_content_calls(&self) -> Vec<(PathBuf, String)> {
            self.save_archive_content_calls.lock().unwrap().clone()
        }
        #[allow(dead_code)]
        fn set_get_file_timestamp_result(&self, path: &Path, result: io::Result<SystemTime>) {
            self.get_file_timestamp_results
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), result);
        }
        #[allow(dead_code)]
        fn get_get_file_timestamp_calls(&self) -> Vec<PathBuf> {
            self.get_file_timestamp_calls.lock().unwrap().clone()
        }
    }
    fn clone_io_error(error: &io::Error) -> io::Error {
        io::Error::new(error.kind(), format!("{error}"))
    }
    impl ArchiverOperations for MockArchiver {
        fn create_content(
            &self,
            nodes: &[FileNode],
            root_path_for_display: &Path,
        ) -> io::Result<String> {
            self.create_archive_content_calls
                .lock()
                .unwrap()
                .push((nodes.to_vec(), root_path_for_display.to_path_buf()));
            self.create_archive_content_result
                .lock()
                .unwrap()
                .as_ref()
                .map(|s| s.clone())
                .map_err(clone_io_error)
        }
        fn check_status(
            &self,
            archive_path_opt: Option<&Path>,
            file_nodes_tree: &[FileNode],
        ) -> ArchiveStatus {
            self.check_archive_status_calls.lock().unwrap().push((
                archive_path_opt.map(|p| p.to_path_buf()),
                file_nodes_tree.to_vec(),
            ));
            *self.check_archive_status_result.lock().unwrap()
        }
        fn save(&self, path: &Path, content: &str) -> io::Result<()> {
            self.save_archive_content_calls
                .lock()
                .unwrap()
                .push((path.to_path_buf(), content.to_string()));
            self.save_archive_content_result
                .lock()
                .unwrap()
                .as_ref()
                .map(|_| ())
                .map_err(clone_io_error)
        }
        fn get_file_timestamp(&self, path: &Path) -> io::Result<SystemTime> {
            self.get_file_timestamp_calls
                .lock()
                .unwrap()
                .push(path.to_path_buf());
            let map = self.get_file_timestamp_results.lock().unwrap();
            match map.get(path) {
                Some(Ok(ts)) => Ok(*ts),
                Some(Err(e)) => Err(clone_io_error(e)),
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("MockArchiver: No timestamp result set for path {path:?}"),
                )),
            }
        }
    }

    struct MockStateManager {
        apply_selection_states_to_nodes_calls: Mutex<Vec<SelectionSnapshotLog>>,
        update_folder_selection_calls: Mutex<Vec<(FileNode, SelectionState)>>,
    }
    impl MockStateManager {
        fn new() -> Self {
            MockStateManager {
                apply_selection_states_to_nodes_calls: Mutex::new(Vec::new()),
                update_folder_selection_calls: Mutex::new(Vec::new()),
            }
        }
        #[allow(dead_code)]
        fn get_apply_selection_states_to_nodes_calls(&self) -> Vec<SelectionSnapshotLog> {
            self.apply_selection_states_to_nodes_calls
                .lock()
                .unwrap()
                .clone()
        }
        #[allow(dead_code)]
        fn get_update_folder_selection_calls(&self) -> Vec<(FileNode, SelectionState)> {
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
            self.apply_selection_states_to_nodes_calls
                .lock()
                .unwrap()
                .push((
                    tree.clone(), // Log state before modification by mock
                    selected_paths.clone(),
                    deselected_paths.clone(),
                ));
            // Basic simulation for mock
            for node in tree.iter_mut() {
                if selected_paths.contains(node.path()) {
                    node.set_state(SelectionState::Selected);
                } else if deselected_paths.contains(node.path()) {
                    node.set_state(SelectionState::Deselected);
                } else {
                    node.set_state(SelectionState::New);
                }
                if node.is_dir() && !node.children.is_empty() {
                    self.apply_selection_states_to_nodes(
                        // Recursive call to self is fine for mock
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
                .push((node.clone(), new_state)); // Log state before modification
            node.set_state(new_state);
            if node.is_dir() {
                for child in node.children.iter_mut() {
                    self.update_folder_selection(child, new_state); // Recursive call
                }
            }
        }
    }

    struct MockTokenCounter {
        counts_for_content: Mutex<HashMap<String, usize>>,
        default_count: usize,
        count_tokens_calls: AtomicUsize,
    }
    impl MockTokenCounter {
        fn new(default_count: usize) -> Self {
            MockTokenCounter {
                counts_for_content: Mutex::new(HashMap::new()),
                default_count,
                count_tokens_calls: AtomicUsize::new(0),
            }
        }
    }
    impl TokenCounterOperations for MockTokenCounter {
        fn count_tokens(&self, content: &str) -> usize {
            self.count_tokens_calls.fetch_add(1, Ordering::Relaxed);
            *self
                .counts_for_content
                .lock()
                .unwrap()
                .get(content)
                .unwrap_or(&self.default_count)
        }
    }

    fn setup_logic_with_mocks() -> MockSetupResult {
        crate::initialize_logging();
        let mock_app_session_data_for_test = Arc::new(Mutex::new(MockProfileRuntimeData::new()));
        let mock_config_manager_arc = Arc::new(MockConfigManager::new());
        let mock_profile_manager_arc = Arc::new(MockProfileManager::new());
        let mock_file_system_scanner_arc = Arc::new(MockFileSystemScanner::new());
        let mock_archiver_arc = Arc::new(MockArchiver::new());
        let mock_state_manager_arc = Arc::new(MockStateManager::new());
        let mock_token_counter_arc = Arc::new(MockTokenCounter::new(1));

        let mut logic = MyAppLogic::new(
            Arc::clone(&mock_app_session_data_for_test)
                as Arc<Mutex<dyn ProfileRuntimeDataOperations>>,
            Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>,
            Arc::clone(&mock_profile_manager_arc) as Arc<dyn ProfileManagerOperations>,
            Arc::clone(&mock_file_system_scanner_arc) as Arc<dyn FileSystemScannerOperations>,
            Arc::clone(&mock_archiver_arc) as Arc<dyn ArchiverOperations>,
            Arc::clone(&mock_token_counter_arc) as Arc<dyn TokenCounterOperations>,
            Arc::clone(&mock_state_manager_arc) as Arc<dyn NodeStateApplicatorOperations>,
        );
        logic.test_set_active_project_root(PathBuf::from("/mock/project_root"));
        (
            logic,
            mock_app_session_data_for_test,
            mock_config_manager_arc,
            mock_profile_manager_arc,
            mock_file_system_scanner_arc,
            mock_archiver_arc,
            mock_state_manager_arc,
            mock_token_counter_arc,
        )
    }

    fn find_command<F>(cmds: &[PlatformCommand], mut predicate: F) -> Option<&PlatformCommand>
    where
        F: FnMut(&PlatformCommand) -> bool,
    {
        cmds.iter().find(|&cmd| predicate(cmd))
    }

    // Helper to find multiple commands matching a predicate
    fn find_commands<F>(cmds: &[PlatformCommand], mut predicate: F) -> Vec<&PlatformCommand>
    where
        F: FnMut(&PlatformCommand) -> bool,
    {
        cmds.iter().filter(|&cmd| predicate(cmd)).collect()
    }

    #[test]
    fn test_startup_with_last_project_and_last_profile_loads_correctly() {
        // Arrange
        let (
            mut logic,
            mock_app_session_mutexed,
            mock_config_manager,
            mock_profile_manager,
            mock_file_system_scanner,
            mock_archiver,
            _mock_state_manager,
            _mock_token_counter,
        ) = setup_logic_with_mocks();

        let last_profile_name_to_load = "MyMockedStartupProfile";
        let project_dir = tempdir().expect("Failed to create temp dir for project");
        let startup_project_root = project_dir.path().to_path_buf();
        let startup_archive_path = startup_project_root.join("startup_archive.txt");

        mock_config_manager
            .set_load_last_project_path_result(Ok(Some(startup_project_root.clone())));
        mock_profile_manager.set_load_last_profile_name_for_project_result(Ok(Some(
            last_profile_name_to_load.to_string(),
        )));

        let mut selected_paths_for_profile = HashSet::new();
        let mock_file_path = startup_project_root.join("mock_startup_file.txt");
        selected_paths_for_profile.insert(mock_file_path.clone());

        let mock_loaded_profile_dto = Profile {
            name: last_profile_name_to_load.to_string(),
            root_folder: startup_project_root.clone(),
            selected_paths: selected_paths_for_profile.clone(),
            deselected_paths: HashSet::new(),
            archive_path: Some(startup_archive_path.clone()),
            file_details: HashMap::new(),
            exclude_patterns: Vec::new(),
        };
        mock_profile_manager.set_load_profile_result(
            last_profile_name_to_load,
            Ok(mock_loaded_profile_dto.clone()),
        );

        let scanned_nodes = vec![FileNode::new_test(
            mock_file_path.clone(),
            "mock_startup_file.txt".into(),
            false,
        )];
        mock_file_system_scanner
            .set_scan_directory_result(&startup_project_root, Ok(scanned_nodes.clone()));

        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_load_profile_into_session_result(Ok(()));
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_snapshot_nodes_for_mock(scanned_nodes.clone());
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_cached_total_token_count_for_mock(5);

        mock_archiver.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

        // Act
        let window_id = WindowId::new(1);
        logic.handle_event(AppEvent::MainWindowUISetupComplete { window_id });
        let cmds = logic.test_collect_commands_until_idle();

        // Assert
        {
            let mock_app_session = mock_app_session_mutexed.lock().unwrap();
            assert_eq!(
                mock_app_session.get_load_profile_into_session_log().len(),
                1,
                "Profile should load into session once on startup"
            );
            assert_eq!(
                mock_app_session.profile_name,
                Some(last_profile_name_to_load.to_string())
            );
            assert_eq!(
                mock_app_session.archive_path,
                Some(startup_archive_path.clone())
            );
        }

        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::ShowWindow { .. }
            ))
            .is_some(),
            "Startup should show the main window after activating profile"
        );

        let expected_title = format!(
            "SourcePacker - [{}] - [{}]",
            last_profile_name_to_load,
            startup_archive_path.display()
        );
        assert!(
            find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)).is_some(),
            "Expected window title to reflect loaded profile and archive"
        );
    }

    #[test]
    fn test_startup_with_no_last_project_prompts_for_folder() {
        // Arrange
        let (
            mut logic,
            _,
            _mock_config_manager,
            _mock_profile_manager,
            _mock_fs,
            _mock_archiver,
            _,
            _,
        ) = setup_logic_with_mocks();
        let window_id = WindowId::new(10);
        logic.test_clear_active_project();

        // Act
        logic.handle_event(AppEvent::MainWindowUISetupComplete { window_id });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(
            find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::ShowFolderPickerDialog { window_id: wid, .. } if *wid == window_id)).is_some(),
            "Startup without a project should immediately prompt for folder selection"
        );
        assert!(
            !find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::ShowWindow { .. }
            ))
            .is_some(),
            "Main window should stay hidden until a project and profile are chosen"
        );
    }

    #[test]
    fn test_startup_with_missing_last_project_path_falls_back_to_prompt() {
        // Arrange
        let (
            mut logic,
            _mock_app_session,
            mock_config_manager,
            _mock_profile_manager,
            _mock_file_system_scanner,
            _mock_archiver,
            _mock_state_manager,
            _mock_token_counter,
        ) = setup_logic_with_mocks();
        let window_id = WindowId::new(11);
        let nonexistent_project = PathBuf::from("/definitely/not/a/project/path");
        mock_config_manager
            .set_load_last_project_path_result(Ok(Some(nonexistent_project.clone())));
        logic.test_clear_active_project();

        // Act
        logic.handle_event(AppEvent::MainWindowUISetupComplete { window_id });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(
            find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::ShowFolderPickerDialog { window_id: wid, .. } if *wid == window_id)).is_some(),
            "Missing project path should trigger folder picker"
        );
        assert_eq!(
            mock_config_manager
                .save_last_project_path_calls
                .load(Ordering::Relaxed),
            1,
            "Missing path should be cleared in config"
        );
        let saved = mock_config_manager
            .saved_project_path
            .lock()
            .unwrap()
            .clone();
        assert!(
            matches!(saved, Some((_, None))),
            "Config should clear last project on missing path"
        );
    }

    #[test]
    fn test_menu_set_archive_path_cancelled() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        {
            let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
            mock_app_session.set_profile_name_for_mock(Some("Test".to_string()));
            mock_app_session.set_root_path_for_scan_for_mock(PathBuf::from("."));
            mock_app_session.set_archive_path_for_mock(None); // Ensure no archive path initially for _update_generate_archive_menu_item_state
        }
        logic.test_set_pending_action(PendingAction::SettingArchivePath);

        // Act
        logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: main_window_id,
            result: None, // User cancelled
        });

        // Assert
        assert!(
            logic.test_pending_action().is_none(),
            "Pending action should be cleared on cancel"
        );
        {
            // Check that app_session_data.set_archive_path was NOT called
            let mock_app_session = mock_app_session_mutexed.lock().unwrap();
            assert_eq!(mock_app_session.get_set_archive_path_calls_log().len(), 0);
        }
    }

    #[test]
    fn test_exclude_patterns_dialog_saved_updates_profile_and_refreshes() {
        // Arrange
        let (
            mut logic,
            mock_app_session_mutexed,
            _mock_config_manager,
            mock_profile_manager_arc,
            mock_file_system_scanner_arc,
            _mock_archiver_arc,
            _mock_state_manager,
            _mock_token_counter,
        ) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(7);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        let root_path = PathBuf::from("/mock/project");
        {
            let mut session = mock_app_session_mutexed.lock().unwrap();
            session.set_profile_name_for_mock(Some("DemoProfile".to_string()));
            session.set_root_path_for_scan_for_mock(root_path.clone());
            session.set_exclude_patterns_for_mock(vec!["node_modules/".to_string()]);
        }

        let scan_nodes = vec![FileNode::new_test(
            root_path.join("src"),
            "src".into(),
            true,
        )];
        mock_file_system_scanner_arc.set_scan_directory_result(&root_path, Ok(scan_nodes));

        let new_patterns_input = "target/\r\n*.log\r\n\n#notes\n";
        let expected_patterns = vec![
            "target/".to_string(),
            "*.log".to_string(),
            "#notes".to_string(),
        ];
        let expected_patterns_for_save = expected_patterns.clone();

        // Act
        logic.handle_event(AppEvent::ExcludePatternsDialogCompleted {
            window_id: main_window_id,
            saved: true,
            patterns: new_patterns_input.to_string(),
        });

        // Assert - profile save contains updated patterns
        let saved_profiles = mock_profile_manager_arc.get_save_profile_calls();
        assert_eq!(saved_profiles.len(), 1);
        let (_project_root, saved_profile, app_name) = &saved_profiles[0];
        assert_eq!(app_name, APP_NAME_FOR_PROFILES);
        assert_eq!(saved_profile.exclude_patterns, expected_patterns_for_save);

        // Assert - runtime data updated
        let session_after = mock_app_session_mutexed.lock().unwrap();
        assert_eq!(
            session_after.get_exclude_patterns_for_mock(),
            expected_patterns
        );
        drop(session_after);

        // Assert - refresh triggered a new scan
        let scan_calls = mock_file_system_scanner_arc.get_scan_directory_calls();
        assert_eq!(scan_calls, vec![root_path]);
    }

    #[test]
    fn test_profile_load_updates_archive_status_via_mock_archiver() {
        // Arrange
        let (
            mut logic,
            mock_app_session_mutexed,
            mock_config_manager,
            mock_profile_manager_arc,
            mock_file_system_scanner_arc,
            mock_archiver_arc,
            _mock_state_manager,
            _mock_token_counter,
        ) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        let profile_name = "ProfileForStatusUpdateViaMockArchiver";
        let root_folder_for_profile = PathBuf::from("/mock/scan_root_status_mock_archiver");
        let archive_file_for_profile = PathBuf::from("/mock/my_mock_archiver_archive.txt");
        let profile_json_path_from_dialog =
            PathBuf::from(format!("/dummy/profiles/{profile_name}.json"));

        let mock_profile_to_load_dto = Profile {
            name: profile_name.to_string(),
            root_folder: root_folder_for_profile.clone(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: Some(archive_file_for_profile.clone()),
            file_details: HashMap::new(),
            exclude_patterns: Vec::new(),
        };
        mock_profile_manager_arc.set_load_profile_from_path_result(
            &profile_json_path_from_dialog,
            Ok(mock_profile_to_load_dto.clone()),
        );
        mock_file_system_scanner_arc
            .set_scan_directory_result(&root_folder_for_profile, Ok(vec![])); // Simulate empty scan for simplicity

        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_load_profile_into_session_result(Ok(()));
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_snapshot_nodes_for_mock(vec![]); // Ensure snapshot nodes are empty for this part

        let archive_error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound));
        mock_archiver_arc.set_check_archive_status_result(archive_error_status);

        // Act
        let event = AppEvent::FileOpenProfileDialogCompleted {
            window_id: main_window_id,
            result: Some(profile_json_path_from_dialog.clone()),
        };
        logic.handle_event(event);
        let cmds = logic.test_drain_commands();

        // Assert
        assert_eq!(
            mock_app_session_mutexed
                .lock()
                .unwrap()
                .get_load_profile_into_session_log()
                .len(),
            1
        );
        assert_eq!(
            mock_config_manager
                .save_last_project_path_calls
                .load(Ordering::Relaxed),
            2
        );
        let saved_last_profile_calls =
            mock_profile_manager_arc.get_save_last_profile_name_for_project_calls();
        assert!(
            saved_last_profile_calls.len() >= 1,
            "Persisting last profile should be attempted"
        );
        assert!(saved_last_profile_calls.iter().any(|call| call
            == &(
                PathBuf::from("/mock/project_root"),
                profile_name.to_string()
            )));

        let archiver_calls = mock_archiver_arc.get_check_archive_status_calls();
        assert_eq!(archiver_calls.len(), 1); // Called once during _activate_profile_and_show_window
        assert_eq!(
            archiver_calls[0].0.as_deref(),
            Some(archive_file_for_profile.as_path())
        );
        assert!(archiver_calls[0].1.is_empty()); // Snapshot nodes were empty

        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::ShowWindow { .. }
            ))
            .is_some()
        );
        let archive_status_text_for_dedicated_label = "Archive: Error: NotFound.".to_string();
        let archive_status_text_for_general_status =
            format!("Archive status error: {archive_error_status:?}");

        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &archive_status_text_for_dedicated_label && *severity == MessageSeverity::Error )).is_some(), "Expected dedicated archive label update for error. Got: {cmds:?}" );
        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text == &archive_status_text_for_general_status )).is_some(), "Expected new general label error for archive. Got: {cmds:?}" );
    }

    #[test]
    fn test_menu_action_generate_archive_triggers_logic() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, _, _, _, mock_archiver, _, _) =
            setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        let profile_name = "ArchiveTestProfile";
        let archive_path = PathBuf::from("/test/archive.txt");
        let root_folder = PathBuf::from("/test/root");
        let file_nodes = vec![FileNode::new_test(
            root_folder.join("file.txt"),
            "file.txt".into(),
            false,
        )];
        {
            let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
            mock_app_session.set_profile_name_for_mock(Some(profile_name.to_string()));
            mock_app_session.set_root_path_for_scan_for_mock(root_folder.clone());
            mock_app_session.set_archive_path_for_mock(Some(archive_path.clone()));
            mock_app_session.set_snapshot_nodes_for_mock(file_nodes.clone());
        }
        mock_archiver.set_create_archive_content_result(Ok("Test Archive Content".to_string()));
        mock_archiver.set_save_archive_content_result(Ok(()));
        mock_archiver.set_check_archive_status_result(ArchiveStatus::UpToDate); // After successful save

        // Act
        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_GENERATE_ARCHIVE,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        let create_calls = mock_archiver.get_create_archive_content_calls();
        assert_eq!(create_calls.len(), 1);
        assert_eq!(create_calls[0].0, file_nodes);
        assert_eq!(create_calls[0].1, root_folder);

        let save_calls = mock_archiver.get_save_archive_content_calls();
        assert_eq!(save_calls.len(), 1);
        assert_eq!(save_calls[0].0, archive_path);
        assert_eq!(save_calls[0].1, "Test Archive Content");

        // Check status update after save
        let archiver_status_calls = mock_archiver.get_check_archive_status_calls();
        assert_eq!(
            archiver_status_calls.len(),
            1,
            "Expected check_status to be called after saving archive"
        );

        let success_text = format!("Archive saved to '{}'.", archive_path.display());
        let archive_up_to_date_text = "Archive: Up to date.".to_string();

        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Information && text == &success_text)).is_some(), "Expected general label success message. Got: {cmds:?}");
        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && severity == &MessageSeverity::Information && text == &archive_up_to_date_text)).is_some(), "Expected archive label update to 'Up to date'. Got: {cmds:?}");
    }

    #[test]
    fn test_generate_archive_missing_file_shows_message_box() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, _, _, _, mock_archiver, _, _) =
            setup_logic_with_mocks();
        let main_window_id = WindowId::new(99);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        let profile_name = "PopupProfile";
        let archive_path = PathBuf::from("/popup/archive.txt");
        let root_folder = PathBuf::from("/popup/root");
        let file_nodes = vec![FileNode::new_test(
            root_folder.join("missing.txt"),
            "missing.txt".into(),
            false,
        )];
        {
            let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
            mock_app_session.set_profile_name_for_mock(Some(profile_name.to_string()));
            mock_app_session.set_root_path_for_scan_for_mock(root_folder.clone());
            mock_app_session.set_archive_path_for_mock(Some(archive_path.clone()));
            mock_app_session.set_snapshot_nodes_for_mock(file_nodes.clone());
        }
        mock_archiver.set_create_archive_content_result(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "mocked missing file",
        )));

        // Act
        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_GENERATE_ARCHIVE,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        let message_box_cmd = find_command(&cmds, |cmd| {
            matches!(
                cmd,
                PlatformCommand::ShowMessageBox {
                    window_id,
                    title,
                    message,
                    severity,
                } if *window_id == main_window_id
                    && title == "Missing Source File"
                    && message.contains("mocked missing file")
                    && *severity == MessageSeverity::Error
            )
        });
        assert!(
            message_box_cmd.is_some(),
            "Expected ShowMessageBox command. Got: {cmds:?}"
        );

        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::UpdateLabelText {
                    control_id,
                    ..
                } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID
            ))
            .is_none(),
            "Unexpected status label update when missing file encountered. Got: {cmds:?}"
        );

        assert!(
            mock_archiver.get_save_archive_content_calls().is_empty(),
            "Archive save should not be attempted when content generation fails"
        );
    }

    #[test]
    fn test_menu_action_generate_archive_no_profile_shows_error() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_profile_name_for_mock(None);

        // Act
        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_GENERATE_ARCHIVE,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No profile loaded"))).is_some(), "Expected 'No profile loaded' error status. Got: {cmds:?}");
    }

    #[test]
    fn test_menu_action_generate_archive_no_archive_path_shows_error() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);
        {
            let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
            mock_app_session.set_profile_name_for_mock(Some("NoArchivePathProfile".to_string()));
            mock_app_session.set_archive_path_for_mock(None);
        }

        // Act
        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_GENERATE_ARCHIVE,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, severity, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No archive path set"))).is_some(), "Expected 'No archive path set' error status. Got: {cmds:?}");
    }

    #[test]
    fn test_profile_selection_cancel_with_active_profile_keeps_application_running() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        {
            let mut data = mock_app_session_mutexed.lock().unwrap();
            data.set_profile_name_for_mock(Some("ExistingProfile".to_string()));
        }

        // Act
        logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
            window_id,
            chosen_profile_name: None,
            create_new_requested: false,
            user_cancelled: true,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(
            cmds.iter()
                .all(|cmd| !matches!(cmd, PlatformCommand::QuitApplication)),
            "Cancelling profile selection with an active profile should not request quit."
        );
    }

    #[test]
    fn test_profile_selection_cancel_without_active_profile_requests_quit() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        {
            let mut data = mock_app_session_mutexed.lock().unwrap();
            data.set_profile_name_for_mock(None);
        }

        // Act
        logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
            window_id,
            chosen_profile_name: None,
            create_new_requested: false,
            user_cancelled: true,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(
            cmds.iter()
                .any(|cmd| matches!(cmd, PlatformCommand::QuitApplication)),
            "Cancelling profile selection with no active profile should request quit."
        );
    }

    #[test]
    fn test_update_current_archive_status_routes_to_dedicated_label() {
        // Arrange
        let (
            mut logic,
            mock_app_session_mutexed,
            _mock_config_manager,
            _mock_profile_manager,
            _mock_file_system_scanner,
            mock_archiver,
            _mock_state_manager,
            _mock_token_counter,
        ) = setup_logic_with_mocks();
        let main_window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(main_window_id);

        {
            let mut mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
            mock_app_session_guard.set_profile_name_for_mock(Some("TestProfile".to_string()));
            mock_app_session_guard.set_root_path_for_scan_for_mock(PathBuf::from("/root"));
            mock_app_session_guard
                .set_archive_path_for_mock(Some(PathBuf::from("/root/archive.txt")));
            mock_app_session_guard.set_snapshot_nodes_for_mock(vec![]);
        }

        // Case 1: ArchiveStatus is an error
        let error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::PermissionDenied));
        let expected_dedicated_error_text = "Archive: Error: PermissionDenied.".to_string();
        mock_archiver.set_check_archive_status_result(error_status);

        // Act 1
        logic.update_current_archive_status();
        let cmds_error = logic.test_drain_commands();

        // Assert 1
        assert!(find_command(&cmds_error, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, control_id, text, severity } if *window_id == main_window_id && *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &expected_dedicated_error_text && *severity == MessageSeverity::Error )).is_some(), "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (Error). Got: {cmds_error:?}" );
        let general_error_text = format!("Archive status error: {error_status:?}");
        assert!(find_command(&cmds_error, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, control_id, text, severity } if *window_id == main_window_id && *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == &general_error_text && *severity == MessageSeverity::Error )).is_some(), "Expected general status update for archive error. Got: {cmds_error:?}" );
        {
            let mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
            assert!(
                mock_app_session_guard
                    .get_profile_name_calls
                    .load(Ordering::Relaxed)
                    >= 1,
                "Case 1: get_profile_name_calls"
            );
            assert!(
                mock_app_session_guard
                    .get_archive_path_calls
                    .load(Ordering::Relaxed)
                    >= 1,
                "Case 1: get_archive_path_calls"
            );
            assert!(
                mock_app_session_guard
                    .get_snapshot_nodes_calls
                    .load(Ordering::Relaxed)
                    >= 1,
                "Case 1: get_snapshot_nodes_calls"
            );
        }

        // Reset call counts for next part of test
        {
            let mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
            mock_app_session_guard
                .get_profile_name_calls
                .store(0, Ordering::Relaxed);
            mock_app_session_guard
                .get_archive_path_calls
                .store(0, Ordering::Relaxed);
            mock_app_session_guard
                .get_snapshot_nodes_calls
                .store(0, Ordering::Relaxed);
        }

        // Case 2: ArchiveStatus is informational (e.g., UpToDate)
        let info_status = ArchiveStatus::UpToDate;
        let expected_dedicated_info_text = "Archive: Up to date.".to_string();
        mock_archiver.set_check_archive_status_result(info_status);

        // Act 2
        logic.update_current_archive_status();
        let cmds_info = logic.test_drain_commands();

        // Assert 2
        assert!(find_command(&cmds_info, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, control_id, text, severity } if *window_id == main_window_id && *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &expected_dedicated_info_text && *severity == MessageSeverity::Information )) .is_some(), "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (Information). Got: {cmds_info:?}");
        // General status is NOT updated for non-error archive status updates beyond the initial log
        let general_info_cmds = find_commands(
            &cmds_info,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID ),
        );
        assert_eq!(
            general_info_cmds.len(),
            0,
            "General status label should not be updated for informational archive status if no error. Got: {cmds_info:?}"
        );

        {
            let mut mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
            assert!(
                mock_app_session_guard
                    .get_profile_name_calls
                    .load(Ordering::Relaxed)
                    > 0,
                "Case 2: get_profile_name_calls should be > 0"
            );
            mock_app_session_guard
                .get_profile_name_calls
                .store(0, Ordering::Relaxed); // Reset for Case 3
            mock_app_session_guard.set_profile_name_for_mock(None); // This is for the logic of Case 3
        }

        // Case 3: No profile loaded
        // Act 3
        logic.update_current_archive_status();
        let cmds_no_profile = logic.test_drain_commands();

        // Assert 3
        let no_profile_archive_text = "Archive: No profile loaded".to_string();
        let no_profile_general_text = "No profile loaded".to_string();
        assert!(find_command(&cmds_no_profile, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &no_profile_archive_text)).is_some(), "Expected archive label for 'No profile loaded'. Got: {cmds_no_profile:?}");
        assert!(find_command(&cmds_no_profile, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == &no_profile_general_text)).is_some(), "Expected general status for 'No profile loaded'. Got: {cmds_no_profile:?}");

        {
            let mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
            assert!(
                mock_app_session_guard
                    .get_profile_name_calls
                    .load(Ordering::Relaxed)
                    > 0,
                "Case 3: get_profile_name_calls should be > 0"
            );
        }
    }

    #[test]
    fn test_is_tree_item_new_for_file_and_folder() {
        // Arrange
        let (logic, mock_app_session_mutexed, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        let mut mutable_logic = logic; // Shadow immutable logic
        mutable_logic.test_set_main_window_id_and_init_ui_state(window_id);

        let root = PathBuf::from("/root");
        let file_new_path = root.join("new_file.txt");
        let file_sel_path = root.join("sel_file.txt");
        let folder_with_new_path = root.join("folder_new_child");
        let file_in_folder_new_path = folder_with_new_path.join("inner_new.txt");
        let folder_no_new_path = root.join("folder_no_new");
        let file_in_folder_no_new_path = folder_no_new_path.join("inner_sel.txt");

        // Populate mock session data
        {
            let mut app_data = mock_app_session_mutexed.lock().unwrap();
            app_data.set_snapshot_nodes_for_mock(vec![
                FileNode::new_full(
                    file_new_path.clone(),
                    "new_file.txt".into(),
                    false,
                    SelectionState::New,
                    vec![],
                    "".to_string(),
                ),
                FileNode::new_full(
                    file_sel_path.clone(),
                    "sel_file.txt".into(),
                    false,
                    SelectionState::Selected,
                    vec![],
                    "".to_string(),
                ),
                FileNode::new_full(
                    folder_with_new_path.clone(),
                    "folder_new_child".into(),
                    true,
                    SelectionState::Selected, // Folder itself might be selected
                    vec![FileNode::new_full(
                        file_in_folder_new_path.clone(),
                        "inner_new.txt".into(),
                        false,
                        SelectionState::New,
                        vec![],
                        "".to_string(),
                    )],
                    "".to_string(),
                ),
                FileNode::new_full(
                    folder_no_new_path.clone(),
                    "folder_no_new".into(),
                    true,
                    SelectionState::Selected,
                    vec![FileNode::new_full(
                        file_in_folder_no_new_path.clone(),
                        "inner_sel.txt".into(),
                        false,
                        SelectionState::Selected,
                        vec![],
                        "".to_string(),
                    )],
                    "".to_string(),
                ),
            ]);
            // Mock results for does_path_or_descendants_contain_new_file are derived from snapshot nodes by mock
        }

        let item_id_file_new = TreeItemId(1);
        let item_id_file_sel = TreeItemId(2);
        let item_id_folder_new = TreeItemId(3); // folder_with_new_path
        let item_id_folder_no_new = TreeItemId(5); // folder_no_new_path

        mutable_logic
            .test_set_path_to_tree_item_id_mapping(file_new_path.clone(), item_id_file_new);
        mutable_logic
            .test_set_path_to_tree_item_id_mapping(file_sel_path.clone(), item_id_file_sel);
        mutable_logic.test_set_path_to_tree_item_id_mapping(
            folder_with_new_path.clone(),
            item_id_folder_new,
        );
        mutable_logic.test_set_path_to_tree_item_id_mapping(
            folder_no_new_path.clone(),
            item_id_folder_no_new,
        );

        // Act & Assert
        assert!(
            mutable_logic.is_tree_item_new(window_id, item_id_file_new),
            "New file should be new"
        );
        assert!(
            !mutable_logic.is_tree_item_new(window_id, item_id_file_sel),
            "Selected file should not be new"
        );
        assert!(
            mutable_logic.is_tree_item_new(window_id, item_id_folder_new),
            "Folder with new child should be new"
        );
        assert!(
            !mutable_logic.is_tree_item_new(window_id, item_id_folder_no_new),
            "Folder with no new child (only selected) should not be new"
        );

        let app_data_guard = mock_app_session_mutexed.lock().unwrap();
        assert!(
            app_data_guard
                .get_node_attributes_for_path_calls
                .load(Ordering::Relaxed)
                >= 4
        ); // Called for each item
        assert!(
            app_data_guard
                .does_path_or_descendants_contain_new_file_calls
                .load(Ordering::Relaxed)
                >= 2
        ); // Called for folders
    }

    #[test]
    fn test_treeview_item_toggled_queues_redraw_for_item_and_parents_on_new_status_change() {
        // Arrange
        let (mut logic, mock_app_session_mutexed, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let root = PathBuf::from("/scan_root");
        let dir1_path = root.join("dir1");
        let file_in_dir1_path = dir1_path.join("file_in_dir1.txt");

        {
            let mut app_data = mock_app_session_mutexed.lock().unwrap();
            app_data.set_root_path_for_scan_for_mock(root.clone());
            app_data.set_snapshot_nodes_for_mock(vec![FileNode::new_full(
                dir1_path.clone(),
                "dir1".into(),
                true,
                SelectionState::New, // Initially folder might be New due to child
                vec![FileNode::new_full(
                    file_in_dir1_path.clone(),
                    "file_in_dir1.txt".into(),
                    false,
                    SelectionState::New, // This file is New
                    vec![],
                    "".to_string(),
                )],
                "".to_string(),
            )]);
        }

        let file_item_id = TreeItemId(10);
        let dir1_item_id = TreeItemId(11);

        logic.test_set_path_to_tree_item_id_mapping(file_in_dir1_path.clone(), file_item_id);
        logic.test_set_path_to_tree_item_id_mapping(dir1_path.clone(), dir1_item_id);

        // Act: Toggle the new file to Selected (no longer "New" for display)
        logic.handle_event(AppEvent::TreeViewItemToggledByUser {
            window_id,
            item_id: file_item_id,
            new_state: CheckState::Checked, // Becomes Selected
        });
        let cmds = logic.test_drain_commands();

        // Assert
        // Check for RedrawTreeItem for the file itself and its parent
        let mut redraw_file_found_count = 0;
        let mut redraw_dir_found_count = 0;
        let mut file_text_updates: Vec<String> = Vec::new();
        let mut dir_text_updates: Vec<String> = Vec::new();
        for cmd in &cmds {
            if let PlatformCommand::RedrawTreeItem {
                item_id: cmd_item_id,
                control_id,
                ..
            } = cmd
            {
                assert_eq!(
                    *control_id,
                    ui_constants::ID_TREEVIEW_CTRL,
                    "Redraw command should target the correct TreeView"
                );
                if *cmd_item_id == file_item_id {
                    redraw_file_found_count += 1;
                }
                if *cmd_item_id == dir1_item_id {
                    redraw_dir_found_count += 1;
                }
            }
            if let PlatformCommand::UpdateTreeItemText {
                item_id: cmd_item_id,
                control_id,
                text,
                ..
            } = cmd
            {
                assert_eq!(
                    *control_id,
                    ui_constants::ID_TREEVIEW_CTRL,
                    "UpdateTreeItemText should target the correct TreeView"
                );
                if *cmd_item_id == file_item_id {
                    file_text_updates.push(text.clone());
                }
                if *cmd_item_id == dir1_item_id {
                    dir_text_updates.push(text.clone());
                }
            }
        }
        // The file itself might get two RedrawTreeItem: one from the direct toggle effect,
        // and one from the "was_considered_new_for_display" logic.
        assert!(
            redraw_file_found_count >= 1,
            "Expected at least one RedrawTreeItem for the toggled file. Got: {cmds:?}, count: {redraw_file_found_count}"
        );
        assert!(
            redraw_dir_found_count >= 1,
            "Expected at least one RedrawTreeItem for the parent directory. Got: {cmds:?}, count: {redraw_dir_found_count}"
        );
        let indicator_suffix = format!(" {}", ui_constants::NEW_ITEM_INDICATOR_CHAR);
        assert!(
            file_text_updates
                .iter()
                .any(|t| !t.contains(&indicator_suffix)),
            "Expected updated text for the file without the indicator. Updates: {file_text_updates:?}"
        );
        assert!(
            dir_text_updates
                .iter()
                .any(|t| !t.contains(&indicator_suffix)),
            "Expected updated text for the parent directory without the indicator. Updates: {dir_text_updates:?}"
        );

        // Verify state change in mock data
        let app_data_final = mock_app_session_mutexed.lock().unwrap();
        let (file_state, _) = app_data_final
            .get_node_attributes_for_path(&file_in_dir1_path)
            .unwrap();
        assert_eq!(
            file_state,
            SelectionState::Selected,
            "File state in mock data should be Selected"
        );
    }

    #[test]
    fn test_treeview_item_selection_changed_updates_state_and_issues_command() {
        // Arrange
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let tree_item_id = TreeItemId(99);
        let path = PathBuf::from("/root/demo.txt");
        logic.test_set_path_to_tree_item_id_mapping(path.clone(), tree_item_id);
        {
            let mut app_data = mock_app_session.lock().unwrap();
            app_data.set_snapshot_nodes_for_mock(vec![FileNode::new_full(
                path,
                "demo.txt".into(),
                false,
                SelectionState::Selected,
                Vec::new(),
                "".to_string(),
            )]);
        }

        // Act
        logic.handle_event(AppEvent::TreeViewItemSelectionChanged {
            window_id,
            item_id: tree_item_id,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert_eq!(logic.test_get_active_viewer_item_id(), Some(tree_item_id));
        let selection_cmd = find_command(&cmds, |cmd| {
            matches!(
                cmd,
                PlatformCommand::SetTreeViewSelection {
                    window_id: wid,
                    control_id,
                    item_id
                } if *wid == window_id
                    && *control_id == ui_constants::ID_TREEVIEW_CTRL
                    && *item_id == tree_item_id
            )
        });
        assert!(
            selection_cmd.is_some(),
            "Expected SetTreeViewSelection command after selection change"
        );
    }

    #[test]
    fn test_select_text_file_loads_content_into_viewer() {
        // Arrange
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let temp_dir = tempdir().expect("temp dir creation");
        let file_path = temp_dir.path().join("preview.txt");
        let lf_only_content = "Preview\nContent\n";
        let expected_content = "Preview\r\nContent\r\n";
        fs::write(&file_path, lf_only_content).expect("write preview file");

        let tree_item_id = TreeItemId(101);
        logic.test_set_path_to_tree_item_id_mapping(file_path.clone(), tree_item_id);

        {
            let mut app_data = mock_app_session.lock().unwrap();
            app_data.set_snapshot_nodes_for_mock(vec![FileNode::new_full(
                file_path.clone(),
                "preview.txt".into(),
                false,
                SelectionState::Selected,
                Vec::new(),
                "".to_string(),
            )]);
        }

        // Act
        logic.handle_event(AppEvent::TreeViewItemSelectionChanged {
            window_id,
            item_id: tree_item_id,
        });
        let cmds = logic.test_drain_commands();

        // Assert
        let viewer_cmd = find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::SetViewerContent { .. })
        });
        assert!(viewer_cmd.is_some(), "Expected SetViewerContent command");

        if let Some(PlatformCommand::SetViewerContent {
            window_id: wid,
            control_id,
            text,
        }) = viewer_cmd
        {
            assert_eq!(*wid, window_id);
            assert_eq!(*control_id, ui_constants::ID_VIEWER_EDIT_CTRL);
            assert_eq!(text, expected_content);
        }
    }

    // --- Tests for newly exposed private functions ---

    #[test]
    fn test_internal_refresh_tree_view_from_cache() {
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let mut dir = FileNode::new_test(PathBuf::from("/root/dir1"), "dir1".into(), true);
        dir.children = vec![FileNode::new_test(
            PathBuf::from("/root/dir1/file2.txt"),
            "file2.txt".into(),
            false,
        )];

        let nodes = vec![
            FileNode::new_test(PathBuf::from("/root/file1.txt"), "file1.txt".into(), false),
            dir,
        ];
        mock_app_session
            .lock()
            .unwrap()
            .set_snapshot_nodes_for_mock(nodes.clone());

        logic.test_refresh_tree_view_from_cache(window_id);
        let cmds = logic.test_drain_commands();

        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            PlatformCommand::PopulateTreeView {
                window_id: cmd_win_id,
                control_id,
                items,
            } => {
                assert_eq!(*cmd_win_id, window_id);
                assert_eq!(*control_id, ui_constants::ID_TREEVIEW_CTRL);
                assert_eq!(items.len(), 2);
                let indicator_suffix = format!(" {}", ui_constants::NEW_ITEM_INDICATOR_CHAR);
                assert_eq!(items[0].text, format!("file1.txt{indicator_suffix}"));
                assert_eq!(items[1].text, format!("dir1{indicator_suffix}"));
                assert_eq!(items[1].children.len(), 1);
                assert_eq!(
                    items[1].children[0].text,
                    format!("file2.txt{indicator_suffix}")
                );
            }
            _ => panic!("Expected PopulateTreeView command, got {:?}", cmds[0]),
        }
        let id_map = logic.test_get_path_to_tree_item_id().unwrap();
        // Check that ui_state's path_to_tree_item_id map is populated
        assert!(id_map.contains_key(&PathBuf::from("/root/file1.txt")));
        assert!(id_map.contains_key(&PathBuf::from("/root/dir1")));
        assert!(id_map.contains_key(&PathBuf::from("/root/dir1/file2.txt")));
        assert_eq!(logic.test_get_next_tree_item_id_counter(), Some(4)); // 1 (file1) + 1 (dir1) + 1 (file2) + 1 (for next)
    }

    #[test]
    fn test_internal_update_token_count_and_request_display() {
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        mock_app_session
            .lock()
            .unwrap()
            .set_cached_total_token_count_for_mock(123);

        logic.test_update_token_count_and_request_display();

        let cmds = logic.test_collect_commands_until_idle();
        let mock = mock_app_session.lock().unwrap();
        assert_eq!(
            mock.recalc_tokens_async_calls.load(Ordering::Relaxed),
            1,
            "Async recalculation should be triggered once",
        );
        assert_eq!(
            mock._update_total_token_count_calls.load(Ordering::Relaxed),
            0,
            "Legacy synchronous path should no longer be used",
        );
        drop(mock);

        let status_texts: Vec<&String> = cmds
            .iter()
            .filter_map(|cmd| match cmd {
                PlatformCommand::UpdateLabelText {
                    control_id, text, ..
                } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            status_texts
                .iter()
                .any(|text| text.contains("Token recalculation finished")),
            "Expected final status message after async token recalculation",
        );

        let token_label_texts: Vec<&String> = cmds
            .iter()
            .filter_map(|cmd| match cmd {
                PlatformCommand::UpdateLabelText {
                    control_id, text, ..
                } if *control_id == ui_constants::STATUS_LABEL_TOKENS_ID => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            token_label_texts
                .iter()
                .any(|text| text.as_str() == "Tokens: 123"),
            "Expected final token label to show total tokens",
        );
    }

    #[test]
    fn test_internal_handle_file_save_dialog_for_setting_archive_path() {
        let (mut logic, mock_app_session, _cfg_mgr, profile_mgr, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        let profile_name = "TestProfile";
        let new_archive_path = PathBuf::from("/new/archive.zip");

        mock_app_session
            .lock()
            .unwrap()
            .set_profile_name_for_mock(Some(profile_name.to_string()));
        mock_app_session
            .lock()
            .unwrap()
            .set_archive_path_for_mock(None); // Start with no archive path

        // Case 1: User selects a path
        logic.test_handle_file_save_dialog_for_setting_archive_path(
            window_id,
            Some(new_archive_path.clone()),
        );
        let cmds = logic.test_drain_commands();

        assert_eq!(
            mock_app_session
                .lock()
                .unwrap()
                .get_set_archive_path_calls_log()
                .last()
                .unwrap()
                .as_ref(),
            Some(&new_archive_path)
        );
        assert_eq!(profile_mgr.get_save_profile_calls().len(), 1);
        assert_eq!(profile_mgr.get_save_profile_calls()[0].1.name, profile_name);
        assert_eq!(
            profile_mgr.get_save_profile_calls()[0].1.archive_path,
            Some(new_archive_path.clone())
        );

        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::SetWindowTitle { .. }
            ))
            .is_some()
        );
        assert!(find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { control_id, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID)
        })
        .is_some());

        // Case 2: User cancels
        mock_app_session
            .lock()
            .unwrap()
            .set_archive_path_for_mock(None); // Reset for this part
        logic.test_handle_file_save_dialog_for_setting_archive_path(window_id, None);
        assert_eq!(profile_mgr.get_save_profile_calls().len(), 1); // Should not have increased
    }

    #[test]
    fn test_internal_make_profile_name() {
        assert_eq!(
            MyAppLogic::test_make_profile_name(Some(PathBuf::from("/path/to/My Profile.json"))),
            Ok("My Profile".to_string())
        );
        assert_eq!(
            MyAppLogic::test_make_profile_name(Some(PathBuf::from("MyProfile"))),
            Ok("MyProfile".to_string())
        );
        assert!(MyAppLogic::test_make_profile_name(Some(PathBuf::from("/path/to/.json"))).is_err()); // Empty stem
        assert!(
            MyAppLogic::test_make_profile_name(Some(PathBuf::from("/path/to/Invalid*Name.json")))
                .is_err()
        );
        assert!(MyAppLogic::test_make_profile_name(None).is_err());
    }

    #[test]
    fn test_profile_ops_no_project_root_are_noop() {
        let (mut logic, _session, _cfg_mgr, profile_mgr, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(41);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        logic.test_clear_active_project();

        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_LOAD_PROFILE,
        });

        assert!(profile_mgr.get_list_profiles_calls().is_empty());
        let cmds = logic.test_drain_commands();
        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::UpdateLabelText { control_id, text, .. }
                    if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID
                        && text.contains("No project folder is open")
            ))
            .is_some()
        );
    }

    #[test]
    fn test_profile_ops_with_project_root_use_project_local_dir() {
        let (mut logic, _session, _cfg_mgr, profile_mgr, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(42);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        logic.handle_event(AppEvent::MenuActionClicked {
            action_id: ui_constants::MENU_ACTION_SAVE_PROFILE_AS,
        });

        let dir_calls = profile_mgr.get_profile_dir_path_calls();
        assert_eq!(
            dir_calls,
            vec![PathBuf::from("/mock/project_root")],
            "Expected Save Profile As to query the current project's profile dir."
        );
    }

    #[test]
    fn test_internal_handle_file_save_dialog_for_saving_profile_as() {
        let (mut logic, mock_app_session, cfg_mgr, profile_mgr, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        mock_app_session
            .lock()
            .unwrap()
            .set_profile_name_for_mock(Some("OldProfile".to_string()));

        let new_profile_path = PathBuf::from("/profiles/New Profile Name.json");

        // Case 1: Valid new name
        logic.test_handle_file_save_dialog_for_saving_profile_as(
            window_id,
            Some(new_profile_path.clone()),
        );
        let cmds = logic.test_drain_commands();

        let app_session_guard = mock_app_session.lock().unwrap();
        assert_eq!(
            app_session_guard
                .get_set_profile_name_calls_log()
                .last()
                .unwrap()
                .as_ref(),
            Some(&"New Profile Name".to_string())
        );
        assert!(
            app_session_guard
                .get_set_archive_path_calls_log()
                .last()
                .unwrap()
                .is_none(),
            "Archive path should be cleared on save as"
        );
        drop(app_session_guard);

        assert_eq!(profile_mgr.get_save_profile_calls().len(), 1);
        assert_eq!(
            profile_mgr.get_save_profile_calls()[0].1.name,
            "New Profile Name"
        );
        assert_eq!(
            cfg_mgr.save_last_project_path_calls.load(Ordering::Relaxed),
            1,
            "Saving a profile should persist the current project path"
        );
        let last_profile_calls = profile_mgr.get_save_last_profile_name_for_project_calls();
        assert_eq!(last_profile_calls.len(), 1);
        assert_eq!(
            last_profile_calls[0],
            (
                PathBuf::from("/mock/project_root"),
                "New Profile Name".to_string()
            )
        );

        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::SetWindowTitle { .. }
            ))
            .is_some()
        );
        assert!(find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { control_id, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID)
        })
        .is_some()); // Status update

        // Case 2: Invalid name from path
        logic.test_handle_file_save_dialog_for_saving_profile_as(
            window_id,
            Some(PathBuf::from("/profiles/Invalid*.json")),
        );
        let cmds_invalid = logic.test_drain_commands();
        assert!(find_command(&cmds_invalid, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { severity, text, .. } if *severity == MessageSeverity::Error && text.contains("Invalid profile name"))).is_some());
    }

    #[test]
    fn test_internal_activate_profile_and_show_window() {
        let (mut logic, mock_app_session, _cfg_mgr, _profile_mgr, fs_scanner, ..) =
            setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let mut profile = Profile::new(
            "ActivatedProfile".to_string(),
            PathBuf::from("/root/active"),
        );
        let selected_file_path = profile.root_folder.join("file.txt");
        profile.selected_paths.insert(selected_file_path.clone());
        fs_scanner.set_scan_directory_result(
            &profile.root_folder,
            Ok(vec![FileNode::new_test(
                selected_file_path.clone(),
                "file.txt".into(),
                false,
            )]),
        );
        mock_app_session
            .lock()
            .unwrap()
            .set_load_profile_into_session_result(Ok(())); // Scan success
        mock_app_session
            .lock()
            .unwrap()
            .set_cached_total_token_count_for_mock(10);

        logic.test_activate_profile_and_show_window(
            window_id,
            profile.clone(),
            "Profile loaded".to_string(),
        );

        let mut cmds = Vec::new();
        while let Some(cmd) = logic.try_dequeue_command() {
            cmds.push(cmd);
        }
        cmds.extend(logic.test_drain_commands());

        let mock = mock_app_session.lock().unwrap();
        assert_eq!(
            mock.recalc_tokens_async_calls.load(Ordering::Relaxed),
            1,
            "Async recalculation should run once during activation",
        );
        assert_eq!(mock.get_load_profile_into_session_log().len(), 1);
        drop(mock);

        assert!(find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title.contains("ActivatedProfile"))
        })
        .is_some());
        assert!(
            find_command(&cmds, |cmd| {
                matches!(cmd, PlatformCommand::PopulateTreeView { items, .. } if !items.is_empty() )
            })
            .is_some()
        );
        assert!(find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { control_id, .. } if *control_id == ui_constants::STATUS_LABEL_ARCHIVE_ID)
        })
        .is_some());

        let token_label_texts: Vec<&String> = cmds
            .iter()
            .filter_map(|cmd| match cmd {
                PlatformCommand::UpdateLabelText {
                    control_id, text, ..
                } if *control_id == ui_constants::STATUS_LABEL_TOKENS_ID => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            !token_label_texts.is_empty(),
            "Expected token label updates during async recalculation",
        );
        assert!(find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { control_id, text, .. } if *control_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Profile loaded")
        })
        .is_some());
        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::ShowWindow { .. }
            ))
            .is_some()
        );
    }

    #[test]
    fn test_internal_handle_input_dialog_for_new_profile_name() {
        let (mut logic, _mock_app_session, _cfg_mgr, profile_mgr, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        profile_mgr.set_list_profiles_result(Ok(vec![])); // Ensure profile selection is triggered if needed

        // Case 1: User cancels (None input)
        logic.test_handle_input_dialog_for_new_profile_name(window_id, None);
        let cmds_cancel = logic.test_drain_commands();
        assert!(
            find_command(&cmds_cancel, |cmd| matches!(
                cmd,
                PlatformCommand::ShowProfileSelectionDialog { .. }
            ))
            .is_some()
        );
        assert!(logic.test_get_pending_new_profile_name().is_none());

        // Case 2: Invalid name
        logic.test_handle_input_dialog_for_new_profile_name(
            window_id,
            Some("Invalid*Name".to_string()),
        );
        let cmds_invalid = logic.test_drain_commands();
        assert!(find_command(&cmds_invalid, |cmd| {
            matches!(cmd, PlatformCommand::ShowInputDialog { title, default_text, .. } if title.contains("Name") && default_text.as_deref() == Some("Invalid*Name"))
        })
        .is_some());
        assert!(find_command(&cmds_invalid, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { severity, .. } if *severity == MessageSeverity::Warning )
        })
        .is_some()); // Warning status
        assert!(logic.test_get_pending_new_profile_name().is_none()); // Should not be set yet

        // Case 3: Valid name
        logic.test_handle_input_dialog_for_new_profile_name(
            window_id,
            Some("Valid Profile".to_string()),
        );
        let cmds_valid = logic.test_drain_commands();
        assert!(
            find_command(&cmds_valid, |cmd| matches!(
                cmd,
                PlatformCommand::ShowFolderPickerDialog { .. }
            ))
            .is_some()
        );
        assert_eq!(
            logic.test_get_pending_new_profile_name(),
            Some("Valid Profile".to_string())
        );
        assert_eq!(
            logic.test_pending_action(),
            Some(&PendingAction::CreatingNewProfileGetRoot)
        );
    }

    #[test]
    fn test_internal_update_window_title_with_profile_and_archive() {
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        // Case 1: No profile
        mock_app_session
            .lock()
            .unwrap()
            .set_profile_name_for_mock(None);
        logic.test_update_window_title_with_profile_and_archive(window_id);
        let cmds1 = logic.test_drain_commands();
        assert!(find_command(&cmds1, |cmd| {
            matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == "SourcePacker - [No Profile Loaded]")
        })
        .is_some());

        // Case 2: Profile, no archive
        mock_app_session
            .lock()
            .unwrap()
            .set_profile_name_for_mock(Some("MyProfile".to_string()));
        mock_app_session
            .lock()
            .unwrap()
            .set_archive_path_for_mock(None);
        logic.test_update_window_title_with_profile_and_archive(window_id);
        let cmds2 = logic.test_drain_commands();
        assert!(find_command(&cmds2, |cmd| {
            matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == "SourcePacker - [MyProfile] - [No Archive Set]")
        })
        .is_some());

        // Case 3: Profile and archive
        mock_app_session
            .lock()
            .unwrap()
            .set_archive_path_for_mock(Some(PathBuf::from("/path/archive.txt")));
        logic.test_update_window_title_with_profile_and_archive(window_id);
        let cmds3 = logic.test_drain_commands();
        assert!(find_command(&cmds3, |cmd| {
            matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == "SourcePacker - [MyProfile] - [/path/archive.txt]")
        })
        .is_some());
    }

    #[test]
    fn test_handle_filter_text_submitted_updates_ui_state() {
        // Arrange
        let (mut logic, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let filter_text_to_submit = "find_me";

        // Act 1: Submit non-empty filter text. Since mock data is empty, this will result in no matches.
        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: filter_text_to_submit.to_string(),
        });
        let cmds_after_submit = logic.test_drain_commands();

        // Assert 1
        assert_eq!(
            logic.test_get_filter_text(),
            Some(filter_text_to_submit.to_string()),
            "Filter text in UI state should be set to the submitted text."
        );
        assert!(
            find_command(&cmds_after_submit, |cmd| matches!(
                cmd,
                PlatformCommand::PopulateTreeView { .. }
            ))
            .is_some(),
            "PopulateTreeView command should be enqueued after submitting text."
        );
        // Because the mock has no nodes, the filter will find no matches, and the style should be DefaultInputError.
        assert!(
            find_command(&cmds_after_submit, |cmd| matches!(cmd,
                PlatformCommand::ApplyStyleToControl { window_id: wid, control_id, style_id }
                    if *wid == window_id && *control_id == ui_constants::FILTER_INPUT_ID && *style_id == StyleId::DefaultInputError
            ))
            .is_some(),
            "Expected error style change for an active filter with no matches."
        );
        assert!(
            find_command(&cmds_after_submit, |cmd| matches!(cmd,
                PlatformCommand::ExpandAllTreeItems { window_id: wid, control_id }
                    if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
            ))
            .is_some(),
            "Expected ExpandAllTreeItems after submitting filter text"
        );

        // Act 2: Submit empty filter text (clearing the filter)
        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "".to_string(),
        });
        let cmds_after_clear = logic.test_drain_commands();

        // Assert 2
        assert!(
            logic.test_get_filter_text().is_none(),
            "Filter text in UI state should be None after submitting empty text."
        );
        assert!(
            find_command(&cmds_after_clear, |cmd| matches!(
                cmd,
                PlatformCommand::PopulateTreeView { .. }
            ))
            .is_some(),
            "PopulateTreeView command should be enqueued after clearing filter."
        );
        assert!(
            find_command(&cmds_after_clear, |cmd| matches!(cmd,
                PlatformCommand::ApplyStyleToControl { window_id: wid, control_id, style_id }
                    if *wid == window_id && *control_id == ui_constants::FILTER_INPUT_ID && *style_id == StyleId::DefaultInput
            ))
            .is_some(),
            "Expected style change to reset filter to default"
        );
        assert!(
            find_command(&cmds_after_clear, |cmd| matches!(cmd,
                PlatformCommand::ExpandAllTreeItems { window_id: wid, control_id }
                    if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
            ))
            .is_some(),
            "Expected ExpandAllTreeItems after clearing filter text"
        );
    }

    #[test]
    fn test_filter_text_submission_populates_filtered_tree() {
        // Arrange
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let mut dir = FileNode::new_test(PathBuf::from("/root/dir1"), "dir1".into(), true);
        dir.children = vec![
            FileNode::new_test(
                PathBuf::from("/root/dir1/match.txt"),
                "match.txt".into(),
                false,
            ),
            FileNode::new_test(
                PathBuf::from("/root/dir1/other.txt"),
                "other.txt".into(),
                false,
            ),
        ];
        let nodes = vec![
            FileNode::new_test(PathBuf::from("/root/other.txt"), "other.txt".into(), false),
            dir,
        ];
        mock_app_session
            .lock()
            .unwrap()
            .set_snapshot_nodes_for_mock(nodes);

        // Act
        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "match".to_string(),
        });
        let cmds = logic.test_drain_commands();

        // Assert
        let populate_cmd = find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::PopulateTreeView { .. })
        });
        assert!(
            populate_cmd.is_some(),
            "Expected PopulateTreeView command after filtering"
        );
        assert!(
            find_command(&cmds, |cmd| matches!(cmd,
                PlatformCommand::ExpandAllTreeItems { window_id: wid, control_id }
                    if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
            ))
            .is_some(),
            "Expected ExpandAllTreeItems after filtering"
        );
        if let Some(PlatformCommand::PopulateTreeView { items, .. }) = populate_cmd {
            assert_eq!(items.len(), 1);
            let indicator_suffix = format!(" {}", ui_constants::NEW_ITEM_INDICATOR_CHAR);
            assert_eq!(items[0].text, format!("dir1{indicator_suffix}"));
            assert_eq!(items[0].children.len(), 1);
            assert_eq!(
                items[0].children[0].text,
                format!("match.txt{indicator_suffix}")
            );
        }
    }

    #[test]
    fn test_expand_button_with_filter_expands_visible() {
        let (mut logic, _mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "filter".to_string(),
        });
        logic.test_drain_commands();

        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::FILTER_EXPAND_BUTTON_ID,
        });
        let cmds = logic.test_drain_commands();

        assert!(find_command(&cmds, |cmd| {
            matches!(cmd,
            PlatformCommand::ExpandVisibleTreeItems { window_id: wid, control_id } if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
        )
        })
        .is_some(), "Expected ExpandVisibleTreeItems command when filter is active");
    }

    #[test]
    fn test_expand_button_without_filter_expands_all() {
        let (mut logic, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::FILTER_EXPAND_BUTTON_ID,
        });
        let cmds = logic.test_drain_commands();

        assert!(find_command(&cmds, |cmd| {
            matches!(cmd,
            PlatformCommand::ExpandAllTreeItems { window_id: wid, control_id } if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
        )
        })
        .is_some(), "Expected ExpandAllTreeItems command when no filter is set");
    }

    #[test]
    fn test_clear_button_clears_filter_and_updates_ui() {
        let (mut logic, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "abc".into(),
        });
        logic.test_drain_commands();

        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::FILTER_CLEAR_BUTTON_ID,
        });
        let cmds = logic.test_drain_commands();

        assert!(logic.test_get_filter_text().is_none());
        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::PopulateTreeView { .. }
            ))
            .is_some()
        );
        assert!(find_command(&cmds, |cmd| {
            matches!(cmd,
            PlatformCommand::SetInputText { window_id: wid, control_id, text } if *wid == window_id && *control_id == ui_constants::FILTER_INPUT_ID && text.is_empty()
        )
        })
        .is_some(), "Expected SetInputText to clear filter input");
        assert!(find_command(&cmds, |cmd| matches!(cmd,
            PlatformCommand::ApplyStyleToControl { window_id: wid, control_id, style_id }
                if *wid == window_id && *control_id == ui_constants::FILTER_INPUT_ID && *style_id == StyleId::DefaultInput
        )).is_some(), "Expected style reset on clear button");
        assert!(
            find_command(&cmds, |cmd| matches!(cmd,
                PlatformCommand::ExpandAllTreeItems { window_id: wid, control_id }
                    if *wid == window_id && *control_id == ui_constants::ID_TREEVIEW_CTRL
            ))
            .is_some(),
            "Expected ExpandAllTreeItems command after clearing filter"
        );
    }

    #[test]
    fn test_search_mode_toggle_button_cycles_and_updates_label() {
        // Arrange
        let (mut logic, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        logic.test_drain_commands();

        // Act
        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::SEARCH_MODE_TOGGLE_BUTTON_ID,
        });
        let cmds_after_first_toggle = logic.test_drain_commands();

        // Assert
        assert_eq!(
            logic.test_get_search_mode(),
            Some(SearchMode::ByContent),
            "Expected search mode to switch to content after first click"
        );
        assert!(
            find_command(&cmds_after_first_toggle, |cmd| matches!(cmd,
                PlatformCommand::SetControlText { window_id: wid, control_id, text }
                    if *wid == window_id
                        && *control_id == ui_constants::SEARCH_MODE_TOGGLE_BUTTON_ID
                        && text == "Content"
            ))
            .is_some(),
            "Expected SetControlText to label button as 'Content'"
        );

        // Act
        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::SEARCH_MODE_TOGGLE_BUTTON_ID,
        });
        let cmds_after_second_toggle = logic.test_drain_commands();

        // Assert
        assert_eq!(
            logic.test_get_search_mode(),
            Some(SearchMode::ByName),
            "Expected search mode to return to name after second click"
        );
        assert!(
            find_command(&cmds_after_second_toggle, |cmd| matches!(cmd,
                PlatformCommand::SetControlText { window_id: wid, control_id, text }
                    if *wid == window_id
                        && *control_id == ui_constants::SEARCH_MODE_TOGGLE_BUTTON_ID
                        && text == "Name"
            ))
            .is_some(),
            "Expected SetControlText to relabel button as 'Name'"
        );
    }

    #[test]
    fn test_content_search_flow_populates_ui_state_with_matches() {
        let (mut logic, mock_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);
        logic.test_drain_commands();

        // Switch to content mode so submitting text triggers the async search path.
        logic.handle_event(AppEvent::ButtonClicked {
            window_id,
            control_id: ui_constants::SEARCH_MODE_TOGGLE_BUTTON_ID,
        });
        logic.test_drain_commands();

        let (tx, rx) = mpsc::channel();
        {
            let guard = mock_session.lock().unwrap();
            guard.set_content_search_receiver_for_mock(Some(rx));
        }

        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "needle".into(),
        });
        logic.test_drain_commands();

        {
            let guard = mock_session.lock().unwrap();
            assert_eq!(
                guard.get_search_content_async_call_count(),
                1,
                "Expected backend search to be triggered exactly once"
            );
        }

        tx.send(ContentSearchProgress {
            is_final: true,
            results: vec![
                ContentSearchResult {
                    path: PathBuf::from("match.txt"),
                    matches: true,
                },
                ContentSearchResult {
                    path: PathBuf::from("miss.txt"),
                    matches: false,
                },
            ],
        })
        .unwrap();

        let cmds = logic.test_collect_commands_until_idle();

        let mut matches = logic
            .test_get_content_search_matches()
            .expect("Expected content search matches to be stored");
        matches.sort();
        assert_eq!(matches, vec![PathBuf::from("match.txt")]);
        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::PopulateTreeView { .. }
            ))
            .is_some(),
            "Expected PopulateTreeView after content search completes"
        );
    }

    #[test]
    fn test_no_match_filter_applies_error_style() {
        // Arrange
        let (mut logic, mock_app_session, ..) = setup_logic_with_mocks();
        let window_id = WindowId::new(1);
        logic.test_set_main_window_id_and_init_ui_state(window_id);

        let nodes = vec![FileNode::new_test(
            PathBuf::from("/root/match.txt"),
            "match.txt".into(),
            false,
        )];
        mock_app_session
            .lock()
            .unwrap()
            .set_snapshot_nodes_for_mock(nodes);

        // Act 1: apply matching filter
        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "match".into(),
        });
        let _ = logic.test_drain_commands();

        // Act 2: apply non-matching filter
        logic.handle_event(AppEvent::InputTextChanged {
            window_id,
            control_id: ui_constants::FILTER_INPUT_ID,
            text: "none".into(),
        });
        let cmds = logic.test_drain_commands();

        // Assert
        assert!(logic.test_get_filter_text().is_some());
        assert!(
            find_command(&cmds, |cmd| matches!(
                cmd,
                PlatformCommand::PopulateTreeView { .. }
            ))
            .is_some()
        );
        assert!(find_command(&cmds, |cmd| matches!(cmd,
            PlatformCommand::ApplyStyleToControl { window_id: wid, control_id, style_id }
                if *wid == window_id && *control_id == ui_constants::FILTER_INPUT_ID && *style_id == StyleId::DefaultInputError
        )).is_some(), "Expected error style for no match");
    }
}
