pub mod archiver;
pub mod config;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile};

// Re-export key functions from submodules
pub use file_system::scan_directory;

pub use profiles::{
    ProfileError, get_profile_dir, list_profiles, load_profile, sanitize_profile_name, save_profile,
};

pub use archiver::{check_archive_status, create_archive_content, get_file_timestamp};
pub use config::{ConfigError, load_last_profile_name, save_last_profile_name};
pub use state_manager::{apply_profile_to_tree, update_folder_selection};
