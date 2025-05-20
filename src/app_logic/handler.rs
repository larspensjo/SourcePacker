use crate::core::{
    self, ArchiveStatus, ConfigError, ConfigManagerOperations, FileNode, FileState, Profile,
    ProfileError,
};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const ID_BUTTON_GENERATE_ARCHIVE_LOGIC: i32 = 1002;
// Made pub(crate) for access from handler_tests.rs
pub(crate) const APP_NAME_FOR_PROFILES: &str = "SourcePackerApp";

type PathToTreeItemIdMap = HashMap<PathBuf, TreeItemId>;

// Made pub(crate) for access from handler_tests.rs
#[derive(Debug)]
pub(crate) enum PendingAction {
    SavingArchive,
    SavingProfile,
}

/*
 * Manages the core application state and UI logic in a platform-agnostic manner.
 * It processes UI events received from the platform layer and generates commands
 * to update the UI. It depends on a `ConfigManagerOperations` trait for handling
 * application configuration, such as loading the last used profile.
 */
pub struct MyAppLogic {
    pub(crate) main_window_id: Option<WindowId>,
    pub(crate) file_nodes_cache: Vec<FileNode>,
    pub(crate) path_to_tree_item_id: PathToTreeItemIdMap,
    pub(crate) next_tree_item_id_counter: u64,
    pub(crate) root_path_for_scan: PathBuf,
    pub(crate) current_profile_name: Option<String>,
    pub(crate) current_profile_cache: Option<Profile>,
    pub(crate) current_archive_status: Option<ArchiveStatus>,
    pub(crate) pending_archive_content: Option<String>,
    pub(crate) pending_action: Option<PendingAction>,
    pub(crate) config_manager: Arc<dyn ConfigManagerOperations>,
}

impl MyAppLogic {
    /*
     * Initializes a new instance of the application logic.
     * It requires a `ConfigManagerOperations` implementation to handle loading
     * and saving application configuration (e.g., the last used profile).
     * Sets up default values for other application states.
     */
    pub fn new(config_manager: Arc<dyn ConfigManagerOperations>) -> Self {
        MyAppLogic {
            main_window_id: None,
            file_nodes_cache: Vec::new(),
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
            root_path_for_scan: PathBuf::from("."),
            current_profile_name: None,
            current_profile_cache: None,
            current_archive_status: None,
            pending_archive_content: None,
            pending_action: None,
            config_manager,
        }
    }

    // Tests for tree item ID generation would be indirect.
    fn generate_tree_item_id(&mut self) -> TreeItemId {
        let id = self.next_tree_item_id_counter;
        self.next_tree_item_id_counter += 1;
        TreeItemId(id)
    }

