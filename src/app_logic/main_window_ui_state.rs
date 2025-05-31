/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions.
 */
use crate::core::ArchiveStatus; // For current_archive_status_for_ui
use crate::platform_layer::{TreeItemId, WindowId};
use std::collections::HashMap;
use std::path::PathBuf;

// These types are currently defined in `handler.rs`.
// We will import them from there for now.
// Later phases might move them if they become exclusive to MainWindowUiState
// or a shared types module.
use super::handler::{PathToTreeItemIdMap, PendingAction};

/*
 * Holds UI-specific state for the main application window.
 * This struct consolidates data that is directly tied to the presentation
 * and interaction logic of the main window, separating it from the core
 * application data (`AppSessionData`) and the central orchestrator (`MyAppLogic`).
 */
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
            next_tree_item_id_counter: 1, // Default initial counter value
            current_archive_status_for_ui: None,
            pending_action: None,
            pending_new_profile_name: None,
        }
    }
}

// Minimal unit test for the constructor
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_layer::WindowId; // For test

    #[test]
    fn test_main_window_ui_state_new() {
        let test_window_id = WindowId(42);
        let ui_state = MainWindowUiState::new(test_window_id);

        assert_eq!(ui_state.window_id, test_window_id);
        assert!(ui_state.path_to_tree_item_id.is_empty());
        assert_eq!(ui_state.next_tree_item_id_counter, 1);
        assert!(ui_state.current_archive_status_for_ui.is_none());
        assert!(ui_state.pending_action.is_none());
        assert!(ui_state.pending_new_profile_name.is_none());
    }
}
