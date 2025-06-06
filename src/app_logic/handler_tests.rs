use super::handler::*;
use crate::app_logic::ui_constants;

use crate::core::{
    ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations, FileNode,
    FileSystemError, FileSystemScannerOperations, NodeStateApplicatorOperations, Profile,
    ProfileError, ProfileManagerOperations, ProfileRuntimeDataOperations, SelectionState,
    TokenCounterOperations, file_node::FileTokenDetails,
};
use crate::platform_layer::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler, TreeItemId,
    WindowId, types::MenuAction,
};

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicUsize, Ordering},
};
use std::time::SystemTime;
use tempfile::{NamedTempFile, tempdir};

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

    // Call counters for &self methods using AtomicUsize
    get_profile_name_calls: AtomicUsize,
    get_archive_path_calls: AtomicUsize,
    get_snapshot_nodes_calls: AtomicUsize,
    get_root_path_for_scan_calls: AtomicUsize,
    get_cached_total_token_count_calls: AtomicUsize,
    get_cached_file_token_details_calls: AtomicUsize,
    create_profile_snapshot_calls: AtomicUsize,
    does_path_or_descendants_contain_new_file_calls: AtomicUsize,

    // Call logs/trackers for &mut self methods (plain types, as they are called on &mut MockProfileRuntimeData)
    _set_profile_name_log: Vec<Option<String>>,
    _set_archive_path_log: Vec<Option<PathBuf>>,
    _set_root_path_for_scan_log: Vec<PathBuf>,
    _set_snapshot_nodes_log: Vec<Vec<FileNode>>,
    _clear_snapshot_nodes_calls: usize,
    _apply_selection_states_to_snapshot_log: Vec<(HashSet<PathBuf>, HashSet<PathBuf>)>,
    _update_node_state_and_collect_changes_log: Vec<(PathBuf, SelectionState)>,
    _set_cached_file_token_details_log: Vec<HashMap<PathBuf, FileTokenDetails>>,
    _update_total_token_count_calls: usize,
    _clear_calls: usize,
    _load_profile_into_session_log: Vec<Profile>,
    _does_path_or_descendants_contain_new_file_log: Mutex<Vec<PathBuf>>,

    // Mock results
    get_node_attributes_for_path_result: Option<(SelectionState, bool)>,
    update_node_state_and_collect_changes_result: Vec<(PathBuf, SelectionState)>,
    load_profile_into_session_result: Result<(), String>,
    does_path_or_descendants_contain_new_file_results: Mutex<HashMap<PathBuf, bool>>,
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

            get_profile_name_calls: AtomicUsize::new(0),
            get_archive_path_calls: AtomicUsize::new(0),
            get_snapshot_nodes_calls: AtomicUsize::new(0),
            get_root_path_for_scan_calls: AtomicUsize::new(0),
            get_cached_total_token_count_calls: AtomicUsize::new(0),
            get_cached_file_token_details_calls: AtomicUsize::new(0),
            create_profile_snapshot_calls: AtomicUsize::new(0),
            does_path_or_descendants_contain_new_file_calls: AtomicUsize::new(0),

            _set_profile_name_log: Vec::new(),
            _set_archive_path_log: Vec::new(),
            _set_root_path_for_scan_log: Vec::new(),
            _set_snapshot_nodes_log: Vec::new(),
            _clear_snapshot_nodes_calls: 0,
            _apply_selection_states_to_snapshot_log: Vec::new(),
            _update_node_state_and_collect_changes_log: Vec::new(),
            _set_cached_file_token_details_log: Vec::new(),
            _update_total_token_count_calls: 0,
            _clear_calls: 0,
            _load_profile_into_session_log: Vec::new(),
            _does_path_or_descendants_contain_new_file_log: Mutex::new(Vec::new()),

            get_node_attributes_for_path_result: None,
            update_node_state_and_collect_changes_result: Vec::new(),
            load_profile_into_session_result: Ok(()),
            does_path_or_descendants_contain_new_file_results: Mutex::new(HashMap::new()),
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
    }
    #[allow(dead_code)]
    fn set_cached_file_token_details_for_mock(
        &mut self,
        details: HashMap<PathBuf, FileTokenDetails>,
    ) {
        self.cached_file_token_details = details;
    }
    #[allow(dead_code)]
    fn set_get_node_attributes_for_path_result(&mut self, result: Option<(SelectionState, bool)>) {
        self.get_node_attributes_for_path_result = result;
    }
    #[allow(dead_code)]
    fn set_update_node_state_and_collect_changes_result(
        &mut self,
        result: Vec<(PathBuf, SelectionState)>,
    ) {
        self.update_node_state_and_collect_changes_result = result;
    }
    #[allow(dead_code)]
    fn set_load_profile_into_session_result(&mut self, result: Result<(), String>) {
        self.load_profile_into_session_result = result;
    }
    #[allow(dead_code)]
    fn set_does_path_or_descendants_contain_new_file_result(&self, path: &Path, result: bool) {
        self.does_path_or_descendants_contain_new_file_results
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), result);
    }

    // Test getters for call logs/counters
    #[allow(dead_code)]
    fn get_load_profile_into_session_calls_log(&self) -> &Vec<Profile> {
        &self._load_profile_into_session_log
    }
    #[allow(dead_code)]
    fn get_set_archive_path_calls_log(&self) -> &Vec<Option<PathBuf>> {
        &self._set_archive_path_log
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
        self._set_profile_name_log.push(name.clone());
        self.profile_name = name;
    }
    fn get_archive_path(&self) -> Option<PathBuf> {
        self.get_archive_path_calls.fetch_add(1, Ordering::Relaxed);
        self.archive_path.clone()
    }
    fn set_archive_path(&mut self, path: Option<PathBuf>) {
        self._set_archive_path_log.push(path.clone());
        self.archive_path = path;
    }
    fn get_root_path_for_scan(&self) -> PathBuf {
        self.get_root_path_for_scan_calls
            .fetch_add(1, Ordering::Relaxed);
        self.root_path_for_scan.clone()
    }
    fn set_root_path_for_scan(&mut self, path: PathBuf) {
        self._set_root_path_for_scan_log.push(path.clone());
        self.root_path_for_scan = path;
    }
    fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
        self.get_snapshot_nodes_calls
            .fetch_add(1, Ordering::Relaxed);
        &self.snapshot_nodes
    }
    fn clear_snapshot_nodes(&mut self) {
        self._clear_snapshot_nodes_calls += 1;
        self.snapshot_nodes.clear();
    }
    fn set_snapshot_nodes(&mut self, nodes: Vec<FileNode>) {
        self._set_snapshot_nodes_log.push(nodes.clone());
        self.snapshot_nodes = nodes;
    }
    fn apply_selection_states_to_snapshot(
        &mut self,
        _state_manager: &dyn NodeStateApplicatorOperations,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    ) {
        self._apply_selection_states_to_snapshot_log
            .push((selected_paths.clone(), deselected_paths.clone()));
        // Basic simulation for mock:
        fn apply_recursive(
            nodes: &mut Vec<FileNode>,
            selected: &HashSet<PathBuf>,
            deselected: &HashSet<PathBuf>,
        ) {
            for node in nodes.iter_mut() {
                if selected.contains(&node.path) {
                    node.state = SelectionState::Selected;
                } else if deselected.contains(&node.path) {
                    node.state = SelectionState::Deselected;
                } else {
                    node.state = SelectionState::New;
                }
                if node.is_dir {
                    apply_recursive(&mut node.children, selected, deselected);
                }
            }
        }
        apply_recursive(&mut self.snapshot_nodes, selected_paths, deselected_paths);
    }
    fn get_node_attributes_for_path(&self, path_to_find: &Path) -> Option<(SelectionState, bool)> {
        // Search self.snapshot_nodes for more functional mock.
        fn find_node_attrs_recursive(
            nodes: &[FileNode],
            path: &Path,
        ) -> Option<(SelectionState, bool)> {
            for node in nodes {
                if node.path == path {
                    return Some((node.state, node.is_dir));
                }
                if node.is_dir {
                    if let Some(attrs) = find_node_attrs_recursive(&node.children, path) {
                        return Some(attrs);
                    }
                }
            }
            None
        }
        find_node_attrs_recursive(&self.snapshot_nodes, path_to_find)
            .or_else(|| self.get_node_attributes_for_path_result.clone()) // Fallback to preset result
    }
    fn update_node_state_and_collect_changes(
        &mut self,
        path: &Path,
        new_state: SelectionState,
        _state_manager: &dyn NodeStateApplicatorOperations,
    ) -> Vec<(PathBuf, SelectionState)> {
        self._update_node_state_and_collect_changes_log
            .push((path.to_path_buf(), new_state));

        // Basic simulation for mock:
        fn update_recursive(
            nodes: &mut Vec<FileNode>,
            target_path: &Path,
            new_sel_state: SelectionState,
            changes: &mut Vec<(PathBuf, SelectionState)>,
        ) -> bool {
            for node in nodes.iter_mut() {
                if node.path == target_path {
                    node.state = new_sel_state;
                    changes.push((node.path.clone(), node.state));
                    if node.is_dir {
                        for child in node.children.iter_mut() {
                            update_recursive_children(child, new_sel_state, changes);
                        }
                    }
                    return true;
                }
                if node.is_dir && target_path.starts_with(&node.path) {
                    // Optimization
                    if update_recursive(&mut node.children, target_path, new_sel_state, changes) {
                        return true;
                    }
                }
            }
            false
        }
        fn update_recursive_children(
            node: &mut FileNode,
            new_sel_state: SelectionState,
            changes: &mut Vec<(PathBuf, SelectionState)>,
        ) {
            node.state = new_sel_state;
            changes.push((node.path.clone(), node.state));
            if node.is_dir {
                for child in node.children.iter_mut() {
                    update_recursive_children(child, new_sel_state, changes);
                }
            }
        }
        let mut actual_changes = Vec::new();
        update_recursive(
            &mut self.snapshot_nodes,
            path,
            new_state,
            &mut actual_changes,
        );

        // Prefer actual_changes if populated by simulation, else use preset result.
        if !actual_changes.is_empty() {
            actual_changes
        } else {
            self.update_node_state_and_collect_changes_result.clone()
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

        // Fallback to actual recursive check on mock's snapshot_nodes if no preset result
        fn check_recursive(nodes: &[FileNode], target_path: &Path) -> Option<bool> {
            for node in nodes {
                if node.path == target_path {
                    return Some(check_node_itself_or_descendants(node));
                }
                if node.is_dir && target_path.starts_with(&node.path) {
                    // Optimization
                    if let Some(found_in_child) = check_recursive(&node.children, target_path) {
                        return Some(found_in_child);
                    }
                }
            }
            None
        }
        fn check_node_itself_or_descendants(node: &FileNode) -> bool {
            if !node.is_dir {
                return node.state == SelectionState::New;
            }
            // For directory, check itself (if it has a state that implies "new" - though folders don't directly become new files)
            // OR check its children
            for child in &node.children {
                if check_node_itself_or_descendants(child) {
                    return true;
                }
            }
            false
        }
        check_recursive(&self.snapshot_nodes, path).unwrap_or(false)
    }

    fn get_cached_file_token_details(&self) -> HashMap<PathBuf, FileTokenDetails> {
        self.get_cached_file_token_details_calls
            .fetch_add(1, Ordering::Relaxed);
        self.cached_file_token_details.clone()
    }
    fn set_cached_file_token_details(&mut self, details: HashMap<PathBuf, FileTokenDetails>) {
        self._set_cached_file_token_details_log
            .push(details.clone());
        self.cached_file_token_details = details;
    }
    fn get_cached_total_token_count(&self) -> usize {
        self.get_cached_total_token_count_calls
            .fetch_add(1, Ordering::Relaxed);
        self.cached_total_token_count
    }
    fn update_total_token_count_for_selected_files(
        &mut self,
        _token_counter: &dyn TokenCounterOperations,
    ) -> usize {
        self._update_total_token_count_calls += 1;
        // Basic simulation for mock based on its own snapshot_nodes
        let mut count = 0;
        fn sum_tokens_recursive(
            nodes: &[FileNode],
            current_sum: &mut usize,
            cached_details: &HashMap<PathBuf, FileTokenDetails>,
        ) {
            for node in nodes {
                if node.is_dir {
                    sum_tokens_recursive(&node.children, current_sum, cached_details);
                } else if node.state == SelectionState::Selected {
                    if let Some(details) = cached_details.get(&node.path) {
                        // Let's assume if it's in cached_details, it has a count we can use
                        *current_sum += details.token_count;
                    } else {
                        // If not in cache, and we don't simulate file reading for mock,
                        // we might default to adding a small number or 0.
                        // For simplicity, let's assume this mock's `update_total_token_count_for_selected_files`
                        // relies on `set_cached_total_token_count_for_mock` or `set_cached_file_token_details_for_mock`
                        // to have been called to set up expected outcomes.
                        // Or, we can use the `self.cached_total_token_count` field directly if not trying to fully simulate.
                    }
                }
            }
        }
        sum_tokens_recursive(
            &self.snapshot_nodes,
            &mut count,
            &self.cached_file_token_details,
        );
        if count > 0 {
            // if simulation yielded something
            self.cached_total_token_count = count;
        }
        // Return the possibly pre-set mock value, or the simulated one.
        self.cached_total_token_count
    }
    fn clear(&mut self) {
        self._clear_calls += 1;
        self.profile_name = None;
        self.archive_path = None;
        self.snapshot_nodes.clear();
        self.root_path_for_scan = PathBuf::from(".");
        self.cached_total_token_count = 0;
        self.cached_file_token_details.clear();
    }
    fn create_profile_snapshot(&self) -> Profile {
        self.create_profile_snapshot_calls
            .fetch_add(1, Ordering::Relaxed);
        let mut profile = Profile::new(
            self.profile_name.clone().unwrap_or_else(String::new),
            self.root_path_for_scan.clone(),
        );
        profile.archive_path = self.archive_path.clone();
        profile.file_details = self.cached_file_token_details.clone(); // Simple mock behavior

        fn gather_paths_recursive(
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
                    _ => {}
                }
                if node.is_dir {
                    gather_paths_recursive(&node.children, selected, deselected);
                }
            }
        }
        gather_paths_recursive(
            &self.snapshot_nodes,
            &mut profile.selected_paths,
            &mut profile.deselected_paths,
        );
        profile
    }
    fn load_profile_into_session(
        &mut self,
        loaded_profile: Profile,
        _file_system_scanner: &dyn FileSystemScannerOperations,
        _state_manager: &dyn NodeStateApplicatorOperations,
        _token_counter: &dyn TokenCounterOperations,
    ) -> Result<(), String> {
        self._load_profile_into_session_log
            .push(loaded_profile.clone());
        let res = self.load_profile_into_session_result.clone();
        if res.is_ok() {
            self.profile_name = Some(loaded_profile.name);
            self.archive_path = loaded_profile.archive_path;
            self.root_path_for_scan = loaded_profile.root_folder;
            self.cached_file_token_details = loaded_profile.file_details;
            // Mock doesn't fully re-scan or re-apply selection for snapshot_nodes here.
            // Tests should set snapshot_nodes directly if needed after this call.
            // Or, we can copy from a pre-set snapshot_nodes for this profile.
            // For now, it just sets profile metadata.
        } else {
            self.clear(); // Simulate clearing data on load failure
        }
        res
    }
    fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
        let mut selected = HashSet::new();
        let mut deselected = HashSet::new();
        fn gather_paths_recursive(
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
                    _ => {}
                }
                if node.is_dir {
                    gather_paths_recursive(&node.children, selected, deselected);
                }
            }
        }
        gather_paths_recursive(&self.snapshot_nodes, &mut selected, &mut deselected);
        return (selected, deselected);
    }
}
// --- End MockProfileRuntimeData ---

