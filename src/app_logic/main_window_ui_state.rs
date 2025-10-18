/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions. It interacts with ProfileRuntimeDataOperations
 * to get necessary data for UI display.
 */
use crate::core::{ArchiveStatus, ProfileRuntimeDataOperations};
use crate::platform_layer::{TreeItemDescriptor, WindowId};
use std::collections::HashMap;

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
    /* Stores the current text used for filtering the TreeView. */
    pub filter_text: Option<String>,
    /* Cached descriptors from the last successful filter operation. */
    pub last_successful_filter_result: Vec<TreeItemDescriptor>,
    /* Indicates that the current filter text yielded no matches. */
    pub filter_no_match: bool,
}

impl MainWindowUiState {
    /*
     * Creates a new `MainWindowUiState` instance for a given window.
     * Initializes with the provided `window_id`, an empty path-to-ID map,
     * a starting counter for tree item IDs, and no pending actions or statuses.
     */
    pub fn new(window_id: WindowId) -> Self {
        log::debug!("MainWindowUiState::new called for window_id: {window_id:?}");
        MainWindowUiState {
            window_id,
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
            current_archive_status_for_ui: None,
            pending_action: None,
            pending_new_profile_name: None,
            filter_text: None,
            last_successful_filter_result: Vec::new(),
            filter_no_match: false,
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
            title = format!("{title} - [{profile_name}]");
            if let Some(archive_path) = app_session_data_ops.get_archive_path() {
                title = format!("{} - [{}]", title, archive_path.display());
            } else {
                title = format!("{title} - [No Archive Set]");
            }
        } else {
            title = format!("{title} - [No Profile Loaded]");
        }
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        FileNode, FileSystemScannerOperations, NodeStateApplicatorOperations, Profile,
        ProfileRuntimeDataOperations, SelectionState, TokenCounterOperations,
    };
    use crate::platform_layer::WindowId;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    // --- MockProfileRuntimeDataOperations for MainWindowUiState tests ---
    // This is a simplified mock, just enough for these tests.
    #[derive(Default)]
    struct MockProfileRuntimeDataOps {
        profile_name: Option<String>,
        archive_path: Option<PathBuf>,
        exclude_patterns: Vec<String>,
        // We don't need to mock all methods, only those used by MainWindowUiState
    }

    impl ProfileRuntimeDataOperations for MockProfileRuntimeDataOps {
        fn get_profile_name(&self) -> Option<String> {
            self.profile_name.clone()
        }
        fn get_archive_path(&self) -> Option<PathBuf> {
            self.archive_path.clone()
        }
        fn get_exclude_patterns(&self) -> Vec<String> {
            self.exclude_patterns.clone()
        }
        fn set_exclude_patterns(&mut self, patterns: Vec<String>) {
            self.exclude_patterns = patterns;
        }

        // --- Unused methods for these specific tests, provide default/dummy implementations ---
        fn set_profile_name(&mut self, _name: Option<String>) {
            unimplemented!("MockProfileRuntimeDataOps: set_profile_name")
        }
        fn set_archive_path(&mut self, _path: Option<PathBuf>) {
            unimplemented!("MockProfileRuntimeDataOps: set_archive_path")
        }
        fn get_root_path_for_scan(&self) -> PathBuf {
            unimplemented!("MockProfileRuntimeDataOps: get_root_path_for_scan")
        }
        fn get_snapshot_nodes(&self) -> &Vec<FileNode> {
            unimplemented!("MockProfileRuntimeDataOps: get_snapshot_nodes")
        }
        fn set_snapshot_nodes(&mut self, _nodes: Vec<FileNode>) {
            unimplemented!("MockProfileRuntimeDataOps: set_snapshot_nodes")
        }
        fn apply_selection_states_to_snapshot(
            &mut self,
            _state_manager: &dyn NodeStateApplicatorOperations,
            _selected_paths: &HashSet<PathBuf>,
            _deselected_paths: &HashSet<PathBuf>,
        ) {
            unimplemented!("MockProfileRuntimeDataOps: apply_selection_states_to_snapshot")
        }
        fn get_node_attributes_for_path(&self, _path: &Path) -> Option<(SelectionState, bool)> {
            unimplemented!("MockProfileRuntimeDataOps: get_node_attributes_for_path")
        }
        fn update_node_state_and_collect_changes(
            &mut self,
            _path: &Path,
            _new_state: SelectionState,
            _state_manager: &dyn NodeStateApplicatorOperations,
        ) -> Vec<(PathBuf, SelectionState)> {
            unimplemented!("MockProfileRuntimeDataOps: update_node_state_and_collect_changes")
        }
        fn does_path_or_descendants_contain_new_file(&self, _path: &Path) -> bool {
            // For MainWindowUiState tests, this method isn't directly called by the tested functions.
            // However, to satisfy the trait, a default implementation is needed.
            // If tests for MainWindowUiState were to indirectly rely on this,
            // this mock would need to be enhanced (e.g., with a settable result).
            log::warn!(
                "MockProfileRuntimeDataOps: does_path_or_descendants_contain_new_file called, returning default false."
            );
            false
        }
        fn update_total_token_count_for_selected_files(
            &mut self,
            _token_counter: &dyn TokenCounterOperations,
        ) -> usize {
            unimplemented!("MockProfileRuntimeDataOps: update_total_token_count_for_selected_files")
        }
        fn clear(&mut self) {
            unimplemented!("MockProfileRuntimeDataOps: clear")
        }
        fn create_profile_snapshot(&self) -> Profile {
            unimplemented!("MockProfileRuntimeDataOps: create_profile_snapshot")
        }
        fn load_profile_into_session(
            &mut self,
            _loaded_profile: Profile,
            _file_system_scanner: &dyn FileSystemScannerOperations,
            _state_manager: &dyn NodeStateApplicatorOperations,
            _token_counter: &dyn TokenCounterOperations,
        ) -> Result<(), String> {
            unimplemented!("MockProfileRuntimeDataOps: load_profile_into_session")
        }
        fn get_current_selection_paths(&self) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
            unimplemented!("MockProfileRuntimeDataOps: get_current_selection_paths")
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
        assert!(ui_state.filter_text.is_none());
        assert!(ui_state.last_successful_filter_result.is_empty());
        assert!(!ui_state.filter_no_match);
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
}
