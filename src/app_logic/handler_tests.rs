use super::handler::*;

use crate::core::{
    self, ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations,
    CoreConfigManagerForConfig, FileNode, FileState, FileSystemError, FileSystemScannerOperations,
    Profile, ProfileError, ProfileManagerOperations, StateManagerOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};
use tempfile::{NamedTempFile, tempdir};

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
// --- End MockConfigManager ---

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
// --- End MockProfileManager ---

// --- MockFileSystemScanner for testing ---
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
        FileSystemError::WalkDir(original_walkdir_error) => {
            let error_message = format!(
                "Mocked WalkDir error: path {:?}, depth {}, io_error: {:?}",
                original_walkdir_error.path(),
                original_walkdir_error.depth(),
                original_walkdir_error.io_error().map(|e| e.kind())
            );
            FileSystemError::Io(io::Error::new(io::ErrorKind::Other, error_message))
        }
        FileSystemError::InvalidPath(p) => FileSystemError::InvalidPath(p.clone()),
    }
}
// --- End MockFileSystemScanner ---

// --- MockArchiver for testing ---
struct MockArchiver {
    create_archive_content_result: Mutex<io::Result<String>>,
    create_archive_content_calls: Mutex<Vec<(Vec<FileNode>, PathBuf)>>,
    check_archive_status_result: Mutex<ArchiveStatus>,
    check_archive_status_calls: Mutex<Vec<(Profile, Vec<FileNode>)>>,
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
    fn get_check_archive_status_calls(&self) -> Vec<(Profile, Vec<FileNode>)> {
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
    fn create_archive_content(
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

    fn check_archive_status(
        &self,
        profile: &Profile,
        file_nodes_tree: &[FileNode],
    ) -> ArchiveStatus {
        self.check_archive_status_calls
            .lock()
            .unwrap()
            .push((profile.clone(), file_nodes_tree.to_vec()));
        *self.check_archive_status_result.lock().unwrap()
    }

    fn save_archive_content(&self, path: &Path, content: &str) -> io::Result<()> {
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
// --- End MockArchiver ---

// --- MockStateManager for testing ---
struct MockStateManager {
    apply_profile_to_tree_calls: Mutex<Vec<(Vec<FileNode>, Profile)>>,
    update_folder_selection_calls: Mutex<Vec<(FileNode, FileState)>>,
    // If you need to simulate changes to the FileNode tree,
    // the mock can store the tree and modify it, or take a closure.
    // For now, just recording calls.
}

impl MockStateManager {
    fn new() -> Self {
        MockStateManager {
            apply_profile_to_tree_calls: Mutex::new(Vec::new()),
            update_folder_selection_calls: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    fn get_apply_profile_to_tree_calls(&self) -> Vec<(Vec<FileNode>, Profile)> {
        self.apply_profile_to_tree_calls.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    fn get_update_folder_selection_calls(&self) -> Vec<(FileNode, FileState)> {
        self.update_folder_selection_calls.lock().unwrap().clone()
    }
}

impl StateManagerOperations for MockStateManager {
    fn apply_profile_to_tree(&self, tree: &mut Vec<FileNode>, profile: &Profile) {
        // For the mock, we'll record the call and also perform a simplified
        // version of the real logic to ensure the tree passed to subsequent
        // operations in MyAppLogic is in a somewhat expected state for tests.
        // A more advanced mock might allow configuring specific behaviors.
        self.apply_profile_to_tree_calls
            .lock()
            .unwrap()
            .push((tree.clone(), profile.clone()));

        // Simulate the real operation for test consistency:
        for node in tree.iter_mut() {
            if profile.selected_paths.contains(&node.path) {
                node.state = FileState::Selected;
            } else if profile.deselected_paths.contains(&node.path) {
                node.state = FileState::Deselected;
            } else {
                node.state = FileState::Unknown;
            }
            if node.is_dir && !node.children.is_empty() {
                // Recurse on a mutable borrow of children
                self.apply_profile_to_tree(&mut node.children, profile);
            }
        }
    }

    fn update_folder_selection(&self, node: &mut FileNode, new_state: FileState) {
        // Record the call
        self.update_folder_selection_calls
            .lock()
            .unwrap()
            .push((node.clone(), new_state));

        // Simulate the real operation for test consistency:
        node.state = new_state;
        if node.is_dir {
            for child in node.children.iter_mut() {
                self.update_folder_selection(child, new_state);
            }
        }
    }
}
// --- End MockStateManager ---

// Instantiate a MyAppLogic with all mocks.
// Return it, and the mocks to make it possible for tests to test.
fn setup_logic_with_mocks() -> (
    MyAppLogic,
    Arc<MockConfigManager>,
    Arc<MockProfileManager>,
    Arc<MockFileSystemScanner>,
    Arc<MockArchiver>,
    Arc<MockStateManager>,
) {
    let mock_config_manager_arc = Arc::new(MockConfigManager::new());
    let mock_profile_manager_arc = Arc::new(MockProfileManager::new());
    let mock_file_system_scanner_arc = Arc::new(MockFileSystemScanner::new());
    let mock_archiver_arc = Arc::new(MockArchiver::new());
    let mock_state_manager_arc = Arc::new(MockStateManager::new());

    let mut logic = MyAppLogic::new(
        Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>,
        Arc::clone(&mock_profile_manager_arc) as Arc<dyn ProfileManagerOperations>,
        Arc::clone(&mock_file_system_scanner_arc) as Arc<dyn FileSystemScannerOperations>,
        Arc::clone(&mock_archiver_arc) as Arc<dyn ArchiverOperations>,
        Arc::clone(&mock_state_manager_arc) as Arc<dyn StateManagerOperations>,
    );
    logic.test_set_main_window_id(Some(WindowId(1)));
    (
        logic,
        mock_config_manager_arc,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        mock_state_manager_arc,
    )
}

#[test]
fn test_on_main_window_created_loads_last_profile_with_all_mocks() {
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager,
        mock_file_system_scanner,
        mock_archiver,
        mock_state_manager,
    ) = setup_logic_with_mocks();

    let last_profile_name_to_load = "MyMockedStartupProfile";
    let startup_profile_root = PathBuf::from("/mock/startup_root");

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let mut selected_paths_for_profile = HashSet::new();
    selected_paths_for_profile.insert(startup_profile_root.join("mock_startup_file.txt"));

    let mock_loaded_profile = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: startup_profile_root.clone(),
        selected_paths: selected_paths_for_profile.clone(),
        deselected_paths: HashSet::new(),
        archive_path: None,
    };
    mock_profile_manager
        .set_load_profile_result(last_profile_name_to_load, Ok(mock_loaded_profile.clone()));

    let mock_scan_result = vec![FileNode::new(
        startup_profile_root.join("mock_startup_file.txt"),
        "mock_startup_file.txt".into(),
        false,
    )];
    mock_file_system_scanner
        .set_scan_directory_result(&startup_profile_root, Ok(mock_scan_result.clone()));

    mock_archiver.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    let _cmds = logic.on_main_window_created(WindowId(1));

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(last_profile_name_to_load)
    );
    assert!(logic.test_current_profile_cache().is_some());
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        last_profile_name_to_load
    );
    assert_eq!(*logic.test_root_path_for_scan(), startup_profile_root);

    // Verify MockStateManager was called for apply_profile_to_tree
    let apply_calls = mock_state_manager.get_apply_profile_to_tree_calls();
    assert_eq!(
        apply_calls.len(),
        1,
        "Expected apply_profile_to_tree to be called once"
    );
    assert_eq!(
        apply_calls[0].1.name, mock_loaded_profile.name,
        "Profile name mismatch in apply_profile_to_tree call"
    );

    assert_eq!(logic.test_file_nodes_cache().len(), 1);
    assert_eq!(
        logic.test_file_nodes_cache()[0].name,
        "mock_startup_file.txt"
    );
    assert_eq!(
        // This depends on MockStateManager correctly simulating the apply logic
        logic.test_file_nodes_cache()[0].state,
        FileState::Selected, // Because we added it to selected_paths_for_profile
        "FileNode state should be Selected after apply_profile_to_tree"
    );

    // Assertions for archive status and archiver interaction
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NotYetGenerated)
    );
    assert_eq!(mock_archiver.get_check_archive_status_calls().len(), 1);
    let (profile_arg, nodes_arg) = &mock_archiver.get_check_archive_status_calls()[0];
    assert_eq!(profile_arg.name, mock_loaded_profile.name);
    assert_eq!(nodes_arg.len(), mock_scan_result.len());
    if !nodes_arg.is_empty() {
        // Check the state of the node passed to check_archive_status
        assert_eq!(
            nodes_arg[0].state,
            FileState::Selected,
            "Node passed to check_archive_status should be Selected"
        );
    }
}

#[test]
fn test_on_main_window_created_no_last_profile_with_mocks() {
    let (
        mut logic,
        _mock_config_manager,
        _mock_profile_manager,
        mock_file_system_scanner,
        _mock_archiver,
        _mock_state_manager, // Present but not primary focus for this path's assertions
    ) = setup_logic_with_mocks();

    let default_scan_path = PathBuf::from(".");
    let mock_default_scan_file_path = default_scan_path.join("default_mock_scan_file.txt");

    let mock_default_scan_result = vec![FileNode::new(
        mock_default_scan_file_path.clone(),
        "default_mock_scan_file.txt".into(),
        false,
    )];
    mock_file_system_scanner
        .set_scan_directory_result(&default_scan_path, Ok(mock_default_scan_result.clone()));

    let _cmds = logic.on_main_window_created(WindowId(1));

    assert!(logic.test_current_profile_name().is_none());
    assert!(logic.test_current_profile_cache().is_none());
    assert_eq!(*logic.test_root_path_for_scan(), default_scan_path);

    let found_dummy_file = logic
        .test_file_nodes_cache()
        .iter()
        .any(|n| n.path == mock_default_scan_file_path);
    assert!(found_dummy_file);
    assert!(logic.test_current_archive_status().is_none());
}

#[test]
fn test_file_open_dialog_completed_updates_state_and_saves_last_profile() {
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        mock_state_manager,
    ) = setup_logic_with_mocks();