// --- Mock Structures (ConfigManager, ProfileManager, FileSystemScanner, Archiver, StateManager) ---
// These are assumed to be correct from previous steps.
struct MockConfigManager {
    load_last_profile_name_result: Mutex<Result<Option<String>, ConfigError>>,
    saved_profile_name: Mutex<Option<(String, String)>>,
}
impl MockConfigManager {
    fn new() -> Self {
        MockConfigManager {
            load_last_profile_name_result: Mutex::new(Ok(None)),
            saved_profile_name: Mutex::new(None),
        }
    }
    fn set_load_last_profile_name_result(&self, result: Result<Option<String>, ConfigError>) {
        *self.load_last_profile_name_result.lock().unwrap() = result;
    }
    fn get_saved_profile_name(&self) -> Option<(String, String)> {
        self.saved_profile_name.lock().unwrap().clone()
    }
}
impl ConfigManagerOperations for MockConfigManager {
    fn load_last_profile_name(&self, _app_name: &str) -> Result<Option<String>, ConfigError> {
        self.load_last_profile_name_result
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
    fn save_last_profile_name(
        &self,
        app_name: &str,
        profile_name: &str,
    ) -> Result<(), ConfigError> {
        *self.saved_profile_name.lock().unwrap() =
            Some((app_name.to_string(), profile_name.to_string()));
        Ok(())
    }
}

struct MockProfileManager {
    load_profile_results: Mutex<HashMap<String, Result<Profile, ProfileError>>>,
    load_profile_from_path_results: Mutex<HashMap<PathBuf, Result<Profile, ProfileError>>>,
    save_profile_calls: Mutex<Vec<(Profile, String)>>,
    save_profile_result: Mutex<Result<(), ProfileError>>,
    list_profiles_result: Mutex<Result<Vec<String>, ProfileError>>,
    get_profile_dir_path_result: Mutex<Option<PathBuf>>,
}
impl MockProfileManager {
    fn new() -> Self {
        MockProfileManager {
            load_profile_results: Mutex::new(HashMap::new()),
            load_profile_from_path_results: Mutex::new(HashMap::new()),
            save_profile_calls: Mutex::new(Vec::new()),
            save_profile_result: Mutex::new(Ok(())),
            list_profiles_result: Mutex::new(Ok(Vec::new())),
            get_profile_dir_path_result: Mutex::new(Some(PathBuf::from("/mock/profiles"))),
        }
    }
    fn set_load_profile_result(&self, profile_name: &str, result: Result<Profile, ProfileError>) {
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
    fn get_save_profile_calls(&self) -> Vec<(Profile, String)> {
        self.save_profile_calls.lock().unwrap().clone()
    }
    #[allow(dead_code)]
    fn set_list_profiles_result(&self, result: Result<Vec<String>, ProfileError>) {
        *self.list_profiles_result.lock().unwrap() = result;
    }
    #[allow(dead_code)]
    fn set_get_profile_dir_path_result(&self, result: Option<PathBuf>) {
        *self.get_profile_dir_path_result.lock().unwrap() = result;
    }
}
impl ProfileManagerOperations for MockProfileManager {
    fn load_profile(&self, profile_name: &str, _app_name: &str) -> Result<Profile, ProfileError> {
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
                format!("MockProfileManager: No result set for path {:?}", path),
            ))),
        }
    }
    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<(), ProfileError> {
        let result_to_return = match *self.save_profile_result.lock().unwrap() {
            Ok(_) => Ok(()),
            Err(ref e) => Err(clone_profile_error(e)),
        };
        if result_to_return.is_ok() {
            self.save_profile_calls
                .lock()
                .unwrap()
                .push((profile.clone(), app_name.to_string()));
        }
        result_to_return
    }
    fn list_profiles(&self, _app_name: &str) -> Result<Vec<String>, ProfileError> {
        match *self.list_profiles_result.lock().unwrap() {
            Ok(ref names) => Ok(names.clone()),
            Err(ref e) => Err(clone_profile_error(e)),
        }
    }
    fn get_profile_dir_path(&self, _app_name: &str) -> Option<PathBuf> {
        self.get_profile_dir_path_result.lock().unwrap().clone()
    }
}
fn clone_profile_error(error: &ProfileError) -> ProfileError {
    match error {
        ProfileError::Io(e) => ProfileError::Io(io::Error::new(e.kind(), format!("{}", e))),
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
    fn scan_directory(&self, root_path: &Path) -> Result<Vec<FileNode>, FileSystemError> {
        self.scan_directory_calls
            .lock()
            .unwrap()
            .push(root_path.to_path_buf());
        let map = self.scan_directory_results.lock().unwrap();
        match map.get(root_path) {
            Some(Ok(nodes)) => Ok(nodes.clone()),
            Some(Err(e)) => Err(clone_file_system_error(e)),
            None => Ok(Vec::new()),
        }
    }
}
fn clone_file_system_error(error: &FileSystemError) -> FileSystemError {
    match error {
        FileSystemError::Io(e) => FileSystemError::Io(io::Error::new(e.kind(), format!("{}", e))),
        FileSystemError::IgnoreError(original_ignore_error) => {
            let error_message = format!("Mocked IgnoreError: {:?}", original_ignore_error);
            let mock_io_err = io::Error::new(io::ErrorKind::Other, error_message);
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
    io::Error::new(error.kind(), format!("{}", error))
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
            .map_err(|e| clone_io_error(e))
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
            .map_err(|e| clone_io_error(e))
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
                format!("MockArchiver: No timestamp result set for path {:?}", path),
            )),
        }
    }
}

