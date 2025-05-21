use super::handler::*;

use crate::core::{
    self, ArchiveStatus, ConfigError, ConfigManagerOperations, CoreConfigManagerForConfig,
    FileNode, FileState, FileSystemError, FileSystemScannerOperations, Profile, ProfileError,
    ProfileManagerOperations,
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
use std::time::Duration;
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
    load_profile_from_path_results: Mutex<HashMap<PathBuf, Result<Profile, ProfileError>>>, // New field
    save_profile_calls: Mutex<Vec<(Profile, String)>>,
    save_profile_result: Mutex<Result<(), ProfileError>>,
    list_profiles_result: Mutex<Result<Vec<String>, ProfileError>>,
    get_profile_dir_path_result: Mutex<Option<PathBuf>>,
}

impl MockProfileManager {
    fn new() -> Self {
        MockProfileManager {
            load_profile_results: Mutex::new(HashMap::new()),
            load_profile_from_path_results: Mutex::new(HashMap::new()), // Initialize
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

    // New setter for load_profile_from_path results
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

    #[allow(dead_code)] // May be used in future tests
    fn set_list_profiles_result(&self, result: Result<Vec<String>, ProfileError>) {
        *self.list_profiles_result.lock().unwrap() = result;
    }

    #[allow(dead_code)] // May be used in future tests
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

    #[allow(dead_code)] // May be used in future tests
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
            Some(Ok(nodes)) => Ok(nodes.clone()), // Clone to avoid holding lock
            Some(Err(e)) => Err(clone_file_system_error(e)), // Clone error
            None => {
                // Default behavior: return Ok(empty list) if no specific result is set for this path
                Ok(Vec::new())
            }
        }
    }
}

// Helper to clone FileSystemError
fn clone_file_system_error(error: &FileSystemError) -> FileSystemError {
    match error {
        FileSystemError::Io(e) => FileSystemError::Io(io::Error::new(e.kind(), format!("{}", e))),
        FileSystemError::WalkDir(original_walkdir_error) => {
            // walkdir::Error is not Clone. We create a new io::Error that represents
            // the original walkdir::Error's details as best as possible, then create
            // a new walkdir::Error from that. This is an approximation for mocking.
            let original_path_display = original_walkdir_error
                .path()
                .map_or_else(|| "unknown path".to_string(), |p| p.display().to_string());
            let depth = original_walkdir_error.depth();

            let representative_io_error = match original_walkdir_error.io_error() {
                Some(io_e_ref) => io::Error::new(
                    io_e_ref.kind(),
                    format!(
                        "Original WalkDir IO error at '{}', depth {}: {}",
                        original_path_display, depth, io_e_ref
                    ),
                ),
                None => {
                    // For non-IO errors like symlink loops, create a generic IO error.
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "Non-IO WalkDir error at '{}', depth {} (e.g., symlink loop)",
                            original_path_display, depth
                        ),
                    )
                }
            };

            //
            // Given our structure, the simplest is to return a *new* `FileSystemError::Io`
            // that *describes* the `WalkDir` error, rather than trying to return a `FileSystemError::WalkDir`
            // with a perfectly cloned `walkdir::Error`. This changes the error type slightly for the test,
            // but preserves testability.
            //
            // OR, as a lesser evil for test purposes, if a walkdir error occurs, we just return
            // a generic IO error that says "a walkdir error happened".

            let error_message = format!(
                "Mocked WalkDir error: path {:?}, depth {}, io_error: {:?}",
                original_walkdir_error.path(),
                original_walkdir_error.depth(),
                original_walkdir_error.io_error().map(|e| e.kind())
            );
            // Return a generic IO error to represent the walkdir failure for the clone.
            // This is an approximation.
            FileSystemError::Io(io::Error::new(io::ErrorKind::Other, error_message))
        }
        FileSystemError::InvalidPath(p) => FileSystemError::InvalidPath(p.clone()),
    }
}
// --- End MockFileSystemScanner ---

