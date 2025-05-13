use super::models::Profile; // Using Profile from the parent 'core' module's re-export
use directories::ProjectDirs;
use serde_json;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub const PROFILE_FILE_EXTENSION: &str = "json";

// Define a custom error type for this module
#[derive(Debug)]
pub enum ProfileError {
    Io(io::Error),
    Serde(serde_json::Error),
    NoProjectDirectory,
    ProfileNotFound(String),
    InvalidProfileName(String),
    // Add more specific errors as needed
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

/// Gets the directory where profiles are stored for the given application name.
/// Typically %APPDATA%\<app_name>\profiles or similar.
/// Returns None if the directory cannot be determined or created.
pub fn get_profile_dir(app_name: &str) -> Option<PathBuf> {
    ProjectDirs::from("com", "SourcePackerOrg", app_name) // Adjust qualifier and organization as needed
        .map(|proj_dirs| {
            let profiles_path = proj_dirs.config_dir().join("profiles");
            if !profiles_path.exists() {
                if let Err(e) = fs::create_dir_all(&profiles_path) {
                    eprintln!(
                        "Failed to create profile directory {:?}: {}",
                        profiles_path, e
                    );
                    return None; // Return None if directory creation fails
                }
            }
            Some(profiles_path)
        })
        .flatten() // If map returns None, flatten keeps it None
}

pub fn sanitize_profile_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn is_valid_profile_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' // Allow spaces for now, will replace with _ for filename
}

/// Saves a profile to a JSON file.
/// The profile name from `profile.name` is used for the filename, after sanitization.
fn save_profile_with_dir_provider<F>(
    profile: &Profile,
    app_name: &str,
    dir_provider: F,
) -> Result<()>
where
    F: FnOnce(&str) -> Option<PathBuf>,
{
    if profile.name.trim().is_empty() || !profile.name.chars().all(is_valid_profile_name_char) {
        print!("Invalid profile name: {}", profile.name);
        return Err(ProfileError::InvalidProfileName(profile.name.clone()));
    }

    let dir = dir_provider(app_name).ok_or(ProfileError::NoProjectDirectory)?;
    let sanitized_filename = sanitize_profile_name(&profile.name);
    let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

    let file = File::create(&file_path)?;
    let writer = BufWriter::new(file);
    println!(
        "save_profile_with_dir_provider: Saving profile to {:?}...",
        file_path
    );
    serde_json::to_writer_pretty(writer, profile)?;
    Ok(())
}

/// Loads a profile from a JSON file.
/// `profile_name` should be the user-facing name, which will be sanitized to find the filename.
fn load_profile_with_dir_provider<F>(
    profile_name: &str,
    app_name: &str,
    dir_provider: F,
) -> Result<Profile>
where
    F: FnOnce(&str) -> Option<PathBuf>,
{
    if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
        return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
    }

    let dir = dir_provider(app_name).ok_or(ProfileError::NoProjectDirectory)?;
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

