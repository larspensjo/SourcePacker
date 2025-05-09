// src/main.rs

mod core;
mod ui_facade;

use crate::core::{FileNode, scan_directory}; // Import scan_directory and FileNode
use std::path::PathBuf;
use ui_facade::{App, UiResult, WindowBuilder}; // For creating a dummy PathBuf

use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

fn main() -> UiResult<()> {
    let app = App::new()?;
    println!("App instance: {:?}", app.instance);

    let main_window = WindowBuilder::new(app)
        .title("SourcePacker - TreeView")
        .size(800, 600)
        .on_destroy(|| {
            println!("Main window on_destroy callback: Quitting application.");
            unsafe {
                PostQuitMessage(0);
            }
        })
        .build()?;
    println!("Window HWND: {:?}", main_window.hwnd);

    // --- Populate TreeView (P2.1) ---
    // 1. Scan a directory to get FileNode data (use a real or dummy path for now)
    //    For testing, let's create a dummy tree or scan a small, known directory.
    //    Replace "C:\\Windows" or "." with a small test directory on your system.
    //    Ensure the path exists. Or create dummy FileNode data.

    // Option A: Scan a real directory (replace with a small, safe directory)
    // let root_to_scan = PathBuf::from("."); // Current directory
    // let whitelist_patterns: Vec<String> = vec!["*.rs".to_string(), "*.toml".to_string()]; // Example
    // let file_nodes = match scan_directory(&root_to_scan, &whitelist_patterns) {
    //     Ok(nodes) => {
    //         if nodes.is_empty() {
    //             println!("Scanned directory but found no matching files/folders for TreeView.");
    //         }
    //         nodes
    //     }
    //     Err(e) => {
    //         eprintln!("Error scanning directory for TreeView: {:?}", e);
    //         Vec::new() // Populate with empty if scan fails
    //     }
    // };

    // Option B: Create dummy FileNode data for initial testing
    let root_to_scan = PathBuf::from("/dummy_root"); // For display purposes in headers
    let file_nodes = vec![
        FileNode::new(
            root_to_scan.join("file1.txt"),
            "file1.txt".to_string(),
            false,
        ),
        FileNode {
            path: root_to_scan.join("src"),
            name: "src".to_string(),
            is_dir: true,
            state: core::FileState::Unknown, // Assuming FileState is in core::models
            children: vec![
                FileNode::new(
                    root_to_scan.join("src/main.rs"),
                    "main.rs".to_string(),
                    false,
                ),
                FileNode::new(root_to_scan.join("src/lib.rs"), "lib.rs".to_string(), false),
            ],
        },
        FileNode::new(
            root_to_scan.join("README.md"),
            "README.md".to_string(),
            false,
        ),
    ];
    // --- End TreeView Population Data Setup ---

    // Window must be shown *before* populating controls if controls depend on window size,
    // or if WM_CREATE is where controls are made visible.
    // However, our TreeView is created in WM_CREATE and sized in WM_SIZE.
    // So, showing first is fine.
    main_window.show();

    // Populate the TreeView after it has been created (in WM_CREATE) and window is shown.
    // It's generally safe to do this after show() as WM_CREATE would have run.
    if !file_nodes.is_empty() {
        main_window.populate_treeview(&file_nodes);
    } else {
        println!("No file nodes to populate TreeView.");
    }

    println!("Running app loop...");
    let run_result = app.run();
    println!("App loop exited.");
    run_result
}
