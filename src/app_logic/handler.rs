use crate::core::{
    self, ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations, FileNode,
    FileState, FileSystemScannerOperations, Profile, ProfileError, ProfileManagerOperations,
    StateManagerOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const ID_BUTTON_GENERATE_ARCHIVE_LOGIC: i32 = 1002;
// Made pub(crate) for access from handler_tests.rs
pub(crate) const APP_NAME_FOR_PROFILES: &str = "SourcePacker";

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
 * to update the UI. It depends on `ConfigManagerOperations` for app configuration,
 * `ProfileManagerOperations` for profile data, `FileSystemScannerOperations`
 * for directory scanning, `ArchiverOperations` for archiving, and
 * `StateManagerOperations` for tree state management.
 */
pub struct MyAppLogic {
    main_window_id: Option<WindowId>,
    file_nodes_cache: Vec<FileNode>,
    path_to_tree_item_id: PathToTreeItemIdMap,
    next_tree_item_id_counter: u64,
    root_path_for_scan: PathBuf,
    current_profile_name: Option<String>,
    current_profile_cache: Option<Profile>,
    current_archive_status: Option<ArchiveStatus>,
    pending_archive_content: Option<String>,
    pending_action: Option<PendingAction>,
    config_manager: Arc<dyn ConfigManagerOperations>,
    profile_manager: Arc<dyn ProfileManagerOperations>,
    file_system_scanner: Arc<dyn FileSystemScannerOperations>,
    archiver: Arc<dyn ArchiverOperations>,
    state_manager: Arc<dyn StateManagerOperations>,
}

impl MyAppLogic {
    /*
     * Initializes a new instance of the application logic.
     * Requires implementations for `ConfigManagerOperations` (app config),
     * `ProfileManagerOperations` (profile data), `FileSystemScannerOperations` (directory scanning),
     * `ArchiverOperations` (archiving), and `StateManagerOperations` (tree state management).
     * Sets up default application states.
     */
    pub fn new(
        config_manager: Arc<dyn ConfigManagerOperations>,
        profile_manager: Arc<dyn ProfileManagerOperations>,
        file_system_scanner: Arc<dyn FileSystemScannerOperations>,
        archiver: Arc<dyn ArchiverOperations>,
        state_manager: Arc<dyn StateManagerOperations>,
    ) -> Self {
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
            profile_manager,
            file_system_scanner,
            archiver,
            state_manager,
        }
    }

    fn generate_tree_item_id(&mut self) -> TreeItemId {
        let id = self.next_tree_item_id_counter;
        self.next_tree_item_id_counter += 1;
        TreeItemId(id)
    }

    pub(crate) fn build_tree_item_descriptors_recursive(&mut self) -> Vec<TreeItemDescriptor> {
        return Self::build_tree_item_descriptors_recursive_internal(
            &self.file_nodes_cache,
            &mut self.path_to_tree_item_id,
            &mut self.next_tree_item_id_counter,
        );
    }

    fn build_tree_item_descriptors_recursive_internal(
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
                children: Self::build_tree_item_descriptors_recursive_internal(
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
     * It attempts to load the last used profile. If successful, it uses that profile's
     * settings for the initial directory scan and state application. Otherwise, it
     * proceeds with a default scan. Finally, it populates the UI and shows the window.
     */
    pub fn on_main_window_created(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        self.main_window_id = Some(window_id);
        let mut commands = Vec::new();
        let mut status_message = "Ready".to_string();
        let mut status_is_error = false;

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
                match self
                    .profile_manager
                    .load_profile(&last_profile_name, APP_NAME_FOR_PROFILES)
                {
                    Ok(profile) => {
                        println!(
                            "AppLogic: Successfully loaded last profile '{}' on startup via manager.",
                            profile.name
                        );
                        self.current_profile_name = Some(profile.name.clone());
                        self.root_path_for_scan = profile.root_folder.clone();
                        self.current_profile_cache = Some(profile);
                        loaded_profile_on_startup = true;
                        status_message = format!(
                            "Profile '{}' loaded.",
                            self.current_profile_name.as_ref().unwrap()
                        );
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Failed to load last profile '{}': {:?}. Using default.",
                            last_profile_name, e
                        );
                        eprintln!("AppLogic: {}", err_msg);
                        self.current_profile_name = None;
                        self.current_profile_cache = None;
                        status_message = err_msg;
                        status_is_error = true;
                    }
                }
            }
            Ok(None) => {
                println!("AppLogic: No last profile name found. Proceeding with default state.");
                status_message = "No last profile. Default state.".to_string();
            }
            Err(e) => {
                let err_msg = format!("Error loading last profile name: {:?}. Using default.", e);
                eprintln!("AppLogic: {}", err_msg);
                status_message = err_msg;
                status_is_error = true;
            }
        }

        println!(
            "AppLogic: Initial scan of directory {:?}",
            self.root_path_for_scan
        );

        match self
            .file_system_scanner
            .scan_directory(&self.root_path_for_scan)
        {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                println!(
                    "AppLogic: Scanned {} top-level nodes.",
                    self.file_nodes_cache.len()
                );

                if loaded_profile_on_startup {
                    if let Some(profile) = &self.current_profile_cache {
                        self.state_manager
                            .apply_profile_to_tree(&mut self.file_nodes_cache, profile);
                        println!(
                            "AppLogic: Applied loaded profile to the scanned tree via state_manager."
                        );
                    }
                }
                // If status_is_error is false, it means profile loading (if attempted) was okay.
                // If it's true, we keep the error message from profile loading.
                if !status_is_error {
                    status_message = format!(
                        "Scanned '{}'. {}",
                        self.root_path_for_scan.display(),
                        if loaded_profile_on_startup {
                            format!(
                                "Profile '{}' applied.",
                                self.current_profile_name
                                    .as_ref()
                                    .unwrap_or(&"".to_string())
                            )
                        } else {
                            "Default scan complete.".to_string()
                        }
                    );
                }
            }
            Err(e) => {
                let err_msg = format!(
                    "Failed to scan directory {:?}: {}",
                    self.root_path_for_scan, e
                );
                eprintln!("AppLogic: {}", err_msg);
                self.file_nodes_cache.clear(); // Keep it clear on scan error
                status_message = err_msg;
                status_is_error = true;
            }
        }

        self.update_current_archive_status();

        self.next_tree_item_id_counter = 1;
        self.path_to_tree_item_id.clear();
        let descriptors = self.build_tree_item_descriptors_recursive();

        if !descriptors.is_empty() {
            commands.push(PlatformCommand::PopulateTreeView {
                window_id,
                items: descriptors,
            });
        } else if self.file_nodes_cache.is_empty() {
            // Explicitly clear tree if scan resulted in empty (e.g., after an error)
            commands.push(PlatformCommand::PopulateTreeView {
                window_id,
                items: vec![],
            });
        }

        commands.push(PlatformCommand::UpdateStatusBarText {
            window_id,
            text: status_message,
            is_error: status_is_error,
        });
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
        let descriptors = Self::build_tree_item_descriptors_recursive_internal(
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
            let status = self
                .archiver
                .check_archive_status(profile, &self.file_nodes_cache);
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

    fn handle_window_close_requested(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        if self.main_window_id == Some(window_id) {
            println!("AppLogic: Main window close requested. Commanding platform to close.");
            commands.push(PlatformCommand::CloseWindow { window_id });
        }
        commands
    }

    fn handle_window_destroyed(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        if self.main_window_id == Some(window_id) {
            println!("AppLogic: Main window destroyed notification received.");
            self.main_window_id = None;
            self.current_profile_name = None;
            self.current_profile_cache = None;
            self.current_archive_status = None;
            self.file_nodes_cache.clear();
            self.path_to_tree_item_id.clear();
        }
        Vec::new()
    }

    fn handle_treeview_item_toggled(
        &mut self,
        window_id: WindowId,
        item_id: TreeItemId,
        new_state: CheckState,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
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
                let node_to_update_model_for =
                    Self::find_filenode_mut(&mut self.file_nodes_cache, &path_for_model_update);

                if let Some(node_model) = node_to_update_model_for {
                    let new_model_file_state = match new_state {
                        CheckState::Checked => FileState::Selected,
                        CheckState::Unchecked => FileState::Deselected,
                    };
                    self.state_manager
                        .update_folder_selection(node_model, new_model_file_state);
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
        commands
    }

    fn handle_button_clicked(
        &mut self,
        window_id: WindowId,
        control_id: i32,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        if self.main_window_id == Some(window_id) && control_id == ID_BUTTON_GENERATE_ARCHIVE_LOGIC
        {
            println!("AppLogic: 'Generate Archive' button clicked.");
            let display_root_path = self.current_profile_cache.as_ref().map_or_else(
                || self.root_path_for_scan.clone(),
                |p| p.root_folder.clone(),
            );

            match self
                .archiver
                .create_archive_content(&self.file_nodes_cache, &display_root_path)
            {
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
                    let err_msg = format!("Failed to create archive content: {}", e);
                    eprintln!("AppLogic: {}", err_msg);
                    commands.push(PlatformCommand::UpdateStatusBarText {
                        window_id,
                        text: err_msg,
                        is_error: true,
                    });
                }
            }
        }
        commands
    }

    fn handle_menu_load_profile_clicked(&mut self) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        println!("AppLogic: MenuLoadProfileClicked received.");
        if let Some(main_id) = self.main_window_id {
            let profile_dir_opt = self
                .profile_manager
                .get_profile_dir_path(APP_NAME_FOR_PROFILES);
            commands.push(PlatformCommand::ShowOpenFileDialog {
                window_id: main_id,
                title: "Load Profile".to_string(),
                filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                initial_dir: profile_dir_opt,
            });
        }
        commands
    }

    fn handle_file_open_dialog_completed(
        &mut self,
        window_id: WindowId,
        result: Option<PathBuf>,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        let mut status_update_cmd: Option<PlatformCommand> = None;

        if self.main_window_id == Some(window_id) {
            if let Some(profile_file_path) = result {
                println!(
                    "AppLogic: Profile selected for load: {:?}",
                    profile_file_path
                );
                match self
                    .profile_manager
                    .load_profile_from_path(&profile_file_path)
                {
                    Ok(loaded_profile) => {
                        let profile_name_clone = loaded_profile.name.clone(); // Clone for status message
                        println!(
                            "AppLogic: Successfully loaded profile '{}' via manager from path.",
                            loaded_profile.name
                        );
                        self.current_profile_name = Some(loaded_profile.name.clone());
                        self.root_path_for_scan = loaded_profile.root_folder.clone();
                        self.current_profile_cache = Some(loaded_profile.clone());

                        if let Err(e) = self
                            .config_manager
                            .save_last_profile_name(APP_NAME_FOR_PROFILES, &loaded_profile.name)
                        {
                            eprintln!(
                                "AppLogic: Failed to save last profile name '{}': {:?}",
                                loaded_profile.name, e
                            );
                            // Non-fatal, but could be a status message.
                        }

                        match self
                            .file_system_scanner
                            .scan_directory(&self.root_path_for_scan)
                        {
                            Ok(nodes) => {
                                self.file_nodes_cache = nodes;
                                self.state_manager.apply_profile_to_tree(
                                    &mut self.file_nodes_cache,
                                    &loaded_profile,
                                );
                                if let Some(cmd) = self.refresh_tree_view_from_cache(window_id) {
                                    commands.push(cmd);
                                }
                                self.update_current_archive_status();
                                status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                                    window_id,
                                    text: format!(
                                        "Profile '{}' loaded and scanned.",
                                        profile_name_clone
                                    ),
                                    is_error: false,
                                });
                            }
                            Err(e) => {
                                let err_msg = format!("Error rescanning dir for profile: {}", e);
                                eprintln!("AppLogic: {}", err_msg);
                                self.file_nodes_cache.clear();
                                if let Some(cmd) = self.refresh_tree_view_from_cache(window_id) {
                                    commands.push(cmd);
                                }
                                self.current_archive_status = None;
                                status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                                    window_id,
                                    text: err_msg,
                                    is_error: true,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Failed to load profile from {:?} via manager: {:?}",
                            profile_file_path, e
                        );
                        eprintln!("AppLogic: {}", err_msg);
                        self.current_profile_name = None;
                        self.current_profile_cache = None;
                        self.current_archive_status = None;
                        status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                            window_id,
                            text: err_msg,
                            is_error: true,
                        });
                    }
                }
            } else {
                println!("AppLogic: Load profile dialog cancelled.");
                status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                    window_id,
                    text: "Load profile cancelled.".to_string(),
                    is_error: false,
                });
            }
        }
        if let Some(cmd) = status_update_cmd {
            commands.push(cmd);
        }
        commands
    }

    fn handle_menu_save_profile_as_clicked(&mut self) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        println!("AppLogic: MenuSaveProfileAsClicked received.");
        if let Some(main_id) = self.main_window_id {
            let profile_dir_opt = self
                .profile_manager
                .get_profile_dir_path(APP_NAME_FOR_PROFILES);
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
                initial_dir: profile_dir_opt,
            });
        }
        commands
    }

    fn handle_file_save_dialog_completed(
        &mut self,
        window_id: WindowId,
        result: Option<PathBuf>,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new(); // Changed from Vec::new() to allow status updates.
        let mut status_update_cmd: Option<PlatformCommand> = None;

        if self.main_window_id == Some(window_id) {
            match self.pending_action.take() {
                Some(PendingAction::SavingArchive) => {
                    if let Some(path) = result {
                        if let Some(content) = self.pending_archive_content.take() {
                            println!("AppLogic: Saving archive to {:?}", path);
                            match self.archiver.save_archive_content(&path, &content) {
                                Ok(_) => {
                                    println!("AppLogic: Successfully saved archive to {:?}", path);
                                    if let Some(profile) = &mut self.current_profile_cache {
                                        profile.archive_path = Some(path.clone());
                                        match self
                                            .profile_manager
                                            .save_profile(profile, APP_NAME_FOR_PROFILES)
                                        {
                                            Ok(_) => println!(
                                                "AppLogic: Profile updated with new archive path via manager."
                                            ),
                                            Err(e) => eprintln!(
                                                "AppLogic: Failed to save profile (via manager) after updating archive path: {}",
                                                e
                                            ), // Could be a status bar error
                                        }
                                    }
                                    self.update_current_archive_status();
                                    status_update_cmd =
                                        Some(PlatformCommand::UpdateStatusBarText {
                                            window_id,
                                            text: format!("Archive saved to '{}'", path.display()),
                                            is_error: false,
                                        });
                                }
                                Err(e) => {
                                    let err_msg =
                                        format!("Failed to write archive to {:?}: {}", path, e);
                                    eprintln!("AppLogic: {}", err_msg);
                                    status_update_cmd =
                                        Some(PlatformCommand::UpdateStatusBarText {
                                            window_id,
                                            text: err_msg,
                                            is_error: true,
                                        });
                                }
                            }
                        } else {
                            eprintln!("AppLogic: SaveArchiveDialog - No pending content.");
                            status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                                window_id,
                                text: "Error: No archive content to save.".to_string(),
                                is_error: true,
                            });
                        }
                    } else {
                        println!("AppLogic: Save archive dialog cancelled.");
                        self.pending_archive_content = None; // Clear content if dialog cancelled
                        status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                            window_id,
                            text: "Save archive cancelled.".to_string(),
                            is_error: false,
                        });
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
                                    .create_profile_from_current_state(profile_name_str.clone());
                                let profile_name_clone = new_profile.name.clone(); // For status msg

                                match self
                                    .profile_manager
                                    .save_profile(&new_profile, APP_NAME_FOR_PROFILES)
                                {
                                    Ok(()) => {
                                        println!(
                                            "AppLogic: Successfully saved profile as '{}' via manager.",
                                            new_profile.name
                                        );
                                        self.current_profile_name = Some(new_profile.name.clone());
                                        self.current_profile_cache = Some(new_profile.clone());
                                        self.root_path_for_scan = self
                                            .current_profile_cache
                                            .as_ref()
                                            .unwrap()
                                            .root_folder
                                            .clone();

                                        if let Err(e) = self.config_manager.save_last_profile_name(
                                            APP_NAME_FOR_PROFILES,
                                            &new_profile.name,
                                        ) {
                                            eprintln!(
                                                "AppLogic: Failed to save last profile name '{}': {:?}",
                                                new_profile.name, e
                                            );
                                        }
                                        self.update_current_archive_status();
                                        status_update_cmd =
                                            Some(PlatformCommand::UpdateStatusBarText {
                                                window_id,
                                                text: format!(
                                                    "Profile '{}' saved.",
                                                    profile_name_clone
                                                ),
                                                is_error: false,
                                            });
                                    }
                                    Err(e) => {
                                        let err_msg = format!(
                                            "Failed to save profile (via manager) as '{}': {}",
                                            new_profile.name, e
                                        );
                                        eprintln!("AppLogic: {}", err_msg);
                                        status_update_cmd =
                                            Some(PlatformCommand::UpdateStatusBarText {
                                                window_id,
                                                text: err_msg,
                                                is_error: true,
                                            });
                                    }
                                }
                            } else {
                                let err_msg =
                                    "Profile save filename stem not valid UTF-8".to_string();
                                eprintln!("AppLogic: {}", err_msg);
                                status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                                    window_id,
                                    text: err_msg,
                                    is_error: true,
                                });
                            }
                        } else {
                            let err_msg =
                                "Could not extract profile name from save path".to_string();
                            eprintln!("AppLogic: {}", err_msg);
                            status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                                window_id,
                                text: err_msg,
                                is_error: true,
                            });
                        }
                    } else {
                        println!("AppLogic: Save profile dialog cancelled.");
                        status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                            window_id,
                            text: "Save profile cancelled.".to_string(),
                            is_error: false,
                        });
                    }
                }
                None => {
                    let err_msg = "FileSaveDialogCompleted received but no pending action was set."
                        .to_string();
                    eprintln!("AppLogic: {}", err_msg);
                    self.pending_archive_content = None;
                    status_update_cmd = Some(PlatformCommand::UpdateStatusBarText {
                        window_id,
                        text: err_msg,
                        is_error: true,
                    });
                }
            }
        }
        if let Some(cmd) = status_update_cmd {
            commands.push(cmd);
        }
        commands
    }

    fn handle_window_resized(
        &mut self,
        _window_id: WindowId,
        _width: i32,
        _height: i32,
    ) -> Vec<PlatformCommand> {
        Vec::new()
    }
}

