use serde::{Deserialize, Serialize}; // For Profile serialization
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

use crate::app_logic::handler::PathToTreeItemIdMap;
use crate::platform_layer::{CheckState, TreeItemDescriptor, TreeItemId};
/*
 * Represents the selection state of a file or folder.
 * Derives Serialize and Deserialize for potential future use if this enum is directly part of a complex state
 * (though current Profile doesn't serialize it directly). Default is added for convenience.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionState {
    Selected,
    Deselected,
    New,
}

impl Default for SelectionState {
    fn default() -> Self {
        SelectionState::New
    }
}

/*
 * Represents a node in the file system tree.
 * It's not directly serialized into profiles; instead, profiles store sets of selected/deselected paths.
 * This approach makes profiles more resilient to file system changes and simplifies serialization.
 */
#[derive(Debug, Clone, PartialEq)] // Not serializing FileNode directly; Profile stores paths.
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub state: SelectionState,
    pub children: Vec<FileNode>, // Children are only populated if is_dir is true
    checksum: String,            // Will be empty striong for directories and some unit tests.
}

impl FileNode {
    /*
     * Creates a new FileNode with default 'Unknown' state and no children.
     * This constructor initializes a basic representation of a file or directory entry
     * before its state is determined by user interaction or profile application.
     */
    pub fn new(path: PathBuf, name: String, is_dir: bool, checksum: String) -> Self {
        FileNode {
            path,
            name,
            is_dir,
            state: SelectionState::default(),
            children: Vec::new(),
            checksum: checksum,
        }
    }

    pub fn is_selected(&self) -> bool {
        self.state == SelectionState::Selected
    }

    pub fn new_file_token_details(&self, token_count: usize) -> FileTokenDetails {
        FileTokenDetails {
            checksum: self.checksum.clone(),
            token_count: token_count,
        }
    }

    fn new_tree_item_descriptor(
        &self,
        id: TreeItemId,
        children: Vec<TreeItemDescriptor>,
    ) -> TreeItemDescriptor {
        TreeItemDescriptor {
            id: id,
            is_folder: self.is_dir,
            children: children,
            text: self.name.clone(),
            state: match self.is_selected() {
                true => CheckState::Checked,
                false => CheckState::Unchecked,
            },
        }
    }

    #[cfg(test)]
    pub fn new_test(path: PathBuf, name: String, is_dir: bool) -> Self {
        FileNode {
            path,
            name,
            is_dir,
            state: SelectionState::default(),
            children: Vec::new(),
            checksum: "".to_string(),
        }
    }

    pub fn checksum_match(&self, file: Option<&FileTokenDetails>) -> bool {
        if let Some(details) = file {
            self.checksum == details.checksum
        } else {
            false
        }
    }

    /// Creates a new FileNode with all fields specified.
    #[cfg(test)]
    pub fn new_full(
        path: PathBuf,
        name: String,
        is_dir: bool,
        state: SelectionState,
        children: Vec<FileNode>,
        checksum: String,
    ) -> Self {
        FileNode {
            path,
            name,
            is_dir,
            state,
            children,
            checksum,
        }
    }

    pub fn build_tree_item_descriptors_recursive(
        nodes: &[FileNode],
        path_to_tree_item_id: &mut PathToTreeItemIdMap,
        next_tree_item_id_counter: &mut u64,
    ) -> Vec<TreeItemDescriptor> {
        let mut descriptors = Vec::new();
        for node in nodes {
            let id_val = *next_tree_item_id_counter;
            *next_tree_item_id_counter += 1;
            let item_id = TreeItemId(id_val);

            path_to_tree_item_id.insert(node.path.clone(), item_id);

            let children = Self::build_tree_item_descriptors_recursive(
                &node.children,
                path_to_tree_item_id,
                next_tree_item_id_counter,
            );
            let descriptor = node.new_tree_item_descriptor(item_id, children);
            descriptors.push(descriptor);
        }
        descriptors
    }

