use super::handler::*;
use crate::app_logic::ui_constants;

use crate::core::{
    ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations, FileNode, FileState,
    FileSystemError, FileSystemScannerOperations, Profile, ProfileError, ProfileManagerOperations,
    StateManagerOperations, TokenCounterOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler, TreeItemId,
    WindowId, types::MenuAction,
};

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tempfile::{NamedTempFile, tempdir};

/*
 * This module contains unit tests for `MyAppLogic` from the `super::handler` module.
 * It utilizes mock implementations of core dependencies (`ConfigManagerOperations`,
 * `ProfileManagerOperations`, etc.) to isolate `MyAppLogic`'s behavior for testing.
 * Tests focus on event handling, state transitions, command generation (now via
 * dequeuing), and error paths.
 */

// --- Mock Structures (ConfigManager, ProfileManager, FileSystemScanner, Archiver, StateManager) ---
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
                node.state = FileState::New;
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

// --- MockTokenCounter for testing ---
struct MockTokenCounter {
    // Maps content string to a specific token count
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
    Arc<MockConfigManager>,
    Arc<MockProfileManager>,
    Arc<MockFileSystemScanner>,
    Arc<MockArchiver>,
    Arc<MockStateManager>,
    Arc<MockTokenCounter>,
) {
    crate::initialize_logging(); // Ensure logging is initialized for tests
    let mock_config_manager_arc = Arc::new(MockConfigManager::new());
    let mock_profile_manager_arc = Arc::new(MockProfileManager::new());
    let mock_file_system_scanner_arc = Arc::new(MockFileSystemScanner::new());
    let mock_archiver_arc = Arc::new(MockArchiver::new());
    let mock_state_manager_arc = Arc::new(MockStateManager::new());
    let mock_token_counter_arc = Arc::new(MockTokenCounter::new(1)); // Default to 1 token if not specified

    let logic = MyAppLogic::new(
        Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>,
        Arc::clone(&mock_profile_manager_arc) as Arc<dyn ProfileManagerOperations>,
        Arc::clone(&mock_file_system_scanner_arc) as Arc<dyn FileSystemScannerOperations>,
        Arc::clone(&mock_archiver_arc) as Arc<dyn ArchiverOperations>,
        Arc::clone(&mock_token_counter_arc) as Arc<dyn TokenCounterOperations>,
        Arc::clone(&mock_state_manager_arc) as Arc<dyn StateManagerOperations>,
    );
    (
        logic,
        mock_config_manager_arc,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        mock_state_manager_arc,
        mock_token_counter_arc,
    )
}

// Helper to check for specific commands, optionally checking properties.
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
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager,
        mock_file_system_scanner,
        mock_archiver,
        _mock_state_manager,
        mock_token_counter,
    ) = setup_logic_with_mocks();

    let last_profile_name_to_load = "MyMockedStartupProfile";
    let startup_profile_root = PathBuf::from("/mock/startup_root");
    let startup_archive_path = startup_profile_root.join("startup_archive.txt");

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let mut selected_paths_for_profile = HashSet::new();
    let mock_file_path = startup_profile_root.join("mock_startup_file.txt");
    selected_paths_for_profile.insert(mock_file_path.clone());

    let mock_loaded_profile = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: startup_profile_root.clone(),
        selected_paths: selected_paths_for_profile.clone(),
        deselected_paths: HashSet::new(),
        archive_path: Some(startup_archive_path.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager
        .set_load_profile_result(last_profile_name_to_load, Ok(mock_loaded_profile.clone()));

    let file_content_for_tokens = "token test content";
    let temp_dir = tempdir().unwrap();
    let (concrete_file_path, _temp_file_guard) =
        create_temp_file_with_content(&temp_dir, "startup_content", file_content_for_tokens);
    mock_token_counter.set_count_for_content(
        &format!("{}\n", file_content_for_tokens), // Add newline to match read content
        5,
    );

    let mut profile_selected_paths = HashSet::new();
    profile_selected_paths.insert(concrete_file_path.clone());

    let mock_loaded_profile_with_temp_file = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: temp_dir.path().to_path_buf(),
        selected_paths: profile_selected_paths,
        deselected_paths: HashSet::new(),
        archive_path: Some(startup_archive_path.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager.set_load_profile_result(
        last_profile_name_to_load,
        Ok(mock_loaded_profile_with_temp_file.clone()),
    );

    let mock_scan_result_nodes = vec![FileNode::new(
        concrete_file_path.clone(),
        "startup_content.txt".into(),
        false,
    )];

    mock_file_system_scanner
        .set_scan_directory_result(temp_dir.path(), Ok(mock_scan_result_nodes.clone()));

    mock_archiver.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

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
    assert_eq!(
        logic.test_current_token_count(),
        5,
        "Token count should be 5 from selected temp file as per mock"
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
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::PopulateTreeView { .. }
        ))
        .is_some(),
        "Expected PopulateTreeView command. Got: {:?}",
        cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some(),
        "Expected ShowWindow command. Got: {:?}",
        cmds
    );
    assert!(
        !find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )).is_some(),
        "SetControlEnabled for the old button should NOT be present. Got: {:?}", cmds
    );

    // Expect two status updates for "Tokens: 5" (one old, one new)
    // and two for the "Profile loaded" message.
    let token_status_text = "Tokens: 5";
    let profile_loaded_text = format!("Profile '{}' loaded.", last_profile_name_to_load);

    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == token_status_text && *severity == MessageSeverity::Information )).is_some(),
        "Expected general label UpdateLabelText for 'Tokens: 5'. Got: {:?}", cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == token_status_text )).is_some(),
        "Expected dedicated token label UpdateLabelText for 'Tokens: 5'. Got: {:?}", cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *text == profile_loaded_text && *severity == MessageSeverity::Information )).is_some(),
        "Expected UpdateLabelText for profile loaded. Got: {:?}", cmds
    );
}

