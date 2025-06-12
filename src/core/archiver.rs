use super::file_node::{ArchiveStatus, FileNode};
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

/*
 * This module handles operations related to creating and managing the state of archives.
 * It includes functions to generate archive content from selected files,
 * retrieve file timestamps, check the synchronization status of an archive,
 * and save archive content to disk. It defines a trait `ArchiverOperations`
 * for abstracting these operations and a concrete implementation `CoreArchiver`.
 */

/*
 * Defines the operations for managing archives.
 * This trait abstracts the specific mechanisms for creating archive content,
 * checking its status against source files, and saving it. It allows for different
 * implementations (e.g., file-system-based, mock) to be used, enhancing testability.
 */
pub trait ArchiverOperations: Send + Sync {
    /*
     * Creates a concatenated string of content from selected files in the tree.
     * Traverses the provided `nodes`. For each `FileNode` that is a file and
     * has `FileState::Selected`, it reads its content and prepends/appends headers.
     * The `root_path_for_display` is used to relativize paths in headers.
     */
    fn create_content(
        &self,
        nodes: &[FileNode],
        root_path_for_display: &Path,
    ) -> io::Result<String>;

    /*
     * Checks the synchronization status of an archive file.
     * Compares the archive's timestamp (if `archive_path` is Some and exists)
     * against the newest timestamp among the selected source files within `file_nodes_tree`.
     * TODO: Does the path have to be an Option?
     */
    fn check_status(
        &self,
        archive_path: Option<&Path>,
        file_nodes_tree: &[FileNode],
    ) -> ArchiveStatus;

    /*
     * Saves the provided archive `content` string to the specified `path`.
     * Implementations should handle overwriting the file if it exists.
     */
    fn save(&self, path: &Path, content: &str) -> io::Result<()>;

    /*
     * Retrieves the last modification timestamp of the file at the given `path`.
     * This is used internally by `check_archive_status` but exposed for potential direct use or testing.
     */
    fn get_file_timestamp(&self, path: &Path) -> io::Result<SystemTime>;
}

/*
 * The core implementation of `ArchiverOperations`.
 * This struct handles the actual file system interactions for creating, checking,
 * and saving archives.
 */
pub struct CoreArchiver {}

impl CoreArchiver {
    /*
     * Creates a new instance of `CoreArchiver`.
     * This constructor doesn't require any parameters.
     * TODO: We should move the path to this structure.
     */
    pub fn new() -> Self {
        CoreArchiver {}
    }
}