struct MockStateManager {
    apply_profile_to_tree_calls: Mutex<Vec<(HashSet<PathBuf>, HashSet<PathBuf>, Vec<FileNode>)>>,
    update_folder_selection_calls: Mutex<Vec<(FileNode, SelectionState)>>,
}
impl MockStateManager {
    fn new() -> Self {
        MockStateManager {
            apply_profile_to_tree_calls: Mutex::new(Vec::new()),
            update_folder_selection_calls: Mutex::new(Vec::new()),
        }
    }
    #[allow(dead_code)]
    fn get_apply_profile_to_tree_calls(
        &self,
    ) -> Vec<(HashSet<PathBuf>, HashSet<PathBuf>, Vec<FileNode>)> {
        self.apply_profile_to_tree_calls.lock().unwrap().clone()
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
        self.apply_profile_to_tree_calls.lock().unwrap().push((
            selected_paths.clone(),
            deselected_paths.clone(),
            tree.clone(),
        ));
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
            .push((node.clone(), new_state));
        node.state = new_state;
        if node.is_dir {
            for child in node.children.iter_mut() {
                self.update_folder_selection(child, new_state);
            }
        }
    }
}

struct MockTokenCounter {
    counts_for_content: Mutex<HashMap<String, usize>>,
    default_count: usize,
}
impl MockTokenCounter {
    fn new(default_count: usize) -> Self {
        MockTokenCounter {
            counts_for_content: Mutex::new(HashMap::new()),
            default_count,
        }
    }
    fn set_count_for_content(&self, content: &str, count: usize) {
        self.counts_for_content
            .lock()
            .unwrap()
            .insert(content.to_string(), count);
    }
}
impl TokenCounterOperations for MockTokenCounter {
    fn count_tokens(&self, content: &str) -> usize {
        *self
            .counts_for_content
            .lock()
            .unwrap()
            .get(content)
            .unwrap_or(&self.default_count)
    }
}