#[test]
fn test_on_main_window_created_loads_profile_no_archive_path() {
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager,
        mock_fs_scanner,
        mock_archiver,
        _,
        _,
    ) = setup_logic_with_mocks();
    let profile_name = "ProfileSansArchive";
    let profile_root = PathBuf::from("/sans/archive");

    mock_config_manager.set_load_last_profile_name_result(Ok(Some(profile_name.to_string())));
    let mock_profile = Profile {
        name: profile_name.to_string(),
        root_folder: profile_root.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: None,
        file_details: HashMap::new(),
    };
    mock_profile_manager.set_load_profile_result(profile_name, Ok(mock_profile.clone()));
    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(vec![]));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected); // For the archive specific status

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    assert_eq!(
        logic.test_current_token_count(),
        0,
        "Token count should be 0 with no selected files"
    );

    let expected_title = format!("SourcePacker - [{}] - [No Archive Set]", profile_name);
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)).is_some(),
        "Expected SetWindowTitle indicating no archive path. Got: {:?}", cmds
    );
    assert!(
        !find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )).is_some(),
        "SetControlEnabled for the old button should NOT be present. Got: {:?}", cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some(),
        "Expected ShowWindow command. Got: {:?}",
        cmds
    );

    let token_status_text = "Tokens: 0";
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == token_status_text && *severity == MessageSeverity::Information )).is_some(),
        "Expected general label UpdateLabelText for 'Tokens: 0'. Got: {:?}", cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == token_status_text )).is_some(),
        "Expected dedicated token label UpdateLabelText for 'Tokens: 0'. Got: {:?}", cmds
    );
}
#[test]
fn test_on_main_window_created_no_last_profile_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _, _) =
        setup_logic_with_mocks();
    mock_config_manager.set_load_last_profile_name_result(Ok(None));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, text, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Information && text.contains("No last profile name found"))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog {
                emphasize_create_new: true,
                ..
            }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_none()
    );
}

#[test]
fn test_on_main_window_created_no_last_profile_but_existing_profiles_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _, _) =
        setup_logic_with_mocks();
    mock_config_manager.set_load_last_profile_name_result(Ok(None));
    let existing_profiles = vec!["ProfileA".to_string(), "ProfileB".to_string()];
    mock_profile_manager.set_list_profiles_result(Ok(existing_profiles.clone()));

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, text, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Information && text.contains("No last profile name found"))).is_some());
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::ShowProfileSelectionDialog { emphasize_create_new: false, available_profiles: ap, .. } if *ap == existing_profiles)).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_none()
    );
}

#[test]
fn test_on_main_window_created_load_last_profile_name_fails_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _, _) =
        setup_logic_with_mocks();
    let config_error = ConfigError::Io(io::Error::new(io::ErrorKind::Other, "config load failure"));
    mock_config_manager.set_load_last_profile_name_result(Err(config_error));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, text, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text.contains("Error loading last profile name"))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog {
                emphasize_create_new: true,
                ..
            }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_none()
    );
}

#[test]
fn test_on_main_window_created_load_profile_fails_triggers_initiate_flow() {
    let (mut logic, mock_config_manager, mock_profile_manager, _, _, _, _) =
        setup_logic_with_mocks();
    let last_profile_name = "ExistingButFailsToLoadProfile";
    mock_config_manager.set_load_last_profile_name_result(Ok(Some(last_profile_name.to_string())));
    let profile_error = ProfileError::Io(io::Error::new(
        io::ErrorKind::NotFound,
        "profile not found physically",
    ));
    mock_profile_manager.set_load_profile_result(last_profile_name, Err(profile_error));
    mock_profile_manager.set_list_profiles_result(Ok(Vec::new()));

    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: WindowId(1),
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, text, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text.contains("Failed to load last profile") && text.contains(last_profile_name))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog {
                emphasize_create_new: true,
                ..
            }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_none()
    );
}

#[test]
fn test_profile_selection_dialog_completed_cancelled_quits_app() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    logic.test_set_main_window_id_and_init_ui_state(WindowId(1));
    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: WindowId(1),
        chosen_profile_name: None,
        create_new_requested: false,
        user_cancelled: true,
    });
    let cmds = logic.test_drain_commands();
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::QuitApplication)).is_some(),
        "Expected QuitApplication command. Got: {:?}",
        cmds
    );
    assert_eq!(cmds.len(), 1, "Expected only QuitApplication command.");
}

#[test]
fn test_profile_selection_dialog_completed_chosen_profile_loads_and_activates() {
    let (
        mut logic,
        _mock_config_manager,
        mock_profile_manager,
        mock_fs_scanner,
        mock_archiver,
        _,
        _mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "ChosenProfile";
    let profile_root = PathBuf::from("/chosen/root");
    let profile_archive_path = profile_root.join("chosen_archive.dat");
    let mock_profile = Profile {
        name: profile_name.to_string(),
        root_folder: profile_root.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(profile_archive_path.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager.set_load_profile_result(profile_name, Ok(mock_profile.clone()));
    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(vec![])); // No files, so token count = 0
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);

    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: main_window_id,
        chosen_profile_name: Some(profile_name.to_string()),
        create_new_requested: false,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_name)
    );
    assert_eq!(logic.test_current_token_count(), 0);

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        profile_name,
        profile_archive_path.display()
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::PopulateTreeView { .. }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some()
    );
    // Check for SetControlEnabled is removed as menu item state isn't managed by this command now
    assert!(
        !find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )).is_some(),
        "SetControlEnabled for the old button should NOT be present. Got: {:?}", cmds
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 0" && *severity == MessageSeverity::Information )).is_some());
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == "Tokens: 0")
        ).is_some(),
        "Expected dedicated token label for 'Tokens: 0'. Got: {:?}", cmds
    );
}

