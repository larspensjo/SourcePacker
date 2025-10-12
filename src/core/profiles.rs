/*
 * This module is responsible for managing user profiles. Profiles store user-specific
 * configurations, such as the root folder to monitor, selection states of files,
 * and associated archive paths. It provides mechanisms to save, load, and list
 * these profiles, abstracting the underlying storage (typically JSON files in a
 * local, non-roaming application-specific directory).
 *
 * It includes a trait for profile operations (`ProfileManagerOperations`) to facilitate
 * testing and dependency injection, and a concrete implementation (`CoreProfileManager`).
 * Profile storage now leverages a shared path utility for determining the base
 * configuration directory, under which a "profiles" subfolder is used.
 */
use super::file_node::Profile;
use crate::core::path_utils; // Import the new path_utils module
use serde_json;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub const PROFILE_FILE_EXTENSION: &str = "json";
const PROFILES_SUBFOLDER_NAME: &str = "profiles";

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
            ProfileError::Io(e) => write!(f, "I/O error: {}", e),
            ProfileError::Serde(e) => write!(f, "Serialization/Deserialization error: {}", e),
            ProfileError::NoProjectDirectory => {
                write!(f, "Could not determine project directory for profiles")
            }
            ProfileError::ProfileNotFound(name) => write!(f, "Profile not found: {}", name),
            ProfileError::InvalidProfileName(name) => write!(
                f,
                "Invalid profile name: {}. Contains invalid characters or is empty.",
                name
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
    fn load_profile(&self, profile_name: &str, app_name: &str) -> Result<Profile>;
    fn load_profile_from_path(&self, path: &Path) -> Result<Profile>;
    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<()>;
    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>>;
    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf>;
}

pub struct CoreProfileManager {}

impl CoreProfileManager {
    pub fn new() -> Self {
        CoreProfileManager {}
    }

