use super::{file_node::FileNode, profiles::PROJECT_CONFIG_DIR_NAME};
use crate::core::checksum_utils;
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/*
 * This module provides functionalities for interacting with the file system,
 * primarily focusing on scanning directory structures while respecting ignore files
 * (like .gitignore). It defines errors specific to these operations, a trait
 * `FileSystemScannerOperations` for abstracting scanning logic, and a concrete
 * implementation `CoreFileSystemScanner`.
 */

/*
 * Defines custom error types for file system operations.
 * This enum centralizes error handling for directory scanning, I/O issues,
 * ignore file processing, and path validity, providing more specific error
 * information.
 */
#[derive(Debug)]
pub enum FileSystemError {
    Io(io::Error),
    IgnoreError(ignore::Error),
    InvalidPath(PathBuf),
}

impl From<io::Error> for FileSystemError {
    fn from(err: io::Error) -> Self {
        FileSystemError::Io(err)
    }
}

impl From<ignore::Error> for FileSystemError {
    fn from(err: ignore::Error) -> Self {
        FileSystemError::IgnoreError(err)
    }
}

impl std::fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemError::Io(e) => write!(f, "I/O error: {e}"),
            FileSystemError::IgnoreError(e) => write!(f, "Ignore pattern processing error: {e}"),
            FileSystemError::InvalidPath(p) => write!(f, "Invalid path: {p:?}"),
        }
    }
}

