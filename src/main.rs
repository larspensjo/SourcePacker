#![allow(unused)]
#![windows_subsystem = "windows"]

mod app_logic;
mod core;
mod platform_layer;
mod ui_description_layer;

use app_logic::handler::MyAppLogic;
use core::{
    CoreArchiver, CoreConfigManagerForConfig, CoreFileSystemScanner, CoreProfileManager,
    CoreStateManager, CoreTikTokenCounter,
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
    initialize_logging();

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
    let core_token_counter = Arc::new(CoreTikTokenCounter::new());
    let core_state_manager = Arc::new(CoreStateManager::new());

    let mut my_app_logic = MyAppLogic::new(
        core_config_manager,
        core_profile_manager,
        core_file_system_scanner,
        core_archiver,
        core_token_counter,
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

    // Get initial UI commands from the description layer
    let mut initial_commands =
        ui_description_layer::build_main_window_static_layout(main_window_id);
    log::debug!(
        "main: Received {} initial UI description commands.",
        initial_commands.len()
    );

    // Append the signal command to indicate completion of static UI setup
    initial_commands.push(
        platform_layer::PlatformCommand::SignalMainWindowUISetupComplete {
            window_id: main_window_id,
        },
    );

    let app_event_handler = Arc::new(Mutex::new(my_app_logic));
    log::debug!(
        "Starting platform event loop. Initial app logic commands will be queued by MainWindowUISetupComplete event."
    );

    // Pass the initial commands to the run loop for execution
    let run_result = platform_interface.main_event_loop(app_event_handler, initial_commands);

    match run_result {
        Ok(()) => log::debug!("Application exited cleanly."),
        Err(e) => {
            log::error!("Application exited with an error: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}

fn initialize_logging() {
    #[cfg(not(test))]
    {
        // Production logger (to file)
        let log_file_path = "source_packer.log";
        match std::fs::File::create(log_file_path) {
            Ok(file) => {
                let mut config_builder = ConfigBuilder::new();

                if let Err(err) = config_builder.set_time_offset_to_local() {
                    eprintln!("Warning: Failed to set local time offset: {:?}", err);
                    // Ignore for now
                }

                let config = config_builder
                    .set_thread_level(LevelFilter::Off)
                    .set_location_level(LevelFilter::Debug)
                    .build();
                if let Err(e) = simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
                    simplelog::LevelFilter::Debug,
                    config,
                    file,
                )]) {
                    eprintln!("Failed to initialize file logger: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to create log file '{}': {}", log_file_path, e);
            }
        }
    }

    #[cfg(test)]
    {
        // Test logger (to stdout/stderr)
        let mut config_builder = ConfigBuilder::new();

        if let Err(err) = config_builder.set_time_offset_to_local() {
            eprintln!("Warning: Failed to set local time offset: {:?}", err);
            // Ignore for now
        }

        let config = config_builder
            .set_thread_level(LevelFilter::Off)
            .set_location_level(LevelFilter::Debug)
            .build();

        // TermLogger is good for console output
        // Use `CombinedLogger` if you still want some tests to log to a file OR if you want multiple terminal loggers
        // For simple stdout, TermLogger is often enough.
        if simplelog::TermLogger::init(
            simplelog::LevelFilter::Debug, // Or a higher level like Info if Debug is too noisy
            config,
            simplelog::TerminalMode::Mixed, // Or ::Stdout
            simplelog::ColorChoice::Auto,
        )
        .is_err()
        {
            // Fallback if TermLogger fails (e.g., no terminal)
            let _ = simplelog::SimpleLogger::init(
                simplelog::LevelFilter::Warn,
                simplelog::Config::default(),
            );
            eprintln!("TermLogger failed, fell back to SimpleLogger for tests.");
        }
    }
}