    /*
     * Retrieves the storage directory for profiles of a given application.
     * This helper method uses `path_utils::get_base_app_config_local_dir` to get the
     * base application configuration directory, then appends a "profiles" subfolder.
     * It ensures this "profiles" subfolder exists, creating it if necessary.
     */
    fn get_profile_storage_dir_impl(app_name: &str) -> Option<PathBuf> {
        path_utils::get_base_app_config_local_dir(app_name).and_then(|base_dir| {
            let profiles_path = base_dir.join(PROFILES_SUBFOLDER_NAME);
            if !profiles_path.exists() {
                if let Err(e) = fs::create_dir_all(&profiles_path) {
                    log::error!(
                        "CoreProfileManager: Failed to create profile storage directory {:?}: {}",
                        profiles_path,
                        e
                    );
                    return None;
                }
                log::debug!(
                    "CoreProfileManager: Created profile storage directory: {:?}",
                    profiles_path
                );
            } else {
                log::trace!(
                    "CoreProfileManager: Profile storage directory already exists: {:?}",
                    profiles_path
                );
            }
            Some(profiles_path)
        })
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
    fn load_profile(&self, profile_name: &str, app_name: &str) -> Result<Profile> {
        log::trace!(
            "CoreProfileManager: Loading profile '{}' for app '{}'",
            profile_name,
            app_name
        );
        if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
        }

        let dir = CoreProfileManager::get_profile_storage_dir_impl(app_name)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(profile_name);
        let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

        if !file_path.exists() {
            log::debug!(
                "CoreProfileManager: Profile file {:?} not found for profile '{}'.",
                file_path,
                profile_name
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
        log::trace!("CoreProfileManager: Loading profile from path {:?}", path);
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
    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<()> {
        log::trace!(
            "CoreProfileManager: Saving profile '{}' for app '{}'",
            profile.name,
            app_name
        );
        if profile.name.trim().is_empty() || !profile.name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile.name.clone()));
        }

        let dir = CoreProfileManager::get_profile_storage_dir_impl(app_name)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(&profile.name);
        let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

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
    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>> {
        log::trace!(
            "CoreProfileManager: Listing profiles for app '{}'",
            app_name
        );
        let dir = match CoreProfileManager::get_profile_storage_dir_impl(app_name) {
            Some(d) => d,
            None => {
                log::debug!(
                    "CoreProfileManager: Profile storage directory not found for app '{}', returning empty list.",
                    app_name
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
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == PROFILE_FILE_EXTENSION {
                            if let Some(stem) = path.file_stem() {
                                profile_names.push(stem.to_string_lossy().into_owned());
                            }
                        }
                    }
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

    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf> {
        CoreProfileManager::get_profile_storage_dir_impl(app_name)
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use crate::core::path_utils;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    struct TestProfileManager {
        // This mock_profile_dir represents <BASE_APP_CONFIG_DIR>/profiles/
        mock_profile_dir: PathBuf,
    }

    impl TestProfileManager {
        fn new(temp_dir_for_base_app_config: &TempDir) -> Self {
            // Simulate the behavior of get_profile_storage_dir_impl
            // TestProfileManager takes a path that would be the *base* app config dir.
            let mock_base_app_config_dir = temp_dir_for_base_app_config.path().to_path_buf();
            let mock_profile_dir = mock_base_app_config_dir.join(PROFILES_SUBFOLDER_NAME);

            fs::create_dir_all(&mock_profile_dir)
                .expect("Failed to create mock profile dir for test");
            TestProfileManager { mock_profile_dir }
        }
    }

    impl ProfileManagerOperations for TestProfileManager {
        // load_profile, save_profile, list_profiles, get_profile_dir_path
        // remain identical to your previous version, as they operate on self.mock_profile_dir
        // which is now understood to be the "profiles" subfolder.
        fn load_profile(&self, profile_name: &str, _app_name: &str) -> Result<Profile> {
            if profile_name.trim().is_empty()
                || !profile_name.chars().all(is_valid_profile_name_char)
            {
                return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
            }
            let sanitized_filename = sanitize_profile_name(profile_name);
            let file_path = self
                .mock_profile_dir
                .join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));
            if !file_path.exists() {
                return Err(ProfileError::ProfileNotFound(profile_name.to_string()));
            }
            let file = File::open(&file_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).map_err(ProfileError::from)
        }

        fn load_profile_from_path(&self, path: &Path) -> Result<Profile> {
            let file = File::open(path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).map_err(ProfileError::from)
        }

        fn save_profile(&self, profile: &Profile, _app_name: &str) -> Result<()> {
            if profile.name.trim().is_empty()
                || !profile.name.chars().all(is_valid_profile_name_char)
            {
                return Err(ProfileError::InvalidProfileName(profile.name.clone()));
            }
            let sanitized_filename = sanitize_profile_name(&profile.name);
            let file_path = self
                .mock_profile_dir
                .join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));
            let file = File::create(file_path)?;
            let writer = BufWriter::new(file);
            serde_json::to_writer_pretty(writer, profile).map_err(ProfileError::from)
        }

        fn list_profiles(&self, _app_name: &str) -> Result<Vec<String>> {
            let mut profile_names = Vec::new();
            if self.mock_profile_dir.exists() {
                for entry_result in fs::read_dir(&self.mock_profile_dir)? {
                    let entry = entry_result?;
                    let path = entry.path();
                    if path.is_file()
                        && path
                            .extension()
                            .is_some_and(|ext| ext == PROFILE_FILE_EXTENSION)
                    {
                        if let Some(stem) = path.file_stem() {
                            profile_names.push(stem.to_string_lossy().into_owned());
                        }
                    }
                }
            }
            profile_names.sort_unstable();
            Ok(profile_names)
        }

        fn get_profile_dir_path(&self, _app_name: &str) -> Option<PathBuf> {
            Some(self.mock_profile_dir.clone())
        }
    }

    #[test]
    fn test_core_profile_manager_get_profile_dir_path_creates_if_not_exists() {
        // Arrange
        let temp_app_name = format!("SourcePackerTest_ProfileDir_{}", rand::random::<u32>());
        let manager = CoreProfileManager::new();

        // Ensure the directories do not exist before the call
        if let Some(base_dir) = path_utils::get_base_app_config_local_dir(&temp_app_name) {
            let profile_dir_to_check = base_dir.join(PROFILES_SUBFOLDER_NAME);
            if profile_dir_to_check.exists() {
                fs::remove_dir_all(&profile_dir_to_check)
                    .expect("Pre-test cleanup failed for profile dir");
            }
            // Also remove base if it's empty now, or path_utils will just return it
            if fs::read_dir(&base_dir).is_ok_and(|mut i| i.next().is_none()) {
                fs::remove_dir(&base_dir).expect("Pre-test cleanup failed for base dir");
            }
        }

        // Act
        let dir_opt = manager.get_profile_dir_path(&temp_app_name);

        // Assert
        assert!(dir_opt.is_some(), "Profile directory should be determined");
        let dir_path = dir_opt.unwrap();
        assert!(
            dir_path.exists(),
            "Profile directory should be created: {:?}",
            dir_path
        );
        assert!(dir_path.is_dir(), "{:?} should be a directory", dir_path);
        assert_eq!(
            dir_path.file_name().unwrap_or_default(),
            PROFILES_SUBFOLDER_NAME,
            "Path should end with '{}' segment. Path was: {:?}",
            PROFILES_SUBFOLDER_NAME,
            dir_path
        );

        if let Some(parent_of_profiles) = dir_path.parent() {
            let parent_of_profiles_str = parent_of_profiles.to_string_lossy();
            assert!(
                parent_of_profiles_str
                    .to_lowercase()
                    .contains(&temp_app_name.to_lowercase()),
                "The parent directory of '{}' ({:?}) should contain the app name '{}'",
                PROFILES_SUBFOLDER_NAME,
                parent_of_profiles,
                temp_app_name
            );
        } else {
            panic!(
                "'{}' directory ({:?}) should have a parent.",
                PROFILES_SUBFOLDER_NAME, dir_path
            );
        }

        // Cleanup
        if let Some(app_base_config_local_dir) =
            path_utils::get_base_app_config_local_dir(&temp_app_name)
        {
            if app_base_config_local_dir.exists() {
                if let Err(e) = fs::remove_dir_all(&app_base_config_local_dir) {
                    eprintln!(
                        "Test cleanup failed for app_base_config_local_dir {:?}: {}",
                        app_base_config_local_dir, e
                    );
                }
            }
        }
    }

    #[test]
    fn test_save_and_load_profile_with_test_manager() -> Result<()> {
        // Arrange
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj); // TestProfileManager now expects the base temp dir

        let app_name_for_test = "TestAppWithMockDir"; // app_name is not strictly used by TestProfileManager
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

        // Act: Save Profile
        manager.save_profile(&original_profile, app_name_for_test)?;

        // Act: Load Profile
        let loaded_profile = manager.load_profile(&profile_name, app_name_for_test)?;

        // Assert
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
    fn test_load_profile_from_path_with_test_manager() -> Result<()> {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);
        let app_name_for_test = "TestAppLoadFromPath";

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

        manager.save_profile(&profile_to_save, app_name_for_test)?;

        let sanitized_filename = sanitize_profile_name(&profile_name);
        let direct_path = manager
            .mock_profile_dir // This IS <temp_base>/profiles/
            .join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

        assert!(
            direct_path.exists(),
            "Profile file should exist at: {:?}",
            direct_path
        );

        let loaded_profile = manager.load_profile_from_path(&direct_path)?;

        assert_eq!(loaded_profile.name, profile_name);
        assert_eq!(loaded_profile.root_folder, root);
        Ok(())
    }