#[test]
fn test_profile_selection_dialog_completed_chosen_profile_load_fails_reinitiates_selection() {
    let (mut logic, _, mock_profile_manager, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "FailingProfile";
    mock_profile_manager.set_load_profile_result(
        profile_name,
        Err(ProfileError::ProfileNotFound(profile_name.to_string())),
    );
    mock_profile_manager.set_list_profiles_result(Ok(vec![])); // Ensure it attempts to show dialog again
    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: main_window_id,
        chosen_profile_name: Some(profile_name.to_string()),
        create_new_requested: false,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text.contains("Could not load profile"))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog { .. }
        ))
        .is_some()
    );
}

#[test]
fn test_profile_selection_dialog_completed_create_new_starts_flow() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    logic.handle_event(AppEvent::ProfileSelectionDialogCompleted {
        window_id: main_window_id,
        chosen_profile_name: None,
        create_new_requested: true,
        user_cancelled: false,
    });
    let cmds = logic.test_drain_commands();

    let input_dialog_cmd = find_command(&cmds, |cmd| {
        matches!(cmd, PlatformCommand::ShowInputDialog { .. })
    });
    assert!(
        input_dialog_cmd.is_some(),
        "Expected ShowInputDialog command"
    );
    if let Some(PlatformCommand::ShowInputDialog {
        title, context_tag, ..
    }) = input_dialog_cmd
    {
        assert!(title.contains("New Profile (1/2): Name"));
        assert_eq!(context_tag.as_deref(), Some("NewProfileName"));
    }
    assert_eq!(
        logic.test_pending_action(),
        Some(PendingAction::CreatingNewProfileGetName).as_ref()
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_valid() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);

    let profile_name = "MyNewProfile";
    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: main_window_id,
        text: Some(profile_name.to_string()),
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();

    assert_eq!(
        logic.test_pending_new_profile_name().map(|s| s.as_str()),
        Some(profile_name)
    );
    let folder_picker_cmd = find_command(&cmds, |cmd| {
        matches!(cmd, PlatformCommand::ShowFolderPickerDialog { .. })
    });
    assert!(
        folder_picker_cmd.is_some(),
        "Expected ShowFolderPickerDialog command"
    );
    if let Some(PlatformCommand::ShowFolderPickerDialog { title, .. }) = folder_picker_cmd {
        assert!(title.contains("New Profile (2/2): Select Root Folder"));
    }
    assert_eq!(
        logic.test_pending_action(),
        Some(PendingAction::CreatingNewProfileGetRoot).as_ref()
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_invalid() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);

    let invalid_name = "My/New/Profile";
    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: main_window_id,
        text: Some(invalid_name.to_string()),
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();

    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Warning && text.contains("Invalid profile name"))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowInputDialog { .. }
        ))
        .is_some()
    );
    assert_eq!(
        logic.test_pending_action(),
        Some(PendingAction::CreatingNewProfileGetName).as_ref()
    );
}

#[test]
fn test_input_dialog_completed_for_new_profile_name_cancelled() {
    let (mut logic, _, mock_profile_manager, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetName);
    mock_profile_manager.set_list_profiles_result(Ok(vec![]));

    logic.handle_event(AppEvent::GenericInputDialogCompleted {
        window_id: main_window_id,
        text: None,
        context_tag: Some("NewProfileName".to_string()),
    });
    let cmds = logic.test_drain_commands();

    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog { .. }
        ))
        .is_some()
    );
    assert!(logic.test_pending_action().is_none());
}

#[test]
fn test_folder_picker_dialog_completed_creates_profile_and_activates() {
    let (
        mut logic,
        _,
        mock_profile_manager,
        mock_fs_scanner,
        mock_archiver,
        _,
        _mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "NewlyCreatedProfile";
    let profile_root = PathBuf::from("/newly/created/root");
    logic.test_set_pending_new_profile_name(Some(profile_name.to_string()));
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetRoot);

    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(vec![]));
    mock_archiver.set_check_archive_status_result(ArchiveStatus::NoFilesSelected);

    logic.handle_event(AppEvent::FolderPickerDialogCompleted {
        window_id: main_window_id,
        path: Some(profile_root.clone()),
    });
    let cmds = logic.test_drain_commands();

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_name)
    );
    assert!(
        mock_profile_manager
            .get_save_profile_calls()
            .iter()
            .any(|(p, _)| p.name == profile_name)
    );
    assert_eq!(logic.test_current_token_count(), 0);

    let expected_title = format!("SourcePacker - [{}] - [No Archive Set]", profile_name);
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle { title, .. } if title == &expected_title)).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::PopulateTreeView { .. }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some()
    );
    // Check for SetControlEnabled is removed
    assert!(
        !find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )).is_some(),
        "SetControlEnabled for the old button should NOT be present. Got: {:?}", cmds
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text,..} if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text.contains(&format!("New profile '{}' created and loaded.", profile_name)))).is_some());
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 0" && *severity == MessageSeverity::Information )).is_some());
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == "Tokens: 0")
        ).is_some()
    );
}