    let profile_to_load_name = "ProfileToLoadViaManager";
    let profile_root_for_scan = PathBuf::from("/mocked/profile/root/for/scan");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/path/to/{}.json", profile_to_load_name));

    let mut selected_paths_for_loaded_profile = HashSet::new();
    selected_paths_for_loaded_profile
        .insert(profile_root_for_scan.join("scanned_after_load_via_manager.txt"));

    let mock_loaded_profile = Profile {
        name: profile_to_load_name.to_string(),
        root_folder: profile_root_for_scan.clone(),
        selected_paths: selected_paths_for_loaded_profile.clone(),
        deselected_paths: HashSet::new(),
        archive_path: None,
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_loaded_profile.clone()),
    );

    let mock_scan_after_load_result = vec![FileNode::new(
        profile_root_for_scan.join("scanned_after_load_via_manager.txt"),
        "scanned_after_load_via_manager.txt".into(),
        false,
    )];
    mock_file_system_scanner_arc.set_scan_directory_result(
        &profile_root_for_scan,
        Ok(mock_scan_after_load_result.clone()),
    );

    mock_archiver_arc.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()),
    };
    let _cmds = logic.handle_event(event);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_load_name)
    );

    // Verify MockStateManager was called for apply_profile_to_tree
    let apply_calls = mock_state_manager.get_apply_profile_to_tree_calls();
    assert_eq!(apply_calls.len(), 1);
    assert_eq!(apply_calls[0].1.name, mock_loaded_profile.name);

    // Assert FileNode state after MockStateManager's apply_profile_to_tree
    assert_eq!(logic.test_file_nodes_cache().len(), 1);
    assert_eq!(
        logic.test_file_nodes_cache()[0].name,
        "scanned_after_load_via_manager.txt"
    );
    assert_eq!(
        // This depends on MockStateManager correctly simulating the apply logic
        logic.test_file_nodes_cache()[0].state,
        FileState::Selected,
        "FileNode state should be Selected after apply_profile_to_tree on profile load"
    );

    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    assert_eq!(saved_name_info.unwrap().1, profile_to_load_name);
    assert_eq!(mock_archiver_arc.get_check_archive_status_calls().len(), 1);
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NotYetGenerated)
    );
}

