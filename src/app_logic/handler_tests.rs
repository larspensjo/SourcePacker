use super::handler::*;

// Import other necessary items from crate::core and crate::platform_layer
use crate::core::{
    self, ArchiveStatus, ConfigError, ConfigManagerOperations, CoreConfigManager, FileNode,
    FileState, Profile, ProfileError,
};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};

// Standard library and external crate imports for tests
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::{NamedTempFile, tempdir};

// --- MockConfigManager for testing ---
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
        match *self.load_last_profile_name_result.lock().unwrap() {
            Ok(ref opt_str) => Ok(opt_str.clone()),
            Err(ConfigError::Io(ref io_err)) => Err(ConfigError::Io(io::Error::new(
                io_err.kind(),
                "mocked io error",
            ))),
            Err(ConfigError::NoProjectDirectory) => Err(ConfigError::NoProjectDirectory),
            Err(ConfigError::Utf8Error(ref utf8_err)) => {
                let dummy_vec = utf8_err.as_bytes().to_vec();
                let recreated_utf8_error = String::from_utf8(dummy_vec).unwrap_err();
                Err(ConfigError::Utf8Error(recreated_utf8_error))
            }
        }
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

fn setup_logic_with_mock_config_manager() -> (MyAppLogic, Arc<MockConfigManager>) {
    let mock_config_manager_arc = Arc::new(MockConfigManager::new());
    let mut logic =
        MyAppLogic::new(Arc::clone(&mock_config_manager_arc) as Arc<dyn ConfigManagerOperations>);
    logic.test_set_main_window_id(Some(WindowId(1)));
    (logic, mock_config_manager_arc)
}

fn create_temp_profile_file_in_profile_subdir(
    base_temp_dir: &tempfile::TempDir,
    app_name: &str,
    profile_name: &str,
    root_folder: &Path,
    archive_path: Option<PathBuf>,
) -> PathBuf {
    let profile = Profile {
        name: profile_name.to_string(),
        root_folder: root_folder.to_path_buf(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path,
    };

    let app_data_dir_for_profiles = base_temp_dir.path().join(app_name).join("profiles");
    fs::create_dir_all(&app_data_dir_for_profiles).unwrap();

    let sanitized_name = core::profiles::sanitize_profile_name(profile_name);
    let final_path = app_data_dir_for_profiles.join(format!("{}.json", sanitized_name));

    let file = File::create(&final_path).expect("Failed to create temp profile file");
    serde_json::to_writer_pretty(file, &profile).expect("Failed to write temp profile file");
    final_path
}

fn create_temp_profile_file_for_direct_load(
    dir: &tempfile::TempDir,
    profile_name_stem: &str,
    root_folder: &Path,
    archive_path: Option<PathBuf>,
) -> PathBuf {
    let profile = Profile {
        name: profile_name_stem.to_string(),
        root_folder: root_folder.to_path_buf(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path,
    };
    let profile_file_path = dir.path().join(format!("{}.json", profile_name_stem));

    let file = File::create(&profile_file_path)
        .expect("Failed to create temp profile file for direct load");
    serde_json::to_writer_pretty(file, &profile)
        .expect("Failed to write temp profile file for direct load");
    profile_file_path
}

fn setup_logic_with_window() -> (MyAppLogic, Arc<MockConfigManager>) {
    setup_logic_with_mock_config_manager()
}

#[test]
fn test_on_main_window_created_loads_last_profile_with_mock() {
    let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
    let temp_base_dir = tempdir().unwrap();

    let last_profile_name_to_load = "MyMockedStartupProfile";
    let startup_profile_root = temp_base_dir.path().join("mock_startup_root");
    fs::create_dir_all(&startup_profile_root).unwrap();
    File::create(startup_profile_root.join("mock_startup_file.txt"))
        .expect("Test setup: Failed to create mock_startup_file.txt");

    mock_config_manager
        .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

    let _profile_json_path = create_temp_profile_file_in_profile_subdir(
        &temp_base_dir,
        APP_NAME_FOR_PROFILES, // Now accessible as pub(crate)
        last_profile_name_to_load,
        &startup_profile_root,
        None,
    );

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
    assert_eq!(logic.test_file_nodes_cache().len(), 1);
    assert_eq!(
        logic.test_file_nodes_cache()[0].name,
        "mock_startup_file.txt"
    );
    assert!(logic.test_current_archive_status().is_some());
}

#[test]
fn test_on_main_window_created_no_last_profile_with_mock() {
    let (mut logic, _mock_config_manager) = setup_logic_with_mock_config_manager();

    let default_scan_path = PathBuf::from(".");
    let dummy_file_path = default_scan_path.join("default_mock_scan_file.txt");
    File::create(&dummy_file_path)
        .expect("Test setup: Failed to create default_mock_scan_file.txt");

    let _cmds = logic.on_main_window_created(WindowId(1));

    assert!(logic.test_current_profile_name().is_none());
    assert!(logic.test_current_profile_cache().is_none());
    assert_eq!(*logic.test_root_path_for_scan(), default_scan_path);

    let found_dummy_file = logic
        .test_file_nodes_cache()
        .iter()
        .any(|n| n.path == dummy_file_path);
    assert!(
        found_dummy_file,
        "Default scan should have found default_mock_scan_file.txt. Cache: {:?}",
        logic
            .test_file_nodes_cache()
            .iter()
            .map(|n| &n.path)
            .collect::<Vec<_>>()
    );
    assert!(logic.test_current_archive_status().is_none());
    fs::remove_file(dummy_file_path)
        .expect("Test cleanup: Failed to remove default_mock_scan_file.txt");
}

#[test]
fn test_file_open_dialog_completed_saves_last_profile_name_with_mock() {
    let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
    let temp_profile_dir = tempdir().unwrap();

    let profile_to_load_name = "ProfileToLoadAndSaveAsLastMocked";
    let profile_root = temp_profile_dir.path().join("prof_mock_root");
    fs::create_dir_all(&profile_root).unwrap();

    let profile_json_path = create_temp_profile_file_for_direct_load(
        &temp_profile_dir,
        profile_to_load_name,
        &profile_root,
        None,
    );

    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_json_path),
    };
    let _cmds = logic.handle_event(event); // PlatformEventHandler trait now in scope

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_load_name)
    );
    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    let (app_name_saved, profile_name_saved) = saved_name_info.unwrap();
    assert_eq!(app_name_saved, APP_NAME_FOR_PROFILES); // Now accessible
    assert_eq!(profile_name_saved, profile_to_load_name);
}