fn setup_logic_with_mocks() -> (
    MyAppLogic,
    Arc<Mutex<MockProfileRuntimeData>>,
    Arc<MockConfigManager>,
    Arc<MockProfileManager>,
    Arc<MockFileSystemScanner>,
    Arc<MockArchiver>,
    Arc<MockStateManager>,
    Arc<MockTokenCounter>,
) {
    crate::initialize_logging();
    let mock_app_session_data_for_test = Arc::new(Mutex::new(MockProfileRuntimeData::new()));
    let mock_config_manager_arc = Arc::new(MockConfigManager::new());
    let mock_profile_manager_arc = Arc::new(MockProfileManager::new());
    let mock_file_system_scanner_arc = Arc::new(MockFileSystemScanner::new());
    let mock_archiver_arc = Arc::new(MockArchiver::new());
    let mock_state_manager_arc = Arc::new(MockStateManager::new());
    let mock_token_counter_arc = Arc::new(MockTokenCounter::new(1));

    let logic = MyAppLogic::new(
        Arc::clone(&mock_app_session_data_for_test) as Arc<Mutex<dyn ProfileRuntimeDataOperations>>,
        Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>,
        Arc::clone(&mock_profile_manager_arc) as Arc<dyn ProfileManagerOperations>,
        Arc::clone(&mock_file_system_scanner_arc) as Arc<dyn FileSystemScannerOperations>,
        Arc::clone(&mock_archiver_arc) as Arc<dyn ArchiverOperations>,
        Arc::clone(&mock_token_counter_arc) as Arc<dyn TokenCounterOperations>,
        Arc::clone(&mock_state_manager_arc) as Arc<dyn NodeStateApplicatorOperations>,
    );
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

fn find_command<'a, F>(cmds: &'a [PlatformCommand], mut predicate: F) -> Option<&'a PlatformCommand>
where
    F: FnMut(&PlatformCommand) -> bool,
{
    cmds.iter().find(|&cmd| predicate(cmd))
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
fn test_on_main_window_created_loads_last_profile_with_all_mocks() {
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
    let startup_profile_root = PathBuf::from("/mock/startup_root");
    let startup_archive_path = startup_profile_root.join("startup_archive.txt");

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let mut selected_paths_for_profile = HashSet::new();
    let mock_file_path = startup_profile_root.join("mock_startup_file.txt");
    selected_paths_for_profile.insert(mock_file_path.clone());

    let mock_loaded_profile_dto = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: startup_profile_root.clone(),
        selected_paths: selected_paths_for_profile.clone(),
        deselected_paths: HashSet::new(),
        archive_path: Some(startup_archive_path.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager.set_load_profile_result(
        last_profile_name_to_load,
        Ok(mock_loaded_profile_dto.clone()),
    );

    let scanned_nodes = vec![FileNode {
        path: mock_file_path.clone(),
        name: "mock_startup_file.txt".into(),
        is_dir: false,
        state: SelectionState::New,
        children: vec![],
        checksum: Some("checksum_for_startup_file".to_string()),
    }];
    mock_file_system_scanner
        .set_scan_directory_result(&startup_profile_root, Ok(scanned_nodes.clone()));

    {
        let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
        mock_app_session.set_load_profile_into_session_result(Ok(()));
        mock_app_session.set_cached_total_token_count_for_mock(5);
    }

    mock_archiver.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    // Act
    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    // Assert
    {
        let mock_app_session = mock_app_session_mutexed.lock().unwrap();
        assert_eq!(
            mock_app_session._load_profile_into_session_log.len(),
            1,
            "load_profile_into_session should be called once on the mock session data"
        );
        let loaded_profile_in_mock = &mock_app_session._load_profile_into_session_log[0];
        assert_eq!(loaded_profile_in_mock.name, last_profile_name_to_load);
    }

    // These assertions use MyAppLogic's test helpers, which internally access the (mocked) ProfileRuntimeDataOperations
    assert_eq!(
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .profile_name
            .clone(),
        Some(last_profile_name_to_load.to_string())
    );
    assert_eq!(
        mock_app_session_mutexed.lock().unwrap().archive_path,
        Some(startup_archive_path.clone())
    );
    assert_eq!(
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .cached_total_token_count,
        5,
        "Token count should be 5 as per mock setup"
    );

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        last_profile_name_to_load,
        startup_archive_path.display()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)).is_some(),
        "Expected SetWindowTitle with correct title. Got: {:?}", cmds
    );
    let general_token_status_text = "Token count updated";
    let dedicated_token_status_text = "Tokens: 5";
    let profile_loaded_final_text = format!("Profile '{}' loaded.", last_profile_name_to_load);
    let profile_loaded_initial_text = format!(
        "Successfully loaded last profile '{}' on startup.",
        last_profile_name_to_load
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == &profile_loaded_initial_text && *severity == MessageSeverity::Information )).is_some(), "Expected initial profile loaded message. Got: {:?}", cmds );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == general_token_status_text && *severity == MessageSeverity::Information )).is_some(), "Expected general 'Token count updated' message. Got: {:?}", cmds );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == dedicated_token_status_text )).is_some(), "Expected dedicated token label 'Tokens: 5'. Got: {:?}", cmds );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *text == profile_loaded_final_text && *severity == MessageSeverity::Information )).is_some(), "Expected final profile loaded message. Got: {:?}", cmds );
}

