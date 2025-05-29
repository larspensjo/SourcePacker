// ===== File: C:\Users\larsp\src\source_packer\src\app_logic\handler_tests.rs =====

use super::handler::*;

use crate::core::{
    self, ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations,
    CoreConfigManagerForConfig, FileNode, FileState, FileSystemError, FileSystemScannerOperations,
    Profile, ProfileError, ProfileManagerOperations, StateManagerOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler,
    TreeItemDescriptor, TreeItemId, WindowId,
};

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};
use tempfile::{NamedTempFile, tempdir};

/*
 * This module contains unit tests for `MyAppLogic` from the `super::handler` module.
 * It utilizes mock implementations of core dependencies (`ConfigManagerOperations`,
 * `ProfileManagerOperations`, etc.) to isolate `MyAppLogic`'s behavior for testing.
 * Tests focus on event handling, state transitions, command generation (now via
 * dequeuing), and error paths.
 */

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
        FileSystemError::IgnoreError(original_ignore_error) => {
            let error_message = format!("Mocked IgnoreError: {:?}", original_ignore_error);
            let mock_io_err = io::Error::new(io::ErrorKind::Other, error_message);
            FileSystemError::IgnoreError(ignore::Error::from(mock_io_err))
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
        self.apply_profile_to_tree_calls
            .lock()
            .unwrap()
            .push((tree.clone(), profile.clone()));

        // Simulate the actual behavior for test consistency
        for node in tree.iter_mut() {
            if profile.selected_paths.contains(&node.path) {
                node.state = FileState::Selected;
            } else if profile.deselected_paths.contains(&node.path) {
                node.state = FileState::Deselected;
            } else {
                node.state = FileState::Unknown;
            }
            if node.is_dir && !node.children.is_empty() {
                self.apply_profile_to_tree(&mut node.children, profile);
            }
        }
    }

    fn update_folder_selection(&self, node: &mut FileNode, new_state: FileState) {
        self.update_folder_selection_calls
            .lock()
            .unwrap()
            .push((node.clone(), new_state));

        // Simulate the actual behavior for test consistency
        node.state = new_state;
        if node.is_dir {
            for child in node.children.iter_mut() {
                self.update_folder_selection(child, new_state);
            }
        }
    }
}
// --- End MockStateManager ---

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

// Helper to filter out debug status messages for command counting
fn get_functional_commands(cmds: &[PlatformCommand]) -> Vec<&PlatformCommand> {
    cmds.iter()
        .filter(|cmd| {
            !matches!(
                cmd,
                PlatformCommand::UpdateStatusBarText {
                    severity: MessageSeverity::Debug,
                    ..
                }
            )
        })
        .collect()
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
    let startup_archive_path = startup_profile_root.join("startup_archive.txt");

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let mut selected_paths_for_profile = HashSet::new();
    selected_paths_for_profile.insert(startup_profile_root.join("mock_startup_file.txt"));

    let mock_loaded_profile = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: startup_profile_root.clone(),
        selected_paths: selected_paths_for_profile.clone(),
        deselected_paths: HashSet::new(),
        archive_path: Some(startup_archive_path.clone()),
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

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(last_profile_name_to_load)
    );
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .archive_path
            .as_ref()
            .unwrap(),
        &startup_archive_path
    );

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        last_profile_name_to_load,
        startup_archive_path.display()
    );
    assert!(
        functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)),
        "Expected SetWindowTitle with profile and archive path. Got: {:?}", functional_cmds
    );
    assert!(
        functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, enabled: true, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )),
        "Expected SetControlEnabled (true) for save button. Got: {:?}", functional_cmds
    );

    let profile_loaded_msg_count = functional_cmds.iter().filter(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText {text, severity: MessageSeverity::Information, ..} if text.contains("Profile") && text.contains("loaded"))).count();
    // There are two "profile loaded" messages: one from on_main_window_created itself ("Successfully loaded last profile...")
    // and one from _activate_profile_and_show_window ("Profile '...' loaded.").
    assert_eq!(
        profile_loaded_msg_count, 2,
        "Expected two 'profile loaded' status messages. Got: {:?}",
        functional_cmds
    );

    // Functional commands:
    // 1. Info (Successfully loaded last profile...)
    // 2. SetWindowTitle
    // 3. PopulateTreeView
    // 4. Info (Profile '...' loaded. - from _activate_profile_and_show_window)
    // 5. Info (Archive: NotYetGenerated - from update_current_archive_status)
    // 6. SetControlEnabled (true)
    // 7. ShowWindow
    assert_eq!(
        functional_cmds.len(),
        7,
        "Expected 7 functional commands for successful profile load. Got: {:?}",
        functional_cmds
    );
}

