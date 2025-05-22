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
 * these profiles, abstracting the underlying storage (typically JSON files).
 * It includes a trait for profile operations to facilitate testing and dependency injection.
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

fn is_valid_profile_name_char(c: char) -> bool {
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
     * Implementations handle retrieving the profile from storage (e.g., a JSON file)
     * and deserializing it.
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
     * Implementations handle serializing the profile and persisting it to storage.
     * The profile's name is typically used to derive the filename.
     */
    fn save_profile(&self, profile: &Profile, app_name: &str) -> Result<()>;

    /*
     * Lists the names of all available profiles for a given application.
     * Implementations scan the profile storage location and return a list of
     * discovered profile names (usually derived from filenames).
     */
    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>>;

    /*
     * Retrieves the path to the directory where profiles for the given application are stored.
     * Returns `None` if the directory cannot be determined or created.
     */
    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf>;
}

/*
 * The core implementation of `ProfileManagerOperations`.
 * This struct handles the actual file system interactions for loading, saving,
 * and listing profiles, storing them as JSON files in a standard application
 * configuration directory.
 */
pub struct CoreProfileManager {}

impl CoreProfileManager {
    /*
     * Creates a new instance of `CoreProfileManager`.
     * This constructor doesn't require any parameters as the manager typically
     * derives paths dynamically based on the application name passed to its methods.
     */
    pub fn new() -> Self {
        CoreProfileManager {}
    }

    // Private helper that was the old get_profile_dir
    fn _get_profile_storage_dir(app_name: &str) -> Option<PathBuf> {
        ProjectDirs::from("com", "SourcePackerOrg", app_name)
            .map(|proj_dirs| {
                let profiles_path = proj_dirs.config_dir().join("profiles");
                if !profiles_path.exists() {
                    if let Err(e) = fs::create_dir_all(&profiles_path) {
                        eprintln!(
                            "Failed to create profile directory {:?}: {}",
                            profiles_path, e
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
    /*
     * Loads a profile by its name for a given application from a JSON file.
     * The profile name is sanitized to form the filename. It reads from the
     * standard profile directory.
     */
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

    /*
     * Loads a profile from a specific file path.
     * This method directly attempts to open and deserialize a profile from the given `path`.
     * It's used when a user selects a specific profile file, bypassing the standard
     * profile directory and naming conventions.
     */
    fn load_profile_from_path(&self, path: &Path) -> Result<Profile> {
        let file = File::open(path)?; // Propagates io::Error as ProfileError::Io
        let reader = BufReader::new(file);
        let profile: Profile = serde_json::from_reader(reader)?; // Propagates serde_json::Error as ProfileError::Serde
        Ok(profile)
    }

    /*
     * Saves a profile to a JSON file within the standard profile directory.
     * The profile's name is sanitized to form the filename.
     */
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

    /*
     * Lists the names of all available profiles from the standard profile directory.
     * Profile names are derived from the filenames (excluding the .json extension).
     */
    fn list_profiles(&self, app_name: &str) -> Result<Vec<String>> {
        let dir = match CoreProfileManager::_get_profile_storage_dir(app_name) {
            Some(d) => d,
            None => return Ok(Vec::new()), // If no dir, no profiles
        };

        let mut profile_names = Vec::new();
        if dir.exists() {
            for entry_result in fs::read_dir(dir)? {
                let entry = entry_result?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == PROFILE_FILE_EXTENSION {
                            // Use the file stem as the profile name, which is what users expect
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

    /*
     * Retrieves the path to the directory where profiles for the given application are stored.
     * This uses the same internal logic as load/save operations.
     */
    fn get_profile_dir_path(&self, app_name: &str) -> Option<PathBuf> {
        CoreProfileManager::_get_profile_storage_dir(app_name)
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    // Helper to get a TestProfileManager instance that uses a temporary directory
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
            // For mock, we assume path is valid and directly try to load.
            // More sophisticated mock might check if path is within its mock_profile_dir,
            // or have specific paths pre-configured to return specific profiles or errors.
            // For this test setup, simple pass-through to actual file ops is okay IF
            // the test using this mock ensures the file exists.
            // However, a true mock should not do real file I/O.
            // Let's simulate by checking a predefined map or returning a default/error.
            // For now, to keep it simple for the refactor, and assuming tests will
            // setup any files needed by this mock's passthrough:
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

        let dir_path = dir_opt.unwrap();
        assert!(
            dir_path.exists(),
            "Profile directory should be created: {:?}",
            dir_path
        );
        assert!(dir_path.is_dir(), "{:?} should be a directory", dir_path);
        assert_eq!(
            dir_path.file_name().unwrap_or_default(),
            "profiles", // &OsStr
            "Path should end with 'profiles' segment. Path was: {:?}",
            dir_path
        );
        if let Some(parent_of_profiles) = dir_path.parent() {
            assert_eq!(parent_of_profiles.file_name().unwrap_or_default(), "config"); // &OsStr
            if let Some(parent_of_config) = parent_of_profiles.parent() {
                assert_eq!(
                    parent_of_config
                        .file_name()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap(), // Convert &OsStr to &str
                    temp_app_name // String, comparison with &str is fine
                );
                if let Some(org_level_dir) = parent_of_config.parent() {
                    assert_eq!(
                        org_level_dir.file_name().unwrap_or_default(),
                        "SourcePackerOrg"
                    ); // &OsStr
                } // else panic! - omitted for brevity
            } // else panic!
        } // else panic!

        // Cleanup
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            // Attempt to remove the "SourcePackerOrg" directory, which is the grandparent of the profile dir
            if let Some(app_specific_dir) = proj_dirs.config_dir().parent() {
                // This is <org_name>/<app_name>
                if let Some(org_dir) = app_specific_dir.parent() {
                    // This is <org_name>
                    if org_dir.exists()
                        && org_dir
                            .file_name()
                            .map_or(false, |name| name == "SourcePackerOrg")
                    {
                        let _ = fs::remove_dir_all(org_dir);
                    }
                }
            }
        }
    }

    #[test]
    fn test_save_and_load_profile_with_test_manager() -> Result<()> {
        let temp_dir_obj = TempDir::new().expect("Failed to create temp dir for test");
        let manager = TestProfileManager::new(&temp_dir_obj); // Use TestProfileManager

        let app_name_for_test = "TestAppWithMockDir"; // app_name is illustrative for TestProfileManager
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
        };

        // Save it first using the manager to ensure it's in the mock_profile_dir
        manager.save_profile(&profile_to_save, app_name_for_test)?;

        // Construct the path it would have been saved to
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

        // CORRECTED: list_profiles returns names derived from the sanitized file stems.
        let mut expected_sanitized_names: Vec<String> = profiles_to_create
            .iter()
            .map(|s| sanitize_profile_name(s)) // Expect the sanitized names
            .collect();

        listed_names.sort_unstable();
        expected_sanitized_names.sort_unstable();
        assert_eq!(listed_names, expected_sanitized_names); // This should now pass
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
        ); // Note: original sanitize doesn't trim spaces, but file stem might
        assert_eq!(sanitize_profile_name("file.with.dots"), "filewithdots");
    }
}