#[test]
fn test_file_save_dialog_completed_for_profile_saves_profile_via_manager() {
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager,
        _mock_file_system_scanner,
        mock_archiver,
        _mock_state_manager,
    ) = setup_logic_with_mocks();

    logic.test_root_path_for_scan_set(&PathBuf::from("/mock/profile/root"));

    let profile_to_save_name = "MyNewlySavedProfileViaManager";
    let profile_save_path_from_dialog = PathBuf::from(format!(
        "/dummy/path/to/{}.json",
        core::profiles::sanitize_profile_name(profile_to_save_name)
    ));

    logic.test_set_pending_action(PendingAction::SavingProfile);
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    let event = AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_save_path_from_dialog.clone()),
    };

    let _cmds = logic.handle_event(event);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_save_name)
    );
    assert!(logic.test_current_profile_cache().is_some());
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        profile_to_save_name
    );
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .root_folder,
        PathBuf::from("/mock/profile/root")
    );

    let save_calls = mock_profile_manager.get_save_profile_calls();
    assert_eq!(save_calls.len(), 1);
    assert_eq!(save_calls[0].0.name, profile_to_save_name);

    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    assert_eq!(saved_name_info.unwrap().1, profile_to_save_name);
    assert_eq!(mock_archiver.get_check_archive_status_calls().len(), 1);
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NotYetGenerated)
    );
}