impl std::error::Error for FileSystemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FileSystemError::Io(e) => Some(e),
            FileSystemError::IgnoreError(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, FileSystemError>;

/*
 * Defines the operations for scanning file systems.
 * This trait abstracts the specific mechanisms for traversing directories and
 * building a representation of the file structure, typically as a tree of `FileNode` objects.
 * Implementations should respect ignore files (e.g., .gitignore) and any additional exclude patterns
 * supplied at scan time.
 */
pub trait FileSystemScannerOperations: Send + Sync {
    /*
     * Scans a directory recursively and builds a tree of FileNode objects.
     * Implementations should traverse the specified `root_path`, respecting standard
     * ignore files like .gitignore, and construct a hierarchical representation of
     * non-ignored files and directories. The tree is typically sorted for consistent presentation.
     */
    fn scan_directory(
        &self,
        root_path: &Path,
        exclude_patterns: &[String],
    ) -> Result<Vec<FileNode>>;
}

/*
 * The core implementation of `FileSystemScannerOperations`.
 * This struct handles the actual file system traversal and `FileNode` tree construction
 * using the `ignore` crate, which respects `.gitignore` and other ignore files.
 * TODO: We should move the root path to this structure.
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
     * Scans a directory recursively and builds a tree of FileNode objects representing all non-ignored files and subdirectories.
     * This function traverses the specified root_path, respecting .gitignore files (and other standard ignore files),
     * and constructs a hierarchical representation.
     * The tree is sorted such that directories appear before files at each level, and then alphabetically.
     */
    fn scan_directory(
        &self,
        root_path: &Path,
        exclude_patterns: &[String],
    ) -> Result<Vec<FileNode>> {
        if !root_path.is_dir() {
            return Err(FileSystemError::InvalidPath(root_path.to_path_buf()));
        }
        log::debug!(
            "FileSystemScanner: Scanning directory {root_path:?}, respecting local .gitignore files."
        );

        let mut nodes_map: HashMap<PathBuf, FileNode> = HashMap::new();
        let mut entry_paths_in_discovery_order: Vec<PathBuf> = Vec::new();

        // Use WalkBuilder from the 'ignore' crate, applying any user-specified exclude patterns.
        let mut walker_builder = WalkBuilder::new(root_path);
        walker_builder
            .standard_filters(true) // Enables standard gitignore-style filtering (gitignore, .ignore, .git/info/exclude)
            .parents(true) // Process ignore files in parent directories.
            .git_global(false) // Do not respect global .gitignore for more hermetic behavior, especially in tests.
            .git_ignore(true) // Respect .gitignore files.
            .git_exclude(true) // Respect .git/info/exclude files.
            .ignore(true) // Respect .ignore files.
            .hidden(true) // Standard behavior: ignore hidden files unless explicitly unignored.
            .sort_by_file_path(|a, b| a.cmp(b)); // Sort entries by path for consistent processing order

        if !exclude_patterns.is_empty() {
            let mut override_builder = OverrideBuilder::new(root_path);
            for pattern in exclude_patterns {
                let trimmed = pattern.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }

                let override_pattern = if let Some(negated) = trimmed.strip_prefix('!') {
                    let include_pattern = negated.trim();
                    if include_pattern.is_empty() {
                        continue;
                    }
                    include_pattern.to_string()
                } else {
                    format!("!{trimmed}")
                };

                if let Err(err) = override_builder.add(&override_pattern) {
                    log::warn!("FileSystemScanner: Invalid exclude pattern '{pattern}': {err}");
                }
            }

            match override_builder.build() {
                Ok(overrides) => {
                    walker_builder.overrides(overrides);
                }
                Err(err) => {
                    log::warn!(
                        "FileSystemScanner: Failed to build overrides for exclude patterns: {err}"
                    );
                }
            }
        }

        let walker = walker_builder.build();

        for entry_result in walker {
            let entry = entry_result?; // Propagates ignore::Error, converted by From trait

            // Skip the root_path itself, as we want its children.
            // The `ignore` crate's walker will yield the starting path if it matches filters.
            if entry.path() == root_path {
                continue;
            }

            let path = entry.path().to_path_buf();

            if is_internal_config_path(root_path, &path) {
                log::trace!(
                    "FileSystemScanner: Skipping internal config path {:?} during scan.",
                    path
                );
                continue;
            }

            // Use file_name from DirEntry as it's relative to its parent.
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().is_some_and(|ft| ft.is_dir());

            let checksum_str;
            if !is_dir {
                // Calculate checksum only for files
                match checksum_utils::calculate_sha256_checksum(&path) {
                    Ok(updated_checksum) => checksum_str = updated_checksum,
                    Err(e) => {
                        log::warn!(
                            "FileSystemScanner: Failed to calculate checksum for file {path:?}: {e}"
                        );
                        checksum_str = String::new();
                    }
                }
            } else {
                checksum_str = String::new();
            }

            let node = FileNode::new(path.clone(), name, is_dir, checksum_str);

            nodes_map.insert(path.clone(), node);
            entry_paths_in_discovery_order.push(path);
        }

        // Tree reconstruction logic:
        // Iterate backwards to build from leaves up to direct children of root_path.
        for child_path_ref in entry_paths_in_discovery_order.iter().rev() {
            let Some(parent_path) = child_path_ref.parent() else {
                continue;
            };
            // We only want to add children to parents that are *also* part of the scan
            // (i.e., not the root_path itself, which acts as the implicit parent of top-level nodes).
            if parent_path == root_path {
                continue;
            }

            if let Some(child_node_owned) = nodes_map.remove(child_path_ref) {
                if let Some(parent_node_mut) = nodes_map.get_mut(parent_path) {
                    parent_node_mut.children.push(child_node_owned);
                } else {
                    // This case implies the parent_path was ignored or not part of the scan results.
                    // The child_node_owned was not ignored, so it becomes a top-level node.
                    // This can happen if a .gitignore rule ignores a directory but un-ignores a file within it.
                    // e.g., `ignored_dir/` and `!ignored_dir/important_file.txt`
                    // In such a scenario, important_file.txt might appear without its explicit parent
                    // if `ignored_dir` itself is not yielded by the walker.
                    // However, `ignore` crate usually yields directories if they contain non-ignored content.
                    // So, we re-insert it into nodes_map to be collected as a top-level node.
                    log::error!(
                        "FileSystemScanner: Parent {parent_path:?} not found in map for child {child_path_ref:?}. Re-inserting child as potential top-level."
                    );
                    nodes_map.insert(child_path_ref.clone(), child_node_owned);
                }
            }
        }

        let mut top_level_nodes: Vec<FileNode> = nodes_map.into_values().collect();
        sort_file_nodes_recursively(&mut top_level_nodes);
        log::debug!(
            "FileSystemScanner: Scan complete. Found {} top-level non-ignored entries for {:?}.",
            top_level_nodes.len(),
            root_path
        );
        Ok(top_level_nodes)
    }
}

