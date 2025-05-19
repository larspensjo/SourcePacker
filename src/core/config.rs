use directories::ProjectDirs;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

const LAST_PROFILE_FILENAME: &str = "last_profile_name.txt";

/*
 * Manages application configuration, such as the last used profile name.
 * It defines errors related to configuration, provides a concrete implementation
 * (`CoreConfigManager`) for configuration operations, and a trait (`ConfigManagerOperations`)
 * for abstracting these operations, facilitating testability and dependency injection.
 */

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
 * Defines the operations for managing application configuration.
 * This trait abstracts the specific mechanisms for loading and saving configuration data,
 * such as the last used profile. It allows for different implementations (e.g., file-based, mock)
 * to be used, enhancing testability.
 */
pub trait ConfigManagerOperations: Send + Sync {
    /*
     * Loads the name of the last used profile for a given application.
     * Implementations should handle storage retrieval (e.g., from a file) and return
     * the profile name if found, or None if no last profile was saved. Errors during
     * loading should be returned as `ConfigError`.
     */
    fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>>;

    /*
     * Saves the name of the last used profile for a given application.
     * Implementations should handle persisting the profile name (e.g., to a file).
     * Errors during saving should be returned as `ConfigError`.
     */
    fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()>;
}

/*
 * The core implementation of `ConfigManagerOperations`.
 * This struct handles the actual file system interactions for loading and saving
 * application configuration, such as the last used profile name, using standard
 * project directory locations.
 */
pub struct CoreConfigManager {}

impl CoreConfigManager {
    /*
     * Creates a new instance of `CoreConfigManager`.
     * This constructor doesn't require any parameters as the manager typically
     * derives paths dynamically based on the application name passed to its methods.
     */
    pub fn new() -> Self {
        CoreConfigManager {}
    }

    /*
     * Retrieves the application's primary configuration directory.
     * This is a private helper method used by the load and save operations.
     * It ensures the directory exists, creating it if necessary.
     */
    fn _get_app_config_dir(&self, app_name: &str) -> Option<PathBuf> {
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

// --- Existing free functions (to be deprecated/removed later) ---

/*
 * (DEPRECATED - Use CoreConfigManager::save_last_profile_name)
 * Saves the name of the last successfully used profile to a configuration file.
 */
pub fn save_last_profile_name(app_name: &str, profile_name: &str) -> Result<()> {
    let manager = CoreConfigManager::new();
    manager.save_last_profile_name(app_name, profile_name)
}

/*
 * (DEPRECATED - Use CoreConfigManager::load_last_profile_name)
 * Loads the name of the last used profile from its configuration file.
 */
pub fn load_last_profile_name(app_name: &str) -> Result<Option<String>> {
    let manager = CoreConfigManager::new();
    manager.load_last_profile_name(app_name)
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
        // For CoreConfigManager, we can't directly mock ProjectDirs easily in this test,
        // so this test will use the actual ProjectDirs logic but point it to a unique app_name.
        // This is more of an integration test for CoreConfigManager with ProjectDirs.
        // A more isolated unit test would require mocking ProjectDirs itself or using TestConfigManager.
        let unique_app_name = format!("TestApp_{}", rand::random::<u64>());
        let manager = CoreConfigManager::new();
        let profile_name = "MyManagedProfile";

        // Test saving
        assert!(
            manager
                .save_last_profile_name(&unique_app_name, profile_name)
                .is_ok()
        );

        // Test loading
        match manager.load_last_profile_name(&unique_app_name) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {:?}", e),
        }

        // Cleanup the test app's config directory
        if let Some(proj_dirs) = ProjectDirs::from("com", "SourcePackerOrg", &unique_app_name) {
            let config_dir_to_remove = proj_dirs.config_dir();
            if config_dir_to_remove.exists() {
                if let Err(e) = fs::remove_dir_all(config_dir_to_remove) {
                    eprintln!("Test cleanup failed for {:?}: {}", config_dir_to_remove, e);
                }
            }
        }
    }

    #[test]
    fn test_test_config_manager_save_and_load_last_profile_name() {
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path); // Uses the mocked path
        let profile_name = "MyLastProfile";
        let app_name = "AnyApp"; // app_name doesn't affect TestConfigManager's path

        // Test saving
        assert!(
            manager
                .save_last_profile_name(app_name, profile_name)
                .is_ok()
        );

        // Test loading
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

        // Create an empty file
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

        // Save first name
        manager
            .save_last_profile_name(app_name, first_profile_name)
            .unwrap();
        let loaded_name1 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name1, first_profile_name);

        // Save second name (should overwrite)
        manager
            .save_last_profile_name(app_name, second_profile_name)
            .unwrap();
        let loaded_name2 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name2, second_profile_name);
    }
}
