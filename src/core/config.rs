use directories::ProjectDirs;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

const LAST_PROFILE_FILENAME: &str = "last_profile_name.txt";

/*
 * Defines custom error types for application configuration management.
 * This enum centralizes error handling for reading or writing configuration files,
 * such as the last used profile name.
 */
#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    NoProjectDirectory,
    Utf8Error(std::string::FromUtf8Error),
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        ConfigError::Io(err)
    }
}

impl From<std::string::FromUtf8Error> for ConfigError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        ConfigError::Utf8Error(err)
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "Configuration I/O error: {}", e),
            ConfigError::NoProjectDirectory => {
                write!(f, "Could not determine project directory for configuration")
            }
            ConfigError::Utf8Error(e) => write!(f, "Configuration file UTF-8 error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::Utf8Error(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/*
 * Retrieves the application's primary configuration directory.
 * This is typically the parent directory of where profiles are stored.
 * It ensures the directory exists, creating it if necessary.
 */
fn get_app_config_dir(app_name: &str) -> Option<PathBuf> {
    ProjectDirs::from("com", "SourcePackerOrg", app_name).and_then(|proj_dirs| {
        let config_path = proj_dirs.config_dir();
        if !config_path.exists() {
            if let Err(e) = fs::create_dir_all(config_path) {
                eprintln!(
                    "Failed to create app config directory {:?}: {}",
                    config_path, e
                );
                return None;
            }
        }
        Some(config_path.to_path_buf())
    })
}

/*
 * Saves the name of the last successfully used profile to a configuration file.
 * This allows the application to attempt reloading this profile on the next startup.
 * The profile name is stored in a simple text file in the app's config directory.
 */
pub fn save_last_profile_name(app_name: &str, profile_name: &str) -> Result<()> {
    let config_dir = get_app_config_dir(app_name).ok_or(ConfigError::NoProjectDirectory)?;
    let file_path = config_dir.join(LAST_PROFILE_FILENAME);

    let mut file = File::create(file_path)?;
    file.write_all(profile_name.as_bytes())?;
    Ok(())
}

/*
 * Loads the name of the last used profile from its configuration file.
 * If the file doesn't exist or cannot be read, it typically indicates no last profile
 * was saved, or the configuration is corrupted.
 */
pub fn load_last_profile_name(app_name: &str) -> Result<Option<String>> {
    let config_dir = get_app_config_dir(app_name).ok_or(ConfigError::NoProjectDirectory)?;
    let file_path = config_dir.join(LAST_PROFILE_FILENAME);

    if !file_path.exists() {
        return Ok(None); // No last profile saved is not an error
    }

    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    if contents.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(contents.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Helper to mock the config directory for tests
    fn mock_config_dir_provider(temp_dir_path: &PathBuf) -> PathBuf {
        if !temp_dir_path.exists() {
            fs::create_dir_all(temp_dir_path).expect("Failed to create mock config dir for test");
        }
        temp_dir_path.clone()
    }

    // Redefine save_last_profile_name for testing with a mock directory
    fn save_last_profile_name_test(mock_config_dir: &PathBuf, profile_name: &str) -> Result<()> {
        let file_path = mock_config_dir.join(LAST_PROFILE_FILENAME);
        let mut file = File::create(file_path)?;
        file.write_all(profile_name.as_bytes())?;
        Ok(())
    }

    // Redefine load_last_profile_name for testing with a mock directory
    fn load_last_profile_name_test(mock_config_dir: &PathBuf) -> Result<Option<String>> {
        let file_path = mock_config_dir.join(LAST_PROFILE_FILENAME);
        if !file_path.exists() {
            return Ok(None);
        }
        let mut file = File::open(file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        if contents.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(contents.trim().to_string()))
        }
    }

    #[test]
    fn test_save_and_load_last_profile_name() {
        let dir = tempdir().unwrap();
        let mock_dir = mock_config_dir_provider(&dir.path().to_path_buf());
        let profile_name = "MyLastProfile";

        // Test saving
        assert!(save_last_profile_name_test(&mock_dir, profile_name).is_ok());

        // Test loading
        match load_last_profile_name_test(&mock_dir) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {:?}", e),
        }
    }

    #[test]
    fn test_load_last_profile_name_not_exists() {
        let dir = tempdir().unwrap();
        let mock_dir = mock_config_dir_provider(&dir.path().to_path_buf());

        match load_last_profile_name_test(&mock_dir) {
            Ok(None) => {} // Expected outcome
            Ok(Some(_)) => panic!("Expected None when file doesn't exist, but got a name."),
            Err(e) => panic!("Unexpected error when file doesn't exist: {:?}", e),
        }
    }

    #[test]
    fn test_load_last_profile_name_empty_file() {
        let dir = tempdir().unwrap();
        let mock_dir = mock_config_dir_provider(&dir.path().to_path_buf());

        // Create an empty file
        let file_path = mock_dir.join(LAST_PROFILE_FILENAME);
        File::create(&file_path).unwrap();

        match load_last_profile_name_test(&mock_dir) {
            Ok(None) => {} // Expected outcome for an empty file (after trim)
            Ok(Some(name)) => panic!("Expected None for empty file, but got: {}", name),
            Err(e) => panic!("Unexpected error for empty file: {:?}", e),
        }
    }

    #[test]
    fn test_save_last_profile_name_overwrites() {
        let dir = tempdir().unwrap();
        let mock_dir = mock_config_dir_provider(&dir.path().to_path_buf());
        let first_profile_name = "OldProfile";
        let second_profile_name = "NewProfile";

        // Save first name
        save_last_profile_name_test(&mock_dir, first_profile_name).unwrap();
        let loaded_name1 = load_last_profile_name_test(&mock_dir).unwrap().unwrap();
        assert_eq!(loaded_name1, first_profile_name);

        // Save second name (should overwrite)
        save_last_profile_name_test(&mock_dir, second_profile_name).unwrap();
        let loaded_name2 = load_last_profile_name_test(&mock_dir).unwrap().unwrap();
        assert_eq!(loaded_name2, second_profile_name);
    }
}
