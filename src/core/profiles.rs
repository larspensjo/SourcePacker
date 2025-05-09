use super::models::Profile; // Using Profile from the parent 'core' module's re-export
use directories::ProjectDirs;
use serde_json;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

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

fn sanitize_profile_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn is_valid_profile_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' // Allow spaces for now, will replace with _ for filename
}

/// Saves a profile to a JSON file.
/// The profile name from `profile.name` is used for the filename, after sanitization.
pub fn save_profile(profile: &Profile, app_name: &str) -> Result<()> {
    if profile.name.trim().is_empty() || !profile.name.chars().all(is_valid_profile_name_char) {
        return Err(ProfileError::InvalidProfileName(profile.name.clone()));
    }

    let dir = get_profile_dir(app_name).ok_or(ProfileError::NoProjectDirectory)?;
    let sanitized_filename = sanitize_profile_name(&profile.name);
    let file_path = dir.join(format!("{}.{}", sanitized_filename, PROFILE_FILE_EXTENSION));

    let file = File::create(&file_path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, profile)?;
    Ok(())
}

/// Loads a profile from a JSON file.
/// `profile_name` should be the user-facing name, which will be sanitized to find the filename.
pub fn load_profile(profile_name: &str, app_name: &str) -> Result<Profile> {
    if profile_name.trim().is_empty() || !profile_name.chars().all(is_valid_profile_name_char) {
        return Err(ProfileError::InvalidProfileName(profile_name.to_string()));
    }

    let dir = get_profile_dir(app_name).ok_or(ProfileError::NoProjectDirectory)?;
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
pub fn list_profiles(app_name: &str) -> Result<Vec<String>> {
    let dir = match get_profile_dir(app_name) {
        Some(d) => d,
        None => return Ok(Vec::new()), // No directory, so no profiles
    };

    let mut profile_names = Vec::new();
    for entry_result in fs::read_dir(dir)? {
        let entry = entry_result?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == PROFILE_FILE_EXTENSION {
                    if let Some(stem) = path.file_stem() {
                        // For now, assume the stem is the "sanitized" name.
                        // Reconstructing the original name perfectly if it had spaces
                        // and the sanitized version replaced spaces might be tricky.
                        // For simplicity, we return the stem as the profile name.
                        // Consider storing the original name inside the JSON if this becomes an issue.
                        profile_names.push(stem.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    profile_names.sort_unstable(); // For consistent listing
    Ok(profile_names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Helper to ensure a clean profile directory for each test function that needs it.
    // This is tricky because ProjectDirs might point to a real user location.
    // For robust tests, we should mock get_profile_dir or use a known temp location.
    // However, `directories` crate doesn't easily allow overriding for tests.
    // We will test `get_profile_dir` separately and assume it works for other tests,
    // or use a more complex setup with environment variables if needed.
    // For now, let's proceed with the understanding that tests *might* create
    // files in a real (but test-specific) app data location.

    // Inside src/core/profiles.rs, in test_get_profile_dir_creates_if_not_exists

    // In src/core/profiles.rs, test_get_profile_dir_creates_if_not_exists

    // In src/core/profiles.rs, test_get_profile_dir_creates_if_not_exists

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

                    // The qualifier "com" does not appear to form a directory segment on Windows
                    // for config_dir with the current ProjectDirs usage.
                    // So, we remove the check for it here.
                    // If you wanted to verify its presence on other platforms where it might appear,
                    // you'd need more complex cfg attributes.
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

    #[test]
    fn test_save_and_load_profile() -> Result<()> {
        let temp_app_name = format!("SourcePackerTest_SaveLoad_{}", rand::random::<u32>());
        let profile_name = "My Test Profile".to_string(); // Name with space
        let root = PathBuf::from("/test/project");
        let mut selected = HashSet::new();
        selected.insert(root.join("src/main.rs"));
        let patterns = vec!["*.rs".to_string()];

        let original_profile = Profile {
            name: profile_name.clone(),
            root_folder: root.clone(),
            selected_paths: selected.clone(),
            deselected_paths: HashSet::new(),
            whitelist_patterns: patterns.clone(),
        };

        save_profile(&original_profile, &temp_app_name)?;

        // Load by the user-facing name
        let loaded_profile = load_profile(&profile_name, &temp_app_name)?;

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

        // Clean up
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let _ = fs::remove_dir_all(proj_dirs.config_dir());
        }
        Ok(())
    }

    #[test]
    fn test_list_profiles() -> Result<()> {
        let temp_app_name = format!("SourcePackerTest_List_{}", rand::random::<u32>());

        // Ensure profile dir exists and is empty initially for this app_name
        if let Some(dir_to_clean) = get_profile_dir(&temp_app_name) {
            if dir_to_clean.exists() {
                for entry in fs::read_dir(&dir_to_clean)? {
                    fs::remove_file(entry?.path())?;
                }
            }
        } else {
            // If get_profile_dir returned None, create it for the test.
            // This is a bit of a workaround due to not mocking get_profile_dir easily.
            let proj_dirs = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name).unwrap();
            fs::create_dir_all(proj_dirs.config_dir().join("profiles"))?;
        }

        let profiles_to_create = vec!["Alpha Profile", "Beta_Profile", "Gamma-Profile"];
        for name_str in &profiles_to_create {
            let p = Profile::new(name_str.to_string(), PathBuf::from("/tmp"));
            save_profile(&p, &temp_app_name)?;
        }

        let mut listed_names = list_profiles(&temp_app_name)?;
        // list_profiles returns sanitized names (stems)
        let mut expected_sanitized_names: Vec<String> = profiles_to_create
            .iter()
            .map(|s| sanitize_profile_name(s))
            .collect();

        listed_names.sort_unstable(); // Ensure order for comparison
        expected_sanitized_names.sort_unstable();

        assert_eq!(listed_names, expected_sanitized_names);

        // Clean up
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let _ = fs::remove_dir_all(proj_dirs.config_dir());
        }
        Ok(())
    }

    #[test]
    fn test_load_non_existent_profile() {
        let temp_app_name = format!("SourcePackerTest_LoadNonExistent_{}", rand::random::<u32>());
        let result = load_profile("This Profile Does Not Exist", &temp_app_name);
        assert!(matches!(result, Err(ProfileError::ProfileNotFound(_))));
        // Clean up
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let _ = fs::remove_dir_all(proj_dirs.config_dir());
        }
    }

    #[test]
    fn test_invalid_profile_names_save() {
        let temp_app_name = format!("SourcePackerTest_InvalidSave_{}", rand::random::<u32>());
        let p_empty = Profile::new("".to_string(), PathBuf::from("/tmp"));
        let p_invalid_char = Profile::new("My/Profile".to_string(), PathBuf::from("/tmp"));

        assert!(matches!(
            save_profile(&p_empty, &temp_app_name),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            save_profile(&p_invalid_char, &temp_app_name),
            Err(ProfileError::InvalidProfileName(_))
        ));
        // Clean up
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let _ = fs::remove_dir_all(proj_dirs.config_dir());
        }
    }

    #[test]
    fn test_invalid_profile_names_load() {
        let temp_app_name = format!("SourcePackerTest_InvalidLoad_{}", rand::random::<u32>());
        assert!(matches!(
            load_profile("", &temp_app_name),
            Err(ProfileError::InvalidProfileName(_))
        ));
        assert!(matches!(
            load_profile("My/Profile", &temp_app_name),
            Err(ProfileError::InvalidProfileName(_))
        ));
        // Clean up
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &temp_app_name) {
            let _ = fs::remove_dir_all(proj_dirs.config_dir());
        }
    }
}
