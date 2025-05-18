use super::models::{ArchiveStatus, FileNode, FileState, Profile}; // Added ArchiveStatus
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime; // For timestamps

/*
 * This module handles operations related to creating and managing the state of archives.
 * It includes functions to generate archive content from selected files,
 * retrieve file timestamps, and check the synchronization status of an archive
 * against its source files.
 */

/*
 * Creates a concatenated string of content from selected files in the tree.
 * Traverses the provided `nodes` (typically the top-level nodes of a scanned directory).
 * For each `FileNode` that is a file and has `FileState::Selected`, it reads its content (UTF-8),
 * and prepends/appends a header/footer.
 */
pub fn create_archive_content(
    nodes: &[FileNode],
    root_path_for_display: &Path,
) -> io::Result<String> {
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
        } else {
            if node.state == FileState::Selected {
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
                        return Err(e); // Propagate error
                    }
                }
                archive_content.push_str(&format!("--- END FILE: {} ---\n\n", display_path));
            }
        }
    }

    Ok(archive_content)
}

pub fn get_file_timestamp(path: &Path) -> io::Result<SystemTime> {
    fs::metadata(path)?.modified()
}

/*
 * Checks the synchronization status of a profile's archive file.
 * It compares the archive's timestamp against the newest timestamp among the
 * selected source files within the provided `file_nodes_tree`.
 * The `file_nodes_tree` should represent the complete scanned file structure for the profile's root.
 */
