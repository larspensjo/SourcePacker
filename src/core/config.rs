/*
 * Manages application-specific configuration settings, such as the name of the
 * last used profile. This module defines how these settings are persisted and
 * retrieved, abstracting the underlying storage mechanism (typically files in a
 * standard user directory).
 *
 * It uses a trait-based approach (`ConfigManagerOperations`) to allow for
 * different storage backends or mock implementations for testing. The primary
 * concrete implementation (`CoreConfigManager`) handles file system interactions,
 * now utilizing a shared path utility for determining the base configuration directory.
 */
use crate::core::path_utils; // Import the new path_utils module
use std::fs::File;
use std::io::{self, Read, Write};

const LAST_PROFILE_FILENAME: &str = "last_profile_name.txt";

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
            ConfigError::Io(e) => write!(f, "Configuration I/O error: {e}"),
            ConfigError::NoProjectDirectory => {
                write!(f, "Could not determine project directory for configuration")
            }
            ConfigError::Utf8Error(e) => write!(f, "Configuration file UTF-8 error: {e}"),
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

pub trait ConfigManagerOperations: Send + Sync {
    fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>>;
    fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()>;
}

pub struct CoreConfigManager {}

impl CoreConfigManager {
    pub fn new() -> Self {
        CoreConfigManager {}
    }

    // _get_app_config_dir is now removed, as its logic is replaced by path_utils.
}

