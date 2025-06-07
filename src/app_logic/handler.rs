use crate::core::{
    self, ArchiveStatus, ArchiverOperations, ConfigManagerOperations, FileNode,
    FileSystemScannerOperations, NodeStateApplicatorOperations, Profile, ProfileManagerOperations,
    ProfileRuntimeDataOperations, SelectionState, TokenCounterOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler,
    TreeItemDescriptor, TreeItemId, WindowId, types::MenuAction,
};
// Import MainWindowUiState, which we'll hold as an Option
use crate::app_logic::{MainWindowUiState, ui_constants};

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex}; // Added Mutex

// Import log macros
use log::{error, info, warn};

pub(crate) const APP_NAME_FOR_PROFILES: &str = "SourcePacker";

// These type aliases are used by MainWindowUiState.
pub(crate) type PathToTreeItemIdMap = HashMap<PathBuf, TreeItemId>;

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum PendingAction {
    SavingProfileAs,
    CreatingNewProfileGetName,
    CreatingNewProfileGetRoot,
    SettingArchivePath,
}

// --- Status Message Macros ---
macro_rules! status_message {
    ($self:expr, $severity:expr, $log_macro:ident, $($arg:tt)*) => {{
        let text = format!($($arg)*);
        $log_macro!("AppLogic Status: {}", text);

        if let Some(ui_state_ref) = &$self.ui_state {
            $self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: ui_state_ref.window_id,
                    label_id: ui_constants::STATUS_LABEL_GENERAL_ID,
                    text: text,
                    severity: $severity,
                });
        }
    }};
}

macro_rules! app_info { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Information, info, $($arg)*) }; }
macro_rules! app_error { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Error, error, $($arg)*) }; }
macro_rules! app_warn { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Warning, warn, $($arg)*) }; }

/*
 * Manages the core application orchestration and UI logic in a platform-agnostic manner.
 * It processes UI events, interacts with core services (config, profiles, file system),
 * and commands the platform layer to update the UI. It holds references to core data
 * via `ProfileRuntimeDataOperations` and `MainWindowUiState` (when a window exists)
 * for UI-specific state. Logging of its operations is done via the `log` crate.
 */
pub struct MyAppLogic {
    // Core application data operations interface
    app_session_data_ops: Arc<Mutex<dyn ProfileRuntimeDataOperations>>,
    // UI-specific state for the main window, present only when the window exists.
    ui_state: Option<MainWindowUiState>,

    // Dependencies (Managers and Services)
    config_manager: Arc<dyn ConfigManagerOperations>,
    profile_manager: Arc<dyn ProfileManagerOperations>,
    file_system_scanner: Arc<dyn FileSystemScannerOperations>,
    archiver: Arc<dyn ArchiverOperations>,
    token_counter_manager: Arc<dyn TokenCounterOperations>,
    state_manager: Arc<dyn NodeStateApplicatorOperations>,
    synchronous_command_queue: VecDeque<PlatformCommand>,
}

impl MyAppLogic {
    /*
     * Initializes a new instance of the application logic.
     * Requires implementations for core services and an Arc<Mutex<dyn ProfileRuntimeDataOperations>>
     * for session data management. Initializes `MainWindowUiState` to `None` as the window is not yet created.
     */
    pub fn new(
        app_session_data_ops: Arc<Mutex<dyn ProfileRuntimeDataOperations>>,
        config_manager: Arc<dyn ConfigManagerOperations>,
        profile_manager: Arc<dyn ProfileManagerOperations>,
        file_system_scanner: Arc<dyn FileSystemScannerOperations>,
        archiver: Arc<dyn ArchiverOperations>,
        token_counter: Arc<dyn TokenCounterOperations>,
        state_manager: Arc<dyn NodeStateApplicatorOperations>,
    ) -> Self {
        log::debug!("MyAppLogic::new called.");
        MyAppLogic {
            app_session_data_ops,
            ui_state: None,
            config_manager,
            profile_manager,
            file_system_scanner,
            archiver,
            token_counter_manager: token_counter,
            state_manager,
            synchronous_command_queue: VecDeque::new(),
        }
    }

    fn build_tree_item_descriptors_recursive_internal(
        nodes: &[FileNode], // This now comes from app_session_data_ops.get_snapshot_nodes()
        path_to_tree_item_id: &mut PathToTreeItemIdMap, // Belongs to MainWindowUiState
        next_tree_item_id_counter: &mut u64, // Belongs to MainWindowUiState
    ) -> Vec<TreeItemDescriptor> {
        // TODO: Phase 3: This method should be moved to MainWindowUiState.
        // For now, it remains a static helper here.
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
                    SelectionState::Selected => CheckState::Checked,
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
     * Handles the completion of the initial static UI setup for the main window.
     * It instantiates `MainWindowUiState`, then attempts to load the last used profile.
     * If successful, it uses that profile's settings to activate the UI. Otherwise,
     * it initiates a flow for the user to select or create a new profile.
     * The main window remains hidden until a profile is active.
     */
    fn _on_ui_setup_complete(&mut self, window_id: WindowId) {
        log::debug!(
            "Main window UI setup complete (ID: {:?}). Initializing MainWindowUiState.",
            window_id
        );
        self.ui_state = Some(MainWindowUiState::new(window_id));

        match self
            .config_manager
            .load_last_profile_name(APP_NAME_FOR_PROFILES)
        {
            Ok(Some(last_profile_name)) if !last_profile_name.is_empty() => {
                log::debug!("Found last used profile name: {}", last_profile_name);
                match self
                    .profile_manager
                    .load_profile(&last_profile_name, APP_NAME_FOR_PROFILES)
                {
                    Ok(profile) => {
                        app_info!(
                            self,
                            "Successfully loaded last profile '{}' on startup.",
                            profile.name
                        );
                        let operation_status_message =
                            format!("Profile '{}' loaded.", profile.name);
                        self._activate_profile_and_show_window(
                            window_id,
                            profile,
                            operation_status_message,
                        );
                    }
                    Err(e) => {
                        app_error!(
                            self,
                            "Failed to load last profile '{}': {:?}. Initiating selection.",
                            last_profile_name,
                            e
                        );
                        self.initiate_profile_selection_or_creation(window_id);
                    }
                }
            }
            Ok(_) => {
                app_info!(
                    self,
                    "No last profile name found or it was empty. Initiating selection/creation."
                );
                self.initiate_profile_selection_or_creation(window_id);
            }
            Err(e) => {
                app_error!(
                    self,
                    "Error loading last profile name: {:?}. Initiating selection.",
                    e
                );
                self.initiate_profile_selection_or_creation(window_id);
            }
        }
    }

    fn refresh_tree_view_from_cache(&mut self, window_id: WindowId) {
        let ui_state = match self.ui_state.as_mut() {
            Some(s) if s.window_id == window_id => s,
            _ => {
                log::error!(
                    "AppLogic: UI state for window_id {:?} must exist to refresh tree view. Current ui_state: {:?}",
                    window_id,
                    self.ui_state.as_ref().map(|s_ref| s_ref.window_id)
                );
                return;
            }
        };

        ui_state.next_tree_item_id_counter = 1;
        ui_state.path_to_tree_item_id.clear();

        let snapshot_nodes = self
            .app_session_data_ops
            .lock()
            .unwrap()
            .get_snapshot_nodes()
            .to_vec(); // Clone to avoid holding lock during recursive call if it were an issue

        let descriptors = Self::build_tree_item_descriptors_recursive_internal(
            &snapshot_nodes, // Use cloned or ref from lock
            &mut ui_state.path_to_tree_item_id,
            &mut ui_state.next_tree_item_id_counter,
        );
        self.synchronous_command_queue
            .push_back(PlatformCommand::PopulateTreeView {
                window_id,
                items: descriptors,
            });
    }
    /*
     * Converts an `ArchiveStatus` enum to a user-friendly string.
     * This function provides plain English descriptions for each status variant.
     */
    pub(crate) fn archive_status_to_plain_string(status: &ArchiveStatus) -> String {
        match status {
            ArchiveStatus::UpToDate => "Up to date.".to_string(),
            ArchiveStatus::NotYetGenerated => "Not yet generated.".to_string(),
            ArchiveStatus::OutdatedRequiresUpdate => "Out of date.".to_string(),
            ArchiveStatus::ArchiveFileMissing => "File missing.".to_string(),
            ArchiveStatus::NoFilesSelected => "No files selected.".to_string(),
            ArchiveStatus::ErrorChecking(Some(kind)) => {
                format!("Error: {:?}.", kind)
            }
            ArchiveStatus::ErrorChecking(None) => "Error: Unknown.".to_string(),
        }
    }

    pub(crate) fn update_current_archive_status(&mut self) {
        log::debug!("AppLogic: update_current_archive_status called.");
        let ui_state_mut = match self.ui_state.as_mut() {
            Some(s) => s,
            None => {
                log::error!(
                    "AppLogic: update_current_archive_status called but no UI state. Status cannot be cached or displayed."
                );
                return;
            }
        };
        let main_window_id = ui_state_mut.window_id;

        let (current_profile_name_opt, archive_path_opt, snapshot_nodes_clone) = {
            let data = self.app_session_data_ops.lock().unwrap();
            (
                data.get_profile_name(),
                data.get_archive_path(),
                data.get_snapshot_nodes().to_vec(), // Clone to release lock
            )
        };

        if current_profile_name_opt.is_none() {
            ui_state_mut.current_archive_status_for_ui = None;
            app_info!(self, "No profile loaded");

            let archive_label_text = "Archive: No profile loaded".to_string();
            self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: main_window_id,
                    label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                    text: archive_label_text,
                    severity: MessageSeverity::Information,
                });
            return;
        }

