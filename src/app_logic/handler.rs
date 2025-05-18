use crate::core::{
    self,          // Keep core re-export for brevity
    ArchiveStatus, // <-- Add ArchiveStatus
    FileNode,
    FileState,
    Profile,
};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
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
 * to update the UI, managing file trees, profiles, archive generation, and archive status.
 */
pub struct MyAppLogic {
    main_window_id: Option<WindowId>,
    file_nodes_cache: Vec<FileNode>, // Represents the full scanned tree for the current root_path
    path_to_tree_item_id: PathToTreeItemIdMap,
    next_tree_item_id_counter: u64,
    root_path_for_scan: PathBuf,
    current_profile_name: Option<String>,
    current_profile_cache: Option<Profile>, // Cache of the currently loaded profile
    current_archive_status: Option<ArchiveStatus>, // <-- Add current_archive_status
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
            current_profile_cache: None,  // <-- Initialize
            current_archive_status: None, // <-- Initialize
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
                    _ => CheckState::Unchecked, // Unknown and Deselected are Unchecked in UI
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
                // Create a dummy error node to display in the tree view
                let error_node_path = PathBuf::from("/error_node_scan_failed");
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
        } else if self.file_nodes_cache.is_empty() {
            // If scan was successful but returned no nodes (e.g. empty dir)
            commands.push(PlatformCommand::PopulateTreeView {
                window_id,
                items: vec![], // Send empty vec to clear treeview
            });
        }
        // If descriptors is empty but cache is not (e.g. error node was created but recursive build failed, unlikely)
        // it would mean the tree is not populated, which might be okay if an error is shown elsewhere.

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
                FileState::Unknown => {} // Unknown states are not persisted in the profile's sets
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
     * This includes the current root scan path, sets of selected and deselected file paths derived
     * from `file_nodes_cache`, and the provided new profile name. The `archive_path` is taken
     * from the `current_profile_cache` if available.
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
        // Always send PopulateTreeView, even with empty items, to clear the view.
        Some(PlatformCommand::PopulateTreeView {
            window_id,
            items: descriptors,
        })
    }

    /*
     * Updates the internal archive status by calling `core::check_archive_status`.
     * This function should be called after loading a profile, generating an archive,
     * or when file selections change that might affect archive state.
     * It prints the new status for now; future steps will update UI.
     */
    fn update_current_archive_status(&mut self) {
        if let Some(profile) = &self.current_profile_cache {
            let status = core::check_archive_status(profile, &self.file_nodes_cache);
            self.current_archive_status = Some(status);
            println!("AppLogic: Archive status updated to: {:?}", status);
            // TODO P2.8: Send command to update status bar UI.
        } else {
            self.current_archive_status = None; // No profile loaded, so no status.
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
                _ => CheckState::Unchecked, // Unknown and Deselected map to Unchecked
            };
            updates.push((*item_id, check_state));

            if node.is_dir {
                for child in &node.children {
                    self.collect_visual_updates_recursive(child, updates);
                }
            }
        } else {
            // This can happen if the tree was refreshed and some nodes disappeared
            // before the UI fully caught up, or if there's a mismatch.
            // It's generally not critical if it happens transiently.
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
                    // Potentially clear other state if needed when main window is gone
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
                    // Scope for mutable borrow of self.file_nodes_cache
                    {
                        let node_to_update_model_for = Self::find_filenode_mut(
                            &mut self.file_nodes_cache,
                            &path_for_model_update,
                        );

                        if let Some(node_model) = node_to_update_model_for {
                            let new_model_file_state = match new_state {
                                CheckState::Checked => FileState::Selected,
                                CheckState::Unchecked => FileState::Deselected,
                                // CheckState::Indeterminate is not used by TreeView toggle event
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
                    } // End scope for mutable borrow of self.file_nodes_cache

                    // Re-borrow self.file_nodes_cache immutably for collecting visual updates
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
                    self.update_current_archive_status(); // Selection changed, update status
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
                    // Determine the root path for display in archive headers.
                    // This should be the root_folder of the current profile, if a profile is loaded.
                    // Otherwise, it falls back to self.root_path_for_scan.
                    let display_root_path = self.current_profile_cache.as_ref().map_or_else(
                        || self.root_path_for_scan.clone(),
                        |p| p.root_folder.clone(),
                    );

                    match core::create_archive_content(
                        &self.file_nodes_cache, // Use the current full tree
                        &display_root_path,     // Pass the determined root for relative paths
                    ) {
                        Ok(content) => {
                            self.pending_archive_content = Some(content);
                            self.pending_action = Some(PendingAction::SavingArchive);

                            // Default filename construction:
                            // Use profile name if available, otherwise "archive.txt"
                            let default_filename = self
                                .current_profile_cache
                                .as_ref()
                                .map(|p| core::profiles::sanitize_profile_name(&p.name) + ".txt")
                                .unwrap_or_else(|| "archive.txt".to_string());

                            // Initial directory for save dialog:
                            // Use profile's archive path's parent dir, or profile's root_folder, or current dir.
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
                        initial_dir: profile_dir_res, // Option<PathBuf>
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

                        // Attempt to load the profile directly from the given path
                        match File::open(&profile_file_path) {
                            Ok(file) => {
                                let reader = std::io::BufReader::new(file);
                                match serde_json::from_reader(reader) {
                                    Ok(loaded_profile) => {
                                        let profile: Profile = loaded_profile; // Type annotation for clarity
                                        println!(
                                            "AppLogic: Successfully loaded profile '{}' directly from path.",
                                            profile.name
                                        );
                                        self.current_profile_name = Some(profile.name.clone());
                                        self.root_path_for_scan = profile.root_folder.clone();
                                        self.current_profile_cache = Some(profile.clone()); // Cache it

                                        match core::scan_directory(&self.root_path_for_scan) {
                                            Ok(nodes) => {
                                                self.file_nodes_cache = nodes;
                                                core::apply_profile_to_tree(
                                                    &mut self.file_nodes_cache,
                                                    &profile, // Use the directly loaded profile
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

                    // Use current_profile_name if set, otherwise "new_profile"
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
                        initial_dir: profile_dir_res, // Option<PathBuf>
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
                                            // Update profile's archive_path if a profile is loaded
                                            if let Some(profile) = &mut self.current_profile_cache {
                                                profile.archive_path = Some(path.clone());
                                                // Persist the profile change immediately
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
                                            self.update_current_archive_status(); // Archive changed, update status
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "AppLogic: Failed to write archive to {:?}: {}",
                                                path, e
                                            );
                                            // TODO: Show error to user via PlatformCommand
                                        }
                                    }
                                } else {
                                    eprintln!("AppLogic: SaveArchiveDialog - No pending content.");
                                }
                            } else {
                                println!("AppLogic: Save archive dialog cancelled.");
                                self.pending_archive_content = None; // Clear if cancelled
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
                                        // Ensure the profile being saved uses its *new* name internally
                                        let mut new_profile = self
                                            .create_profile_from_current_state(
                                                profile_name_str.clone(),
                                            );
                                        // The name in the struct should match the filename stem
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
                                                // Update current app state to reflect the newly saved/overwritten profile
                                                self.current_profile_name =
                                                    Some(new_profile.name.clone());
                                                self.current_profile_cache = Some(new_profile);
                                                self.root_path_for_scan = self
                                                    .current_profile_cache
                                                    .as_ref()
                                                    .unwrap()
                                                    .root_folder
                                                    .clone();

                                                // Since this might be a "Save As" of an existing profile with a new name,
                                                // or overwriting, the archive status might need re-check if paths changed.
                                                // Or, it might be a new profile based on current selections.
                                                self.update_current_archive_status();
                                                // TODO: Update status bar with new profile name and archive status
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "AppLogic: Failed to save profile as '{}': {}",
                                                    new_profile.name, e
                                                );
                                                // TODO: Show error to user via PlatformCommand
                                            }
                                        }
                                    } else {
                                        eprintln!(
                                            "AppLogic: Profile save filename stem not valid UTF-8"
                                        );
                                        // TODO: Show error to user
                                    }
                                } else {
                                    eprintln!(
                                        "AppLogic: Could not extract profile name from save path"
                                    );
                                    // TODO: Show error to user
                                }
                            } else {
                                println!("AppLogic: Save profile dialog cancelled.");
                            }
                        }
                        None => {
                            // This can happen if a dialog was shown for a reason not tracked by PendingAction
                            // or if PendingAction was cleared prematurely.
                            eprintln!(
                                "AppLogic: FileSaveDialogCompleted received but no pending action was set."
                            );
                            self.pending_archive_content = None; // Ensure cleanup
                        }
                    }
                }
            }
            AppEvent::WindowResized { .. } => {
                // Currently, no app logic reaction to resize, platform handles control resizing.
                // If app logic needed to react (e.g. change layout density), it would go here.
            }
        }
        commands
    }

    fn on_quit(&mut self) {
        println!("AppLogic: on_quit called by platform. Application is exiting.");
        // Perform any final cleanup if necessary
    }
}