#[test]
fn test_folder_picker_dialog_completed_cancelled() {
    let (mut logic, _, mock_profile_manager, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_pending_new_profile_name(Some("TempName".to_string()));
    logic.test_set_pending_action(PendingAction::CreatingNewProfileGetRoot);
    mock_profile_manager.set_list_profiles_result(Ok(vec![]));

    logic.handle_event(AppEvent::FolderPickerDialogCompleted {
        window_id: main_window_id,
        path: None,
    });
    let cmds = logic.test_drain_commands();

    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog { .. }
        ))
        .is_some()
    );
    assert!(logic.test_pending_action().is_none());
    assert!(logic.test_pending_new_profile_name().is_none());
}

#[test]
fn test_initiate_profile_selection_failure_to_list_profiles() {
    let (mut logic, _, mock_profile_manager, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    mock_profile_manager.set_list_profiles_result(Err(ProfileError::Io(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "cannot access profiles dir",
    ))));
    logic.initiate_profile_selection_or_creation(main_window_id);
    let cmds = logic.test_drain_commands();
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, text,.. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error && text.contains("Failed to list profiles"))).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowProfileSelectionDialog { .. }
        ))
        .is_none()
    );
}

#[test]
fn test_file_open_dialog_completed_updates_state_and_saves_last_profile() {
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        _,
        mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_to_load_name = "ProfileToLoadViaManager";
    let profile_root_for_scan = PathBuf::from("/mocked/profile/root/for/scan");
    let archive_path_for_loaded_profile = profile_root_for_scan.join("archive.dat");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/path/to/{}.json", profile_to_load_name));

    let temp_dir = tempdir().unwrap();
    let file_content_for_tokens = "opened file tokens content";
    let (concrete_file_path, _temp_file_guard) =
        create_temp_file_with_content(&temp_dir, "opened_content", file_content_for_tokens);
    mock_token_counter.set_count_for_content(
        &format!("{}\n", file_content_for_tokens), // Add newline to match read content
        7,
    );

    let mut selected_paths = HashSet::new();
    selected_paths.insert(concrete_file_path.clone());

    let mock_loaded_profile = Profile {
        name: profile_to_load_name.to_string(),
        root_folder: temp_dir.path().to_path_buf(),
        selected_paths,
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_path_for_loaded_profile.clone()),
        file_details: HashMap::new(),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_loaded_profile.clone()),
    );
    let scanned_nodes = vec![FileNode::new(
        concrete_file_path.clone(),
        "opened_content.txt".into(),
        false,
    )];
    mock_file_system_scanner_arc.set_scan_directory_result(temp_dir.path(), Ok(scanned_nodes));
    mock_archiver_arc.set_check_archive_status_result(ArchiveStatus::NotYetGenerated);

    let event = AppEvent::FileOpenProfileDialogCompleted {
        window_id: main_window_id,
        result: Some(profile_json_path_from_dialog.clone()),
    };
    logic.handle_event(event);
    let cmds = logic.test_drain_commands();

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_load_name)
    );
    assert_eq!(
        mock_config_manager.get_saved_profile_name(),
        Some((
            APP_NAME_FOR_PROFILES.to_string(),
            profile_to_load_name.to_string()
        ))
    );
    assert_eq!(
        logic.test_current_token_count(),
        7,
        "Token count should be 7 as per mock"
    );

    let expected_title = format!(
        "SourcePacker - [{}] - [{}]",
        profile_to_load_name,
        archive_path_for_loaded_profile.display()
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetWindowTitle {title, ..} if title == &expected_title)).is_some());
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::PopulateTreeView { .. }
        ))
        .is_some()
    );
    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some()
    );
    // Check for SetControlEnabled is removed
    assert!(
        !find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, .. } if *control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC )).is_some(),
        "SetControlEnabled for the old button should NOT be present. Got: {:?}", cmds
    );
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText {label_id, text, ..} if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text.contains(&format!("Profile '{}' loaded and scanned.", profile_to_load_name)))).is_some());
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 7" && *severity == MessageSeverity::Information )).is_some());
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == "Tokens: 7")
        ).is_some()
    );
}

#[test]
fn test_handle_window_close_requested_generates_close_command() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    logic.handle_event(AppEvent::WindowCloseRequestedByUser {
        window_id: main_window_id,
    });
    let cmds = logic.test_drain_commands();
    assert!(find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::CloseWindow { window_id: id, .. } if *id == main_window_id )).is_some());
    assert_eq!(cmds.len(), 1);
}

#[test]
fn test_menu_set_archive_path_cancelled() {
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_current_profile_cache(Some(Profile::new("Test".into(), PathBuf::from("."))));
    logic.test_set_pending_action(PendingAction::SettingArchivePath);

    logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: main_window_id,
        result: None,
    });
    let _cmds = logic.test_drain_commands(); // Drain commands to avoid interference
    // No specific command is expected here now other than potential status updates.
    // For example, if _update_generate_archive_menu_item_state logged something via app_info.
    // If it only logs via log::debug, then functional_cmds would likely be empty.
}