// Updated setup function
fn setup_logic_with_mocks() -> (
    MyAppLogic,
    Arc<MockConfigManager>,
    Arc<MockProfileManager>,
    Arc<MockFileSystemScanner>,
) {
    let mock_config_manager_arc = Arc::new(MockConfigManager::new());
    let mock_profile_manager_arc = Arc::new(MockProfileManager::new());
    let mock_file_system_scanner_arc = Arc::new(MockFileSystemScanner::new());

    let mut logic = MyAppLogic::new(
        Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>,
        Arc::clone(&mock_profile_manager_arc) as Arc<dyn ProfileManagerOperations>,
        Arc::clone(&mock_file_system_scanner_arc) as Arc<dyn FileSystemScannerOperations>,
    );
    logic.test_set_main_window_id(Some(WindowId(1)));
    (
        logic,
        mock_config_manager_arc,
        mock_profile_manager_arc,
        mock_file_system_scanner_arc,
    )
}

#[test]
fn test_on_main_window_created_loads_last_profile_with_mocks() {
    let (mut logic, mock_config_manager, mock_profile_manager, mock_file_system_scanner) =
        setup_logic_with_mocks();

    let last_profile_name_to_load = "MyMockedStartupProfile";
    let startup_profile_root = PathBuf::from("/mock/startup_root"); // Mocked path

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let mock_loaded_profile = Profile {
        name: last_profile_name_to_load.to_string(),
        root_folder: startup_profile_root.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: None,
    };
    mock_profile_manager
        .set_load_profile_result(last_profile_name_to_load, Ok(mock_loaded_profile));

    // Configure MockFileSystemScanner to return a mock FileNode tree
    let mock_scan_result = vec![FileNode::new(
        startup_profile_root.join("mock_startup_file.txt"),
        "mock_startup_file.txt".into(),
        false,
    )];
    mock_file_system_scanner
        .set_scan_directory_result(&startup_profile_root, Ok(mock_scan_result.clone()));

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

    // Assert against the mocked scan result
    assert_eq!(logic.test_file_nodes_cache().len(), 1);
    assert_eq!(
        logic.test_file_nodes_cache()[0].name,
        "mock_startup_file.txt"
    );
    assert_eq!(
        logic.test_file_nodes_cache()[0].path,
        startup_profile_root.join("mock_startup_file.txt")
    );
    assert!(logic.test_current_archive_status().is_some());
}

#[test]
fn test_on_main_window_created_no_last_profile_with_mocks() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, mock_file_system_scanner) =
        setup_logic_with_mocks();
    // MockConfigManager defaults to Ok(None)
    // MockProfileManager won't be called

    let default_scan_path = PathBuf::from("."); // Current logic defaults to "."
    let mock_default_scan_file_path = default_scan_path.join("default_mock_scan_file.txt");

    // Configure MockFileSystemScanner for the default path
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
    assert!(
        found_dummy_file,
        "Default scan should have found default_mock_scan_file.txt from mock. Cache: {:?}",
        logic
            .test_file_nodes_cache()
            .iter()
            .map(|n| &n.path)
            .collect::<Vec<_>>()
    );
    assert!(logic.test_current_archive_status().is_none());
}