// Unit tests for app_logic::handler
#[cfg(test)]
mod handler_tests {
    use super::*;
    use crate::core::ProfileError;
    use std::fs::{self, File}; // Added File for setup
    use std::io::Write; // For writing to temp files
    use std::thread;
    use std::time::Duration;
    use tempfile::{NamedTempFile, tempdir}; // Added tempdir for profile tests // For testing profile load errors

    // Helper to create a basic MyAppLogic with a main window id for tests
    fn setup_logic_with_window() -> MyAppLogic {
        let mut logic = MyAppLogic::new();
        logic.main_window_id = Some(WindowId(1));
        logic
    }

    // Helper to create a temporary profile file for loading tests
    fn create_temp_profile_file(
        dir: &tempfile::TempDir,
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
        let sanitized_name = core::profiles::sanitize_profile_name(profile_name);
        let profile_file_path = dir.path().join(format!("{}.json", sanitized_name));

        let app_data_dir = dir.path().join(APP_NAME_FOR_PROFILES).join("profiles");
        fs::create_dir_all(&app_data_dir).unwrap();
        let final_path = app_data_dir.join(format!("{}.json", sanitized_name));

        let file = File::create(&final_path).expect("Failed to create temp profile file");
        serde_json::to_writer_pretty(file, &profile).expect("Failed to write temp profile file");
        final_path
    }

