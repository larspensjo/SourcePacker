/*
 * This module provides utility functions for path manipulation, focusing on
 * retrieving and ensuring the existence of application-specific directories.
 * It aims to centralize common directory logic used by different parts of the core.
 */
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

/*
 * Retrieves the application's primary local configuration directory.
 * This function determines the platform-specific path for local (non-roaming)
 * application configuration data. It ensures the directory exists, creating it
 * if necessary. The path is derived without using an organization qualifier,
 * placing it directly under the user's local application data directory structure
 * (e.g., AppData/Local on Windows).
 *
 * Args:
 *   app_name: The name of the application, used to derive the directory path.
 *
 * Returns:
 *   An `Option<PathBuf>` containing the path to the directory if successful,
 *   or `None` if the directory could not be determined or created (e.g., due
 *   to I/O errors or if `ProjectDirs` fails to identify a suitable location).
 */
pub fn get_base_app_config_local_dir(app_name: &str) -> Option<PathBuf> {
    log::trace!(
        "PathUtils: Attempting to get base app config local dir for '{}'",
        app_name
    );
    ProjectDirs::from("", "", app_name).and_then(|proj_dirs| {
        let config_path = proj_dirs.config_local_dir();
        if !config_path.exists() {
            if let Err(e) = fs::create_dir_all(config_path) {
                log::error!(
                    "PathUtils: Failed to create base app config directory {:?}: {}",
                    config_path,
                    e
                );
                return None;
            }
            log::debug!(
                "PathUtils: Created base app config directory: {:?}",
                config_path
            );
        } else {
            log::trace!(
                "PathUtils: Base app config directory already exists: {:?}",
                config_path
            );
        }
        Some(config_path.to_path_buf())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    // Note: ProjectDirs behavior can be environment-dependent.
    // These tests verify its basic functionality assuming a typical environment.

    #[test]
    fn test_get_base_app_config_local_dir_creates_if_not_exists() {
        // Arrange
        // Using a highly unique app name to avoid collision with actual user configs
        // or other test runs.
        let unique_app_name = format!("TestApp_PathUtils_Create_{}", rand::random::<u128>());
        // Ensure the directory does not exist before the call (important for this test)
        if let Some(proj_dirs) = ProjectDirs::from("", "", &unique_app_name) {
            let path_to_check = proj_dirs.config_local_dir();
            if path_to_check.exists() {
                fs::remove_dir_all(path_to_check)
                    .expect("Pre-test cleanup failed for newly generated unique_app_name path");
            }
        }

        // Act
        let path_opt = get_base_app_config_local_dir(&unique_app_name);

        // Assert
        assert!(
            path_opt.is_some(),
            "Should return a path for a new app name"
        );
        let path = path_opt.unwrap();
        assert!(
            path.exists(),
            "Directory should have been created at {:?}",
            path
        );
        assert!(path.is_dir());
        assert!(
            path
                .to_string_lossy()
                .to_lowercase()
                .contains(&unique_app_name.to_lowercase()),
            "Path should contain the app name. Path: {:?}",
            path
        );

        // Cleanup: Remove the created directory
        if path.exists() {
            // Use the same logic ProjectDirs would use to find the directory to remove.
            if let Some(proj_dirs_for_cleanup) = ProjectDirs::from("", "", &unique_app_name) {
                let dir_to_remove = proj_dirs_for_cleanup.config_local_dir();
                if dir_to_remove.exists() {
                    // Double check before removing
                    if let Err(e) = fs::remove_dir_all(dir_to_remove) {
                        // Log error but don't fail test for cleanup issue.
                        eprintln!(
                            "Test cleanup error for get_base_app_config_local_dir_creates_if_not_exists (dir: {}): {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_get_base_app_config_local_dir_returns_existing() {
        // Arrange
        let unique_app_name = format!("TestApp_PathUtils_Existing_{}", rand::random::<u128>());

        // Create it once
        let first_path = get_base_app_config_local_dir(&unique_app_name)
            .expect("First creation of base app config dir failed");
        assert!(
            first_path.exists(),
            "Directory should exist after first call"
        );

        // Act: Call it again
        let second_path_opt = get_base_app_config_local_dir(&unique_app_name);

        // Assert
        assert!(
            second_path_opt.is_some(),
            "Should return a path on second call"
        );
        assert_eq!(
            second_path_opt.unwrap(),
            first_path,
            "Should return the same existing path"
        );

        // Cleanup
        if first_path.exists() {
            if let Some(proj_dirs_for_cleanup) = ProjectDirs::from("", "", &unique_app_name) {
                let dir_to_remove = proj_dirs_for_cleanup.config_local_dir();
                if dir_to_remove.exists() {
                    if let Err(e) = fs::remove_dir_all(dir_to_remove) {
                        eprintln!(
                            "Test cleanup error for get_base_app_config_local_dir_returns_existing (dir: {}): {}",
                            first_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }
}
