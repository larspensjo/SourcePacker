use serde::{Deserialize, Serialize}; // For Profile serialization
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;

/*
 * Represents the selection state of a file or folder.
 * Derives Serialize and Deserialize for potential future use if this enum is directly part of a complex state
 * (though current Profile doesn't serialize it directly). Default is added for convenience.
 */
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

/*
 * Represents a node in the file system tree.
 * It's not directly serialized into profiles; instead, profiles store sets of selected/deselected paths.
 * This approach makes profiles more resilient to file system changes and simplifies serialization.
 */
#[derive(Debug, Clone)] // Not serializing FileNode directly; Profile stores paths.
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub state: FileState,
    pub children: Vec<FileNode>, // Children are only populated if is_dir is true
}

impl FileNode {
    /*
     * Creates a new FileNode with default 'Unknown' state and no children.
     * This constructor initializes a basic representation of a file or directory entry
     * before its state is determined by user interaction or profile application.
     */
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

/*
 * Represents a user profile, storing selection states and configurations for a specific root folder.
 * This structure is serialized to/from JSON for persistence. It now includes an `archive_path`
 * to associate the profile directly with its output archive, and no longer contains whitelist patterns.
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub root_folder: PathBuf,
    // Store actual paths for selected/deselected items.
    // This is simpler than trying to persist the state of every node in a tree,
    // especially when the tree structure can change.
    pub selected_paths: HashSet<PathBuf>,
    pub deselected_paths: HashSet<PathBuf>,
    pub archive_path: Option<PathBuf>,
}

impl Profile {
    /*
     * Creates a new, empty profile for a given name and root folder.
     * Initializes with empty selection sets and no specific archive path. The archive path
     * will typically be set later, either by user interaction or when loading a profile.
     */
    pub fn new(name: String, root_folder: PathBuf) -> Self {
        Profile {
            name,
            root_folder,
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: None, // New profiles start without an archive path
        }
    }
}

/*
 * Represents the synchronization status of a profile's archive file.
 * This enum indicates whether the archive is up-to-date with selected source files,
 * needs regeneration, or if there were issues determining its status.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveStatus {
    UpToDate,
    OutdatedRequiresUpdate,
    NotYetGenerated,                      // Profile has no archive_path associated
    ArchiveFileMissing,                   // archive_path is set, but file doesn't exist
    NoFilesSelected, // No files are selected, so archive status is moot or "up to date" by default.
    ErrorChecking(Option<io::ErrorKind>), // An I/O error occurred (optional: kind of error)
}

#[cfg(test)]
mod tests {
    use super::{FileNode, FileState, Profile};
    use std::io;
    use std::path::PathBuf; // Added for ArchiveStatus::ErrorChecking

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

    #[test]
    fn test_profile_new_defaults() {
        let profile_name = "TestProfile".to_string();
        let root_path = PathBuf::from("/test/root");
        let profile = Profile::new(profile_name.clone(), root_path.clone());

        assert_eq!(profile.name, profile_name);
        assert_eq!(profile.root_folder, root_path);
        assert!(profile.selected_paths.is_empty());
        assert!(profile.deselected_paths.is_empty());
        assert_eq!(profile.archive_path, None);
    }
}