    // Simplified helper: creates profile directly in the temp dir's root for clarity in test.
    fn create_temp_profile_file_for_direct_load(
        dir: &tempfile::TempDir, // The base temp directory
        profile_name_stem: &str, // e.g., "ProfileWithArchive"
        root_folder: &Path,
        archive_path: Option<PathBuf>,
    ) -> PathBuf {
        let profile = Profile {
            name: profile_name_stem.to_string(), // The name field in JSON
            root_folder: root_folder.to_path_buf(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path,
        };
        // The actual filename will be profile_name_stem.json
        let profile_file_path = dir.path().join(format!("{}.json", profile_name_stem));

        let file = File::create(&profile_file_path)
            .expect("Failed to create temp profile file for direct load");
        serde_json::to_writer_pretty(file, &profile)
            .expect("Failed to write temp profile file for direct load");
        profile_file_path // Return the direct path to this file
    }

    #[test]
    fn test_handle_button_click_generates_save_dialog_archive() {
        let mut logic = setup_logic_with_window();
        // No file_nodes_cache or root_path_for_scan needed for this specific test path
        // as create_archive_content will just produce empty content if cache is empty.

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
                assert_eq!(default_filename, "archive.txt"); // Default when no profile loaded
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
        logic.root_path_for_scan = temp_root.path().to_path_buf(); // Align scan path

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
                // Corrected assertion:
                assert_eq!(
                    *default_filename, // Dereference default_filename (which is &String)
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

        // Simulate a profile being loaded to test archive_path update
        let temp_root_for_profile = tempdir().unwrap();
        logic.current_profile_cache = Some(Profile::new(
            "test_profile_for_archive_save".into(),
            temp_root_for_profile.path().to_path_buf(),
        ));
        // IMPORTANT: file_nodes_cache is empty for this test.

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

        // Check if profile's archive_path was updated
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

        // Corrected Assertion:
        // With an empty file_nodes_cache (no files selected), the status should be NoFilesSelected.
        assert_eq!(
            logic.current_archive_status,
            Some(ArchiveStatus::NoFilesSelected), // <--- CORRECTED EXPECTATION
            "Archive status should be NoFilesSelected when no files are in cache/selected"
        );
    }

    #[test]
    fn test_handle_file_save_dialog_completed_for_profile_with_path() {
        let mut logic = setup_logic_with_window();
        logic.pending_action = Some(PendingAction::SavingProfile);
        // Mock root_path_for_scan as create_profile_from_current_state uses it
        let temp_scan_dir = tempdir().unwrap();
        logic.root_path_for_scan = temp_scan_dir.path().to_path_buf();

        // Mock where profiles are saved for this test
        let temp_profiles_storage_dir = tempdir().unwrap();
        let app_name_for_test_profiles = "TestAppForProfileSave";
        let mock_actual_profile_dir = temp_profiles_storage_dir
            .path()
            .join(app_name_for_test_profiles)
            .join("profiles");
        fs::create_dir_all(&mock_actual_profile_dir).unwrap();

        // The path from the dialog will determine the profile name
        let profile_name_from_dialog = "MySavedProfile";
        let profile_save_path_from_dialog =
            mock_actual_profile_dir.join(format!("{}.json", profile_name_from_dialog));

        // Override core::save_profile and core::load_profile to use our temp_profiles_storage_dir
        // This is tricky without DI for free functions. For this test, we'll assume
        // core::save_profile correctly uses APP_NAME_FOR_PROFILES and that resolves
        // to a path we can inspect or mock if ProjectDirs was mockable.
        // For now, we'll focus on MyAppLogic's state changes.

        let original_get_profile_dir = core::profiles::get_profile_dir; // Store original
        // This is a simplification. Proper mocking would involve a trait or function pointer.
        // We'll rely on the fact that save_profile uses APP_NAME_FOR_PROFILES.
        // We can't easily redirect ProjectDirs without more complex mocking.
        // So, we'll check MyAppLogic's state update and assume core::save_profile works.

        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: Some(profile_save_path_from_dialog.clone()),
        });

        assert!(
            cmds.is_empty(),
            "No UI commands expected directly from profile save completion"
        );
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

        // Archive status should also be updated (likely NotGenerated or NoFilesSelected for a new profile)
        assert!(logic.current_archive_status.is_some());

        // Cleanup: remove the test profile dir if it was created by ProjectDirs under actual AppData.
        // This is hard to do robustly without knowing the exact path ProjectDirs chose for APP_NAME_FOR_PROFILES.
        // For tests, it's better to mock get_profile_dir if possible.
        // Since we can't easily mock it here, this test has a side effect.
    }