#[test]
fn test_on_main_window_created_loads_profile_no_archive_path() {
    let (mut logic, mock_config_manager, mock_profile_manager, mock_fs_scanner, mock_archiver, _) =
        setup_logic_with_mocks();
    let profile_name = "ProfileSansArchive";
    mock_config_manager.set_load_last_profile_name_result(Ok(Some(profile_name.to_string())));
    let mock_profile = Profile {
        name: profile_name.to_string(),
        root_folder: PathBuf::from("/sans/archive"),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: None,
    };
    mock_profile_manager.set_load_profile_result(profile_name, Ok(mock_profile.clone()));
    mock_fs_scanner.set_scan_directory_result(&mock_profile.root_folder, Ok(vec![]));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    let expected_title = format!("SourcePacker - [{}] - [No Archive Set]", profile_name);
    assert!(
        functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)),
        "Expected SetWindowTitle indicating no archive path. Got: {:?}", functional_cmds
    );
    assert!(
        functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, enabled: false, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )),
        "Expected SetControlEnabled (false) for save button. Got: {:?}", functional_cmds
    );
    assert!(
        functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Information, .. } if text.contains("Button 'Save to Archive' disabled"))),
        "Expected status update for disabled button. Got: {:?}", functional_cmds
    );
    // Commands: Info(loaded), SetTitle, Populate, Info(profile loaded from activate), Info(archive status), SetCtrlEnabled(false), Info(button disabled), ShowWindow
    assert_eq!(
        functional_cmds.len(),
        8,
        "Expected 8 functional commands. Got: {:?}",
        functional_cmds
    );
}

#[test]
fn test_on_main_window_created_no_last_profile_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    mock_config_manager.set_load_last_profile_name_result(Ok(None));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Expected: Info (No last profile found), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { severity: MessageSeverity::Information, text, .. } if text.contains("No last profile name found"))));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::ShowProfileSelectionDialog {
            emphasize_create_new: true,
            ..
        }
    )));
    assert!(
        !cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowWindow { .. }))
    );
}

#[test]
fn test_on_main_window_created_no_last_profile_but_existing_profiles_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    mock_config_manager.set_load_last_profile_name_result(Ok(None));
    let existing_profiles = vec!["ProfileA".to_string(), "ProfileB".to_string()];
    mock_profile_manager.set_list_profiles_result(Ok(existing_profiles.clone()));

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Expected: Info (No last profile found), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { severity: MessageSeverity::Information, text, .. } if text.contains("No last profile name found"))));
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::ShowProfileSelectionDialog { emphasize_create_new: false, available_profiles, .. } if *available_profiles == existing_profiles)));
}

#[test]
fn test_on_main_window_created_load_last_profile_name_fails_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    let config_error = ConfigError::Io(io::Error::new(io::ErrorKind::Other, "config load failure"));
    mock_config_manager.set_load_last_profile_name_result(Err(config_error));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Expected: Error (Error loading last profile), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { severity: MessageSeverity::Error, text, .. } if text.contains("Error loading last profile name"))));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::ShowProfileSelectionDialog {
            emphasize_create_new: true,
            ..
        }
    )));
    assert!(
        !cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowWindow { .. }))
    );
}

#[test]
fn test_on_main_window_created_load_profile_fails_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    let last_profile_name = "ExistingButFailsToLoadProfile";
    mock_config_manager.set_load_last_profile_name_result(Ok(Some(last_profile_name.to_string())));
    let profile_error = ProfileError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        "profile not found physically",
    ));
    mock_profile_manager.set_load_profile_result(last_profile_name, Err(profile_error));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic._on_ui_setup_complete(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Expected: Error (Failed to load last profile), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { severity: MessageSeverity::Error, text, .. } if text.contains("Failed to load last profile") && text.contains(last_profile_name))));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::ShowProfileSelectionDialog {
            emphasize_create_new: true,
            ..
        }
    )));
    assert!(
        !cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowWindow { .. }))
    );
}

#[test]
fn test_profile_selection_dialog_completed_cancelled_quits_app() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: WindowId(1),
        chosen_profile_name: None,
        create_new_requested: false,
        user_cancelled: true,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);
    // Expected: Info (Profile selection cancelled), QuitApplication
    assert_eq!(
        functional_cmds.len(),
        2, // << UPDATED FROM 1 to 2
        "Expected 2 functional commands (Info + QuitApplication). Got: {:?}",
        functional_cmds
    );
    assert!(matches!(
        functional_cmds[0],
        PlatformCommand::UpdateStatusBarText {
            severity: MessageSeverity::Information,
            ..
        }
    ));
    assert!(matches!(
        functional_cmds[1],
        PlatformCommand::QuitApplication
    ));
}