#[test]
fn test_profile_load_updates_archive_status_via_mock_archiver() {
    let (
        mut logic,
        _mock_config_manager,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
        mock_archiver_arc,
        _,
        _mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

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
        file_details: HashMap::new(),
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_profile_to_load.clone()),
    );
    mock_file_system_scanner_arc.set_scan_directory_result(&root_folder_for_profile, Ok(vec![]));

    let archive_error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound));
    mock_archiver_arc.set_check_archive_status_result(archive_error_status.clone());

    let event = AppEvent::FileOpenProfileDialogCompleted {
        window_id: main_window_id,
        result: Some(profile_json_path_from_dialog.clone()),
    };
    logic.handle_event(event);
    let cmds = logic.test_drain_commands();

    assert!(
        find_command(&cmds, |cmd| matches!(
            cmd,
            PlatformCommand::ShowWindow { .. }
        ))
        .is_some()
    );

    // Verify specific archive status message
    // For the dedicated label, we expect the plain English string.
    let archive_status_text_for_dedicated_label = "Archive: Error: file not found.".to_string();
    let archive_status_text_for_general_status =
        format!("Archive status error: {:?}", archive_error_status); // General label uses Debug format.

    // 1. Dedicated archive label
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
                if *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID &&
                   text == &archive_status_text_for_dedicated_label &&
                   *severity == MessageSeverity::Error
            )
        )
        .is_some(),
        "Expected dedicated archive label update for error. Got: {:?}",
        cmds
    );
    // 2. General status (new general label) via app_error!
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
                if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID &&
                   *severity == MessageSeverity::Error &&
                   text == &archive_status_text_for_general_status
            )
        )
        .is_some(),
        "Expected new general label error for archive. Got: {:?}",
        cmds
    );

    // Verify token count message (will be 0)
    let token_text = "Tokens: 0";
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == token_text && *severity == MessageSeverity::Information )).is_some(),
        "Expected UpdateLabelText for general label for 'Tokens: 0'. Got: {:?}", cmds
    );
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID && text == token_text && *severity == MessageSeverity::Information )).is_some(),
        "Expected UpdateLabelText for dedicated token label for 'Tokens: 0'. Got: {:?}", cmds
    );
}

#[test]
fn test_token_count_updates_on_tree_item_toggle() {
    // Arrange
    let (mut logic, _, _, _, _, _mock_state_manager, mock_token_counter) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let temp_dir = tempdir().unwrap();
    let content1 = "hello world";
    let content2 = "another example test";

    let (file1_path, _temp_file1_guard) =
        create_temp_file_with_content(&temp_dir, "file1", content1);
    let (file2_path, _temp_file2_guard) =
        create_temp_file_with_content(&temp_dir, "file2", content2);

    mock_token_counter.set_count_for_content(
        &format!("{}\n", content1), // Add newline
        2,
    );
    mock_token_counter.set_count_for_content(
        &format!("{}\n", content2), // Add newline
        3,
    );

    let node1_item_id = TreeItemId(101);
    let node2_item_id = TreeItemId(102);

    let file_nodes = vec![
        FileNode {
            path: file1_path.clone(),
            name: "file1.txt".to_string(),
            is_dir: false,
            state: FileState::Selected,
            children: Vec::new(),
            checksum: None,
        },
        FileNode {
            path: file2_path.clone(),
            name: "file2.txt".to_string(),
            is_dir: false,
            state: FileState::Deselected,
            children: Vec::new(),
            checksum: None,
        },
    ];
    logic.test_set_file_nodes_cache(file_nodes);
    logic.test_path_to_tree_item_id_insert(&file1_path, node1_item_id);
    logic.test_path_to_tree_item_id_insert(&file2_path, node2_item_id);

    // Act: Initial token count update
    logic._update_token_count_and_request_display();

    // Assert: Initial token count (only file1 selected)
    assert_eq!(
        logic.test_current_token_count(),
        2,
        "Initial count should be for file1 only (2 tokens)"
    );
    let initial_cmds = logic.test_drain_commands();
    assert!(
        initial_cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 2" && *severity == MessageSeverity::Information)),
        "Expected UpdateLabelText for 'Tokens: 2'. Got: {:?}", initial_cmds
    );

    // Act: Toggle file2 to be selected
    logic.handle_event(AppEvent::TreeViewItemToggledByUser {
        window_id: main_window_id,
        item_id: node2_item_id,
        new_state: CheckState::Checked,
    });

    // Assert: Token count after file2 is selected (file1 + file2)
    assert_eq!(
        logic.test_current_token_count(),
        5, // 2 (file1) + 3 (file2)
        "Count should be 2 (file1) + 3 (file2) = 5"
    );
    let cmds_after_toggle1 = logic.test_drain_commands();
    assert!(
        cmds_after_toggle1.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 5" && *severity == MessageSeverity::Information)),
        "Expected UpdateLabelText for 'Tokens: 5'. Got: {:?}", cmds_after_toggle1
    );

    // Act: Toggle file1 to be unselected
    logic.handle_event(AppEvent::TreeViewItemToggledByUser {
        window_id: main_window_id,
        item_id: node1_item_id,
        new_state: CheckState::Unchecked,
    });

    // Assert: Token count after file1 is unselected (only file2)
    assert_eq!(
        logic.test_current_token_count(),
        3,
        "Count should be 3 (file2 only)"
    );
    let cmds_after_toggle2 = logic.test_drain_commands();
    assert!(
        cmds_after_toggle2.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == "Tokens: 3" && *severity == MessageSeverity::Information)),
        "Expected UpdateLabelText for 'Tokens: 3'. Got: {:?}", cmds_after_toggle2
    );
}

