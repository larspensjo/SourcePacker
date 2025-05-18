use crate::core::{self, FileNode, FileState, Profile};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const ID_BUTTON_GENERATE_ARCHIVE_LOGIC: i32 = 1002;
const APP_NAME_FOR_PROFILES: &str = "SourcePackerApp";

type PathToTreeItemIdMap = HashMap<PathBuf, TreeItemId>;

#[derive(Debug)]
enum PendingAction {
    SavingArchive,
    SavingProfile,
}

/*
 * Manages the core application state and UI logic in a platform-agnostic manner.
 * It processes UI events received from the platform layer and generates commands
 * to update the UI, managing file trees, profiles, and archive generation.
 */
pub struct MyAppLogic {
    main_window_id: Option<WindowId>,
    file_nodes_cache: Vec<FileNode>,
    path_to_tree_item_id: PathToTreeItemIdMap,
    next_tree_item_id_counter: u64,
    root_path_for_scan: PathBuf,
    current_profile_name: Option<String>,
    pending_archive_content: Option<String>,
    pending_action: Option<PendingAction>,
}

impl MyAppLogic {
    /*
     * Initializes a new instance of the application logic.
     * Sets up default values for the application state, including an initial root path
     * and an empty file cache.
     */
    pub fn new() -> Self {
        MyAppLogic {
            main_window_id: None,
            file_nodes_cache: Vec::new(),
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
            root_path_for_scan: PathBuf::from("."),
            current_profile_name: None,
            pending_archive_content: None,
            pending_action: None,
        }
    }

    fn generate_tree_item_id(&mut self) -> TreeItemId {
        let id = self.next_tree_item_id_counter;
        self.next_tree_item_id_counter += 1;
        TreeItemId(id)
    }

    fn build_tree_item_descriptors_recursive(
        nodes: &[FileNode],
        path_to_tree_item_id: &mut PathToTreeItemIdMap,
        next_tree_item_id_counter: &mut u64,
    ) -> Vec<TreeItemDescriptor> {
        let mut descriptors = Vec::new();
        for node in nodes {
            let id_val = *next_tree_item_id_counter;
            *next_tree_item_id_counter += 1;
            let item_id = TreeItemId(id_val);

            path_to_tree_item_id.insert(node.path.clone(), item_id);

            let descriptor = TreeItemDescriptor {
                id: item_id,
                text: node.name.clone(),
                is_folder: node.is_dir,
                state: match node.state {
                    FileState::Selected => CheckState::Checked,
                    _ => CheckState::Unchecked,
                },
                children: Self::build_tree_item_descriptors_recursive(
                    &node.children,
                    path_to_tree_item_id,
                    next_tree_item_id_counter,
                ),
            };
            descriptors.push(descriptor);
        }
        descriptors
    }