#[test]
fn test_menu_set_archive_path_cancelled() {
    // Arrange
    let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    {
        let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
        mock_app_session.set_profile_name_for_mock(Some("Test".to_string()));
        mock_app_session.set_root_path_for_scan_for_mock(PathBuf::from("."));
    }
    logic.test_set_pending_action(PendingAction::SettingArchivePath);

    // Act
    logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: main_window_id,
        result: None,
    });
    let _cmds = logic.test_drain_commands();

    // Assert
    assert!(
        logic.test_pending_action().is_none(),
        "Pending action should be cleared on cancel"
    );
    {
        let mock_app_session = mock_app_session_mutexed.lock().unwrap();
        assert_eq!(mock_app_session._set_archive_path_log.len(), 0);
    }
}

#[test]
fn test_profile_load_updates_archive_status_via_mock_archiver() {
    // Arrange
    let (
        mut logic,
        mock_app_session_mutexed,
        _mock_config_manager,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        _mock_state_manager,
        _mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "ProfileForStatusUpdateViaMockArchiver";
    let root_folder_for_profile = PathBuf::from("/mock/scan_root_status_mock_archiver");
    let archive_file_for_profile = PathBuf::from("/mock/my_mock_archiver_archive.txt");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/profiles/{}.json", profile_name));

    let mock_profile_to_load_dto = Profile {
        name: profile_name.to_string(),
        root_folder: root_folder_for_profile.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_for_profile.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_profile_to_load_dto.clone()),
    );
    mock_file_system_scanner_arc.set_scan_directory_result(&root_folder_for_profile, Ok(vec![]));
    {
        mock_app_session_mutexed
            .lock()
            .unwrap()
            .set_load_profile_into_session_result(Ok(()));
    }
    let archive_error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound));
    mock_archiver_arc.set_check_archive_status_result(archive_error_status.clone());

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
            ._load_profile_into_session_log
            .len(),
        1
    );
    let archiver_calls = mock_archiver_arc.get_check_archive_status_calls();
    assert_eq!(archiver_calls.len(), 1);
    assert_eq!(
        archiver_calls[0].0.as_deref(),
        Some(archive_file_for_profile.as_path())
    );
    assert!(archiver_calls[0].1.is_empty());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some()
    );
    let archive_status_text_for_dedicated_label = "Archive: Error: NotFound.".to_string();
    let archive_status_text_for_general_status =
        format!("Archive status error: {:?}", archive_error_status);
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &archive_status_text_for_dedicated_label && *severity == MessageSeverity::Error )).is_some(), "Expected dedicated archive label update for error. Got: {:?}", cmds );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text == &archive_status_text_for_general_status )).is_some(), "Expected new general label error for archive. Got: {:?}", cmds );
}