#[test]
fn test_profile_selection_dialog_completed_chosen_profile_loads_and_activates() {
    let (mut logic, mock_config_manager, mock_profile_manager, mock_fs_scanner, mock_archiver, _) =
        setup_logic_with_mocks();
    let profile_name = "ChosenProfile";
    let profile_root = PathBuf::from("/chosen/root");
    let profile_archive_path = profile_root.join("chosen_archive.dat");
    let mock_profile = Profile {
        name: profile_name.to_string(),
        root_folder: profile_root.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(profile_archive_path.clone()),
    };
    mock_profile_manager.set_load_profile_result(profile_name, Ok(mock_profile.clone()));
    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(vec![]));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);

    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: WindowId(1),
        chosen_profile_name: Some(profile_name.to_string()),
        create_new_requested: false,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        profile_name,
        profile_archive_path.display()
    );
    // Commands: SetTitle, PopulateTree, Info(Profile '...' loaded), Info(Archive status), SetCtrlEnabled(true), ShowWindow
    assert_eq!(
        functional_cmds.len(),
        6,
        "Expected 6 functional commands. Got: {:?}",
        functional_cmds
    ); // Adjusted from 7
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)));
}

#[test]
fn test_profile_selection_dialog_completed_chosen_profile_load_fails_reinitiates_selection() {
    let (mut logic, _, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    let profile_name = "FailingProfile";
    mock_profile_manager.set_load_profile_result(
        profile_name,
        Err(ProfileError::ProfileNotFound(profile_name.to_string())),
    );
    mock_profile_manager.set_list_profiles_result(Ok(vec![]));
    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: WindowId(1),
        chosen_profile_name: Some(profile_name.to_string()),
        create_new_requested: false,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Error(Could not load profile), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    ); // Adjusted from 4
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Error, .. } if text.contains("Could not load profile"))));
    assert!(
        functional_cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowProfileSelectionDialog { .. }))
    );
}

#[test]
fn test_profile_selection_dialog_completed_create_new_starts_flow() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: WindowId(1),
        chosen_profile_name: None,
        create_new_requested: true,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: ShowInputDialog
    assert_eq!(
        functional_cmds.len(),
        1,
        "Expected 1 functional command. Got: {:?}",
        functional_cmds
    ); // Adjusted from 2
    match functional_cmds
        .iter()
        .find(|cmd| matches!(cmd, PlatformCommand::ShowInputDialog { .. }))
    {
        Some(PlatformCommand::ShowInputDialog {
            title, context_tag, ..
        }) => {
            assert!(title.contains("New Profile (1/2): Name"));
            assert_eq!(context_tag.as_deref(), Some("NewProfileName"));
        }
        _ => panic!(
            "Expected ShowInputDialog for new profile name. Got {:?}",
            functional_cmds // Use filtered commands for panic message
        ),
    }
    assert_eq!(
        logic.test_pending_action().as_ref().unwrap(),
        &PendingAction::CreatingNewProfileGetName
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_valid() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);
    let profile_name = "MyNewProfile";
    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: WindowId(1),
        text: Some(profile_name.to_string()),
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    assert_eq!(
        logic.test_pending_new_profile_name().as_deref(),
        Some(profile_name)
    );
    // Commands: ShowFolderPickerDialog (directly)
    assert_eq!(
        functional_cmds.len(),
        1,
        "Expected 1 functional command (ShowFolderPickerDialog). Got: {:?}",
        functional_cmds
    ); // Adjusted from 2
    match functional_cmds
        .iter()
        .find(|cmd| matches!(cmd, PlatformCommand::ShowFolderPickerDialog { .. }))
    {
        Some(PlatformCommand::ShowFolderPickerDialog { title, .. }) => {
            assert!(title.contains("New Profile (2/2): Select Root Folder"));
        }
        _ => panic!("Expected ShowFolderPickerDialog. Got {:?}", functional_cmds), // Changed from cmds
    }
    assert_eq!(
        logic.test_pending_action().as_ref().unwrap(),
        &PendingAction::CreatingNewProfileGetRoot
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_invalid() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);
    let invalid_name = "My/New/Profile";
    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: WindowId(1),
        text: Some(invalid_name.to_string()),
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Info(dialog complete), Warning(Invalid name), ShowInputDialog (retry)
    assert_eq!(
        functional_cmds.len(),
        2,
        "Expected 2 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Warning, .. } if text.contains("Invalid profile name"))));
    assert!(
        functional_cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowInputDialog { .. }))
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_cancelled() {
    let (mut logic, _, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);
    mock_profile_manager.set_list_profiles_result(Ok(vec![])); // For initiate_profile_selection
    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: WindowId(1),
        text: None,
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Debug(Input dialog completed), Info(New profile name cancelled), Debug(Initiating profile selection), ShowProfileSelectionDialog
    // Functional: Info(New profile name cancelled), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2, // << UPDATED FROM 1 to 2
        "Expected 2 functional commands (Info + ShowProfileSelectionDialog). Got: {:?}",
        functional_cmds
    );
    assert!(matches!(
        functional_cmds[0],
        PlatformCommand::UpdateStatusBarText {
            severity: MessageSeverity::Information,
            ..
        }
    ));
    assert!(matches!(
        functional_cmds[1],
        PlatformCommand::ShowProfileSelectionDialog { .. }
    ));
    assert!(logic.test_pending_action().is_none());
}