    /*
     * Handles the event indicating the main application window has been created by the platform layer.
     * It performs an initial directory scan based on the current root path and populates the UI
     * with the discovered file structure. It returns a list of platform commands to show the window
     * and display the initial file tree.
     */
    pub fn on_main_window_created(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        self.main_window_id = Some(window_id);
        let mut commands = Vec::new();

        println!(
            "AppLogic: Initial scan of directory {:?}",
            self.root_path_for_scan
        );

        // Updated call to scan_directory
        match core::scan_directory(&self.root_path_for_scan) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                println!(
                    "AppLogic: Scanned {} top-level nodes.",
                    self.file_nodes_cache.len()
                );
            }
            Err(e) => {
                eprintln!(
                    "AppLogic: Failed to scan directory {:?}: {}",
                    self.root_path_for_scan, e
                );
                let error_node_path = PathBuf::from("/error_node");
                self.file_nodes_cache = vec![FileNode::new(
                    error_node_path,
                    format!("Error scanning directory: {}", e),
                    false,
                )];
            }
        }

        self.next_tree_item_id_counter = 1;
        self.path_to_tree_item_id.clear();
        let descriptors = Self::build_tree_item_descriptors_recursive(
            &self.file_nodes_cache,
            &mut self.path_to_tree_item_id,
            &mut self.next_tree_item_id_counter,
        );

        if !descriptors.is_empty() {
            commands.push(PlatformCommand::PopulateTreeView {
                window_id,
                items: descriptors,
            });
        }
        commands.push(PlatformCommand::ShowWindow { window_id });
        commands
    }

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
     * Creates a `Profile` object based on the current application state.
     * This includes the current root scan path, sets of selected and deselected file paths,
     * and the provided new profile name. Whitelist patterns are no longer part of the profile.
     */
    fn create_profile_from_current_state(&self, new_profile_name: String) -> Profile {
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();

        Self::gather_selected_deselected_paths_recursive(
            &self.file_nodes_cache,
            &mut selected_paths,
            &mut deselected_paths,
        );

        Profile {
            name: new_profile_name,
            root_folder: self.root_path_for_scan.clone(),
            selected_paths,
            deselected_paths,
            archive_path: self.current_profile_name.as_ref().and_then(|name| {
                // Attempt to carry over archive_path
                core::load_profile(name, APP_NAME_FOR_PROFILES)
                    .ok()
                    .and_then(|p| p.archive_path)
            }),
        }
    }

    fn refresh_tree_view_from_cache(&mut self, window_id: WindowId) -> Option<PlatformCommand> {
        self.next_tree_item_id_counter = 1;
        self.path_to_tree_item_id.clear();
        let descriptors = Self::build_tree_item_descriptors_recursive(
            &self.file_nodes_cache,
            &mut self.path_to_tree_item_id,
            &mut self.next_tree_item_id_counter,
        );
        if descriptors.is_empty() && self.file_nodes_cache.is_empty() {
            Some(PlatformCommand::PopulateTreeView {
                window_id,
                items: vec![],
            })
        } else if !descriptors.is_empty() {
            Some(PlatformCommand::PopulateTreeView {
                window_id,
                items: descriptors,
            })
        } else {
            None
        }
    }

    fn find_filenode_mut<'a>(
        nodes: &'a mut [FileNode],
        path_to_find: &Path,
    ) -> Option<&'a mut FileNode> {
        for node in nodes.iter_mut() {
            if node.path == path_to_find {
                return Some(node);
            }
            if node.is_dir && !node.children.is_empty() {
                if let Some(found_in_child) =
                    Self::find_filenode_mut(&mut node.children, path_to_find)
                {
                    return Some(found_in_child);
                }
            }
        }
        None
    }

    fn find_filenode_ref<'a>(nodes: &'a [FileNode], path_to_find: &Path) -> Option<&'a FileNode> {
        for node in nodes.iter() {
            if node.path == path_to_find {
                return Some(node);
            }
            if node.is_dir && !node.children.is_empty() {
                if let Some(found_in_child) = Self::find_filenode_ref(&node.children, path_to_find)
                {
                    return Some(found_in_child);
                }
            }
        }
        None
    }

    fn collect_visual_updates_recursive(
        &self,
        node: &FileNode,
        updates: &mut Vec<(TreeItemId, CheckState)>,
    ) {
        if let Some(item_id) = self.path_to_tree_item_id.get(&node.path) {
            let check_state = match node.state {
                FileState::Selected => CheckState::Checked,
                _ => CheckState::Unchecked,
            };
            updates.push((*item_id, check_state));

            if node.is_dir {
                for child in &node.children {
                    self.collect_visual_updates_recursive(child, updates);
                }
            }
        } else {
            eprintln!(
                "AppLogic: Could not find TreeItemId for path {:?} during visual update collection.",
                node.path
            );
        }
    }
}

