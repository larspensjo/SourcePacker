use crate::core::{
    self, AppSessionData, ArchiveStatus, ArchiverOperations, ConfigError, ConfigManagerOperations,
    FileNode, FileState, FileSystemScannerOperations, Profile, ProfileError,
    ProfileManagerOperations, StateManagerOperations, TokenCounterOperations,
};
use crate::platform_layer::{
    AppEvent, CheckState, MessageSeverity, PlatformCommand, PlatformEventHandler,
    TreeItemDescriptor, TreeItemId, WindowId, types::MenuAction,
};
// Import MainWindowUiState, which we'll hold as an Option
use crate::app_logic::{MainWindowUiState, ui_constants};

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Import log macros
use log::{debug, error, info, trace, warn};

pub const ID_BUTTON_GENERATE_ARCHIVE_LOGIC: i32 = 1002;
pub(crate) const APP_NAME_FOR_PROFILES: &str = "SourcePacker";

// These type aliases are used by MainWindowUiState.
// If MainWindowUiState were in a different crate or a deeply nested module
// without easy access to `super::handler`, these might need to be defined
// in a more central location (e.g., `crate::app_logic::types`).
// For now, `main_window_ui_state.rs` imports them using `super::handler::`.
pub(crate) type PathToTreeItemIdMap = HashMap<PathBuf, TreeItemId>;

#[derive(Debug, PartialEq, Clone)] // Added Clone for use in MockStateManager test assertions
pub(crate) enum PendingAction {
    SavingArchive,
    SavingProfile,
    CreatingNewProfileGetName,
    CreatingNewProfileGetRoot,
    SettingArchivePath,
}

// --- Status Message Macros ---
macro_rules! status_message {
    ($self:expr, $severity:expr, $log_macro:ident, $($arg:tt)*) => {{
        let text = format!($($arg)*);
        // Log using the standard `log` crate
        $log_macro!("AppLogic Status: {}", text);

        // Update UI status bar
        if let Some(ui_state_ref) = &$self.ui_state {
            $self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: ui_state_ref.window_id,
                    label_id: ui_constants::STATUS_LABEL_GENERAL_ID,
                    text: text,
                    severity: $severity,
                });
        } else {
            // Fallback if no window_id (e.g., log to console/logger, or simply do nothing if UI update is impossible)
            // The message is already logged by $log_macro!, so no need for eprintln here for status.
        }
    }};
}

// Specific severity macros, now also call the corresponding `log` macro.
macro_rules! app_info { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Information, info, $($arg)*) }; }
macro_rules! app_error { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Error, error, $($arg)*) }; }
macro_rules! app_warn { ($self:expr, $($arg:tt)*) => { status_message!($self, MessageSeverity::Warning, warn, $($arg)*) }; }

/*
 * Manages the core application orchestration and UI logic in a platform-agnostic manner.
 * It processes UI events, interacts with core services (config, profiles, file system),
 * and commands the platform layer to update the UI. It holds `AppSessionData` for
 * core application state and `MainWindowUiState` (when a window exists) for UI-specific state.
 * Logging of its operations is done via the `log` crate.
 */
pub struct MyAppLogic {
    // Core application data
    app_session_data: AppSessionData,
    // UI-specific state for the main window, present only when the window exists.
    ui_state: Option<MainWindowUiState>,

    // Dependencies (Managers and Services)
    config_manager: Arc<dyn ConfigManagerOperations>,
    profile_manager: Arc<dyn ProfileManagerOperations>,
    file_system_scanner: Arc<dyn FileSystemScannerOperations>,
    archiver: Arc<dyn ArchiverOperations>,
    token_counter_manager: Arc<dyn TokenCounterOperations>,
    state_manager: Arc<dyn StateManagerOperations>,
    synchronous_command_queue: VecDeque<PlatformCommand>,
}

impl MyAppLogic {
    /*
     * Initializes a new instance of the application logic.
     * Requires implementations for `ConfigManagerOperations`, `ProfileManagerOperations`,
     * `FileSystemScannerOperations`, `ArchiverOperations`, and `StateManagerOperations`.
     * Initializes `AppSessionData` with defaults and sets `MainWindowUiState` to `None`
     * as the window is not yet created.
     */
    pub fn new(
        config_manager: Arc<dyn ConfigManagerOperations>,
        profile_manager: Arc<dyn ProfileManagerOperations>,
        file_system_scanner: Arc<dyn FileSystemScannerOperations>,
        archiver: Arc<dyn ArchiverOperations>,
        token_counter: Arc<dyn TokenCounterOperations>,
        state_manager: Arc<dyn StateManagerOperations>,
    ) -> Self {
        log::debug!("MyAppLogic::new called.");
        MyAppLogic {
            app_session_data: AppSessionData::new(), // Initialize AppSessionData
            ui_state: None,                          // MainWindowUiState is None initially
            config_manager,
            profile_manager,
            file_system_scanner,
            archiver,
            token_counter_manager: token_counter,
            state_manager,
            synchronous_command_queue: VecDeque::new(),
        }
    }