#[test]
fn test_token_count_updates_on_profile_activation() {
    // Arrange
    let (
        mut logic,
        mock_config_manager,
        mock_profile_manager,
        mock_fs_scanner,
        _,
        _,
        mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);

    let temp_dir = tempdir().unwrap();
    let content_a = "alpha beta gamma";
    let content_b = "delta epsilon";
    let (file_a_path, _temp_file_a_guard) =
        create_temp_file_with_content(&temp_dir, "fileA", content_a);
    let (file_b_path, _temp_file_b_guard) =
        create_temp_file_with_content(&temp_dir, "fileB", content_b);

    mock_token_counter.set_count_for_content(&format!("{}\n", content_a), 10); // Add newline
    mock_token_counter.set_count_for_content(&format!("{}\n", content_b), 5); // Add newline

    let profile_name = "TestProfileForTokenActivation";
    let profile_root = temp_dir.path().to_path_buf();

    let mut selected_paths = HashSet::new();
    selected_paths.insert(file_a_path.clone());

    let profile = Profile {
        name: profile_name.to_string(),
        root_folder: profile_root.clone(),
        selected_paths,
        deselected_paths: HashSet::new(),
        archive_path: None,
        file_details: HashMap::new(),
    };

    mock_config_manager.set_load_last_profile_name_result(Ok(Some(profile_name.to_string())));
    mock_profile_manager.set_load_profile_result(profile_name, Ok(profile.clone()));

    let scanned_nodes = vec![
        FileNode {
            path: file_a_path.clone(),
            name: "fileA.txt".to_string(),
            is_dir: false,
            state: FileState::New,
            children: Vec::new(),
            checksum: None,
        },
        FileNode {
            path: file_b_path.clone(),
            name: "fileB.txt".to_string(),
            is_dir: false,
            state: FileState::New,
            children: Vec::new(),
            checksum: None,
        },
    ];
    mock_fs_scanner.set_scan_directory_result(&profile_root, Ok(scanned_nodes));

    // Act
    logic.handle_event(AppEvent::MainWindowUISetupComplete {
        window_id: main_window_id,
    });

    // Assert
    assert_eq!(
        logic.test_current_token_count(),
        10,
        "Token count should be for file_a (10 tokens as per mock)"
    );

    let cmds = logic.test_drain_commands();
    let token_text = "Tokens: 10";
    assert!(
        cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == token_text && *severity == MessageSeverity::Information)),
        "Expected UpdateLabelText for 'Tokens: 10'. Got: {:?}", cmds
    );
}

#[test]
fn test_token_count_handles_file_read_errors_gracefully_and_displays() {
    // Arrange
    let (mut logic, _, _, _, _, _, mock_token_counter) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let temp_dir = tempdir().unwrap();
    let readable_content = "one two three four";
    let (readable_file_path, _temp_readable_guard) =
        create_temp_file_with_content(&temp_dir, "readable", readable_content);
    let unreadable_file_path = temp_dir.path().join("non_existent_file.txt");

    mock_token_counter.set_count_for_content(&format!("{}\n", readable_content), 12); // Add newline

    let file_nodes = vec![
        FileNode {
            path: readable_file_path.clone(),
            name: "readable.txt".to_string(),
            is_dir: false,
            state: FileState::Selected,
            children: Vec::new(),
            checksum: None,
        },
        FileNode {
            path: unreadable_file_path.clone(),
            name: "non_existent_file.txt".to_string(),
            is_dir: false,
            state: FileState::Selected,
            children: Vec::new(),
            checksum: None,
        },
    ];
    logic.test_set_file_nodes_cache(file_nodes);

    // Act
    logic._update_token_count_and_request_display();

    // Assert
    assert_eq!(
        logic.test_current_token_count(),
        12,
        "Token count should only include readable_file.txt (12 tokens as per mock)"
    );

    let cmds = logic.test_drain_commands();
    let token_text = "Tokens: 12";
    assert!(
        cmds.iter().any(|cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. } if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && text == token_text && *severity == MessageSeverity::Information)),
        "Expected UpdateLabelText 'Tokens: 12'. Got: {:?}", cmds
    );
}

#[test]
fn test_menu_action_generate_archive_triggers_logic() {
    // Arrange
    let (mut logic, _, _, _, mock_archiver, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile_name = "ArchiveTestProfile";
    let archive_path = PathBuf::from("/test/archive.txt");
    let profile = Profile {
        name: profile_name.to_string(),
        root_folder: PathBuf::from("/test/root"),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_path.clone()),
        file_details: HashMap::new(),
    };
    logic.test_set_current_profile_cache(Some(profile.clone()));
    logic.test_set_file_nodes_cache(vec![FileNode::new(
        PathBuf::from("/test/root/file.txt"),
        "file.txt".into(),
        false,
    )]);

    mock_archiver.set_create_archive_content_result(Ok("Test Archive Content".to_string()));
    mock_archiver.set_save_archive_content_result(Ok(()));
    // When update_current_archive_status is called after saving, let's say it's UpToDate
    mock_archiver.set_check_archive_status_result(ArchiveStatus::UpToDate);

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
    });

    // Assert
    let create_calls = mock_archiver.get_create_archive_content_calls();
    assert_eq!(
        create_calls.len(),
        1,
        "create_archive_content should be called once"
    );

    let save_calls = mock_archiver.get_save_archive_content_calls();
    assert_eq!(
        save_calls.len(),
        1,
        "save_archive_content should be called once"
    );
    assert_eq!(save_calls[0].0, archive_path);
    assert_eq!(save_calls[0].1, "Test Archive Content");

    let cmds = logic.test_drain_commands();
    // Corrected success_text to match the actual app_info! message
    let success_text = format!(
        "Archive saved to '{}'.", // "successfully" removed
        archive_path.display()
    );
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
            if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Information && text == &success_text)
        )
        .is_some(),
        "Expected new label success message. Got: {:?}",
        cmds
    );

    // Also verify the archive status update command
    let archive_status_update_text = "Archive: Up to date.".to_string();
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
            if *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID && severity == &MessageSeverity::Information && text == &archive_status_update_text)
        )
        .is_some(),
        "Expected archive status label update. Got: {:?}",
        cmds
    );
}

#[test]
fn test_menu_action_generate_archive_no_profile_shows_error() {
    // Arrange
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_current_profile_cache(None); // No profile loaded

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
    });

    // Assert
    let cmds = logic.test_drain_commands();
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
            if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No profile loaded"))
        )
        .is_some(),
        "Expected 'No profile loaded' error status. Got: {:?}",
        cmds
    );
}