impl PlatformEventHandler for MyAppLogic {
    fn handle_event(&mut self, event: AppEvent) -> Vec<PlatformCommand> {
        match event {
            AppEvent::WindowCloseRequested { window_id } => {
                self.handle_window_close_requested(window_id)
            }
            AppEvent::WindowDestroyed { window_id } => self.handle_window_destroyed(window_id),
            AppEvent::TreeViewItemToggled {
                window_id,
                item_id,
                new_state,
            } => self.handle_treeview_item_toggled(window_id, item_id, new_state),
            AppEvent::ButtonClicked {
                window_id,
                control_id,
            } => self.handle_button_clicked(window_id, control_id),
            AppEvent::MenuLoadProfileClicked => self.handle_menu_load_profile_clicked(),
            AppEvent::FileOpenDialogCompleted { window_id, result } => {
                self.handle_file_open_dialog_completed(window_id, result)
            }
            AppEvent::MenuSaveProfileAsClicked => self.handle_menu_save_profile_as_clicked(),
            AppEvent::FileSaveDialogCompleted { window_id, result } => {
                self.handle_file_save_dialog_completed(window_id, result)
            }
            AppEvent::WindowResized {
                window_id,
                width,
                height,
            } => self.handle_window_resized(window_id, width, height),
        }
    }