impl PlatformEventHandler for MyAppLogic {
    /*
     * Primary event handling method for the application logic.
     * This method receives platform-agnostic UI events, updates the internal application
     * state accordingly, and returns a list of platform commands to effect changes in the native UI.
     */
    fn handle_event(&mut self, event: AppEvent) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        match event {
            AppEvent::WindowCloseRequested { window_id } => {
                if self.main_window_id == Some(window_id) {
                    println!(
                        "AppLogic: Main window close requested. Commanding platform to close."
                    );
                    commands.push(PlatformCommand::CloseWindow { window_id });
                }
            }
            AppEvent::WindowDestroyed { window_id } => {
                if self.main_window_id == Some(window_id) {
                    println!("AppLogic: Main window destroyed notification received.");
                    self.main_window_id = None;
                }
            }
            AppEvent::TreeViewItemToggled {
                window_id,
                item_id,
                new_state,
            } => {
                println!(
                    "AppLogic: TreeItem {:?} in window {:?} toggled to UI state {:?}.",
                    item_id, window_id, new_state
                );

                let mut path_of_toggled_node: Option<PathBuf> = None;
                for (path_candidate, id_in_map) in &self.path_to_tree_item_id {
                    if *id_in_map == item_id {
                        path_of_toggled_node = Some(path_candidate.clone());
                        break;
                    }
                }

                if let Some(path_for_model_update) = path_of_toggled_node {
                    {
                        let node_to_update_model_for = Self::find_filenode_mut(
                            &mut self.file_nodes_cache,
                            &path_for_model_update,
                        );

                        if let Some(node_model) = node_to_update_model_for {
                            let new_model_file_state = match new_state {
                                CheckState::Checked => FileState::Selected,
                                CheckState::Unchecked => FileState::Deselected,
                            };
                            core::state_manager::update_folder_selection(
                                node_model,
                                new_model_file_state,
                            );
                        } else {
                            eprintln!(
                                "AppLogic: Model node not found for path {:?} to update state.",
                                path_for_model_update
                            );
                        }
                    }

                    if let Some(root_node_for_visual_update) =
                        Self::find_filenode_ref(&self.file_nodes_cache, &path_for_model_update)
                    {
                        let mut visual_updates_list = Vec::new();
                        self.collect_visual_updates_recursive(
                            root_node_for_visual_update,
                            &mut visual_updates_list,
                        );
                        println!(
                            "AppLogic: Requesting {} visual updates for TreeView after toggle.",
                            visual_updates_list.len()
                        );
                        for (id_to_update_ui, state_for_ui) in visual_updates_list {
                            commands.push(PlatformCommand::UpdateTreeItemVisualState {
                                window_id,
                                item_id: id_to_update_ui,
                                new_state: state_for_ui,
                            });
                        }
                    } else {
                        eprintln!(
                            "AppLogic: Model node not found for path {:?} to collect visual updates.",
                            path_for_model_update
                        );
                    }
                } else {
                    eprintln!(
                        "AppLogic: Could not find path for TreeItemId {:?} from UI event.",
                        item_id
                    );
                }
            }
            AppEvent::ButtonClicked {
                window_id,
                control_id,
            } => {
                if self.main_window_id == Some(window_id)
                    && control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC
                {
                    println!("AppLogic: 'Generate Archive' button clicked.");
                    match core::create_archive_content(
                        &self.file_nodes_cache,
                        &self.root_path_for_scan,
                    ) {
                        Ok(content) => {
                            self.pending_archive_content = Some(content);
                            self.pending_action = Some(PendingAction::SavingArchive);
                            commands.push(PlatformCommand::ShowSaveFileDialog {
                                window_id,
                                title: "Save Archive As".to_string(),
                                default_filename: "archive.txt".to_string(),
                                filter_spec: "Text Files (*.txt)\0*.txt\0All Files (*.*)\0*.*\0\0"
                                    .to_string(),
                                initial_dir: None,
                            });
                        }
                        Err(e) => {
                            eprintln!("AppLogic: Failed to create archive content: {}", e);
                        }
                    }
                }
            }
            AppEvent::MenuLoadProfileClicked => {
                println!("AppLogic: MenuLoadProfileClicked received.");
                if let Some(main_id) = self.main_window_id {
                    let profile_dir = core::profiles::get_profile_dir(APP_NAME_FOR_PROFILES);
                    commands.push(PlatformCommand::ShowOpenFileDialog {
                        window_id: main_id,
                        title: "Load Profile".to_string(),
                        filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                        initial_dir: profile_dir.map(|p| p.to_path_buf()), // Ensure Option<PathBuf>
                    });
                }
            }

            AppEvent::FileOpenDialogCompleted { window_id, result } => {
                if self.main_window_id == Some(window_id) {
                    if let Some(profile_file_path) = result {
                        println!(
                            "AppLogic: Profile selected for load: {:?}",
                            profile_file_path
                        );
                        if let Some(profile_name_osstr) = profile_file_path.file_stem() {
                            if let Some(profile_name_str) =
                                profile_name_osstr.to_str().map(|s| s.to_string())
                            {
                                match core::load_profile(&profile_name_str, APP_NAME_FOR_PROFILES) {
                                    Ok(loaded_profile) => {
                                        println!(
                                            "AppLogic: Successfully loaded profile '{}'",
                                            loaded_profile.name
                                        );
                                        self.current_profile_name =
                                            Some(loaded_profile.name.clone());
                                        self.root_path_for_scan =
                                            loaded_profile.root_folder.clone();

                                        match core::scan_directory(&self.root_path_for_scan) {
                                            Ok(nodes) => {
                                                self.file_nodes_cache = nodes;
                                                core::apply_profile_to_tree(
                                                    &mut self.file_nodes_cache,
                                                    &loaded_profile,
                                                );
                                                if let Some(cmd) =
                                                    self.refresh_tree_view_from_cache(window_id)
                                                {
                                                    commands.push(cmd);
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "AppLogic: Error rescanning dir for profile: {}",
                                                    e
                                                );
                                                self.file_nodes_cache.clear();
                                                if let Some(cmd) =
                                                    self.refresh_tree_view_from_cache(window_id)
                                                {
                                                    commands.push(cmd);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => eprintln!(
                                        "AppLogic: Failed to load profile '{}': {}",
                                        profile_name_str, e
                                    ),
                                }
                            } else {
                                eprintln!(
                                    "AppLogic: Profile filename stem not valid UTF-8: {:?}",
                                    profile_file_path
                                );
                            }
                        } else {
                            eprintln!(
                                "AppLogic: Could not extract profile name from path: {:?}",
                                profile_file_path
                            );
                        }
                    } else {
                        println!("AppLogic: Load profile dialog cancelled.");
                    }
                }
            }

            AppEvent::MenuSaveProfileAsClicked => {
                println!("AppLogic: MenuSaveProfileAsClicked received.");
                if let Some(main_id) = self.main_window_id {
                    let profile_dir = core::profiles::get_profile_dir(APP_NAME_FOR_PROFILES);
                    let sanitized_current_name = self.current_profile_name.as_ref().map_or_else(
                        || "new_profile".to_string(),
                        |name| core::profiles::sanitize_profile_name(name),
                    );
                    let default_filename = format!("{}.json", sanitized_current_name);

                    self.pending_action = Some(PendingAction::SavingProfile);
                    commands.push(PlatformCommand::ShowSaveFileDialog {
                        window_id: main_id,
                        title: "Save Profile As".to_string(),
                        default_filename,
                        filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                        initial_dir: profile_dir.map(|p| p.to_path_buf()), // Ensure Option<PathBuf>
                    });
                }
            }

            AppEvent::FileSaveDialogCompleted { window_id, result } => {
                if self.main_window_id == Some(window_id) {
                    match self.pending_action.take() {
                        Some(PendingAction::SavingArchive) => {
                            if let Some(path) = result {
                                if let Some(content) = self.pending_archive_content.take() {
                                    println!("AppLogic: Saving archive to {:?}", path);
                                    match fs::write(&path, content) {
                                        Ok(_) => println!(
                                            "AppLogic: Successfully saved archive to {:?}",
                                            path
                                        ),
                                        Err(e) => eprintln!(
                                            "AppLogic: Failed to write archive to {:?}: {}",
                                            path, e
                                        ),
                                    }
                                } else {
                                    eprintln!("AppLogic: SaveArchiveDialog - No pending content.");
                                }
                            } else {
                                println!("AppLogic: Save archive dialog cancelled.");
                                self.pending_archive_content = None;
                            }
                        }
                        Some(PendingAction::SavingProfile) => {
                            if let Some(profile_save_path) = result {
                                println!(
                                    "AppLogic: Profile save path selected: {:?}",
                                    profile_save_path
                                );
                                if let Some(profile_name_osstr) = profile_save_path.file_stem() {
                                    if let Some(profile_name_str) =
                                        profile_name_osstr.to_str().map(|s| s.to_string())
                                    {
                                        let new_profile = self
                                            .create_profile_from_current_state(profile_name_str);
                                        match core::save_profile(
                                            &new_profile,
                                            APP_NAME_FOR_PROFILES,
                                        ) {
                                            Ok(()) => {
                                                println!(
                                                    "AppLogic: Successfully saved profile as '{}'",
                                                    new_profile.name
                                                );
                                                self.current_profile_name = Some(new_profile.name);
                                                // TODO: Update status bar
                                            }
                                            Err(e) => eprintln!(
                                                "AppLogic: Failed to save profile as '{}': {}",
                                                new_profile.name, e
                                            ),
                                        }
                                    } else {
                                        eprintln!(
                                            "AppLogic: Profile save filename stem not valid UTF-8"
                                        );
                                    }
                                } else {
                                    eprintln!(
                                        "AppLogic: Could not extract profile name from save path"
                                    );
                                }
                            } else {
                                println!("AppLogic: Save profile dialog cancelled.");
                            }
                        }
                        None => {
                            eprintln!(
                                "AppLogic: FileSaveDialogCompleted received but no pending action was set."
                            );
                            self.pending_archive_content = None;
                        }
                    }
                }
            }
            AppEvent::WindowResized { .. } => {}
        }
        commands
    }

    fn on_quit(&mut self) {
        println!("AppLogic: on_quit called by platform. Application is exiting.");
    }
}

// Unit tests for app_logic::handler
// Note: Some tests might need adjustment if they implicitly relied on whitelist_patterns
// (e.g., by setting it up in MyAppLogic for a test).
// For now, the existing tests mostly focus on event->command flow.
#[cfg(test)]
mod handler_tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn test_handle_button_click_generates_save_dialog() {
        let mut logic = MyAppLogic::new();
        logic.main_window_id = Some(WindowId(1));
        logic.file_nodes_cache = vec![];
        logic.root_path_for_scan = PathBuf::from(".");

        let cmds = logic.handle_event(AppEvent::ButtonClicked {
            window_id: WindowId(1),
            control_id: ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
        });
        assert!(matches!(
            cmds.as_slice(),
            [PlatformCommand::ShowSaveFileDialog { .. }]
        ));
    }

    #[test]
    fn test_handle_file_save_dialog_completed_with_path() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);
        logic.pending_archive_content = Some("ARCHIVE CONTENT".to_string());

        // Create a temp file and pass its path to the event
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: main_id,
            result: Some(path.clone()),
        });

        // No follow-up commands expected
        assert!(cmds.is_empty());
        // pending_archive_content should be cleared
        assert_eq!(logic.pending_archive_content, None);
        // And the file should have been written
        let written = fs::read_to_string(path).unwrap();
        assert_eq!(written, "ARCHIVE CONTENT");
    }

    #[test]
    fn test_handle_file_save_dialog_cancelled() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);
        logic.pending_archive_content = Some("WILL BE CLEARED".to_string());

        // Simulate user cancelling the save dialog
        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: main_id,
            result: None,
        });

        assert!(cmds.is_empty());
        // pending content should be cleared on cancel
        assert_eq!(logic.pending_archive_content, None);
    }

    #[test]
    fn test_handle_treeview_item_toggled_updates_model_and_emits_visual_update() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);

        // Prepare the cache + mapping
        let foo_path = PathBuf::from("/tmp/foo");
        logic.file_nodes_cache = vec![FileNode::new(foo_path.clone(), "foo".into(), false)];
        logic
            .path_to_tree_item_id
            .insert(foo_path.clone(), TreeItemId(42));

        let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
            window_id: main_id,
            item_id: TreeItemId(42),
            new_state: CheckState::Checked,
        });

        // Exactly one command?
        assert_eq!(cmds.len(), 1);

        // Destructure it and compare each field
        match &cmds[0] {
            PlatformCommand::UpdateTreeItemVisualState {
                window_id,
                item_id,
                new_state,
            } => {
                assert_eq!(*window_id, main_id);
                assert_eq!(*item_id, TreeItemId(42));
                assert_eq!(*new_state, CheckState::Checked);
            }
            other => panic!("expected UpdateTreeItemVisualState, got {:?}", other),
        }
    }
    #[test]
    fn test_handle_window_close_requested_generates_close_command() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(7);
        logic.main_window_id = Some(main_id);

        let cmds = logic.handle_event(AppEvent::WindowCloseRequested { window_id: main_id });

        // Exactly one command?
        assert_eq!(cmds.len(), 1);

        // Destructure it and compare the field
        match &cmds[0] {
            PlatformCommand::CloseWindow { window_id } => {
                assert_eq!(*window_id, main_id);
            }
            other => panic!("expected CloseWindow, got {:?}", other),
        }
    }

    #[test]
    fn test_handle_window_destroyed_clears_main_window_id() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(7);
        logic.main_window_id = Some(main_id);

        let cmds = logic.handle_event(AppEvent::WindowDestroyed { window_id: main_id });

        // No commands, but main_window_id should be dropped
        assert!(cmds.is_empty());
        assert_eq!(logic.main_window_id, None);
    }

    /// Helper to build a simple tree:
    /// root (dir)
    ///  ├─ file1 (file)
    ///  └─ sub  (dir)
    ///       └─ file2 (file)
    fn make_test_tree() -> (MyAppLogic, PathBuf, PathBuf) {
        let mut logic = MyAppLogic::new();

        let root_p = PathBuf::from("/root");
        let file1_p = root_p.join("file1.txt");
        let sub_p = root_p.join("sub");
        let file2_p = sub_p.join("file2.txt");

        // Build nodes
        let mut root = FileNode::new(root_p.clone(), "root".into(), true);
        let file1 = FileNode::new(file1_p.clone(), "file1.txt".into(), false);
        let mut sub = FileNode::new(sub_p.clone(), "sub".into(), true);
        let file2 = FileNode::new(file2_p.clone(), "file2.txt".into(), false);

        sub.children.push(file2);
        root.children.push(file1);
        root.children.push(sub);
        logic.file_nodes_cache = vec![root];

        // Prepare empty ID map & counter
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();

        (logic, file1_p, file2_p)
    }

    #[test]
    fn test_build_tree_item_descriptors_recursive() {
        let (mut logic, _, _) = make_test_tree();

        // Fire off descriptor build
        let descriptors = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );
        // Should have two top-level descriptors: "root" only
        assert_eq!(descriptors.len(), 1);

        // Unpack the single root descriptor
        let root_desc = &descriptors[0];
        assert_eq!(root_desc.text, "root");
        assert!(root_desc.is_folder);
        assert!(matches!(root_desc.state, CheckState::Unchecked));
        // It should have two children descriptors
        assert_eq!(root_desc.children.len(), 2);

        // Verify first child is file1.txt
        let first = &root_desc.children[0];
        assert_eq!(first.text, "file1.txt");
        assert!(!first.is_folder);
        assert!(matches!(first.state, CheckState::Unchecked));

        // And second child is the "sub" folder
        let second = &root_desc.children[1];
        assert_eq!(second.text, "sub");
        assert!(second.is_folder);
    }
    // :contentReference[oaicite:0]{index=0}&#8203;:contentReference[oaicite:1]{index=1}

    #[test]
    fn test_find_filenode_mut_and_ref() {
        let (mut logic, file1_p, file2_p) = make_test_tree();

        // Mutable find on file1.txt
        let file1_node = MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p);
        assert!(file1_node.is_some());
        file1_node.unwrap().state = FileState::Selected;

        // Immutable find sees the change
        let file1_ref = MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &file1_p);
        assert!(file1_ref.is_some());
        assert_eq!(file1_ref.unwrap().state, FileState::Selected);

        // Non-existent path returns None
        let none_node =
            MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &PathBuf::from("/no/such/path"));
        assert!(none_node.is_none());
    }
    // :contentReference[oaicite:2]{index=2}&#8203;:contentReference[oaicite:3]{index=3}

    #[test]
    fn test_collect_visual_updates_recursive() {
        let (mut logic, file1_p, file2_p) = make_test_tree();

        // First, populate the ID map so collect_visual_updates has something to look up
        let _ = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );

        // Toggle file1.txt to Selected in the model
        {
            let f1 = MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p).unwrap();
            f1.state = FileState::Selected;
        }

        // Now collect updates starting at root
        let root_node = &logic.file_nodes_cache[0];
        let mut updates = Vec::new();
        logic.collect_visual_updates_recursive(root_node, &mut updates);

        // We expect three entries: root, file1, sub and its child
        // But only file1.txt shows as Checked; others Unchecked
        // Find the tuple for file1
        assert!(updates.iter().any(|(id, state)| {
            *state == CheckState::Checked
                && *id == *logic.path_to_tree_item_id.get(&file1_p).unwrap()
        }));
        // And a sample unordered check for an Unchecked entry (sub folder)
        assert!(
            updates
                .iter()
                .any(|(_, state)| { matches!(state, CheckState::Unchecked) })
        );
    }
    // :contentReference[oaicite:4]{index=4}&#8203;:contentReference[oaicite:5]{index=5}
}
