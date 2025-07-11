use super::file_node::{FileNode, SelectionState};
use std::collections::HashSet;
use std::path::PathBuf;

/*
 * This module is responsible for managing the state of `FileNode` trees,
 * specifically applying profile settings to them and handling recursive
 * state updates for folder selections. It defines a trait `StateManagerOperations`
 * for abstracting these operations and a concrete implementation `CoreStateManager`.
 */

/*
 * Defines the operations for managing the state of `FileNode` trees.
 * This trait abstracts how profile selection states are applied to a file tree
 * and how folder selection changes propagate to children. This facilitates
 * testability by allowing mock implementations.
 */
pub trait NodeStateApplicatorOperations: Send + Sync {
    /*
     * Applies the selection states from a `Profile`'s path sets to a tree of `FileNode`s.
     * Sets `FileState::Selected` for paths in `selected_paths`,
     * `FileState::Deselected` for paths in `deselected_paths`,
     * and `FileState::New` for others. Modifies the `tree` in place.
     */
    fn apply_selection_states_to_nodes(
        &self,
        tree: &mut Vec<FileNode>,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    );

    /*
     * Updates the selection state of a folder `FileNode` and all its children recursively.
     * Sets the `new_state` on the provided `node` and all its descendant nodes.
     * Modifies the `node` and its children in place.
     */
    fn update_folder_selection(&self, node: &mut FileNode, new_state: SelectionState);
}

/*
 * The core implementation of `StateManagerOperations`.
 * This struct provides the concrete logic for applying profile states and
 * updating folder selections within a `FileNode` tree.
 */
pub struct NodeStateApplicator {}

impl NodeStateApplicator {
    /*
     * Creates a new instance of `CoreStateManager`.
     * This constructor doesn't require any parameters.
     */
    pub fn new() -> Self {
        NodeStateApplicator {}
    }
}

impl Default for NodeStateApplicator {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeStateApplicatorOperations for NodeStateApplicator {
    fn apply_selection_states_to_nodes(
        &self,
        tree: &mut Vec<FileNode>,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    ) {
        for node in tree.iter_mut() {
            if selected_paths.contains(node.path()) {
                node.set_state(SelectionState::Selected);
            } else if deselected_paths.contains(node.path()) {
                node.set_state(SelectionState::Deselected);
            } else {
                node.set_state(SelectionState::New);
            }

            if node.is_dir() && !node.children.is_empty() {
                self.apply_selection_states_to_nodes(
                    &mut node.children,
                    selected_paths,
                    deselected_paths,
                );
            }
        }
    }

