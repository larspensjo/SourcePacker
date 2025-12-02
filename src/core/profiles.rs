/*
 * This module is responsible for managing user profiles. Profiles store user-specific
 * configurations, such as the root folder to monitor, selection states of files,
 * and associated archive paths. It provides mechanisms to save, load, and list
 * these profiles, abstracting the underlying storage (JSON files inside the
 * project-local `.sourcepacker/profiles` directory).
 *
 * It includes a trait for profile operations (`ProfileManagerOperations`) to facilitate
 * testing and dependency injection, and a concrete implementation (`CoreProfileManager`).
 * Profile storage now leverages the active project's `.sourcepacker` directory,
 * under which a "profiles" subfolder is used.
 */
use super::file_node::Profile;
use serde_json;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

pub const PROFILE_FILE_EXTENSION: &str = "json";
const PROFILES_SUBFOLDER_NAME: &str = "profiles";
pub const PROJECT_CONFIG_DIR_NAME: &str = ".sourcepacker";
const LAST_PROFILE_FILENAME: &str = "last_profile.txt";

#[derive(Debug)]
pub enum ProfileError {
    Io(io::Error),
    Serde(serde_json::Error),
    NoProjectDirectory,
    ProfileNotFound(String),
    InvalidProfileName(String),
}

impl From<io::Error> for ProfileError {
    fn from(err: io::Error) -> Self {
        ProfileError::Io(err)
    }
}

impl From<serde_json::Error> for ProfileError {
    fn from(err: serde_json::Error) -> Self {
        ProfileError::Serde(err)
    }
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::Io(e) => write!(f, "I/O error: {e}"),
            ProfileError::Serde(e) => write!(f, "Serialization/Deserialization error: {e}"),
            ProfileError::NoProjectDirectory => {
                write!(f, "Could not determine project directory for profiles")
            }
            ProfileError::ProfileNotFound(name) => write!(f, "Profile not found: {name}"),
            ProfileError::InvalidProfileName(name) => write!(
                f,
                "Invalid profile name: {name}. Contains invalid characters or is empty."
            ),
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProfileError::Io(e) => Some(e),
            ProfileError::Serde(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ProfileError>;

pub fn sanitize_profile_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

pub fn is_valid_profile_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == ' '
}

pub trait ProfileManagerOperations: Send + Sync {
    fn load_profile(
        &self,
        project_root: &Path,
        profile_name: &str,
        app_name: &str,
    ) -> Result<Profile>;
    fn load_profile_from_path(&self, path: &Path) -> Result<Profile>;
    fn save_profile(&self, project_root: &Path, profile: &Profile, app_name: &str) -> Result<()>;
    fn list_profiles(&self, project_root: &Path, app_name: &str) -> Result<Vec<String>>;
    fn get_profile_dir_path(&self, project_root: &Path, app_name: &str) -> Option<PathBuf>;
    fn save_last_profile_name_for_project(
        &self,
        project_root: &Path,
        profile_name: &str,
    ) -> Result<()>;
    fn load_last_profile_name_for_project(&self, project_root: &Path) -> Result<Option<String>>;
}

pub struct CoreProfileManager {}

impl CoreProfileManager {
    pub fn new() -> Self {
        CoreProfileManager {}
    }

    fn ensure_project_config_dir(project_root: &Path) -> Option<PathBuf> {
        let config_dir = project_root.join(PROJECT_CONFIG_DIR_NAME);
        if !config_dir.exists() {
            if let Err(e) = fs::create_dir_all(&config_dir) {
                log::error!(
                    "CoreProfileManager: Failed to create project config dir {config_dir:?}: {e}"
                );
                return None;
            }
            log::debug!("CoreProfileManager: Created project config directory: {config_dir:?}");
        } else {
            log::trace!(
                "CoreProfileManager: Project config directory already exists: {config_dir:?}"
            );
        }
        Some(config_dir)
    }