impl Default for CoreArchiver {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiverOperations for CoreArchiver {
    fn create_content(
        &self,
        nodes: &[FileNode],
        root_path_for_display: &Path,
    ) -> io::Result<String> {
        // Initialize archive_content with the new global header line.
        let mut archive_content = format!(
            "// Combined files from {}\n",
            root_path_for_display.display()
        );
        let mut buffer = Vec::new();

        for node in nodes.iter().rev() {
            buffer.push(node);
        }

        while let Some(node) = buffer.pop() {
            if node.is_dir() {
                for child in node.children.iter().rev() {
                    buffer.push(child);
                }
            } else if node.is_selected() {
                let display_path = node
                    .path()
                    .strip_prefix(root_path_for_display)
                    .unwrap_or(node.path())
                    .to_string_lossy();

                archive_content.push_str(&format!("// ===== File: {} =====\n", display_path));

                match fs::read_to_string(node.path()) {
                    Ok(content) => {
                        archive_content.push_str(&content);
                        if !content.ends_with('\n') {
                            archive_content.push('\n');
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }
        Ok(archive_content)
    }

    fn get_file_timestamp(&self, path: &Path) -> io::Result<SystemTime> {
        fs::metadata(path)?.modified()
    }

    fn check_status(
        &self,
        archive_file_path_opt: Option<&Path>,
        file_nodes_tree: &[FileNode],
    ) -> ArchiveStatus {
        let current_archive_path = match archive_file_path_opt {
            Some(p) => p,
            None => {
                log::debug!(
                    "Archiver: check_archive_status - No archive path provided, status is NotYetGenerated."
                );
                return ArchiveStatus::NotYetGenerated;
            }
        };

        let archive_timestamp = match self.get_file_timestamp(current_archive_path) {
            Ok(ts) => ts,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                log::debug!(
                    "Archiver: check_archive_status - Archive file {:?} missing.",
                    current_archive_path
                );
                return ArchiveStatus::ArchiveFileMissing;
            }
            Err(e) => {
                log::error!(
                    "Archiver: Error getting archive timestamp for {:?}: {}",
                    current_archive_path,
                    e
                );
                return ArchiveStatus::ErrorChecking(Some(e.kind()));
            }
        };

        let mut newest_selected_file_timestamp: Option<SystemTime> = None;
        let mut has_selected_files = false;

        /* Helper recursive function to iterate through nodes and find selected files.
         * It now needs to take `&dyn ArchiverOperations` to call `get_file_timestamp`. */
        fn find_newest_selected(
            archiver: &dyn ArchiverOperations,
            nodes: &[FileNode],
            current_newest: &mut Option<SystemTime>,
            has_any_selected: &mut bool,
        ) -> io::Result<()> {
            for node in nodes {
                if node.is_dir() {
                    find_newest_selected(
                        archiver,
                        &node.children,
                        current_newest,
                        has_any_selected,
                    )?;
                } else if node.is_selected() {
                    *has_any_selected = true;
                    let file_ts = archiver.get_file_timestamp(node.path())?;
                    if let Some(newest) = current_newest {
                        if file_ts > *newest {
                            *current_newest = Some(file_ts);
                        }
                    } else {
                        *current_newest = Some(file_ts);
                    }
                }
            }
            Ok(())
        }

        if let Err(e) = find_newest_selected(
            self,
            file_nodes_tree,
            &mut newest_selected_file_timestamp,
            &mut has_selected_files,
        ) {
            log::error!("Archiver: Error checking source file timestamps: {}", e);
            return ArchiveStatus::ErrorChecking(Some(e.kind()));
        }

        if !has_selected_files {
            log::debug!("Archiver: check_archive_status - No files selected.");
            return ArchiveStatus::NoFilesSelected;
        }

        match newest_selected_file_timestamp {
            Some(newest_src_ts) => {
                if newest_src_ts > archive_timestamp {
                    log::debug!(
                        "Archiver: check_archive_status - Archive {:?} is OUTDATED.",
                        current_archive_path
                    );
                    ArchiveStatus::OutdatedRequiresUpdate
                } else {
                    log::debug!(
                        "Archiver: check_archive_status - Archive {:?} is UP TO DATE.",
                        current_archive_path
                    );
                    ArchiveStatus::UpToDate
                }
            }
            None => {
                /* This case should ideally not be reached if has_selected_files is true,
                 * but implies an issue like a selected file not having a timestamp.
                 * find_newest_selected would usually return Err in such a case. */
                log::warn!(
                    "Archiver: check_archive_status - Files selected, but newest_selected_file_timestamp is None for archive {:?}. Defaulting to ErrorChecking.",
                    current_archive_path
                );
                ArchiveStatus::ErrorChecking(None)
            }
        }
    }

    fn save(&self, path: &Path, content: &str) -> io::Result<()> {
        fs::write(path, content)
    }
}

#[cfg(test)]
mod archiver_tests {
    use super::*;
    use crate::core::file_node::{FileNode, SelectionState};
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    // Helper to create a FileNode for testing
    fn new_test_file_node(
        base_path: &Path,
        name: &str,
        is_dir: bool,
        state: SelectionState,
        children: Vec<FileNode>,
    ) -> FileNode {
        FileNode::new_full(
            base_path.join(name),
            name.to_string(),
            is_dir,
            state,
            children,
            "".to_string(),
        )
    }

    // Test helper for ArchiverOperations using CoreArchiver
    fn test_with_archiver<F, R>(test_fn: F) -> R
    where
        F: FnOnce(&dyn ArchiverOperations) -> R,
    {
        crate::initialize_logging();
        let archiver = CoreArchiver::new();
        test_fn(&archiver)
    }

    #[test]
    fn test_core_archiver_create_archive_from_selected_files() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let base_path = dir.path();

            let file1_path = base_path.join("file1.txt");
            let mut f1 = File::create(&file1_path)?;
            writeln!(f1, "Content of file1.")?;
            drop(f1);

            let subdir_path = base_path.join("subdir");
            fs::create_dir_all(&subdir_path)?;
            let file2_path = subdir_path.join("file2.rs");
            let mut f2 = File::create(&file2_path)?;
            writeln!(f2, "Content of file2 (Rust).")?;
            writeln!(f2, "Another line.")?;
            drop(f2);

            let file3_path = base_path.join("file3.md");
            File::create(&file3_path)?.sync_all()?;

            let nodes = vec![
                new_test_file_node(
                    base_path,
                    "file1.txt",
                    false,
                    SelectionState::Selected,
                    vec![],
                ),
                new_test_file_node(
                    base_path,
                    "subdir",
                    true,
                    SelectionState::New,
                    vec![new_test_file_node(
                        &subdir_path,
                        "file2.rs",
                        false,
                        SelectionState::Selected,
                        vec![],
                    )],
                ),
                new_test_file_node(
                    base_path,
                    "file3.md",
                    false,
                    SelectionState::Deselected,
                    vec![],
                ),
            ];

            // Act
            let archive = archiver.create_content(&nodes, base_path)?;

            // Assert
            let path1_display = "file1.txt";
            let path2_display_os_specific = PathBuf::from("subdir").join("file2.rs");
            let path2_display_str = path2_display_os_specific.to_string_lossy();
            let root_display_str = base_path.display();

            let expected_content = format!(
                "// Combined files from {}\n\
                 // ===== File: {} =====\n\
                 Content of file1.\n\
                 // ===== File: {} =====\n\
                 Content of file2 (Rust).\n\
                 Another line.\n",
                root_display_str, path1_display, path2_display_str
            );
            assert_eq!(archive, expected_content);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_save_archive_content() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_path = dir.path().join("my_archive.txt");
            let content = "This is the test archive content.";

            // Act
            archiver.save(&archive_path, content)?;

            // Assert
            let read_content = fs::read_to_string(&archive_path)?;
            assert_eq!(read_content, content);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_get_file_timestamp_exists() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let file_path = dir.path().join("test_ts.txt");
            File::create(&file_path)?.write_all(b"content")?;
            thread::sleep(Duration::from_millis(10));

            // Act
            let ts = archiver.get_file_timestamp(&file_path)?;

            // Assert
            assert!(ts <= SystemTime::now());
            Ok(())
        })
    }

