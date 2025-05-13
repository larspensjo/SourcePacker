use super::models::FileNode; // Using FileNode from the parent 'core' module's re-export
use glob::{Pattern, PatternError};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Define a custom error type for this module
#[derive(Debug)]
pub enum FileSystemError {
    Io(io::Error),
    WalkDir(walkdir::Error),
    GlobPattern(PatternError),
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

impl From<PatternError> for FileSystemError {
    fn from(err: PatternError) -> Self {
        FileSystemError::GlobPattern(err)
    }
}

impl std::fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemError::Io(e) => write!(f, "I/O error: {}", e),
            FileSystemError::WalkDir(e) => write!(f, "Directory traversal error: {}", e),
            FileSystemError::GlobPattern(e) => write!(f, "Glob pattern error: {}", e),
            FileSystemError::InvalidPath(p) => write!(f, "Invalid path: {:?}", p),
        }
    }
}

impl std::error::Error for FileSystemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FileSystemError::Io(e) => Some(e),
            FileSystemError::WalkDir(e) => Some(e),
            FileSystemError::GlobPattern(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, FileSystemError>;

/// Scans a directory recursively and builds a tree of FileNode objects.
/// Only files matching the whitelist_patterns are included.
/// Folders are included if they potentially contain whitelisted files or are on the path to one.
pub fn scan_directory(
    root_path: &Path,
    whitelist_patterns_str: &[String],
) -> Result<Vec<FileNode>> {
    if !root_path.is_dir() {
        return Err(FileSystemError::InvalidPath(root_path.to_path_buf()));
    }

    // Compile glob patterns
    let whitelist_patterns: Vec<Pattern> = whitelist_patterns_str
        .iter()
        .map(|s| Pattern::new(s))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut tree_nodes: HashMap<PathBuf, FileNode> = HashMap::new();
    let mut top_level_nodes: Vec<FileNode> = Vec::new();

    // Use WalkDir to get all entries.
    // min_depth(1) to skip the root_path itself as an entry, we handle it as the container.
    for entry_result in WalkDir::new(root_path).min_depth(1) {
        let entry = entry_result?; // Propagate WalkDir errors
        process_entry(
            entry,
            root_path,
            &whitelist_patterns,
            &mut tree_nodes,
            &mut top_level_nodes,
        )?; // Propagate our custom errors
    }

    // Assemble top_level_nodes by adding directories from tree_nodes that are direct children of root_path
    for (dir_path, node) in tree_nodes {
        // tree_nodes now only contains directories
        if node.is_dir {
            // Should always be true by construction
            if let Some(parent_path) = dir_path.parent() {
                if parent_path == root_path {
                    // This directory is a direct child of root_path.
                    // The tree_nodes map holds the definitive versions of directories with their children.
                    if !top_level_nodes.iter().any(|n| n.path == dir_path) {
                        top_level_nodes.push(node);
                    }
                }
            }
        }
    }

    // Sort nodes alphabetically by name for consistent display
    sort_file_nodes_recursively(&mut top_level_nodes);
    Ok(top_level_nodes)
}

/// Processes a single directory entry from WalkDir.
/// If it's a whitelisted file, it adds the file and its necessary parent directories
/// to the `tree_nodes` map and `top_level_nodes` vector.
fn process_entry(
    entry: walkdir::DirEntry,
    root_path: &Path,
    whitelist_patterns: &[Pattern],
    tree_nodes: &mut HashMap<PathBuf, FileNode>,
    top_level_nodes: &mut Vec<FileNode>,
) -> Result<()> {
    let path = entry.path().to_path_buf();
    let name = entry.file_name().to_string_lossy().into_owned();
    let is_dir = entry.file_type().is_dir();

    if is_dir {
        // Directories are created on-demand when a child file necessitates them.
        return Ok(());
    }

    // It's a file, check against whitelist patterns.
    let mut matches_whitelist = false;
    if !whitelist_patterns.is_empty() {
        for pattern in whitelist_patterns {
            if pattern.matches_path(&path) {
                matches_whitelist = true;
                break;
            }
            if let Ok(relative_path) = path.strip_prefix(root_path) {
                if pattern.matches_path(relative_path) {
                    matches_whitelist = true;
                    break;
                }
            }
        }
    }

    if matches_whitelist {
        // If a file matches, ensure all its parent directories up to root_path are in `tree_nodes`.
        let mut current_path_for_ascent = path.clone();
        let mut child_node_to_add_to_parent: Option<FileNode> =
            Some(FileNode::new(path.clone(), name.clone(), false));

        // Iterate upwards from the file's parent to the root_path's parent
        loop {
            let parent_path_opt = current_path_for_ascent.parent();
            match parent_path_opt {
                None => break,
                Some(p_ref) if p_ref == root_path.parent().unwrap_or_else(|| Path::new("")) => {
                    break;
                }
                Some(p_ref) if p_ref != root_path && !p_ref.starts_with(root_path) => break,
                Some(parent_path_ref) => {
                    let parent_path_owned = parent_path_ref.to_path_buf();
                    let parent_node =
                        tree_nodes
                            .entry(parent_path_owned.clone())
                            .or_insert_with(|| {
                                FileNode::new(
                                    parent_path_owned.clone(),
                                    parent_path_owned
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned(),
                                    true, // It's a directory because it's a parent
                                )
                            });

                    if let Some(child_node) = child_node_to_add_to_parent.take() {
                        if !parent_node
                            .children
                            .iter()
                            .any(|c| c.path == child_node.path)
                        {
                            parent_node.children.push(child_node);
                        }
                    }

                    current_path_for_ascent = parent_path_owned;
                    if current_path_for_ascent == root_path {
                        break;
                    }
                }
            }
        }

        // After the loop, if child_node_to_add_to_parent still has a value,
        // it means the original matched file was directly under the root_path.
        if let Some(direct_child_of_root) = child_node_to_add_to_parent.take() {
            if let Some(p) = direct_child_of_root.path.parent() {
                if p == root_path {
                    if !top_level_nodes
                        .iter()
                        .any(|n| n.path == direct_child_of_root.path)
                    {
                        top_level_nodes.push(direct_child_of_root);
                    }
                }
            }
        }
    }
    Ok(())
}

fn sort_file_nodes_recursively(nodes: &mut Vec<FileNode>) {
    nodes.sort_by(|a, b| {
        // Sort directories before files, then alphabetically
        if a.is_dir && !b.is_dir {
            std::cmp::Ordering::Less
        } else if !a.is_dir && b.is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    for node in nodes {
        if node.is_dir {
            sort_file_nodes_recursively(&mut node.children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir; // From the tempfile crate

    // Helper to create a simple directory structure for testing
    fn setup_test_dir(base_path: &Path) -> io::Result<()> {
        fs::create_dir_all(base_path.join("src"))?;
        fs::create_dir_all(base_path.join("doc"))?;
        fs::create_dir_all(base_path.join("empty_dir"))?;

        File::create(base_path.join("src/main.rs"))?.sync_all()?;
        File::create(base_path.join("src/lib.rs"))?.sync_all()?;
        File::create(base_path.join("doc/README.md"))?.sync_all()?;
        File::create(base_path.join("LICENSE.txt"))?.sync_all()?;
        Ok(())
    }

    #[test]
    fn test_scan_empty_whitelist() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let whitelist: Vec<String> = vec![]; // Empty whitelist
        let nodes = scan_directory(dir.path(), &whitelist)?;
        assert!(
            nodes.is_empty(),
            "Scan with empty whitelist should yield no nodes by current logic"
        );
        Ok(())
    }

    #[test]
    fn test_scan_specific_files() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let whitelist = vec!["src/main.rs".to_string(), "*.md".to_string()];
        let nodes = scan_directory(dir.path(), &whitelist)?;

        assert_eq!(
            nodes.len(),
            2,
            "Expected 2 top-level entries (src dir, doc dir)"
        );

        let src_node = nodes.iter().find(|n| n.name == "src" && n.is_dir);
        assert!(src_node.is_some(), "Should find 'src' directory");
        assert_eq!(
            src_node.unwrap().children.len(),
            1,
            "src should have 1 child (main.rs)"
        );
        assert_eq!(src_node.unwrap().children[0].name, "main.rs");

        let doc_node = nodes.iter().find(|n| n.name == "doc" && n.is_dir);
        assert!(doc_node.is_some(), "Should find 'doc' directory");
        assert_eq!(
            doc_node.unwrap().children.len(),
            1,
            "doc should have 1 child (README.md)"
        );
        assert_eq!(doc_node.unwrap().children[0].name, "README.md");

        Ok(())
    }

    #[test]
    fn test_scan_wildcard_rs_files() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let whitelist = vec!["**/*.rs".to_string()]; // All .rs files recursively
        let nodes = scan_directory(dir.path(), &whitelist)?;

        // Expecting only 'src' at top level as it contains .rs files
        assert_eq!(nodes.len(), 1, "Expected 1 top-level entry (src dir)");
        let src_node = &nodes[0];
        assert_eq!(src_node.name, "src");
        assert!(src_node.is_dir);
        assert_eq!(
            src_node.children.len(),
            2,
            "src should have main.rs and lib.rs"
        );
        assert!(src_node.children.iter().any(|c| c.name == "main.rs"));
        assert!(src_node.children.iter().any(|c| c.name == "lib.rs"));
        Ok(())
    }

    #[test]
    fn test_scan_no_matches() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?;

        let whitelist = vec!["*.nonexistent".to_string()];
        let nodes = scan_directory(dir.path(), &whitelist)?;
        assert!(
            nodes.is_empty(),
            "Scan with no matching patterns should yield no nodes"
        );
        Ok(())
    }

    #[test]
    fn test_scan_includes_empty_dirs_if_parent_of_match() -> Result<()> {
        let dir = tempdir()?;
        fs::create_dir_all(dir.path().join("parent/empty_child"))?;
        File::create(dir.path().join("parent/file.txt"))?.sync_all()?;

        let whitelist = vec!["**/file.txt".to_string()];
        let nodes = scan_directory(dir.path(), &whitelist)?;

        assert_eq!(nodes.len(), 1); // "parent" dir
        let parent_node = &nodes[0];
        assert_eq!(parent_node.name, "parent");
        assert!(parent_node.is_dir);
        // "empty_child" should NOT be included because it doesn't contain or lead to a whitelisted file
        // "file.txt" should be.
        assert_eq!(parent_node.children.len(), 1);
        assert!(parent_node.children.iter().any(|c| c.name == "file.txt"));
        assert!(!parent_node.children.iter().any(|c| c.name == "empty_child"));
        Ok(())
    }

    #[test]
    fn test_invalid_root_path() {
        let non_existent_path = Path::new("this_path_does_not_exist_hopefully");
        let whitelist: Vec<String> = vec![];
        let result = scan_directory(non_existent_path, &whitelist);
        assert!(matches!(result, Err(FileSystemError::InvalidPath(_))));
    }
}