#[test]
fn test_folder_picker_dialog_completed_creates_profile_and_activates() {
    let (mut logic, _, mock_profile_manager, mock_fs_scanner, mock_archiver, _) =
        setup_logic_with_mocks();
    let profile_name = "NewlyCreatedProfile";
    let profile_root = PathBuf::from("/newly/created/root");
    logic.test_set_pending_new_profile_name(Some(profile_name.to_string()));
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetRoot);
    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(vec![]));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);
    logic.handle_event(AppEvent::FolderPickerDialogCompleted {
        window_id: WindowId(1),
        path: Some(profile_root.clone()),
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    let expected_title = format!("SourcePacker - [{}] - [No Archive Set]", profile_name);
    // Key Commands: SetTitle, PopulateTree, Info(profile '...' created and loaded), Info(Archive status), SetCtrlEnabled(false), Info(Button disabled), ShowWindow
    assert_eq!(
        functional_cmds.len(),
        7,
        "Expected 7 functional commands. Got: {:?}",
        functional_cmds
    ); // Adjusted from 9
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::SetControlEnabled { enabled: false, .. }
    )));
}

#[test]
fn test_folder_picker_dialog_completed_cancelled() {
    let (mut logic, _, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    logic.test_set_pending_new_profile_name(Some("TempName".to_string()));
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetRoot);
    mock_profile_manager.set_list_profiles_result(Ok(vec![])); // For initiate_profile_selection
    logic.handle_event(AppEvent::FolderPickerDialogCompleted {
        window_id: WindowId(1),
        path: None,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Debug(Folder picker completed), Info(Root folder selection cancelled), Debug(Initiating profile selection), ShowProfileSelectionDialog
    // Functional: Info(Root folder selection cancelled), ShowProfileSelectionDialog
    assert_eq!(
        functional_cmds.len(),
        2, // << UPDATED FROM 1 to 2
        "Expected 2 functional commands (Info + ShowProfileSelectionDialog). Got: {:?}",
        functional_cmds
    );
    assert!(matches!(
        functional_cmds[0],
        PlatformCommand::UpdateStatusBarText {
            severity: MessageSeverity::Information,
            ..
        }
    ));
    assert!(matches!(
        functional_cmds[1],
        PlatformCommand::ShowProfileSelectionDialog { .. }
    ));
    assert!(logic.test_pending_action().is_none());
    assert!(logic.test_pending_new_profile_name().is_none());
}

#[test]
fn test_initiate_profile_selection_failure_to_list_profiles() {
    let (mut logic, _, mock_profile_manager, _, _, _) = setup_logic_with_mocks();
    mock_profile_manager.set_list_profiles_result(Err(ProfileError::Io(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "cannot access profiles dir",
    ))));
    logic.initiate_profile_selection_or_creation(WindowId(1));
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Info(initiating selection), Error(failed to list)
    assert_eq!(
        functional_cmds.len(),
        1,
        "Expected 1 functional Error command. Got: {:?}",
        functional_cmds
    ); // Debug message filtered out
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::UpdateStatusBarText {
            severity: MessageSeverity::Error,
            ..
        }
    )));
}