    #[test]
    fn test_get_file_timestamp_not_exists() {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir().expect("Failed to create temp dir");
            let file_path = dir.path().join("non_existent_ts.txt");

            // Act
            let result = archiver.get_file_timestamp(&file_path);

            // Assert
            assert!(result.is_err());
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotFound);
        });
    }

    #[test]
    fn test_core_archiver_check_archive_status_outdated() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path1 = dir.path().join("src_file1.txt");
            let src_file_path2 = dir.path().join("src_file2.txt");

            File::create(&archive_file_path)?.write_all(b"old archive content")?;
            thread::sleep(Duration::from_millis(20));

            File::create(&src_file_path1)?.write_all(b"source1")?;
            thread::sleep(Duration::from_millis(20));
            File::create(&src_file_path2)?.write_all(b"source2")?;

            let file_nodes = vec![
                new_test_file_node(
                    dir.path(),
                    "src_file1.txt",
                    false,
                    SelectionState::Selected,
                    vec![],
                ),
                new_test_file_node(
                    dir.path(),
                    "src_file2.txt",
                    false,
                    SelectionState::Selected,
                    vec![],
                ),
            ];

            // Act
            let status = archiver.check_status(Some(&archive_file_path), &file_nodes);

            // Assert
            assert_eq!(status, ArchiveStatus::OutdatedRequiresUpdate);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_check_archive_status_not_generated() {
        test_with_archiver(|archiver| {
            // Arrange
            let file_nodes = vec![];

            // Act
            let status = archiver.check_status(None, &file_nodes);

            // Assert
            assert_eq!(status, ArchiveStatus::NotYetGenerated);
        });
    }

    #[test]
    fn test_core_archiver_check_archive_status_archive_file_missing() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let missing_archive_path = dir.path().join("missing_archive.txt");
            let file_nodes = vec![];

            // Act
            let status = archiver.check_status(Some(&missing_archive_path), &file_nodes);

            // Assert
            assert_eq!(status, ArchiveStatus::ArchiveFileMissing);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_check_archive_status_no_files_selected() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            File::create(&archive_file_path)?.write_all(b"archive content")?;

            let file_nodes = vec![
                new_test_file_node(
                    dir.path(),
                    "file1.txt",
                    false,
                    SelectionState::Deselected,
                    vec![],
                ),
                new_test_file_node(dir.path(), "file2.txt", false, SelectionState::New, vec![]),
            ];

            // Act
            let status = archiver.check_status(Some(&archive_file_path), &file_nodes);

            // Assert
            assert_eq!(status, ArchiveStatus::NoFilesSelected);
            Ok(())
        })
    }

    #[test]
    fn test_check_archive_status_up_to_date() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path = dir.path().join("src_file.txt");

            File::create(&src_file_path)?.write_all(b"source")?;
            thread::sleep(Duration::from_millis(20));
            File::create(&archive_file_path)?.write_all(b"archive content")?;

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "src_file.txt",
                false,
                SelectionState::Selected,
                vec![],
            )];

            // Act
            let status = archiver.check_status(Some(&archive_file_path), &file_nodes);

            // Assert
            assert_eq!(status, ArchiveStatus::UpToDate);
            Ok(())
        })
    }

    #[test]
    fn test_check_archive_status_up_to_date_empty_archive_selected_older() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path = dir.path().join("src_file.txt");

            File::create(&src_file_path)?.write_all(b"source")?;
            thread::sleep(Duration::from_millis(20));
            File::create(&archive_file_path)?.write_all(b"")?;

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "src_file.txt",
                false,
                SelectionState::Selected,
                vec![],
            )];

            let src_ts = archiver.get_file_timestamp(&src_file_path)?;
            let archive_ts = archiver.get_file_timestamp(&archive_file_path)?;
            assert!(
                archive_ts > src_ts,
                "Test setup: archive should be newer than source"
            );

            // Act
            let status = archiver.check_status(Some(&archive_file_path), &file_nodes);

            // Assert
            assert_eq!(
                status,
                ArchiveStatus::UpToDate,
                "Expected UpToDate when archive is newer, even if empty, and selected files are older."
            );
            Ok(())
        })
    }

    #[test]
    fn test_check_archive_status_error_checking_src_missing() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            File::create(&archive_file_path)?.write_all(b"archive")?;

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "missing_src.txt",
                false,
                SelectionState::Selected,
                vec![],
            )];

            // Act
            let status = archiver.check_status(Some(&archive_file_path), &file_nodes);

            // Assert
            assert_eq!(
                status,
                ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound))
            );
            Ok(())
        })
    }

    #[test]
    fn test_create_archive_no_selected_files() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let base_path = dir.path();
            let nodes = vec![
                new_test_file_node(
                    base_path,
                    "file1.txt",
                    false,
                    SelectionState::Deselected,
                    vec![],
                ),
                new_test_file_node(base_path, "file2.txt", false, SelectionState::New, vec![]),
            ];

            // Act
            let archive = archiver.create_content(&nodes, base_path)?;

            // Assert
            let expected_content = format!("// Combined files from {}\n", base_path.display());
            assert_eq!(archive, expected_content);
            Ok(())
        })
    }

    #[test]
    fn test_create_archive_file_read_error() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let base_path = dir.path();
            let nodes = vec![new_test_file_node(
                base_path,
                "non_existent_file.txt",
                false,
                SelectionState::Selected,
                vec![],
            )];

            // Act
            let result = archiver.create_content(&nodes, base_path);

            // Assert
            assert!(result.is_err());
            if let Err(e) = result {
                assert_eq!(e.kind(), io::ErrorKind::NotFound);
            }
            Ok(())
        })
    }

    #[test]
    fn test_create_archive_ensure_newline_before_footer() -> io::Result<()> {
        test_with_archiver(|archiver| {
            // Arrange
            let dir = tempdir()?;
            let base_path = dir.path();

            let file_no_trailing_newline_path = base_path.join("no_newline.txt");
            let mut f = File::create(&file_no_trailing_newline_path)?;
            write!(f, "Line without trailing newline")?;
            drop(f);

            let nodes = vec![new_test_file_node(
                base_path,
                "no_newline.txt",
                false,
                SelectionState::Selected,
                vec![],
            )];

            // Act
            let archive = archiver.create_content(&nodes, base_path)?;

            // Assert
            let root_display_str = base_path.display();
            let expected_content = format!(
                "// Combined files from {}\n\
                 // ===== File: no_newline.txt =====\n\
                 Line without trailing newline\n",
                root_display_str
            );
            assert_eq!(archive, expected_content);
            Ok(())
        })
    }
}