pub fn check_archive_status(
    profile: &Profile,
    file_nodes_tree: &[FileNode], // The full tree to find selected files from
) -> ArchiveStatus {
    let archive_path = match &profile.archive_path {
        Some(p) => p,
        None => return ArchiveStatus::NotYetGenerated,
    };

    let archive_timestamp = match get_file_timestamp(archive_path) {
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
    fn find_newest_selected(
        nodes: &[FileNode],
        current_newest: &mut Option<SystemTime>,
        has_any_selected: &mut bool,
    ) -> io::Result<()> {
        for node in nodes {
            if node.is_dir {
                find_newest_selected(&node.children, current_newest, has_any_selected)?;
            } else if node.state == FileState::Selected {
                *has_any_selected = true;
                let file_ts = get_file_timestamp(&node.path)?;
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
        file_nodes_tree,
        &mut newest_selected_file_timestamp,
        &mut has_selected_files,
    ) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error checking source file timestamps: {}", e);
            // If a selected file is missing or unreadable, it's an error state for checking.
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
        None => {
            // This case should be covered by `NoFilesSelected` if `has_selected_files` remains false.
            // If `has_selected_files` is true but `newest_selected_file_timestamp` is None,
            // it implies an issue with timestamp retrieval that wasn't an IO error (unlikely with current logic).
            // For safety, treat as error.
            ArchiveStatus::ErrorChecking(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{FileNode, FileState, Profile};
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::thread; // For introducing delays
    use std::time::Duration; // For delays
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

    #[test]
    fn test_create_archive_from_selected_files() -> io::Result<()> {
        let dir = tempdir()?;
        let base_path = dir.path();

        let file1_path = base_path.join("file1.txt");
        let mut f1 = File::create(&file1_path)?;
        writeln!(f1, "Content of file1.")?;
        drop(f1); // Ensure closed

        let subdir_path = base_path.join("subdir");
        fs::create_dir_all(&subdir_path)?;
        let file2_path = subdir_path.join("file2.rs");
        let mut f2 = File::create(&file2_path)?;
        writeln!(f2, "Content of file2 (Rust).")?;
        writeln!(f2, "Another line.")?;
        drop(f2); // Ensure closed

        let file3_path = base_path.join("file3.md");
        let mut f3 = File::create(&file3_path)?;
        writeln!(f3, "Content of file3 (Markdown).")?;
        drop(f3); // Ensure closed

        let nodes = vec![
            new_test_file_node(base_path, "file1.txt", false, FileState::Selected, vec![]),
            new_test_file_node(
                base_path,
                "subdir",
                true,
                FileState::Unknown, // Parent folder state doesn't matter for content generation here
                vec![new_test_file_node(
                    &subdir_path, // Use subdir_path for child
                    "file2.rs",
                    false,
                    FileState::Selected,
                    vec![],
                )],
            ),
            new_test_file_node(base_path, "file3.md", false, FileState::Deselected, vec![]),
            new_test_file_node(base_path, "empty_dir", true, FileState::Unknown, vec![]),
            new_test_file_node(
                base_path,
                "file4_unknown.txt",
                false,
                FileState::Unknown,
                vec![],
            ),
        ];

        let archive = create_archive_content(&nodes, base_path)?;

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
    }

    #[test]
    fn test_create_archive_no_selected_files() -> io::Result<()> {
        let dir = tempdir()?;
        let base_path = dir.path();
        let nodes = vec![
            new_test_file_node(base_path, "file1.txt", false, FileState::Deselected, vec![]),
            new_test_file_node(base_path, "file2.txt", false, FileState::Unknown, vec![]),
        ];
        let archive = create_archive_content(&nodes, base_path)?;
        assert_eq!(archive, "");
        Ok(())
    }

    #[test]
    fn test_create_archive_file_read_error() -> io::Result<()> {
        let dir = tempdir()?;
        let base_path = dir.path();

        let nodes = vec![new_test_file_node(
            base_path,
            "non_existent_file.txt",
            false,
            FileState::Selected,
            vec![],
        )];

        let result = create_archive_content(&nodes, base_path);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
        Ok(())
    }

    #[test]
    fn test_create_archive_ensure_newline_before_footer() -> io::Result<()> {
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

        let archive = create_archive_content(&nodes, base_path)?;

        let expected_content = concat!(
            "--- START FILE: no_newline.txt ---\n",
            "Line without trailing newline\n",
            "--- END FILE: no_newline.txt ---\n\n"
        );
        assert_eq!(archive, expected_content);
        Ok(())
    }

    #[test]
    fn test_get_file_timestamp_exists() -> io::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("test_ts.txt");
        File::create(&file_path)?.write_all(b"content")?;
        thread::sleep(Duration::from_millis(10)); // Ensure time difference

        let ts = get_file_timestamp(&file_path)?;
        assert!(ts <= SystemTime::now()); // Basic sanity check
        Ok(())
    }

    #[test]
    fn test_get_file_timestamp_not_exists() {
        let dir = tempdir().expect("Failed to create temp dir");
        let file_path = dir.path().join("non_existent_ts.txt");
        let result = get_file_timestamp(&file_path);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_check_archive_status_not_generated() {
        let profile = Profile::new("test".into(), PathBuf::from("/root")); // No archive_path
        let file_nodes = vec![];
        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(status, ArchiveStatus::NotYetGenerated);
    }

    #[test]
    fn test_check_archive_status_archive_file_missing() -> io::Result<()> {
        let dir = tempdir()?;
        let mut profile = Profile::new("test".into(), PathBuf::from("/root"));
        profile.archive_path = Some(dir.path().join("missing_archive.txt")); // Path set, but file won't exist

        let file_nodes = vec![];
        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(status, ArchiveStatus::ArchiveFileMissing);
        Ok(())
    }

    #[test]
    fn test_check_archive_status_no_files_selected() -> io::Result<()> {
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
                FileState::Deselected,
                vec![],
            ),
            new_test_file_node(dir.path(), "file2.txt", false, FileState::Unknown, vec![]),
        ];
        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(status, ArchiveStatus::NoFilesSelected);
        Ok(())
    }

    #[test]
    fn test_check_archive_status_up_to_date() -> io::Result<()> {
        let dir = tempdir()?;
        let archive_file_path = dir.path().join("archive.txt");
        let src_file_path = dir.path().join("src_file.txt");

        // Create src file first
        File::create(&src_file_path)?.write_all(b"source")?;
        thread::sleep(Duration::from_millis(20)); // Ensure src is older or same

        // Create archive file later (or same time)
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

        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(status, ArchiveStatus::UpToDate);
        Ok(())
    }

    #[test]
    fn test_check_archive_status_up_to_date_empty_archive_selected_older() -> io::Result<()> {
        let dir = tempdir()?;
        let archive_file_path = dir.path().join("archive.txt");
        let src_file_path = dir.path().join("src_file.txt");

        // Create src file first
        File::create(&src_file_path)?.write_all(b"source")?;
        thread::sleep(Duration::from_millis(20));

        // Create archive file later (newer)
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

        // Sanity check timestamps
        let src_ts = get_file_timestamp(&src_file_path)?;
        let archive_ts = get_file_timestamp(&archive_file_path)?;
        assert!(
            archive_ts > src_ts,
            "Test setup: archive should be newer than source"
        );

        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(
            status,
            ArchiveStatus::UpToDate,
            "Expected UpToDate when archive is newer, even if empty, and selected files are older."
        );
        Ok(())
    }

    #[test]
    fn test_check_archive_status_outdated() -> io::Result<()> {
        let dir = tempdir()?;
        let archive_file_path = dir.path().join("archive.txt");
        let src_file_path1 = dir.path().join("src_file1.txt");
        let src_file_path2 = dir.path().join("src_file2.txt"); // Newer source file

        // Create archive file first
        File::create(&archive_file_path)?.write_all(b"old archive content")?;
        thread::sleep(Duration::from_millis(20)); // Ensure archive is older

        // Create source files
        File::create(&src_file_path1)?.write_all(b"source1")?;
        thread::sleep(Duration::from_millis(20)); // Make src2 definitively newer
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

        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(status, ArchiveStatus::OutdatedRequiresUpdate);
        Ok(())
    }

    #[test]
    fn test_check_archive_status_error_checking_src_missing() -> io::Result<()> {
        let dir = tempdir()?;
        let archive_file_path = dir.path().join("archive.txt");
        File::create(&archive_file_path)?.write_all(b"archive")?;

        let mut profile = Profile::new("test".into(), dir.path().to_path_buf());
        profile.archive_path = Some(archive_file_path);

        let file_nodes = vec![new_test_file_node(
            dir.path(),
            "missing_src.txt",
            false,
            FileState::Selected,
            vec![],
        )]; // Selected file doesn't exist

        let status = check_archive_status(&profile, &file_nodes);
        assert_eq!(
            status,
            ArchiveStatus::ErrorChecking(Some(io::ErrorKind::NotFound))
        );
        Ok(())
    }
}