#[test]
fn test_file_save_dialog_completed_for_profile_saves_last_profile_name_with_mock() {
    let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
    let temp_scan_dir = tempdir().unwrap();
    logic.test_root_path_for_scan_set(temp_scan_dir.path());

    let temp_base_app_data_dir = tempdir().unwrap();
    let profile_to_save_name = "MyNewlySavedProfileMocked";

    let mock_profile_storage_dir = temp_base_app_data_dir
        .path()
        .join(APP_NAME_FOR_PROFILES) // Now accessible
        .join("profiles");
    fs::create_dir_all(&mock_profile_storage_dir).unwrap();
    let profile_save_path_from_dialog = mock_profile_storage_dir.join(format!(
        "{}.json",
        core::profiles::sanitize_profile_name(profile_to_save_name)
    ));

    logic.test_set_pending_action(PendingAction::SavingProfile); // Now accessible
    let event = AppEvent::FileSaveDialogCompleted {
        window_id: WindowId(1),
        result: Some(profile_save_path_from_dialog.clone()),
    };

    let _cmds = logic.handle_event(event); // PlatformEventHandler trait now in scope

    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_to_save_name)
    );
    assert!(logic.test_current_profile_cache().is_some());
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        profile_to_save_name
    );
    assert!(profile_save_path_from_dialog.exists());

    let saved_name_info = mock_config_manager.get_saved_profile_name();
    assert!(saved_name_info.is_some());
    let (app_name_saved, profile_name_saved) = saved_name_info.unwrap();
    assert_eq!(app_name_saved, APP_NAME_FOR_PROFILES); // Now accessible
    assert_eq!(profile_name_saved, profile_to_save_name);

    if profile_save_path_from_dialog.exists() {
        fs::remove_file(&profile_save_path_from_dialog).unwrap();
    }
}