    // This helper remains static for now. It's called by the above method.
    fn build_tree_item_descriptors_recursive_internal(
        nodes: &[FileNode],
        path_to_tree_item_id: &mut PathToTreeItemIdMap, // Param
        next_tree_item_id_counter: &mut u64,            // Param
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
        self.ui_state = Some(MainWindowUiState::new(window_id)); // Instantiate MainWindowUiState

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
                // Original code used .expect(), implies this should not happen if logic is correct.
                // Logging an error and returning is a graceful way to handle if it does.
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

        let descriptors = Self::build_tree_item_descriptors_recursive_internal(
            &self.app_session_data.file_nodes_cache,
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
     * This function provides plain English descriptions for each status variant,
     * making them more understandable in the UI. For I/O errors, it also
     * attempts to describe common `io::ErrorKind` values.
     */
    fn archive_status_to_plain_string(status: &ArchiveStatus) -> String {
        match status {
            ArchiveStatus::UpToDate => "Up to date.".to_string(),
            ArchiveStatus::NotYetGenerated => "Not yet generated.".to_string(),
            ArchiveStatus::OutdatedRequiresUpdate => "Out of date.".to_string(),
            ArchiveStatus::ArchiveFileMissing => "File missing.".to_string(), // This case is usually caught before check_archive_status
            ArchiveStatus::NoFilesSelected => "No files selected.".to_string(),
            ArchiveStatus::ErrorChecking(Some(kind)) => {
                let kind_str = match kind {
                    io::ErrorKind::NotFound => "file not found",
                    io::ErrorKind::PermissionDenied => "permission denied",
                    io::ErrorKind::ConnectionRefused => "connection refused",
                    io::ErrorKind::ConnectionReset => "connection reset",
                    io::ErrorKind::ConnectionAborted => "connection aborted",
                    io::ErrorKind::NotConnected => "not connected",
                    io::ErrorKind::AddrInUse => "address in use",
                    io::ErrorKind::AddrNotAvailable => "address not available",
                    io::ErrorKind::BrokenPipe => "broken pipe",
                    io::ErrorKind::AlreadyExists => "already exists",
                    io::ErrorKind::WouldBlock => "operation would block",
                    io::ErrorKind::InvalidInput => "invalid input",
                    io::ErrorKind::InvalidData => "invalid data",
                    io::ErrorKind::TimedOut => "timed out",
                    io::ErrorKind::WriteZero => "write zero",
                    io::ErrorKind::Interrupted => "interrupted",
                    io::ErrorKind::Unsupported => "unsupported operation",
                    io::ErrorKind::UnexpectedEof => "unexpected end of file",
                    io::ErrorKind::OutOfMemory => "out of memory",
                    _ => "an I/O error occurred",
                };
                format!("Error: {}.", kind_str)
            }
            ArchiveStatus::ErrorChecking(None) => "Error: Unknown.".to_string(),
        }
    }

    pub(crate) fn update_current_archive_status(&mut self) {
        let ui_state_mut = match self.ui_state.as_mut() {
            Some(s) => s,
            None => {
                log::error!(
                    "AppLogic: update_current_archive_status called but no UI state. Status cannot be cached or displayed."
                );
                return;
            }
        };

        let main_window_id = ui_state_mut.window_id; // Get window_id for commands

        let profile = match &self.app_session_data.current_profile_cache {
            Some(p) => p,
            None => {
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
        };

        let status = self
            .archiver
            .check_archive_status(profile, &self.app_session_data.file_nodes_cache);

        ui_state_mut.current_archive_status_for_ui = Some(status.clone());

        let plain_status_string = Self::archive_status_to_plain_string(&status);
        let archive_label_text = format!("Archive: {}", plain_status_string);

        let severity_for_archive_msg = match status {
            ArchiveStatus::ErrorChecking(_) => MessageSeverity::Error,
            _ => MessageSeverity::Information,
        };

        // Update the DEDICATED archive label with the plain English string
        self.synchronous_command_queue
            .push_back(PlatformCommand::UpdateLabelText {
                window_id: main_window_id,
                label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                text: archive_label_text,
                severity: severity_for_archive_msg,
            });

        // If it's an error, also send it to the general status message system.
        // The general status message (via app_error!) will use the Debug representation
        // of the status for more detailed logging, while the dedicated label uses plain English.
        if severity_for_archive_msg == MessageSeverity::Error {
            app_error!(self, "Archive status error: {:?}", status);
        } else {
            log::debug!(
                "AppLogic UpdateArchiveStatus (not an error): {}",
                plain_status_string
            );
        }
    }

    // This static helper remains unchanged as it operates on passed-in data.
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

    // This static helper remains unchanged.
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

    // Relies on path_to_tree_item_id from ui_state.
    pub(crate) fn collect_visual_updates_recursive(
        &self,
        node: &FileNode,
        updates: &mut Vec<(TreeItemId, CheckState)>,
    ) {
        let ui_state_ref = match self.ui_state.as_ref() {
            Some(ui_state) => ui_state,
            None => {
                log::warn!(
                    "AppLogic: collect_visual_updates_recursive called but no UI state. Cannot get path_to_tree_item_id."
                );
                return;
            }
        };

        if let Some(item_id) = ui_state_ref.path_to_tree_item_id.get(&node.path) {
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
            log::error!(
                "AppLogic: Could not find TreeItemId for path {:?} during visual update collection.",
                node.path
            );
        }
    }

    /*
     * Recalculates the estimated token count for all currently selected files and
     * requests the UI to display this count.
     * It updates the old status bar, the new general status label (via `app_info!`),
     * and the new dedicated token count label.
     */
    pub(crate) fn _update_token_count_and_request_display(&mut self) {
        let token_count = self
            .app_session_data
            .update_token_count(&*self.token_counter_manager);

        let token_text = format!("Tokens: {}", token_count);

        app_info!(self, "{}", token_text);

        // Additionally, send to the NEW DEDICATED token label
        if let Some(ui_state_ref) = &self.ui_state {
            self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: ui_state_ref.window_id,
                    label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
                    text: token_text.clone(), // Clone because app_info! also uses token_text
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
            // Not the main window, or no UI state. This specific handler might only care about main window.
            // If other windows could exist and be closed, this logic might need adjustment.
            // For now, assume it's for the main window.
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
                self.ui_state.as_ref().unwrap().window_id // Safe due to check
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
        new_state: CheckState,
    ) {
        let ui_state_ref = match self.ui_state.as_ref() {
            Some(s) if s.window_id == window_id => s,
            _ => {
                // Event is for a window not managed by self.ui_state or ui_state is None.
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
            new_state
        );

        let mut path_of_toggled_node: Option<PathBuf> = None;
        for (path_candidate, id_in_map) in &ui_state_ref.path_to_tree_item_id {
            if *id_in_map == item_id {
                path_of_toggled_node = Some(path_candidate.clone());
                break;
            }
        }

        let path_for_model_update = match path_of_toggled_node {
            Some(p) => p,
            None => {
                log::error!(
                    "AppLogic: Could not find path for TreeItemId {:?} from UI event.",
                    item_id
                );
                return;
            }
        };

        {
            // Scope for mutable borrow of app_session_data.file_nodes_cache
            let node_to_update_model_for = Self::find_filenode_mut(
                &mut self.app_session_data.file_nodes_cache,
                &path_for_model_update,
            );

            if let Some(node_model) = node_to_update_model_for {
                let new_model_file_state = match new_state {
                    CheckState::Checked => FileState::Selected,
                    CheckState::Unchecked => FileState::Deselected,
                };
                self.state_manager
                    .update_folder_selection(node_model, new_model_file_state);
            } else {
                log::error!(
                    "AppLogic: Model node not found for path {:?} to update state.",
                    path_for_model_update
                );
                // Continue to update UI based on other states if necessary, or return
            }
        } // End scope for mutable borrow

        let root_node_for_visual_update = match Self::find_filenode_ref(
            &self.app_session_data.file_nodes_cache,
            &path_for_model_update,
        ) {
            Some(node) => node,
            None => {
                log::error!(
                    "AppLogic: Model node not found for path {:?} to collect visual updates.",
                    path_for_model_update
                );
                // Attempt to update global status even if this specific node isn't found for visual update propagation
                self.update_current_archive_status();
                self._update_token_count_and_request_display();
                return;
            }
        };

        let mut visual_updates_list = Vec::new();
        self.collect_visual_updates_recursive(
            root_node_for_visual_update,
            &mut visual_updates_list,
        );

        log::debug!(
            "Requesting {} visual updates for TreeView after toggle.",
            visual_updates_list.len()
        );
        for (id_to_update_ui, state_for_ui) in visual_updates_list {
            self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateTreeItemVisualState {
                    window_id,
                    item_id: id_to_update_ui,
                    new_state: state_for_ui,
                });
        }

        self.update_current_archive_status();
        self._update_token_count_and_request_display();
    }

    fn _do_generate_archive(&mut self) {
        if self.ui_state.is_none() {
            // This is already an early-out pattern.
            log::error!("Cannot generate archive: No UI state (main window).");
            return;
        }
        log::debug!("'Save to Archive' (via menu or old button) triggered.");

        let profile = match &self.app_session_data.current_profile_cache {
            Some(p) => p,
            None => {
                app_error!(self, "No profile loaded. Cannot save archive.");
                return;
            }
        };

        let archive_path = match &profile.archive_path {
            Some(ap) => ap,
            None => {
                app_error!(
                    self,
                    "No archive path set for current profile. Cannot save archive."
                );
                return;
            }
        };

        let display_root_path = profile.root_folder.clone();
        match self
            .archiver
            .create_archive_content(&self.app_session_data.file_nodes_cache, &display_root_path)
        {
            Ok(content) => match self.archiver.save_archive_content(archive_path, &content) {
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
        // This method is currently minimal. If it grows, early-outs for window_id match
        // or control_id checks could be useful. For now, it's simple.
        log::warn!(
            "Button with control_id {} clicked on window {:?}, but no specific action is currently mapped to it.",
            control_id,
            window_id
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
                    loaded_profile.name
                );

                self.app_session_data.current_profile_name = Some(loaded_profile.name.clone());
                self.app_session_data.root_path_for_scan = loaded_profile.root_folder.clone();

                if let Err(e) = self
                    .config_manager
                    .save_last_profile_name(APP_NAME_FOR_PROFILES, &loaded_profile.name)
                {
                    app_warn!(
                        self,
                        "Failed to save last profile name '{}': {:?}",
                        loaded_profile.name,
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
                self.app_session_data.current_profile_name = None;
                self.app_session_data.current_profile_cache = None;
                if let Some(ui_state_mut) = self.ui_state.as_mut() {
                    // Safe to check again after error
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
            .app_session_data
            .current_profile_name
            .as_ref()
            .map_or_else(|| "new_profile".to_string(), |name| name.clone());
        let sanitized_current_name = core::profiles::sanitize_profile_name(&base_name);
        let default_filename = format!("{}.json", sanitized_current_name);

        ui_state_mut.pending_action = Some(PendingAction::SavingProfile);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowSaveFileDialog {
                window_id: ui_state_mut.window_id,
                title: "Save Profile As".to_string(),
                default_filename,
                filter_spec: "Profile Files (*.json)\0*.json\0\0".to_string(),
                initial_dir: profile_dir_opt,
            });
    }

    fn handle_file_save_dialog_completed(&mut self, window_id: WindowId, result: Option<PathBuf>) {
        let ui_state_mut_option = self.ui_state.as_mut().filter(|s| s.window_id == window_id);
        let ui_state_mut = match ui_state_mut_option {
            Some(s) => s,
            None => {
                log::warn!(
                    "FileSaveDialogCompleted for an unknown or non-main window (ID: {:?}). Ignoring.",
                    window_id
                );
                return;
            }
        };

        let action = ui_state_mut.pending_action.take();
        match action {
            Some(PendingAction::SettingArchivePath) => {
                let path = match result {
                    Some(p) => p,
                    None => {
                        log::debug!("Set archive path cancelled.");
                        self._update_generate_archive_menu_item_state(window_id); // window_id is from event
                        return;
                    }
                };

                log::debug!("Archive path selected: {:?}", path);
                let profile = match &mut self.app_session_data.current_profile_cache {
                    Some(p) => p,
                    None => {
                        app_error!(self, "No profile active to set archive path for.");
                        return;
                    }
                };

                profile.archive_path = Some(path.clone());
                match self
                    .profile_manager
                    .save_profile(profile, APP_NAME_FOR_PROFILES)
                {
                    Ok(_) => {
                        app_info!(
                            self,
                            "Archive path set to '{}' for profile '{}' and saved.",
                            path.display(),
                            profile.name
                        );
                        self._update_window_title_with_profile_and_archive(window_id);
                        self.update_current_archive_status();
                        self._update_generate_archive_menu_item_state(window_id);
                    }
                    Err(e) => {
                        app_error!(
                            self,
                            "Failed to save profile '{}' after setting archive path: {}",
                            profile.name,
                            e
                        );
                    }
                }
            }
            Some(PendingAction::SavingArchive) => {
                app_warn!(
                    self,
                    "Obsolete 'SavingArchive' action handled. This should not happen."
                );
                if result.is_none() {
                    log::debug!("Save archive (obsolete path) cancelled.");
                }
            }
            Some(PendingAction::SavingProfile) => {
                let profile_save_path = match result {
                    Some(psp) => psp,
                    None => {
                        log::debug!("Save profile cancelled.");
                        return;
                    }
                };

                log::debug!("Profile save path selected: {:?}", profile_save_path);
                let profile_name_osstr = match profile_save_path.file_stem() {
                    Some(name) => name,
                    None => {
                        app_error!(
                            self,
                            "Could not extract profile name from save path. Profile not saved."
                        );
                        return;
                    }
                };

                let profile_name_str = match profile_name_osstr.to_str().map(|s| s.to_string()) {
                    Some(name_str) => name_str,
                    None => {
                        app_error!(
                            self,
                            "Profile save filename stem not valid UTF-8. Profile not saved."
                        );
                        return;
                    }
                };

                if profile_name_str.trim().is_empty()
                    || !profile_name_str
                        .chars()
                        .all(core::profiles::is_valid_profile_name_char)
                {
                    app_error!(
                        self,
                        "Invalid profile name extracted from path: '{}'. Profile not saved.",
                        profile_name_str
                    );
                    return;
                }

                let new_profile = self.app_session_data.create_profile_from_session_state(
                    profile_name_str.clone(),
                    &*self.token_counter_manager,
                );
                let profile_name_clone = new_profile.name.clone();
                match self
                    .profile_manager
                    .save_profile(&new_profile, APP_NAME_FOR_PROFILES)
                {
                    Ok(()) => {
                        log::debug!(
                            "Successfully saved profile as '{}' via manager.",
                            new_profile.name
                        );
                        self.app_session_data.current_profile_name = Some(new_profile.name.clone());
                        self.app_session_data.current_profile_cache = Some(new_profile.clone());
                        self.app_session_data.root_path_for_scan = self
                            .app_session_data
                            .current_profile_cache
                            .as_ref()
                            .unwrap()
                            .root_folder
                            .clone();

                        self._update_window_title_with_profile_and_archive(window_id);

                        if let Err(e) = self
                            .config_manager
                            .save_last_profile_name(APP_NAME_FOR_PROFILES, &new_profile.name)
                        {
                            app_warn!(
                                self,
                                "Failed to save last profile name '{}': {:?}",
                                new_profile.name,
                                e
                            );
                        }
                        self.update_current_archive_status();
                        self._update_generate_archive_menu_item_state(window_id);
                        app_info!(self, "Profile '{}' saved.", profile_name_clone);
                    }
                    Err(e) => {
                        app_error!(
                            self,
                            "Failed to save profile (via manager) as '{}': {}",
                            new_profile.name,
                            e
                        );
                    }
                }
            }
            Some(PendingAction::CreatingNewProfileGetName)
            | Some(PendingAction::CreatingNewProfileGetRoot) => {
                app_error!(
                    self,
                    "Unexpected FileSaveDialogCompleted with pending action: {:?}",
                    action
                );
            }
            None => {
                app_warn!(
                    self,
                    "FileSaveDialogCompleted received but no pending action was set."
                );
            }
        }
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

        let current_profile_clone = match self.app_session_data.current_profile_cache.clone() {
            Some(profile) => profile,
            None => {
                app_warn!(self, "Refresh: No profile active.");
                return;
            }
        };

        let root_path_to_scan = current_profile_clone.root_folder.clone();
        log::debug!(
            "Refreshing file list for profile '{}', root: {:?}",
            current_profile_clone.name,
            root_path_to_scan
        );

        match self.file_system_scanner.scan_directory(&root_path_to_scan) {
            Ok(new_nodes) => {
                self.app_session_data.file_nodes_cache = new_nodes;
                log::debug!(
                    "Scan successful, {} top-level nodes found.",
                    self.app_session_data.file_nodes_cache.len()
                );

                self.state_manager.apply_profile_to_tree(
                    &mut self.app_session_data.file_nodes_cache,
                    &current_profile_clone,
                );
                log::debug!(
                    "Applied profile '{}' to refreshed tree.",
                    current_profile_clone.name
                );

                self.refresh_tree_view_from_cache(main_window_id);
                self.update_current_archive_status();
                self._update_token_count_and_request_display();
                app_info!(
                    self,
                    "File list refreshed for profile '{}'.",
                    current_profile_clone.name
                );
            }
            Err(e) => {
                app_error!(self, "Failed to refresh file list: {}", e);
            }
        }
    }

    /*
     * Activates a given profile: sets it as current in `AppSessionData`, scans its root folder,
     * applies its selection state, refreshes UI elements, and shows the window.
     * Assumes `self.ui_state` is Some and `window_id` matches `self.ui_state.window_id`.
     */
    fn _activate_profile_and_show_window(
        &mut self,
        window_id: WindowId, // This is the main window's ID, confirmed by caller
        profile_to_activate: Profile,
        initial_operation_status_message: String,
    ) {
        // This assert remains valuable.
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "Mismatched window ID or no UI state for _activate_profile_and_show_window"
        );

        let scan_result = self.app_session_data.activate_and_populate_data(
            profile_to_activate,
            &*self.file_system_scanner,
            &*self.state_manager,
            &*self.token_counter_manager,
        );

        let (scan_was_successful, final_status_message) = match scan_result {
            Ok(_) => (true, initial_operation_status_message),
            Err(scan_error_message) => (false, scan_error_message),
        };

        self._update_window_title_with_profile_and_archive(window_id);

        {
            // Scope for ui_state_mut borrow
            let ui_state_mut = self
                .ui_state
                .as_mut()
                .expect("UI state must exist here for _activate_profile_and_show_window");
            ui_state_mut.next_tree_item_id_counter = 1;
            ui_state_mut.path_to_tree_item_id.clear();

            let descriptors = Self::build_tree_item_descriptors_recursive_internal(
                &self.app_session_data.file_nodes_cache,
                &mut ui_state_mut.path_to_tree_item_id,
                &mut ui_state_mut.next_tree_item_id_counter,
            );
            self.synchronous_command_queue
                .push_back(PlatformCommand::PopulateTreeView {
                    window_id,
                    items: descriptors,
                });
        }

        self.update_current_archive_status(); // This will queue archive status updates

        let token_count_for_display = self.app_session_data.cached_current_token_count;
        let token_text = format!("Tokens: {}", token_count_for_display);

        app_info!(self, "{}", token_text);

        // Additionally, send to the NEW DEDICATED token label
        // This was missing and is added now.
        if let Some(ui_state_ref) = &self.ui_state {
            // ui_state definitely Some here
            self.synchronous_command_queue
                .push_back(PlatformCommand::UpdateLabelText {
                    window_id: ui_state_ref.window_id,
                    label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
                    text: token_text.clone(), // Clone because app_info! also uses token_text
                    severity: MessageSeverity::Information,
                });
        }

        if scan_was_successful {
            app_info!(self, "{}", final_status_message);
        } else {
            app_error!(self, "{}", final_status_message);
        }

        self._update_generate_archive_menu_item_state(window_id);
        self.synchronous_command_queue
            .push_back(PlatformCommand::ShowWindow { window_id });
    }

    // Assumes `self.ui_state` is Some and `window_id` matches `self.ui_state.window_id`.
    pub(crate) fn initiate_profile_selection_or_creation(&mut self, window_id: WindowId) {
        log::debug!("Initiating profile selection or creation flow.");
        // This assert remains valuable.
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
                // Not cancelled, not create_new, but no profile name. This implies an unexpected state.
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

    // Assumes `self.ui_state` is Some and `window_id` matches `self.ui_state.window_id`.
    fn start_new_profile_creation_flow(&mut self, window_id: WindowId) {
        log::debug!("Starting new profile creation flow (Step 1: Get Name).");
        // .expect() is used here as this function's contract assumes valid ui_state.
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
        window_id: WindowId, // This is the event's window_id
        text: Option<String>,
        context_tag: Option<String>,
    ) {
        // Ensure the event is for the main window managed by ui_state.
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
        // Now we know self.ui_state is Some and its window_id matches the event's window_id.

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
                        let ui_state_mut = self.ui_state.as_mut().unwrap(); // Safe due to initial check
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
                    // Log the warning.
                    app_warn!(
                        self,
                        "Invalid profile name. Please use only letters, numbers, spaces, underscores, or hyphens."
                    );
                    // Re-show dialog.
                    self.synchronous_command_queue
                        .push_back(PlatformCommand::ShowInputDialog {
                            window_id, // Use event's window_id
                            title: "New Profile (1/2): Name".to_string(),
                            prompt: "Enter a name for the new profile (invalid previous attempt):"
                                .to_string(),
                            default_text: Some(profile_name_text), // Pass back the invalid attempt
                            context_tag: Some("NewProfileName".to_string()),
                        });
                    // pending_action in ui_state should remain CreatingNewProfileGetName (was set by start_new_profile_creation_flow)
                    return;
                }

                log::debug!(
                    "New profile name '{}' is valid. Proceeding to Step 2 (Get Root Folder).",
                    profile_name_text
                );
                // Now, safely get ui_state_mut to modify it.
                let ui_state_mut = self.ui_state.as_mut().unwrap(); // Safe due to initial check
                ui_state_mut.pending_new_profile_name = Some(profile_name_text);
                ui_state_mut.pending_action = Some(PendingAction::CreatingNewProfileGetRoot);

                self.synchronous_command_queue
                    .push_back(PlatformCommand::ShowFolderPickerDialog {
                        window_id, // Use event's window_id
                        title: "New Profile (2/2): Select Root Folder".to_string(),
                        initial_dir: None,
                    });
            }
            _ => {
                // Log first, then mutate ui_state if necessary.
                app_warn!(
                    self,
                    "InputDialogCompleted with unhandled context: {:?}",
                    context_tag
                );
                let ui_state_mut = self.ui_state.as_mut().unwrap(); // Safe due to initial check
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
        // `ui_state_mut` is now a mutable reference to MainWindowUiState.

        log::debug!("FolderPickerDialogCompleted: path: {:?}", path);
        ui_state_mut.pending_action = None; // Clear regardless of path outcome for this action type.

        let root_folder_path = match path {
            Some(p) => p,
            None => {
                log::debug!("Root folder selection cancelled. Returning to profile selection.");
                ui_state_mut.pending_new_profile_name = None; // Clear pending name
                self.initiate_profile_selection_or_creation(window_id);
                return;
            }
        };

        let profile_name = match ui_state_mut.pending_new_profile_name.take() {
            // Take from ui_state_mut
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
        let new_profile = Profile::new(profile_name.clone(), root_folder_path.clone());

        match self
            .profile_manager
            .save_profile(&new_profile, APP_NAME_FOR_PROFILES)
        {
            Ok(_) => {
                log::debug!("Successfully saved new profile '{}'.", new_profile.name);
                let operation_status_message =
                    format!("New profile '{}' created and loaded.", new_profile.name);

                if let Err(e) = self
                    .config_manager
                    .save_last_profile_name(APP_NAME_FOR_PROFILES, &new_profile.name)
                {
                    app_warn!(
                        self,
                        "Failed to save last profile name '{}': {:?}",
                        new_profile.name,
                        e
                    );
                }
                self._activate_profile_and_show_window(
                    window_id,
                    new_profile,
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

    // Assumes `self.ui_state` is Some and `window_id` matches `self.ui_state.window_id`.
    fn _update_window_title_with_profile_and_archive(&mut self, window_id: WindowId) {
        assert!(
            self.ui_state
                .as_ref()
                .map_or(false, |s| s.window_id == window_id),
            "_update_window_title_with_profile_and_archive called with mismatching window ID or no UI state."
        );

        let title = MainWindowUiState::compose_window_title(&self.app_session_data);
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
            .app_session_data
            .current_profile_cache
            .as_ref()
            .and_then(|p| p.archive_path.as_ref())
            .is_some();

        // Currently, menu item enabled state is not dynamically managed by commands.
        // Logic is checked when action is triggered. This log indicates if action *can* succeed.
        if enabled {
            log::debug!("'Generate Archive' menu item can now function (archive path is set).");
        } else {
            log::debug!(
                "'Generate Archive' menu item functionality depends on archive path (currently not set)."
            );
        }
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
        if self.app_session_data.current_profile_cache.is_none() {
            app_warn!(self, "Cannot set archive path: No profile is active.");
            return;
        }

        ui_state_mut.pending_action = Some(PendingAction::SettingArchivePath);

        let default_filename = self
            .app_session_data
            .current_profile_cache
            .as_ref()
            .and_then(|p| p.archive_path.as_ref().and_then(|ap| ap.file_name()))
            .map(|os_name| os_name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.app_session_data
                    .current_profile_cache
                    .as_ref()
                    .map(|p| core::profiles::sanitize_profile_name(&p.name) + ".txt")
                    .unwrap_or_else(|| "archive.txt".to_string())
            });

        let initial_dir_for_dialog = self
            .app_session_data
            .current_profile_cache
            .as_ref()
            .and_then(|p| {
                p.archive_path
                    .as_ref()
                    .and_then(|ap| ap.parent().map(PathBuf::from))
            })
            .or_else(|| {
                self.app_session_data
                    .current_profile_cache
                    .as_ref()
                    .map(|p| p.root_folder.clone())
            });

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
            AppEvent::MenuActionClicked {
                window_id: _,
                action,
            } => {
                // window_id from menu events often not used if actions are global
                match action {
                    MenuAction::LoadProfile => self.handle_menu_load_profile_clicked(),
                    MenuAction::SaveProfileAs => self.handle_menu_save_profile_as_clicked(),
                    MenuAction::SetArchivePath => self.handle_menu_set_archive_path_clicked(),
                    MenuAction::RefreshFileList => self.handle_menu_refresh_file_list_clicked(),
                    MenuAction::GenerateArchive => self._do_generate_archive(),
                }
            }
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

        let active_profile_name_opt = self.app_session_data.current_profile_name.clone();
        if let Some(active_profile_name) = active_profile_name_opt {
            if !active_profile_name.is_empty() {
                let profile_to_save = self.app_session_data.create_profile_from_session_state(
                    active_profile_name.clone(),
                    &*self.token_counter_manager,
                );
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

        let profile_name_to_save_in_config = self
            .app_session_data
            .current_profile_name
            .as_deref()
            .unwrap_or("");
        log::debug!(
            "AppLogic: Attempting to save last profile name '{}' to config on exit.",
            profile_name_to_save_in_config
        );
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /*
     * Queries if a specific tree item is currently in the "New" state.
     * This implementation retrieves the `FileNode` associated with the given
     * `window_id` and `item_id` and checks its `state` field. If the item
     * cannot be found or its state is not `FileState::New`, it returns `false`.
     */
    fn is_tree_item_new(&self, window_id: WindowId, item_id: TreeItemId) -> bool {
        // Ensure ui_state exists and matches the window_id.
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

        // Find the path associated with the TreeItemId.
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

        // Find the FileNode for the path in the cache.
        match Self::find_filenode_ref(&self.app_session_data.file_nodes_cache, path) {
            Some(node) => {
                log::debug!(
                    "is_tree_item_new: Found FileNode for path {:?} with state {:?}.",
                    path,
                    node.state,
                );
                node.state == FileState::New && !node.is_dir // This is a hack for now, do not show directories as "new".
            }
            None => {
                log::trace!(
                    "is_tree_item_new: FileNode not found for path {:?}. Returning false.",
                    path
                );
                false
            }
        }
    }
}

#[cfg(test)]
impl MyAppLogic {
    // Accessors for AppSessionData fields
    pub(crate) fn test_app_session_data(&self) -> &AppSessionData {
        &self.app_session_data
    }
    pub(crate) fn test_app_session_data_mut(&mut self) -> &mut AppSessionData {
        &mut self.app_session_data
    }

    // Accessors for MainWindowUiState fields (via Option)
    pub(crate) fn test_ui_state(&self) -> &Option<MainWindowUiState> {
        &self.ui_state
    }
    pub(crate) fn test_ui_state_mut(&mut self) -> &mut Option<MainWindowUiState> {
        &mut self.ui_state
    }

    // Convenience getters for fields previously directly on MyAppLogic, now via new structs
    pub(crate) fn test_main_window_id(&self) -> Option<WindowId> {
        self.ui_state.as_ref().map(|s| s.window_id)
    }
    // For tests that need to set up UI state as if _on_ui_setup_complete happened
    pub(crate) fn test_set_main_window_id_and_init_ui_state(&mut self, id: WindowId) {
        self.ui_state = Some(MainWindowUiState::new(id));
    }
    // If a test specifically needs to set ui_state to None
    pub(crate) fn test_clear_ui_state(&mut self) {
        self.ui_state = None;
    }

    pub(crate) fn test_file_nodes_cache(&mut self) -> &mut Vec<FileNode> {
        &mut self.app_session_data.file_nodes_cache
    }
    pub(crate) fn test_set_file_nodes_cache(&mut self, v: Vec<FileNode>) {
        self.app_session_data.file_nodes_cache = v;
    }

    pub(crate) fn test_find_filenode_mut(&mut self, path_to_find: &Path) -> Option<&mut FileNode> {
        Self::find_filenode_mut(&mut self.app_session_data.file_nodes_cache, path_to_find)
    }

    // For path_to_tree_item_id, it's now in MainWindowUiState
    pub(crate) fn test_path_to_tree_item_id(&self) -> Option<&PathToTreeItemIdMap> {
        self.ui_state.as_ref().map(|s| &s.path_to_tree_item_id)
    }
    // Test utility to insert into path_to_tree_item_id, ensuring ui_state exists
    pub(crate) fn test_path_to_tree_item_id_insert(&mut self, path: &PathBuf, id: TreeItemId) {
        self.ui_state
            .as_mut()
            .unwrap()
            .path_to_tree_item_id
            .insert(path.to_path_buf(), id);
    }
    // Test utility to clear path_to_tree_item_id and reset counter
    pub(crate) fn test_path_to_tree_item_id_clear_and_reset_counter(&mut self) {
        if let Some(ui_state_mut) = self.ui_state.as_mut() {
            ui_state_mut.path_to_tree_item_id.clear();
            ui_state_mut.next_tree_item_id_counter = 1;
        }
    }

    pub(crate) fn test_root_path_for_scan(&self) -> &PathBuf {
        &self.app_session_data.root_path_for_scan
    }
    pub(crate) fn test_set_root_path_for_scan(&mut self, v: PathBuf) {
        self.app_session_data.root_path_for_scan = v;
    }

    pub(crate) fn test_current_profile_name(&self) -> &Option<String> {
        &self.app_session_data.current_profile_name
    }
    pub(crate) fn test_set_current_profile_name(&mut self, v: Option<String>) {
        self.app_session_data.current_profile_name = v;
    }

    pub(crate) fn test_current_profile_cache(&self) -> &Option<Profile> {
        &self.app_session_data.current_profile_cache
    }
    pub(crate) fn test_set_current_profile_cache(&mut self, v: Option<Profile>) {
        self.app_session_data.current_profile_cache = v;
    }

    pub(crate) fn test_current_archive_status_for_ui(&self) -> Option<&ArchiveStatus> {
        self.ui_state
            .as_ref()
            .and_then(|s| s.current_archive_status_for_ui.as_ref())
    }
    pub(crate) fn test_set_current_archive_status_for_ui(&mut self, v: Option<ArchiveStatus>) {
        if let Some(s) = self.ui_state.as_mut() {
            s.current_archive_status_for_ui = v;
        }
    }

    // Combined setter for profile data (session) and archive status (UI)
    pub(crate) fn test_set_current_profile_and_status(
        &mut self,
        name: Option<String>,
        cache: Option<Profile>,
        status_for_ui: Option<ArchiveStatus>,
    ) {
        self.app_session_data.current_profile_name = name;
        self.app_session_data.current_profile_cache = cache;
        if let Some(s) = self.ui_state.as_mut() {
            s.current_archive_status_for_ui = status_for_ui;
        } else if status_for_ui.is_some() {
            // This case might indicate a test setup issue if status is set without UI state
            warn!(
                "test_set_current_profile_and_status: Attempted to set UI status while ui_state is None."
            );
        }
    }

    pub(crate) fn test_pending_action(&self) -> Option<&PendingAction> {
        self.ui_state
            .as_ref()
            .and_then(|s| s.pending_action.as_ref())
    }
    pub(crate) fn test_set_pending_action(&mut self, v: PendingAction) {
        self.ui_state.as_mut().unwrap().pending_action = Some(v);
    }
    pub(crate) fn test_clear_pending_action(&mut self) {
        if let Some(s) = self.ui_state.as_mut() {
            s.pending_action = None;
        }
    }

    pub(crate) fn test_pending_new_profile_name(&self) -> Option<&String> {
        self.ui_state
            .as_ref()
            .and_then(|s| s.pending_new_profile_name.as_ref())
    }
    pub(crate) fn test_set_pending_new_profile_name(&mut self, v: Option<String>) {
        if let Some(s) = self.ui_state.as_mut() {
            s.pending_new_profile_name = v;
        }
    }

    pub(crate) fn test_config_manager(&self) -> &Arc<dyn ConfigManagerOperations> {
        &self.config_manager
    }

    pub(crate) fn test_drain_commands(&mut self) -> Vec<PlatformCommand> {
        self.synchronous_command_queue.drain(..).collect()
    }

    pub(crate) fn test_current_token_count(&self) -> usize {
        self.app_session_data.cached_current_token_count
    }
}
