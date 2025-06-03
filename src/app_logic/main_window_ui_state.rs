/*
 * This module defines the MainWindowUiState struct.
 * MainWindowUiState is responsible for holding and managing state
 * specifically related to the main application window's UI. This includes
 * things like the window identifier, mappings for UI elements (e.g., tree items),
 * UI-specific status caches (like archive status), and temporary data for
 * dialog flows or pending UI actions.
 */
use crate::app_logic::ui_constants;
use crate::core::ArchiveStatus; // For current_archive_status_for_ui
use crate::core::ProfileRuntimeData; // Import AppSessionData so it's in scope
use crate::platform_layer::{MessageSeverity, PlatformCommand, TreeItemId, WindowId};
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
            next_tree_item_id_counter: 1, // Default initial counter value
            current_archive_status_for_ui: None,
            pending_action: None,
            pending_new_profile_name: None,
        }
    }

    /*
     * Composes the main window title string based on the current application session data.
     * Includes the application name, current profile name (if any), and archive path status.
     */
    pub fn compose_window_title(app_session_data: &ProfileRuntimeData) -> String {
        let mut title = "SourcePacker".to_string();
        if let Some(profile_name) = app_session_data.get_current_profile_name() {
            title = format!("{} - [{}]", title, profile_name);
            if let Some(archive_path) = app_session_data.get_current_archive_path() {
                title = format!("{} - [{}]", title, archive_path.display());
            } else {
                title = format!("{} - [No Archive Set]", title);
            }
        } else {
            // If no profile name, don't show archive path either
            title = format!("{} - [No Profile Loaded]", title);
        }
        title
    }

    /*
     * Builds a list of `PlatformCommand`s for initially displaying profile information.
     * This function is intended to generate commands that reflect the state of a newly
     * activated profile. It includes setting the window title, and commands to update
     * the general status label, the dedicated token count label, and the dedicated
     * archive status label. It also includes a command to set the enabled state of the
     * "Generate Archive" button.
     *
     * Note: TreeView population is handled separately by `MyAppLogic` as it involves
     * more complex recursive descriptor building.
     */
    pub fn build_initial_profile_display_commands(
        &self,
        app_session_data: &ProfileRuntimeData, // Needs to read from AppSessionData
        initial_status_message: String,
        scan_was_successful: bool,
    ) -> Vec<PlatformCommand> {
        let mut commands = Vec::new();
        let window_id = self.window_id;

        // 1. Set Window Title
        let title = Self::compose_window_title(app_session_data);
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

        // 3. Token Count (Dedicated Token Label, and General Label via app_info! in MyAppLogic)
        let token_text = format!("Tokens: {}", app_session_data.get_cached_token_count());
        // Update dedicated token label
        commands.push(PlatformCommand::UpdateLabelText {
            window_id,
            label_id: ui_constants::STATUS_LABEL_TOKENS_ID,
            text: token_text, // MyAppLogic will also send this to general via app_info!
            severity: MessageSeverity::Information,
        });

        // 4. Archive Status (Dedicated Archive Label, and General Label if error)
        // Relies on self.current_archive_status_for_ui being up-to-date.
        // MyAppLogic should call its update_current_archive_status (which updates this field)
        // before calling this command builder.
        if app_session_data.get_current_profile_name().is_some() {
            if let Some(status) = &self.current_archive_status_for_ui {
                // Use plain string for dedicated label, MyAppLogic uses Debug for general error.
                let plain_status_string =
                    crate::app_logic::handler::MyAppLogic::archive_status_to_plain_string(status);
                let archive_label_text = format!("Archive: {}", plain_status_string);

                let archive_severity = if matches!(status, ArchiveStatus::ErrorChecking(_)) {
                    MessageSeverity::Error
                } else {
                    MessageSeverity::Information
                };

                // Update dedicated archive label
                commands.push(PlatformCommand::UpdateLabelText {
                    window_id,
                    label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                    text: archive_label_text.clone(),
                    severity: archive_severity,
                });

                // If it's an error, also update the general status label via app_error! in MyAppLogic.
                // This command builder doesn't need to queue it for general if MyAppLogic's update_current_archive_status does.
            } else {
                // This case implies archive status hasn't been determined yet *after* profile load.
                // MyAppLogic.update_current_archive_status should handle setting this.
                // For initial display, if it's None here, it means MyAppLogic hasn't run update_current_archive_status yet
                // after this profile became active. We can send a default "unknown" or let MyAppLogic handle it.
                // Let's send a neutral message for the dedicated label.
                let unknown_archive_status_text = "Archive: Status pending...".to_string();
                commands.push(PlatformCommand::UpdateLabelText {
                    window_id,
                    label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                    text: unknown_archive_status_text,
                    severity: MessageSeverity::Information,
                });
            }
        } else {
            // Case: No profile loaded.
            let no_profile_msg_archive_label = "Archive: No profile loaded".to_string();
            commands.push(PlatformCommand::UpdateLabelText {
                window_id,
                label_id: ui_constants::STATUS_LABEL_ARCHIVE_ID,
                text: no_profile_msg_archive_label.clone(),
                severity: MessageSeverity::Information,
            });
        }

        // 5. Save Button State (This button ID might be from an older design phase)
        // The Step 5.F plan does not remove this button, so the command is retained.
        let save_button_enabled = app_session_data.archive_path.is_some();
        commands.push(PlatformCommand::SetControlEnabled {
            window_id,
            control_id: super::handler::ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
            enabled: save_button_enabled,
        });

        commands
    }
}

