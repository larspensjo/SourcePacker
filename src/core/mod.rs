pub mod archiver;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile}; // Added ArchiveStatus

// Re-export key functions from submodules
pub use file_system::scan_directory;

pub use profiles::{
    ProfileError, // Also re-export the error type for this module
    list_profiles,
    load_profile,
    save_profile,
};

pub use archiver::{check_archive_status, create_archive_content, get_file_timestamp}; // Added check_archive_status, get_file_timestamp
pub use state_manager::{apply_profile_to_tree, update_folder_selection};
