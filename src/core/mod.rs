pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

// Re-export
pub use file_system::scan_directory;
pub use models::{FileNode, FileState, Profile};

// Re-export profile management functions
pub use profiles::{
    ProfileError, // Also re-export the error type for this module
    get_profile_dir,
    list_profiles,
    load_profile,
    save_profile,
};

pub use state_manager::{apply_profile_to_tree, update_folder_selection};