// Minimal unit test for the constructor
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_logic::ui_constants;
    use crate::core::{ArchiveStatus, models::FileTokenDetails}; // Added FileTokenDetails
    use crate::platform_layer::WindowId;
    use std::collections::HashMap; // Added for HashMap

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
    fn test_build_initial_profile_display_commands_generates_update_label_text() {
        // Arrange
        crate::initialize_logging();
        let window_id = WindowId(1);
        let mut ui_state = MainWindowUiState::new(window_id);
        // TODO: Should use a new(....) function.
        let mut app_session_data = ProfileRuntimeData {
            profile_name: Some("TestProfile".to_string()),
            root_path_for_scan: PathBuf::from("/root"),
            archive_path: Some(PathBuf::from("/root/archive.txt")),
            file_system_snapshot_nodes: vec![],
            cached_token_count: 123,
            cached_file_token_details: HashMap::new(), // Initialize new field
        };

        ui_state.current_archive_status_for_ui = Some(ArchiveStatus::UpToDate);

        let initial_status_msg_text = "Profile loaded.".to_string();
        let token_msg_text = "Tokens: 123".to_string();
        let archive_msg_text_plain = "Archive: Up to date.".to_string();

        // Act
        let commands = ui_state.build_initial_profile_display_commands(
            &app_session_data,
            initial_status_msg_text.clone(),
            true, // scan_was_successful
        );

        log::debug!(
            "Generated commands for test_build_initial_profile_display_commands_generates_update_label_text:"
        );
        for (i, cmd) in commands.iter().enumerate() {
            log::debug!("{}: {:?}", i, cmd);
        }

        // Assert
        let mut general_initial_status_found = false;
        let mut dedicated_token_status_found = false;
        let mut dedicated_archive_status_found = false;
        let mut general_no_profile_archive_fallback_found = false;

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
                    // Check for the "No profile loaded" message if current_archive_status_for_ui was None
                    if text == "No profile loaded" && *severity == MessageSeverity::Information {
                        // Not expected here
                        general_no_profile_archive_fallback_found = true;
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
            "Archive status for dedicated archive label not found or incorrect. Expected plain format."
        );
        assert!(
            !general_no_profile_archive_fallback_found,
            "General label should not have fallback archive message when profile name is Some."
        );

        // Check for SetWindowTitle and SetControlEnabled as well
        assert!(
            commands
                .iter()
                .any(|cmd| matches!(cmd, PlatformCommand::SetWindowTitle { .. }))
        );
        assert!(commands.iter().any(|cmd| matches!(cmd, PlatformCommand::SetControlEnabled { control_id, enabled, .. }
            if *control_id == crate::app_logic::handler::ID_BUTTON_GENERATE_ARCHIVE_LOGIC && *enabled)));
    }
}