#[test]
fn test_file_open_dialog_completed_updates_state_and_saves_last_profile() {
    // MODIFIED: This test now uses the MockProfileManager for loading from path.
    let (mut logic, mock_config_manager, mock_profile_manager_arc, mock_file_system_scanner_arc) = // Renamed for clarity
        setup_logic_with_mocks();

    let profile_to_load_name = "ProfileToLoadViaManager";
    let profile_root_for_scan = PathBuf::from("/mocked/profile/root/for/scan");
    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/path/to/{}.json", profile_to_load_name)); // This path is just a key for the mock

    // Configure MockProfileManager for load_profile_from_path
    let mock_loaded_profile = Profile {
        name: profile_to_load_name.to_string(),
        root_folder: profile_root_for_scan.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: None,
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_loaded_profile.clone()), // Clone as it's moved into the mock
    );

    // Configure MockFileSystemScanner for the scan that happens after profile load
    let mock_scan_after_load_result = vec![FileNode::new(
        profile_root_for_scan.join("scanned_after_load_via_manager.txt"),
        "scanned_after_load_via_manager.txt".into(),
        false,
    )];
    mock_file_system_scanner_arc.set_scan_directory_result(
        &profile_root_for_scan,
        Ok(mock_scan_after_load_result.clone()),
    );

    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()), // Use the path key
    };
    let _cmds = logic.handle_event(event);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_load_name)
    );
    assert!(logic.test_current_profile_cache().is_some());
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .root_folder,
        profile_root_for_scan
    );
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        profile_to_load_name
    );

    // Assert scan result from mock
    assert_eq!(logic.test_file_nodes_cache().len(), 1);
    assert_eq!(
        logic.test_file_nodes_cache()[0].name,
        "scanned_after_load_via_manager.txt"
    );

    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    let (app_name_saved, profile_name_saved) = saved_name_info.unwrap();
    assert_eq!(app_name_saved, APP_NAME_FOR_PROFILES);
    assert_eq!(profile_name_saved, profile_to_load_name);
}

#[test]
fn test_file_save_dialog_completed_for_profile_saves_profile_via_manager() {
    let (mut logic, mock_config_manager, mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    // No need for temp_scan_dir if scan_directory is not directly called in this path.
    // The root_path_for_scan is set internally when a profile is created/loaded.
    // Let's ensure it has a value if create_profile_from_current_state relies on it.
    logic.test_root_path_for_scan_set(&PathBuf::from("/mock/profile/root"));

    let profile_to_save_name = "MyNewlySavedProfileViaManager";
    let profile_save_path_from_dialog = PathBuf::from(format!(
        "/dummy/path/to/{}.json",
        core::profiles::sanitize_profile_name(profile_to_save_name)
    ));

    logic.test_set_pending_action(PendingAction::SavingProfile);
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
        PathBuf::from("/mock/profile/root") // Matches what was set via test_root_path_for_scan_set
    );

    let save_calls = mock_profile_manager.get_save_profile_calls();
    assert_eq!(save_calls.len(), 1);
    assert_eq!(save_calls[0].0.name, profile_to_save_name);
    assert_eq!(
        save_calls[0].0.root_folder,
        PathBuf::from("/mock/profile/root")
    );
    assert_eq!(save_calls[0].1, APP_NAME_FOR_PROFILES);

    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    assert_eq!(saved_name_info.unwrap().1, profile_to_save_name);
}

#[test]
fn test_handle_button_click_generates_save_dialog_archive() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    let cmds = logic.handle_event(AppEvent::ButtonClicked {
        window_id: WindowId(1),
        control_id: ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
    });
    assert_eq!(cmds.len(), 1, "Expected one command for save dialog");
    match &cmds[0] {
        PlatformCommand::ShowSaveFileDialog {
            title,
            default_filename,
            ..
        } => {
            assert_eq!(title, "Save Archive As");
            assert_eq!(default_filename, "archive.txt");
        }
        _ => panic!("Expected ShowSaveFileDialog for archive"),
    }
}

#[test]
fn test_handle_button_click_generate_archive_with_profile_context() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    let temp_root_path = PathBuf::from("/mock/archive_button_root"); // Mock path
    let profile_name = "MyTestProfileForArchiveButton";
    let archive_file_path = temp_root_path.join("my_archive_for_button.txt");

    logic.test_set_current_profile_cache(Some(Profile {
        name: profile_name.to_string(),
        root_folder: temp_root_path.clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_path.clone()),
    }));
    logic.test_root_path_for_scan_set(&temp_root_path); // Keep consistent

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
}