    fn on_quit(&mut self) {
        println!("AppLogic: on_quit called by platform. Application is exiting.");

        let temp_name: Option<&str>;
        if let Some(name_ref) = &self.current_profile_name {
            temp_name = Some(name_ref.as_str());
            println!("Debug: Matched Some: {}", temp_name.unwrap());
        } else {
            temp_name = Some(""); // Using Some("") to avoid direct use of "" in the next step
            println!("Debug: Matched None, using empty string placeholder");
        }

        let profile_name_to_save = temp_name.unwrap_or(""); // Safely unwrap or provide default
        println!("Debug: profile_name_to_save is '{}'", profile_name_to_save);

        match self
            .config_manager
            .save_last_profile_name(APP_NAME_FOR_PROFILES, profile_name_to_save)
        {
            Ok(_) => {
                if profile_name_to_save.is_empty() {
                    println!("AppLogic: Successfully cleared last profile name in config on exit.");
                } else {
                    println!(
                        "AppLogic: Successfully saved last active profile name '{}' to config on exit.",
                        profile_name_to_save
                    );
                }
            }
            Err(e) => {
                // Since we are quitting, there's not much to do other than log.
                // The user won't see UI updates here.
                eprintln!(
                    "AppLogic: Error saving last profile name to config on exit: {:?}",
                    e
                );
            }
        }
    }
}