        let status = self
            .archiver
            .check_status(archive_path_opt.as_deref(), &snapshot_nodes_clone);
        log::debug!(
            "AppLogic: Checked archive status for profile '{:?}', archive path '{:?}', status: {:?}",
            current_profile_name_opt,
            archive_path_opt.as_deref(),
            status
        );

        ui_state_mut.current_archive_status_for_ui = Some(status.clone());

        let plain_status_string = Self::archive_status_to_plain_string(&status);
        let archive_label_text = format!("Archive: {}", plain_status_string);

        let severity_for_archive_msg = match status {
            ArchiveStatus::ErrorChecking(_) => MessageSeverity::Error,
            _ => MessageSeverity::Information,
        };

        self.synchronous_command_queue
            .push_back(PlatformCommand::UpdateLabelText {
                window_id: main_window_id,
                label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                text: archive_label_text,
                severity: severity_for_archive_msg,
            });

        if severity_for_archive_msg == MessageSeverity::Error {
            app_error!(self, "Archive status error: {:?}", status);
        } else {
            log::debug!(
                "AppLogic UpdateArchiveStatus (not an error): {}",
                plain_status_string
            );
        }
    }

    // collect_visual_updates_recursive is removed, replaced by logic in handle_treeview_item_toggled.

    /*
     * Recalculates the estimated token count for all currently selected files and
     * requests the UI to display this count.
     * It updates the general status label and the dedicated token count label.
     */
    pub(crate) fn _update_token_count_and_request_display(&mut self) {
        let token_count = self
            .app_session_data_ops
            .lock()
            .unwrap()
            .update_total_token_count_for_selected_files(&*self.token_counter_manager);

        app_info!(self, "Token count updated"); // This logs to general status

        if let Some(ui_state_ref) = &self.ui_state {
            self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: ui_state_ref.window_id,
                    label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
                    text: format!("Tokens: {}", token_count),
                    severity: MessageSeverity::Information,
                });
        }
    }

    fn handle_window_close_requested(&mut self, window_id: WindowId) {
        if !self
            .ui_state
            .as_ref()
            .map_or(false, |s| s.window_id == window_id)
        {
            return;
        }
        self.synchronous_command_queue
            .push_back(PlatformCommand::CloseWindow { window_id });
    }

    fn handle_window_destroyed(&mut self, window_id: WindowId) {
        if self
            .ui_state
            .as_ref()
            .map_or(false, |s| s.window_id == window_id)
        {
            log::debug!(
                "AppLogic: Main window (ID: {:?}) destroyed notification received. Clearing UI state.",
                self.ui_state.as_ref().unwrap().window_id
            );
            self.ui_state = None;
        } else {
            log::debug!(
                "AppLogic: Window (ID: {:?}) destroyed, but it was not the main window tracked by ui_state.",
                window_id
            );
        }
    }

    fn handle_treeview_item_toggled(
        &mut self,
        window_id: WindowId,
        item_id: TreeItemId,
        new_check_state: CheckState,
    ) {
        let ui_state_ref = match self.ui_state.as_ref() {
            Some(s) if s.window_id == window_id => s,
            _ => {
                log::debug!(
                    "AppLogic: TreeViewItemToggled event for non-matching or non-existent UI state. Window ID: {:?}. Ignoring.",
                    window_id
                );
                return;
            }
        };

        log::debug!(
            "TreeItem {:?} toggled to UI state {:?}.",
            item_id,
            new_check_state
        );

        let mut path_of_toggled_node_opt: Option<PathBuf> = None;
        for (path_candidate, id_in_map) in &ui_state_ref.path_to_tree_item_id {
            if *id_in_map == item_id {
                path_of_toggled_node_opt = Some(path_candidate.clone());
                break;
            }
        }

        let path_for_model_update = match path_of_toggled_node_opt {
            Some(p) => p,
            None => {
                log::error!(
                    "AppLogic: Could not find path for TreeItemId {:?} from UI event.",
                    item_id
                );
                return;
            }
        };

        // Get the original state of the node before updating it.
        // We need to check if the node *itself* was New, or if it's a folder,
        // if it or any descendants contained a new file.
        let was_considered_new_for_display: bool = {
            let app_data_guard = self.app_session_data_ops.lock().unwrap();
            if let Some((original_state, is_dir)) =
                app_data_guard.get_node_attributes_for_path(&path_for_model_update)
            {
                if is_dir {
                    // For directories, it was "new" for display if it or its descendants had a new file
                    app_data_guard.does_path_or_descendants_contain_new_file(&path_for_model_update)
                } else {
                    // For files, it was "new" if its state was New
                    original_state == SelectionState::New
                }
            } else {
                log::warn!(
                    "AppLogic: Could not get original node attributes for path {:?} to check if it was New.",
                    path_for_model_update
                );
                false
            }
        };

        let new_model_file_state = match new_check_state {
            CheckState::Checked => SelectionState::Selected,
            CheckState::Unchecked => SelectionState::Deselected,
        };

        let collected_changes = self
            .app_session_data_ops
            .lock()
            .unwrap()
            .update_node_state_and_collect_changes(
                &path_for_model_update,
                new_model_file_state,
                &*self.state_manager,
            );

        log::debug!(
            "Requesting {} visual updates for TreeView after toggle of {:?}.",
            collected_changes.len(),
            path_for_model_update
        );

        for (changed_path, new_file_state) in collected_changes {
            if let Some(tree_item_id_to_update) =
                ui_state_ref.path_to_tree_item_id.get(&changed_path)
            {
                let check_state_for_ui = match new_file_state {
                    SelectionState::Selected => CheckState::Checked,
                    _ => CheckState::Unchecked, // This includes Deselected and New (though New shouldn't be set here by update_folder_selection)
                };
                self.synchronous_command_queue.push_back(
                    PlatformCommand::UpdateTreeItemVisualState {
                        window_id,
                        item_id: *tree_item_id_to_update,
                        new_state: check_state_for_ui,
                    },
                );
                // After a state change, we also need to check if the "New" indicator needs to be redrawn
                // for this specific item (and potentially its parents, handled by is_tree_item_new).
                // This redraw is particularly for the item whose state directly changed.
                self.synchronous_command_queue
                    .push_back(PlatformCommand::RedrawTreeItem {
                        window_id,
                        item_id: *tree_item_id_to_update,
                    });
            } else {
                log::error!(
                    "AppLogic: Path {:?} (from collected_changes) not found in path_to_tree_item_id during TreeViewItemToggled update.",
                    changed_path
                );
            }
        }

        // If the primary item toggled *was* considered "new" for display purposes,
        // and its state changed (to Selected/Deselected),
        // queue a command to redraw it and its affected ancestors.
        // The actual `is_tree_item_new` check for the *current* state will determine if the dot remains.
        // The RedrawTreeItem command ensures the UI updates if the "new" status *might* have changed.
        if was_considered_new_for_display {
            // The primary item itself needs a redraw check
            self.synchronous_command_queue
                .push_back(PlatformCommand::RedrawTreeItem { window_id, item_id });
            log::debug!(
                "AppLogic: Item {:?} (path {:?}) was considered 'New' for display and changed state. Queueing RedrawTreeItem.",
                item_id,
                path_for_model_update
            );

            // Also, enqueue redraws for all parent items, as their "contains new file" status might have changed.
            let mut current_path_for_ancestor_check = path_for_model_update.clone();
            // Loop while current path has a parent and that parent is not "above" the scan root
            // (i.e., parent_path is not equal to the parent of the scan root itself).
            let scan_root_parent = self
                .app_session_data_ops
                .lock()
                .unwrap()
                .get_root_path_for_scan()
                .parent()
                .map(|p| p.to_path_buf());

            while let Some(parent_path) = current_path_for_ancestor_check.parent() {
                // Stop if parent_path is effectively the directory containing the root_path_for_scan
                if Some(parent_path.to_path_buf()) == scan_root_parent
                    || parent_path.as_os_str().is_empty()
                {
                    break;
                }

                if let Some(parent_item_id) = ui_state_ref.path_to_tree_item_id.get(parent_path) {
                    self.synchronous_command_queue
                        .push_back(PlatformCommand::RedrawTreeItem {
                            window_id,
                            item_id: *parent_item_id,
                        });
                    log::debug!(
                        "AppLogic: Queueing RedrawTreeItem for ancestor {:?} (path {:?}) due to toggle of descendant.",
                        parent_item_id,
                        parent_path
                    );
                }
                current_path_for_ancestor_check = parent_path.to_path_buf();
            }
        }

        self.update_current_archive_status();
        self._update_token_count_and_request_display();
    }

    fn _do_generate_archive(&mut self) {
        if self.ui_state.is_none() {
            log::error!("Cannot generate archive: No UI state (main window).");
            return;
        }
        log::debug!("'Generate Archive' (via menu or old button) triggered.");

        let (current_profile_name_opt, archive_path_opt, snapshot_nodes_clone, root_path_clone) = {
            let data = self.app_session_data_ops.lock().unwrap();
            (
                data.get_profile_name(),
                data.get_archive_path(),
                data.get_snapshot_nodes().to_vec(), // Clone to release lock
                data.get_root_path_for_scan(),
            )
        };

        if current_profile_name_opt.is_none() {
            app_error!(self, "No profile loaded. Cannot save archive.");
            return;
        }

        let archive_path = match archive_path_opt {
            Some(ap) => ap,
            None => {
                app_error!(
                    self,
                    "No archive path set for current profile. Cannot save archive."
                );
                return;
            }
        };

        match self
            .archiver
            .create_content(&snapshot_nodes_clone, &root_path_clone)
        {
            Ok(content) => match self.archiver.save(&archive_path, &content) {
                Ok(_) => {
                    app_info!(self, "Archive saved to '{}'.", archive_path.display());
                    self.update_current_archive_status();
                }
                Err(e) => {
                    app_error!(
                        self,
                        "Failed to save archive content to '{}': {}",
                        archive_path.display(),
                        e
                    );
                }
            },
            Err(e) => {
                app_error!(self, "Failed to create archive content: {}", e);
            }
        }
    }

    fn handle_button_clicked(&mut self, window_id: WindowId, control_id: i32) {
        unimplemented!(
            "ButtonClicked handler not implemented for window_id {:?} control_id: {}",
            window_id,
            control_id,
        );
    }

    fn handle_menu_load_profile_clicked(&mut self) {
        log::debug!("MenuAction::LoadProfile action received by AppLogic.");
        let ui_state_ref = match self.ui_state.as_ref() {
            Some(s) => s,
            None => {
                log::warn!("Cannot handle LoadProfile: No UI state (main window).");
                return;
            }
        };

        let profile_dir_opt = self
            .profile_manager
            .get_profile_dir_path(APP_NAME_FOR_PROFILES);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowOpenFileDialog {
                window_id: ui_state_ref.window_id,
                title: "Load Profile".to_string(),
                filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                initial_dir: profile_dir_opt,
            });
    }

    fn handle_file_open_dialog_completed(&mut self, window_id: WindowId, result: Option<PathBuf>) {
        if !self
            .ui_state
            .as_ref()
            .map_or(false, |s| s.window_id == window_id)
        {
            log::warn!(
                "FileOpenProfileDialogCompleted for non-matching or non-existent UI state. Window ID: {:?}. Ignoring.",
                window_id
            );
            return;
        }

        let profile_file_path = match result {
            Some(pfp) => pfp,
            None => {
                log::debug!("Load profile cancelled.");
                return;
            }
        };

        log::debug!("Profile selected for load: {:?}", profile_file_path);
        match self
            .profile_manager
            .load_profile_from_path(&profile_file_path)
        {
            Ok(loaded_profile) => {
                let profile_name_clone = loaded_profile.name.clone();
                log::debug!(
                    "Successfully loaded profile '{}' via manager from path.",
                    profile_name_clone
                );

                if let Err(e) = self
                    .config_manager
                    .save_last_profile_name(APP_NAME_FOR_PROFILES, &profile_name_clone)
                {
                    app_warn!(
                        self,
                        "Failed to save last profile name '{}': {:?}",
                        profile_name_clone,
                        e
                    );
                }
                let status_msg = format!("Profile '{}' loaded and scanned.", profile_name_clone);
                self._activate_profile_and_show_window(window_id, loaded_profile, status_msg);
            }
            Err(e) => {
                app_error!(
                    self,
                    "Failed to load profile from {:?} via manager: {:?}",
                    profile_file_path,
                    e
                );
                self.app_session_data_ops.lock().unwrap().clear();

                if let Some(ui_state_mut) = self.ui_state.as_mut() {
                    ui_state_mut.current_archive_status_for_ui = None;
                }
            }
        }
    }

    fn handle_menu_save_profile_as_clicked(&mut self) {
        log::debug!("MenuAction::SaveProfileAs action received by AppLogic.");
        let ui_state_mut = match self.ui_state.as_mut() {
            Some(s) => s,
            None => {
                log::warn!("Cannot handle SaveProfileAs: No UI state (main window).");
                return;
            }
        };

        let profile_dir_opt = self
            .profile_manager
            .get_profile_dir_path(APP_NAME_FOR_PROFILES);
        let base_name = self
            .app_session_data_ops
            .lock()
            .unwrap()
            .get_profile_name()
            .map_or_else(|| "new_profile".to_string(), |name| name.clone());
        let sanitized_current_name = core::profiles::sanitize_profile_name(&base_name);
        let default_filename = format!("{}.json", sanitized_current_name);

        ui_state_mut.pending_action = Some(PendingAction::SavingProfileAs);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowSaveFileDialog {
                window_id: ui_state_mut.window_id,
                title: "Save Profile As".to_string(),
                default_filename,
                filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                initial_dir: profile_dir_opt,
            });
    }

    /*
     * Handles the outcome of a file save dialog. The behavior depends on the
     * `pending_action` that was active when the dialog was initiated.
     * This function delegates to specific handlers based on that action.
     */
    fn handle_file_save_dialog_completed(&mut self, window_id: WindowId, result: Option<PathBuf>) {
        // Ensure UI state exists for the given window_id and get mutable access.
        let ui_state_mut = match self.ui_state.as_mut().filter(|s| s.window_id == window_id) {
            Some(s) => s,
            None => {
                log::warn!(
                    "FileSaveDialogCompleted received for an unknown or non-main window (ID: {:?}). Ignoring event.",
                    window_id
                );
                return;
            }
        };

        // Take the pending action. This consumes it from ui_state_mut.
        let action = ui_state_mut.pending_action.take();
        log::debug!(
            "FileSaveDialogCompleted with pending action: {:?}, for result: {:?}",
            action,
            result
        );

        match action {
            Some(PendingAction::SettingArchivePath) => {
                self._handle_file_save_dialog_for_setting_archive_path(window_id, result);
            }
            Some(PendingAction::SavingProfileAs) => {
                self._handle_file_save_dialog_for_saving_profile_as(window_id, result);
            }
            Some(PendingAction::CreatingNewProfileGetName)
            | Some(PendingAction::CreatingNewProfileGetRoot) => {
                // These pending actions expect an InputDialog or FolderPickerDialog, not a FileSaveDialog.
                app_error!(
                    self,
                    "FileSaveDialogCompleted received, but was expecting dialog for {:?}. This is a logic error.",
                    action // action is already Some(PendingAction) here.
                );
            }
            None => {
                // This implies a FileSaveDialog was completed but MyAppLogic wasn't expecting one.
                app_warn!(
                    self,
                    "FileSaveDialogCompleted received, but no pending action was set. Ignoring."
                );
            }
        }
    }

    /*
     * Handles the outcome of a file save dialog initiated for setting a profile's archive path.
     * If a path is selected, it updates the current profile's archive path in the
     * application session data, saves the profile modifications, and refreshes relevant UI elements
     * like the window title and archive status indicators.
     */
    fn _handle_file_save_dialog_for_setting_archive_path(
        &mut self,
        window_id: WindowId,
        result: Option<PathBuf>,
    ) {
        let path = match result {
            Some(p) => p,
            None => {
                log::debug!("User cancelled the 'Set Archive Path' dialog.");
                // Update UI elements that might depend on the archive path status being unchanged.
                self._update_generate_archive_menu_item_state(window_id);
                return;
            }
        };

        log::debug!("User selected archive path: {:?}", path);

        let profile_to_save_opt = {
            let mut profile_runtime_data = self.app_session_data_ops.lock().unwrap();
            if profile_runtime_data.get_profile_name().is_none() {
                app_error!(self, "No profile is active. Cannot set archive path.");
                return; // No active profile, so nothing to set the path for.
            }
            profile_runtime_data.set_archive_path(Some(path.clone()));
            Some(profile_runtime_data.create_profile_snapshot())
        };

        // This should always be Some if we didn't return early due to no active profile.
        let profile_to_save = match profile_to_save_opt {
            Some(p) => p,
            None => {
                // This case indicates an unexpected logic flow if reached.
                log::error!(
                    "_handle_file_save_dialog_for_setting_archive_path: profile_to_save was unexpectedly None despite an active profile check."
                );
                return;
            }
        };

        match self
            .profile_manager
            .save_profile(&profile_to_save, APP_NAME_FOR_PROFILES)
        {
            Ok(_) => {
                app_info!(
                    self,
                    "Archive path set to '{}' for profile '{}' and profile saved.",
                    path.display(),
                    profile_to_save.name
                );
                self._update_window_title_with_profile_and_archive(window_id);
                self.update_current_archive_status();
                self._update_generate_archive_menu_item_state(window_id);
            }
            Err(e) => {
                app_error!(
                    self,
                    "Failed to save profile '{}' after setting archive path: {}",
                    profile_to_save.name,
                    e
                );
            }
        }
    }

    fn make_profile_name(path: Option<PathBuf>) -> Result<String, String> {
        let profile_save_path =
            path.ok_or_else(|| "User cancelled the 'Save Profile As' dialog.".to_string())?;

        log::debug!(
            "User selected path for 'Save Profile As': {:?}",
            profile_save_path
        );

        let profile_name_osstr = profile_save_path
            .file_stem()
            .ok_or_else(|| "Could not extract profile name from save path.".to_string())?;

        let profile_name_str = profile_name_osstr
            .to_str()
            .ok_or_else(|| "Profile save filename stem not valid UTF-8.".to_string())?
            .to_string();

        if profile_name_str.trim().is_empty()
            || !profile_name_str
                .chars()
                .all(core::profiles::is_valid_profile_name_char)
        {
            return Err(format!(
                "Invalid profile name extracted from path: '{}'",
                profile_name_str
            ));
        }

        Ok(profile_name_str)
    }

    /*
     * Handles the outcome of a file save dialog initiated for saving the current profile under a new name or path.
     * If a path is selected, it extracts the new profile name, updates the application session data
     * to reflect this new profile (name, path, etc.), saves it through the profile manager,
     * and refreshes relevant UI elements.
     */
    fn _handle_file_save_dialog_for_saving_profile_as(
        &mut self,
        window_id: WindowId,
        result: Option<PathBuf>,
    ) {
        let profile_name_str = match Self::make_profile_name(result) {
            Ok(pfn) => pfn,
            Err(e) => {
                app_error!(self, "{}", e);
                return;
            }
        };

        // Update the application session data to reflect the new profile name.
        let profile = {
            // Local scope for Mutex lock.
            let mut profile_runtime_data = self.app_session_data_ops.lock().unwrap();
            profile_runtime_data.set_profile_name(Some(profile_name_str));
            profile_runtime_data.set_archive_path(None); // Reset archive path for new profile
            profile_runtime_data.create_profile_snapshot()
        };
        if let Err(e) = self
            .profile_manager
            .save_profile(&profile, APP_NAME_FOR_PROFILES)
        {
            app_error!(
                self,
                "Failed to save profile '{}' in 'Save Profile As': {}",
                profile.name,
                e
            );
        }
        self._update_window_title_with_profile_and_archive(window_id);
        if let Err(e) = self
            .config_manager
            .save_last_profile_name(APP_NAME_FOR_PROFILES, &profile.name)
        {
            app_warn!(
                self,
                "Failed to save last profile name '{}': {:?}",
                profile.name,
                e
            );
        }
        self.update_current_archive_status();
    }

    fn handle_window_resized(&mut self, _window_id: WindowId, _width: i32, _height: i32) {
        log::debug!(
            "Window resized: ID {:?}, W:{}, H:{}",
            _window_id,
            _width,
            _height
        );
    }

    fn handle_menu_refresh_file_list_clicked(&mut self) {
        log::debug!("MenuAction::RefreshFileList action received by AppLogic.");
        let main_window_id = match self.ui_state.as_ref().map(|s| s.window_id) {
            Some(id) => id,
            None => {
                log::error!("AppLogic: Refresh requested but no main window UI state. Ignoring.");
                return;
            }
        };

        let (current_profile_name_clone, root_path_to_scan, current_selection_paths_opt) = {
            let data = self.app_session_data_ops.lock().unwrap();
            let name = data.get_profile_name();
            if name.is_none() {
                app_warn!(self, "Refresh: No profile active.");
                return;
            }

            // Use the new method here
            let (selected, deselected) = data.get_current_selection_paths();

            (
                name,
                data.get_root_path_for_scan(),
                Some((selected, deselected)), // Pass the actual current selections
            )
        };

        let current_profile_name = match current_profile_name_clone {
            Some(n) => n,
            None => return, // Already handled by the check above
        };
        let (current_selected_paths, current_deselected_paths) = match current_selection_paths_opt {
            Some(paths) => paths,
            None => {
                // Should not happen if profile is active
                app_error!(
                    self,
                    "Refresh: Could not get current selection paths for active profile."
                );
                return;
            }
        };

        log::debug!(
            "Refreshing file list for profile '{}', root: {:?}",
            current_profile_name,
            root_path_to_scan
        );

        // TODO: Do we really need a full new scan_directory here?
        match self.file_system_scanner.scan_directory(&root_path_to_scan) {
            Ok(new_nodes) => {
                {
                    let mut data = self.app_session_data_ops.lock().unwrap();
                    data.set_snapshot_nodes(new_nodes);
                    log::debug!(
                        "Scan successful, {} top-level nodes found.",
                        data.get_snapshot_nodes().len()
                    );

                    data.apply_selection_states_to_snapshot(
                        &*self.state_manager,
                        &current_selected_paths,
                        &current_deselected_paths,
                    );

                    // The ProfileRuntimeData::update_cached_file_details_recursive logic
                    // is now internal to load_profile_into_session or similar,
                    // or would need a dedicated trait method if used standalone.
                    // For refresh, it's implicitly handled if selections are re-applied and token count is updated.
                    // We need to ensure the token details are updated.
                    // For now, let's assume update_total_token_count also refreshes details if necessary,
                    // or that apply_selection_states_to_snapshot might trigger it.
                    // A more explicit call might be `data.update_cached_file_details(&*self.token_counter_manager);`
                    // Let's rely on update_total_token_count to handle this for now.
                }

                log::debug!(
                    "Applied selections from profile '{}' to refreshed tree and updated token cache.",
                    current_profile_name
                );

                self.refresh_tree_view_from_cache(main_window_id);
                self.update_current_archive_status();
                self._update_token_count_and_request_display();
                app_info!(
                    self,
                    "File list refreshed for profile '{}'.",
                    current_profile_name
                );
            }
            Err(e) => {
                app_error!(self, "Failed to refresh file list: {}", e);
            }
        }
    }

    /*
     * Activates a given profile: sets it as current in `ProfileRuntimeDataOperations`, scans its root folder,
     * applies its selection state, refreshes UI elements, and shows the window.
     * Assumes `self.ui_state` is Some and `window_id` matches `self.ui_state.window_id`.
     */
    fn _activate_profile_and_show_window(
        &mut self,
        window_id: WindowId,
        profile_to_activate: Profile,
        initial_operation_status_message: String,
    ) {
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "Mismatched window ID or no UI state for _activate_profile_and_show_window"
        );

        let scan_result = {
            let mut data = self.app_session_data_ops.lock().unwrap();
            data.load_profile_into_session(
                profile_to_activate,
                &*self.file_system_scanner,
                &*self.state_manager,
                &*self.token_counter_manager,
            )
        };

        let (scan_was_successful, final_status_message) = match scan_result {
            Ok(_) => (true, initial_operation_status_message),
            Err(scan_error_message) => (false, scan_error_message),
        };

        self._update_window_title_with_profile_and_archive(window_id);

        {
            let ui_state_mut = self
                .ui_state
                .as_mut()
                .expect("UI state must exist here for _activate_profile_and_show_window");
            ui_state_mut.next_tree_item_id_counter = 1;
            ui_state_mut.path_to_tree_item_id.clear();

            let snapshot_nodes_clone = self
                .app_session_data_ops
                .lock()
                .unwrap()
                .get_snapshot_nodes()
                .to_vec(); // Clone to release lock

            let descriptors = Self::build_tree_item_descriptors_recursive_internal(
                &snapshot_nodes_clone,
                &mut ui_state_mut.path_to_tree_item_id,
                &mut ui_state_mut.next_tree_item_id_counter,
            );
            self.synchronous_command_queue
                .push_back(PlatformCommand::PopulateTreeView {
                    window_id,
                    items: descriptors,
                });
        }

        self.update_current_archive_status();
        self._update_token_count_and_request_display();

        if scan_was_successful {
            app_info!(self, "{}", final_status_message);
        } else {
            app_error!(self, "{}", final_status_message);
        }

        self._update_generate_archive_menu_item_state(window_id);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowWindow { window_id });
    }

    pub(crate) fn initiate_profile_selection_or_creation(&mut self, window_id: WindowId) {
        log::debug!("Initiating profile selection or creation flow.");
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "initiate_profile_selection_or_creation called with mismatching window ID or no UI state."
        );

        match self.profile_manager.list_profiles(APP_NAME_FOR_PROFILES) {
            Ok(available_profiles) => {
                let (title, prompt, emphasize_create_new) = if available_profiles.is_empty() {
                    (
                        "Welcome to SourcePacker!".to_string(),
                        "No profiles found. Please create a new profile to get started."
                            .to_string(),
                        true,
                    )
                } else {
                    (
                        "Select or Create Profile".to_string(),
                        "Please select an existing profile, or create a new one.".to_string(),
                        false,
                    )
                };
                log::debug!(
                    "Found {} available profiles. Dialog prompt: '{}'",
                    available_profiles.len(),
                    prompt
                );
                self.synchronous_command_queue.push_back(
                    PlatformCommand::ShowProfileSelectionDialog {
                        window_id,
                        available_profiles,
                        title,
                        prompt,
                        emphasize_create_new,
                    },
                );
            }
            Err(e) => {
                app_error!(
                    self,
                    "Failed to list profiles: {:?}. Cannot proceed with profile selection.",
                    e
                );
            }
        }
    }

    fn handle_profile_selection_dialog_completed(
        &mut self,
        window_id: WindowId,
        chosen_profile_name: Option<String>,
        create_new_requested: bool,
        user_cancelled: bool,
    ) {
        if !self
            .ui_state
            .as_ref()
            .map_or(false, |s| s.window_id == window_id)
        {
            log::debug!(
                "ProfileSelectionDialogCompleted for non-matching or non-existent UI state. Window ID: {:?}. Ignoring.",
                window_id
            );
            return;
        }

        log::debug!(
            "ProfileSelectionDialogCompleted event received: chosen: {:?}, create_new: {}, cancelled: {}",
            chosen_profile_name,
            create_new_requested,
            user_cancelled
        );

        if user_cancelled {
            log::debug!("Profile selection was cancelled by user. Quitting application.");
            self.synchronous_command_queue
                .push_back(PlatformCommand::QuitApplication);
            return;
        }

        if create_new_requested {
            log::debug!("User requested to create a new profile.");
            self.start_new_profile_creation_flow(window_id);
            return;
        }

        let profile_name_to_load = match chosen_profile_name {
            Some(name) => name,
            None => {
                app_warn!(
                    self,
                    "ProfileSelectionDialogCompleted in unexpected state (no choice, not create, not cancelled). Re-initiating."
                );
                self.initiate_profile_selection_or_creation(window_id);
                return;
            }
        };

        log::debug!(
            "User chose profile '{}'. Attempting to load.",
            profile_name_to_load
        );
        match self
            .profile_manager
            .load_profile(&profile_name_to_load, APP_NAME_FOR_PROFILES)
        {
            Ok(profile) => {
                log::debug!("Successfully loaded chosen profile '{}'.", profile.name);
                let operation_status_message = format!("Profile '{}' loaded.", profile.name);
                if let Err(e) = self
                    .config_manager
                    .save_last_profile_name(APP_NAME_FOR_PROFILES, &profile.name)
                {
                    app_warn!(
                        self,
                        "Failed to save last profile name '{}': {:?}",
                        profile.name,
                        e
                    );
                }
                self._activate_profile_and_show_window(
                    window_id,
                    profile,
                    operation_status_message,
                );
            }
            Err(e) => {
                app_error!(
                    self,
                    "Could not load profile '{}': {:?}. Please try again or create a new one.",
                    profile_name_to_load,
                    e
                );
                self.initiate_profile_selection_or_creation(window_id);
            }
        }
    }

    fn start_new_profile_creation_flow(&mut self, window_id: WindowId) {
        log::debug!("Starting new profile creation flow (Step 1: Get Name).");
        let ui_state_mut = self
            .ui_state
            .as_mut()
            .filter(|s| s.window_id == window_id)
            .expect("UI state must exist and match window_id for start_new_profile_creation_flow");

        ui_state_mut.pending_action = Some(PendingAction::CreatingNewProfileGetName);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowInputDialog {
                window_id,
                title: "New Profile (1/2): Name".to_string(),
                prompt: "Enter a name for the new profile:".to_string(),
                default_text: None,
                context_tag: Some("NewProfileName".to_string()),
            });
    }

    fn handle_input_dialog_completed(
        &mut self,
        window_id: WindowId,
        text: Option<String>,
        context_tag: Option<String>,
    ) {
        if !self
            .ui_state
            .as_ref()
            .map_or(false, |s| s.window_id == window_id)
        {
            log::warn!(
                "InputDialogCompleted for an unknown or non-main window (ID: {:?}). Ignoring.",
                window_id
            );
            return;
        }

        log::debug!(
            "InputDialogCompleted: text: {:?}, context_tag: {:?}",
            text,
            context_tag
        );

        match context_tag.as_deref() {
            Some("NewProfileName") => {
                let profile_name_text = match text {
                    Some(t) => t,
                    None => {
                        log::debug!(
                            "New profile name input cancelled. Returning to profile selection."
                        );
                        let ui_state_mut = self.ui_state.as_mut().unwrap();
                        ui_state_mut.pending_action = None;
                        ui_state_mut.pending_new_profile_name = None;
                        self.initiate_profile_selection_or_creation(window_id);
                        return;
                    }
                };

                if profile_name_text.trim().is_empty()
                    || !profile_name_text
                        .chars()
                        .all(core::profiles::is_valid_profile_name_char)
                {
                    app_warn!(
                        self,
                        "Invalid profile name. Please use only letters, numbers, spaces, underscores, or hyphens."
                    );
                    self.synchronous_command_queue
                        .push_back(PlatformCommand::ShowInputDialog {
                            window_id,
                            title: "New Profile (1/2): Name".to_string(),
                            prompt: "Enter a name for the new profile (invalid previous attempt):"
                                .to_string(),
                            default_text: Some(profile_name_text),
                            context_tag: Some("NewProfileName".to_string()),
                        });
                    return;
                }

                log::debug!(
                    "New profile name '{}' is valid. Proceeding to Step 2 (Get Root Folder).",
                    profile_name_text
                );
                let ui_state_mut = self.ui_state.as_mut().unwrap();
                ui_state_mut.pending_new_profile_name = Some(profile_name_text);
                ui_state_mut.pending_action = Some(PendingAction::CreatingNewProfileGetRoot);

                self.synchronous_command_queue
                    .push_back(PlatformCommand::ShowFolderPickerDialog {
                        window_id,
                        title: "New Profile (2/2): Select Root Folder".to_string(),
                        initial_dir: None,
                    });
            }
            _ => {
                app_warn!(
                    self,
                    "InputDialogCompleted with unhandled context: {:?}",
                    context_tag
                );
                let ui_state_mut = self.ui_state.as_mut().unwrap();
                ui_state_mut.pending_action = None;
            }
        }
    }

    fn handle_folder_picker_dialog_completed(
        &mut self,
        window_id: WindowId,
        path: Option<PathBuf>,
    ) {
        let ui_state_mut = match self.ui_state.as_mut().filter(|s| s.window_id == window_id) {
            Some(s) => s,
            None => {
                log::warn!(
                    "FolderPickerDialogCompleted for an unknown or non-main window (ID: {:?}). Ignoring.",
                    window_id
                );
                return;
            }
        };

        log::debug!("FolderPickerDialogCompleted: path: {:?}", path);
        ui_state_mut.pending_action = None;

        let root_folder_path = match path {
            Some(p) => p,
            None => {
                log::debug!("Root folder selection cancelled. Returning to profile selection.");
                ui_state_mut.pending_new_profile_name = None;
                self.initiate_profile_selection_or_creation(window_id);
                return;
            }
        };

        let profile_name = match ui_state_mut.pending_new_profile_name.take() {
            Some(name) => name,
            None => {
                app_warn!(
                    self,
                    "FolderPickerDialogCompleted but no pending profile name. Re-initiating profile selection."
                );
                self.initiate_profile_selection_or_creation(window_id);
                return;
            }
        };

        log::debug!(
            "Creating new profile '{}' with root folder {:?}.",
            profile_name,
            root_folder_path
        );
        let new_profile_dto = Profile::new(profile_name.clone(), root_folder_path.clone());

        match self
            .profile_manager
            .save_profile(&new_profile_dto, APP_NAME_FOR_PROFILES)
        {
            Ok(_) => {
                log::debug!("Successfully saved new profile '{}'.", new_profile_dto.name);
                let operation_status_message =
                    format!("New profile '{}' created and loaded.", new_profile_dto.name);

                if let Err(e) = self
                    .config_manager
                    .save_last_profile_name(APP_NAME_FOR_PROFILES, &new_profile_dto.name)
                {
                    app_warn!(
                        self,
                        "Failed to save last profile name '{}': {:?}",
                        new_profile_dto.name,
                        e
                    );
                }
                self._activate_profile_and_show_window(
                    window_id,
                    new_profile_dto,
                    operation_status_message,
                );
            }
            Err(e) => {
                app_error!(
                    self,
                    "Failed to save new profile '{}': {:?}. Please try again.",
                    profile_name,
                    e
                );
                self.initiate_profile_selection_or_creation(window_id);
            }
        }
    }

    fn _update_window_title_with_profile_and_archive(&mut self, window_id: WindowId) {
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "_update_window_title_with_profile_and_archive called with mismatching window ID or no UI state."
        );

        // Lock once to get a reference to app_session_data_ops implementation
        let app_data_ops_guard = self.app_session_data_ops.lock().unwrap();
        // Pass the dereferenced guard (which is &dyn ProfileRuntimeDataOperations)
        // to MainWindowUiState::compose_window_title.
        let title = MainWindowUiState::compose_window_title(&*app_data_ops_guard);

        // Explicitly drop the guard before pushing to the command queue if it's no longer needed,
        // though in this specific case, its scope ends naturally.
        drop(app_data_ops_guard);

        self.synchronous_command_queue
            .push_back(PlatformCommand::SetWindowTitle { window_id, title });
    }
    fn _update_generate_archive_menu_item_state(&mut self, window_id: WindowId) {
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "_update_generate_archive_menu_item_state called with mismatching window ID or no UI state."
        );

        let enabled = self
            .app_session_data_ops
            .lock()
            .unwrap()
            .get_archive_path()
            .is_some();

        if enabled {
            log::debug!("'Generate Archive' menu item can now function (archive path is set).");
        } else {
            log::debug!(
                "'Generate Archive' menu item functionality depends on archive path (currently not set)."
            );
        }
        // Note: The actual command to enable/disable the menu item seems to be missing from original logic too.
        // This method currently only logs. If it's meant to send a command, that needs to be added.
        // For MVP separation, MyAppLogic decides if it should be enabled, then queues a command.
        // The actual PlatformCommand::SetMenuItemEnabled is not shown here.
    }

    fn handle_menu_set_archive_path_clicked(&mut self) {
        let ui_state_mut = match self.ui_state.as_mut() {
            Some(s) => s,
            None => {
                log::warn!("Cannot handle SetArchivePath: No UI state (main window).");
                return;
            }
        };

        log::debug!("MenuAction::SetArchivePath action received by AppLogic.");
        let (current_profile_name_opt, current_archive_path_opt, current_root_path) = {
            let data = self.app_session_data_ops.lock().unwrap();
            (
                data.get_profile_name(),
                data.get_archive_path(),
                data.get_root_path_for_scan(),
            )
        };

        if current_profile_name_opt.is_none() {
            app_warn!(self, "Cannot set archive path: No profile is active.");
            return;
        }

        ui_state_mut.pending_action = Some(PendingAction::SettingArchivePath);

        let default_filename = current_archive_path_opt
            .as_ref()
            .and_then(|ap| ap.file_name())
            .map(|os_name| os_name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                current_profile_name_opt
                    .as_ref()
                    .map(|p_name| core::profiles::sanitize_profile_name(p_name) + ".txt")
                    .unwrap_or_else(|| "archive.txt".to_string())
            });

        let initial_dir_for_dialog = current_archive_path_opt
            .as_ref()
            .and_then(|ap| ap.parent().map(PathBuf::from))
            .or_else(|| Some(current_root_path));

        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowSaveFileDialog {
                window_id: ui_state_mut.window_id,
                title: "Set Archive File Path".to_string(),
                default_filename,
                filter_spec: "Text Files (*.txt)\0*.txt\0All Files (*.*)\0*.*\0\0".to_string(),
                initial_dir: initial_dir_for_dialog,
            });
    }
}

