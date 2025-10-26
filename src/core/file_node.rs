use serde::{Deserialize, Serialize}; // For Profile serialization
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::app_logic::{handler::PathToTreeItemIdMap, ui_constants};
use crate::platform_layer::{CheckState, TreeItemDescriptor, TreeItemId};
/*
 * Represents the selection state of a file or folder.
 * Derives Serialize and Deserialize for potential future use if this enum is directly part of a complex state
 * (though current Profile doesn't serialize it directly). Default is added for convenience.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionState {
    Selected,
    Deselected,
    #[default]
    New,
}

/*
 * Represents a node in the file system tree.
 * It's not directly serialized into profiles; instead, profiles store sets of selected/deselected paths.
 * This approach makes profiles more resilient to file system changes and simplifies serialization.
 */
#[derive(Debug, Clone, PartialEq)] // Not serializing FileNode directly; Profile stores paths.
pub struct FileNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    state: SelectionState,
    pub children: Vec<FileNode>, // Children are only populated if is_dir is true
    checksum: String,            // Will be empty string for directories and some unit tests.
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
            checksum,
        }
    }

    pub fn is_selected(&self) -> bool {
        self.state == SelectionState::Selected
    }

    pub fn state(&self) -> SelectionState {
        self.state
    }

    pub fn set_state(&mut self, new_state: SelectionState) {
        self.state = new_state;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub fn new_file_token_details(&self, token_count: usize) -> FileTokenDetails {
        FileTokenDetails {
            checksum: self.checksum.clone(),
            token_count,
        }
    }

    fn new_tree_item_descriptor(
        &self,
        id: TreeItemId,
        children: Vec<TreeItemDescriptor>,
        display_new_indicator: bool,
    ) -> TreeItemDescriptor {
        let mut text = self.name.clone();
        if display_new_indicator {
            text.push(' ');
            text.push(ui_constants::NEW_ITEM_INDICATOR_CHAR);
        }

        TreeItemDescriptor {
            id,
            is_folder: self.is_dir,
            children,
            text,
            state: match self.is_selected() {
                true => CheckState::Checked,
                false => CheckState::Unchecked,
            },
        }
    }

    /*
     * Determines whether the UI should render the "new item" indicator for this node.
     * Files return true when their selection state is `New`, while directories surface the
     * indicator only when at least one descendant file is still marked `New`.
     */
    fn should_display_new_indicator(&self) -> bool {
        if !self.is_dir {
            return self.state == SelectionState::New;
        }

        self.children
            .iter()
            .any(|child| child.should_display_new_indicator())
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

            path_to_tree_item_id.insert(node.path().to_path_buf(), item_id);

            let children = Self::build_tree_item_descriptors_recursive(
                &node.children,
                path_to_tree_item_id,
                next_tree_item_id_counter,
            );
            let display_new_indicator = node.should_display_new_indicator();
            let descriptor =
                node.new_tree_item_descriptor(item_id, children, display_new_indicator);
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
        let mut pattern = filter_text.to_lowercase();
        if !pattern.contains('*') && !pattern.contains('?') {
            pattern = format!("*{pattern}*");
        }
        let normalized_pattern = pattern.clone();
        log::debug!(
            "FileNode::build_tree_item_descriptors_filtered invoked with pattern {:?} and {} top-level nodes.",
            normalized_pattern,
            nodes.len()
        );
        let glob =
            glob::Pattern::new(&pattern).unwrap_or_else(|_| glob::Pattern::new("*").unwrap());

        fn recurse(
            nodes: &[FileNode],
            glob: &glob::Pattern,
            map: &mut PathToTreeItemIdMap,
            counter: &mut u64,
        ) -> Vec<(TreeItemDescriptor, bool)> {
            let mut descriptors = Vec::new();
            for node in nodes {
                let child_results = recurse(&node.children, glob, map, counter);
                let mut children = Vec::with_capacity(child_results.len());
                let mut visible_children_have_new = false;
                for (child_descriptor, child_has_visible_new_indicator) in child_results {
                    if child_has_visible_new_indicator {
                        visible_children_have_new = true;
                    }
                    children.push(child_descriptor);
                }

                let name_lower = node.name().to_lowercase();
                let name_matches = glob.matches(&name_lower);
                if name_matches || !children.is_empty() {
                    let id_val = *counter;
                    *counter += 1;
                    let item_id = TreeItemId(id_val);
                    map.insert(node.path().to_path_buf(), item_id);
                    let display_new_indicator = if node.is_dir {
                        visible_children_have_new
                    } else {
                        node.state == SelectionState::New
                    };
                    let descriptor =
                        node.new_tree_item_descriptor(item_id, children, display_new_indicator);
                    descriptors.push((descriptor, display_new_indicator));
                }
            }
            descriptors
        }

        let results = recurse(
            nodes,
            &glob,
            path_to_tree_item_id,
            next_tree_item_id_counter,
        );
        results
            .into_iter()
            .map(|(descriptor, _)| descriptor)
            .collect()
    }

    pub fn build_tree_item_descriptors_from_matches(
        nodes: &[FileNode],
        matches: &HashSet<PathBuf>,
        path_to_tree_item_id: &mut PathToTreeItemIdMap,
        next_tree_item_id_counter: &mut u64,
    ) -> Vec<TreeItemDescriptor> {
        fn build_for_node(
            node: &FileNode,
            matches: &HashSet<PathBuf>,
            map: &mut PathToTreeItemIdMap,
            counter: &mut u64,
        ) -> Option<(TreeItemDescriptor, bool)> {
            let mut child_descriptors = Vec::new();
            let mut visible_children_have_new = false;
            for child in &node.children {
                if let Some((child_descriptor, child_has_visible_new)) =
                    build_for_node(child, matches, map, counter)
                {
                    if child_has_visible_new {
                        visible_children_have_new = true;
                    }
                    child_descriptors.push(child_descriptor);
                }
            }

            let node_matches = matches.contains(node.path());
            let include_node = node_matches || !child_descriptors.is_empty();
            if !include_node {
                return None;
            }

            let display_new_indicator = if node.is_dir {
                visible_children_have_new
            } else {
                node.state == SelectionState::New
            };

            let item_id = TreeItemId(*counter);
            *counter += 1;
            map.insert(node.path().to_path_buf(), item_id);

            let descriptor =
                node.new_tree_item_descriptor(item_id, child_descriptors, display_new_indicator);
            Some((descriptor, display_new_indicator))
        }

        let mut descriptors = Vec::new();
        for node in nodes {
            if let Some((descriptor, _)) = build_for_node(
                node,
                matches,
                path_to_tree_item_id,
                next_tree_item_id_counter,
            ) {
                descriptors.push(descriptor);
            }
        }
        descriptors
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
 * to associate the profile directly with its output archive and `exclude_patterns` that mirror
 * gitignore-style filters for omitting files from scans.
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
    /* Patterns describing files/folders that should be ignored during tree scans.
     * The `#[serde(default)]` attribute preserves compatibility with profiles saved before patterns existed. */
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
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
            exclude_patterns: Vec::new(),
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
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf}; // Added for ArchiveStatus::ErrorChecking

    #[test]
    fn test_filenode_new_defaults() {
        let p = PathBuf::from("/tmp/foo");
        let n = FileNode::new_test(p.clone(), "foo".into(), false);
        assert_eq!(n.path(), p.as_path());
        assert_eq!(n.name(), "foo");
        assert!(!n.is_dir());
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
        assert!(profile.file_details.is_empty());
        assert!(profile.exclude_patterns.is_empty());
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
        assert!(deserialized.exclude_patterns.is_empty());
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
        assert!(descriptors[1].is_folder);
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

        // Substring filter should match "match.txt"
        let descriptors = FileNode::build_tree_item_descriptors_filtered(
            &nodes,
            "match",
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
            "MATCH",
            &mut map2,
            &mut counter2,
        );
        assert_eq!(descriptors_ci.len(), 1);
        assert_eq!(descriptors_ci[0].children.len(), 1);

        // Wildcard pattern should match "other.txt"
        let mut map3 = HashMap::new();
        let mut counter3 = 1;
        let descriptors_wc = FileNode::build_tree_item_descriptors_filtered(
            &nodes,
            "*other*",
            &mut map3,
            &mut counter3,
        );
        assert_eq!(descriptors_wc.len(), 1);
        assert_eq!(descriptors_wc[0].children.len(), 1);
        assert_eq!(descriptors_wc[0].children[0].text, "other.txt");
    }

    #[test]
    fn test_build_tree_item_descriptors_from_matches() {
        crate::initialize_logging();
        let mut root = FileNode::new_full(
            PathBuf::from("/root"),
            "root".into(),
            true,
            SelectionState::Selected,
            Vec::new(),
            "ck-root".into(),
        );
        let mut src_dir = FileNode::new_full(
            PathBuf::from("/root/src"),
            "src".into(),
            true,
            SelectionState::Selected,
            Vec::new(),
            "ck-src".into(),
        );
        src_dir.children.push(FileNode::new_full(
            PathBuf::from("/root/src/lib.rs"),
            "lib.rs".into(),
            false,
            SelectionState::Selected,
            Vec::new(),
            "ck-lib".into(),
        ));
        src_dir.children.push(FileNode::new_full(
            PathBuf::from("/root/src/main.rs"),
            "main.rs".into(),
            false,
            SelectionState::Selected,
            Vec::new(),
            "ck-main".into(),
        ));
        root.children.push(src_dir);
        root.children.push(FileNode::new_full(
            PathBuf::from("/root/README.md"),
            "README.md".into(),
            false,
            SelectionState::Selected,
            Vec::new(),
            "ck-readme".into(),
        ));

        let mut path_to_id_map = HashMap::new();
        let mut counter = 1;
        let matches = HashSet::from([
            PathBuf::from("/root/src/lib.rs"),
            PathBuf::from("/root/README.md"),
        ]);

        let descriptors = FileNode::build_tree_item_descriptors_from_matches(
            &[root],
            &matches,
            &mut path_to_id_map,
            &mut counter,
        );

        assert_eq!(descriptors.len(), 1);
        let root_descriptor = &descriptors[0];
        assert_eq!(root_descriptor.text, "root");
        assert_eq!(root_descriptor.children.len(), 2);

        let src_descriptor = &root_descriptor.children[0];
        assert_eq!(src_descriptor.text, "src");
        assert_eq!(src_descriptor.children.len(), 1);
        assert_eq!(src_descriptor.children[0].text, "lib.rs");

        let readme_descriptor = &root_descriptor.children[1];
        assert_eq!(readme_descriptor.text, "README.md");
        assert!(readme_descriptor.children.is_empty());

        assert!(path_to_id_map.contains_key(Path::new("/root")));
        assert!(path_to_id_map.contains_key(Path::new("/root/src")));
        assert!(path_to_id_map.contains_key(Path::new("/root/src/lib.rs")));
        assert!(path_to_id_map.contains_key(Path::new("/root/README.md")));
        assert!(!path_to_id_map.contains_key(Path::new("/root/src/main.rs")));
    }

    #[test]
    fn test_filtered_parent_excludes_hidden_new_indicator() {
        // [FileSelStateNewV3] Verify hidden "New" descendants do not add the indicator to parents.
        // Arrange
        let visible_child = FileNode::new_full(
            PathBuf::from("/parent/visible.txt"),
            "visible.txt".into(),
            false,
            SelectionState::Selected,
            vec![],
            "".to_string(),
        );
        let hidden_new_child = FileNode::new_full(
            PathBuf::from("/parent/hidden_new.txt"),
            "hidden_new.txt".into(),
            false,
            SelectionState::New,
            vec![],
            "".to_string(),
        );
        let parent = FileNode::new_full(
            PathBuf::from("/parent"),
            "parent".into(),
            true,
            SelectionState::Deselected,
            vec![visible_child, hidden_new_child],
            "".to_string(),
        );
        let mut path_to_id_map = HashMap::new();
        let mut id_counter = 1;

        // Act
        let descriptors = FileNode::build_tree_item_descriptors_filtered(
            &[parent],
            "visible",
            &mut path_to_id_map,
            &mut id_counter,
        );

        // Assert
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].text, "parent");
        assert_eq!(descriptors[0].children.len(), 1);
        assert_eq!(descriptors[0].children[0].text, "visible.txt");
        assert_eq!(path_to_id_map.len(), 2);
    }

    #[test]
    fn test_directory_with_new_state_but_no_children_has_no_indicator() {
        let dir = FileNode::new_full(
            PathBuf::from("/parent"),
            "parent".into(),
            true,
            SelectionState::New,
            Vec::new(),
            "".to_string(),
        );
        assert!(
            !dir.should_display_new_indicator(),
            "Directory without children should not report new indicator"
        );

        let mut path_to_id_map = HashMap::new();
        let mut id_counter = 1;
        let descriptors = FileNode::build_tree_item_descriptors_recursive(
            &[dir],
            &mut path_to_id_map,
            &mut id_counter,
        );

        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].text, "parent");
        assert!(descriptors[0].children.is_empty());
    }

    #[test]
    fn test_new_indicator_applied_to_new_file_and_parent() {
        // Arrange
        let new_child = FileNode::new_full(
            PathBuf::from("/parent/new_child.txt"),
            "new_child.txt".into(),
            false,
            SelectionState::New,
            vec![],
            "".to_string(),
        );
        let parent = FileNode::new_full(
            PathBuf::from("/parent"),
            "parent".into(),
            true,
            SelectionState::Deselected,
            vec![new_child],
            "".to_string(),
        );
        let nodes = vec![parent];
        let mut path_to_id_map = HashMap::new();
        let mut id_counter = 1;

        // Act
        let descriptors = FileNode::build_tree_item_descriptors_recursive(
            &nodes,
            &mut path_to_id_map,
            &mut id_counter,
        );

        // Assert
        assert_eq!(descriptors.len(), 1);
        assert_eq!(
            descriptors[0].text,
            format!(
                "parent {}",
                crate::app_logic::ui_constants::NEW_ITEM_INDICATOR_CHAR
            )
        );
        assert_eq!(descriptors[0].children.len(), 1);
        assert_eq!(
            descriptors[0].children[0].text,
            format!(
                "new_child.txt {}",
                crate::app_logic::ui_constants::NEW_ITEM_INDICATOR_CHAR
            )
        );
    }
}