fn sort_file_nodes_recursively(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        if a.is_dir() && !b.is_dir() {
            std::cmp::Ordering::Less
        } else if !a.is_dir() && b.is_dir() {
            std::cmp::Ordering::Greater
        } else {
            a.name().cmp(b.name())
        }
    });

    for node in nodes.iter_mut() {
        if node.is_dir() && !node.children.is_empty() {
            sort_file_nodes_recursively(&mut node.children);
        }
    }
}

fn is_internal_config_path(root_path: &Path, candidate_path: &Path) -> bool {
    let config_component = OsStr::new(PROJECT_CONFIG_DIR_NAME);
    if let Ok(relative) = candidate_path.strip_prefix(root_path) {
        return relative
            .components()
            .any(|component| component.as_os_str() == config_component);
    }

    // Fallback: treat any absolute path containing the component as internal even if prefix stripping failed.
    candidate_path
        .components()
        .any(|component| component.as_os_str() == config_component)
}

#[cfg(test)]
mod tests {
    use crate::core::file_node::FileTokenDetails;

    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;
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
        scanner.scan_directory(path, &Vec::new())
    }

    // Helper to create .gitignore file for tests
    fn create_gitignore(dir_path: &Path, content: &str) -> io::Result<()> {
        let gitignore_path = dir_path.join(".gitignore");
        let mut file = File::create(gitignore_path)?;
        writeln!(file, "{content}")?;
        Ok(())
    }

    // Setup for testing .gitignore behavior
    fn setup_test_dir_for_ignore(base_path: &Path) -> io::Result<()> {
        // Create a dummy .git directory to make 'ignore' crate behave more like it's in a repo
        fs::create_dir_all(base_path.join(".git"))?;

        fs::create_dir_all(base_path.join("src"))?;
        fs::create_dir_all(base_path.join("doc"))?;
        fs::create_dir_all(base_path.join("target"))?; // To be ignored by root .gitignore
        fs::create_dir_all(base_path.join("src").join("sub_src"))?;
        fs::create_dir_all(base_path.join("logs"))?; // To be mostly ignored
        fs::create_dir_all(base_path.join("data").join("sensitive"))?; // sensitive to be ignored by data/.gitignore

        File::create(base_path.join("src/main.rs"))?.sync_all()?;
        File::create(base_path.join("src/lib.rs"))?.sync_all()?;
        File::create(base_path.join("src/sub_src/deep.rs"))?.sync_all()?;
        File::create(base_path.join("src/sub_src/temp.tmp"))?.sync_all()?; // Ignored by *.tmp
        File::create(base_path.join("doc/README.md"))?.sync_all()?;
        File::create(base_path.join("LICENSE.txt"))?.sync_all()?;
        File::create(base_path.join("root_file.toml"))?.sync_all()?;
        File::create(base_path.join("target/debug_output.bin"))?.sync_all()?;
        File::create(base_path.join("logs/app.log"))?.sync_all()?; // Ignored by logs/*
        File::create(base_path.join("logs/trace.log"))?.sync_all()?; // Un-ignored by !logs/trace.log
        File::create(base_path.join("data/config.json"))?.sync_all()?;
        File::create(base_path.join("data/sensitive/secret.key"))?.sync_all()?;

        // Root .gitignore
        create_gitignore(base_path, "target/\n*.tmp\nlogs/*\n!logs/trace.log\n")?;
        // .gitignore in data/
        create_gitignore(base_path.join("data").as_path(), "sensitive/\n")?;
        Ok(())
    }

    #[test]
    fn test_scan_respects_gitignore_rules() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir_for_ignore(dir.path())?;
        let scanner = CoreFileSystemScanner::new();

        log::debug!("--- Starting test_scan_respects_gitignore_rules ---");
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;
        log::debug!("--- Scan finished for test_scan_respects_gitignore_rules ---");

        let top_level_names: Vec<String> = nodes.iter().map(|n| n.name().to_string()).collect();
        // Expected: data, doc, logs, src, LICENSE.txt, root_file.toml
        // "target" dir should now be properly ignored.
        assert_eq!(
            top_level_names.len(),
            6,
            "Expected 6 top-level non-ignored items. Found names: {top_level_names:?}"
        );
        assert!(top_level_names.contains(&"data".to_string()));
        assert!(top_level_names.contains(&"doc".to_string()));
        assert!(top_level_names.contains(&"logs".to_string()));
        assert!(top_level_names.contains(&"src".to_string()));
        assert!(top_level_names.contains(&"LICENSE.txt".to_string()));
        assert!(top_level_names.contains(&"root_file.toml".to_string()));
        assert!(
            !top_level_names.contains(&"target".to_string()),
            "Top level names should not contain 'target'"
        );

        // Check 'src' directory contents
        let src_node = nodes.iter().find(|n| n.name() == "src").unwrap();
        // Expected children in src: lib.rs, main.rs, sub_src (directory)
        // src/sub_src/temp.tmp is ignored by *.tmp
        assert_eq!(
            src_node.children.len(),
            3,
            "src should have 3 non-ignored children. Found: {:?}",
            src_node
                .children
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
        );
        assert!(src_node.children.iter().any(|n| n.name() == "lib.rs"));
        assert!(src_node.children.iter().any(|n| n.name() == "main.rs"));
        let sub_src_node = src_node
            .children
            .iter()
            .find(|n| n.name() == "sub_src" && n.is_dir())
            .expect("sub_src directory should exist");
        // Expected children in src/sub_src: deep.rs
        // temp.tmp is ignored.
        assert_eq!(
            sub_src_node.children.len(),
            1,
            "sub_src should have 1 non-ignored child. Found: {:?}",
            sub_src_node
                .children
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
        );
        assert_eq!(sub_src_node.children[0].name(), "deep.rs");

        // Check 'logs' directory contents
        let logs_node = nodes.iter().find(|n| n.name() == "logs").unwrap();
        // Expected children in logs: trace.log (app.log is ignored by logs/*, trace.log is un-ignored)
        assert_eq!(
            logs_node.children.len(),
            1,
            "logs should have 1 non-ignored child (trace.log). Found: {:?}",
            logs_node
                .children
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
        );
        assert_eq!(logs_node.children[0].name(), "trace.log");

        // Check 'data' directory contents
        let data_node = nodes.iter().find(|n| n.name() == "data").unwrap();
        // Expected children in data: config.json
        // data/sensitive/ is ignored by data/.gitignore
        assert_eq!(
            data_node.children.len(),
            1,
            "data should have 1 non-ignored child (config.json). Found: {:?}",
            data_node
                .children
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
        );
        assert_eq!(data_node.children[0].name(), "config.json");

        Ok(())
    }

    #[test]
    fn test_scan_respects_custom_exclude_patterns() -> Result<()> {
        // Arrange
        let dir = tempdir()?;
        let build_dir = dir.path().join("build");
        let notes_dir = dir.path().join("notes");
        fs::create_dir_all(&build_dir)?;
        fs::create_dir_all(dir.path().join("src"))?;
        fs::create_dir_all(&notes_dir)?;
        fs::write(build_dir.join("output.bin"), b"binary content")?;
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}")?;
        fs::write(notes_dir.join("activity.log"), "log entry")?;
        fs::write(dir.path().join("README.md"), "# keep me")?;
        let scanner = CoreFileSystemScanner::new();
        let exclude_patterns = vec!["*.log".to_string(), "build/".to_string()];

        // Act
        let nodes = scanner.scan_directory(dir.path(), &exclude_patterns)?;

        // Assert
        fn tree_contains_path(nodes: &[FileNode], target: &Path) -> bool {
            for node in nodes {
                if node.path() == target {
                    return true;
                }
                if node.is_dir() && tree_contains_path(&node.children, target) {
                    return true;
                }
            }
            false
        }

        assert!(
            !tree_contains_path(&nodes, &build_dir),
            "Build directory should be excluded by profile patterns."
        );
        assert!(
            !tree_contains_path(&nodes, &notes_dir.join("activity.log")),
            "Log files matching *.log should be excluded."
        );
        assert!(
            tree_contains_path(&nodes, &dir.path().join("src").join("main.rs")),
            "Non-matching files must remain in the scan results."
        );
        Ok(())
    }

    #[test]
    fn test_scan_structure_without_ignores() -> Result<()> {
        let dir = tempdir()?;
        setup_test_dir(dir.path())?; // Uses the original setup without .gitignore
        let scanner = CoreFileSystemScanner::new();

        log::debug!("--- Starting test_scan_structure_without_ignores ---");
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;
        log::debug!("--- Scan finished for test_scan_structure_without_ignores ---");

        // This test should behave as before since no .gitignore files are present
        assert_eq!(
            nodes.len(),
            5,
            "Scan should return all top-level items. Found names: {:?}",
            nodes.iter().map(|n| n.name()).collect::<Vec<_>>()
        );

        let src_node = nodes
            .iter()
            .find(|n| n.name() == "src" && n.is_dir())
            .unwrap();
        assert_eq!(
            src_node.children.len(),
            3,
            "src children mismatch. Found: {:?}",
            src_node
                .children
                .iter()
                .map(|n| n.name())
                .collect::<Vec<_>>()
        ); // sub_src, lib.rs, main.rs

        let sub_src_node = src_node
            .children
            .iter()
            .find(|n| n.name() == "sub_src" && n.is_dir())
            .unwrap();
        assert_eq!(
            sub_src_node.children.len(),
            1,
            "sub_src children count mismatch. Found: {:?}",
            sub_src_node
                .children
                .iter()
                .map(|n| n.name())
                .collect::<Vec<_>>()
        ); // deep.rs
        assert_eq!(sub_src_node.children[0].name(), "deep.rs");

        Ok(())
    }

    #[test]
    fn test_scan_includes_empty_dirs_correctly() -> Result<()> {
        let dir = tempdir()?;
        let parent_dir = dir.path().join("parent");
        fs::create_dir_all(parent_dir.join("empty_child"))?;
        File::create(parent_dir.join("file.txt"))?.sync_all()?;
        let another_empty_top_dir = dir.path().join("another_empty_top_level_dir");
        fs::create_dir_all(&another_empty_top_dir)?;
        // Create a .gitignore in the root that ignores nothing relevant here,
        // to ensure WalkBuilder is active but doesn't interfere with this specific test's goal.
        create_gitignore(dir.path(), "#empty .gitignore\n")?;

        let scanner = CoreFileSystemScanner::new();
        log::debug!("--- Starting test_scan_includes_empty_dirs_correctly ---");
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;
        log::debug!("--- Scan finished for test_scan_includes_empty_dirs_correctly ---");

        // Print details for debugging if assertions fail
        if nodes.len() != 2 {
            log::debug!("Nodes found (expected 2):");
            for node in &nodes {
                log::debug!("  Top-level: {} (is_dir: {})", node.name(), node.is_dir());
                for child in &node.children {
                    log::debug!("    Child: {} (is_dir: {})", child.name(), child.is_dir());
                }
            }
        }

        assert_eq!(
            nodes.len(),
            2,
            "Expected 2 top-level entries. Found: {:?}",
            nodes.iter().map(|n| n.name()).collect::<Vec<_>>()
        );

        let top_level_names: Vec<&str> = nodes.iter().map(|n| n.name()).collect();
        // Order depends on WalkBuilder's sort_by_file_path, then our recursive sort.
        // "another_empty_top_level_dir", "parent" is a likely order.
        assert!(top_level_names.contains(&"another_empty_top_level_dir"));
        assert!(top_level_names.contains(&"parent"));

        let parent_node = nodes
            .iter()
            .find(|n| n.name() == "parent")
            .expect("Should find 'parent' dir");
        assert!(parent_node.is_dir());
        assert_eq!(
            parent_node.children.len(),
            2,
            "Expected 'empty_child' and 'file.txt' in 'parent'. Found: {:?}",
            parent_node
                .children
                .iter()
                .map(|n| n.name())
                .collect::<Vec<_>>()
        );

        let parent_children_names: Vec<String> = parent_node
            .children
            .iter()
            .map(|n| n.name().to_string())
            .collect();
        assert!(parent_children_names.contains(&"empty_child".to_string()));
        assert!(parent_children_names.contains(&"file.txt".to_string()));

        assert!(
            parent_node
                .children
                .iter()
                .any(|c| c.name() == "file.txt" && !c.is_dir())
        );
        let empty_child_node = parent_node
            .children
            .iter()
            .find(|c| c.name() == "empty_child")
            .unwrap();
        assert!(empty_child_node.is_dir());
        assert!(
            empty_child_node.children.is_empty(),
            "empty_child in parent should have no children"
        );

        let another_empty_node = nodes
            .iter()
            .find(|n| n.name() == "another_empty_top_level_dir")
            .expect("Should find 'another_empty_top_level_dir'");
        assert!(another_empty_node.is_dir());
        assert!(
            another_empty_node.children.is_empty(),
            "another_empty_top_level_dir should have no children"
        );
        Ok(())
    }

    #[test]
    fn test_invalid_root_path() {
        let non_existent_path = Path::new("this_path_does_not_exist_hopefully");
        let scanner = CoreFileSystemScanner::new();
        log::debug!("--- Starting test_invalid_root_path ---");
        let result = test_scan_with_scanner(&scanner, non_existent_path);
        log::debug!("--- Scan finished for test_invalid_root_path ---");
        assert!(matches!(result, Err(FileSystemError::InvalidPath(_))));
    }

    #[test]
    fn test_scan_populates_checksums_for_files() -> Result<()> {
        // TODO: This test may need to be improved. Some of the logic was cut.
        let dir = tempdir()?;
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");
        let subdir_path = dir.path().join("subdir");
        fs::create_dir(&subdir_path)?;
        let file_in_subdir_path = subdir_path.join("file3.txt");

        fs::write(&file1_path, "content1")?;
        fs::write(&file2_path, "content2")?;
        fs::write(&file_in_subdir_path, "content3")?;

        let scanner = CoreFileSystemScanner::new();
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;

        let file1_node = nodes.iter().find(|n| n.path() == file1_path).unwrap();
        let ftd = FileTokenDetails {
            checksum: checksum_utils::calculate_sha256_checksum(&file1_path).unwrap(),
            token_count: 0,
        };
        assert!(file1_node.checksum_match(Some(&ftd)));

        let subdir_node = nodes.iter().find(|n| n.path() == subdir_path).unwrap();
        assert!(subdir_node.is_dir());
        Ok(())
    }

    fn tree_contains_component(nodes: &[FileNode], target: &str) -> bool {
        for node in nodes {
            if node.name() == target {
                return true;
            }
            if tree_contains_component(&node.children, target) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_scan_skips_internal_sourcepacker_directory() -> Result<()> {
        let dir = tempdir()?;
        let internal_dir = dir.path().join(PROJECT_CONFIG_DIR_NAME);
        fs::create_dir_all(internal_dir.join("profiles"))?;
        fs::write(
            internal_dir.join("profiles").join("hidden_profile.json"),
            "{ }",
        )?;
        fs::write(dir.path().join("visible.txt"), "visible content")?;

        let scanner = CoreFileSystemScanner::new();
        let nodes = test_scan_with_scanner(&scanner, dir.path())?;

        let top_level_names: Vec<&str> = nodes.iter().map(|n| n.name()).collect();
        assert!(
            top_level_names.contains(&"visible.txt"),
            "Expected regular project files to remain in scan results."
        );
        assert!(
            !top_level_names.contains(&PROJECT_CONFIG_DIR_NAME),
            "Internal {PROJECT_CONFIG_DIR_NAME} directory should be excluded from scan results."
        );
        assert!(
            !tree_contains_component(&nodes, PROJECT_CONFIG_DIR_NAME),
            "Internal {PROJECT_CONFIG_DIR_NAME} directory should not appear in any subtree."
        );

        Ok(())
    }
}
