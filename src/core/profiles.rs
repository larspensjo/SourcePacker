use super::models::Profile;
use directories::ProjectDirs;
use serde_json;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

/*
 * This module is responsible for managing user profiles. Profiles store user-specific
 * configurations, such as the root folder to monitor, selection states of files,
 * and associated archive paths. It provides mechanisms to save, load, and list
 * these profiles, abstracting the underlying storage (typically JSON files in a
 * local, non-roaming application-specific directory).
 *
 * It includes a trait for profile operations (`ProfileManagerOperations`) to facilitate
 * testing and dependency injection, and a concrete implementation (`CoreProfileManager`).
 */

pub const PROFILE_FILE_EXTENSION: &str = "json";

/*
 * Defines custom error types for profile management operations.
 * This enum allows for more specific error reporting than generic I/O or Serde errors,
 * aiding in debugging and user feedback for profile-related issues.
 */
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

/*
 * Sanitizes a profile name to make it suitable for use as a filename.
 * This function filters out characters that are typically problematic in filenames,
 * retaining alphanumeric characters, underscores, and hyphens.
 */
pub fn sanitize_profile_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

pub fn is_valid_profile_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == ' '
}

/*
 * Defines the operations for managing user profiles.
 * This trait abstracts the specific mechanisms for loading, saving, listing profiles,
 * and retrieving the profile storage directory. It allows for different implementations
 * (e.g., file-based, mock) to be used, enhancing testability.
 */
pub trait ProfileManagerOperations: Send + Sync {
    /*
     * Loads a profile by its name for a given application.
     * Implementations handle retrieving the profile from storage (e.g., a JSON file
     * in the application's local configuration directory) and deserializing it.
     */
    fn load_profile(&self, profile_name: &str, app_name: &str) -> Result<Profile>;

    /*
     * Loads a profile directly from a specified file system path.
     * Implementations handle opening the file at the given path, reading its content,
     * and deserializing it into a `Profile` object. This is typically used when the
     * user selects a profile file via an open dialog.
     */
    fn load_profile_from_path(&self, path: &Path) -> Result<Profile>;

    /*
     * Saves a given profile for a specific application.
     * Implementations handle serializing the profile and persisting it to storage
     * (e.g., as a JSON file in the application's local configuration directory).
     * The profile's name is typically used to derive the filename.
     */
    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<()>;

    /*
     * Lists the names of all available profiles for a given application.
     * Implementations scan the profile storage location (the application's local
     * configuration directory) and return a list of discovered profile names
     * (usually derived from filenames).
     */
    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>>;

    /*
     * Retrieves the path to the directory where profiles for the given application are stored.
     * This path is typically under the application's local configuration directory.
     * Returns `None` if the directory cannot be determined or created.
     */
    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf>;
}

/*
 * The core implementation of `ProfileManagerOperations`.
 * This struct handles the actual file system interactions for loading, saving,
 * and listing profiles, storing them as JSON files in a standard application
 * local (non-roaming) configuration directory, under a "profiles" subfolder.
 * It does not use an additional organization-level sub-folder.
 */
pub struct CoreProfileManager {}

impl CoreProfileManager {
    /*
     * Creates a new instance of `CoreProfileManager`.
     */
    pub fn new() -> Self {
        CoreProfileManager {}
    }

    /*
     * Retrieves the storage directory for profiles of a given application.
     * This is a private helper method. The path is `%LOCALAPPDATA%/<app_name>/profiles/`.
     * It ensures the directory exists, creating it if necessary.
     */
    fn _get_profile_storage_dir(app_name: &str) -> Option<PathBuf> {
        ProjectDirs::from("", "", app_name) // Use empty qualifier and organization
            .map(|proj_dirs| {
                let profiles_path = proj_dirs.config_local_dir().join("profiles"); // Use config_local_dir
                if !profiles_path.exists() {
                    if let Err(e) = fs::create_dir_all(&profiles_path) {
                        log::error!(
                            "Failed to create profile directory {:?}: {}",
                            profiles_path,
                            e
                        );
                        return None;
                    }
                }
                Some(profiles_path)
            })
            .flatten()
    }
}

impl Default for CoreProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileManagerOperations for CoreProfileManager {
    fn load_profile(&self, profile_name: &str, app_name: &str) -> Result<Profile> {
        if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
        }

        let dir = CoreProfileManager::_get_profile_storage_dir(app_name)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(profile_name);
        let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

