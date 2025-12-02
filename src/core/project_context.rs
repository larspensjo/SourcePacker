/*
 * Domain object representing a SourcePacker project root. It centralizes the
 * project-local layout (e.g., `.sourcepacker`, profile storage, pointer files)
 * so that callers use semantic resolvers instead of hand-built paths.
 * The intent is to keep filesystem topology knowledge inside `core`, while
 * higher layers work with the opaque `ProjectContext` value.
 */
use crate::core::profiles::sanitize_profile_name;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::path::{Component, Path, PathBuf};

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

/*
 * Strongly typed profile name with validation done at construction time.
 * Ensures caller checks naming rules once and then reuses the validated value.
 */
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileName {
    inner: String,
}

#[derive(Debug)]
pub enum ProfileNameError {
    Empty,
    InvalidChars(String),
}

impl std::fmt::Display for ProfileNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileNameError::Empty => write!(f, "Profile name is empty"),
            ProfileNameError::InvalidChars(s) => {
                write!(f, "Profile name contains invalid characters: {s}")
            }
        }
    }
}

impl std::error::Error for ProfileNameError {}

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

    pub(super) fn resolve_profile_file(&self, profile_name: &ProfileName) -> PathBuf {
        self.resolve_profiles_dir().join(format!(
            "{}.{PROFILE_FILE_EXTENSION}",
            profile_name.sanitized_for_filename()
        ))
    }
}

impl ProfileName {
    pub fn new<S: AsRef<str>>(name: S) -> Result<Self, ProfileNameError> {
        let name_ref = name.as_ref();
        if name_ref.trim().is_empty() {
            return Err(ProfileNameError::Empty);
        }
        let all_valid = name_ref
            .chars()
            .all(crate::core::profiles::is_valid_profile_name_char);
        if !all_valid {
            return Err(ProfileNameError::InvalidChars(name_ref.to_string()));
        }
        Ok(ProfileName {
            inner: name_ref.to_string(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn sanitized_for_filename(&self) -> String {
        sanitize_profile_name(&self.inner)
    }
}

impl std::fmt::Display for ProfileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<ProfileName> for String {
    fn from(value: ProfileName) -> Self {
        value.inner
    }
}

impl AsRef<str> for ProfileName {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl Serialize for ProfileName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProfileName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ProfileName::new(s).map_err(de::Error::custom)
    }
}

/*
 * Represents a project-relative path that has been validated to reside under
 * the project root. Prevents accidental path traversal or mixing absolute and
 * relative paths when performing project-scoped operations.
 */
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectRelativePath {
    relative: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum ProjectRelativePathError {
    OutsideRoot(PathBuf),
    NotRelative(PathBuf),
}

#[allow(dead_code)]
impl ProjectRelativePath {
    pub fn try_from_absolute(
        project: &ProjectContext,
        absolute: &Path,
    ) -> Result<Self, ProjectRelativePathError> {
        let abs = absolute
            .canonicalize()
            .unwrap_or_else(|_| absolute.to_path_buf());
        let root = project
            .root_path()
            .canonicalize()
            .unwrap_or_else(|_| project.root_path().to_path_buf());
        if !abs.starts_with(&root) {
            return Err(ProjectRelativePathError::OutsideRoot(abs));
        }
        let rel = abs
            .strip_prefix(&root)
            .map_err(|_| ProjectRelativePathError::OutsideRoot(abs.clone()))?;
        Ok(ProjectRelativePath {
            relative: rel.to_path_buf(),
        })
    }

    pub fn from_relative<P: Into<PathBuf>>(relative: P) -> Result<Self, ProjectRelativePathError> {
        let rel = relative.into();
        if rel.is_absolute() {
            return Err(ProjectRelativePathError::NotRelative(rel));
        }
        if rel.components().any(|c| c == Component::ParentDir) {
            return Err(ProjectRelativePathError::OutsideRoot(rel));
        }
        Ok(ProjectRelativePath { relative: rel })
    }

    pub fn to_absolute(&self, project: &ProjectContext) -> PathBuf {
        project.root_path().join(&self.relative)
    }

    pub fn as_path(&self) -> &Path {
        &self.relative
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
        let profile_name = ProfileName::new("My Profile").unwrap();

        // Act
        let resolved = ctx.resolve_profile_file(&profile_name);

        // Assert
        let expected_file_name = format!(
            "{}.{}",
            sanitize_profile_name(profile_name.as_str()),
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

    #[test]
    fn test_profile_name_validation_and_sanitization() {
        assert!(ProfileName::new("Good_Name-123").is_ok());
        assert!(matches!(ProfileName::new(""), Err(ProfileNameError::Empty)));
        assert!(matches!(
            ProfileName::new("Bad*Name"),
            Err(ProfileNameError::InvalidChars(_))
        ));

        let pn = ProfileName::new("My Profile").unwrap();
        assert_eq!(pn.as_str(), "My Profile");
        assert_eq!(pn.sanitized_for_filename(), "MyProfile");
    }

    #[test]
    fn test_project_relative_path_round_trip() {
        let project = ProjectContext::new(PathBuf::from("/root/project"));
        let abs = PathBuf::from("/root/project/src/main.rs");
        let rel = ProjectRelativePath::try_from_absolute(&project, &abs).unwrap();
        assert_eq!(rel.as_path(), Path::new("src/main.rs"));
        assert_eq!(rel.to_absolute(&project), abs);
    }

    #[test]
    fn test_project_relative_path_rejects_outside_root() {
        let project = ProjectContext::new(PathBuf::from("/root/project"));
        let abs = PathBuf::from("/root/other/file.txt");
        assert!(matches!(
            ProjectRelativePath::try_from_absolute(&project, &abs),
            Err(ProjectRelativePathError::OutsideRoot(_))
        ));
    }
}