#[test]
fn test_handle_button_click_generates_save_dialog_archive() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    let cmds = logic.handle_event(AppEvent::ButtonClicked {
        // PlatformEventHandler trait now in scope
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
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    let temp_root = tempdir().unwrap();
    let profile_name = "MyTestProfile".to_string();
    let archive_file = temp_root.path().join("my_archive.txt");

    logic.test_set_current_profile_cache(Some(Profile {
        name: profile_name.clone(),
        root_folder: temp_root.path().to_path_buf(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file.clone()),
    }));
    logic.test_root_path_for_scan_set(&temp_root.path());

    let cmds = logic.handle_event(AppEvent::ButtonClicked {
        // PlatformEventHandler trait now in scope
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
            assert_eq!(initial_dir.as_deref(), archive_file.parent());
        }
        _ => panic!("Expected ShowSaveFileDialog with profile context"),
    }
}

#[test]
fn test_handle_file_save_dialog_completed_for_archive_with_path() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_pending_action(PendingAction::SavingArchive); // Now accessible
    logic.test_set_pending_archive_content("ARCHIVE CONTENT".to_string());

    let tmp_file = NamedTempFile::new().unwrap();
    let archive_save_path = tmp_file.path().to_path_buf();
    let temp_root_for_profile = tempdir().unwrap();
    logic.test_set_current_profile_cache(Some(Profile::new(
        "test_profile_for_archive_save".into(),
        temp_root_for_profile.path().to_path_buf(),
    )));
    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        // PlatformEventHandler trait now in scope
        window_id: WindowId(1),
        result: Some(archive_save_path.clone()),
    });

    assert!(
        cmds.is_empty(),
        "No follow-up UI commands expected directly from save completion"
    );
    assert_eq!(
        *logic.test_pending_archive_content(),
        None,
        "Pending content should be cleared"
    );
    let written_content = fs::read_to_string(&archive_save_path).unwrap();
    assert_eq!(written_content, "ARCHIVE CONTENT");
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .archive_path
            .as_ref()
            .unwrap(),
        &archive_save_path
    );
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NoFilesSelected),
        "Archive status should be NoFilesSelected when no files are in cache/selected"
    );
}

#[test]
fn test_handle_file_save_dialog_cancelled_for_archive() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_pending_action(PendingAction::SavingArchive); // Now accessible
    logic.test_set_pending_archive_content("WILL BE CLEARED".to_string());

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        // PlatformEventHandler trait now in scope
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
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_pending_action(PendingAction::SavingProfile); // Now accessible

    let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
        // PlatformEventHandler trait now in scope
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
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    let temp_scan_dir = tempdir().unwrap();
    logic.test_root_path_for_scan_set(temp_scan_dir.path());
    let archive_file_path = temp_scan_dir.path().join("archive.txt");
    File::create(&archive_file_path)
        .unwrap()
        .write_all(b"old archive content")
        .unwrap();
    thread::sleep(Duration::from_millis(50));
    let foo_path = logic.test_root_path_for_scan().join("foo.txt");
    File::create(&foo_path)
        .unwrap()
        .write_all(b"foo content - will be selected")
        .unwrap();
    logic.test_set_file_nodes_cache(vec![FileNode::new(
        foo_path.clone(),
        "foo.txt".into(),
        false,
    )]);
    logic.test_set_current_profile_cache(Some(Profile {
        name: "test_profile_for_toggle".into(),
        root_folder: logic.test_root_path_for_scan().clone(),
        selected_paths: HashSet::new(),
        deselected_paths: HashSet::new(),
        archive_path: Some(archive_file_path.clone()),
    }));
    logic.test_path_to_tree_item_id_clear();
    // Calls pub(crate) MyAppLogic::build_tree_item_descriptors_recursive
    let _descriptors = logic.build_tree_item_descriptors_recursive();
    let tree_item_id_for_foo = *logic.test_path_to_tree_item_id().get(&foo_path).unwrap();
    let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
        // PlatformEventHandler trait now in scope
        window_id: WindowId(1),
        item_id: tree_item_id_for_foo,
        new_state: CheckState::Checked,
    });
    assert_eq!(cmds.len(), 1, "Expected one visual update command");
    match &cmds[0] {
        PlatformCommand::UpdateTreeItemVisualState {
            item_id, new_state, ..
        } => {
            assert_eq!(*item_id, tree_item_id_for_foo); // Fixed: *item_id -> item_id (TreeItemId is Copy)
            assert_eq!(*new_state, CheckState::Checked); // Fixed: *new_state -> new_state (CheckState is Copy)
        }
        _ => panic!("Expected UpdateTreeItemVisualState"),
    }
    assert_eq!(
        logic.test_file_nodes_cache()[0].state,
        FileState::Selected,
        "Model state should be Selected"
    );
    let archive_ts = core::get_file_timestamp(&archive_file_path).unwrap();
    let foo_ts = core::get_file_timestamp(&foo_path).unwrap();
    assert!(
        foo_ts > archive_ts,
        "Test Sanity Check: foo.txt ({:?}) should be newer than archive ({:?})",
        foo_ts,
        archive_ts
    );
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::OutdatedRequiresUpdate),
        "Archive status incorrect after toggle. Expected Outdated. Foo_ts: {:?}, Archive_ts: {:?}",
        foo_ts,
        archive_ts
    );
}