    #[test]
    fn test_list_profiles_with_test_manager() -> Result<()> {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);
        let app_name_for_test = "TestAppListMockDir";

        let initial_listed_names = manager.list_profiles(app_name_for_test)?;
        assert!(initial_listed_names.is_empty());

        let profiles_to_create = vec!["Mock Alpha", "Mock_Beta", "Mock-Gamma"];
        for name_str in &profiles_to_create {
            let p = Profile::new(name_str.to_string(), PathBuf::from("/tmp_mock"));
            manager.save_profile(&p, app_name_for_test)?;
        }

        let mut listed_names = manager.list_profiles(app_name_for_test)?;

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
    fn test_load_non_existent_profile_with_test_manager() {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);
        let app_name_for_test = "TestAppLoadNonExistentMockDir";
        let result = manager.load_profile("This Profile Does Not Exist In Mock", app_name_for_test);
        assert!(matches!(result, Err(ProfileError::ProfileNotFound(_))));
    }

    #[test]
    fn test_invalid_profile_names_save_with_test_manager() {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);
        let app_name_for_test = "TestAppInvalidSaveMockDir";
        let p_empty = Profile::new("".to_string(), PathBuf::from("/tmp_mock"));
        let p_invalid_char = Profile::new("My/MockProfile".to_string(), PathBuf::from("/tmp_mock"));

        assert!(matches!(
            manager.save_profile(&p_empty, app_name_for_test),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            manager.save_profile(&p_invalid_char, app_name_for_test),
            Err(ProfileError::InvalidProfileName(_))
        ));
    }

    #[test]
    fn test_invalid_profile_names_load_with_test_manager() {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);
        let app_name_for_test = "TestAppInvalidLoadMockDir";
        assert!(matches!(
            manager.load_profile("", app_name_for_test),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            manager.load_profile("My/MockProfile", app_name_for_test),
            Err(ProfileError::InvalidProfileName(_))
        ));
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
