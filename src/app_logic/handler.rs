use crate::core::{self, FileNode, FileState};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf}; // For writing the archive file

// Define control IDs used by app_logic to identify controls, must match platform layer
pub const ID_BUTTON_GENERATE_ARCHIVE_LOGIC: i32 = 1002; // Matches platform_layer's ID

/// Maps a `PathBuf` (unique identifier for a FileNode) to the `TreeItemId` currently
/// representing it in the UI. This map is rebuilt during `PopulateTreeView`.
type PathToTreeItemIdMap = HashMap<PathBuf, TreeItemId>;

/// Main structure for the application's UI logic.
/// It is platform-agnostic and interacts with the UI via `AppEvent`s and `PlatformCommand`s.
pub struct MyAppLogic {
    main_window_id: Option<WindowId>,
    /// The current tree of file nodes being displayed. This is the source of truth for content.
    file_nodes_cache: Vec<FileNode>,
    /// Maps the `PathBuf` of a `FileNode` to its current `TreeItemId` in the UI.
    path_to_tree_item_id: PathToTreeItemIdMap,
    /// Counter to generate unique `TreeItemId`s during descriptor building.
    next_tree_item_id_counter: u64,
    /// The root path used for the last directory scan.
    root_path_for_scan: PathBuf,
    /// Temporarily stores content of the generated archive before saving.
    pending_archive_content: Option<String>,
}

impl MyAppLogic {
    pub fn new() -> Self {
        MyAppLogic {
            main_window_id: None,
            file_nodes_cache: Vec::new(),
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
            root_path_for_scan: PathBuf::new(), // Initialize empty, set in on_main_window_created
            pending_archive_content: None,
        }
    }

    fn generate_tree_item_id(&mut self) -> TreeItemId {
        let id = self.next_tree_item_id_counter;
        self.next_tree_item_id_counter += 1;
        TreeItemId(id)
    }