#[test]
fn test_handle_file_save_dialog_completed_for_archive_updates_profile_via_manager() {
    let (mut logic, _mock_config_manager, mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingArchive);
    logic.test_set_pending_archive_content("ARCHIVE CONTENT FOR MANAGER TEST".to_string());

    let tmp_file_obj = NamedTempFile::new().unwrap();
    let archive_save_path = tmp_file_obj.path().to_path_buf();
    let temp_root_for_profile = PathBuf::from("/mock/profile_for_archive_save"); // Mock path
    let profile_name_for_save = "test_profile_for_archive_save_via_manager";

    logic.test_set_current_profile_cache(Some(Profile::new(
        profile_name_for_save.into(),
        temp_root_for_profile.clone(),
    )));
    // Ensure root_path_for_scan is consistent if any internal logic relies on it during this flow.
    logic.test_root_path_for_scan_set(&temp_root_for_profile);

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: Some(archive_save_path.clone()),
    });

    assert!(
        cmds.is_empty(),
        "No follow-up UI commands expected directly"
    );
    assert_eq!(
        *logic.test_pending_archive_content(),
        None,
        "Pending content should be cleared"
    );
    let written_content = fs::read_to_string(&archive_save_path).unwrap();
    assert_eq!(written_content, "ARCHIVE CONTENT FOR MANAGER TEST");

    let cached_profile = logic.test_current_profile_cache().as_ref().unwrap();
    assert_eq!(
        cached_profile.archive_path.as_ref().unwrap(),
        &archive_save_path
    );

    let save_calls = mock_profile_manager.get_save_profile_calls();
    assert_eq!(save_calls.len(), 1);
    assert_eq!(save_calls[0].0.name, profile_name_for_save);
    assert_eq!(
        save_calls[0].0.archive_path,
        Some(archive_save_path.clone())
    );
    assert_eq!(save_calls[0].1, APP_NAME_FOR_PROFILES);

    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NoFilesSelected)
    );
}

#[test]
fn test_handle_file_save_dialog_cancelled_for_archive() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingArchive);
    logic.test_set_pending_archive_content("WILL BE CLEARED".to_string());

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: None,
    });

    assert!(cmds.is_empty());
    assert_eq!(
        *logic.test_pending_archive_content(),
        None,
        "Pending content should be cleared on cancel"
    );
    assert!(
        logic.test_pending_action().is_none(),
        "Pending action should be cleared"
    );
}

#[test]
fn test_handle_file_save_dialog_cancelled_for_profile() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    logic.test_set_pending_action(PendingAction::SavingProfile);

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: None,
    });
    assert!(cmds.is_empty());
    assert!(
        logic.test_pending_action().is_none(),
        "Pending action should be cleared on cancel"
    );
}