#[test]
fn test_menu_action_generate_archive_triggers_logic() {
    // Arrange
    let (mut logic, mock_app_session_mutexed, _, _, _, mock_archiver, _, _) =
        setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "ArchiveTestProfile";
    let archive_path = PathBuf::from("/test/archive.txt");
    let root_folder = PathBuf::from("/test/root");
    let file_nodes = vec![FileNode::new(
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
    mock_archiver.set_check_archive_status_result(ArchiveStatus::UpToDate);

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
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
    let success_text = format!("Archive saved to '{}'.", archive_path.display());
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Information && text == &success_text)).is_some(), "Expected new label success message. Got: {:?}", cmds);
}

#[test]
fn test_menu_action_generate_archive_no_profile_shows_error() {
    // Arrange
    let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    mock_app_session_mutexed
        .lock()
        .unwrap()
        .set_profile_name_for_mock(None);

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
    });
    let cmds = logic.test_drain_commands();

    // Assert
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No profile loaded"))).is_some(), "Expected 'No profile loaded' error status. Got: {:?}", cmds);
}

#[test]
fn test_menu_action_generate_archive_no_archive_path_shows_error() {
    // Arrange
    let (mut logic, mock_app_session_mutexed, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    {
        let mut mock_app_session = mock_app_session_mutexed.lock().unwrap();
        mock_app_session.set_profile_name_for_mock(Some("NoArchivePathProfile".to_string()));
        mock_app_session.set_archive_path_for_mock(None);
    }

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
    });
    let cmds = logic.test_drain_commands();

    // Assert
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No archive path set"))).is_some(), "Expected 'No archive path set' error status. Got: {:?}", cmds);
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
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    {
        let mut mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
        mock_app_session_guard.set_profile_name_for_mock(Some("TestProfile".to_string()));
        mock_app_session_guard.set_root_path_for_scan_for_mock(PathBuf::from("/root"));
        mock_app_session_guard.set_archive_path_for_mock(Some(PathBuf::from("/root/archive.txt")));
        mock_app_session_guard.set_snapshot_nodes_for_mock(vec![]);
    }

    // Case 1: ArchiveStatus is an error
    let error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::PermissionDenied));
    let expected_dedicated_error_text = "Archive: Error: PermissionDenied.".to_string();
    mock_archiver.set_check_archive_status_result(error_status.clone());

    // Act 1
    logic.update_current_archive_status();
    let cmds_error = logic.test_drain_commands();

    // Assert 1
    assert!(find_command(&cmds_error, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity } if *window_id == main_window_id && *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && text == &expected_dedicated_error_text && *severity == MessageSeverity::Error )).is_some(), "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (Error). Got: {:?}", cmds_error );
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
    mock_archiver.set_check_archive_status_result(info_status.clone());

    // Act 2
    logic.update_current_archive_status();
    let _cmds_info = logic.test_drain_commands();

    // Assert 2
    {
        let mut mock_app_session_guard = mock_app_session_mutexed.lock().unwrap();
        assert!(
            mock_app_session_guard
                .get_profile_name_calls
                .load(Ordering::Relaxed)
                > 0,
            "Case 2: get_profile_name_calls should be > 0"
        );
        // Reset for Case 3
        mock_app_session_guard
            .get_profile_name_calls
            .store(0, Ordering::Relaxed);
        mock_app_session_guard.set_profile_name_for_mock(None); // This is for the logic of Case 3
    }

    // Act 3
    logic.update_current_archive_status();
    let _cmds_no_profile = logic.test_drain_commands();

    // Assert 3
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
    let window_id = WindowId(1);
    let mut mutable_logic = logic; // Shadow immutable logic
    mutable_logic.test_set_main_window_id_and_init_ui_state(window_id);

    let root = PathBuf::from("/root");
    let file_new_path = root.join("new_file.txt");
    let file_sel_path = root.join("sel_file.txt");
    let folder_with_new_path = root.join("folder_new_child");
    let file_in_folder_new_path = folder_with_new_path.join("inner_new.txt");
    let folder_no_new_path = root.join("folder_no_new");

    // Populate mock session data
    {
        let mut app_data = mock_app_session_mutexed.lock().unwrap();
        app_data.set_snapshot_nodes_for_mock(vec![
            FileNode {
                path: file_new_path.clone(),
                name: "new_file.txt".into(),
                is_dir: false,
                state: SelectionState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                path: file_sel_path.clone(),
                name: "sel_file.txt".into(),
                is_dir: false,
                state: SelectionState::Selected,
                children: vec![],
                checksum: None,
            },
            FileNode {
                path: folder_with_new_path.clone(),
                name: "folder_new_child".into(),
                is_dir: true,
                state: SelectionState::New,
                children: vec![
                    // Folder itself can be New or Selected, doesn't matter for this test as much as its content
                    FileNode {
                        path: file_in_folder_new_path.clone(),
                        name: "inner_new.txt".into(),
                        is_dir: false,
                        state: SelectionState::New,
                        children: vec![],
                        checksum: None,
                    },
                ],
                checksum: None,
            },
            FileNode {
                path: folder_no_new_path.clone(),
                name: "folder_no_new".into(),
                is_dir: true,
                state: SelectionState::Selected,
                children: vec![],
                checksum: None,
            },
        ]);
        // Set mock results for does_path_or_descendants_contain_new_file
        app_data.set_does_path_or_descendants_contain_new_file_result(&folder_with_new_path, true);
        app_data.set_does_path_or_descendants_contain_new_file_result(&folder_no_new_path, false);
        // Files don't use this specific mock result in `is_tree_item_new`, they use get_node_attributes_for_path
    }

    // Use the new test helper to populate path_to_tree_item_id
    let item_id_file_new = TreeItemId(1);
    let item_id_file_sel = TreeItemId(2);
    let item_id_folder_new = TreeItemId(3);
    let item_id_folder_no_new = TreeItemId(5);
    // TreeItemId(4) for file_in_folder_new_path is not directly queried by is_tree_item_new in this test's assertions

    mutable_logic.test_set_path_to_tree_item_id_mapping(file_new_path.clone(), item_id_file_new);
    mutable_logic.test_set_path_to_tree_item_id_mapping(file_sel_path.clone(), item_id_file_sel);
    mutable_logic
        .test_set_path_to_tree_item_id_mapping(folder_with_new_path.clone(), item_id_folder_new);
    mutable_logic
        .test_set_path_to_tree_item_id_mapping(folder_no_new_path.clone(), item_id_folder_no_new);

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
        "Folder with no new child should not be new"
    );

    // Check if the mock method was called for folders (it's also called for files by the mock's own fallback)
    let log = mock_app_session_mutexed
        .lock()
        .unwrap()
        .get_does_path_or_descendants_contain_new_file_log();
    assert!(
        log.contains(&folder_with_new_path),
        "does_path_or_descendants_contain_new_file_log should contain folder_with_new_path"
    );
    assert!(
        log.contains(&folder_no_new_path),
        "does_path_or_descendants_contain_new_file_log should contain folder_no_new_path"
    );

    // Check that get_node_attributes_for_path was called for files (it will be, even if the mock for does_path... is also hit)
    let get_attrs_calls_before = mock_app_session_mutexed
        .lock()
        .unwrap()
        .get_node_attributes_for_path_result
        .is_some(); // Check if a preset was used
}