#[test]
fn test_handle_window_close_requested_generates_close_command() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    let cmds = logic.handle_event(AppEvent::WindowCloseRequested {
        // PlatformEventHandler trait now in scope
        window_id: WindowId(1),
    });
    assert_eq!(cmds.len(), 1);
    assert!(matches!(cmds[0], PlatformCommand::CloseWindow { .. }));
}

#[test]
fn test_handle_window_destroyed_clears_main_window_id_and_state() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
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
        // PlatformEventHandler trait now in scope
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
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    logic.test_path_to_tree_item_id_clear();
    // Calls pub(crate) MyAppLogic::build_tree_item_descriptors_recursive
    let descriptors = logic.build_tree_item_descriptors_recursive();
    assert_eq!(
        descriptors.len(),
        2,
        "Expected two top-level descriptors: file1.txt and sub"
    );
    let file1_desc = descriptors.iter().find(|d| d.text == "file1.txt").unwrap();
    assert!(!file1_desc.is_folder);
    assert!(file1_desc.children.is_empty());
    assert!(matches!(file1_desc.state, CheckState::Unchecked));
    let sub_desc = descriptors.iter().find(|d| d.text == "sub").unwrap();
    assert!(sub_desc.is_folder);
    assert_eq!(
        sub_desc.children.len(),
        1,
        "Sub folder should have one child (file2.txt)"
    );
    assert_eq!(sub_desc.children[0].text, "file2.txt");
    assert!(!sub_desc.children[0].is_folder);
    assert!(matches!(sub_desc.state, CheckState::Unchecked));
    assert_eq!(logic.test_path_to_tree_item_id().len(), 3);
    assert!(
        logic
            .test_path_to_tree_item_id()
            .contains_key(&PathBuf::from("/root/file1.txt"))
    );
    assert!(
        logic
            .test_path_to_tree_item_id()
            .contains_key(&PathBuf::from("/root/sub"))
    );
    assert!(
        logic
            .test_path_to_tree_item_id()
            .contains_key(&PathBuf::from("/root/sub/file2.txt"))
    );
}

#[test]
fn test_find_filenode_mut_and_ref_applogic() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    let file1_p = PathBuf::from("/root/file1.txt");
    let file2_p = PathBuf::from("/root/sub/file2.txt");
    // Calls pub(crate) MyAppLogic::find_filenode_mut
    let file1_node_mut = logic.test_find_filenode_mut(&file1_p);
    assert!(file1_node_mut.is_some());
    file1_node_mut.unwrap().state = FileState::Selected;
    // Calls pub(crate) MyAppLogic::find_filenode_ref
    let file1_node_ref = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &file1_p);
    assert!(file1_node_ref.is_some());
    assert_eq!(file1_node_ref.unwrap().state, FileState::Selected);
    // Calls pub(crate) MyAppLogic::find_filenode_ref
    let file2_node_ref = MyAppLogic::find_filenode_ref(logic.test_file_nodes_cache(), &file2_p);
    assert!(file2_node_ref.is_some());
    assert_eq!(file2_node_ref.unwrap().name, "file2.txt");
    // Calls pub(crate) MyAppLogic::find_filenode_ref
    let none_node = MyAppLogic::find_filenode_ref(
        logic.test_file_nodes_cache(),
        &PathBuf::from("/no/such/path"),
    );
    assert!(none_node.is_none());
}