impl PlatformEventHandler for MyAppLogic {
    fn try_dequeue_command(&mut self) -> Option<PlatformCommand> {
        self.synchronous_command_queue.pop_front()
    }

    fn handle_event(&mut self, event: AppEvent) {
        log::trace!("AppLogic: Handling event: {:?}", event);
        match event {
            AppEvent::WindowCloseRequestedByUser { window_id } => {
                self.handle_window_close_requested(window_id);
            }
            AppEvent::WindowDestroyed { window_id } => {
                self.handle_window_destroyed(window_id);
            }
            AppEvent::TreeViewItemToggledByUser {
                window_id,
                item_id,
                new_state,
            } => {
                self.handle_treeview_item_toggled(window_id, item_id, new_state);
            }
            AppEvent::ButtonClicked {
                window_id,
                control_id,
            } => {
                self.handle_button_clicked(window_id, control_id);
            }
            AppEvent::MenuActionClicked { action } => match action {
                MenuAction::LoadProfile => self.handle_menu_load_profile_clicked(),
                MenuAction::SaveProfileAs => self.handle_menu_save_profile_as_clicked(),
                MenuAction::SetArchivePath => self.handle_menu_set_archive_path_clicked(),
                MenuAction::RefreshFileList => self.handle_menu_refresh_file_list_clicked(),
                MenuAction::GenerateArchive => self._do_generate_archive(),
            },
            AppEvent::FileOpenProfileDialogCompleted { window_id, result } => {
                self.handle_file_open_dialog_completed(window_id, result);
            }
            AppEvent::FileSaveDialogCompleted { window_id, result } => {
                self.handle_file_save_dialog_completed(window_id, result);
            }
            AppEvent::WindowResized {
                window_id,
                width,
                height,
            } => {
                self.handle_window_resized(window_id, width, height);
            }
            AppEvent::ProfileSelectionDialogCompleted {
                window_id,
                chosen_profile_name,
                create_new_requested,
                user_cancelled,
            } => {
                self.handle_profile_selection_dialog_completed(
                    window_id,
                    chosen_profile_name,
                    create_new_requested,
                    user_cancelled,
                );
            }
            AppEvent::GenericInputDialogCompleted {
                window_id,
                text,
                context_tag,
            } => {
                self.handle_input_dialog_completed(window_id, text, context_tag);
            }
            AppEvent::FolderPickerDialogCompleted { window_id, path } => {
                self.handle_folder_picker_dialog_completed(window_id, path);
            }
            AppEvent::MainWindowUISetupComplete { window_id } => {
                self._on_ui_setup_complete(window_id);
            }
        }
    }