    /*
     * Ensures the project-local profile storage directory exists under
     * `<project_root>/.sourcepacker/profiles`.
     */
    fn get_profile_storage_dir_impl(project_root: &Path) -> Option<PathBuf> {
        let config_dir = CoreProfileManager::ensure_project_config_dir(project_root)?;

        let profiles_path = config_dir.join(PROFILES_SUBFOLDER_NAME);
        if !profiles_path.exists() {
            if let Err(e) = fs::create_dir_all(&profiles_path) {
                log::error!(
                    "CoreProfileManager: Failed to create profile storage directory {profiles_path:?}: {e}"
                );
                return None;
            }
            log::debug!("CoreProfileManager: Created profile storage directory: {profiles_path:?}");
        } else {
            log::trace!(
                "CoreProfileManager: Profile storage directory already exists: {profiles_path:?}"
            );
        }

        Some(profiles_path)
    }
}

impl Default for CoreProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileManagerOperations for CoreProfileManager {
    /*
     * Loads a profile by its name for a given application.
     * It uses `get_profile_storage_dir_impl` to determine the profile storage directory.
     * Profile names are sanitized before being used as filenames.
     */
    fn load_profile(
        &self,
        project_root: &Path,
        profile_name: &str,
        app_name: &str,
    ) -> Result<Profile> {
        log::trace!("CoreProfileManager: Loading profile '{profile_name}' for app '{app_name}'");
        if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
        }

        let dir = CoreProfileManager::get_profile_storage_dir_impl(project_root)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(profile_name);
        let file_path = dir.join(format!("{sanitized_filename}.{PROFILE_FILE_EXTENSION}"));

        if !file_path.exists() {
            log::debug!(
                "CoreProfileManager: Profile file {file_path:?} not found for profile '{profile_name}'."
            );
            return Err(ProfileError::ProfileNotFound(profile_name.to_string()));
        }

        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let profile: Profile = serde_json::from_reader(reader)?;
        log::debug!(
            "CoreProfileManager: Successfully loaded profile '{}' from {:?}.",
            profile.name, // Use profile.name as it's authoritative after load
            file_path
        );
        Ok(profile)
    }

    fn load_profile_from_path(&self, path: &Path) -> Result<Profile> {
        log::trace!("CoreProfileManager: Loading profile from path {path:?}");
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let profile: Profile = serde_json::from_reader(reader)?;
        log::debug!(
            "CoreProfileManager: Successfully loaded profile '{}' from path {:?}.",
            profile.name,
            path
        );
        Ok(profile)
    }

