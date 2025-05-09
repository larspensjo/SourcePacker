// src/core/archiver.rs

use super::models::{FileNode, FileState};
use std::fs;
use std::io;
use std::path::Path;

// Define a custom error type for this module (optional for now, can just return io::Result<String>)
// For simplicity in this step, we'll let fs::read_to_string propagate its io::Error.
// If more complex error handling is needed (e.g., partial success), define a custom error.

/// Creates a concatenated string of content from selected files in the tree.
///
/// Traverses the provided `nodes` (typically the top-level nodes of a scanned directory).
/// For each `FileNode` that is a file and has `FileState::Selected`:
/// - Reads its content (assumed UTF-8).
/// - Prepends a header: `--- START FILE: path/to/file ---`
/// - Appends a footer: `--- END FILE: path/to/file ---`
///
/// Returns an `io::Result<String>` where `Ok(String)` is the concatenated content,
/// and `Err` can occur if file reading fails.
pub fn create_archive_content(
    nodes: &[FileNode],
    root_path_for_display: &Path,
) -> io::Result<String> {
    let mut archive_content = String::new();
    let mut buffer = Vec::new(); // To store nodes for traversal (DFS)

    // Initialize buffer with top-level nodes, in reverse for correct pop order for DFS processing
    for node in nodes.iter().rev() {
        buffer.push(node);
    }

    while let Some(node) = buffer.pop() {
        if node.is_dir {
            // If it's a directory, add its children to the buffer for traversal (in reverse)
            for child in node.children.iter().rev() {
                buffer.push(child);
            }
        } else {
            // It's a file, check if selected
            if node.state == FileState::Selected {
                // Try to get a path relative to the root_path_for_display for cleaner headers
                let display_path = node
                    .path
                    .strip_prefix(root_path_for_display)
                    .unwrap_or(&node.path) // Fallback to full path if stripping fails
                    .to_string_lossy();

                println!(
                    "Attempting to read selected file: {:?}, (display_path: {})",
                    node.path, display_path
                );

                archive_content.push_str(&format!("--- START FILE: {} ---\n", display_path));

                match fs::read_to_string(&node.path) {
                    Ok(content) => {
                        archive_content.push_str(&content);
                        // Ensure a newline after file content if not already present, before footer
                        if !content.ends_with('\n') {
                            archive_content.push('\n');
                        }
                    }
                    Err(e) => {
                        // Option 1: Propagate the first error encountered
                        // return Err(e);

                        // Option 2: Append an error message to the archive and continue (more user-friendly)
                        archive_content.push_str(&format!(
                            "!!! ERROR READING FILE: {} - {} !!!\n",
                            display_path, e
                        ));
                        // No specific error type returned here, but the archive will contain the error.
                        // For now, let's choose to propagate the error as per io::Result.
                        // To append error messages, the function signature would need to change
                        // (e.g., return String and log errors separately, or return a custom Result).
                        // For this P1.5, propagating io::Error is simpler.
                        println!("Error reading file {:?}: {}", node.path, e);
                        return Err(e);
                    }
                }
                archive_content.push_str(&format!("--- END FILE: {} ---\n\n", display_path));
            }
        }
    }

    Ok(archive_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{FileNode, FileState}; // Ensure full path if used like this
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};
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

        // Create dummy files
        let file1_path = base_path.join("file1.txt");
        let mut f1 = File::create(&file1_path)?;
        writeln!(f1, "Content of file1.")?;

        let file2_path = base_path.join("subdir").join("file2.rs");
        fs::create_dir_all(base_path.join("subdir"))?;
        let mut f2 = File::create(&file2_path)?;
        writeln!(f2, "Content of file2 (Rust).")?;
        writeln!(f2, "Another line.")?; // Test with multiple lines

        let file3_path = base_path.join("file3.md"); // Not selected
        let mut f3 = File::create(&file3_path)?;
        writeln!(f3, "Content of file3 (Markdown).")?;

        let nodes = vec![
            new_test_file_node(base_path, "file1.txt", false, FileState::Selected, vec![]),
            new_test_file_node(
                base_path,
                "subdir",
                true,
                FileState::Unknown,
                vec![new_test_file_node(
                    &base_path.join("subdir"),
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

        // Construct expected path strings carefully
        let path1_display = "file1.txt";

        // Create platform-specific path for subdir/file2.rs
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

        // Non-existent file path for a selected node
        let nodes = vec![new_test_file_node(
            base_path,
            "non_existent_file.txt", // This path doesn't actually exist on disk
            false,
            FileState::Selected,
            vec![],
        )];

        let result = create_archive_content(&nodes, base_path);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
        Ok(()) // <-- Added Ok(()) at the end
    }

    #[test]
    fn test_create_archive_ensure_newline_before_footer() -> io::Result<()> {
        let dir = tempdir()?;
        let base_path = dir.path();

        let file_no_trailing_newline_path = base_path.join("no_newline.txt");
        let mut f = File::create(&file_no_trailing_newline_path)?;
        write!(f, "Line without trailing newline")?; // No \n

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
            "Line without trailing newline\n", // Newline added here
            "--- END FILE: no_newline.txt ---\n\n"
        );
        assert_eq!(archive, expected_content);
        Ok(())
    }
}