#[cfg(test)]
impl MyAppLogic {
    pub(crate) fn test_main_window_id(&self) -> Option<WindowId> {
        self.main_window_id
    }
    pub(crate) fn test_set_main_window_id(&mut self, v: Option<WindowId>) {
        self.main_window_id = v;
    }

    pub(crate) fn test_file_nodes_cache(&mut self) -> &mut Vec<FileNode> {
        &mut self.file_nodes_cache
    }
    pub(crate) fn test_set_file_nodes_cache(&mut self, v: Vec<FileNode>) {
        self.file_nodes_cache = v;
    }
    pub(crate) fn test_find_filenode_mut(&mut self, path_to_find: &Path) -> Option<&mut FileNode> {
        return Self::find_filenode_mut(&mut self.file_nodes_cache, path_to_find);
    }

    pub(crate) fn test_path_to_tree_item_id(&self) -> &PathToTreeItemIdMap {
        &self.path_to_tree_item_id
    }
    pub(crate) fn test_set_path_to_tree_item_id(&mut self, v: PathToTreeItemIdMap) {
        self.path_to_tree_item_id = v;
    }
    pub(crate) fn test_path_to_tree_item_id_clear(&mut self) {
        self.next_tree_item_id_counter = 1;
        self.path_to_tree_item_id.clear();
    }
    pub(crate) fn test_path_to_tree_item_id_insert(&mut self, path: &PathBuf, id: TreeItemId) {
        self.path_to_tree_item_id.insert(path.to_path_buf(), id);
    }

