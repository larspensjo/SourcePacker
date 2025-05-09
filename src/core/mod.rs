pub mod file_system;
pub mod models; // <-- Add this line

// Re-export the data structures and key functions
pub use file_system::scan_directory;
pub use models::{FileNode, FileState, Profile};
