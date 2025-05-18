use super::models::FileNode;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/*
 * Defines custom error types for file system operations.
 * This enum centralizes error handling for directory scanning, I/O issues,
 * and path validity, providing more specific error information than generic I/O errors.
 */
#[derive(Debug)]
pub enum FileSystemError {
    Io(io::Error),
    WalkDir(walkdir::Error),
    InvalidPath(PathBuf),
}

impl From<io::Error> for FileSystemError {
    fn from(err: io::Error) -> Self {
        FileSystemError::Io(err)
    }
}

impl From<walkdir::Error> for FileSystemError {
    fn from(err: walkdir::Error) -> Self {
        FileSystemError::WalkDir(err)
    }
}

impl std::fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemError::Io(e) => write!(f, "I/O error: {}", e),
            FileSystemError::WalkDir(e) => write!(f, "Directory traversal error: {}", e),
            FileSystemError::InvalidPath(p) => write!(f, "Invalid path: {:?}", p),
        }
    }
}

impl std::error::Error for FileSystemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FileSystemError::Io(e) => Some(e),
            FileSystemError::WalkDir(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, FileSystemError>;

/*
 * Scans a directory recursively and builds a tree of FileNode objects representing all files and subdirectories.
 * This function traverses the specified root_path and constructs a hierarchical representation.
 * All discovered files and directories are included in the resulting tree.
 * The tree is sorted such that directories appear before files at each level, and then alphabetically.
 */
pub fn scan_directory(root_path: &Path) -> Result<Vec<FileNode>> {
    if !root_path.is_dir() {
        return Err(FileSystemError::InvalidPath(root_path.to_path_buf()));
    }

    let mut nodes_map: HashMap<PathBuf, FileNode> = HashMap::new();
    // Store paths in WalkDir's discovery order (parent before children, depth-first).
    // Adding .sort_by_file_name() to WalkDir ensures consistent ordering for items at the same level,
    // which can be helpful for reproducibility, though the tree construction logic itself
    // doesn't strictly depend on this same-level sort.
    let mut entry_paths_in_discovery_order: Vec<PathBuf> = Vec::new();

    // Phase 1: Discover all entries. Create FileNode for each (with empty children).
    // Store them in `nodes_map` and their paths in `entry_paths_in_discovery_order`.
    // `min_depth(1)` ensures we process items *inside* root_path, not root_path itself.
    for entry_result in WalkDir::new(root_path).min_depth(1).sort_by_file_name() {
        let entry = entry_result?; // Propagate WalkDir errors
        let path = entry.path().to_path_buf();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_dir();

        let node = FileNode::new(path.clone(), name, is_dir);
        nodes_map.insert(path.clone(), node);
        entry_paths_in_discovery_order.push(path);
    }

    // Phase 2: Build the tree structure.
    // Iterate through discovered paths in *reverse* order.
    // This ensures that deeper children are processed first and moved into their direct parents'
    // `children` Vec. The parent nodes are mutated in `nodes_map`.
    for child_path_ref in entry_paths_in_discovery_order.iter().rev() {
        if let Some(parent_path) = child_path_ref.parent() {
            // We only link children to parents that are *not* the initial `root_path`
            // (as `root_path` itself isn't in `nodes_map`) and are part of the scan.
            if parent_path != root_path {
                // If the child_path is still in the map (i.e., it hasn't been moved to a parent yet)
                if let Some(child_node_owned) = nodes_map.remove(child_path_ref) {
                    // The parent_path should also be in nodes_map at this point because it appeared
                    // earlier in WalkDir's sequence and would be processed later in this reverse iteration.
                    if let Some(parent_node_mut) = nodes_map.get_mut(parent_path) {
                        parent_node_mut.children.push(child_node_owned);
                    } else {
                        // This case should ideally not be reached if logic is sound.
                        // It would mean parent_path was not found in nodes_map, which is unexpected
                        // if parent_path is not root_path and was yielded by WalkDir.
                        // To prevent losing the child node, put it back.
                        nodes_map.insert(child_path_ref.clone(), child_node_owned);
                        // eprintln!(
                        //     "Warning: Parent {} for child {} not found in map during tree build. Child re-added.",
                        //     parent_path.display(), child_path_ref.display()
                        // );
                    }
                }
                // If `nodes_map.remove(child_path_ref)` returned None, it means `child_path_ref` was
                // already removed and added to its parent (e.g., a grandchild moved into a child). This is fine.
            }
        }
    }

    // Phase 3: Collect top-level nodes.
    // After Phase 2, `nodes_map` only contains nodes whose parent is `root_path` (i.e., top-level nodes).
    // All other nodes have been moved into their respective parent's `children` Vec.
    let mut top_level_nodes: Vec<FileNode> = nodes_map.into_values().collect();

    // Phase 4: Sort all node lists recursively.
    // This sorts the top_level_nodes list and, for each directory, its children list, and so on.
    sort_file_nodes_recursively(&mut top_level_nodes);

    Ok(top_level_nodes)
}

fn sort_file_nodes_recursively(nodes: &mut Vec<FileNode>) {
    nodes.sort_by(|a, b| {
        if a.is_dir && !b.is_dir {
            std::cmp::Ordering::Less // Directories first
        } else if !a.is_dir && b.is_dir {
            std::cmp::Ordering::Greater // Files after directories
        } else {
            a.name.cmp(&b.name) // Then sort alphabetically by name
        }
    });

    // Recursively sort children of directories
    for node in nodes.iter_mut() {
        if node.is_dir && !node.children.is_empty() {
            sort_file_nodes_recursively(&mut node.children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    fn setup_test_dir(base_path: &Path) -> io::Result<()> {
        fs::create_dir_all(base_path.join("src"))?;
        fs::create_dir_all(base_path.join("doc"))?;
        fs::create_dir_all(base_path.join("empty_dir"))?;
        fs::create_dir_all(base_path.join("src").join("sub_src"))?;

        File::create(base_path.join("src/main.rs"))?.sync_all()?;
        File::create(base_path.join("src/lib.rs"))?.sync_all()?;
        File::create(base_path.join("src/sub_src/deep.rs"))?.sync_all()?;
        File::create(base_path.join("doc/README.md"))?.sync_all()?;
        File::create(base_path.join("LICENSE.txt"))?.sync_all()?;
        File::create(base_path.join("root_file.toml"))?.sync_all()?;
        Ok(())
    }

    #[test]
    fn test_scan_all_files_and_dirs_when_no_filtering() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let nodes = scan_directory(dir.path())?;
        // Expected sorted order: doc (d), empty_dir (d), src (d), LICENSE.txt (f), root_file.toml (f)
        assert_eq!(
            nodes.len(),
            5,
            "Scan should return all top-level items. Found names: {:?}",
            nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );

        let names: Vec<&String> = nodes.iter().map(|n| &n.name).collect();
        assert_eq!(
            names,
            vec!["doc", "empty_dir", "src", "LICENSE.txt", "root_file.toml"]
        );

        assert!(nodes.iter().any(|n| n.name == "doc" && n.is_dir));
        assert!(nodes.iter().any(|n| n.name == "empty_dir" && n.is_dir));
        assert!(nodes.iter().any(|n| n.name == "LICENSE.txt" && !n.is_dir));
        assert!(
            nodes
                .iter()
                .any(|n| n.name == "root_file.toml" && !n.is_dir)
        );
        assert!(nodes.iter().any(|n| n.name == "src" && n.is_dir));
        Ok(())
    }

    #[test]
    fn test_scan_includes_all_items_in_structure() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let nodes = scan_directory(dir.path())?;

        // Check top-level items
        assert_eq!(nodes.len(), 5);

        let src_node = nodes
            .iter()
            .find(|n| n.name == "src" && n.is_dir)
            .unwrap_or_else(|| {
                panic!(
                    "src node not found. Top level: {:?}",
                    nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
                )
            });
        // Expected in src (sorted): sub_src (d), lib.rs (f), main.rs (f)
        assert_eq!(
            src_node.children.len(),
            3,
            "src children mismatch. Found: {:?}",
            src_node
                .children
                .iter()
                .map(|n| &n.name)
                .collect::<Vec<_>>()
        );

        let src_children_names: Vec<&String> = src_node.children.iter().map(|n| &n.name).collect();
        assert_eq!(src_children_names, vec!["sub_src", "lib.rs", "main.rs"]);

        assert!(
            src_node
                .children
                .iter()
                .any(|n| n.name == "lib.rs" && !n.is_dir)
        );
        assert!(
            src_node
                .children
                .iter()
                .any(|n| n.name == "main.rs" && !n.is_dir)
        );
        let sub_src_node = src_node
            .children
            .iter()
            .find(|n| n.name == "sub_src" && n.is_dir)
            .unwrap();
        // Expected in sub_src (sorted): deep.rs (f)
        assert_eq!(
            sub_src_node.children.len(),
            1,
            "sub_src children count mismatch. Found: {:?}",
            sub_src_node
                .children
                .iter()
                .map(|n| &n.name)
                .collect::<Vec<_>>()
        );
        assert_eq!(sub_src_node.children[0].name, "deep.rs");

        let doc_node = nodes.iter().find(|n| n.name == "doc" && n.is_dir).unwrap();
        // Expected in doc (sorted): README.md (f)
        assert_eq!(doc_node.children.len(), 1);
        assert_eq!(doc_node.children[0].name, "README.md");

        let empty_dir_node = nodes
            .iter()
            .find(|n| n.name == "empty_dir" && n.is_dir)
            .unwrap();
        assert!(empty_dir_node.children.is_empty());

        Ok(())
    }

    #[test]
    fn test_scan_complex_structure_returns_all() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let nodes = scan_directory(dir.path())?;

        assert_eq!(
            nodes.len(),
            5,
            "Expected 5 top-level entries. Found: {:?}",
            nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );

        let src_node = nodes.iter().find(|n| n.name == "src").unwrap();
        assert!(src_node.is_dir);
        // Expected in src (sorted): sub_src (d), lib.rs (f), main.rs (f)
        assert_eq!(
            src_node.children.len(),
            3,
            "src should have 3 children. Found: {:?}",
            src_node
                .children
                .iter()
                .map(|n| &n.name)
                .collect::<Vec<_>>()
        );
        assert!(src_node.children.iter().any(|c| c.name == "main.rs"));
        assert!(src_node.children.iter().any(|c| c.name == "lib.rs"));

        let sub_src_node = src_node
            .children
            .iter()
            .find(|c| c.name == "sub_src")
            .unwrap();
        assert!(sub_src_node.is_dir);
        // Expected in sub_src (sorted): deep.rs (f)
        assert_eq!(
            sub_src_node.children.len(),
            1,
            "sub_src should have deep.rs"
        );
        assert_eq!(sub_src_node.children[0].name, "deep.rs");

        let doc_node = nodes.iter().find(|n| n.name == "doc").unwrap();
        assert!(doc_node.is_dir);
        // Expected in doc (sorted): README.md (f)
        assert_eq!(doc_node.children.len(), 1, "doc should have README.md");
        assert_eq!(doc_node.children[0].name, "README.md");

        assert!(nodes.iter().any(|n| n.name == "LICENSE.txt" && !n.is_dir));
        assert!(
            nodes
                .iter()
                .any(|n| n.name == "root_file.toml" && !n.is_dir)
        );

        let empty_dir_node = nodes.iter().find(|n| n.name == "empty_dir").unwrap();
        assert!(empty_dir_node.is_dir);
        assert!(
            empty_dir_node.children.is_empty(),
            "empty_dir should have no children"
        );
        Ok(())
    }

    #[test]
    fn test_scan_includes_empty_dirs_correctly() -> Result<()> {
        let dir = tempdir()?;
        fs::create_dir_all(dir.path().join("parent/empty_child"))?;
        File::create(dir.path().join("parent/file.txt"))?.sync_all()?;
        fs::create_dir_all(dir.path().join("another_empty_top_level_dir"))?;

        let nodes = scan_directory(dir.path())?;
        // Expected sorted: another_empty_top_level_dir (d), parent (d)
        assert_eq!(
            nodes.len(),
            2,
            "Expected 2 top-level entries. Found: {:?}",
            nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );

        let top_level_names: Vec<&String> = nodes.iter().map(|n| &n.name).collect();
        assert_eq!(
            top_level_names,
            vec!["another_empty_top_level_dir", "parent"]
        );

        let parent_node = nodes
            .iter()
            .find(|n| n.name == "parent")
            .expect("Should find 'parent' dir");
        assert!(parent_node.is_dir);
        // Expected sorted: empty_child (d), file.txt (f)
        assert_eq!(
            parent_node.children.len(),
            2,
            "Expected 'empty_child' and 'file.txt' in 'parent'. Found: {:?}",
            parent_node
                .children
                .iter()
                .map(|n| &n.name)
                .collect::<Vec<_>>()
        );
        let parent_children_names: Vec<&String> =
            parent_node.children.iter().map(|n| &n.name).collect();
        assert_eq!(parent_children_names, vec!["empty_child", "file.txt"]);

        assert!(
            parent_node
                .children
                .iter()
                .any(|c| c.name == "file.txt" && !c.is_dir)
        );
        assert!(
            parent_node
                .children
                .iter()
                .any(|c| c.name == "empty_child" && c.is_dir)
        );

        let another_empty_node = nodes
            .iter()
            .find(|n| n.name == "another_empty_top_level_dir")
            .expect("Should find 'another_empty_top_level_dir'");
        assert!(another_empty_node.is_dir);
        assert!(another_empty_node.children.is_empty());
        Ok(())
    }

    #[test]
    fn test_invalid_root_path() {
        let non_existent_path = Path::new("this_path_does_not_exist_hopefully");
        let result = scan_directory(non_existent_path);
        assert!(matches!(result, Err(FileSystemError::InvalidPath(_))));
    }
}