#[test]
fn test_handle_treeview_item_toggled_updates_model_visuals_and_archive_status() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();

    // For this test, actual file system interaction is for timestamp checking with core::get_file_timestamp
    // The scanning part is not directly tested here, but the initial FileNode setup is manual.
    let temp_scan_dir = tempdir().unwrap(); // Used for creating real files for timestamp checks
    logic.test_root_path_for_scan_set(temp_scan_dir.path()); // Set the base for paths

    let archive_file_path = temp_scan_dir.path().join("archive.txt");
    File::create(&archive_file_path)
        .unwrap()
        .write_all(b"old archive content")
        .unwrap();
    thread::sleep(Duration::from_millis(50)); // Ensure time difference for timestamps

    let foo_path_relative_to_scan_root = PathBuf::from("foo.txt");
    let foo_full_path = temp_scan_dir.path().join(&foo_path_relative_to_scan_root);
    File::create(&foo_full_path)
        .unwrap()
        .write_all(b"foo content - will be selected")
        .unwrap();

    logic.test_set_file_nodes_cache(vec![FileNode::new(
        foo_full_path.clone(), // Use full path for the FileNode
        "foo.txt".into(),
        false,
    )]);
    logic.test_set_current_profile_cache(Some(Profile {
        name: "test_profile_for_toggle".into(),
        root_folder: logic.test_root_path_for_scan().clone(), // Use the scan root
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_path.clone()),
    }));

    logic.test_path_to_tree_item_id_clear();
    let _descriptors = logic.build_tree_item_descriptors_recursive(); // Populates path_to_tree_item_id

    // Ensure foo_full_path is in the map
    assert!(
        logic
            .test_path_to_tree_item_id()
            .contains_key(&foo_full_path),
        "foo_full_path not found in path_to_tree_item_id map. Map: {:?}",
        logic.test_path_to_tree_item_id()
    );

    let tree_item_id_for_foo = *logic
        .test_path_to_tree_item_id()
        .get(&foo_full_path)
        .expect("TreeItemId for foo.txt not found in map");

    let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
        window_id: WindowId(1),
        item_id: tree_item_id_for_foo,
        new_state: CheckState::Checked,
    });
    assert_eq!(cmds.len(), 1, "Expected one visual update command");
    match &cmds[0] {
        PlatformCommand::UpdateTreeItemVisualState {
            item_id, new_state, ..
        } => {
            assert_eq!(*item_id, tree_item_id_for_foo);
            assert_eq!(*new_state, CheckState::Checked);
        }
        _ => panic!("Expected UpdateTreeItemVisualState"),
    }
    assert_eq!(
        logic.test_file_nodes_cache()[0].state,
        FileState::Selected,
        "Model state should be Selected"
    );

    let archive_ts = core::get_file_timestamp(&archive_file_path).unwrap();
    let foo_ts = core::get_file_timestamp(&foo_full_path).unwrap();
    assert!(
        foo_ts > archive_ts,
        "Test Sanity Check: foo.txt ({:?}) should be newer than archive ({:?})",
        foo_ts,
        archive_ts
    );
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::OutdatedRequiresUpdate)
    );
}

#[test]
fn test_handle_window_close_requested_generates_close_command() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    let cmds = logic.handle_event(AppEvent::WindowCloseRequested {
        window_id: WindowId(1),
    });
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], PlatformCommand::CloseWindow { .. }));
}

#[test]
fn test_handle_window_destroyed_clears_main_window_id_and_state() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
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
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    logic.test_path_to_tree_item_id_clear();
    let descriptors = logic.build_tree_item_descriptors_recursive();
    assert_eq!(descriptors.len(), 2);
    // Verifying structure and IDs
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

    // Check path_to_tree_item_id map
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
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());

    let path_to_find = PathBuf::from("/root/sub/file2.txt");

    // Test find_filenode_ref
    let found_ref = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &path_to_find);
    assert!(found_ref.is_some());
    assert_eq!(found_ref.unwrap().name, "file2.txt");

    // Test find_filenode_mut
    let found_mut = logic.test_find_filenode_mut(&path_to_find);
    assert!(found_mut.is_some());
    assert_eq!(found_mut.as_ref().unwrap().name, "file2.txt");

    // Modify and check
    if let Some(node) = found_mut {
        node.state = FileState::Selected;
    }

    let ref_after_mut = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &path_to_find);
    assert_eq!(ref_after_mut.unwrap().state, FileState::Selected);
}

