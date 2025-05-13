use serde::{Deserialize, Serialize}; // For Profile serialization
use std::collections::HashSet;
use std::path::PathBuf;

// Represents the selection state of a file or folder.
// Derives Serialize and Deserialize because if we ever decide to save a more complex state that includes this enum directly
// (though current Profile doesn't), it's ready. Default is added for convenience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileState {
    Selected,
    Deselected,
    Unknown,
}

impl Default for FileState {
    fn default() -> Self {
        FileState::Unknown
    }
}

// Represents a node in the file system tree.
// Only derives Debug and Clone. We are not directly serializing the FileNode tree structure into the profile.
// Instead, the Profile struct will store sets of paths that are selected or deselected. This avoids complexities
// with recursive serialization and makes profiles more resilient to file system changes (though we still need to handle missing paths).
#[derive(Debug, Clone)] // Not serializing FileNode directly; Profile stores paths.
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub state: FileState,
    pub children: Vec<FileNode>, // Children are only populated if is_dir is true
}

impl FileNode {
    /// Creates a new FileNode.
    pub fn new(path: PathBuf, name: String, is_dir: bool) -> Self {
        FileNode {
            path,
            name,
            is_dir,
            state: FileState::default(), // Initial state is Unknown
            children: Vec::new(),
        }
    }
}

// Represents a user profile, storing selection states and configurations.
// Derives Debug, Clone, Serialize, and Deserialize as this is the structure that will be saved to and loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub root_folder: PathBuf,
    // Store actual paths for selected/deselected items.
    // This is simpler than trying to persist the state of every node in a tree,
    // especially when the tree structure can change.
    pub selected_paths: HashSet<PathBuf>,
    pub deselected_paths: HashSet<PathBuf>,
    pub whitelist_patterns: Vec<String>,
}

impl Profile {
    /// Creates a new, empty profile for a given name and root folder.
    pub fn new(name: String, root_folder: PathBuf) -> Self {
        Profile {
            name,
            root_folder,
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            whitelist_patterns: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileNode, FileState};
    use std::path::{Path, PathBuf}; // Import FileNode and FileState into the tests module

    #[test]
    fn test_filenode_new_defaults() {
        let p = PathBuf::from("/tmp/foo");
        let n = FileNode::new(p.clone(), "foo".into(), false);
        assert_eq!(n.path, p);
        assert_eq!(n.name, "foo");
        assert_eq!(n.is_dir, false);
        assert_eq!(n.state, FileState::Unknown);
        assert!(n.children.is_empty());
    }
}
