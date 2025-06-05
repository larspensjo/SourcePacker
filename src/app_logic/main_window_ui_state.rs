/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions. It interacts with ProfileRuntimeDataOperations
 * to get necessary data for UI display.
 */
use crate::app_logic::ui_constants;
use crate::core::{ArchiveStatus, ProfileRuntimeDataOperations};
use crate::platform_layer::{MessageSeverity, PlatformCommand, TreeItemId, WindowId};
use std::collections::HashMap;
use std::path::PathBuf;

// These types are currently defined in `handler.rs`.
use super::handler::{PathToTreeItemIdMap, PendingAction};

/*
 * Holds UI-specific state for the main application window.
 * This struct consolidates data that is directly tied to the presentation
 * and interaction logic of the main window, separating it from the core
 * application data (accessed via `ProfileRuntimeDataOperations`) and the
 * central orchestrator (`MyAppLogic`).
 */
#[derive(Debug)]
pub struct MainWindowUiState {
    /* The unique identifier for the main application window. */
    pub window_id: WindowId,
    /* Maps file/directory paths to their corresponding TreeItemId in the UI's tree view. */
    pub path_to_tree_item_id: PathToTreeItemIdMap,
    /* A counter to generate unique TreeItemIds for the tree view. */
    pub next_tree_item_id_counter: u64,
    /* A cache of the current archive status, specifically for UI display purposes. */
    pub current_archive_status_for_ui: Option<ArchiveStatus>,
    /* Tracks any pending multi-step UI action, like saving a file or a dialog sequence. */
    pub pending_action: Option<PendingAction>,
    /* Stores a temporary profile name, typically used during a new profile creation flow. */
    pub pending_new_profile_name: Option<String>,
}

impl MainWindowUiState {
    /*
     * Creates a new `MainWindowUiState` instance for a given window.
     * Initializes with the provided `window_id`, an empty path-to-ID map,
     * a starting counter for tree item IDs, and no pending actions or statuses.
     */
    pub fn new(window_id: WindowId) -> Self {
        log::debug!(
            "MainWindowUiState::new called for window_id: {:?}",
            window_id
        );
        MainWindowUiState {
            window_id,
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
            current_archive_status_for_ui: None,
            pending_action: None,
            pending_new_profile_name: None,
        }
    }

    /*
     * Composes the main window title string based on the current application session data.
     * Includes the application name, current profile name (if any), and archive path status,
     * obtained via the `ProfileRuntimeDataOperations` trait.
     */
    pub fn compose_window_title(app_session_data_ops: &dyn ProfileRuntimeDataOperations) -> String {
        let mut title = "SourcePacker".to_string();
        if let Some(profile_name) = app_session_data_ops.get_profile_name() {
            title = format!("{} - [{}]", title, profile_name);
            if let Some(archive_path) = app_session_data_ops.get_archive_path() {
                title = format!("{} - [{}]", title, archive_path.display());
            } else {
                title = format!("{} - [No Archive Set]", title);
            }
        } else {
            title = format!("{} - [No Profile Loaded]", title);
        }
        title
    }