    pub(crate) fn test_next_tree_item_id_counter(&self) -> u64 {
        self.next_tree_item_id_counter
    }
    pub(crate) fn test_set_next_tree_item_id_counter(&mut self, v: u64) {
        self.next_tree_item_id_counter = v;
    }

    pub(crate) fn test_root_path_for_scan(&self) -> &PathBuf {
        &self.root_path_for_scan
    }
    pub(crate) fn test_set_root_path_for_scan(&mut self, v: PathBuf) {
        self.root_path_for_scan = v;
    }
    pub(crate) fn test_root_path_for_scan_set(&mut self, v: &Path) {
        self.root_path_for_scan = v.to_path_buf();
    }

    pub(crate) fn test_current_profile_name(&self) -> &Option<String> {
        &self.current_profile_name
    }
    pub(crate) fn test_set_current_profile_name(&mut self, v: Option<String>) {
        self.current_profile_name = v;
    }
    pub(crate) fn test_current_set(
        &mut self,
        name: Option<String>,
        cache: Option<Profile>,
        status: Option<ArchiveStatus>,
    ) {
        self.current_profile_name = name;
        self.current_profile_cache = cache;
        self.current_archive_status = status;
    }

    pub(crate) fn test_current_profile_cache(&self) -> &Option<Profile> {
        &self.current_profile_cache
    }
    pub(crate) fn test_set_current_profile_cache(&mut self, v: Option<Profile>) {
        self.current_profile_cache = v;
    }