    #[test]
    fn test_handle_file_save_dialog_cancelled_for_archive() {
        let mut logic = setup_logic_with_window();
        logic.pending_action = Some(PendingAction::SavingArchive);
        logic.pending_archive_content = Some("WILL BE CLEARED".to_string());

        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: WindowId(1),
            result: None, // Simulate cancellation
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
            result: None, // Simulate cancellation
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

        // 1. Create the archive file FIRST (making it older)
        let archive_file_path = temp_scan_dir.path().join("archive.txt");
        File::create(&archive_file_path)
            .unwrap()
            .write_all(b"old archive content")
            .unwrap();
        // Ensure its timestamp is set before creating the newer source file.
        // A small delay can help, especially on fast systems/filesystems.
        thread::sleep(Duration::from_millis(50)); // Increased delay

        // 2. Create the source file AFTER the archive (making it newer)
        let foo_path = logic.root_path_for_scan.join("foo.txt");
        File::create(&foo_path)
            .unwrap()
            .write_all(b"foo content - will be selected")
            .unwrap();

        // Initial state: foo.txt is Unselected (default FileNode state)
        logic.file_nodes_cache = vec![FileNode::new(foo_path.clone(), "foo.txt".into(), false)];

        logic.current_profile_cache = Some(Profile {
            name: "test_profile_for_toggle".into(),
            root_folder: logic.root_path_for_scan.clone(),
            selected_paths: HashSet::new(), // Initially no paths are selected in the profile itself
            deselected_paths: HashSet::new(),
            archive_path: Some(archive_file_path.clone()), // Link to the older archive
        });

        // Manually build the ID map as refresh_tree_view_from_cache would do
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();
        let _descriptors = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );
        let tree_item_id_for_foo = *logic.path_to_tree_item_id.get(&foo_path).unwrap();