#[test]
fn test_collect_visual_updates_recursive_applogic() {
    let (mut logic, _mock_config_manager, _mock_profile_manager, _mock_file_system_scanner) =
        setup_logic_with_mocks();
    let mut test_tree = make_test_tree_for_applogic();
    // Set some states
    test_tree[0].state = FileState::Selected; // file1.txt
    test_tree[1].children[0].state = FileState::Deselected; // sub/file2.txt

    logic.test_set_file_nodes_cache(test_tree);
    logic.test_path_to_tree_item_id_clear(); // Important to clear before building descriptors
    let _descriptors = logic.build_tree_item_descriptors_recursive(); // This populates path_to_tree_item_id

    let mut updates = Vec::new();
    // Collect updates for the root node (or a specific node if needed)
    // Here we iterate through top-level nodes to collect all.
    // Clone the cache for iteration to avoid borrow checker issues with `logic`
    for node_ref in logic.test_file_nodes_cache().clone().iter() {
        logic.collect_visual_updates_recursive(node_ref, &mut updates);
    }

    // Expected updates: file1.txt (Selected), sub (Unknown), sub/file2.txt (Deselected)
    // The exact TreeItemIds are generated, so we check based on path mapping
    let path_map = logic.test_path_to_tree_item_id();

    let file1_id = path_map.get(&PathBuf::from("/root/file1.txt")).unwrap();
    let sub_id = path_map.get(&PathBuf::from("/root/sub")).unwrap();
    let file2_id = path_map.get(&PathBuf::from("/root/sub/file2.txt")).unwrap();

    assert!(updates.contains(&(*file1_id, CheckState::Checked)));
    assert!(updates.contains(&(*sub_id, CheckState::Unchecked))); // dir 'sub' itself is Unknown
    assert!(updates.contains(&(*file2_id, CheckState::Unchecked))); // file2.txt is Deselected -> Unchecked

    // There should be 3 items in the map if path_to_tree_item_id is correctly populated
    // and file_nodes_cache has 3 distinct nodes that get mapped.
    // make_test_tree_for_applogic has file1.txt, sub, and sub/file2.txt.
    // Total 3 distinct paths are added to path_map from build_tree_item_descriptors_recursive.
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

#[test]
fn test_profile_load_updates_archive_status_direct_load() {
    // Consider renaming test for clarity if desired, e.g., test_profile_load_via_manager_updates_archive_status
    let (mut logic, _mock_config_manager, mock_profile_manager_arc, mock_file_system_scanner_arc) =
        setup_logic_with_mocks();

    let profile_name = "ProfileForStatusUpdateViaManager";
    let root_folder_for_profile = PathBuf::from("/mock/scan_root_status_manager");
    // This is a mock path for the archive file. Crucially, for this test,
    // we *do not* create this file on the file system.
    let archive_file_for_profile =
        PathBuf::from("/mock/my_manager_archive_status_DOES_NOT_EXIST.txt");

    let profile_json_path_from_dialog =
        PathBuf::from(format!("/dummy/profiles/{}.json", profile_name));

    let mock_profile_to_load = Profile {
        name: profile_name.to_string(),
        root_folder: root_folder_for_profile.clone(),
        selected_paths: HashSet::new(), // No files are selected in the profile
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_for_profile.clone()), // Profile IS associated with an archive path
    };
    mock_profile_manager_arc.set_load_profile_from_path_result(
        &profile_json_path_from_dialog,
        Ok(mock_profile_to_load.clone()),
    );

    // Mock the scan result (empty, so no FileNodes are selected from scan either)
    mock_file_system_scanner_arc.set_scan_directory_result(&root_folder_for_profile, Ok(vec![]));

    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path_from_dialog.clone()),
    };
    let _cmds = logic.handle_event(event);

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_name)
    );
    assert!(logic.test_current_profile_cache().is_some());
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        profile_name
    );
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .root_folder,
        root_folder_for_profile
    );
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .archive_path,
        Some(archive_file_for_profile.clone())
    );

    // CORRECTED EXPECTATION:
    // Since profile.archive_path is Some(...), but the actual file at that path
    // does not exist (because we didn't create it in this test),
    // core::check_archive_status should return ArchiveFileMissing.
    // This takes precedence over NoFilesSelected.
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::ArchiveFileMissing) // <<<< CORRECTED EXPECTATION
    );
}