#[test]
fn test_menu_action_generate_archive_no_archive_path_shows_error() {
    // Arrange
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let profile = Profile {
        name: "NoArchivePathProfile".to_string(),
        root_folder: PathBuf::from("/test/root"),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: None, // No archive path set
        file_details: HashMap::new(),
    };
    logic.test_set_current_profile_cache(Some(profile.clone()));

    // Act
    logic.handle_event(AppEvent::MenuActionClicked {
        window_id: main_window_id,
        action: MenuAction::GenerateArchive,
    });

    // Assert
    let cmds = logic.test_drain_commands();
    assert!(
        find_command(
            &cmds,
            |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { label_id, text, severity, .. }
            if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && severity == &MessageSeverity::Error && text.contains("No archive path set"))
        )
        .is_some(),
        "Expected 'No archive path set' error status. Got: {:?}",
        cmds
    );
}

#[test]
fn test_update_token_count_queues_all_relevant_commands() {
    // Renamed for clarity
    // Arrange
    let (mut logic, _, _, _, _, _, _mock_token_counter) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let expected_message = "Tokens: 0"; // Default token count with no files
    let expected_severity = MessageSeverity::Information;

    // Act
    logic._update_token_count_and_request_display();
    let cmds = logic.test_drain_commands();

    // Assert
    // 1. Check for the new UpdateLabelText command for the general label (from app_info!)
    assert!(
        find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
                if *window_id == main_window_id &&
                   *label_id == ui_constants::STATUS_LABEL_GENERAL_ID &&
                   text == expected_message &&
                   *severity == expected_severity)
        })
        .is_some(),
        "Expected UpdateLabelText command for new general status label. Got: {:?}",
        cmds
    );

    // 2. Check for the new UpdateLabelText command for the dedicated token label (explicitly queued)
    assert!(
        find_command(&cmds, |cmd| {
            matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
                if *window_id == main_window_id &&
                   *label_id == ui_constants::STATUS_LABEL_TOKENS_ID &&
                   text == expected_message &&
                   *severity == expected_severity)
        })
        .is_some(),
        "Expected UpdateLabelText command for new dedicated token label. Got: {:?}",
        cmds
    );

    // Ensure exactly these two commands are present.
    assert_eq!(
        cmds.len(),
        2,
        "Expected exactly two commands (new general label, new dedicated token label). Got: {:?}",
        cmds
    );
}