#[test]
fn test_treeview_item_toggled_queues_redraw_for_item_and_parents_on_new_status_change() {
    // Arrange
    let (mut logic, mock_app_session_mutexed, ..) = setup_logic_with_mocks();
    let window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(window_id);

    let root = PathBuf::from("/scan_root");
    let dir1_path = root.join("dir1");
    let file_in_dir1_path = dir1_path.join("file_in_dir1.txt");

    {
        let mut app_data = mock_app_session_mutexed.lock().unwrap();
        app_data.set_root_path_for_scan_for_mock(root.clone());
        app_data.set_snapshot_nodes_for_mock(vec![FileNode {
            path: dir1_path.clone(),
            name: "dir1".into(),
            is_dir: true,
            state: SelectionState::New,
            children: vec![FileNode {
                path: file_in_dir1_path.clone(),
                name: "file_in_dir1.txt".into(),
                is_dir: false,
                state: SelectionState::New,
                children: vec![],
                checksum: None,
            }],
            checksum: None,
        }]);
        // Mocking the "new" status: file is new, folder contains new
        app_data.set_does_path_or_descendants_contain_new_file_result(&file_in_dir1_path, true); // File itself
        app_data.set_does_path_or_descendants_contain_new_file_result(&dir1_path, true); // Folder because of child
    }

    let file_item_id = TreeItemId(10);
    let dir1_item_id = TreeItemId(11);

    logic.test_set_path_to_tree_item_id_mapping(file_in_dir1_path.clone(), file_item_id);
    logic.test_set_path_to_tree_item_id_mapping(dir1_path.clone(), dir1_item_id);

    // After toggling, the file is no longer New, and we'll mock that the directory no longer contains New files.
    // This setup is for *after* the state change occurs within MyAppLogic.
    mock_app_session_mutexed
        .lock()
        .unwrap()
        .set_does_path_or_descendants_contain_new_file_result(&file_in_dir1_path, false);
    mock_app_session_mutexed
        .lock()
        .unwrap()
        .set_does_path_or_descendants_contain_new_file_result(&dir1_path, false);

    // Act: Toggle the new file to Selected (no longer "New" for display)
    logic.handle_event(AppEvent::TreeViewItemToggledByUser {
        window_id,
        item_id: file_item_id,
        new_state: CheckState::Checked, // Becomes Selected
    });
    let cmds = logic.test_drain_commands();

    // Assert
    // Check for RedrawTreeItem for the file itself
    let mut redraw_file_found = false;
    let mut redraw_dir_found = false;
    for cmd in &cmds {
        if let PlatformCommand::RedrawTreeItem {
            item_id: cmd_item_id,
            ..
        } = cmd
        {
            if *cmd_item_id == file_item_id {
                redraw_file_found = true;
            }
            if *cmd_item_id == dir1_item_id {
                redraw_dir_found = true;
            }
        }
    }

    assert!(
        redraw_file_found,
        "Expected RedrawTreeItem for the toggled file. Got: {:?}",
        cmds
    );
    assert!(
        redraw_dir_found,
        "Expected RedrawTreeItem for the parent directory. Got: {:?}",
        cmds
    );
}
