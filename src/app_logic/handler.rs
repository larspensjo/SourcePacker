use crate::core::{self, FileNode, FileState};
use crate::platform_layer::{
    AppEvent, CheckState, PlatformCommand, PlatformEventHandler, TreeItemDescriptor, TreeItemId,
    WindowId,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
}

impl MyAppLogic {
    pub fn new() -> Self {
        MyAppLogic {
            main_window_id: None,
            file_nodes_cache: Vec::new(),
            path_to_tree_item_id: HashMap::new(),
            next_tree_item_id_counter: 1,
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
                    _ => CheckState::Unchecked,
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
        let root_path_for_scan = PathBuf::from("."); // Scans the directory where the executable runs
        // Example: scan for .rs and .toml files in src, and Cargo.toml in root
        let whitelist_patterns = vec![
            "src/**/*.rs".to_string(),
            "src/**/*.toml".to_string(),
            "Cargo.toml".to_string(),
            "*.md".to_string(), // Include markdown files
        ];

        println!(
            "AppLogic: Scanning directory {:?} with patterns: {:?}",
            root_path_for_scan, whitelist_patterns
        );

        match core::scan_directory(&root_path_for_scan, &whitelist_patterns) {
            Ok(nodes) => {
                self.file_nodes_cache = nodes;
                println!(
                    "AppLogic: Scanned {} top-level nodes.",
                    self.file_nodes_cache.len()
                );
                if self.file_nodes_cache.is_empty() {
                    println!(
                        "AppLogic: No files matched whitelist patterns in {:?}. Tree will be empty.",
                        root_path_for_scan
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "AppLogic: Failed to scan directory {:?}: {}",
                    root_path_for_scan, e
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
                    // The platform layer's app.rs should manage PostQuitMessage
                    // based on its active_windows_count and quit signals.
                }
            }
            AppEvent::TreeViewItemToggled {
                window_id,
                item_id,   // This is the TreeItemId from the UI event
                new_state, // This is the CheckState from the UI event (Checked or Unchecked)
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
                    // Block for mutable model update
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
                    } // Mutable borrow of self.file_nodes_cache ends here

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
            AppEvent::WindowResized { .. } => { /* Ignored for now */ }
        }
        commands
    }

    fn on_quit(&mut self) {
        println!("AppLogic: on_quit called by platform. Application is exiting.");
        // Perform any final cleanup for app_logic here if needed.
    }
}