    fn on_quit(&mut self) {
        log::debug!("AppLogic: on_quit called by platform. Application is exiting.");
        let profile_runtime_data = self.app_session_data_ops.lock().unwrap();

        let active_profile_name_opt = profile_runtime_data.get_profile_name();
        if let Some(active_profile_name) = active_profile_name_opt.as_ref() {
            if !active_profile_name.is_empty() {
                let profile_to_save = profile_runtime_data.create_profile_snapshot();
                log::debug!(
                    "AppLogic: Attempting to save content of active profile '{}' on exit.",
                    active_profile_name
                );
                match self
                    .profile_manager
                    .save_profile(&profile_to_save, APP_NAME_FOR_PROFILES)
                {
                    Ok(_) => log::debug!(
                        "AppLogic: Successfully saved content of profile '{}' to disk on exit.",
                        active_profile_name
                    ),
                    Err(e) => log::error!(
                        "AppLogic: Error saving content of profile '{}' on exit: {:?}",
                        active_profile_name,
                        e
                    ),
                }
            }
        }

        let profile_name_to_save_in_config = active_profile_name_opt.as_deref().unwrap_or("");
        log::debug!(
            "AppLogic: Attempting to save last profile name '{}' to config on exit.",
            profile_name_to_save_in_config
        );
        // Release lock on `data` before calling config_manager which might involve I/O
        drop(profile_runtime_data);

        match self
            .config_manager
            .save_last_profile_name(APP_NAME_FOR_PROFILES, profile_name_to_save_in_config)
        {
            Ok(_) => {
                if profile_name_to_save_in_config.is_empty() {
                    log::debug!(
                        "AppLogic: Successfully cleared/unset last profile name in config on exit."
                    );
                } else {
                    log::debug!(
                        "AppLogic: Successfully saved last active profile name '{}' to config on exit.",
                        profile_name_to_save_in_config
                    );
                }
            }
            Err(e) => log::error!(
                "AppLogic: Error saving last profile name to config on exit: {:?}",
                e
            ),
        }
    }