#[test]
fn test_collect_visual_updates_recursive_applogic() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    logic.test_set_file_nodes_cache(make_test_tree_for_applogic());
    let file1_p = PathBuf::from("/root/file1.txt");
    let sub_p = PathBuf::from("/root/sub");
    let file2_p = PathBuf::from("/root/sub/file2.txt");
    logic.test_path_to_tree_item_id_clear();
    // Calls pub(crate) MyAppLogic::build_tree_item_descriptors_recursive
    let _ = logic.build_tree_item_descriptors_recursive();
    {
        // Calls pub(crate) MyAppLogic::find_filenode_mut
        let f1_mut = logic.test_find_filenode_mut(&file1_p).unwrap();
        f1_mut.state = FileState::Selected;
    }
    let sub_node_for_update_path = PathBuf::from("/root/sub");
    {
        // Calls pub(crate) MyAppLogic::find_filenode_mut
        let file2_mut = logic.test_find_filenode_mut(&file2_p).unwrap();
        file2_mut.state = FileState::Selected;
        // Calls pub(crate) MyAppLogic::find_filenode_mut
        let sub_node_mut = logic
            .test_find_filenode_mut(&sub_node_for_update_path)
            .unwrap();
        sub_node_mut.state = FileState::Unknown;
    }
    let mut updates = Vec::new();
    let sub_node_ref = logic
        .test_file_nodes_cache()
        .iter()
        .find(|n| n.path == sub_node_for_update_path)
        .unwrap()
        .clone();
    // Calls pub(crate) logic.collect_visual_updates_recursive
    logic.collect_visual_updates_recursive(&sub_node_ref, &mut updates);
    assert_eq!(
        updates.len(),
        2,
        "Expected updates for 'sub' and 'file2.txt'"
    );
    let sub_item_id = *logic.test_path_to_tree_item_id().get(&sub_p).unwrap();
    assert!(
        updates
            .iter()
            .any(|(id, state)| *id == sub_item_id && *state == CheckState::Unchecked)
    );
    let file2_item_id = *logic.test_path_to_tree_item_id().get(&file2_p).unwrap();
    assert!(
        updates
            .iter()
            .any(|(id, state)| *id == file2_item_id && *state == CheckState::Checked)
    );
}

#[test]
fn test_profile_load_updates_archive_status() {
    let (mut logic, _mock_config_manager) = setup_logic_with_window();
    let temp_dir = tempdir().unwrap();
    let profile_name = "ProfileToLoadDirectly";
    let root_folder_for_profile = temp_dir.path().join("scan_root_direct");
    fs::create_dir_all(&root_folder_for_profile).unwrap();
    let archive_file_for_profile = temp_dir.path().join("my_direct_archive.txt");
    File::create(&archive_file_for_profile)
        .unwrap()
        .write_all(b"direct archive content")
        .unwrap();
    let actual_profile_json_path = create_temp_profile_file_for_direct_load(
        &temp_dir,
        profile_name,
        &root_folder_for_profile,
        Some(archive_file_for_profile.clone()),
    );
    let event = AppEvent::FileOpenDialogCompleted {
        window_id: WindowId(1),
        result: Some(actual_profile_json_path.clone()),
    };
    let _cmds = logic.handle_event(event); // PlatformEventHandler trait now in scope
    assert_eq!(
        logic.test_current_profile_name().as_deref(),
        Some(profile_name),
        "Profile name mismatch after load"
    );
    assert!(
        logic.test_current_profile_cache().is_some(),
        "Profile cache should be populated"
    );
    assert_eq!(
        logic.test_current_profile_cache().as_ref().unwrap().name,
        profile_name,
        "Name in cached profile mismatch"
    );
    assert_eq!(
        logic
            .test_current_profile_cache()
            .as_ref()
            .unwrap()
            .archive_path
            .as_ref()
            .unwrap(),
        &archive_file_for_profile,
        "Archive path in cached profile mismatch"
    );
    assert_eq!(
        *logic.test_current_archive_status(),
        Some(ArchiveStatus::NoFilesSelected),
        "Archive status after load is incorrect"
    );
}
