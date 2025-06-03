use directories::ProjectDirs;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

const LAST_PROFILE_FILENAME: &str = "last_profile_name.txt";

/*
 * Manages application-specific configuration settings, such as the name of the
 * last used profile. This module defines how these settings are persisted and
 * retrieved, abstracting the underlying storage mechanism (typically files in a
 * standard user directory).
 *
 * It uses a trait-based approach (`ConfigManagerOperations`) to allow for
 * different storage backends or mock implementations for testing. The primary
 * concrete implementation (`CoreConfigManager`) handles file system interactions.
 */

/*
 * Defines custom error types for application configuration management.
 * This enum centralizes error handling for reading or writing configuration files,
 * such as the last used profile name, providing specific error contexts.
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
 * Defines the operations for managing application configuration.
 * This trait abstracts the specific mechanisms for loading and saving configuration data,
 * such as the last used profile. It allows for different implementations (e.g., file-based, mock)
 * to be used, enhancing testability.
 */
pub trait ConfigManagerOperations: Send + Sync {
    /*
     * Loads the name of the last used profile for a given application.
     * Implementations should handle storage retrieval (e.g., from a file in the
     * application's local configuration directory) and return the profile name
     * if found, or `None` if no last profile was saved.
     */
    fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>>;

    /*
     * Saves the name of the last used profile for a given application.
     * Implementations should handle persisting the profile name (e.g., to a file
     * in the application's local configuration directory).
     */
    fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()>;
}

/*
 * The core implementation of `ConfigManagerOperations`.
 * This struct handles the actual file system interactions for loading and saving
 * application configuration, such as the last used profile name, using standard
 * local (non-roaming) project directory locations without an organization sub-folder.
 * TOOD: We should move the profile name here.
 */
pub struct CoreConfigManager {}

impl CoreConfigManager {
    /*
     * Creates a new instance of `CoreConfigManager`.
     */
    pub fn new() -> Self {
        CoreConfigManager {}
    }

    /*
     * Retrieves the application's primary local configuration directory.
     * This is a private helper method used by the load and save operations.
     * It ensures the directory exists, creating it if necessary. The path
     * will be under the user's local application data directory (e.g., AppData/Local).
     */
    fn _get_app_config_dir(&self, app_name: &str) -> Option<PathBuf> {
        ProjectDirs::from("", "", app_name).and_then(|proj_dirs| {
            let config_path = proj_dirs.config_local_dir(); // Changed to config_local_dir
            if !config_path.exists() {
                if let Err(e) = fs::create_dir_all(config_path) {
                    log::error!(
                        "Failed to create app config directory {:?}: {}",
                        config_path, e
                    );
                    return None;
                }
            }
            Some(config_path.to_path_buf())
        })
    }
}

