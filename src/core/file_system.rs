use super::models::FileNode;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/*
 * This module provides functionalities for interacting with the file system,
 * primarily focusing on scanning directory structures. It defines errors specific
 * to these operations, a trait `FileSystemScannerOperations` for abstracting
 * scanning logic, and a concrete implementation `CoreFileSystemScanner`.
 */

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
 * Defines the operations for scanning file systems.
 * This trait abstracts the specific mechanisms for traversing directories and
 * building a representation of the file structure, typically as a tree of `FileNode` objects.
 * It allows for different implementations (e.g., file-system-based, mock) to be used.
 */
pub trait FileSystemScannerOperations: Send + Sync {
    /*
     * Scans a directory recursively and builds a tree of FileNode objects.
     * Implementations should traverse the specified `root_path` and construct a
     * hierarchical representation. All discovered files and directories should be
     * included. The tree is typically sorted for consistent presentation.
     */
    fn scan_directory(&self, root_path: &Path) -> Result<Vec<FileNode>>;
}

/*
 * The core implementation of `FileSystemScannerOperations`.
 * This struct handles the actual file system traversal and `FileNode` tree construction
 * using the `walkdir` crate.
 */
pub struct CoreFileSystemScanner {}

impl CoreFileSystemScanner {
    /*
     * Creates a new instance of `CoreFileSystemScanner`.
     * This constructor doesn't require any parameters.
     */
    pub fn new() -> Self {
        CoreFileSystemScanner {}
    }
}

impl Default for CoreFileSystemScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystemScannerOperations for CoreFileSystemScanner {
    /*
     * Scans a directory recursively and builds a tree of FileNode objects representing all files and subdirectories.
     * This function traverses the specified root_path and constructs a hierarchical representation.
     * All discovered files and directories are included in the resulting tree.
     * The tree is sorted such that directories appear before files at each level, and then alphabetically.
     */
    fn scan_directory(&self, root_path: &Path) -> Result<Vec<FileNode>> {
        if !root_path.is_dir() {
            return Err(FileSystemError::InvalidPath(root_path.to_path_buf()));
        }

        let mut nodes_map: HashMap<PathBuf, FileNode> = HashMap::new();
        let mut entry_paths_in_discovery_order: Vec<PathBuf> = Vec::new();

        for entry_result in WalkDir::new(root_path).min_depth(1).sort_by_file_name() {
            let entry = entry_result?;
            let path = entry.path().to_path_buf();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().is_dir();

            let node = FileNode::new(path.clone(), name, is_dir);
            nodes_map.insert(path.clone(), node);
            entry_paths_in_discovery_order.push(path);
        }

        for child_path_ref in entry_paths_in_discovery_order.iter().rev() {
            if let Some(parent_path) = child_path_ref.parent() {
                if parent_path != root_path {
                    if let Some(child_node_owned) = nodes_map.remove(child_path_ref) {
                        if let Some(parent_node_mut) = nodes_map.get_mut(parent_path) {
                            parent_node_mut.children.push(child_node_owned);
                        } else {
                            nodes_map.insert(child_path_ref.clone(), child_node_owned);
                        }
                    }
                }
            }
        }

        let mut top_level_nodes: Vec<FileNode> = nodes_map.into_values().collect();
        sort_file_nodes_recursively(&mut top_level_nodes);
        Ok(top_level_nodes)
    }
}

fn sort_file_nodes_recursively(nodes: &mut Vec<FileNode>) {
    nodes.sort_by(|a, b| {
        if a.is_dir && !b.is_dir {
            std::cmp::Ordering::Less
        } else if !a.is_dir && b.is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    for node in nodes.iter_mut() {
        if node.is_dir && !node.children.is_empty() {
            sort_file_nodes_recursively(&mut node.children);
        }
    }
}

/*
 * (DEPRECATED - Use CoreFileSystemScanner::scan_directory)
 * Scans a directory recursively and builds a tree of FileNode objects representing all files and subdirectories.
 */
#[deprecated(
    since = "0.1.0",
    note = "Please use `FileSystemScannerOperations::scan_directory` via an injected manager instance."
)]
pub fn scan_directory(root_path: &Path) -> Result<Vec<FileNode>> {
    let scanner = CoreFileSystemScanner::new();
    scanner.scan_directory(root_path)
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

    // Test helper for FileSystemScannerOperations using CoreFileSystemScanner
    fn test_scan_with_scanner(
        scanner: &dyn FileSystemScannerOperations,
        path: &Path,
    ) -> Result<Vec<FileNode>> {
        scanner.scan_directory(path)
    }

    #[test]
    fn test_scan_all_files_and_dirs_when_no_filtering() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;
        let scanner = CoreFileSystemScanner::new();

        let nodes = test_scan_with_scanner(&scanner, dir.path())?;
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
        let scanner = CoreFileSystemScanner::new();
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;

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
        let scanner = CoreFileSystemScanner::new();
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;

        assert_eq!(
            nodes.len(),
            5,
            "Expected 5 top-level entries. Found: {:?}",
            nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );

        let src_node = nodes.iter().find(|n| n.name == "src").unwrap();
        assert!(src_node.is_dir);
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
        assert_eq!(
            sub_src_node.children.len(),
            1,
            "sub_src should have deep.rs"
        );
        assert_eq!(sub_src_node.children[0].name, "deep.rs");

        let doc_node = nodes.iter().find(|n| n.name == "doc").unwrap();
        assert!(doc_node.is_dir);
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
        let scanner = CoreFileSystemScanner::new();

        let nodes = test_scan_with_scanner(&scanner, dir.path())?;
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
        let scanner = CoreFileSystemScanner::new();
        let result = test_scan_with_scanner(&scanner, non_existent_path);
        assert!(matches!(result, Err(FileSystemError::InvalidPath(_))));
    }
}