    /*
     * Saves a given profile for a specific application.
     * It uses `get_profile_storage_dir_impl` to determine the profile storage directory.
     * The profile's name is sanitized to derive the filename.
     */
    fn save_profile(&self, project_root: &Path, profile: &Profile, app_name: &str) -> Result<()> {
        log::trace!(
            "CoreProfileManager: Saving profile '{}' for app '{}'",
            profile.name,
            app_name
        );
        if profile.name.trim().is_empty() || !profile.name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile.name.clone()));
        }

        let dir = CoreProfileManager::get_profile_storage_dir_impl(project_root)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(&profile.name);
        let file_path = dir.join(format!("{sanitized_filename}.{PROFILE_FILE_EXTENSION}"));

        let file = File::create(&file_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, profile)?;
        log::debug!(
            "CoreProfileManager: Successfully saved profile '{}' to {:?}.",
            profile.name,
            file_path
        );
        Ok(())
    }

    /*
     * Lists the names of all available profiles for a given application.
     * It scans the directory returned by `get_profile_storage_dir_impl`.
     */
    fn list_profiles(&self, project_root: &Path, app_name: &str) -> Result<Vec<String>> {
        log::trace!("CoreProfileManager: Listing profiles for app '{app_name}'");
        let dir = match CoreProfileManager::get_profile_storage_dir_impl(project_root) {
            Some(d) => d,
            None => {
                log::debug!(
                    "CoreProfileManager: Profile storage directory not found for app '{app_name}', returning empty list."
                );
                return Ok(Vec::new());
            } // If base dir couldn't be obtained, no profiles dir exists.
        };

        let mut profile_names = Vec::new();
        if dir.exists() {
            // This check is a bit redundant as get_profile_storage_dir_impl ensures it.
            for entry_result in fs::read_dir(dir)? {
                let entry = entry_result?;
                let path = entry.path();
                if path.is_file()
                    && let Some(ext) = path.extension()
                    && ext == PROFILE_FILE_EXTENSION
                    && let Some(stem) = path.file_stem()
                {
                    profile_names.push(stem.to_string_lossy().into_owned());
                }
            }
        }
        profile_names.sort_unstable();
        log::debug!(
            "CoreProfileManager: Found {} profiles for app '{}'.",
            profile_names.len(),
            app_name
        );
        Ok(profile_names)
    }

    fn get_profile_dir_path(&self, project_root: &Path, _app_name: &str) -> Option<PathBuf> {
        CoreProfileManager::get_profile_storage_dir_impl(project_root)
    }

    fn save_last_profile_name_for_project(
        &self,
        project_root: &Path,
        profile_name: &str,
    ) -> Result<()> {
        if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
        }

        let config_dir = CoreProfileManager::ensure_project_config_dir(project_root)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let file_path = config_dir.join(LAST_PROFILE_FILENAME);

        let mut file = File::create(&file_path)?;
        file.write_all(profile_name.as_bytes())?;
        log::debug!(
            "CoreProfileManager: Saved last profile '{profile_name}' for project at {:?}.",
            project_root
        );
        Ok(())
    }

    fn load_last_profile_name_for_project(&self, project_root: &Path) -> Result<Option<String>> {
        let config_dir = CoreProfileManager::ensure_project_config_dir(project_root)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let file_path = config_dir.join(LAST_PROFILE_FILENAME);

        if !file_path.exists() {
            log::trace!(
                "CoreProfileManager: No last profile file for project at {:?}.",
                project_root
            );
            return Ok(None);
        }

        let mut file = File::open(&file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let trimmed = contents.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use tempfile::TempDir;

    const APP_NAME_FOR_TESTS: &str = "SourcePackerTests";

    #[test]
    fn test_core_profile_manager_get_profile_dir_path_creates_if_not_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();

        let dir_opt = manager.get_profile_dir_path(project_root, APP_NAME_FOR_TESTS);

        assert!(dir_opt.is_some(), "Profile directory should be determined");
        let dir_path = dir_opt.unwrap();
        assert!(dir_path.exists(), "Profile directory should be created");
        assert!(dir_path.is_dir(), "Returned path must be a directory");
        assert_eq!(
            dir_path.file_name().unwrap_or_default(),
            PROFILES_SUBFOLDER_NAME
        );

        let parent = dir_path.parent().expect("profiles dir should have parent");
        assert_eq!(
            parent.file_name().unwrap_or_default(),
            PROJECT_CONFIG_DIR_NAME
        );
    }

    #[test]
    fn test_save_and_load_profile_project_local() -> Result<()> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();

        let profile_name = "My Mocked Profile".to_string();
        let root = PathBuf::from("/mock/project");
        let mut selected = HashSet::new();
        selected.insert(root.join("src/mock_main.rs"));

        let original_profile = Profile {
            name: profile_name.clone(),
            root_folder: root.clone(),
            selected_paths: selected.clone(),
            deselected_paths: HashSet::new(),
            archive_path: Some(PathBuf::from("/mock/archive.txt")),
            file_details: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        manager.save_profile(project_root, &original_profile, APP_NAME_FOR_TESTS)?;

        let loaded_profile =
            manager.load_profile(project_root, &profile_name, APP_NAME_FOR_TESTS)?;

        assert_eq!(loaded_profile.name, original_profile.name);
        assert_eq!(loaded_profile.root_folder, original_profile.root_folder);
        assert_eq!(
            loaded_profile.selected_paths,
            original_profile.selected_paths
        );
        assert_eq!(loaded_profile.archive_path, original_profile.archive_path);
        Ok(())
    }

    #[test]
    fn test_load_profile_from_path_project_local() -> Result<()> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();

        let profile_name = "DirectLoadProfile".to_string();
        let root = PathBuf::from("/direct/load/project");
        let profile_to_save = Profile {
            name: profile_name.clone(),
            root_folder: root.clone(),
            selected_paths: HashSet::new(),
            deselected_paths: HashSet::new(),
            archive_path: None,
            file_details: HashMap::new(),
            exclude_patterns: Vec::new(),
        };

        manager.save_profile(project_root, &profile_to_save, APP_NAME_FOR_TESTS)?;

        let sanitized_filename = sanitize_profile_name(&profile_name);
        let direct_path = manager
            .get_profile_dir_path(project_root, APP_NAME_FOR_TESTS)
            .expect("profiles dir should exist")
            .join(format!("{sanitized_filename}.{PROFILE_FILE_EXTENSION}"));

        assert!(direct_path.exists(), "Profile file should exist");

        let loaded_profile = manager.load_profile_from_path(&direct_path)?;

        assert_eq!(loaded_profile.name, profile_name);
        assert_eq!(loaded_profile.root_folder, root);
        Ok(())
    }

    #[test]
    fn test_list_profiles_project_local() -> Result<()> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();

        let initial_listed_names = manager.list_profiles(project_root, APP_NAME_FOR_TESTS)?;
        assert!(initial_listed_names.is_empty());

        let profiles_to_create = vec!["Mock Alpha", "Mock_Beta", "Mock-Gamma"];
        for name_str in &profiles_to_create {
            let p = Profile::new(name_str.to_string(), PathBuf::from("/tmp_mock"));
            manager.save_profile(project_root, &p, APP_NAME_FOR_TESTS)?;
        }

        let mut listed_names = manager.list_profiles(project_root, APP_NAME_FOR_TESTS)?;

        let mut expected_sanitized_names: Vec<String> = profiles_to_create
            .iter()
            .map(|s| sanitize_profile_name(s))
            .collect();

        listed_names.sort_unstable();
        expected_sanitized_names.sort_unstable();
        assert_eq!(listed_names, expected_sanitized_names);
        Ok(())
    }

    #[test]
    fn test_load_non_existent_profile_project_local() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();
        let result = manager.load_profile(
            project_root,
            "This Profile Does Not Exist",
            APP_NAME_FOR_TESTS,
        );
        assert!(matches!(result, Err(ProfileError::ProfileNotFound(_))));
    }

    #[test]
    fn test_invalid_profile_names_save_project_local() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();
        let p_empty = Profile::new("".to_string(), PathBuf::from("/tmp_mock"));
        let p_invalid_char = Profile::new("My/MockProfile".to_string(), PathBuf::from("/tmp_mock"));

        assert!(matches!(
            manager.save_profile(project_root, &p_empty, APP_NAME_FOR_TESTS),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            manager.save_profile(project_root, &p_invalid_char, APP_NAME_FOR_TESTS),
            Err(ProfileError::InvalidProfileName(_))
        ));
    }

    #[test]
    fn test_invalid_profile_names_load_project_local() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();
        assert!(matches!(
            manager.load_profile(project_root, "", APP_NAME_FOR_TESTS),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            manager.load_profile(project_root, "My/MockProfile", APP_NAME_FOR_TESTS),
            Err(ProfileError::InvalidProfileName(_))
        ));
    }

    #[test]
    fn test_save_and_load_last_profile_name_for_project() -> Result<()> {
        // Arrange
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();
        let profile_name = "RecentProfile";

        // Act
        manager.save_last_profile_name_for_project(project_root, profile_name)?;

        // Assert
        let loaded = manager.load_last_profile_name_for_project(project_root)?;
        assert_eq!(loaded, Some(profile_name.to_string()));
        Ok(())
    }

    #[test]
    fn test_load_last_profile_name_for_project_none_when_missing_or_empty() -> Result<()> {
        // Arrange
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let project_root = temp_dir.path();
        let manager = CoreProfileManager::new();

        // Act & Assert: missing file yields None
        let loaded_missing = manager.load_last_profile_name_for_project(project_root)?;
        assert!(loaded_missing.is_none());

        // Arrange: create empty file
        let empty_file_path = project_root
            .join(PROJECT_CONFIG_DIR_NAME)
            .join(LAST_PROFILE_FILENAME);
        if let Some(parent) = empty_file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create config dir for empty file test");
        }
        File::create(&empty_file_path).expect("Failed to create empty last profile file");

        // Act & Assert: empty contents treated as None
        let loaded_empty = manager.load_last_profile_name_for_project(project_root)?;
        assert!(loaded_empty.is_none());
        Ok(())
    }

    #[test]
    fn test_sanitize_profile_name_variations() {
        assert_eq!(sanitize_profile_name("My Profile 1"), "MyProfile1");
        assert_eq!(sanitize_profile_name("My_Profile-1"), "My_Profile-1");
        assert_eq!(sanitize_profile_name("!@#$%^&*()"), "");
        assert_eq!(
            sanitize_profile_name("  LeadingTrailingSpaces  "),
            "LeadingTrailingSpaces"
        );
        assert_eq!(sanitize_profile_name("file.with.dots"), "filewithdots");
    }
}
