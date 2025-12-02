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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolvers_from_simple_root() {
        // Arrange
        let root = PathBuf::from(r"C:\project");
        let ctx = ProjectContext::new(root.clone());

        // Act
        let config_dir = ctx.resolve_config_dir();
        let profiles_dir = ctx.resolve_profiles_dir();
        let last_profile_file = ctx.resolve_last_profile_pointer_file();

        // Assert
        assert_eq!(config_dir, root.join(PROJECT_CONFIG_DIR_NAME));
        assert_eq!(
            profiles_dir,
            root.join(PROJECT_CONFIG_DIR_NAME)
                .join(PROFILES_SUBFOLDER_NAME)
        );
        assert_eq!(
            last_profile_file,
            root.join(PROJECT_CONFIG_DIR_NAME)
                .join(LAST_PROFILE_FILENAME)
        );
    }

    #[test]
    fn test_resolve_profile_file_uses_sanitization() {
        // Arrange
        let root = PathBuf::from(r"C:\project");
        let ctx = ProjectContext::new(root.clone());
        let profile_name = "My Profile!";

        // Act
        let resolved = ctx.resolve_profile_file(profile_name);

        // Assert
        let expected_file_name = format!(
            "{}.{}",
            sanitize_profile_name(profile_name),
            PROFILE_FILE_EXTENSION
        );
        let expected_path = root
            .join(PROJECT_CONFIG_DIR_NAME)
            .join(PROFILES_SUBFOLDER_NAME)
            .join(expected_file_name);
        assert_eq!(resolved, expected_path);
    }

    #[test]
    fn test_display_name_returns_folder_name() {
        // Arrange
        let root = PathBuf::from(r"C:\work\my_project");
        let ctx = ProjectContext::new(root);

        // Act
        let display = ctx.display_name();

        // Assert
        assert_eq!(display, "my_project");
    }
}
