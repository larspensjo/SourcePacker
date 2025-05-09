use super::models::{FileNode, FileState, Profile};
use std::path::Path; // For comparing paths

/// Applies the selection states from a Profile to a tree of FileNodes.
///
/// - Sets `FileState::Selected` for paths in `profile.selected_paths`.
/// - Sets `FileState::Deselected` for paths in `profile.deselected_paths`.
/// - Other nodes remain `FileState::Unknown` (their default or current state).
///
/// This function modifies the `tree` in place.
pub fn apply_profile_to_tree(tree: &mut Vec<FileNode>, profile: &Profile) {
    for node in tree.iter_mut() {
        // Check if the current node's path is in the profile's selected_paths
        if profile.selected_paths.contains(&node.path) {
            node.state = FileState::Selected;
        }
        // Check if the current node's path is in the profile's deselected_paths
        // A path could theoretically be in both, deselected usually takes precedence or last one wins.
        // Let's assume if it's explicitly deselected, that overrides a selection.
        // Or, if already selected, don't change. For simplicity now, let deselected override.
        // A more robust model might prevent a path from being in both sets in the Profile struct.
        else if profile.deselected_paths.contains(&node.path) {
            node.state = FileState::Deselected;
        }
        // If not in either, it retains its current state (which should be Unknown if freshly scanned).
        // If a node was previously e.g. Selected but is no longer in selected_paths in the new profile,
        // and not in deselected_paths, it should revert to Unknown.
        // So, we should explicitly set to Unknown if not found in either.
        else {
            // This ensures that if a path was previously selected/deselected but is no longer
            // explicitly mentioned in the current profile's sets, it becomes Unknown.
            // This covers the case where a profile is loaded over an existing tree state.
            // For a freshly scanned tree, this just re-confirms the default.
            node.state = FileState::Unknown;
        }

        // Recursively apply to children if it's a directory
        if node.is_dir && !node.children.is_empty() {
            apply_profile_to_tree(&mut node.children, profile);
        }
    }
}