    /*
     * Builds a list of `PlatformCommand`s for initially displaying profile information.
     * This function generates commands reflecting a newly activated profile's state,
     * including window title, status labels (general, token, archive), and the
     * "Generate Archive" button state, using data from `ProfileRuntimeDataOperations`.
     */
    pub fn build_initial_profile_display_commands(
        &self,
        app_session_data_ops: &dyn ProfileRuntimeDataOperations,
        initial_status_message: String,
        scan_was_successful: bool,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        let window_id = self.window_id;

        // 1. Set Window Title
        let title = Self::compose_window_title(app_session_data_ops);
        commands.push(PlatformCommand::SetWindowTitle { window_id, title });

        // 2. Initial Status Message (General Label)
        commands.push(PlatformCommand::UpdateLabelText {
            window_id,
            label_id: ui_constants::STATUS_LABEL_GENERAL_ID,
            text: initial_status_message,
            severity: if scan_was_successful {
                MessageSeverity::Information
            } else {
                MessageSeverity::Error
            },
        });

        // 3. Token Count (Dedicated Token Label)
        // MyAppLogic will also send a "Token count updated" to general status label via app_info!
        let token_text = format!(
            "Tokens: {}",
            app_session_data_ops.get_cached_total_token_count()
        );
        commands.push(PlatformCommand::UpdateLabelText {
            window_id,
            label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
            text: token_text,
            severity: MessageSeverity::Information,
        });

        // 4. Archive Status (Dedicated Archive Label)
        // MyAppLogic will also send an error to general status label if archive status is an error.
        if app_session_data_ops.get_profile_name().is_some() {
            if let Some(status) = &self.current_archive_status_for_ui {
                let plain_status_string =
                    crate::app_logic::handler::MyAppLogic::archive_status_to_plain_string(status);
                let archive_label_text = format!("Archive: {}", plain_status_string);
                let archive_severity = if matches!(status, ArchiveStatus::ErrorChecking(_)) {
                    MessageSeverity::Error
                } else {
                    MessageSeverity::Information
                };
                commands.push(PlatformCommand::UpdateLabelText {
                    window_id,
                    label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                    text: archive_label_text.clone(),
                    severity: archive_severity,
                });
            } else {
                let unknown_archive_status_text = "Archive: Status pending...".to_string();
                commands.push(PlatformCommand::UpdateLabelText {
                    window_id,
                    label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                    text: unknown_archive_status_text,
                    severity: MessageSeverity::Information,
                });
            }
        } else {
            let no_profile_msg_archive_label = "Archive: No profile loaded".to_string();
            commands.push(PlatformCommand::UpdateLabelText {
                window_id,
                label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                text: no_profile_msg_archive_label.clone(),
                severity: MessageSeverity::Information,
            });
        }

        // 5. "Generate Archive" Button State (ID from handler.rs, might move later)
        let save_button_enabled = app_session_data_ops.get_archive_path().is_some();
        commands.push(PlatformCommand::SetControlEnabled {
            window_id,
            control_id: super::handler::ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
            enabled: save_button_enabled,
        });

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_logic::ui_constants;
    use crate::core::{
        ArchiveStatus, FileNode, FileState, FileSystemScannerOperations, Profile,
        ProfileRuntimeDataOperations, StateManagerOperations, TokenCounterOperations,
        models::FileTokenDetails,
    };
    use crate::platform_layer::WindowId;
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, RwLock}; // For MockProfileRuntimeDataOps

    // --- MockProfileRuntimeDataOperations for MainWindowUiState tests ---
    // This is a simplified mock, just enough for these tests.
    // A more complete mock might be shared if handler_tests also needs one.
    #[derive(Default)]
    struct MockProfileRuntimeDataOps {
        profile_name: Option<String>,
        archive_path: Option<PathBuf>,
        cached_total_token_count: usize,
        // We don't need to mock all methods, only those used by MainWindowUiState
    }

    impl ProfileRuntimeDataOperations for MockProfileRuntimeDataOps {
        fn get_profile_name(&self) -> Option<String> {
            self.profile_name.clone()
        }
        fn get_archive_path(&self) -> Option<PathBuf> {
            self.archive_path.clone()
        }
        fn get_cached_total_token_count(&self) -> usize {
            self.cached_total_token_count
        }