#[test]
fn test_file_open_dialog_completed_updates_state_and_saves_last_profile() {
    let (
        mut logic,
        _,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        _,
    ) = setup_logic_with_mocks();
    let profile_to_load_name = "ProfileToLoadViaManager";
    let profile_root_for_scan = PathBuf::from("/mocked/profile/root/for/scan");
    let archive_path_for_loaded_profile = profile_root_for_scan.join("archive.dat");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/path/to/{}.json", profile_to_load_name));
    let mock_loaded_profile = Profile {
        name: profile_to_load_name.to_string(),
        root_folder: profile_root_for_scan.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_path_for_loaded_profile.clone()),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_loaded_profile.clone()),
    );
    mock_file_system_scanner_arc.set_scan_directory_result(&profile_root_for_scan, Ok(vec![]));
    mock_archiver_arc.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    let event = AppEvent::FileOpenProfileDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()),
    };
    logic.handle_event(event);
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_load_name)
    );

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        profile_to_load_name,
        archive_path_for_loaded_profile.display()
    );
    assert!(functional_cmds.iter().any(
        |cmd| matches!(cmd, PlatformCommand::SetWindowTitle {title, ..} if title == &expected_title)
    ));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::SetControlEnabled { enabled: true, .. }
    )));
    assert!(
        functional_cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowWindow { .. }))
    );
    // Key functional commands: SetTitle, PopulateTree, Status(Profile ... loaded and scanned), Status(archive), SetCtrlEnabled, ShowWindow
    assert_eq!(
        functional_cmds.len(),
        6,
        "Expected 6 functional commands. Got: {:?}",
        functional_cmds
    ); // Adjusted from 7
}

#[test]
fn test_handle_window_close_requested_generates_close_command() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    logic.handle_event(AppEvent::WindowCloseRequestedByUser {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Expect CloseWindow command directly from handler
    assert_eq!(
        functional_cmds.len(),
        1,
        "Expected 1 functional command (CloseWindow). Got: {:?}",
        functional_cmds
    );
    assert!(matches!(
        functional_cmds[0],
        PlatformCommand::CloseWindow { .. }
    ));
}

#[test]
fn test_menu_set_archive_path_cancelled() {
    let (mut logic, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = logic.test_main_window_id().unwrap();
    logic.test_set_current_profile_cache(Some(Profile::new("Test".into(), PathBuf::from("."))));
    logic.test_set_pending_action(PendingAction::SettingArchivePath);

    logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: main_window_id,
        result: None,
    });
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: Info(dialog complete), Info(Set archive path cancelled), SetControlEnabled(false), Info(button disabled)
    assert_eq!(
        functional_cmds.len(),
        3,
        "Expected 3 functional commands. Got: {:?}",
        functional_cmds
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Information, .. } if text.contains("Set archive path cancelled."))));
    assert!(functional_cmds.iter().any(|cmd| matches!(
        cmd,
        PlatformCommand::SetControlEnabled { enabled: false, .. }
    )));
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Information, .. } if text.contains("Button 'Save to Archive' disabled"))));
}

#[test]
fn test_profile_load_updates_archive_status_via_mock_archiver() {
    let (
        mut logic,
        _,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        _,
    ) = setup_logic_with_mocks();
    let profile_name = "ProfileForStatusUpdateViaMockArchiver";
    let root_folder_for_profile = PathBuf::from("/mock/scan_root_status_mock_archiver");
    let archive_file_for_profile = PathBuf::from("/mock/my_mock_archiver_archive.txt");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/profiles/{}.json", profile_name));
    let mock_profile_to_load = Profile {
        name: profile_name.to_string(),
        root_folder: root_folder_for_profile.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_for_profile.clone()),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_profile_to_load.clone()),
    );
    mock_file_system_scanner_arc.set_scan_directory_result(&root_folder_for_profile, Ok(vec![]));
    mock_archiver_arc.set_check_archive_status_result(ArchiveStatus::ErrorChecking(Some(
        io::ErrorKind::NotFound,
    )));

    let event = AppEvent::FileOpenProfileDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()),
    };
    logic.handle_event(event);
    let cmds = logic.test_drain_commands();
    let functional_cmds = get_functional_commands(&cmds);

    // Commands: SetTitle, PopulateTree, Status(Profile loaded and scanned), Status(archive Error), SetCtrlEnabled, ShowWindow
    assert_eq!(
        functional_cmds.len(),
        6,
        "Expected 6 functional commands. Got: {:?}",
        functional_cmds
    ); // Adjusted from 7
    assert!(
        functional_cmds
            .iter()
            .any(|cmd| matches!(cmd, PlatformCommand::ShowWindow { .. }))
    );
    assert!(functional_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateStatusBarText { text, severity: MessageSeverity::Error, .. } if text.contains("Archive: ErrorChecking"))));
}