    /*
     * Builds `TreeItemDescriptor`s for nodes matching the provided filter text.
     * The filter is compared against file and folder names case-insensitively.
     * Parent folders of matching nodes are included so the resulting structure
     * still forms a valid tree that reveals where each match resides.
     */
    pub fn build_tree_item_descriptors_filtered(
        nodes: &[FileNode],
        filter_text: &str,
        path_to_tree_item_id: &mut PathToTreeItemIdMap,
        next_tree_item_id_counter: &mut u64,
    ) -> Vec<TreeItemDescriptor> {
        let filter_lower = filter_text.to_lowercase();

        fn recurse(
            nodes: &[FileNode],
            filter_lower: &str,
            map: &mut PathToTreeItemIdMap,
            counter: &mut u64,
        ) -> Vec<TreeItemDescriptor> {
            let mut descriptors = Vec::new();
            for node in nodes {
                let children = recurse(&node.children, filter_lower, map, counter);
                let name_matches = node.name.to_lowercase() == *filter_lower;
                if name_matches || !children.is_empty() {
                    let id_val = *counter;
                    *counter += 1;
                    let item_id = TreeItemId(id_val);
                    map.insert(node.path.clone(), item_id);
                    let descriptor = node.new_tree_item_descriptor(item_id, children);
                    descriptors.push(descriptor);
                }
            }
            descriptors
        }

        recurse(
            nodes,
            &filter_lower,
            path_to_tree_item_id,
            next_tree_item_id_counter,
        )
    }
}

/*
 * Stores the checksum and token count for a single file.
 * This structure is used within the `Profile` to cache token count information,
 * allowing for faster token calculation by avoiding re-processing of unchanged files.
 */
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileTokenDetails {
    pub checksum: String,
    pub token_count: usize,
}

