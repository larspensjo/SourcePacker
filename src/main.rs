#![allow(unused)]

mod app_logic;
mod core;
mod platform_layer;
mod ui_description_layer;

use app_logic::handler::MyAppLogic;
use core::{
    CoreArchiver, CoreConfigManagerForConfig, CoreFileSystemScanner, CoreProfileManager,
    CoreStateManager,
};
use platform_layer::{PlatformInterface, PlatformResult, WindowConfig};
use std::fs::File;
use std::sync::{Arc, Mutex};

use simplelog::{CombinedLogger, ConfigBuilder, LevelFilter, WriteLogger};

/*
 * This is the main entry point for the SourcePacker application.
 * It orchestrates the initialization of all major components:
 *  1. Logging facilities.
 *  2. Core services (config, profiles, file scanning, archiving, state management).
 *  3. The platform layer interface (`PlatformInterface`).
 *  4. The application logic layer (`MyAppLogic`).
 *  5. The UI description layer (used to generate initial UI commands).
 *
 * The sequence of operations is:
 *  - Initialize logging.
 *  - Create the `PlatformInterface`.
 *  - Instantiate `MyAppLogic` with its core dependencies.
 *  - Request the platform layer to create the main application window.
 *  - Obtain UI structure commands from the `ui_description_layer`.
 *  - Execute these structural commands via the `PlatformInterface`.
 *  - Notify `MyAppLogic` that the main window's static UI is ready, allowing it
 *    to enqueue commands for data population and visibility.
 *  - Start the platform's event loop (`PlatformInterface::run`), passing in the
 *    application logic as the event handler.
 */
fn main() -> PlatformResult<()> {
    let log_file_path = "source_packer.log";
    match File::create(log_file_path) {
        Ok(file) => {
            let config = ConfigBuilder::new().set_time_format_rfc3339().build();

            if let Err(e) =
                CombinedLogger::init(vec![WriteLogger::new(LevelFilter::Debug, config, file)])
            {
                eprintln!("Failed to initialize logger: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Failed to create log file '{}': {}", log_file_path, e);
        }
    }

    log::debug!("Initialize Platform Layer");

    let platform_interface = match PlatformInterface::new("SourcePacker".to_string()) {
        Ok(pi) => pi,
        Err(e) => {
            log::error!("Fatal: Failed to initialize the platform layer: {:?}", e);
            return Err(e);
        }
    };
    log::debug!("Initialize Core Services and Application Logic.");

    let core_config_manager = Arc::new(CoreConfigManagerForConfig::new());
    let core_profile_manager = Arc::new(CoreProfileManager::new());
    let core_file_system_scanner = Arc::new(CoreFileSystemScanner::new());
    let core_archiver = Arc::new(CoreArchiver::new());
    let core_state_manager = Arc::new(CoreStateManager::new());

    let mut my_app_logic = MyAppLogic::new(
        core_config_manager,
        core_profile_manager,
        core_file_system_scanner,
        core_archiver,
        core_state_manager,
    );
    log::debug!("Create Main Window Frame.");

    let main_window_config = WindowConfig {
        title: "SourcePacker",
        width: 800,
        height: 600,
    };

    let main_window_id = match platform_interface.create_window(main_window_config) {
        Ok(id) => id,
        Err(e) => {
            log::error!("Fatal: Failed to create the main window: {:?}", e);
            return Err(e);
        }
    };
    log::debug!("Main window requested with ID: {:?}", main_window_id);
    log::debug!("Describe and Create Static UI Structure");

    let ui_commands = ui_description_layer::describe_main_window_layout(main_window_id);
    log::debug!(
        "main: Received {} UI description commands.",
        ui_commands.len()
    );
    for command in ui_commands {
        if let Err(e) = platform_interface.execute_command(command) {
            log::error!("Fatal: Failed to execute UI description command: {:?}", e);
            return Err(e);
        }
    }

    log::debug!("Notify App Logic of UI Readiness");
    my_app_logic.on_main_window_created(main_window_id);
    log::debug!("AppLogic.on_main_window_created called; initial commands enqueued.");

    let app_event_handler = Arc::new(Mutex::new(my_app_logic));

    log::debug!("Starting platform event loop...");
    let run_result = platform_interface.run(app_event_handler);

    match run_result {
        Ok(()) => log::debug!("Application exited cleanly."),
        Err(e) => {
            log::error!("Application exited with an error: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}