    /// Converts the internal `FileNode`s to `TreeItemDescriptor`s for the platform layer.
    /// Also populates the `path_to_tree_item_id` mapping.
    fn build_tree_item_descriptors_recursive(
        nodes: &[FileNode],
        path_to_tree_item_id: &mut PathToTreeItemIdMap, // Pass map mutably
        next_tree_item_id_counter: &mut u64,            // Pass counter mutably
    ) -> Vec<TreeItemDescriptor> {
        let mut descriptors = Vec::new();
        for node in nodes {
            let id_val = *next_tree_item_id_counter; // Deref and use
            *next_tree_item_id_counter += 1; // Increment
            let item_id = TreeItemId(id_val);

            path_to_tree_item_id.insert(node.path.clone(), item_id); // Use the mutable map

            let descriptor = TreeItemDescriptor {
                id: item_id,
                text: node.name.clone(),
                is_folder: node.is_dir,
                state: match node.state {
                    FileState::Selected => CheckState::Checked,
                    _ => CheckState::Unchecked, // Default to unchecked for Deselected/Unknown
                },
                // Recursive call passes the mutable refs along
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

    /// Called once the main window is created by the platform layer.
    pub fn on_main_window_created(&mut self, window_id: WindowId) -> Vec<PlatformCommand> {
        self.main_window_id = Some(window_id);
        let mut commands = Vec::new();

        // --- Actual data loading ---
        // Modify this path and patterns as needed for your project structure
        let root_path = PathBuf::from("."); // Scans the directory where the executable runs
        self.root_path_for_scan = root_path.clone(); // Store it

        // Example: scan for .rs and .toml files in src, and Cargo.toml in root
        let whitelist_patterns = vec![
            "src/**/*.rs".to_string(),
            "src/**/*.toml".to_string(),
            "Cargo.toml".to_string(),
            "doc/*.md".to_string(), // Include markdown files
        ];

        println!(
            "AppLogic: Scanning directory {:?} with patterns: {:?}",
            self.root_path_for_scan, whitelist_patterns
        );

        match core::scan_directory(&self.root_path_for_scan, &whitelist_patterns) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                println!(
                    "AppLogic: Scanned {} top-level nodes.",
                    self.file_nodes_cache.len()
                );
                if self.file_nodes_cache.is_empty() {
                    println!(
                        "AppLogic: No files matched whitelist patterns in {:?}. Tree will be empty.",
                        self.root_path_for_scan
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "AppLogic: Failed to scan directory {:?}: {}",
                    self.root_path_for_scan, e
                );
                let error_node_path = PathBuf::from("/error_node");
                self.file_nodes_cache = vec![FileNode::new(
                    error_node_path,
                    format!("Error scanning directory: {}", e),
                    false,
                )];
            }
        }
        // --- End of data loading ---

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
        }
        commands.push(PlatformCommand::ShowWindow { window_id });
        commands
    }

    /// Finds a mutable reference to a `FileNode` within a slice by its `PathBuf`.
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

    /// Finds an immutable reference to a `FileNode` within a slice by its `PathBuf`.
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

    /// Recursively collects `TreeItemId`s and their new `CheckState` for nodes
    /// that need their UI updated, starting from a given `FileNode`.
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
                    // This command should lead the platform to call DestroyWindow.
                    commands.push(PlatformCommand::CloseWindow { window_id });
                }
            }
            AppEvent::WindowDestroyed { window_id } => {
                if self.main_window_id == Some(window_id) {
                    println!("AppLogic: Main window destroyed notification received.");
                    self.main_window_id = None;
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
                                // If UI unchecks, model becomes Deselected
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

                    // Perform a non-mutable find to get a reference for collecting visual updates
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
                    match core::create_archive_content(
                        &self.file_nodes_cache,
                        &self.root_path_for_scan,
                    ) {
                        Ok(content) => {
                            self.pending_archive_content = Some(content);
                            commands.push(PlatformCommand::ShowSaveFileDialog {
                                window_id,
                                title: "Save Archive As".to_string(),
                                default_filename: "archive.txt".to_string(),
                                filter_spec: "Text Files (*.txt)\0*.txt\0All Files (*.*)\0*.*\0\0"
                                    .to_string(),
                            });
                        }
                        Err(e) => {
                            eprintln!("AppLogic: Failed to create archive content: {}", e);
                            // Future: Show a message box to the user via a PlatformCommand
                            // For now, just log. If content generation fails, pending_archive_content remains None.
                        }
                    }
                }
            }
            AppEvent::FileSaveDialogCompleted { window_id, result } => {
                if self.main_window_id == Some(window_id) {
                    match result {
                        Some(path) => {
                            if let Some(content) = self.pending_archive_content.take() {
                                println!("AppLogic: Saving archive to {:?}", path);
                                match fs::write(&path, content) {
                                    Ok(_) => println!(
                                        "AppLogic: Successfully saved archive to {:?}",
                                        path
                                    ),
                                    Err(e) => eprintln!(
                                        "AppLogic: Failed to write archive to {:?}: {}",
                                        path, e
                                    ),
                                }
                            } else {
                                eprintln!(
                                    "AppLogic: FileSaveDialogCompleted with path, but no pending content to save."
                                );
                            }
                        }
                        None => {
                            println!("AppLogic: File save dialog cancelled by user.");
                            self.pending_archive_content = None; // Clear pending content if cancelled
                        }
                    }
                }
            }
            AppEvent::WindowResized { .. } => { /* Ignored for now by app logic, platform handles layout */
            }
        }
        commands
    }

    fn on_quit(&mut self) {
        println!("AppLogic: on_quit called by platform. Application is exiting.");
        // Perform any final cleanup for app_logic here if needed.
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Bring MyAppLogic and other items into scope
    use tempfile::NamedTempFile; // For creating temporary files in tests

    #[test]
    fn test_handle_button_click_generates_save_dialog() {
        let mut logic = MyAppLogic::new();
        logic.main_window_id = Some(WindowId(1));
        logic.file_nodes_cache = vec![]; // empty tree
        logic.root_path_for_scan = PathBuf::from(".");
        // Ensure pending_archive_content is None initially
        let cmds = logic.handle_event(AppEvent::ButtonClicked {
            window_id: WindowId(1),
            control_id: ID_BUTTON_GENERATE_ARCHIVE_LOGIC,
        });
        // Expect a ShowSaveFileDialog command
        assert!(matches!(
            cmds.as_slice(),
            [PlatformCommand::ShowSaveFileDialog { .. }]
        ));
    }

    #[test]
    fn test_handle_file_save_dialog_completed_with_path() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);
        logic.pending_archive_content = Some("ARCHIVE CONTENT".to_string());

        // Create a temp file and pass its path to the event
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: main_id,
            result: Some(path.clone()),
        });

        // No follow-up commands expected
        assert!(cmds.is_empty());
        // pending_archive_content should be cleared
        assert_eq!(logic.pending_archive_content, None);
        // And the file should have been written
        let written = fs::read_to_string(path).unwrap();
        assert_eq!(written, "ARCHIVE CONTENT");
    }

    #[test]
    fn test_handle_file_save_dialog_cancelled() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);
        logic.pending_archive_content = Some("WILL BE CLEARED".to_string());

        // Simulate user cancelling the save dialog
        let cmds = logic.handle_event(AppEvent::FileSaveDialogCompleted {
            window_id: main_id,
            result: None,
        });

        assert!(cmds.is_empty());
        // pending content should be cleared on cancel
        assert_eq!(logic.pending_archive_content, None);
    }

    #[test]
    fn test_handle_treeview_item_toggled_updates_model_and_emits_visual_update() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(1);
        logic.main_window_id = Some(main_id);

        // Prepare the cache + mapping
        let foo_path = PathBuf::from("/tmp/foo");
        logic.file_nodes_cache = vec![FileNode::new(foo_path.clone(), "foo".into(), false)];
        logic
            .path_to_tree_item_id
            .insert(foo_path.clone(), TreeItemId(42));

        let cmds = logic.handle_event(AppEvent::TreeViewItemToggled {
            window_id: main_id,
            item_id: TreeItemId(42),
            new_state: CheckState::Checked,
        });

        // Exactly one command?
        assert_eq!(cmds.len(), 1);

        // Destructure it and compare each field
        match &cmds[0] {
            PlatformCommand::UpdateTreeItemVisualState {
                window_id,
                item_id,
                new_state,
            } => {
                assert_eq!(*window_id, main_id);
                assert_eq!(*item_id, TreeItemId(42));
                assert_eq!(*new_state, CheckState::Checked);
            }
            other => panic!("expected UpdateTreeItemVisualState, got {:?}", other),
        }
    }
    #[test]
    fn test_handle_window_close_requested_generates_close_command() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(7);
        logic.main_window_id = Some(main_id);

        let cmds = logic.handle_event(AppEvent::WindowCloseRequested { window_id: main_id });

        // Exactly one command?
        assert_eq!(cmds.len(), 1);

        // Destructure it and compare the field
        match &cmds[0] {
            PlatformCommand::CloseWindow { window_id } => {
                assert_eq!(*window_id, main_id);
            }
            other => panic!("expected CloseWindow, got {:?}", other),
        }
    }

    #[test]
    fn test_handle_window_destroyed_clears_main_window_id() {
        let mut logic = MyAppLogic::new();
        let main_id = WindowId(7);
        logic.main_window_id = Some(main_id);

        let cmds = logic.handle_event(AppEvent::WindowDestroyed { window_id: main_id });

        // No commands, but main_window_id should be dropped
        assert!(cmds.is_empty());
        assert_eq!(logic.main_window_id, None);
    }
}