/// Updates the selection state of a folder and all its children recursively.
///
/// - If `select` is true, all items are set to `FileState::Selected`.
/// - If `select` is false, all items are set to `FileState::Deselected`.
///
/// This function modifies the `node` and its children in place.
pub fn update_folder_selection(node: &mut FileNode, new_state: FileState) {
    // A folder cannot be in an "Unknown" state after explicit user interaction.
    // If the user action is to set it to "Unknown", that might mean reverting to profile state,
    // which is a different operation. This function is for direct selection/deselection.
    if new_state == FileState::Unknown {
        // This case should ideally not be called directly with Unknown from typical UI.
        // If it is, we might want to set the folder to Unknown and leave children,
        // or set all to Unknown. For now, let's assume this means select/deselect.
        // For simplicity, let's restrict this function to Selected/Deselected.
        // The caller should handle "tristate" logic leading to Unknown differently.
        // However, to match the plan's intention of setting all children, we'll proceed,
        // but note that directly setting a folder and its children to "Unknown" is unusual.
        // Let's re-interpret `select: bool` from the plan as `make_selected: bool`.
        // The plan states "select: bool". Let's rename to `make_selected`.
        // For now, I will use `new_state: FileState` as it's more flexible.
    }

    node.state = new_state;

    if node.is_dir {
        for child in node.children.iter_mut() {
            update_folder_selection(child, new_state); // Recursively apply the same state
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
                state: FileState::Unknown,
                children: vec![],
            },
            FileNode {
                path: PathBuf::from("/root/dir1"),
                name: "dir1".to_string(),
                is_dir: true,
                state: FileState::Unknown,
                children: vec![
                    FileNode {
                        path: PathBuf::from("/root/dir1/file2.txt"),
                        name: "file2.txt".to_string(),
                        is_dir: false,
                        state: FileState::Unknown,
                        children: vec![],
                    },
                    FileNode {
                        path: PathBuf::from("/root/dir1/subdir"),
                        name: "subdir".to_string(),
                        is_dir: true,
                        state: FileState::Unknown,
                        children: vec![FileNode {
                            path: PathBuf::from("/root/dir1/subdir/file3.txt"),
                            name: "file3.txt".to_string(),
                            is_dir: false,
                            state: FileState::Unknown,
                            children: vec![],
                        }],
                    },
                ],
            },
            FileNode {
                path: PathBuf::from("/root/file4.ext"), // Different extension
                name: "file4.ext".to_string(),
                is_dir: false,
                state: FileState::Unknown,
                children: vec![],
            },
        ]
    }

    #[test]
    fn test_apply_profile_select_deselect() {
        let mut tree = create_test_tree();
        let mut profile = Profile::new("test_profile".to_string(), PathBuf::from("/root"));

        profile
            .selected_paths
            .insert(PathBuf::from("/root/file1.txt"));
        profile
            .selected_paths
            .insert(PathBuf::from("/root/dir1/subdir/file3.txt"));
        profile
            .deselected_paths
            .insert(PathBuf::from("/root/dir1/file2.txt"));

        apply_profile_to_tree(&mut tree, &profile);

        // file1.txt should be Selected
        assert_eq!(tree[0].state, FileState::Selected);
        assert_eq!(tree[0].path, PathBuf::from("/root/file1.txt"));

        // dir1 itself is Unknown as it's not explicitly in selected/deselected
        assert_eq!(tree[1].state, FileState::Unknown);
        assert_eq!(tree[1].path, PathBuf::from("/root/dir1"));

        // dir1/file2.txt should be Deselected
        assert_eq!(tree[1].children[0].state, FileState::Deselected);
        assert_eq!(
            tree[1].children[0].path,
            PathBuf::from("/root/dir1/file2.txt")
        );

        // dir1/subdir itself is Unknown
        assert_eq!(tree[1].children[1].state, FileState::Unknown);
        assert_eq!(tree[1].children[1].path, PathBuf::from("/root/dir1/subdir"));

        // dir1/subdir/file3.txt should be Selected
        assert_eq!(tree[1].children[1].children[0].state, FileState::Selected);
        assert_eq!(
            tree[1].children[1].children[0].path,
            PathBuf::from("/root/dir1/subdir/file3.txt")
        );

        // file4.ext should be Unknown
        assert_eq!(tree[2].state, FileState::Unknown);
        assert_eq!(tree[2].path, PathBuf::from("/root/file4.ext"));
    }

    #[test]
    fn test_apply_profile_reverts_to_unknown() {
        let mut tree = create_test_tree();
        // Pre-set a state that should be overridden
        tree[0].state = FileState::Selected; // file1.txt is initially selected

        let mut profile = Profile::new("test_profile".to_string(), PathBuf::from("/root"));
        // Profile does NOT select file1.txt, nor deselect it.
        profile
            .selected_paths
            .insert(PathBuf::from("/root/dir1/file2.txt"));

        apply_profile_to_tree(&mut tree, &profile);

        // file1.txt should now be Unknown because it's not in the profile's sets
        assert_eq!(tree[0].state, FileState::Unknown);
        assert_eq!(tree[0].path, PathBuf::from("/root/file1.txt"));

        // dir1/file2.txt should be Selected
        assert_eq!(tree[1].children[0].state, FileState::Selected);
    }

    #[test]
    fn test_update_folder_selection_select_all() {
        let mut tree = create_test_tree();
        // Select the 'dir1' folder
        // tree[1] is dir1
        update_folder_selection(&mut tree[1], FileState::Selected);

        // dir1 itself should be Selected
        assert_eq!(tree[1].state, FileState::Selected);
        // dir1/file2.txt should be Selected
        assert_eq!(tree[1].children[0].state, FileState::Selected);
        // dir1/subdir should be Selected
        assert_eq!(tree[1].children[1].state, FileState::Selected);
        // dir1/subdir/file3.txt should be Selected
        assert_eq!(tree[1].children[1].children[0].state, FileState::Selected);

        // Other nodes should be unaffected
        assert_eq!(tree[0].state, FileState::Unknown); // file1.txt
    }

    #[test]
    fn test_update_folder_selection_deselect_all() {
        let mut tree = create_test_tree();
        // Pre-select everything under dir1 to test deselection
        update_folder_selection(&mut tree[1], FileState::Selected);
        assert_eq!(tree[1].children[1].children[0].state, FileState::Selected); // Sanity check

        // Now deselect dir1
        update_folder_selection(&mut tree[1], FileState::Deselected);

        // dir1 itself should be Deselected
        assert_eq!(tree[1].state, FileState::Deselected);
        // dir1/file2.txt should be Deselected
        assert_eq!(tree[1].children[0].state, FileState::Deselected);
        // dir1/subdir should be Deselected
        assert_eq!(tree[1].children[1].state, FileState::Deselected);
        // dir1/subdir/file3.txt should be Deselected
        assert_eq!(tree[1].children[1].children[0].state, FileState::Deselected);
    }

    #[test]
    fn test_update_folder_selection_on_file_node() {
        let mut tree = create_test_tree();
        // tree[0] is file1.txt
        update_folder_selection(&mut tree[0], FileState::Selected);
        assert_eq!(tree[0].state, FileState::Selected);

        update_folder_selection(&mut tree[0], FileState::Deselected);
        assert_eq!(tree[0].state, FileState::Deselected);

        // Check that children are not affected (it has no children, but good principle)
        assert_eq!(tree[0].children.len(), 0);
    }
}