#[test]
fn test_update_current_archive_status_routes_to_dedicated_label() {
    // Arrange
    let (
        mut logic,
        _mock_config_manager,
        _mock_profile_manager,
        _mock_file_system_scanner,
        mock_archiver,
        _mock_state_manager,
        _mock_token_counter,
    ) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);
    logic.test_set_current_profile_cache(Some(Profile::new(
        "TestProfile".to_string(),
        PathBuf::from("/root"),
    )));

    // Case 1: ArchiveStatus is an error
    let error_status = ArchiveStatus::ErrorChecking(Some(io::ErrorKind::PermissionDenied));
    // For the dedicated label, we expect the plain English string.
    let expected_dedicated_error_text = "Archive: Error: permission denied.".to_string();
    mock_archiver.set_check_archive_status_result(error_status.clone());

    // Act 1
    logic.update_current_archive_status();
    let cmds_error = logic.test_drain_commands();

    // Assert 1
    // Check for dedicated archive label update (Error)
    assert!(
        find_command(&cmds_error, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID &&
               text == &expected_dedicated_error_text &&
               *severity == MessageSeverity::Error
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (Error). Got: {:?}",
        cmds_error
    );
    // Check for general status update (Error, via app_error!)
    // The general status label will use the Debug representation.
    let expected_general_error_text = format!("Archive status error: {:?}", error_status);
    assert!(
        find_command(&cmds_error, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && // From app_error!
               text == &expected_general_error_text  &&
               *severity == MessageSeverity::Error
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_GENERAL_ID from app_error! (Error). Got: {:?}",
        cmds_error
    );

    // Case 2: ArchiveStatus is informational (e.g., UpToDate)
    let info_status = ArchiveStatus::UpToDate;
    // For the dedicated label, we expect the plain English string.
    let expected_dedicated_info_text = "Archive: Up to date.".to_string();
    mock_archiver.set_check_archive_status_result(info_status.clone());

    // Act 2
    logic.update_current_archive_status();
    let cmds_info = logic.test_drain_commands();

    // Assert 2
    // Check for dedicated archive label update (Information)
    assert!(
        find_command(&cmds_info, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID &&
               text == &expected_dedicated_info_text && // Use the plain English text
               *severity == MessageSeverity::Information
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (Information). Got: {:?}",
        cmds_info
    );
    // Ensure no general error messages were sent for informational status
    let general_error_cmd_count = cmds_info.iter().filter(|cmd| {
        matches!(cmd, PlatformCommand::UpdateLabelText { label_id, severity, .. }
            if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && *severity == MessageSeverity::Error)
    }).count();
    assert_eq!(
        general_error_cmd_count, 0,
        "No general error messages should be queued for informational archive status. Got: {:?}",
        cmds_info
    );

    // Case 3: No profile loaded
    logic.test_set_current_profile_cache(None);
    let no_profile_msg_archive_label = "Archive: No profile loaded";
    let no_profile_msg_general = "No profile loaded";

    // Act 3
    logic.update_current_archive_status();
    let cmds_no_profile = logic.test_drain_commands();

    // Assert 3
    // Check for dedicated archive label update
    assert!(
        find_command(&cmds_no_profile, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID &&
               text == no_profile_msg_archive_label &&
               *severity == MessageSeverity::Information
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_ARCHIVE_ID (No Profile). Got: {:?}",
        cmds_no_profile
    );
    // Check for general status update (via app_info!)
    assert!(
        find_command(&cmds_no_profile, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && // From app_info!
               text == no_profile_msg_general &&
               *severity == MessageSeverity::Information
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_GENERAL_ID from app_info! (No Profile). Got: {:?}",
        cmds_no_profile
    );
}

#[test]
fn test_update_token_count_routes_to_dedicated_label() {
    // Arrange
    let (mut logic, _, _, _, _, _, mock_token_counter) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    // Mock AppSessionData to produce a specific token count, e.g., 123
    // For simplicity, we'll let update_token_count run with default mocks,
    // which will likely result in 0 if no files/content are set up.
    // Or, we can mock `mock_token_counter` behavior if `AppSessionData::update_token_count` uses it.
    // Assuming `update_token_count` sums counts of selected files.
    // Let's set up one selected file.
    let temp_dir = tempdir().unwrap();
    let (file1_path, _temp_file1_guard) =
        create_temp_file_with_content(&temp_dir, "file1_tokens", "token content");
    mock_token_counter.set_count_for_content("token content\n", 42);
    logic.test_app_session_data_mut().file_nodes_cache = vec![FileNode {
        path: file1_path.clone(),
        name: "file1_tokens.txt".to_string(),
        is_dir: false,
        state: FileState::Selected, // Mark as selected
        children: Vec::new(),
        checksum: Some("dummy_checksum_so_cache_is_not_used_directly".into()), // Ensure read attempt
    }];

    // Act
    logic._update_token_count_and_request_display();
    let cmds = logic.test_drain_commands();

    // Assert
    let expected_token_text = "Tokens: 42"; // Based on mocked count above
    let expected_severity = MessageSeverity::Information;

    // Check for dedicated token label update
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_TOKENS_ID &&
               text == expected_token_text &&
               *severity == expected_severity
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_TOKENS_ID. Got: {:?}",
        cmds
    );

    // Check for general status update (via app_info!)
    assert!(
        find_command(&cmds, |cmd| matches!(cmd, PlatformCommand::UpdateLabelText { window_id, label_id, text, severity }
            if *window_id == main_window_id &&
               *label_id == ui_constants::STATUS_LABEL_GENERAL_ID && // From app_info!
               text == expected_token_text &&
               *severity == expected_severity
        )).is_some(),
        "Expected UpdateLabelText for STATUS_LABEL_GENERAL_ID from app_info!. Got: {:?}",
        cmds
    );

    // Should be 2 commands: 1 for dedicated token label, 1 from app_info! (general label)
    assert_eq!(
        cmds.len(),
        2,
        "Expected 2 commands for token update. Got: {:?}",
        cmds
    );
}

#[test]
fn test_is_tree_item_new_logic() {
    // Arrange
    let (mut logic, _, _, _, _, _, _) = setup_logic_with_mocks();
    let main_window_id = WindowId(1);
    logic.test_set_main_window_id_and_init_ui_state(main_window_id);

    let path1 = PathBuf::from("/file1.txt");
    let item_id1 = TreeItemId(1);
    let node1 = FileNode {
        path: path1.clone(),
        name: "file1.txt".to_string(),
        is_dir: false,
        state: FileState::New, // Item 1 is New
        children: vec![],
        checksum: None,
    };

    let path2 = PathBuf::from("/file2.txt");
    let item_id2 = TreeItemId(2);
    let node2 = FileNode {
        path: path2.clone(),
        name: "file2.txt".to_string(),
        is_dir: false,
        state: FileState::Selected, // Item 2 is Selected
        children: vec![],
        checksum: None,
    };

    let item_id3 = TreeItemId(3);
    // Node 3 is not added to file_nodes_cache to test not found scenario for node

    let path4 = PathBuf::from("/file4.txt");
    let item_id4 = TreeItemId(4);
    let node4 = FileNode {
        path: path4.clone(),
        name: "file4.txt".to_string(),
        is_dir: false,
        state: FileState::Deselected, // Item 4 is Deselected
        children: vec![],
        checksum: None,
    };

    logic.test_set_file_nodes_cache(vec![node1.clone(), node2.clone(), node4.clone()]);
    logic.test_path_to_tree_item_id_insert(&path1, item_id1);
    logic.test_path_to_tree_item_id_insert(&path2, item_id2);
    // item_id3 path is not inserted into path_to_tree_item_id to test path not found
    logic.test_path_to_tree_item_id_insert(&path4, item_id4);

    // Act & Assert
    assert!(
        logic.is_tree_item_new(main_window_id, item_id1),
        "Item 1 should be New"
    );
    assert!(
        !logic.is_tree_item_new(main_window_id, item_id2),
        "Item 2 should not be New (it's Selected)"
    );
    assert!(
        !logic.is_tree_item_new(main_window_id, TreeItemId(99)),
        "Non-existent ItemId (99) should not be New"
    );
    assert!(
        !logic.is_tree_item_new(WindowId(2), item_id1),
        "Item 1 with wrong window_id should not be New (UI state mismatch)"
    );
    assert!(
        !logic.is_tree_item_new(main_window_id, item_id3),
        "Item 3 (path not in map) should not be New"
    );
    assert!(
        !logic.is_tree_item_new(main_window_id, item_id4),
        "Item 4 should not be New (it's Deselected)"
    );

    // Test with no UI state
    logic.test_clear_ui_state();
    assert!(
        !logic.is_tree_item_new(main_window_id, item_id1),
        "Item 1 should not be New when UI state is None"
    );
}
