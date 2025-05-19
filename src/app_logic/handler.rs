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
 * to update the UI. It depends on a `ConfigManagerOperations` trait for handling
 * application configuration, such as loading the last used profile.
 */
pub struct MyAppLogic {
    main_window_id: Option<WindowId>,
    file_nodes_cache: Vec<FileNode>, // Represents the full scanned tree for the current root_path
    path_to_tree_item_id: PathToTreeItemIdMap,
    next_tree_item_id_counter: u64,
    root_path_for_scan: PathBuf,
    current_profile_name: Option<String>,
    current_profile_cache: Option<Profile>, // Cache of the currently loaded profile
    current_archive_status: Option<ArchiveStatus>,
    pending_archive_content: Option<String>,
    pending_action: Option<PendingAction>,
    config_manager: Arc<dyn ConfigManagerOperations>,
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
            root_path_for_scan: PathBuf::from("."), // Default, might be overridden by last profile
            current_profile_name: None,
            current_profile_cache: None,
            current_archive_status: None,
            pending_archive_content: None,
            pending_action: None,
            config_manager,
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
                        self.current_profile_cache = Some(profile); // Cache it
                        loaded_profile_on_startup = true;
                    }
                    Err(e) => {
                        eprintln!(
                            "AppLogic: Failed to load last profile '{}': {:?}. Proceeding with default.",
                            last_profile_name, e
                        );
                        // Reset possibly inconsistent state if load_profile failed mid-way
                        self.current_profile_name = None;
                        self.current_profile_cache = None;
                        // self.root_path_for_scan remains default PathBuf::from(".")
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

                // If a profile was loaded on startup, apply its state
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

        // P2.6: Update archive status after initial load/scan
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
            // TODO P2.8: Send command to update status bar UI.
        } else {
            self.current_archive_status = None;
            println!("AppLogic: No profile loaded, archive status cleared.");
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
                            // TODO: Show error to user via PlatformCommand
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

                                        // P2.6: Save last loaded profile name
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
                                        let mut new_profile = self
                                            .create_profile_from_current_state(
                                                profile_name_str.clone(),
                                            );
                                        new_profile.name = profile_name_str;

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
                                                    Some(new_profile.clone()); // clone new_profile here
                                                self.root_path_for_scan = self
                                                    .current_profile_cache
                                                    .as_ref()
                                                    .unwrap()
                                                    .root_folder
                                                    .clone();

                                                // P2.6: Save last saved profile name
                                                if let Err(e) = self
                                                    .config_manager // Use self.config_manager
                                                    .save_last_profile_name(
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

#[cfg(test)]
mod handler_tests {
    use super::*;
    use crate::core::{ConfigError, CoreConfigManager, ProfileError};
    use std::fs::{self, File};
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use tempfile::{NamedTempFile, tempdir};

    // --- MockConfigManager for testing ---
    struct MockConfigManager {
        load_last_profile_name_result: Mutex<Result<Option<String>, ConfigError>>,
        saved_profile_name: Mutex<Option<(String, String)>>, // (app_name, profile_name)
    }

    impl MockConfigManager {
        fn new() -> Self {
            MockConfigManager {
                load_last_profile_name_result: Mutex::new(Ok(None)), // Default: no profile
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
            // Clone the result to return it.
            // The error type ConfigError is not Clone, so we need to handle it.
            // For simplicity in the mock, if it's an error, we'll return a generic Io error.
            // A more sophisticated mock might store the exact error to return.
            match *self.load_last_profile_name_result.lock().unwrap() {
                Ok(ref opt_str) => Ok(opt_str.clone()),
                Err(ConfigError::Io(ref io_err)) => {
                    // Attempt to recreate a similar IO error. This is tricky.
                    // For mock purposes, a new error of the same kind might suffice.
                    Err(ConfigError::Io(io::Error::new(
                        io_err.kind(),
                        "mocked io error",
                    )))
                }
                Err(ConfigError::NoProjectDirectory) => Err(ConfigError::NoProjectDirectory),
                Err(ConfigError::Utf8Error(ref utf8_err)) => {
                    // Recreate FromUtf8Error (it stores the original Vec<u8> and a Utf8Error)
                    // This is complex to truly clone. For a mock, we might simplify.
                    // For now, let's just return a generic Utf8Error representation.
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

    // Helper to create MyAppLogic with a mock config manager and window id
    fn setup_logic_with_mock_config_manager() -> (MyAppLogic, Arc<MockConfigManager>) {
        let mock_config_manager = Arc::new(MockConfigManager::new());
        let mut logic =
            MyAppLogic::new(Arc::clone(&mock_config_manager) as Arc<dyn ConfigManagerOperations>);
        logic.main_window_id = Some(WindowId(1));
        (logic, mock_config_manager)
    }

    // Helper to create a temporary profile file for loading tests
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

    #[test]
    fn test_on_main_window_created_loads_last_profile_with_mock() {
        let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
        let temp_base_dir = tempdir().unwrap(); // For profile JSON file

        let last_profile_name_to_load = "MyMockedStartupProfile";
        let startup_profile_root = temp_base_dir.path().join("mock_startup_root");
        fs::create_dir_all(&startup_profile_root).unwrap();
        File::create(startup_profile_root.join("mock_startup_file.txt"))
            .expect("Test setup: Failed to create mock_startup_file.txt");

        // 1. Configure the MockConfigManager to return the desired profile name
        mock_config_manager
            .set_load_last_profile_name_result(Ok(Some(last_profile_name_to_load.to_string())));

        // 2. Create the actual profile JSON file that core::load_profile will read
        //    (MyAppLogic still uses core::load_profile directly)
        let _profile_json_path = create_temp_profile_file_in_profile_subdir(
            &temp_base_dir, // This dir is for the actual profile JSON
            APP_NAME_FOR_PROFILES,
            last_profile_name_to_load,
            &startup_profile_root,
            None,
        );

        let _cmds = logic.on_main_window_created(WindowId(1)); // WindowId is already set by helper

        assert_eq!(
            logic.current_profile_name.as_deref(),
            Some(last_profile_name_to_load)
        );
        assert!(logic.current_profile_cache.is_some());
        assert_eq!(
            logic.current_profile_cache.as_ref().unwrap().name,
            last_profile_name_to_load
        );
        assert_eq!(logic.root_path_for_scan, startup_profile_root);
        assert_eq!(logic.file_nodes_cache.len(), 1);
        assert_eq!(logic.file_nodes_cache[0].name, "mock_startup_file.txt");
        assert!(logic.current_archive_status.is_some());
    }

    #[test]
    fn test_on_main_window_created_no_last_profile_with_mock() {
        let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
        // MockConfigManager defaults to Ok(None) for load_last_profile_name

        let default_scan_path = PathBuf::from(".");
        let dummy_file_path = default_scan_path.join("default_mock_scan_file.txt");
        File::create(&dummy_file_path)
            .expect("Test setup: Failed to create default_mock_scan_file.txt");

        let _cmds = logic.on_main_window_created(WindowId(1));

        assert!(logic.current_profile_name.is_none());
        assert!(logic.current_profile_cache.is_none());
        assert_eq!(logic.root_path_for_scan, default_scan_path);

        let found_dummy_file = logic
            .file_nodes_cache
            .iter()
            .any(|n| n.path == dummy_file_path);
        assert!(
            found_dummy_file,
            "Default scan should have found default_mock_scan_file.txt. Cache: {:?}",
            logic
                .file_nodes_cache
                .iter()
                .map(|n| &n.path)
                .collect::<Vec<_>>()
        );
        assert!(logic.current_archive_status.is_none());
        fs::remove_file(dummy_file_path)
            .expect("Test cleanup: Failed to remove default_mock_scan_file.txt");
    }

    #[test]
    fn test_file_open_dialog_completed_saves_last_profile_name_with_mock() {
        let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
        let temp_profile_dir = tempdir().unwrap(); // For profile JSON

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
        let _cmds = logic.handle_event(event);

        assert_eq!(
            logic.current_profile_name.as_deref(),
            Some(profile_to_load_name)
        );
        let saved_name_info = mock_config_manager.get_saved_profile_name();
        assert!(saved_name_info.is_some());
        assert_eq!(saved_name_info.unwrap().0, APP_NAME_FOR_PROFILES);
        assert_eq!(
            logic.current_profile_name.as_deref(),
            saved_name_info.unwrap().1.as_str().into()
        );
    }

    #[test]
    fn test_file_save_dialog_completed_for_profile_saves_last_profile_name_with_mock() {
        let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
        let temp_scan_dir = tempdir().unwrap();
        logic.root_path_for_scan = temp_scan_dir.path().to_path_buf();

        let temp_base_app_data_dir = tempdir().unwrap(); // For the actual profile.json save
        let profile_to_save_name = "MyNewlySavedProfileMocked";

        // core::save_profile will use ProjectDirs, so we need a real-like path structure
        // if we don't mock core::profiles::save_profile itself (which we aren't yet).
        let mock_profile_storage_dir = temp_base_app_data_dir
            .path()
            .join(APP_NAME_FOR_PROFILES)
            .join("profiles");
        fs::create_dir_all(&mock_profile_storage_dir).unwrap();
        let profile_save_path_from_dialog = mock_profile_storage_dir.join(format!(
            "{}.json",
            core::profiles::sanitize_profile_name(profile_to_save_name)
        ));

        logic.pending_action = Some(PendingAction::SavingProfile);
        let event = AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: Some(profile_save_path_from_dialog.clone()),
        };

        let _cmds = logic.handle_event(event);

        assert_eq!(
            logic.current_profile_name.as_deref(),
            Some(profile_to_save_name)
        );
        assert!(logic.current_profile_cache.is_some());
        assert_eq!(
            logic.current_profile_cache.as_ref().unwrap().name,
            profile_to_save_name
        );
        assert!(profile_save_path_from_dialog.exists());

        let saved_name_info = mock_config_manager.get_saved_profile_name();
        assert!(saved_name_info.is_some());
        assert_eq!(saved_name_info.unwrap().0, APP_NAME_FOR_PROFILES);
        assert_eq!(
            logic.current_profile_name.as_deref(),
            saved_name_info.unwrap().1.as_str().into()
        );
    }

    // ... (other existing tests can remain, they don't interact with config manager as much)
    // Minimal setup logic helper, primarily for tests not focused on config loading
    fn setup_logic_with_window() -> MyAppLogic {
        let dummy_config_manager = Arc::new(CoreConfigManager::new()); // Or a simple mock
        let mut logic = MyAppLogic::new(dummy_config_manager);
        logic.main_window_id = Some(WindowId(1));
        logic
    }

    #[test]
    fn test_handle_button_click_generates_save_dialog_archive() {
        let mut logic = setup_logic_with_window();
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
        let mut logic = setup_logic_with_window();
        let temp_root = tempdir().unwrap();
        let profile_name = "MyTestProfile".to_string();
        let archive_file = temp_root.path().join("my_archive.txt");

        logic.current_profile_cache = Some(Profile {
            name: profile_name.clone(),
            root_folder: temp_root.path().to_path_buf(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: Some(archive_file.clone()),
        });
        logic.root_path_for_scan = temp_root.path().to_path_buf();

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
                assert_eq!(initial_dir.as_deref(), archive_file.parent());
            }
            _ => panic!("Expected ShowSaveFileDialog with profile context"),
        }
    }

    #[test]
    fn test_handle_file_save_dialog_completed_for_archive_with_path() {
        let mut logic = setup_logic_with_window();
        logic.pending_action = Some(PendingAction::SavingArchive);
        logic.pending_archive_content = Some("ARCHIVE CONTENT".to_string());

        let tmp_file = NamedTempFile::new().unwrap();
        let archive_save_path = tmp_file.path().to_path_buf();
        let temp_root_for_profile = tempdir().unwrap();
        logic.current_profile_cache = Some(Profile::new(
            "test_profile_for_archive_save".into(),
            temp_root_for_profile.path().to_path_buf(),
        ));
        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: Some(archive_save_path.clone()),
        });

        assert!(
            cmds.is_empty(),
            "No follow-up UI commands expected directly from save completion"
        );
        assert_eq!(
            logic.pending_archive_content, None,
            "Pending content should be cleared"
        );
        let written_content = fs::read_to_string(&archive_save_path).unwrap();
        assert_eq!(written_content, "ARCHIVE CONTENT");
        assert_eq!(
            logic
                .current_profile_cache
                .as_ref()
                .unwrap()
                .archive_path
                .as_ref()
                .unwrap(),
            &archive_save_path
        );
        assert_eq!(
            logic.current_archive_status,
            Some(ArchiveStatus::NoFilesSelected),
            "Archive status should be NoFilesSelected when no files are in cache/selected"
        );
    }

    #[test]
    fn test_handle_file_save_dialog_completed_for_profile_with_path() {
        // This test is complex because it involves `core::save_profile` which uses `ProjectDirs`
        // and `self.config_manager.save_last_profile_name` which is now mocked or real.
        // We'll use the setup_logic_with_mock_config_manager for the latter part.
        let (mut logic, mock_config_manager) = setup_logic_with_mock_config_manager();
        logic.pending_action = Some(PendingAction::SavingProfile);
        let temp_scan_dir = tempdir().unwrap();
        logic.root_path_for_scan = temp_scan_dir.path().to_path_buf();

        // For core::save_profile, it uses real ProjectDirs.
        // We need a unique app name for this test to avoid interference.
        let unique_app_name_for_profile_save =
            format!("TestApp_SaveProfile_{}", rand::random::<u64>());
        let temp_profiles_storage_dir = tempdir().unwrap(); // This isn't directly used by core::save_profile
        // if ProjectDirs provides a different path.

        let profile_name_from_dialog = "MySavedProfileViaDialog";

        // Determine where core::save_profile WILL save it based on unique_app_name
        let expected_profile_dir =
            core::profiles::get_profile_dir(&unique_app_name_for_profile_save)
                .expect("Should get a profile dir for unique app name");
        fs::create_dir_all(&expected_profile_dir).unwrap(); // Ensure it exists

        let sanitized_name = core::profiles::sanitize_profile_name(profile_name_from_dialog);
        let profile_save_path_from_dialog =
            expected_profile_dir.join(format!("{}.json", sanitized_name));

        // We need to adjust APP_NAME_FOR_PROFILES for the duration of this specific test
        // for core::save_profile. This is tricky.
        // Alternatively, this test assumes APP_NAME_FOR_PROFILES leads to a writable temp location.
        // For this iteration, let's assume APP_NAME_FOR_PROFILES is fine and `core::save_profile`
        // will write to its designated (possibly temp if tests are set up that way) location.
        // The mock config manager is for `self.config_manager.save_last_profile_name`.

        let _cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: Some(profile_save_path_from_dialog.clone()),
        });

        assert_eq!(
            logic.current_profile_name.as_deref(),
            Some(profile_name_from_dialog)
        );
        assert!(logic.current_profile_cache.is_some());
        assert_eq!(
            logic.current_profile_cache.as_ref().unwrap().name,
            profile_name_from_dialog
        );
        assert_eq!(
            logic.current_profile_cache.as_ref().unwrap().root_folder,
            temp_scan_dir.path()
        );
        assert!(logic.current_archive_status.is_some());
        assert!(
            profile_save_path_from_dialog.exists(),
            "Profile JSON file should have been saved by core::save_profile to {:?}",
            profile_save_path_from_dialog
        );

        let saved_config = mock_config_manager.get_saved_profile_name().unwrap();
        assert_eq!(saved_config.0, APP_NAME_FOR_PROFILES); // Check app_name used for config
        assert_eq!(saved_config.1, profile_name_from_dialog); // Check profile_name saved

        // Cleanup the uniquely named profile directory if it was created by core::save_profile
        // This depends on core::save_profile actually using APP_NAME_FOR_PROFILES.
        // If it used unique_app_name_for_profile_save, that's what needs cleanup.
        // The current core::save_profile takes app_name as argument, so it used APP_NAME_FOR_PROFILES.
        if profile_save_path_from_dialog.exists() {
            fs::remove_file(&profile_save_path_from_dialog).unwrap();
        }
        // Attempt to remove parent dirs if they are empty, carefully.
        if expected_profile_dir.exists()
            && fs::read_dir(&expected_profile_dir).map_or(false, |mut d| d.next().is_none())
        {
            fs::remove_dir(&expected_profile_dir).ok();
            if let Some(p) = expected_profile_dir.parent() {
                if p.exists() && fs::read_dir(p).map_or(false, |mut d| d.next().is_none()) {
                    fs::remove_dir(p).ok();
                }
            }
        }
    }

    #[test]
    fn test_handle_file_save_dialog_cancelled_for_archive() {
        let mut logic = setup_logic_with_window();
        logic.pending_action = Some(PendingAction::SavingArchive);
        logic.pending_archive_content = Some("WILL BE CLEARED".to_string());

        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: None,
        });

        assert!(cmds.is_empty());
        assert_eq!(
            logic.pending_archive_content, None,
            "Pending content should be cleared on cancel"
        );
        assert!(
            logic.pending_action.is_none(),
            "Pending action should be cleared"
        );
    }

    #[test]
    fn test_handle_file_save_dialog_cancelled_for_profile() {
        let mut logic = setup_logic_with_window();
        logic.pending_action = Some(PendingAction::SavingProfile);

        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: None,
        });
        assert!(cmds.is_empty());
        assert!(
            logic.pending_action.is_none(),
            "Pending action should be cleared on cancel"
        );
    }

    #[test]
    fn test_handle_treeview_item_toggled_updates_model_visuals_and_archive_status() {
        let mut logic = setup_logic_with_window();
        let temp_scan_dir = tempdir().unwrap();
        logic.root_path_for_scan = temp_scan_dir.path().to_path_buf();
        let archive_file_path = temp_scan_dir.path().join("archive.txt");
        File::create(&archive_file_path)
            .unwrap()
            .write_all(b"old archive content")
            .unwrap();
        thread::sleep(Duration::from_millis(50));
        let foo_path = logic.root_path_for_scan.join("foo.txt");
        File::create(&foo_path)
            .unwrap()
            .write_all(b"foo content - will be selected")
            .unwrap();
        logic.file_nodes_cache = vec![FileNode::new(foo_path.clone(), "foo.txt".into(), false)];
        logic.current_profile_cache = Some(Profile {
            name: "test_profile_for_toggle".into(),
            root_folder: logic.root_path_for_scan.clone(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: Some(archive_file_path.clone()),
        });
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();
        let _descriptors = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );
        let tree_item_id_for_foo = *logic.path_to_tree_item_id.get(&foo_path).unwrap();
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
            logic.file_nodes_cache[0].state,
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
            logic.current_archive_status,
            Some(ArchiveStatus::OutdatedRequiresUpdate),
            "Archive status incorrect after toggle. Expected Outdated. Foo_ts: {:?}, Archive_ts: {:?}",
            foo_ts,
            archive_ts
        );
    }

    #[test]
    fn test_handle_window_close_requested_generates_close_command() {
        let mut logic = setup_logic_with_window();
        let cmds = logic.handle_event(AppEvent::WindowCloseRequested {
            window_id: WindowId(1),
        });
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], PlatformCommand::CloseWindow { .. }));
    }

    #[test]
    fn test_handle_window_destroyed_clears_main_window_id_and_state() {
        let mut logic = setup_logic_with_window();
        logic.current_profile_name = Some("Test".to_string());
        logic.current_profile_cache = Some(Profile::new("Test".into(), PathBuf::from(".")));
        logic.current_archive_status = Some(ArchiveStatus::UpToDate);
        logic
            .file_nodes_cache
            .push(FileNode::new(PathBuf::from("./file"), "file".into(), false));
        logic
            .path_to_tree_item_id
            .insert(PathBuf::from("./file"), TreeItemId(1));

        let cmds = logic.handle_event(AppEvent::WindowDestroyed {
            window_id: WindowId(1),
        });

        assert!(cmds.is_empty());
        assert_eq!(logic.main_window_id, None);
        assert!(logic.current_profile_name.is_none());
        assert!(logic.current_profile_cache.is_none());
        assert!(logic.current_archive_status.is_none());
        assert!(logic.file_nodes_cache.is_empty());
        assert!(logic.path_to_tree_item_id.is_empty());
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
        let mut logic = setup_logic_with_window(); // Uses default CoreConfigManager
        logic.file_nodes_cache = make_test_tree_for_applogic();
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();
        let descriptors = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );
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
        assert_eq!(logic.path_to_tree_item_id.len(), 3);
        assert!(
            logic
                .path_to_tree_item_id
                .contains_key(&PathBuf::from("/root/file1.txt"))
        );
        assert!(
            logic
                .path_to_tree_item_id
                .contains_key(&PathBuf::from("/root/sub"))
        );
        assert!(
            logic
                .path_to_tree_item_id
                .contains_key(&PathBuf::from("/root/sub/file2.txt"))
        );
    }

    #[test]
    fn test_find_filenode_mut_and_ref_applogic() {
        let mut logic = setup_logic_with_window();
        logic.file_nodes_cache = make_test_tree_for_applogic();
        let file1_p = PathBuf::from("/root/file1.txt");
        let file2_p = PathBuf::from("/root/sub/file2.txt");
        let file1_node_mut = MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p);
        assert!(file1_node_mut.is_some());
        file1_node_mut.unwrap().state = FileState::Selected;
        let file1_node_ref = MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &file1_p);
        assert!(file1_node_ref.is_some());
        assert_eq!(file1_node_ref.unwrap().state, FileState::Selected);
        let file2_node_ref = MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &file2_p);
        assert!(file2_node_ref.is_some());
        assert_eq!(file2_node_ref.unwrap().name, "file2.txt");
        let none_node =
            MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &PathBuf::from("/no/such/path"));
        assert!(none_node.is_none());
    }

    #[test]
    fn test_collect_visual_updates_recursive_applogic() {
        let mut logic = setup_logic_with_window();
        logic.file_nodes_cache = make_test_tree_for_applogic();
        let file1_p = PathBuf::from("/root/file1.txt");
        let sub_p = PathBuf::from("/root/sub");
        let file2_p = PathBuf::from("/root/sub/file2.txt");
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();
        let _ = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );
        {
            let f1_mut =
                MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p).unwrap();
            f1_mut.state = FileState::Selected;
        }
        let sub_node_for_update_path = PathBuf::from("/root/sub");
        {
            let file2_mut =
                MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file2_p).unwrap();
            file2_mut.state = FileState::Selected;
            let sub_node_mut = MyAppLogic::find_filenode_mut(
                &mut logic.file_nodes_cache,
                &sub_node_for_update_path,
            )
            .unwrap();
            sub_node_mut.state = FileState::Unknown;
        }
        let mut updates = Vec::new();
        let sub_node_ref = logic
            .file_nodes_cache
            .iter()
            .find(|n| n.path == sub_node_for_update_path)
            .unwrap();
        logic.collect_visual_updates_recursive(sub_node_ref, &mut updates);
        assert_eq!(
            updates.len(),
            2,
            "Expected updates for 'sub' and 'file2.txt'"
        );
        let sub_item_id = *logic.path_to_tree_item_id.get(&sub_p).unwrap();
        assert!(
            updates
                .iter()
                .any(|(id, state)| *id == sub_item_id && *state == CheckState::Unchecked)
        );
        let file2_item_id = *logic.path_to_tree_item_id.get(&file2_p).unwrap();
        assert!(
            updates
                .iter()
                .any(|(id, state)| *id == file2_item_id && *state == CheckState::Checked)
        );
    }

    #[test]
    fn test_profile_load_updates_archive_status() {
        let mut logic = setup_logic_with_window();
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
        let _cmds = logic.handle_event(event);
        assert_eq!(
            logic.current_profile_name.as_deref(),
            Some(profile_name),
            "Profile name mismatch after load"
        );
        assert!(
            logic.current_profile_cache.is_some(),
            "Profile cache should be populated"
        );
        assert_eq!(
            logic.current_profile_cache.as_ref().unwrap().name,
            profile_name,
            "Name in cached profile mismatch"
        );
        assert_eq!(
            logic
                .current_profile_cache
                .as_ref()
                .unwrap()
                .archive_path
                .as_ref()
                .unwrap(),
            &archive_file_for_profile,
            "Archive path in cached profile mismatch"
        );
        assert_eq!(
            logic.current_archive_status,
            Some(ArchiveStatus::NoFilesSelected),
            "Archive status after load is incorrect"
        );
    }
}