/*
 * Represents a user profile, storing selection states and configurations for a specific root folder.
 * This structure is serialized to/from JSON for persistence. It now includes an `archive_path`
 * to associate the profile directly with its output archive, and no longer contains whitelist patterns.
 * TODO: Shouldn't use pub for everything.
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
    /* Stores cached token counts and checksums for files.
     * The `#[serde(default)]` attribute ensures that profiles saved before this field existed can still be loaded. */
    #[serde(default)]
    pub file_details: HashMap<PathBuf, FileTokenDetails>,
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
            archive_path: None,
            file_details: HashMap::new(),
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
    use super::{FileNode, FileTokenDetails, Profile, SelectionState};
    use crate::platform_layer::{CheckState, TreeItemId};
    use std::collections::HashMap;
    use std::path::PathBuf; // Added for ArchiveStatus::ErrorChecking

    #[test]
    fn test_filenode_new_defaults() {
        let p = PathBuf::from("/tmp/foo");
        let n = FileNode::new_test(p.clone(), "foo".into(), false);
        assert_eq!(n.path, p);
        assert_eq!(n.name, "foo");
        assert_eq!(n.is_dir, false);
        assert_eq!(n.state, SelectionState::New);
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

    #[test]
    fn test_profile_serialization_with_file_details() {
        let mut profile = Profile::new("TestProfile".to_string(), PathBuf::from("/test/root"));
        profile.file_details.insert(
            PathBuf::from("/test/root/file1.txt"),
            FileTokenDetails {
                checksum: "cs1".to_string(),
                token_count: 100,
            },
        );
        let serialized = serde_json::to_string(&profile).unwrap();
        assert!(serialized.contains("file_details"));
        assert!(serialized.contains("file1.txt"));
        assert!(serialized.contains("cs1"));
        let deserialized: Profile = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.file_details.len(), 1);
    }

    #[test]
    fn test_build_tree_item_descriptors_recursive_internal() {
        let nodes = vec![
            FileNode::new_full(
                PathBuf::from("/file1.txt"),
                "file1.txt".into(),
                false,
                SelectionState::Selected,
                vec![],
                "".to_string(),
            ),
            FileNode::new_full(
                PathBuf::from("/dir1"),
                "dir1".into(),
                true,
                SelectionState::New,
                vec![FileNode::new_full(
                    PathBuf::from("/dir1/file2.txt"),
                    "file2.txt".into(),
                    false,
                    SelectionState::Deselected,
                    vec![],
                    "".to_string(),
                )],
                "".to_string(),
            ),
        ];
        let mut path_to_id_map = HashMap::new();
        let mut id_counter = 100; // Start from a non-zero value to make it distinct

        let descriptors = FileNode::build_tree_item_descriptors_recursive(
            &nodes,
            &mut path_to_id_map,
            &mut id_counter,
        );

        assert_eq!(descriptors.len(), 2);
        // File 1
        assert_eq!(descriptors[0].text, "file1.txt");
        assert_eq!(descriptors[0].id, TreeItemId(100));
        assert_eq!(descriptors[0].state, CheckState::Checked);
        assert_eq!(
            path_to_id_map.get(&PathBuf::from("/file1.txt")),
            Some(&TreeItemId(100))
        );
        // Dir 1
        assert_eq!(descriptors[1].text, "dir1");
        assert_eq!(descriptors[1].id, TreeItemId(101));
        assert_eq!(descriptors[1].is_folder, true);
        assert_eq!(descriptors[1].state, CheckState::Unchecked); // New/Deselected map to Unchecked
        assert_eq!(
            path_to_id_map.get(&PathBuf::from("/dir1")),
            Some(&TreeItemId(101))
        );
        // File 2 (in Dir1)
        assert_eq!(descriptors[1].children.len(), 1);
        assert_eq!(descriptors[1].children[0].text, "file2.txt");
        assert_eq!(descriptors[1].children[0].id, TreeItemId(102));
        assert_eq!(descriptors[1].children[0].state, CheckState::Unchecked);
        assert_eq!(
            path_to_id_map.get(&PathBuf::from("/dir1/file2.txt")),
            Some(&TreeItemId(102))
        );

        assert_eq!(id_counter, 103); // Counter should be next available ID
        assert_eq!(path_to_id_map.len(), 3);
    }

    #[test]
    fn test_build_tree_item_descriptors_filtered() {
        let dir = FileNode::new_full(
            PathBuf::from("/dir1"),
            "dir1".into(),
            true,
            SelectionState::New,
            vec![
                FileNode::new_full(
                    PathBuf::from("/dir1/match.txt"),
                    "match.txt".into(),
                    false,
                    SelectionState::Selected,
                    vec![],
                    "".to_string(),
                ),
                FileNode::new_full(
                    PathBuf::from("/dir1/other.txt"),
                    "other.txt".into(),
                    false,
                    SelectionState::Deselected,
                    vec![],
                    "".to_string(),
                ),
            ],
            "".to_string(),
        );
        let nodes = vec![
            FileNode::new_full(
                PathBuf::from("/root.txt"),
                "root.txt".into(),
                false,
                SelectionState::Deselected,
                vec![],
                "".to_string(),
            ),
            dir,
        ];

        let mut path_to_id_map = HashMap::new();
        let mut id_counter = 1;

        let descriptors = FileNode::build_tree_item_descriptors_filtered(
            &nodes,
            "match.txt",
            &mut path_to_id_map,
            &mut id_counter,
        );

        assert_eq!(descriptors.len(), 1); // Only dir1 should be top level
        assert_eq!(descriptors[0].text, "dir1");
        assert_eq!(descriptors[0].children.len(), 1);
        assert_eq!(descriptors[0].children[0].text, "match.txt");
        assert_eq!(path_to_id_map.len(), 2);

        // Case-insensitive check
        let mut map2 = HashMap::new();
        let mut counter2 = 1;
        let descriptors_ci = FileNode::build_tree_item_descriptors_filtered(
            &nodes,
            "MATCH.TXT",
            &mut map2,
            &mut counter2,
        );
        assert_eq!(descriptors_ci.len(), 1);
        assert_eq!(descriptors_ci[0].children.len(), 1);
    }
}