    pub(crate) fn test_current_archive_status(&self) -> &Option<ArchiveStatus> {
        &self.current_archive_status
    }
    pub(crate) fn test_set_current_archive_status(&mut self, v: Option<ArchiveStatus>) {
        self.current_archive_status = v;
    }

    pub(crate) fn test_pending_archive_content(&self) -> &Option<String> {
        &self.pending_archive_content
    }
    pub(crate) fn test_set_pending_archive_content(&mut self, v: String) {
        self.pending_archive_content = Some(v);
    }

    pub(crate) fn test_pending_action(&self) -> &Option<PendingAction> {
        &self.pending_action
    }
    pub(crate) fn test_set_pending_action(&mut self, v: PendingAction) {
        self.pending_action = Some(v);
    }

    pub(crate) fn test_config_manager(&self) -> &Arc<dyn ConfigManagerOperations> {
        &self.config_manager
    }
    pub(crate) fn test_set_config_manager(&mut self, v: Arc<dyn ConfigManagerOperations>) {
        self.config_manager = v;
    }

    pub(crate) fn test_profile_manager(&self) -> &Arc<dyn ProfileManagerOperations> {
        &self.profile_manager
    }
    pub(crate) fn test_set_profile_manager(&mut self, v: Arc<dyn ProfileManagerOperations>) {
        self.profile_manager = v;
    }

    pub(crate) fn test_file_system_scanner(&self) -> &Arc<dyn FileSystemScannerOperations> {
        &self.file_system_scanner
    }
    pub(crate) fn test_set_file_system_scanner(&mut self, v: Arc<dyn FileSystemScannerOperations>) {
        self.file_system_scanner = v;
    }

    pub(crate) fn test_archiver(&self) -> &Arc<dyn ArchiverOperations> {
        &self.archiver
    }
    pub(crate) fn test_set_archiver(&mut self, v: Arc<dyn ArchiverOperations>) {
        self.archiver = v;
    }

    pub(crate) fn test_state_manager(&self) -> &Arc<dyn StateManagerOperations> {
        &self.state_manager
    }
    pub(crate) fn test_set_state_manager(&mut self, v: Arc<dyn StateManagerOperations>) {
        self.state_manager = v;
    }
}
