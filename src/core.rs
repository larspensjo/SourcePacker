/*
 * This module consolidates the core, platform-agnostic logic of the application.
 * It re-exports key data structures and core functionalities (including abstractions
 * like `FileSystemScannerOperations`, `ProfileManagerOperations`, `ConfigManagerOperations`,
 * `ArchiverOperations`, `StateManagerOperations`, and the new `ProfileRuntimeDataOperations`)
 * for file system operations, profile management, configuration, archiving, state management,
 * and session data handling. It also includes utilities for token estimation and path utilities.
 */
pub mod archiver;
pub mod checksum_utils;
pub mod config;
pub mod file_node;
pub mod file_system;
pub mod node_state_applicator;
pub mod path_utils;
pub mod profile_runtime_data;
pub mod profiles;
pub mod token_progress;
pub mod tokenizer_utils;

// Re-export key structures and enums
pub use file_node::{ArchiveStatus, FileNode, Profile, SelectionState};

// Re-export file system related items
pub use file_system::{CoreFileSystemScanner, FileSystemScannerOperations};

#[cfg(test)]
pub use file_system::FileSystemError;

// Re-export profile related items
pub use profiles::{CoreProfileManager, ProfileManagerOperations};

#[cfg(test)]
pub use profiles::ProfileError;

// Re-export archiver related items
pub use archiver::{ArchiverOperations, CoreArchiver};

// Re-export config related items
pub use config::{ConfigManagerOperations, CoreConfigManager as CoreConfigManagerForConfig};

#[cfg(test)]
pub use config::ConfigError;

pub use node_state_applicator::{NodeStateApplicator, NodeStateApplicatorOperations};

pub use tokenizer_utils::{CoreTikTokenCounter, TokenCounterOperations};

// Re-export AppSessionData (now ProfileRuntimeData) related items
pub use profile_runtime_data::{
    ProfileRuntimeData, ProfileRuntimeDataOperations, TokenProgressChannel,
};

pub use token_progress::TokenProgress;