        if !file_path.exists() {
            return Err(ProfileError::ProfileNotFound(profile_name.to_string()));
        }

        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);
        let profile: Profile = serde_json::from_reader(reader)?;
        Ok(profile)
    }

    fn load_profile_from_path(&self, path: &Path) -> Result<Profile> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let profile: Profile = serde_json::from_reader(reader)?;
        Ok(profile)
    }

    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<()> {
        if profile.name.trim().is_empty() || !profile.name.chars().all(is_valid_profile_name_char) {
            return Err(ProfileError::InvalidProfileName(profile.name.clone()));
        }

        let dir = CoreProfileManager::_get_profile_storage_dir(app_name)
            .ok_or(ProfileError::NoProjectDirectory)?;
        let sanitized_filename = sanitize_profile_name(&profile.name);
        let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

        let file = File::create(&file_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, profile)?;
        Ok(())
    }

    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>> {
        let dir = match CoreProfileManager::_get_profile_storage_dir(app_name) {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        let mut profile_names = Vec::new();
        if dir.exists() {
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
        Ok(profile_names)
    }

    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf> {
        CoreProfileManager::_get_profile_storage_dir(app_name)
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    struct TestProfileManager {
        mock_profile_dir: PathBuf,
    }

    impl TestProfileManager {
        fn new(temp_dir: &TempDir) -> Self {
            let mock_profile_dir = temp_dir.path().join("profiles");
            fs::create_dir_all(&mock_profile_dir)
                .expect("Failed to create mock profile dir for test");
            TestProfileManager { mock_profile_dir }
        }
    }

    impl ProfileManagerOperations for TestProfileManager {
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
                            .map_or(false, |ext| ext == PROFILE_FILE_EXTENSION)
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
        let temp_app_name = format!("SourcePackerTest_DirCreate_{}", rand::random::<u32>());
        let manager = CoreProfileManager::new();
        let dir_opt = manager.get_profile_dir_path(&temp_app_name);
        assert!(dir_opt.is_some(), "Profile directory should be determined");

        let dir_path = dir_opt.unwrap(); // This is <LOCALAPPDATA>/<app_name>/profiles
        assert!(
            dir_path.exists(),
            "Profile directory should be created: {:?}",
            dir_path
        );
        assert!(dir_path.is_dir(), "{:?} should be a directory", dir_path);
        assert_eq!(
            dir_path.file_name().unwrap_or_default(),
            "profiles",
            "Path should end with 'profiles' segment. Path was: {:?}",
            dir_path
        );

        // The parent of "profiles" should be the directory returned by config_local_dir()
        // which for ProjectDirs::from("", "", app_name) is typically <LOCALAPPDATA>/<app_name>
        // or <LOCALAPPDATA>/<app_name>/config or similar.
        if let Some(parent_of_profiles) = dir_path.parent() {
            // `parent_of_profiles` is the path returned by `config_local_dir()`
            // For ProjectDirs::from("", "", app_name) on Windows, this is typically:
            // C:\Users\<user>\AppData\Local\<app_name>\
            // OR C:\Users\<user>\AppData\Local\<app_name>\config
            // The key is that `temp_app_name` should be a segment in this path.
            let parent_of_profiles_str = parent_of_profiles.to_string_lossy();
            assert!(
                parent_of_profiles_str.contains(&temp_app_name),
                "The parent directory of 'profiles' ({:?}) should contain the app name '{}'",
                parent_of_profiles,
                temp_app_name
            );

            assert!(
                !parent_of_profiles_str.contains("SourcePackerOrg"),
                "Path should not contain 'SourcePackerOrg'. Path was: {:?}",
                parent_of_profiles_str
            );

            // Verify it's generally in AppData/Local (platform dependent check)
            if cfg!(windows) {
                assert!(
                    parent_of_profiles_str
                        .to_lowercase()
                        .contains("appdata\\local"),
                    "Path should be under AppData\\Local on Windows. Path was: {:?}",
                    parent_of_profiles_str
                );
            } else if cfg!(target_os = "macos") {
                // Example for macOS: ~/Library/Application Support/<app_name>
                // or ~/Library/Caches/<app_name> if data_local_dir() was used
                // config_local_dir() on macOS is typically ~/Library/Application Support/<bundle_id or app_name>
                assert!(
                    parent_of_profiles_str.contains("Library/Application Support")
                        || parent_of_profiles_str.contains("Library/Preferences"),
                    "Path on macOS should be under Library/Application Support or Preferences. Path was: {:?}",
                    parent_of_profiles_str
                );
            } else if cfg!(unix) {
                // Example for Linux: ~/.config/<app_name> or ~/.local/share/<app_name>
                assert!(
                    parent_of_profiles_str.contains(".config")
                        || parent_of_profiles_str.contains(".local/share"),
                    "Path on Linux should be under .config or .local/share. Path was: {:?}",
                    parent_of_profiles_str
                );
            }
        } else {
            panic!(
                "'profiles' directory ({:?}) should have a parent.",
                dir_path
            );
        }

        // Cleanup
        if let Some(proj_dirs) = ProjectDirs::from("", "", &temp_app_name) {
            // The directory to remove is the one returned by config_local_dir()
            // because "profiles" is a subdirectory within it.
            let app_base_config_local_dir = proj_dirs.config_local_dir();
            if app_base_config_local_dir.exists() {
                if let Err(e) = fs::remove_dir_all(app_base_config_local_dir) {
                    log::error!(
                        "Test cleanup failed for {:?}: {}",
                        proj_dirs.config_local_dir(),
                        e
                    );
                }
            }
        }
    }

    #[test]
    fn test_save_and_load_profile_with_test_manager() -> Result<()> {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj);

        let app_name_for_test = "TestAppWithMockDir";
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
        };

        manager.save_profile(&original_profile, app_name_for_test)?;
        let loaded_profile = manager.load_profile(&profile_name, app_name_for_test)?;

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
        };

        manager.save_profile(&profile_to_save, app_name_for_test)?;

        let sanitized_filename = sanitize_profile_name(&profile_name);
        let direct_path = manager
            .mock_profile_dir
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