    pub(crate) fn build_tree_item_descriptors_recursive(
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
     * Handles the event indicating the main application window has been created.
     * It attempts to load the last used profile using the configured `ConfigManagerOperations`.
     * If successful, it uses that profile's settings for the initial directory scan and state
     * application. Otherwise, it proceeds with a default scan. Finally, it populates the UI
     * and shows the window.
     */
    pub fn on_main_window_created(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        self.main_window_id = Some(window_id);
        let mut commands = Vec::new();

        // P2.6: Attempt to load the last used profile
        let mut loaded_profile_on_startup = false;
        match self
            .config_manager
            .load_last_profile_name(APP_NAME_FOR_PROFILES)
        {
            Ok(Some(last_profile_name)) => {
                println!(
                    "AppLogic: Found last used profile name: {}",
                    last_profile_name
                );
                match core::load_profile(&last_profile_name, APP_NAME_FOR_PROFILES) {
                    Ok(profile) => {
                        println!(
                            "AppLogic: Successfully loaded last profile '{}' on startup.",
                            profile.name
                        );
                        self.current_profile_name = Some(profile.name.clone());
                        self.root_path_for_scan = profile.root_folder.clone();
                        self.current_profile_cache = Some(profile);
                        loaded_profile_on_startup = true;
                    }
                    Err(e) => {
                        eprintln!(
                            "AppLogic: Failed to load last profile '{}': {:?}. Proceeding with default.",
                            last_profile_name, e
                        );
                        self.current_profile_name = None;
                        self.current_profile_cache = None;
                    }
                }
            }
            Ok(None) => {
                println!("AppLogic: No last profile name found. Proceeding with default state.");
            }
            Err(e) => {
                eprintln!(
                    "AppLogic: Error loading last profile name: {:?}. Proceeding with default.",
                    e
                );
            }
        }

        println!(
            "AppLogic: Initial scan of directory {:?}",
            self.root_path_for_scan
        );

        match core::scan_directory(&self.root_path_for_scan) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                println!(
                    "AppLogic: Scanned {} top-level nodes.",
                    self.file_nodes_cache.len()
                );

                if loaded_profile_on_startup {
                    if let Some(profile) = &self.current_profile_cache {
                        core::apply_profile_to_tree(&mut self.file_nodes_cache, profile);
                        println!("AppLogic: Applied loaded profile to the scanned tree.");
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "AppLogic: Failed to scan directory {:?}: {}",
                    self.root_path_for_scan, e
                );
                let error_node_path = PathBuf::from("/error_node_scan_failed");
                self.file_nodes_cache = vec![FileNode::new(
                    error_node_path,
                    format!("Error scanning directory: {}", e),
                    false,
                )];
            }
        }

        self.update_current_archive_status();

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
        } else if self.file_nodes_cache.is_empty() {
            commands.push(PlatformCommand::PopulateTreeView {
                window_id,
                items: vec![],
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
            archive_path: self
                .current_profile_cache
                .as_ref()
                .and_then(|p| p.archive_path.clone()),
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
        Some(PlatformCommand::PopulateTreeView {
            window_id,
            items: descriptors,
        })
    }

    fn update_current_archive_status(&mut self) {
        if let Some(profile) = &self.current_profile_cache {
            let status = core::check_archive_status(profile, &self.file_nodes_cache);
            self.current_archive_status = Some(status);
            println!("AppLogic: Archive status updated to: {:?}", status);
        } else {
            self.current_archive_status = None;
            println!("AppLogic: No profile loaded, archive status cleared.");
        }
    }

    pub(crate) fn find_filenode_mut<'a>(
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

    pub(crate) fn find_filenode_ref<'a>(
        nodes: &'a [FileNode],
        path_to_find: &Path,
    ) -> Option<&'a FileNode> {
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

    pub(crate) fn collect_visual_updates_recursive(
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
                    self.current_profile_name = None;
                    self.current_profile_cache = None;
                    self.current_archive_status = None;
                    self.file_nodes_cache.clear();
                    self.path_to_tree_item_id.clear();
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
                    self.update_current_archive_status();
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
                    let display_root_path = self.current_profile_cache.as_ref().map_or_else(
                        || self.root_path_for_scan.clone(),
                        |p| p.root_folder.clone(),
                    );

                    match core::create_archive_content(&self.file_nodes_cache, &display_root_path) {
                        Ok(content) => {
                            self.pending_archive_content = Some(content);
                            self.pending_action = Some(PendingAction::SavingArchive);

                            let default_filename = self
                                .current_profile_cache
                                .as_ref()
                                .map(|p| core::profiles::sanitize_profile_name(&p.name) + ".txt")
                                .unwrap_or_else(|| "archive.txt".to_string());

                            let initial_dir_for_dialog = self
                                .current_profile_cache
                                .as_ref()
                                .and_then(|p| {
                                    p.archive_path
                                        .as_ref()
                                        .and_then(|ap| ap.parent().map(PathBuf::from))
                                })
                                .or_else(|| {
                                    self.current_profile_cache
                                        .as_ref()
                                        .map(|p| p.root_folder.clone())
                                })
                                .or_else(|| Some(self.root_path_for_scan.clone()));

                            commands.push(PlatformCommand::ShowSaveFileDialog {
                                window_id,
                                title: "Save Archive As".to_string(),
                                default_filename,
                                filter_spec: "Text Files (*.txt)\0*.txt\0All Files (*.*)\0*.*\0\0"
                                    .to_string(),
                                initial_dir: initial_dir_for_dialog,
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
                    let profile_dir_res = core::profiles::get_profile_dir(APP_NAME_FOR_PROFILES);
                    commands.push(PlatformCommand::ShowOpenFileDialog {
                        window_id: main_id,
                        title: "Load Profile".to_string(),
                        filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                        initial_dir: profile_dir_res,
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
                        match File::open(&profile_file_path) {
                            Ok(file) => {
                                let reader = std::io::BufReader::new(file);
                                match serde_json::from_reader(reader) {
                                    Ok(loaded_profile) => {
                                        let profile: Profile = loaded_profile;
                                        println!(
                                            "AppLogic: Successfully loaded profile '{}' directly from path.",
                                            profile.name
                                        );
                                        self.current_profile_name = Some(profile.name.clone());
                                        self.root_path_for_scan = profile.root_folder.clone();
                                        self.current_profile_cache = Some(profile.clone());

                                        if let Err(e) = self.config_manager.save_last_profile_name(
                                            APP_NAME_FOR_PROFILES,
                                            &profile.name,
                                        ) {
                                            eprintln!(
                                                "AppLogic: Failed to save last profile name '{}': {:?}",
                                                profile.name, e
                                            );
                                        }

                                        match core::scan_directory(&self.root_path_for_scan) {
                                            Ok(nodes) => {
                                                self.file_nodes_cache = nodes;
                                                core::apply_profile_to_tree(
                                                    &mut self.file_nodes_cache,
                                                    &profile,
                                                );
                                                if let Some(cmd) =
                                                    self.refresh_tree_view_from_cache(window_id)
                                                {
                                                    commands.push(cmd);
                                                }
                                                self.update_current_archive_status();
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
                                                self.current_archive_status = None;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "AppLogic: Failed to deserialize profile from {:?}: {}",
                                            profile_file_path, e
                                        );
                                        self.current_profile_name = None;
                                        self.current_profile_cache = None;
                                        self.current_archive_status = None;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "AppLogic: Failed to open profile file {:?}: {}",
                                    profile_file_path, e
                                );
                                self.current_profile_name = None;
                                self.current_profile_cache = None;
                                self.current_archive_status = None;
                            }
                        }
                    } else {
                        println!("AppLogic: Load profile dialog cancelled.");
                    }
                }
            }

            AppEvent::MenuSaveProfileAsClicked => {
                println!("AppLogic: MenuSaveProfileAsClicked received.");
                if let Some(main_id) = self.main_window_id {
                    let profile_dir_res = core::profiles::get_profile_dir(APP_NAME_FOR_PROFILES);
                    let base_name = self
                        .current_profile_name
                        .as_ref()
                        .map_or_else(|| "new_profile".to_string(), |name| name.clone());
                    let sanitized_current_name = core::profiles::sanitize_profile_name(&base_name);
                    let default_filename = format!("{}.json", sanitized_current_name);

                    self.pending_action = Some(PendingAction::SavingProfile);
                    commands.push(PlatformCommand::ShowSaveFileDialog {
                        window_id: main_id,
                        title: "Save Profile As".to_string(),
                        default_filename,
                        filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                        initial_dir: profile_dir_res,
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
                                        Ok(_) => {
                                            println!(
                                                "AppLogic: Successfully saved archive to {:?}",
                                                path
                                            );
                                            if let Some(profile) = &mut self.current_profile_cache {
                                                profile.archive_path = Some(path.clone());
                                                match core::save_profile(
                                                    profile,
                                                    APP_NAME_FOR_PROFILES,
                                                ) {
                                                    Ok(_) => println!(
                                                        "AppLogic: Profile updated with new archive path."
                                                    ),
                                                    Err(e) => eprintln!(
                                                        "AppLogic: Failed to save profile after updating archive path: {}",
                                                        e
                                                    ),
                                                }
                                            }
                                            self.update_current_archive_status();
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "AppLogic: Failed to write archive to {:?}: {}",
                                                path, e
                                            );
                                        }
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
                                        let new_profile = self.create_profile_from_current_state(
                                            profile_name_str.clone(),
                                        );

                                        match core::save_profile(
                                            &new_profile,
                                            APP_NAME_FOR_PROFILES,
                                        ) {
                                            Ok(()) => {
                                                println!(
                                                    "AppLogic: Successfully saved profile as '{}'",
                                                    new_profile.name
                                                );
                                                self.current_profile_name =
                                                    Some(new_profile.name.clone());
                                                self.current_profile_cache =
                                                    Some(new_profile.clone());
                                                self.root_path_for_scan = self
                                                    .current_profile_cache
                                                    .as_ref()
                                                    .unwrap()
                                                    .root_folder
                                                    .clone();

                                                if let Err(e) =
                                                    self.config_manager.save_last_profile_name(
                                                        APP_NAME_FOR_PROFILES,
                                                        &new_profile.name,
                                                    )
                                                {
                                                    eprintln!(
                                                        "AppLogic: Failed to save last profile name '{}': {:?}",
                                                        new_profile.name, e
                                                    );
                                                }
                                                self.update_current_archive_status();
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "AppLogic: Failed to save profile as '{}': {}",
                                                    new_profile.name, e
                                                );
                                            }
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