    /*
     * Queries if a specific tree item should display the "New" indicator.
     * For files, this means the file's state is `SelectionState::New`.
     * For folders, this means the folder itself or any of its descendant files
     * are in the `SelectionState::New` state.
     * It looks up the path for the item_id, then queries ProfileRuntimeDataOperations.
     */
    fn is_tree_item_new(&self, window_id: WindowId, item_id: TreeItemId) -> bool {
        let ui_state = match &self.ui_state {
            Some(s) if s.window_id == window_id => s,
            _ => {
                log::trace!(
                    "is_tree_item_new: UI state not found or window ID mismatch for {:?}. Returning false.",
                    window_id
                );
                return false;
            }
        };

        let path_opt = ui_state
            .path_to_tree_item_id
            .iter()
            .find(|(_path_candidate, id_in_map)| **id_in_map == item_id)
            .map(|(path_key, _id_value)| path_key);

        let path = match path_opt {
            Some(p) => p,
            None => {
                log::trace!(
                    "is_tree_item_new: Path not found for TreeItemId {:?}. Returning false.",
                    item_id
                );
                return false;
            }
        };

        let app_data_guard = self.app_session_data_ops.lock().unwrap();
        match app_data_guard.get_node_attributes_for_path(path) {
            Some((file_state, is_dir)) => {
                if is_dir {
                    // For directories, check if it or any descendant contains a new file
                    let contains_new =
                        app_data_guard.does_path_or_descendants_contain_new_file(path);
                    log::debug!(
                        "is_tree_item_new: Directory {:?} (ItemID {:?}) contains new file: {}.",
                        path,
                        item_id,
                        contains_new
                    );
                    contains_new
                } else {
                    // For files, check its own state
                    let is_new_file = file_state == SelectionState::New;
                    log::debug!(
                        "is_tree_item_new: File {:?} (ItemID {:?}) is new: {}.",
                        path,
                        item_id,
                        is_new_file
                    );
                    is_new_file
                }
            }
            None => {
                log::trace!(
                    "is_tree_item_new: FileNode attributes not found for path {:?}. Returning false.",
                    path
                );
                false
            }
        }
    }
}