    fn update_folder_selection(&self, node: &mut FileNode, new_state: SelectionState) {
        // Logic moved from the old free function
        node.set_state(new_state);
        if node.is_dir() {
            for child in node.children.iter_mut() {
                self.update_folder_selection(child, new_state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::file_node::{FileNode, SelectionState}; // Ensure full path if needed
    use std::collections::HashSet;
    use std::path::PathBuf;

    // Helper to create a simple tree for testing
    fn create_test_tree() -> Vec<FileNode> {
        vec![
            FileNode::new_full(
                PathBuf::from("/root/file1.txt"),
                "file1.txt".to_string(),
                false,
                SelectionState::New,
                vec![],
                "".to_string(),
            ),
            FileNode::new_full(
                PathBuf::from("/root/dir1"),
                "dir1".to_string(),
                true,
                SelectionState::New,
                vec![
                    FileNode::new_full(
                        PathBuf::from("/root/dir1/file2.txt"),
                        "file2.txt".to_string(),
                        false,
                        SelectionState::New,
                        vec![],
                        "".to_string(),
                    ),
                    FileNode::new_full(
                        PathBuf::from("/root/dir1/subdir"),
                        "subdir".to_string(),
                        true,
                        SelectionState::New,
                        vec![FileNode::new_full(
                            PathBuf::from("/root/dir1/subdir/file3.txt"),
                            "file3.txt".to_string(),
                            false,
                            SelectionState::New,
                            vec![],
                            "".to_string(),
                        )],
                        "".to_string(),
                    ),
                ],
                "".to_string(),
            ),
            FileNode::new_full(
                PathBuf::from("/root/file4.ext"),
                "file4.ext".to_string(),
                false,
                SelectionState::New,
                vec![],
                "".to_string(),
            ),
        ]
    }

    // Test helper for StateManagerOperations using CoreStateManager
    fn test_with_state_manager<F, R>(test_fn: F) -> R
    where
        F: FnOnce(&dyn NodeStateApplicatorOperations) -> R,
    {
        let manager = NodeStateApplicator::new();
        test_fn(&manager)
    }

    #[test]
    fn test_core_state_manager_apply_profile_select_deselect() {
        // Arrange
        let manager = NodeStateApplicator::new();
        let mut tree = create_test_tree();
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();

        selected_paths.insert(PathBuf::from("/root/file1.txt"));
        selected_paths.insert(PathBuf::from("/root/dir1/subdir/file3.txt"));
        deselected_paths.insert(PathBuf::from("/root/dir1/file2.txt"));

        // Act
        manager.apply_selection_states_to_nodes(&mut tree, &selected_paths, &deselected_paths);

        // Assert
        assert_eq!(tree[0].state(), SelectionState::Selected); // file1.txt
        assert_eq!(tree[1].state(), SelectionState::New); // dir1
        assert_eq!(tree[1].children[0].state(), SelectionState::Deselected); // dir1/file2.txt
        assert_eq!(tree[1].children[1].state(), SelectionState::New); // dir1/subdir
        assert_eq!(
            tree[1].children[1].children[0].state(),
            SelectionState::Selected
        ); // dir1/subdir/file3.txt
        assert_eq!(tree[2].state(), SelectionState::New); // file4.ext
    }

    #[test]
    fn test_core_state_manager_apply_profile_reverts_to_new() {
        // Arrange
        let manager = NodeStateApplicator::new();
        let mut tree = create_test_tree();
        tree[0].set_state(SelectionState::Selected); // Pre-set state

        let mut selected_paths = HashSet::new();
        selected_paths.insert(PathBuf::from("/root/dir1/file2.txt"));
        let deselected_paths = HashSet::new();

        // Act
        manager.apply_selection_states_to_nodes(&mut tree, &selected_paths, &deselected_paths);

        // Assert
        assert_eq!(tree[0].state(), SelectionState::New); // Should revert to New as it's not in selected_paths
        assert_eq!(tree[1].children[0].state(), SelectionState::Selected); // dir1/file2.txt should be selected
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_select_all() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();

            // Act
            manager.update_folder_selection(&mut tree[1], SelectionState::Selected);

            // Assert
            assert_eq!(tree[1].state(), SelectionState::Selected);
            assert_eq!(tree[1].children[0].state(), SelectionState::Selected);
            assert_eq!(tree[1].children[1].state(), SelectionState::Selected);
            assert_eq!(
                tree[1].children[1].children[0].state(),
                SelectionState::Selected
            );
            assert_eq!(tree[0].state(), SelectionState::New); // Other nodes unaffected
        });
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_deselect_all() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();
            manager.update_folder_selection(&mut tree[1], SelectionState::Selected); // Pre-select

            // Act
            manager.update_folder_selection(&mut tree[1], SelectionState::Deselected); // Then deselect

            // Assert
            assert_eq!(tree[1].state(), SelectionState::Deselected);
            assert_eq!(tree[1].children[0].state(), SelectionState::Deselected);
            assert_eq!(tree[1].children[1].state(), SelectionState::Deselected);
            assert_eq!(
                tree[1].children[1].children[0].state(),
                SelectionState::Deselected
            );
        });
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_on_file_node() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();

            // Act & Assert for Selected
            manager.update_folder_selection(&mut tree[0], SelectionState::Selected);
            assert_eq!(tree[0].state(), SelectionState::Selected);

            // Act & Assert for Deselected
            manager.update_folder_selection(&mut tree[0], SelectionState::Deselected);
            assert_eq!(tree[0].state(), SelectionState::Deselected);

            // Children should remain empty as it's a file node
            assert_eq!(tree[0].children.len(), 0);
        });
    }
}