impl Default for CoreConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigManagerOperations for CoreConfigManager {
    fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>> {
        let config_dir = self
            ._get_app_config_dir(app_name)
            .ok_or(ConfigError::NoProjectDirectory)?;
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

    fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()> {
        let config_dir = self
            ._get_app_config_dir(app_name)
            .ok_or(ConfigError::NoProjectDirectory)?;
        let file_path = config_dir.join(LAST_PROFILE_FILENAME);

        let mut file = File::create(file_path)?;
        file.write_all(profile_name.as_bytes())?;
        Ok(())
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

    // Test helper for CoreConfigManager that allows specifying the config directory
    struct TestConfigManager {
        mock_config_dir: PathBuf,
    }

    impl TestConfigManager {
        fn new(mock_config_dir: PathBuf) -> Self {
            TestConfigManager { mock_config_dir }
        }

        // Shadow the private _get_app_config_dir for testing purposes
        fn _get_app_config_dir_test(&self, _app_name: &str) -> Option<PathBuf> {
            Some(self.mock_config_dir.clone())
        }
    }

    // Implement the trait for TestConfigManager using its own _get_app_config_dir_test
    impl ConfigManagerOperations for TestConfigManager {
        fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>> {
            let config_dir = self
                ._get_app_config_dir_test(app_name) // Use the test version
                .ok_or(ConfigError::NoProjectDirectory)?;
            let file_path = config_dir.join(LAST_PROFILE_FILENAME);

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

        fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()> {
            let config_dir = self
                ._get_app_config_dir_test(app_name) // Use the test version
                .ok_or(ConfigError::NoProjectDirectory)?;
            let file_path = config_dir.join(LAST_PROFILE_FILENAME);

            let mut file = File::create(file_path)?;
            file.write_all(profile_name.as_bytes())?;
            Ok(())
        }
    }

    #[test]
    fn test_core_config_manager_save_and_load() {
        let dir = tempdir().unwrap();
        let unique_app_name = format!("TestApp_{}", rand::random::<u64>());
        let manager = CoreConfigManager::new();
        let profile_name = "MyManagedProfile";

        assert!(
            manager
                .save_last_profile_name(&unique_app_name, profile_name)
                .is_ok()
        );

        match manager.load_last_profile_name(&unique_app_name) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {:?}", e),
        }

        // Verify path structure (basic check)
        if let Some(proj_dirs) = ProjectDirs::from("", "", &unique_app_name) {
            let config_local_dir = proj_dirs.config_local_dir();
            assert!(config_local_dir.join(LAST_PROFILE_FILENAME).exists());
            assert!(
                config_local_dir
                    .to_string_lossy()
                    .contains(&unique_app_name)
            );
            assert!(
                !config_local_dir
                    .to_string_lossy()
                    .contains("SourcePackerOrg")
            );
            assert!(config_local_dir.to_string_lossy().contains("Local")); // Check for AppData/Local part

            // Cleanup the test app's config directory
            if config_local_dir.exists() {
                if let Err(e) = fs::remove_dir_all(config_local_dir) {
                    log::error!(
                        "Test cleanup failed for {:?}: {}",
                        proj_dirs.config_local_dir(),
                        e
                    );
                }
            }
        } else {
            panic!("Could not get ProjectDirs for cleanup and path verification.");
        }
    }

    #[test]
    fn test_test_config_manager_save_and_load_last_profile_name() {
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let profile_name = "MyLastProfile";
        let app_name = "AnyApp";

        assert!(
            manager
                .save_last_profile_name(app_name, profile_name)
                .is_ok()
        );

        match manager.load_last_profile_name(app_name) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {:?}", e),
        }
    }

    #[test]
    fn test_test_config_manager_load_last_profile_name_not_exists() {
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let app_name = "AnyApp";

        match manager.load_last_profile_name(app_name) {
            Ok(None) => {} // Expected outcome
            Ok(Some(_)) => panic!("Expected None when file doesn't exist, but got a name."),
            Err(e) => panic!("Unexpected error when file doesn't exist: {:?}", e),
        }
    }

    #[test]
    fn test_test_config_manager_load_last_profile_name_empty_file() {
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path.clone());
        let app_name = "AnyApp";

        let file_path = mock_dir_path.join(LAST_PROFILE_FILENAME);
        File::create(&file_path).unwrap();

        match manager.load_last_profile_name(app_name) {
            Ok(None) => {} // Expected outcome for an empty file (after trim)
            Ok(Some(name)) => panic!("Expected None for empty file, but got: {}", name),
            Err(e) => panic!("Unexpected error for empty file: {:?}", e),
        }
    }

    #[test]
    fn test_test_config_manager_save_last_profile_name_overwrites() {
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let app_name = "AnyApp";
        let first_profile_name = "OldProfile";
        let second_profile_name = "NewProfile";

        manager
            .save_last_profile_name(app_name, first_profile_name)
            .unwrap();
        let loaded_name1 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name1, first_profile_name);

        manager
            .save_last_profile_name(app_name, second_profile_name)
            .unwrap();
        let loaded_name2 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name2, second_profile_name);
    }
}