/// Lists the names of all available profiles.
/// Names are derived from the filenames in the profile directory.
fn list_profiles_with_dir_provider<F>(app_name: &str, dir_provider: F) -> Result<Vec<String>>
where
    F: FnOnce(&str) -> Option<PathBuf>,
{
    let dir = match dir_provider(app_name) {
        Some(d) => d,
        None => return Ok(Vec::new()), // No directory, so no profiles
    };

    println!(
        "list_profiles_with_dir_provider: Looking for profiles in {:?}",
        dir
    );

    let mut profile_names = Vec::new();
    if dir.exists() {
        // Ensure directory exists before trying to read it
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
    println!(
        "list_profiles_with_dir_provider: Found profiles: {:?}",
        profile_names
    );
    Ok(profile_names)
}

// --- Public API wrappers that use the real get_profile_dir ---

/// Saves a profile to a JSON file.
/// The profile name from `profile.name` is used for the filename, after sanitization.
pub fn save_profile(profile: &Profile, app_name: &str) -> Result<()> {
    save_profile_with_dir_provider(profile, app_name, get_profile_dir)
}

/// Loads a profile from a JSON file.
/// `profile_name` should be the user-facing name, which will be sanitized to find the filename.
pub fn load_profile(profile_name: &str, app_name: &str) -> Result<Profile> {
    load_profile_with_dir_provider(profile_name, app_name, get_profile_dir)
}

/// Lists the names of all available profiles.
/// Names are derived from the filenames in the profile directory.
pub fn list_profiles(app_name: &str) -> Result<Vec<String>> {
    list_profiles_with_dir_provider(app_name, get_profile_dir)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs; // Removed env, path::PathBuf as they are not used directly in test_save_and_load_profile_roundtrip
    use tempfile::TempDir;

    #[test]
    fn test_get_profile_dir_creates_if_not_exists() {
        let temp_app_name = format!("SourcePackerTest_DirCreate_{}", rand::random::<u32>());
        let dir_opt = get_profile_dir(&temp_app_name);
        assert!(dir_opt.is_some(), "Profile directory should be determined");

        let dir_path = dir_opt.unwrap(); // This is .../temp_app_name/config/profiles
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

        if let Some(parent_of_profiles) = dir_path.parent() {
            // This is .../temp_app_name/config
            let parent_of_profiles_name_osstr = parent_of_profiles.file_name().unwrap_or_default();
            let parent_of_profiles_name_str = parent_of_profiles_name_osstr
                .to_str()
                .expect("Parent of profiles filename should be UTF-8");
            assert_eq!(
                parent_of_profiles_name_str, "config",
                "Parent of 'profiles' dir should be 'config'. Parent was: {:?}",
                parent_of_profiles
            );

            if let Some(parent_of_config) = parent_of_profiles.parent() {
                // This is .../temp_app_name
                let parent_of_config_name_osstr = parent_of_config.file_name().unwrap_or_default();
                let parent_of_config_name_str = parent_of_config_name_osstr
                    .to_str()
                    .expect("Parent of config filename should be UTF-8");
                assert_eq!(
                    parent_of_config_name_str, temp_app_name,
                    "Parent of 'config' dir should be the app-specific dir named after temp_app_name. Parent was: {:?}",
                    parent_of_config
                );

                let path_str_app_config_dir = parent_of_config.to_string_lossy();
                assert!(
                    path_str_app_config_dir.contains(&temp_app_name),
                    "Path string for app_name_dir should contain temp_app_name. Path was: {}",
                    path_str_app_config_dir
                );

                if let Some(org_level_dir) = parent_of_config.parent() {
                    // This is .../SourcePackerOrg
                    let org_level_name_osstr = org_level_dir.file_name().unwrap_or_default();
                    let org_level_name_str = org_level_name_osstr
                        .to_str()
                        .expect("Org level dir name should be UTF-8");
                    assert_eq!(
                        org_level_name_str, "SourcePackerOrg",
                        "Parent of app_name dir should be Organization. Parent was: {:?}",
                        org_level_dir
                    );
                } else {
                    panic!(
                        "App name directory {:?} has no parent (expected org level), which is unexpected.",
                        parent_of_config
                    );
                }
            } else {
                panic!(
                    "'config' directory {:?} has no parent, which is unexpected.",
                    parent_of_profiles
                );
            }
        } else {
            panic!(
                "Profiles directory {:?} has no parent, which is unexpected.",
                dir_path
            );
        }

        // Clean up logic (remains the same)
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let app_base_dir = proj_dirs.config_dir().parent();
            if let Some(dir_to_remove) = app_base_dir {
                if dir_to_remove.exists()
                    && dir_to_remove.to_string_lossy().contains(&temp_app_name)
                {
                    if let Err(e) = fs::remove_dir_all(dir_to_remove) {
                        eprintln!("Test cleanup failed for {:?}: {}", dir_to_remove, e);
                    }
                }
            } else {
                let config_dir_to_remove = proj_dirs.config_dir();
                if config_dir_to_remove.exists() {
                    if let Err(e) = fs::remove_dir_all(config_dir_to_remove) {
                        eprintln!("Test cleanup failed for {:?}: {}", config_dir_to_remove, e);
                    }
                }
            }
        }
    }

    /// Helper to create a mock directory provider for tests.
    /// It takes a `&TempDir` to ensure the directory exists for the lifetime of the test.
    /// The `TempDir` itself handles cleanup on drop.
    fn mock_dir_provider(temp_dir_path: &Path) -> impl FnOnce(&str) -> Option<PathBuf> + '_ {
        // The mock function returns a clone of the TempDir's path.
        // The real `get_profile_dir` creates the "profiles" subdirectory.
        // For the mock, we'll also ensure this subdirectory exists within the temp dir.
        let mock_profiles_path = temp_dir_path.to_path_buf(); // Using TempDir directly as the "profiles" dir
        if !mock_profiles_path.exists() {
            fs::create_dir_all(&mock_profiles_path)
                .expect("Failed to create mock profiles path for test");
        }

        move |_app_name: &str| -> Option<PathBuf> { Some(mock_profiles_path.clone()) }
    }

    #[test]
    fn test_save_and_load_profile_with_mock_dir() -> Result<()> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let app_name_for_test = "TestAppWithMockDir";
        let profile_name = "My Mocked Profile".to_string();
        let root = PathBuf::from("/mock/project");
        let mut selected = HashSet::new();
        selected.insert(root.join("src/mock_main.rs"));
        let patterns = vec!["*.rs".to_string()];

        let original_profile = Profile {
            name: profile_name.clone(),
            root_folder: root.clone(),
            selected_paths: selected.clone(),
            deselected_paths: HashSet::new(),
            whitelist_patterns: patterns.clone(),
        };

        // Use the _with_dir_provider version with our mock
        save_profile_with_dir_provider(
            &original_profile,
            app_name_for_test,
            mock_dir_provider(temp_dir.path()),
        )?;

        let loaded_profile = load_profile_with_dir_provider(
            &profile_name,
            app_name_for_test,
            mock_dir_provider(temp_dir.path()),
        )?;

        assert_eq!(loaded_profile.name, original_profile.name);
        assert_eq!(loaded_profile.root_folder, original_profile.root_folder);
        assert_eq!(
            loaded_profile.selected_paths,
            original_profile.selected_paths
        );
        assert_eq!(
            loaded_profile.whitelist_patterns,
            original_profile.whitelist_patterns
        );

        // temp_dir will be dropped here, cleaning up automatically.
        Ok(())
    }

    #[test]
    fn test_list_profiles_with_mock_dir() -> Result<()> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let app_name_for_test = "TestAppListMockDir";

        // Ensure profile dir is initially "empty" (by using a fresh temp_dir)
        let initial_listed_names =
            list_profiles_with_dir_provider(app_name_for_test, mock_dir_provider(temp_dir.path()))?;
        assert!(
            initial_listed_names.is_empty(),
            "Initially, no profiles should be listed from mock dir"
        );

        let profiles_to_create = vec!["Mock Alpha", "Mock_Beta", "Mock-Gamma"];
        for name_str in &profiles_to_create {
            let p = Profile::new(name_str.to_string(), PathBuf::from("/tmp_mock"));
            save_profile_with_dir_provider(
                &p,
                app_name_for_test,
                mock_dir_provider(temp_dir.path()),
            )?;
        }

        let mut listed_names =
            list_profiles_with_dir_provider(app_name_for_test, mock_dir_provider(temp_dir.path()))?;

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
    fn test_load_non_existent_profile_with_mock_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let app_name_for_test = "TestAppLoadNonExistentMockDir";
        let result = load_profile_with_dir_provider(
            "This Profile Does Not Exist In Mock",
            app_name_for_test,
            mock_dir_provider(temp_dir.path()),
        );
        assert!(matches!(result, Err(ProfileError::ProfileNotFound(_))));
    }

    #[test]
    fn test_invalid_profile_names_save_with_mock_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let app_name_for_test = "TestAppInvalidSaveMockDir";
        let p_empty = Profile::new("".to_string(), PathBuf::from("/tmp_mock"));
        let p_invalid_char = Profile::new("My/MockProfile".to_string(), PathBuf::from("/tmp_mock"));

        assert!(matches!(
            save_profile_with_dir_provider(
                &p_empty,
                app_name_for_test,
                mock_dir_provider(temp_dir.path())
            ),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            save_profile_with_dir_provider(
                &p_invalid_char,
                app_name_for_test,
                mock_dir_provider(temp_dir.path())
            ),
            Err(ProfileError::InvalidProfileName(_))
        ));
    }

    #[test]
    fn test_invalid_profile_names_load_with_mock_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let app_name_for_test = "TestAppInvalidLoadMockDir";
        assert!(matches!(
            load_profile_with_dir_provider(
                "",
                app_name_for_test,
                mock_dir_provider(temp_dir.path())
            ),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            load_profile_with_dir_provider(
                "My/MockProfile",
                app_name_for_test,
                mock_dir_provider(temp_dir.path())
            ),
            Err(ProfileError::InvalidProfileName(_))
        ));
    }

    // Remove the old `with_temp_config_dir` helper and tests that used it directly.
    // `test_save_and_load_profile_roundtrip`, `test_list_profiles_returns_all_names`,
    // `test_load_nonexistent_profile_errors` are now effectively replaced by their
    // `_with_mock_dir` counterparts above.

    // If you had other tests relying on the old `get_profile_dir` implicitly,
    // you'd refactor them similarly to use `_with_dir_provider` and `mock_dir_provider`.

    // The original `test_save_and_load_profile` and `test_list_profiles` that were not using `with_temp_config_dir`
    // but were still problematic because they used the real profile dir, can be removed if the new
    // `_with_mock_dir` tests cover their intent. Otherwise, refactor them too.
    // Let's assume `test_save_and_load_profile_with_mock_dir` and `test_list_profiles_with_mock_dir`
    // are the correct replacements for `test_save_and_load_profile_roundtrip` and `test_list_profiles_returns_all_names`.
}
