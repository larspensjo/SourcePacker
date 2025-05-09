mod core;
mod ui_facade;

use crate::core::{FileNode, scan_directory}; // Import scan_directory and FileNode
use std::path::PathBuf;
use ui_facade::{App, UiResult, WindowBuilder}; // For creating a dummy PathBuf

use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;
fn main() -> UiResult<()> {
    let app = App::new()?;
    println!("App instance: {:?}", app.instance);

    // main_window does not need to be mutable if populate_treeview_with_data takes &self
    let main_window = WindowBuilder::new(app)
        .title("SourcePacker - TreeView with Checkboxes")
        .size(800, 600)
        .on_destroy(|| {
            println!("Main window on_destroy callback: Quitting application.");
            unsafe {
                PostQuitMessage(0);
            }
        })
        .build()?;
    println!("Window HWND: {:?}", main_window.hwnd);

    let root_to_scan = PathBuf::from("/dummy_root"); // For display purposes
    let file_nodes_data = vec![
        FileNode::new(
            root_to_scan.join("file1.txt"),
            "file1.txt".to_string(),
            false,
        ),
        FileNode {
            path: root_to_scan.join("src"),
            name: "src".to_string(),
            is_dir: true,
            state: core::FileState::Unknown,
            children: vec![
                FileNode::new(
                    root_to_scan.join("src/main.rs"),
                    "main.rs".to_string(),
                    false,
                ),
                FileNode {
                    // Example of a pre-selected child
                    path: root_to_scan.join("src/lib.rs"),
                    name: "lib.rs".to_string(),
                    is_dir: false,
                    state: core::FileState::Selected, // Pre-select this one
                    children: vec![],
                },
            ],
        },
        FileNode::new(
            root_to_scan.join("README.md"),
            "README.md".to_string(),
            false,
        ),
    ];

    main_window.show();

    if !file_nodes_data.is_empty() {
        main_window.populate_treeview_with_data(file_nodes_data); // Use the new method
    } else {
        println!("No file nodes to populate TreeView.");
    }

    println!("Running app loop...");
    let run_result = app.run();
    println!("App loop exited.");
    run_result
}
