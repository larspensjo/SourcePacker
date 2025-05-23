use super::models::{ArchiveStatus, FileNode, FileState, Profile};
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
    fn create_archive_content(
        &self,
        nodes: &[FileNode],
        root_path_for_display: &Path,
    ) -> io::Result<String>;

    /*
     * Checks the synchronization status of a profile's archive file.
     * Compares the archive's timestamp against the newest timestamp among the
     * selected source files within the `file_nodes_tree`.
     */
    fn check_archive_status(
        &self,
        profile: &Profile,
        file_nodes_tree: &[FileNode],
    ) -> ArchiveStatus;

    /*
     * Saves the provided archive `content` string to the specified `path`.
     * Implementations should handle overwriting the file if it exists.
     */
    fn save_archive_content(&self, path: &Path, content: &str) -> io::Result<()>;

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
    fn create_archive_content(
        &self,
        nodes: &[FileNode],
        root_path_for_display: &Path,
    ) -> io::Result<String> {
        // Logic moved from the old free function
        let mut archive_content = String::new();
        let mut buffer = Vec::new();

        for node in nodes.iter().rev() {
            buffer.push(node);
        }

        while let Some(node) = buffer.pop() {
            if node.is_dir {
                for child in node.children.iter().rev() {
                    buffer.push(child);
                }
            } else if node.state == FileState::Selected {
                let display_path = node
                    .path
                    .strip_prefix(root_path_for_display)
                    .unwrap_or(&node.path)
                    .to_string_lossy();

                archive_content.push_str(&format!("--- START FILE: {} ---\n", display_path));

                match fs::read_to_string(&node.path) {
                    Ok(content) => {
                        archive_content.push_str(&content);
                        if !content.ends_with('\n') {
                            archive_content.push('\n');
                        }
                    }
                    Err(e) => {
                        archive_content.push_str(&format!(
                            "!!! ERROR READING FILE: {} - {} !!!\n",
                            display_path, e
                        ));
                        return Err(e);
                    }
                }
                archive_content.push_str(&format!("--- END FILE: {} ---\n\n", display_path));
            }
        }
        Ok(archive_content)
    }

    fn get_file_timestamp(&self, path: &Path) -> io::Result<SystemTime> {
        // Logic moved from the old free function
        fs::metadata(path)?.modified()
    }

    fn check_archive_status(
        &self,
        profile: &Profile,
        file_nodes_tree: &[FileNode],
    ) -> ArchiveStatus {
        // Logic moved from the old free function, uses self.get_file_timestamp
        let archive_path = match &profile.archive_path {
            Some(p) => p,
            None => return ArchiveStatus::NotYetGenerated,
        };

        let archive_timestamp = match self.get_file_timestamp(archive_path) {
            Ok(ts) => ts,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return ArchiveStatus::ArchiveFileMissing;
            }
            Err(e) => {
                eprintln!(
                    "Error getting archive timestamp for {:?}: {}",
                    archive_path, e
                );
                return ArchiveStatus::ErrorChecking(Some(e.kind()));
            }
        };

        let mut newest_selected_file_timestamp: Option<SystemTime> = None;
        let mut has_selected_files = false;

        // Helper recursive function to iterate through nodes and find selected files
        // It now needs to take `&dyn ArchiverOperations` to call `get_file_timestamp`.
        fn find_newest_selected(
            archiver: &dyn ArchiverOperations, // Pass self or a reference to the trait object
            nodes: &[FileNode],
            current_newest: &mut Option<SystemTime>,
            has_any_selected: &mut bool,
        ) -> io::Result<()> {
            for node in nodes {
                if node.is_dir {
                    find_newest_selected(
                        archiver,
                        &node.children,
                        current_newest,
                        has_any_selected,
                    )?;
                } else if node.state == FileState::Selected {
                    *has_any_selected = true;
                    // Call via the archiver trait object
                    let file_ts = archiver.get_file_timestamp(&node.path)?;
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

        match find_newest_selected(
            self, // Pass self as the ArchiverOperations impl
            file_nodes_tree,
            &mut newest_selected_file_timestamp,
            &mut has_selected_files,
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error checking source file timestamps: {}", e);
                return ArchiveStatus::ErrorChecking(Some(e.kind()));
            }
        }

        if !has_selected_files {
            return ArchiveStatus::NoFilesSelected;
        }

        match newest_selected_file_timestamp {
            Some(newest_src_ts) => {
                if newest_src_ts > archive_timestamp {
                    ArchiveStatus::OutdatedRequiresUpdate
                } else {
                    ArchiveStatus::UpToDate
                }
            }
            None => ArchiveStatus::ErrorChecking(None),
        }
    }

    fn save_archive_content(&self, path: &Path, content: &str) -> io::Result<()> {
        fs::write(path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{FileNode, FileState, Profile};
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
        state: FileState,
        children: Vec<FileNode>,
    ) -> FileNode {
        FileNode {
            path: base_path.join(name),
            name: name.to_string(),
            is_dir,
            state,
            children,
        }
    }

    // Test helper for ArchiverOperations using CoreArchiver
    fn test_with_archiver<F, R>(test_fn: F) -> R
    where
        F: FnOnce(&dyn ArchiverOperations) -> R,
    {
        let archiver = CoreArchiver::new();
        test_fn(&archiver)
    }

    #[test]
    fn test_core_archiver_create_archive_from_selected_files() -> io::Result<()> {
        test_with_archiver(|archiver| {
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
            File::create(&file3_path)?.sync_all()?; // Create an empty file for this test case

            let nodes = vec![
                new_test_file_node(base_path, "file1.txt", false, FileState::Selected, vec![]),
                new_test_file_node(
                    base_path,
                    "subdir",
                    true,
                    FileState::Unknown,
                    vec![new_test_file_node(
                        &subdir_path,
                        "file2.rs",
                        false,
                        FileState::Selected,
                        vec![],
                    )],
                ),
                new_test_file_node(
                    base_path,
                    "file3.md",
                    false,
                    FileState::Deselected, // This file is deselected
                    vec![],
                ),
            ];

            let archive = archiver.create_archive_content(&nodes, base_path)?;

            let path1_display = "file1.txt";
            let path2_display_os_specific = PathBuf::from("subdir").join("file2.rs");
            let path2_display_str = path2_display_os_specific.to_string_lossy();

            let expected_content = format!(
                "--- START FILE: {} ---\n\
                 Content of file1.\n\
                 --- END FILE: {} ---\n\n\
                 --- START FILE: {} ---\n\
                 Content of file2 (Rust).\n\
                 Another line.\n\
                 --- END FILE: {} ---\n\n",
                path1_display, path1_display, path2_display_str, path2_display_str
            );
            assert_eq!(archive, expected_content);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_save_archive_content() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_path = dir.path().join("my_archive.txt");
            let content = "This is the test archive content.";

            archiver.save_archive_content(&archive_path, content)?;

            let read_content = fs::read_to_string(&archive_path)?;
            assert_eq!(read_content, content);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_get_file_timestamp_exists() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let file_path = dir.path().join("test_ts.txt");
            File::create(&file_path)?.write_all(b"content")?;
            thread::sleep(Duration::from_millis(10)); // Ensure time difference for timestamp

            let ts = archiver.get_file_timestamp(&file_path)?;
            assert!(ts <= SystemTime::now()); // Basic sanity check
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_get_file_timestamp_not_exists() {
        // Renamed to match original, and adapted
        test_with_archiver(|archiver| {
            let dir = tempdir().expect("Failed to create temp dir");
            let file_path = dir.path().join("non_existent_ts.txt");
            let result = archiver.get_file_timestamp(&file_path);
            assert!(result.is_err());
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotFound);
        });
    }

    #[test]
    fn test_core_archiver_check_archive_status_outdated() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path1 = dir.path().join("src_file1.txt");
            let src_file_path2 = dir.path().join("src_file2.txt"); // Newer source file

            File::create(&archive_file_path)?.write_all(b"old archive content")?;
            thread::sleep(Duration::from_millis(20));

            File::create(&src_file_path1)?.write_all(b"source1")?;
            thread::sleep(Duration::from_millis(20));
            File::create(&src_file_path2)?.write_all(b"source2")?;

            let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
            profile.archive_path = Some(archive_file_path);

            let file_nodes = vec![
                new_test_file_node(
                    dir.path(),
                    "src_file1.txt",
                    false,
                    FileState::Selected,
                    vec![],
                ),
                new_test_file_node(
                    dir.path(),
                    "src_file2.txt",
                    false,
                    FileState::Selected,
                    vec![],
                ),
            ];

            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(status, ArchiveStatus::OutdatedRequiresUpdate);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_check_archive_status_not_generated() {
        test_with_archiver(|archiver| {
            let profile = Profile::new("test".into(), PathBuf::from("/root")); // No archive_path
            let file_nodes = vec![];
            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(status, ArchiveStatus::NotYetGenerated);
        });
    }

    #[test]
    fn test_core_archiver_check_archive_status_archive_file_missing() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let mut profile = Profile::new("test".into(), PathBuf::from("/root"));
            profile.archive_path = Some(dir.path().join("missing_archive.txt")); // Path set, but file won't exist

            let file_nodes = vec![];
            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(status, ArchiveStatus::ArchiveFileMissing);
            Ok(())
        })
    }

    #[test]
    fn test_core_archiver_check_archive_status_no_files_selected() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            File::create(&archive_file_path)?.write_all(b"archive content")?;

            let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
            profile.archive_path = Some(archive_file_path);

            let file_nodes = vec![
                new_test_file_node(
                    dir.path(),
                    "file1.txt",
                    false,
                    FileState::Deselected, // Not selected
                    vec![],
                ),
                new_test_file_node(
                    dir.path(),
                    "file2.txt",
                    false,
                    FileState::Unknown, // Not selected
                    vec![],
                ),
            ];
            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(status, ArchiveStatus::NoFilesSelected);
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_check_archive_status_up_to_date() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path = dir.path().join("src_file.txt");

            File::create(&src_file_path)?.write_all(b"source")?;
            thread::sleep(Duration::from_millis(20));

            File::create(&archive_file_path)?.write_all(b"archive content")?;

            let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
            profile.archive_path = Some(archive_file_path);

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "src_file.txt",
                false,
                FileState::Selected,
                vec![],
            )];

            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(status, ArchiveStatus::UpToDate);
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_check_archive_status_up_to_date_empty_archive_selected_older() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            let src_file_path = dir.path().join("src_file.txt");

            File::create(&src_file_path)?.write_all(b"source")?;
            thread::sleep(Duration::from_millis(20));

            File::create(&archive_file_path)?.write_all(b"")?; // Empty archive

            let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
            profile.archive_path = Some(archive_file_path.clone());

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "src_file.txt",
                false,
                FileState::Selected,
                vec![],
            )];

            let src_ts = archiver.get_file_timestamp(&src_file_path)?;
            let archive_ts = archiver.get_file_timestamp(&archive_file_path)?;
            assert!(
                archive_ts > src_ts,
                "Test setup: archive should be newer than source"
            );

            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(
                status,
                ArchiveStatus::UpToDate,
                "Expected UpToDate when archive is newer, even if empty, and selected files are older."
            );
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_check_archive_status_error_checking_src_missing() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let archive_file_path = dir.path().join("archive.txt");
            File::create(&archive_file_path)?.write_all(b"archive")?;

            let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
            profile.archive_path = Some(archive_file_path);

            let file_nodes = vec![new_test_file_node(
                dir.path(),
                "missing_src.txt", // This file does not actually exist on disk
                false,
                FileState::Selected,
                vec![],
            )];

            let status = archiver.check_archive_status(&profile, &file_nodes);
            assert_eq!(
                status,
                ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound))
            );
            Ok(())
        })
    }

    // The original `archiver.rs` also had these tests, which should be adapted if not already.
    // I am adapting them based on the names from the provided `archiver.rs` in the prompt.
    #[test] // Added from original file content, now adapted
    fn test_create_archive_no_selected_files() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let base_path = dir.path();
            let nodes = vec![
                new_test_file_node(base_path, "file1.txt", false, FileState::Deselected, vec![]),
                new_test_file_node(base_path, "file2.txt", false, FileState::Unknown, vec![]),
            ];
            let archive = archiver.create_archive_content(&nodes, base_path)?;
            assert_eq!(archive, "");
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_create_archive_file_read_error() -> io::Result<()> {
        test_with_archiver(|archiver| {
            let dir = tempdir()?;
            let base_path = dir.path();

            let nodes = vec![new_test_file_node(
                base_path,
                "non_existent_file.txt", // This file won't exist
                false,
                FileState::Selected,
                vec![],
            )];

            let result = archiver.create_archive_content(&nodes, base_path);
            assert!(result.is_err());
            if let Err(e) = result {
                assert_eq!(e.kind(), io::ErrorKind::NotFound);
            }
            Ok(())
        })
    }

    #[test] // Added from original file content, now adapted
    fn test_create_archive_ensure_newline_before_footer() -> io::Result<()> {
        test_with_archiver(|archiver| {
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
                FileState::Selected,
                vec![],
            )];

            let archive = archiver.create_archive_content(&nodes, base_path)?;

            let expected_content = concat!(
                "--- START FILE: no_newline.txt ---\n",
                "Line without trailing newline\n", // archiver adds this newline
                "--- END FILE: no_newline.txt ---\n\n"
            );
            assert_eq!(archive, expected_content);
            Ok(())
        })
    }
}
