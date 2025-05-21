pub mod archiver;
pub mod config;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

/*
 * This module consolidates the core, platform-agnostic logic of the application.
 * It re-exports key data structures, core functionalities for file system operations,
 * profile management (including abstractions like `ProfileManagerOperations`),
 * and configuration management (including `ConfigManagerOperations`).
 */

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile};

// Re-export key functions/structs from submodules
pub use file_system::scan_directory;

// Re-export profile related items
pub use profiles::{
    CoreProfileManager, // New concrete struct
    ProfileError,
    ProfileManagerOperations, // New trait
    sanitize_profile_name,
};
// Keep deprecated free functions for now for compatibility during refactor
pub use profiles::{get_profile_dir, list_profiles, load_profile, save_profile};

pub use archiver::{check_archive_status, create_archive_content, get_file_timestamp};

// Re-export config related items
pub use config::{
    ConfigError, ConfigManagerOperations, CoreConfigManager as CoreConfigManagerForConfig,
}; // Alias to avoid name clash
pub use config::{load_last_profile_name, save_last_profile_name}; // These are also deprecated

pub use state_manager::{apply_profile_to_tree, update_folder_selection};
