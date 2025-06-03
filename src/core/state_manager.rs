use super::models::{FileNode, FileState, Profile};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
pub trait StateManagerOperations: Send + Sync {
    /*
     * Applies the selection states from a `Profile`'s path sets to a tree of `FileNode`s.
     * Sets `FileState::Selected` for paths in `selected_paths`,
     * `FileState::Deselected` for paths in `deselected_paths`,
     * and `FileState::New` for others. Modifies the `tree` in place.
     * TODO: Should be renamed, as it is no longer a Profile that is used.
     */
    fn apply_profile_to_tree(
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
    fn update_folder_selection(&self, node: &mut FileNode, new_state: FileState);
}

/*
 * The core implementation of `StateManagerOperations`.
 * This struct provides the concrete logic for applying profile states and
 * updating folder selections within a `FileNode` tree.
 */
pub struct CoreStateManager {}

impl CoreStateManager {
    /*
     * Creates a new instance of `CoreStateManager`.
     * This constructor doesn't require any parameters.
     */
    pub fn new() -> Self {
        CoreStateManager {}
    }
}

impl Default for CoreStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StateManagerOperations for CoreStateManager {
    fn apply_profile_to_tree(
        &self,
        tree: &mut Vec<FileNode>,
        selected_paths: &HashSet<PathBuf>,
        deselected_paths: &HashSet<PathBuf>,
    ) {
        for node in tree.iter_mut() {
            if selected_paths.contains(&node.path) {
                node.state = FileState::Selected;
            } else if deselected_paths.contains(&node.path) {
                node.state = FileState::Deselected;
            } else {
                node.state = FileState::New;
            }

            if node.is_dir && !node.children.is_empty() {
                self.apply_profile_to_tree(&mut node.children, selected_paths, deselected_paths);
            }
        }
    }

    fn update_folder_selection(&self, node: &mut FileNode, new_state: FileState) {
        // Logic moved from the old free function
        node.state = new_state;
        if node.is_dir {
            for child in node.children.iter_mut() {
                self.update_folder_selection(child, new_state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{FileNode, FileState, Profile}; // Ensure full path if needed
    use std::collections::HashSet;
    use std::path::PathBuf;

    // Helper to create a simple tree for testing
    fn create_test_tree() -> Vec<FileNode> {
        vec![
            FileNode {
                path: PathBuf::from("/root/file1.txt"),
                name: "file1.txt".to_string(),
                is_dir: false,
                state: FileState::New,
                children: vec![],
                checksum: None,
            },
            FileNode {
                path: PathBuf::from("/root/dir1"),
                name: "dir1".to_string(),
                is_dir: true,
                state: FileState::New,
                children: vec![
                    FileNode {
                        path: PathBuf::from("/root/dir1/file2.txt"),
                        name: "file2.txt".to_string(),
                        is_dir: false,
                        state: FileState::New,
                        children: vec![],
                        checksum: None,
                    },
                    FileNode {
                        path: PathBuf::from("/root/dir1/subdir"),
                        name: "subdir".to_string(),
                        is_dir: true,
                        state: FileState::New,
                        children: vec![FileNode {
                            path: PathBuf::from("/root/dir1/subdir/file3.txt"),
                            name: "file3.txt".to_string(),
                            is_dir: false,
                            state: FileState::New,
                            children: vec![],
                            checksum: None,
                        }],
                        checksum: None,
                    },
                ],
                checksum: None,
            },
            FileNode {
                path: PathBuf::from("/root/file4.ext"), // Different extension
                name: "file4.ext".to_string(),
                is_dir: false,
                state: FileState::New,
                children: vec![],
                checksum: None,
            },
        ]
    }

    // Test helper for StateManagerOperations using CoreStateManager
    fn test_with_state_manager<F, R>(test_fn: F) -> R
    where
        F: FnOnce(&dyn StateManagerOperations) -> R,
    {
        let manager = CoreStateManager::new();
        test_fn(&manager)
    }

    #[test]
    fn test_core_state_manager_apply_profile_select_deselect() {
        // Arrange
        let manager = CoreStateManager::new();
        let mut tree = create_test_tree();
        let mut selected_paths = HashSet::new();
        let mut deselected_paths = HashSet::new();

        selected_paths.insert(PathBuf::from("/root/file1.txt"));
        selected_paths.insert(PathBuf::from("/root/dir1/subdir/file3.txt"));
        deselected_paths.insert(PathBuf::from("/root/dir1/file2.txt"));

        // Act
        manager.apply_profile_to_tree(&mut tree, &selected_paths, &deselected_paths);

        // Assert
        assert_eq!(tree[0].state, FileState::Selected); // file1.txt
        assert_eq!(tree[1].state, FileState::New); // dir1
        assert_eq!(tree[1].children[0].state, FileState::Deselected); // dir1/file2.txt
        assert_eq!(tree[1].children[1].state, FileState::New); // dir1/subdir
        assert_eq!(tree[1].children[1].children[0].state, FileState::Selected); // dir1/subdir/file3.txt
        assert_eq!(tree[2].state, FileState::New); // file4.ext
    }

    #[test]
    fn test_core_state_manager_apply_profile_reverts_to_new() {
        // Arrange
        let manager = CoreStateManager::new();
        let mut tree = create_test_tree();
        tree[0].state = FileState::Selected; // Pre-set state

        let mut selected_paths = HashSet::new();
        selected_paths.insert(PathBuf::from("/root/dir1/file2.txt"));
        let deselected_paths = HashSet::new();

        // Act
        manager.apply_profile_to_tree(&mut tree, &selected_paths, &deselected_paths);

        // Assert
        assert_eq!(tree[0].state, FileState::New); // Should revert to New as it's not in selected_paths
        assert_eq!(tree[1].children[0].state, FileState::Selected); // dir1/file2.txt should be selected
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_select_all() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();

            // Act
            manager.update_folder_selection(&mut tree[1], FileState::Selected);

            // Assert
            assert_eq!(tree[1].state, FileState::Selected);
            assert_eq!(tree[1].children[0].state, FileState::Selected);
            assert_eq!(tree[1].children[1].state, FileState::Selected);
            assert_eq!(tree[1].children[1].children[0].state, FileState::Selected);
            assert_eq!(tree[0].state, FileState::New); // Other nodes unaffected
        });
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_deselect_all() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();
            manager.update_folder_selection(&mut tree[1], FileState::Selected); // Pre-select

            // Act
            manager.update_folder_selection(&mut tree[1], FileState::Deselected); // Then deselect

            // Assert
            assert_eq!(tree[1].state, FileState::Deselected);
            assert_eq!(tree[1].children[0].state, FileState::Deselected);
            assert_eq!(tree[1].children[1].state, FileState::Deselected);
            assert_eq!(tree[1].children[1].children[0].state, FileState::Deselected);
        });
    }

    #[test]
    fn test_core_state_manager_update_folder_selection_on_file_node() {
        test_with_state_manager(|manager| {
            // Arrange
            let mut tree = create_test_tree();

            // Act & Assert for Selected
            manager.update_folder_selection(&mut tree[0], FileState::Selected);
            assert_eq!(tree[0].state, FileState::Selected);

            // Act & Assert for Deselected
            manager.update_folder_selection(&mut tree[0], FileState::Deselected);
            assert_eq!(tree[0].state, FileState::Deselected);

            // Children should remain empty as it's a file node
            assert_eq!(tree[0].children.len(), 0);
        });
    }
}
