/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions. It interacts with ProfileRuntimeDataOperations
 * to get necessary data for UI display.
 */
use crate::core::{ArchiveStatus, ContentSearchProgress, FileNode, ProfileRuntimeDataOperations};
use crate::platform_layer::{TreeItemDescriptor, TreeItemId, WindowId};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::core::{TokenProgress, TokenProgressChannel};
#[cfg(test)]
use std::sync::Arc;

// These types are currently defined in `handler.rs`.
use super::handler::{PathToTreeItemIdMap, PendingAction};

/*
 * Represents the available search strategies for the tree view filter bar.
 * `ByName` limits filtering to path and file names, while `ByContent`
 * will route filter requests through the asynchronous content search pipeline.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    ByName,
    ByContent,
}

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
    window_id: WindowId,
    /* Maps file/directory paths to their corresponding TreeItemId in the UI's tree view. */
    path_to_tree_item_id: PathToTreeItemIdMap,
    /* A counter to generate unique TreeItemIds for the tree view. */
    next_tree_item_id_counter: u64,
    /* A cache of the current archive status, specifically for UI display purposes. */
    current_archive_status_for_ui: Option<ArchiveStatus>,
    /* Tracks any pending multi-step UI action, like saving a file or a dialog sequence. */
    pending_action: Option<PendingAction>,
    /* Stores a temporary profile name, typically used during a new profile creation flow. */
    pending_new_profile_name: Option<String>,
    /* Stores the current text used for filtering the TreeView. */
    filter_text: Option<String>,
    /* Cached descriptors from the last successful filter operation. */
    last_successful_filter_result: Vec<TreeItemDescriptor>,
    /* Indicates that the current filter text yielded no matches. */
    filter_no_match: bool,
    /* Tracks which TreeView item is currently selected for viewing in the preview pane. */
    active_viewer_item_id: Option<TreeItemId>,
    /* Tracks whether the filter bar operates on names or file content. */
    search_mode: SearchMode,
    /* Latest content-search matches keyed by absolute file path, if any. */
    content_search_matches: Option<HashSet<PathBuf>>,
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
            active_viewer_item_id: None,
            search_mode: SearchMode::ByName,
            content_search_matches: None,
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

    /*
     * Returns the WindowId associated with this UI state. Providing a dedicated accessor
     * keeps the identifier immutable to callers while still enabling them to compare IDs
     * without exposing internal fields.
     */
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    /*
     * Returns the currently active search mode without exposing mutable access.
     */
    pub fn search_mode(&self) -> SearchMode {
        self.search_mode
    }

    /*
     * Toggles between the supported search modes and returns the new mode.
     * Keeping the flip logic here ensures other callers do not need to clone or mutate
     * unrelated UI state just to change how the filter bar behaves.
     */
    pub fn toggle_search_mode(&mut self) -> SearchMode {
        self.search_mode = match self.search_mode {
            SearchMode::ByName => SearchMode::ByContent,
            SearchMode::ByContent => SearchMode::ByName,
        };
        self.search_mode
    }

    /*
     * Stores the current set of files that matched the most recent content search.
     * Passing `None` signals that results are pending or have been cleared; passing
     * `Some(HashSet)` caches the full result set (even if empty) for downstream consumers.
     */
    pub fn set_content_search_matches(&mut self, matches: Option<HashSet<PathBuf>>) {
        self.content_search_matches = matches;
    }

    /*
     * Provides read-only access to the cached content-search matches so the UI layer
     * can decide whether to show a file when content filtering is active.
     */
    pub fn content_search_matches(&self) -> Option<&HashSet<PathBuf>> {
        self.content_search_matches.as_ref()
    }

    /*
     * Associates or clears the cached archive status used for quick UI updates. Encapsulating
     * this assignment allows future logic (such as change detection) to be centralized.
     */
    pub fn set_archive_status(&mut self, status: Option<ArchiveStatus>) {
        self.current_archive_status_for_ui = status;
    }

    /*
     * Retrieves the cached archive status if one is currently stored.
     */
    pub fn archive_status(&self) -> Option<&ArchiveStatus> {
        self.current_archive_status_for_ui.as_ref()
    }

    pub fn set_active_viewer_item_id(&mut self, item_id: Option<TreeItemId>) {
        self.active_viewer_item_id = item_id;
    }

    pub fn active_viewer_item_id(&self) -> Option<TreeItemId> {
        self.active_viewer_item_id
    }

    /*
     * Records the current multi-step UI action and cleanly replaces any existing value.
     * Centralizing this setter ensures that logging or validation can be added later
     * without touching call sites.
     */
    pub fn set_pending_action(&mut self, action: Option<PendingAction>) {
        self.pending_action = action;
    }

    /*
     * Removes and returns any active pending action. Using `Option::take` via this helper
     * avoids leaking the internal option and keeps state transitions explicit.
     */
    pub fn take_pending_action(&mut self) -> Option<PendingAction> {
        self.pending_action.take()
    }

    /*
     * Provides read-only access to the current pending action, if one exists.
     */
    pub fn pending_action(&self) -> Option<&PendingAction> {
        self.pending_action.as_ref()
    }

    /*
     * Sets or clears the temporary profile name captured during the profile-creation flow.
     */
    pub fn set_pending_new_profile_name(&mut self, name: Option<String>) {
        self.pending_new_profile_name = name;
    }

    /*
     * Takes ownership of the pending profile name, clearing it from the state in the process.
     */
    pub fn take_pending_new_profile_name(&mut self) -> Option<String> {
        self.pending_new_profile_name.take()
    }

    /*
     * Returns the pending profile name reference when the name has been captured but not yet used.
     */
    pub fn pending_new_profile_name(&self) -> Option<&str> {
        self.pending_new_profile_name.as_deref()
    }

    /*
     * Updates the stored filter text and returns whether the filter should be considered active.
     * Empty strings disable the filter entirely.
     */
    pub fn set_filter_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            self.filter_text = None;
            self.filter_no_match = false;
            return false;
        }

        self.filter_text = Some(text.to_string());
        true
    }

    /*
     * Clears the filter text and resets the associated flags so future tree rebuilds operate
     * on the full snapshot.
     */
    pub fn clear_filter(&mut self) {
        self.filter_text = None;
        self.filter_no_match = false;
        self.content_search_matches = None;
    }

    /*
     * Returns the currently active filter text, if any.
     */
    pub fn filter_text(&self) -> Option<&str> {
        self.filter_text.as_deref()
    }

    /*
     * Indicates whether the last rebuild produced no matches for the active filter.
     */
    pub fn filter_had_no_match(&self) -> bool {
        self.filter_no_match
    }

    /*
     * Rebuilds the cached TreeView descriptors using the current filter and snapshot. The method
     * updates internal maps, preserves the last successful filter result for reuse, and returns
     * the list of descriptors that should be rendered.
     */
    pub fn rebuild_tree_descriptors(
        &mut self,
        snapshot_nodes: &[FileNode],
    ) -> Vec<TreeItemDescriptor> {
        if let Some(matches) = self.content_search_matches.as_ref() {
            self.path_to_tree_item_id.clear();
            self.next_tree_item_id_counter = 1;
            let descriptors = FileNode::build_tree_item_descriptors_from_matches(
                snapshot_nodes,
                matches,
                &mut self.path_to_tree_item_id,
                &mut self.next_tree_item_id_counter,
            );
            self.filter_no_match = descriptors.is_empty();
            self.last_successful_filter_result = descriptors.clone();
            return descriptors;
        }

        let active_filter = self.filter_text.clone();
        self.path_to_tree_item_id.clear();
        self.next_tree_item_id_counter = 1;

        let descriptors = if let Some(filter) = active_filter.as_deref() {
            FileNode::build_tree_item_descriptors_filtered(
                snapshot_nodes,
                filter,
                &mut self.path_to_tree_item_id,
                &mut self.next_tree_item_id_counter,
            )
        } else {
            FileNode::build_tree_item_descriptors_recursive(
                snapshot_nodes,
                &mut self.path_to_tree_item_id,
                &mut self.next_tree_item_id_counter,
            )
        };

        if active_filter.is_some() {
            if descriptors.is_empty() {
                self.filter_no_match = true;
                return self.last_successful_filter_result.clone();
            }
            self.filter_no_match = false;
            self.last_successful_filter_result = descriptors.clone();
            descriptors
        } else {
            self.filter_no_match = false;
            self.last_successful_filter_result = descriptors.clone();
            descriptors
        }
    }

    /*
     * Returns the TreeItemId for a given path if one is registered. This avoids exposing the
     * underlying map and keeps path lookups consistent across the application.
     */
    pub fn tree_item_id_for_path(&self, path: &Path) -> Option<TreeItemId> {
        self.path_to_tree_item_id.get(path).copied()
    }

    /*
     * Locates the original path associated with a TreeItemId. A clone of the path is returned
     * because the mapping is stored internally and should not be exposed mutably.
     */
    pub fn path_for_tree_item(&self, tree_item_id: TreeItemId) -> Option<PathBuf> {
        self.path_to_tree_item_id.iter().find_map(|(path, id)| {
            if *id == tree_item_id {
                Some(path.clone())
            } else {
                None
            }
        })
    }

    /*
     * Exposes the cached descriptors from the last successful filter execution so callers
     * can reuse them (for example, to keep the tree populated during "no match" states).
     */
    pub fn last_successful_filter_descriptors(&self) -> &[TreeItemDescriptor] {
        &self.last_successful_filter_result
    }

    #[cfg(test)]
    pub(crate) fn path_map_for_test(&self) -> &PathToTreeItemIdMap {
        &self.path_to_tree_item_id
    }

    #[cfg(test)]
    pub(crate) fn next_tree_item_counter_for_test(&self) -> u64 {
        self.next_tree_item_id_counter
    }

    #[cfg(test)]
    pub(crate) fn insert_tree_item_mapping_for_test(&mut self, path: PathBuf, id: TreeItemId) {
        self.path_to_tree_item_id.insert(path, id);
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
    use std::sync::mpsc;

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
        fn recalc_tokens_async(
            &mut self,
            _token_counter: Arc<dyn crate::core::TokenCounterOperations>,
            _only_selected: bool,
        ) -> Option<TokenProgressChannel> {
            unimplemented!("MockProfileRuntimeDataOps: recalc_tokens_async")
        }
        fn apply_token_progress(&mut self, _progress: TokenProgress) -> usize {
            unimplemented!("MockProfileRuntimeDataOps: apply_token_progress")
        }
        fn search_content_async(
            &self,
            _search_term: String,
        ) -> Option<mpsc::Receiver<ContentSearchProgress>> {
            None
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
        assert_eq!(ui_state.window_id(), test_window_id);
        assert!(ui_state.path_map_for_test().is_empty());
        assert_eq!(ui_state.next_tree_item_counter_for_test(), 1);
        assert!(ui_state.archive_status().is_none());
        assert!(ui_state.pending_action().is_none());
        assert!(ui_state.pending_new_profile_name().is_none());
        assert!(ui_state.filter_text().is_none());
        assert!(ui_state.last_successful_filter_descriptors().is_empty());
        assert!(!ui_state.filter_had_no_match());
        assert!(ui_state.active_viewer_item_id().is_none());
        assert_eq!(ui_state.search_mode(), SearchMode::ByName);
        assert!(ui_state.content_search_matches().is_none());
    }

    #[test]
    fn toggle_search_mode_flips_between_variants() {
        // Arrange
        crate::initialize_logging();
        let window_id = WindowId(11);
        let mut ui_state = MainWindowUiState::new(window_id);

        // Act / Assert
        assert_eq!(ui_state.search_mode(), SearchMode::ByName);
        assert_eq!(ui_state.toggle_search_mode(), SearchMode::ByContent);
        assert_eq!(ui_state.search_mode(), SearchMode::ByContent);
        assert_eq!(ui_state.toggle_search_mode(), SearchMode::ByName);
    }

    #[test]
    fn rebuild_tree_descriptors_tracks_filter_state() {
        // Arrange
        crate::initialize_logging();
        let window_id = WindowId(7);
        let mut ui_state = MainWindowUiState::new(window_id);

        let base_path = PathBuf::from("/root");
        let file_a_path = base_path.join("alpha.txt");
        let file_b_path = base_path.join("beta.txt");

        let nodes = vec![
            FileNode::new_full(
                file_a_path.clone(),
                "alpha.txt".into(),
                false,
                SelectionState::Selected,
                Vec::new(),
                "".to_string(),
            ),
            FileNode::new_full(
                file_b_path.clone(),
                "beta.txt".into(),
                false,
                SelectionState::Selected,
                Vec::new(),
                "".to_string(),
            ),
        ];

        // Act: no filter
        let all_descriptors = ui_state.rebuild_tree_descriptors(&nodes);

        // Assert: both nodes present and mappings created
        assert_eq!(all_descriptors.len(), 2);
        assert!(ui_state.tree_item_id_for_path(&file_a_path).is_some());
        let first_id = all_descriptors[0].id;
        assert_eq!(
            ui_state.path_for_tree_item(first_id).as_deref(),
            Some(file_a_path.as_path())
        );
        assert!(!ui_state.filter_had_no_match());

        // Act: apply filter that matches beta
        assert!(ui_state.set_filter_text("beta"));
        let filtered_descriptors = ui_state.rebuild_tree_descriptors(&nodes);
        let filtered_texts: Vec<String> = filtered_descriptors
            .iter()
            .map(|descriptor| descriptor.text.clone())
            .collect();

        // Assert: single descriptor returned, filter active with matches
        assert_eq!(filtered_descriptors.len(), 1);
        assert_eq!(filtered_descriptors[0].text, "beta.txt");
        assert!(!ui_state.filter_had_no_match());

        // Act: apply filter with no matches
        assert!(ui_state.set_filter_text("zzz"));
        let no_match_descriptors = ui_state.rebuild_tree_descriptors(&nodes);

        // Assert: reuses last successful descriptors and flags no-match
        let no_match_texts: Vec<String> = no_match_descriptors
            .iter()
            .map(|descriptor| descriptor.text.clone())
            .collect();
        assert_eq!(no_match_texts, filtered_texts);
        assert!(ui_state.filter_had_no_match());
    }

    #[test]
    fn rebuild_tree_descriptors_uses_content_matches_when_available() {
        crate::initialize_logging();
        let window_id = WindowId(9);
        let mut ui_state = MainWindowUiState::new(window_id);

        let nodes = vec![FileNode::new_full(
            PathBuf::from("/root/match.txt"),
            "match.txt".into(),
            false,
            SelectionState::Selected,
            Vec::new(),
            "".to_string(),
        )];

        let mut matches = HashSet::new();
        matches.insert(PathBuf::from("/root/match.txt"));
        ui_state.set_content_search_matches(Some(matches));

        let descriptors = ui_state.rebuild_tree_descriptors(&nodes);
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].text, "match.txt");
        assert!(!ui_state.filter_had_no_match());
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