#[test]
fn test_handle_button_click_generates_save_dialog_archive_with_mock_archiver() {
    let (mut logic, _, _, _, mock_archiver, _mock_state_manager) = setup_logic_with_mocks();

    let mock_content = "Mock archive content from archiver".to_string();
    mock_archiver.set_create_archive_content_result(Ok(mock_content.clone()));

    let cmds = logic.handle_event(AppEvent::ButtonClicked {
        window_id: WindowId(1),
        control_id: ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
    });
    assert_eq!(cmds.len(), 1, "Expected one command for save dialog");
    match &cmds[0] {
        PlatformCommand::ShowSaveFileDialog { .. } => {}
        _ => panic!("Expected ShowSaveFileDialog for archive"),
    }
    assert_eq!(
        logic.test_pending_archive_content().as_deref(),
        Some(mock_content.as_str())
    );
    assert_eq!(mock_archiver.get_create_archive_content_calls().len(), 1);
}

#[test]
fn test_handle_button_click_generate_archive_with_profile_context() {
    let (mut logic, _, _, _, mock_archiver, _mock_state_manager) = setup_logic_with_mocks();
    let temp_root_path = PathBuf::from("/mock/archive_button_root");
    let profile_name = "MyTestProfileForArchiveButton";
    let archive_file_path = temp_root_path.join("my_archive_for_button.txt");

    logic.test_set_current_profile_cache(Some(Profile {
        name: profile_name.to_string(),
        root_folder: temp_root_path.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_path.clone()),
    }));
    logic.test_root_path_for_scan_set(&temp_root_path);

    mock_archiver.set_create_archive_content_result(Ok("content".to_string()));

    let cmds = logic.handle_event(AppEvent::ButtonClicked {
        window_id: WindowId(1),
        control_id: ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
    });
    assert_eq!(cmds.len(), 1);
    match &cmds[0] {
        PlatformCommand::ShowSaveFileDialog {
            default_filename,
            initial_dir,
            ..
        } => {
            assert_eq!(
                *default_filename,
                format!(
                    "{}.txt",
                    core::profiles::sanitize_profile_name(&profile_name)
                )
            );
            assert_eq!(initial_dir.as_deref(), archive_file_path.parent());
        }
        _ => panic!("Expected ShowSaveFileDialog with profile context"),
    }
    assert_eq!(mock_archiver.get_create_archive_content_calls().len(), 1);
}