        // Pre-check: Before toggle, foo.txt is not selected. If archive exists, status might be
        // NoFilesSelected (if logic.file_nodes_cache was empty before this FileNode was added) or
        // UpToDate (if logic.file_nodes_cache had foo.txt but it was Deselected/Unknown and archive is newer).
        // For simplicity, let's assume initial state before toggle doesn't impact the post-toggle check significantly,
        // as the toggle to Selected is the key event.
        // We can call update_current_archive_status to set a baseline if needed, but it's not strictly necessary
        // for this test's focus.

        // 3. Simulate toggling foo.txt to Selected
        let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
            window_id: WindowId(1),
            item_id: tree_item_id_for_foo,
            new_state: CheckState::Checked, // Toggle to Selected
        });

        // --- Assertions ---
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

        // Now, foo.txt is selected and it's newer than archive_file_path.
        // So, the status MUST be OutdatedRequiresUpdate.
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
            Some(ArchiveStatus::OutdatedRequiresUpdate), // <-- This is the key assertion
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

        let mut root_node = FileNode::new(root_p.clone(), "root".into(), true); // This would not be in file_nodes_cache directly
        let file1_node = FileNode::new(file1_p.clone(), "file1.txt".into(), false);
        let mut sub_node = FileNode::new(sub_p.clone(), "sub".into(), true);
        let file2_node = FileNode::new(file2_p.clone(), "file2.txt".into(), false);

        sub_node.children.push(file2_node);
        // In MyAppLogic, file_nodes_cache contains top-level items relative to root_path_for_scan
        // So, if root_path_for_scan was "/root", then file1_node and sub_node would be top-level.
        // For this helper, let's assume root_path_for_scan is "/" and these are its children.
        vec![file1_node, sub_node]
    }

    #[test]
    fn test_build_tree_item_descriptors_recursive_applogic() {
        let mut logic = MyAppLogic::new(); // No window needed for this static method test part
        logic.file_nodes_cache = make_test_tree_for_applogic(); // file1.txt, sub (dir)

        logic.next_tree_item_id_counter = 1; // Reset for predictability
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

        // Check ID mapping
        assert_eq!(logic.path_to_tree_item_id.len(), 3); // file1, sub, file2
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
        let mut logic = MyAppLogic::new();
        logic.file_nodes_cache = make_test_tree_for_applogic();
        let file1_p = PathBuf::from("/root/file1.txt");
        let file2_p = PathBuf::from("/root/sub/file2.txt");

        // Mutable find on file1.txt
        let file1_node_mut = MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p);
        assert!(file1_node_mut.is_some());
        file1_node_mut.unwrap().state = FileState::Selected;

        // Immutable find sees the change
        let file1_node_ref = MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &file1_p);
        assert!(file1_node_ref.is_some());
        assert_eq!(file1_node_ref.unwrap().state, FileState::Selected);

        // Find nested file2.txt
        let file2_node_ref = MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &file2_p);
        assert!(file2_node_ref.is_some());
        assert_eq!(file2_node_ref.unwrap().name, "file2.txt");

        let none_node =
            MyAppLogic::find_filenode_ref(&logic.file_nodes_cache, &PathBuf::from("/no/such/path"));
        assert!(none_node.is_none());
    }

    #[test]
    fn test_collect_visual_updates_recursive_applogic() {
        let mut logic = MyAppLogic::new();
        logic.file_nodes_cache = make_test_tree_for_applogic();

        let file1_p = PathBuf::from("/root/file1.txt");
        let sub_p = PathBuf::from("/root/sub"); // For checking an Unchecked entry
        let file2_p = PathBuf::from("/root/sub/file2.txt");

        // First, populate the ID map
        logic.next_tree_item_id_counter = 1;
        logic.path_to_tree_item_id.clear();
        let _ = MyAppLogic::build_tree_item_descriptors_recursive(
            &logic.file_nodes_cache,
            &mut logic.path_to_tree_item_id,
            &mut logic.next_tree_item_id_counter,
        );

        // Toggle file1.txt to Selected in the model
        {
            let f1_mut =
                MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file1_p).unwrap();
            f1_mut.state = FileState::Selected;
        }

        // Collect updates starting from one of the top-level nodes (e.g., the one for file1.txt)
        // Or collect for the whole cache. Let's collect for a specific node path (file1.txt)
        // To test recursion, we'd start from a directory. Let's start from 'sub' after setting its child 'file2.txt'

        // Let's modify file2 to be selected, and collect updates for 'sub'
        let sub_node_for_update_path = PathBuf::from("/root/sub");
        {
            let file2_mut =
                MyAppLogic::find_filenode_mut(&mut logic.file_nodes_cache, &file2_p).unwrap();
            file2_mut.state = FileState::Selected; // file2 is selected
            let sub_node_mut = MyAppLogic::find_filenode_mut(
                &mut logic.file_nodes_cache,
                &sub_node_for_update_path,
            )
            .unwrap();
            sub_node_mut.state = FileState::Unknown; // sub folder itself is unknown
        }

        let mut updates = Vec::new();
        // We need to find the 'sub' node in the cache to pass to collect_visual_updates_recursive
        let sub_node_ref = logic
            .file_nodes_cache
            .iter()
            .find(|n| n.path == sub_node_for_update_path)
            .unwrap();
        logic.collect_visual_updates_recursive(sub_node_ref, &mut updates);

        // We expect two entries from 'sub' node: 'sub' itself (Unknown -> Unchecked), and 'file2.txt' (Selected -> Checked)
        assert_eq!(
            updates.len(),
            2,
            "Expected updates for 'sub' and 'file2.txt'"
        );

        // Check 'sub' folder's visual state (should be Unchecked as its model state is Unknown)
        let sub_item_id = *logic.path_to_tree_item_id.get(&sub_p).unwrap();
        assert!(
            updates
                .iter()
                .any(|(id, state)| *id == sub_item_id && *state == CheckState::Unchecked)
        );

        // Check 'file2.txt' visual state (should be Checked as its model state is Selected)
        let file2_item_id = *logic.path_to_tree_item_id.get(&file2_p).unwrap();
        assert!(
            updates
                .iter()
                .any(|(id, state)| *id == file2_item_id && *state == CheckState::Checked)
        );
    }

    // Test for P2.5: Profile loading updates archive status
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

        // Use the simplified helper that creates the file directly in temp_dir
        let actual_profile_json_path = create_temp_profile_file_for_direct_load(
            &temp_dir,
            profile_name, // This will be the stem of the filename, and also the 'name' field in JSON
            &root_folder_for_profile,
            Some(archive_file_for_profile.clone()),
        );

        let event = AppEvent::FileOpenDialogCompleted {
            window_id: WindowId(1),
            result: Some(actual_profile_json_path.clone()), // Pass the direct path to the .json
        };

        let _cmds = logic.handle_event(event);

        // Assertions:
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

        // Since root_folder_for_profile is empty and scanned, file_nodes_cache will be empty.
        // With an existing archive and no selected files, status should be NoFilesSelected.
        assert_eq!(
            logic.current_archive_status,
            Some(ArchiveStatus::NoFilesSelected),
            "Archive status after load is incorrect"
        );
    }
}
