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
use std::sync::{Arc, Mutex}; // For creating/truncating the log file

// Logging imports
use simplelog::{CombinedLogger, ConfigBuilder, LevelFilter, WriteLogger};

/*
 * This is the main entry point for the SourcePacker application.
 * It initializes core components, the platform layer, application logic,
 * and the UI description layer. It then sets up the main window and
 * starts the platform's event loop. Logging is also initialized here.
 */
fn main() -> PlatformResult<()> {
    // --- Initialize Logging ---
    // Configure the logger to write to "source_packer.log" and truncate it on each run.
    // We also set a default LevelFilter. For development, LevelFilter::Debug is good.
    // For release, you might prefer LevelFilter::Info or LevelFilter::Warn.
    let log_file_path = "source_packer.log";
    match File::create(log_file_path) {
        // This creates or truncates the file
        Ok(file) => {
            let config = ConfigBuilder::new()
                .set_time_format_rfc3339()
                // .set_thread_level(LevelFilter::Off) // Don't log thread_id
                // .set_target_level(LevelFilter::Trace) // Don't include target (module path) by default
                .build();

            if let Err(e) = CombinedLogger::init(vec![
                WriteLogger::new(LevelFilter::Debug, config, file),
                // You could also add TermLogger here for console output if desired:
                // TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            ]) {
                eprintln!("Failed to initialize logger: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Failed to create log file '{}': {}", log_file_path, e);
        }
    }

    log::debug!("Application Starting...");

    let platform_interface = match PlatformInterface::new("SourcePacker".to_string()) {
        Ok(pi) => pi,
        Err(e) => {
            log::error!("Fatal: Failed to initialize the platform layer: {:?}", e);
            return Err(e);
        }
    };
    log::debug!("Platform interface initialized.");

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
    log::debug!("Application logic initialized.");

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