#[test]
fn test_handle_file_save_dialog_completed_for_archive_updates_profile_via_mock_archiver() {
    let (mut logic, _, mock_profile_manager, _, mock_archiver, _mock_state_manager) =
        setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingArchive);
    let mock_archive_data = "ARCHIVE CONTENT FOR MOCK ARCHIVER TEST".to_string();
    logic.test_set_pending_archive_content(mock_archive_data.clone());

    let archive_save_path = PathBuf::from("/mock/saved_archive_via_mock.txt");
    let temp_root_for_profile = PathBuf::from("/mock/profile_for_archive_save_mock");
    let profile_name_for_save = "test_profile_for_archive_save_via_mock_archiver";

    logic.test_set_current_profile_cache(Some(Profile::new(
        profile_name_for_save.into(),
        temp_root_for_profile.clone(),
    )));
    logic.test_root_path_for_scan_set(&temp_root_for_profile);

    mock_archiver.set_save_archive_content_result(Ok(()));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::UpToDate);

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: Some(archive_save_path.clone()),
    });

    assert!(cmds.is_empty());
    assert_eq!(*logic.test_pending_archive_content(), None);

    let save_calls_archiver = mock_archiver.get_save_archive_content_calls();
    assert_eq!(save_calls_archiver.len(), 1);
    assert_eq!(save_calls_archiver[0].0, archive_save_path);
    assert_eq!(save_calls_archiver[0].1, mock_archive_data);

    let cached_profile = logic.test_current_profile_cache().as_ref().unwrap();
    assert_eq!(
        cached_profile.archive_path.as_ref().unwrap(),
        &archive_save_path
    );

    let save_calls_profile_mgr = mock_profile_manager.get_save_profile_calls();
    assert_eq!(save_calls_profile_mgr.len(), 1);
    assert_eq!(
        save_calls_profile_mgr[0].0.archive_path,
        Some(archive_save_path.clone())
    );

    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::UpToDate)
    );
    assert_eq!(mock_archiver.get_check_archive_status_calls().len(), 1);
}

#[test]
fn test_handle_file_save_dialog_cancelled_for_archive() {
    let (mut logic, _, _, _, _mock_archiver, _mock_state_manager) = setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingArchive);
    logic.test_set_pending_archive_content("WILL BE CLEARED".to_string());

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: None,
    });

    assert!(cmds.is_empty());
    assert_eq!(*logic.test_pending_archive_content(), None);
    assert!(logic.test_pending_action().is_none());
}

#[test]
fn test_handle_file_save_dialog_cancelled_for_profile() {
    let (mut logic, _, _, _, _mock_archiver, _mock_state_manager) = setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingProfile);

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: None,
    });
    assert!(cmds.is_empty());
    assert!(logic.test_pending_action().is_none());
}

#[test]
fn test_handle_treeview_item_toggled_updates_archive_status_via_mock_archiver() {
    let (mut logic, _, _, _, mock_archiver, mock_state_manager) = setup_logic_with_mocks();

    let scan_root = PathBuf::from("/mock/scan_for_toggle_mock_archiver");
    logic.test_root_path_for_scan_set(&scan_root);

    let foo_path_relative = PathBuf::from("foo.txt");
    let foo_full_path = scan_root.join(&foo_path_relative);

    logic.test_set_file_nodes_cache(vec![FileNode::new(
        foo_full_path.clone(),
        "foo.txt".into(),
        false,
    )]);
    logic.test_set_current_profile_cache(Some(Profile {
        name: "test_profile_for_toggle_mock_archiver".into(),
        root_folder: scan_root.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(scan_root.join("archive.txt")),
    }));

    logic.test_path_to_tree_item_id_clear();
    let _descriptors = logic.build_tree_item_descriptors_recursive();
    let tree_item_id_for_foo = *logic
        .test_path_to_tree_item_id()
        .get(&foo_full_path)
        .unwrap();

    mock_archiver.set_check_archive_status_result(ArchiveStatus::OutdatedRequiresUpdate);

    let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
        window_id: WindowId(1),
        item_id: tree_item_id_for_foo,
        new_state: CheckState::Checked,
    });

    // Verify MockStateManager was called for update_folder_selection
    let update_calls = mock_state_manager.get_update_folder_selection_calls();
    assert_eq!(update_calls.len(), 1);
    assert_eq!(update_calls[0].0.path, foo_full_path); // Path of the node passed to update_folder_selection
    assert_eq!(update_calls[0].1, FileState::Selected); // New state passed

    assert_eq!(logic.test_file_nodes_cache()[0].state, FileState::Selected);
    assert_eq!(cmds.len(), 1);

    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::OutdatedRequiresUpdate)
    );
    assert_eq!(mock_archiver.get_check_archive_status_calls().len(), 1);
}

