pub mod archiver;
pub mod config;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

/*
 * This module consolidates the core, platform-agnostic logic of the application.
 * It re-exports key data structures, core functionalities for file system operations
 * (including abstractions like `FileSystemScannerOperations`), profile management
 * (including `ProfileManagerOperations`), and configuration management
 * (including `ConfigManagerOperations`).
 */

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile};

// Re-export file system related items
pub use file_system::{
    CoreFileSystemScanner, // New concrete struct
    FileSystemError,
    FileSystemScannerOperations, // New trait
};
// Keep deprecated free function for now for compatibility during refactor
#[deprecated(
    since = "0.1.0",
    note = "Please use `FileSystemScannerOperations::scan_directory` via an injected manager instance."
)]
pub use file_system::scan_directory;

// Re-export profile related items
pub use profiles::{
    CoreProfileManager, ProfileError, ProfileManagerOperations, sanitize_profile_name,
};
// Keep deprecated free functions for now for compatibility during refactor
#[deprecated(
    since = "0.1.0",
    note = "Please use `ProfileManagerOperations::get_profile_dir_path` via an injected manager instance."
)]
pub use profiles::get_profile_dir;
#[deprecated(
    since = "0.1.0",
    note = "Please use `ProfileManagerOperations::list_profiles` via an injected manager instance."
)]
pub use profiles::list_profiles;
#[deprecated(
    since = "0.1.0",
    note = "Please use `ProfileManagerOperations::load_profile` via an injected manager instance."
)]
pub use profiles::load_profile;
#[deprecated(
    since = "0.1.0",
    note = "Please use `ProfileManagerOperations::save_profile` via an injected manager instance."
)]
pub use profiles::save_profile;

pub use archiver::{check_archive_status, create_archive_content, get_file_timestamp};

// Re-export config related items
#[deprecated(
    since = "0.1.0",
    note = "Please use `ConfigManagerOperations::load_last_profile_name` via an injected manager instance."
)]
pub use config::load_last_profile_name;
#[deprecated(
    since = "0.1.0",
    note = "Please use `ConfigManagerOperations::save_last_profile_name` via an injected manager instance."
)]
pub use config::save_last_profile_name;
pub use config::{
    ConfigError, ConfigManagerOperations, CoreConfigManager as CoreConfigManagerForConfig,
};

pub use state_manager::{apply_profile_to_tree, update_folder_selection};
