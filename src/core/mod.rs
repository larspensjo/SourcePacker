// src/core/mod.rs

pub mod models;

// Re-export the data structures for easier access from other parts of the application.
pub use models::{FileNode, FileState, Profile};
