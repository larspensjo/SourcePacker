/*
 * This module consolidates the core, platform-agnostic logic of the application.
 * It re-exports key data structures and core functionalities (including abstractions
 * like `FileSystemScannerOperations`, `ProfileManagerOperations`, `ConfigManagerOperations`,
 * `ArchiverOperations`, `StateManagerOperations`, and the new `ProfileRuntimeDataOperations`)
 * for file system operations, profile management, configuration, archiving, state management,
 * and session data handling. It also includes utilities for token estimation.
 */
pub mod app_session_data;
pub mod archiver;
pub mod checksum_utils;
pub mod config;
pub mod file_system;
pub mod models;
pub mod profiles;
pub mod state_manager;
pub mod tokenizer_utils;

// Re-export key structures and enums
pub use models::{ArchiveStatus, FileNode, FileState, Profile};

// Re-export file system related items
pub use file_system::{CoreFileSystemScanner, FileSystemError, FileSystemScannerOperations};

// Re-export profile related items
pub use profiles::{
    CoreProfileManager, ProfileError, ProfileManagerOperations, sanitize_profile_name,
};

// Re-export archiver related items
pub use archiver::{ArchiverOperations, CoreArchiver};

// Re-export config related items
pub use config::{
    ConfigError, ConfigManagerOperations, CoreConfigManager as CoreConfigManagerForConfig,
};

pub use state_manager::{CoreStateManager, StateManagerOperations};

pub use tokenizer_utils::{
    CoreTikTokenCounter, SimpleWhitespaceTokenCounter, TokenCounterOperations,
};

// Re-export AppSessionData (now ProfileRuntimeData) related items
pub use app_session_data::{ProfileRuntimeData, ProfileRuntimeDataOperations};