        // --- Unused methods for these specific tests, provide default/dummy implementations ---
        fn set_profile_name(&mut self, _name: Option<String>) {
            unimplemented!()
        }
        fn set_archive_path(&mut self, _path: Option<PathBuf>) {
            unimplemented!()
        }
        fn get_root_path_for_scan(&self) -> PathBuf {
            unimplemented!()
        }
        fn set_root_path_for_scan(&mut self, _path: PathBuf) {
            unimplemented!()
        }
        fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
            unimplemented!()
        }
        fn clear_snapshot_nodes(&mut self) {
            unimplemented!()
        }
        fn set_snapshot_nodes(&mut self, _nodes: Vec<FileNode>) {
            unimplemented!()
        }
        fn apply_selection_states_to_snapshot(
            &mut self,
            _state_manager: &dyn StateManagerOperations,
            _selected_paths: &HashSet<PathBuf>,
            _deselected_paths: &HashSet<PathBuf>,
        ) {
            unimplemented!()
        }
        fn get_node_attributes_for_path(&self, _path: &Path) -> Option<(FileState, bool)> {
            unimplemented!()
        }
        fn update_node_state_and_collect_changes(
            &mut self,
            _path: &Path,
            _new_state: FileState,
            _state_manager: &dyn StateManagerOperations,
        ) -> Vec<(PathBuf, FileState)> {
            unimplemented!()
        }
        fn get_cached_file_token_details(&self) -> HashMap<PathBuf, FileTokenDetails> {
            unimplemented!()
        }
        fn set_cached_file_token_details(&mut self, _details: HashMap<PathBuf, FileTokenDetails>) {
            unimplemented!()
        }
        fn update_total_token_count_for_selected_files(
            &mut self,
            _token_counter: &dyn TokenCounterOperations,
        ) -> usize {
            unimplemented!()
        }
        fn clear(&mut self) {
            unimplemented!()
        }
        fn create_profile_snapshot(&self) -> Profile {
            unimplemented!()
        }
        fn load_profile_into_session(
            &mut self,
            _loaded_profile: Profile,
            _file_system_scanner: &dyn FileSystemScannerOperations,
            _state_manager: &dyn StateManagerOperations,
            _token_counter: &dyn TokenCounterOperations,
        ) -> Result<(), String> {
            unimplemented!()
        }
        fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
            let mut selected = HashSet::new();
            let mut deselected = HashSet::new();
            return (selected, deselected);
        }
    }

    #[test]
    fn test_main_window_ui_state_new() {
        // Arrange
        crate::initialize_logging();
        let test_window_id = WindowId(42);

        // Act
        let ui_state = MainWindowUiState::new(test_window_id);

        // Assert
        assert_eq!(ui_state.window_id, test_window_id);
        assert!(ui_state.path_to_tree_item_id.is_empty());
        assert_eq!(ui_state.next_tree_item_id_counter, 1);
        assert!(ui_state.current_archive_status_for_ui.is_none());
        assert!(ui_state.pending_action.is_none());
        assert!(ui_state.pending_new_profile_name.is_none());
    }

    #[test]
    fn test_compose_window_title_with_mock_ops() {
        // Arrange
        crate::initialize_logging();
        let mut mock_ops = MockProfileRuntimeDataOps::default();

        // Case 1: No profile
        let title1 = MainWindowUiState::compose_window_title(&mock_ops);
        assert_eq!(title1, "SourcePacker - [No Profile Loaded]");

        // Case 2: Profile, no archive path
        mock_ops.profile_name = Some("MyProfile".to_string());
        let title2 = MainWindowUiState::compose_window_title(&mock_ops);
        assert_eq!(title2, "SourcePacker - [MyProfile] - [No Archive Set]");

        // Case 3: Profile and archive path
        mock_ops.archive_path = Some(PathBuf::from("/path/to/archive.zip"));
        let title3 = MainWindowUiState::compose_window_title(&mock_ops);
        assert_eq!(
            title3,
            "SourcePacker - [MyProfile] - [/path/to/archive.zip]"
        );
    }

    #[test]
    fn test_build_initial_profile_display_commands_generates_update_label_text() {
        // Arrange
        crate::initialize_logging();
        let window_id = WindowId(1);
        let mut ui_state = MainWindowUiState::new(window_id);

        let mut mock_app_session_ops = MockProfileRuntimeDataOps {
            profile_name: Some("TestProfile".to_string()),
            archive_path: Some(PathBuf::from("/root/archive.txt")),
            cached_total_token_count: 123,
        };
        ui_state.current_archive_status_for_ui = Some(ArchiveStatus::UpToDate);

        let initial_status_msg_text = "Profile loaded.".to_string();
        let token_msg_text = "Tokens: 123".to_string();
        let archive_msg_text_plain = "Archive: Up to date.".to_string();

        // Act
        let commands = ui_state.build_initial_profile_display_commands(
            &mock_app_session_ops, // Pass the mock ops
            initial_status_msg_text.clone(),
            true, // scan_was_successful
        );

        // Assert
        // (Assertions for command content remain largely the same as before,
        //  as they check the generated PlatformCommands)
        let mut general_initial_status_found = false;
        let mut dedicated_token_status_found = false;
        let mut dedicated_archive_status_found = false;

        for cmd in &commands {
            if let PlatformCommand::UpdateLabelText {
                label_id,
                text,
                severity,
                ..
            } = cmd
            {
                if *label_id == ui_constants::STATUS_LABEL_GENERAL_ID {
                    if text == &initial_status_msg_text && *severity == MessageSeverity::Information
                    {
                        general_initial_status_found = true;
                    }
                } else if *label_id == ui_constants::STATUS_LABEL_TOKENS_ID {
                    if text == &token_msg_text && *severity == MessageSeverity::Information {
                        dedicated_token_status_found = true;
                    }
                } else if *label_id == ui_constants::STATUS_LABEL_ARCHIVE_ID {
                    if text == &archive_msg_text_plain && *severity == MessageSeverity::Information
                    {
                        dedicated_archive_status_found = true;
                    }
                }
            }
        }

        assert!(
            general_initial_status_found,
            "Initial status message for general label not found or incorrect."
        );
        assert!(
            dedicated_token_status_found,
            "Token status message for dedicated token label not found or incorrect."
        );
        assert!(
            dedicated_archive_status_found,
            "Archive status for dedicated archive label not found or incorrect."
        );

        assert!(
            commands
                .iter()
                .any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { .. }))
        );
        assert!(commands.iter().any(|cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, enabled, .. }
            if *control_id == crate::app_logic::handler::ID_BUTTON_GENERATE_ARCHIVE_LOGIC && *enabled)));
    }
}