#[test]
fn test_profile_load_updates_archive_status_via_mock_archiver() {
    let (
        mut logic,
        _mock_config_manager,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        mock_state_manager, // Get the mock
    ) = setup_logic_with_mocks();

    let profile_name = "ProfileForStatusUpdateViaMockArchiver";
    let root_folder_for_profile = PathBuf::from("/mock/scan_root_status_mock_archiver");
    let archive_file_for_profile = PathBuf::from("/mock/my_mock_archiver_archive.txt");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/profiles/{}.json", profile_name));

    let mock_profile_to_load = Profile {
        name: profile_name.to_string(),
        root_folder: root_folder_for_profile.clone(),
        selected_paths: HashSet::new(), // Empty selected paths
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_for_profile.clone()),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_profile_to_load.clone()),
    );

    let scanned_file_nodes = vec![FileNode::new(
        root_folder_for_profile.join("some_file.txt"),
        "some_file.txt".into(),
        false,
    )];
    mock_file_system_scanner_arc
        .set_scan_directory_result(&root_folder_for_profile, Ok(scanned_file_nodes.clone()));

    mock_archiver_arc.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);

    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()),
    };
    let _cmds = logic.handle_event(event);

    // Verify MockStateManager was called for apply_profile_to_tree
    let apply_calls = mock_state_manager.get_apply_profile_to_tree_calls();
    assert_eq!(apply_calls.len(), 1);
    assert_eq!(apply_calls[0].1.name, mock_profile_to_load.name);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_name)
    );
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NoFilesSelected)
    );
    assert_eq!(mock_archiver_arc.get_check_archive_status_calls().len(), 1);
    let (profile_call_arg, nodes_call_arg) = &mock_archiver_arc.get_check_archive_status_calls()[0];
    assert_eq!(profile_call_arg.name, profile_name);
    assert_eq!(nodes_call_arg.len(), scanned_file_nodes.len());
    if !nodes_call_arg.is_empty() {
        assert_eq!(nodes_call_arg[0].name, scanned_file_nodes[0].name);
        // After MockStateManager.apply_profile_to_tree with empty selected_paths, state should be Unknown
        assert_eq!(
            nodes_call_arg[0].state,
            FileState::Unknown,
            "Node state passed to check_archive_status should be Unknown"
        );
    }
}

#[test]
fn test_handle_window_close_requested_generates_close_command() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    let cmds = logic.handle_event(AppEvent::WindowCloseRequested {
        window_id: WindowId(1),
    });
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], PlatformCommand::CloseWindow { .. }));
}

#[test]
fn test_handle_window_destroyed_clears_main_window_id_and_state() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_current_set(
        Some("Test".to_string()),
        Some(Profile::new("Test".into(), PathBuf::from("."))),
        Some(ArchiveStatus::UpToDate),
    );
    logic.test_file_nodes_cache().push(FileNode::new(
        PathBuf::from("./file"),
        "file".into(),
        false,
    ));
    logic.test_path_to_tree_item_id_insert(&PathBuf::from("./file"), TreeItemId(1));

    let cmds = logic.handle_event(AppEvent::WindowDestroyed {
        window_id: WindowId(1),
    });

    assert!(cmds.is_empty());
    assert_eq!(logic.test_main_window_id(), None);
    assert!(logic.test_current_profile_name().is_none());
    assert!(logic.test_current_profile_cache().is_none());
    assert!(logic.test_current_archive_status().is_none());
    assert!(logic.test_file_nodes_cache().is_empty());
    assert!(logic.test_path_to_tree_item_id().is_empty());
}

