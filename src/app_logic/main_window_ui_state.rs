/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions.
 */
use crate::core::AppSessionData; // Import AppSessionData so it's in scope
use crate::core::ArchiveStatus; // For current_archive_status_for_ui
use crate::platform_layer::{MessageSeverity, PlatformCommand, TreeItemId, WindowId}; // Import PlatformCommand and MessageSeverity
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

    pub fn compose_window_title(app_session_data: &AppSessionData) -> String {
        let mut title = "SourcePacker".to_string();
        if let Some(profile_cache) = &app_session_data.current_profile_cache {
            title = format!("{} - [{}]", title, profile_cache.name);
            if let Some(archive_path) = &profile_cache.archive_path {
                title = format!("{} - [{}]", title, archive_path.display());
            } else {
                title = format!("{} - [No Archive Set]", title);
            }
        }
        title
    }

    // This version returns commands. MyAppLogic queues them.
    pub fn build_initial_profile_display_commands(
        &self,
        app_session_data: &AppSessionData, // Needs to read from AppSessionData
        initial_status_message: String,
        scan_was_successful: bool,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        let window_id = self.window_id;
        // 1. Set Window Title
        let title = Self::compose_window_title(app_session_data);
        commands.push(PlatformCommand::SetWindowTitle { window_id, title });
        // 2. Populate Tree View
        // In Phase 3, self.build_tree_item_descriptors() would be a method here.
        // For now, let's imagine it's called and returns descriptors.
        // let tree_items = self.build_tree_item_descriptors(&app_session_data.file_nodes_cache);
        // commands.push(PlatformCommand::PopulateTreeView { window_id, items: tree_items });
        // For now, MyAppLogic would call its own build_tree_item_descriptors_recursive
        // and this helper in MainWindowUiState might not exist yet, or be simpler.
        // The plan for Phase 3 is to move build_tree_item_descriptors here.
        // 3. Initial Status Message (passed in)
        commands.push(PlatformCommand::UpdateStatusBarText {
            window_id,
            text: initial_status_message,
            severity: if scan_was_successful {
                MessageSeverity::Information
            } else {
                MessageSeverity::Error
            },
        });

        // 4. Token Count (data from AppSessionData)
        commands.push(PlatformCommand::UpdateStatusBarText {
            window_id,
            text: format!("Tokens: {}", app_session_data.cached_current_token_count),
            severity: MessageSeverity::Information,
        });

        // 5. Archive Status (data from self.current_archive_status_for_ui, which MyAppLogic would update first)
        // This part needs careful thought: update_current_archive_status is still in MyAppLogic.
        // So, MyAppLogic would call that, THEN call this method,
        // or this method needs the Archiver passed to it to check status,
        // or it just relies on self.current_archive_status_for_ui being up-to-date.
        if let Some(status) = &self.current_archive_status_for_ui {
            let status_text = format!("Archive: {:?}", status);
            commands.push(PlatformCommand::UpdateStatusBarText {
                window_id,
                text: status_text,
                severity: if matches!(status, ArchiveStatus::ErrorChecking(_)) {
                    MessageSeverity::Error
                } else {
                    MessageSeverity::Information
                },
            });
        }
        // 6. Save Button State
        let save_button_enabled = app_session_data
            .current_profile_cache
            .as_ref()
            .and_then(|p| p.archive_path.as_ref())
            .is_some();
        commands.push(PlatformCommand::SetControlEnabled {
            window_id,
            control_id: super::handler::ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
            enabled: save_button_enabled,
        });
        // 7. Show Window (This should arguably be the very last thing MyAppLogic does)
        // commands.push(PlatformCommand::ShowWindow { window_id });
        commands
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
