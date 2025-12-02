/*
 * Domain object representing a SourcePacker project root. It centralizes the
 * project-local layout (e.g., `.sourcepacker`, profile storage, pointer files)
 * so that callers use semantic resolvers instead of hand-built paths.
 * The intent is to keep filesystem topology knowledge inside `core`, while
 * higher layers work with the opaque `ProjectContext` value.
 */
use crate::core::profiles::sanitize_profile_name;
use std::path::{Path, PathBuf};

pub(super) const PROJECT_CONFIG_DIR_NAME: &str = ".sourcepacker";
pub(super) const PROFILES_SUBFOLDER_NAME: &str = "profiles";
pub(super) const PROFILE_FILE_EXTENSION: &str = "json";
pub(super) const LAST_PROFILE_FILENAME: &str = "last_profile.txt";

/*
 * Opaque handle to a project root. It wraps the root `PathBuf` and exposes
 * semantic path resolvers so only the core layer knows the internal folder
 * layout used for profiles and pointers.
 */
#[derive(Debug, Clone)]
pub struct ProjectContext {
    root: PathBuf,
}

impl ProjectContext {
    pub fn new(root: PathBuf) -> Self {
        ProjectContext { root }
    }

    pub fn display_name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    /*
     * Temporary helper for legacy call sites that still need the underlying
     * root path. Future phases will route operations through core managers
     * instead of exposing raw paths to app logic.
     */
    pub(crate) fn root_path(&self) -> &Path {
        &self.root
    }

    pub(super) fn resolve_root_for_serialization(&self) -> &Path {
        &self.root
    }

    pub(super) fn resolve_config_dir(&self) -> PathBuf {
        self.root.join(PROJECT_CONFIG_DIR_NAME)
    }

    pub(super) fn resolve_profiles_dir(&self) -> PathBuf {
        self.resolve_config_dir().join(PROFILES_SUBFOLDER_NAME)
    }

    pub(super) fn resolve_last_profile_pointer_file(&self) -> PathBuf {
        self.resolve_config_dir().join(LAST_PROFILE_FILENAME)
    }

    pub(super) fn resolve_profile_file(&self, profile_name: &str) -> PathBuf {
        let sanitized = sanitize_profile_name(profile_name);
        self.resolve_profiles_dir()
            .join(format!("{sanitized}.{PROFILE_FILE_EXTENSION}"))
    }
}