impl Default for CoreConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigManagerOperations for CoreConfigManager {
    /*
     * Loads the name of the last used profile for a given application.
     * It uses `path_utils::get_base_app_config_local_dir` to find the application's
     * local configuration directory. It then reads the profile name from
     * `last_profile_name.txt` within that directory.
     */
    fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>> {
        log::trace!("CoreConfigManager: Loading last profile name for app '{app_name}'");
        let config_dir = path_utils::get_base_app_config_local_dir(app_name)
            .ok_or(ConfigError::NoProjectDirectory)?;
        let file_path = config_dir.join(LAST_PROFILE_FILENAME);

        if !file_path.exists() {
            log::debug!("CoreConfigManager: Last profile file {file_path:?} does not exist.");
            return Ok(None);
        }

        let mut file = File::open(&file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        if contents.trim().is_empty() {
            log::debug!("CoreConfigManager: Last profile file {file_path:?} is empty.");
            Ok(None)
        } else {
            let profile_name = contents.trim().to_string();
            log::debug!(
                "CoreConfigManager: Loaded last profile name '{profile_name}' from {file_path:?}."
            );
            Ok(Some(profile_name))
        }
    }

    /*
     * Saves the name of the last used profile for a given application.
     * It uses `path_utils::get_base_app_config_local_dir` to find the application's
     * local configuration directory. It then writes the profile name to
     * `last_profile_name.txt` within that directory.
     */
    fn save_last_profile_name(&self, app_name: &str, profile_name: &str) -> Result<()> {
        log::trace!(
            "CoreConfigManager: Saving last profile name '{profile_name}' for app '{app_name}'"
        );
        let config_dir = path_utils::get_base_app_config_local_dir(app_name)
            .ok_or(ConfigError::NoProjectDirectory)?;
        let file_path = config_dir.join(LAST_PROFILE_FILENAME);

        let mut file = File::create(&file_path)?;
        file.write_all(profile_name.as_bytes())?;
        log::debug!(
            "CoreConfigManager: Saved last profile name '{profile_name}' to {file_path:?}."
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::path_utils; // For setting up mock directories if ProjectDirs was used directly.
    // Now this might not be needed as TestConfigManager won't use ProjectDirs.
    use std::fs; // For direct fs operations in TestConfigManager
    use std::path::PathBuf;
    use tempfile::tempdir;

    // Test helper for CoreConfigManager that allows specifying the config directory
    struct TestConfigManager {
        mock_config_dir: PathBuf,
    }

    impl TestConfigManager {
        fn new(mock_config_dir: PathBuf) -> Self {
            if !mock_config_dir.exists() {
                fs::create_dir_all(&mock_config_dir)
                    .expect("Failed to create mock config dir for test");
            }
            TestConfigManager { mock_config_dir }
        }

        // This internal method simulates what path_utils::get_base_app_config_local_dir would do,
        // but uses the predefined mock_config_dir.
        fn get_mock_app_config_dir(&self, _app_name: &str) -> Option<PathBuf> {
            Some(self.mock_config_dir.clone())
        }
    }

    // Implement the trait for TestConfigManager using its own get_mock_app_config_dir
    impl ConfigManagerOperations for TestConfigManager {
        fn load_last_profile_name(&self, app_name: &str) -> Result<Option<String>> {
            let config_dir = self
                .get_mock_app_config_dir(app_name) // Use the test version
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
                .get_mock_app_config_dir(app_name) // Use the test version
                .ok_or(ConfigError::NoProjectDirectory)?;
            let file_path = config_dir.join(LAST_PROFILE_FILENAME);

            let mut file = File::create(file_path)?;
            file.write_all(profile_name.as_bytes())?;
            Ok(())
        }
    }

    #[test]
    fn test_core_config_manager_save_and_load() {
        // Arrange
        let unique_app_name = format!("TestApp_CoreConfig_{}", rand::random::<u64>());
        let manager = CoreConfigManager::new();
        let profile_name = "MyManagedProfile";

        // Ensure the directory does not exist before save (relying on path_utils to create it)
        if let Some(base_dir) = path_utils::get_base_app_config_local_dir(&unique_app_name) {
            if base_dir.join(LAST_PROFILE_FILENAME).exists() {
                fs::remove_file(base_dir.join(LAST_PROFILE_FILENAME))
                    .expect("Pre-test cleanup: failed to remove last_profile file");
            }
            // We don't remove the base_dir itself here, as path_utils will handle its creation/existence.
            // The main thing is that the specific file for this test isn't present initially.
        }

        // Act & Assert Save
        assert!(
            manager
                .save_last_profile_name(&unique_app_name, profile_name)
                .is_ok(),
            "Saving last profile name should succeed."
        );

        // Act & Assert Load
        match manager.load_last_profile_name(&unique_app_name) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {e:?}"),
        }

        // Verify path structure (basic check using path_utils as source of truth)
        if let Some(config_local_dir) = path_utils::get_base_app_config_local_dir(&unique_app_name)
        {
            assert!(
                config_local_dir.join(LAST_PROFILE_FILENAME).exists(),
                "Last profile file should exist in the directory provided by path_utils."
            );
            assert!(
                config_local_dir
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&unique_app_name.to_lowercase())
            );
            // Cleanup the test app's config directory
            if config_local_dir.exists() {
                if let Err(e) = fs::remove_dir_all(&config_local_dir) {
                    // Pass by reference
                    eprintln!("Test cleanup failed for config_local_dir {config_local_dir:?}: {e}");
                }
            }
        } else {
            panic!(
                "Could not get base app config dir via path_utils for verification and cleanup."
            );
        }
    }

    #[test]
    fn test_test_config_manager_save_and_load_last_profile_name() {
        // Arrange
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let profile_name = "MyLastProfile";
        let app_name = "AnyApp";

        // Act & Assert Save
        assert!(
            manager
                .save_last_profile_name(app_name, profile_name)
                .is_ok()
        );

        // Act & Assert Load
        match manager.load_last_profile_name(app_name) {
            Ok(Some(loaded_name)) => assert_eq!(loaded_name, profile_name),
            Ok(None) => panic!("Expected to load a profile name, but got None."),
            Err(e) => panic!("Failed to load profile name: {e:?}"),
        }
    }

    #[test]
    fn test_test_config_manager_load_last_profile_name_not_exists() {
        // Arrange
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let app_name = "AnyApp";

        // Act & Assert
        match manager.load_last_profile_name(app_name) {
            Ok(None) => {} // Expected outcome
            Ok(Some(_)) => panic!("Expected None when file doesn't exist, but got a name."),
            Err(e) => panic!("Unexpected error when file doesn't exist: {e:?}"),
        }
    }

    #[test]
    fn test_test_config_manager_load_last_profile_name_empty_file() {
        // Arrange
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path.clone());
        let app_name = "AnyApp";

        let file_path = mock_dir_path.join(LAST_PROFILE_FILENAME);
        File::create(&file_path).unwrap(); // Create an empty file

        // Act & Assert
        match manager.load_last_profile_name(app_name) {
            Ok(None) => {} // Expected outcome for an empty file (after trim)
            Ok(Some(name)) => panic!("Expected None for empty file, but got: {name}"),
            Err(e) => panic!("Unexpected error for empty file: {e:?}"),
        }
    }

    #[test]
    fn test_test_config_manager_save_last_profile_name_overwrites() {
        // Arrange
        let dir = tempdir().unwrap();
        let mock_dir_path = dir.path().to_path_buf();
        let manager = TestConfigManager::new(mock_dir_path);
        let app_name = "AnyApp";
        let first_profile_name = "OldProfile";
        let second_profile_name = "NewProfile";

        // Act & Assert First Save/Load
        manager
            .save_last_profile_name(app_name, first_profile_name)
            .unwrap();
        let loaded_name1 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name1, first_profile_name);

        // Act & Assert Second Save/Load (Overwrite)
        manager
            .save_last_profile_name(app_name, second_profile_name)
            .unwrap();
        let loaded_name2 = manager.load_last_profile_name(app_name).unwrap().unwrap();
        assert_eq!(loaded_name2, second_profile_name);
    }
}