// The purpose of these test helpers is to allow testing the internal state of MyAppLogic
#[cfg(test)]
impl MyAppLogic {
    pub(crate) fn test_set_main_window_id_and_init_ui_state(&mut self, id: WindowId) {
        self.ui_state = Some(MainWindowUiState::new(id));
    }
    pub(crate) fn test_pending_action(&self) -> Option<&PendingAction> {
        self.ui_state
            .as_ref()
            .and_then(|s| s.pending_action.as_ref())
    }
    pub(crate) fn test_set_pending_action(&mut self, v: PendingAction) {
        self.ui_state.as_mut().unwrap().pending_action = Some(v);
    }

    pub(crate) fn test_drain_commands(&mut self) -> Vec<PlatformCommand> {
        self.synchronous_command_queue.drain(..).collect()
    }

    pub(crate) fn test_set_path_to_tree_item_id_mapping(&mut self, path: PathBuf, id: TreeItemId) {
        if let Some(ui_state) = &mut self.ui_state {
            log::debug!(
                "Test helper: Mapping path {:?} to TreeItemId {:?}",
                path,
                id
            );
            ui_state.path_to_tree_item_id.insert(path, id);
        } else {
            panic!(
                "ui_state not initialized in test_set_path_to_tree_item_id_mapping. Call test_set_main_window_id_and_init_ui_state first."
            );
        }
    }
}
