pub mod archiver;
pub mod config;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;

/*
 * This module consolidates the core, platform-agnostic logic of the application.
 * It re-exports key data structures and core functionalities (including abstractions
 * like `FileSystemScannerOperations`, `ProfileManagerOperations`, `ConfigManagerOperations`,
 * `ArchiverOperations`, and `StateManagerOperations`) for file system operations,
 * profile management, configuration, archiving, and state management.
 */

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile};

// Re-export file system related items
#[deprecated(
    since = "0.1.0",
    note = "Please use `FileSystemScannerOperations::scan_directory` via an injected manager instance."
)]
pub use file_system::scan_directory;
pub use file_system::{CoreFileSystemScanner, FileSystemError, FileSystemScannerOperations};

// Re-export profile related items
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
pub use profiles::{
    CoreProfileManager, ProfileError, ProfileManagerOperations, sanitize_profile_name,
};

// Re-export archiver related items
#[deprecated(
    since = "0.1.1",
    note = "Please use `ArchiverOperations::check_archive_status` via an injected manager instance."
)]
pub use archiver::check_archive_status;
#[deprecated(
    since = "0.1.1",
    note = "Please use `ArchiverOperations::create_archive_content` via an injected manager instance."
)]
pub use archiver::create_archive_content;
#[deprecated(
    since = "0.1.1",
    note = "Please use `ArchiverOperations::get_file_timestamp` via an injected manager instance."
)]
pub use archiver::get_file_timestamp;
pub use archiver::{ArchiverOperations, CoreArchiver};

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

// Re-export state_manager related items
#[deprecated(
    since = "0.1.2",
    note = "Please use `StateManagerOperations::apply_profile_to_tree` via an injected manager instance."
)]
pub use state_manager::apply_profile_to_tree;
#[deprecated(
    since = "0.1.2",
    note = "Please use `StateManagerOperations::update_folder_selection` via an injected manager instance."
)]
pub use state_manager::update_folder_selection;
pub use state_manager::{CoreStateManager, StateManagerOperations}; // New exports