fn make_test_tree_for_applogic() -> Vec<FileNode> {
    let root_p = PathBuf::from("/root");
    let file1_p = root_p.join("file1.txt");
    let sub_p = root_p.join("sub");
    let file2_p = sub_p.join("file2.txt");
    let mut sub_node = FileNode::new(sub_p.clone(), "sub".into(), true);
    let file2_node = FileNode::new(file2_p.clone(), "file2.txt".into(), false);
    sub_node.children.push(file2_node);
    vec![
        FileNode::new(file1_p.clone(), "file1.txt".into(), false),
        sub_node,
    ]
}

#[test]
fn test_build_tree_item_descriptors_recursive_applogic() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    logic.test_path_to_tree_item_id_clear();
    let descriptors = logic.build_tree_item_descriptors_recursive();
    assert_eq!(descriptors.len(), 2);
    assert_eq!(descriptors[0].text, "file1.txt");
    assert!(!descriptors[0].is_folder);
    let file1_id = descriptors[0].id;
    assert_eq!(descriptors[1].text, "sub");
    assert!(descriptors[1].is_folder);
    assert_eq!(descriptors[1].children.len(), 1);
    let sub_id = descriptors[1].id;

    assert_eq!(descriptors[1].children[0].text, "file2.txt");
    assert!(!descriptors[1].children[0].is_folder);
    let file2_id = descriptors[1].children[0].id;

    let path_map = logic.test_path_to_tree_item_id();
    assert_eq!(
        path_map.get(&PathBuf::from("/root/file1.txt")),
        Some(&file1_id)
    );
    assert_eq!(path_map.get(&PathBuf::from("/root/sub")), Some(&sub_id));
    assert_eq!(
        path_map.get(&PathBuf::from("/root/sub/file2.txt")),
        Some(&file2_id)
    );
}

#[test]
fn test_find_filenode_mut_and_ref_applogic() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    let path_to_find = PathBuf::from("/root/sub/file2.txt");

    let found_ref = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &path_to_find);
    assert!(found_ref.is_some());
    assert_eq!(found_ref.unwrap().name, "file2.txt");

    let found_mut = logic.test_find_filenode_mut(&path_to_find);
    assert!(found_mut.is_some());
    if let Some(node) = found_mut {
        node.state = FileState::Selected;
    }
    let ref_after_mut = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &path_to_find);
    assert_eq!(ref_after_mut.unwrap().state, FileState::Selected);
}

#[test]
fn test_collect_visual_updates_recursive_applogic() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    let mut test_tree = make_test_tree_for_applogic();
    test_tree[0].state = FileState::Selected;
    test_tree[1].children[0].state = FileState::Deselected;

    logic.test_set_file_nodes_cache(test_tree);
    logic.test_path_to_tree_item_id_clear();
    let _descriptors = logic.build_tree_item_descriptors_recursive();

    let mut updates = Vec::new();
    for node_ref in logic.test_file_nodes_cache().clone().iter() {
        logic.collect_visual_updates_recursive(node_ref, &mut updates);
    }
    assert_eq!(updates.len(), 3);

    let path_map = logic.test_path_to_tree_item_id();

    let file1_id = path_map.get(&PathBuf::from("/root/file1.txt")).unwrap();
    let sub_id = path_map.get(&PathBuf::from("/root/sub")).unwrap();
    let file2_id = path_map.get(&PathBuf::from("/root/sub/file2.txt")).unwrap();

    assert!(updates.contains(&(*file1_id, CheckState::Checked)));
    assert!(updates.contains(&(*sub_id, CheckState::Unchecked)));
    assert!(updates.contains(&(*file2_id, CheckState::Unchecked)));

    assert_eq!(
        updates.len(),
        3,
        "Expected 3 updates for the 3 nodes in the test tree. Updates: {:?}",
        updates
    );
    assert_eq!(
        path_map.len(),
        3,
        "Expected 3 items in path_map. Map: {:?}",
        path_map
    );
}
