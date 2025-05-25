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
use std::sync::{Arc, Mutex};

fn main() -> PlatformResult<()> {
    println!("Application Starting...");

    let platform_interface = match PlatformInterface::new("SourcePacker".to_string()) {
        Ok(pi) => pi,
        Err(e) => {
            eprintln!("Fatal: Failed to initialize the platform layer: {:?}", e);
            return Err(e);
        }
    };
    println!("Platform interface initialized.");

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
    println!("Application logic initialized.");

    let main_window_config = WindowConfig {
        title: "SourcePacker",
        width: 800,
        height: 600,
    };

    let main_window_id = match platform_interface.create_window(main_window_config) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Fatal: Failed to create the main window: {:?}", e);
            return Err(e);
        }
    };
    println!("Main window requested with ID: {:?}", main_window_id);

    // Call on_main_window_created to enqueue initial commands
    my_app_logic.on_main_window_created(main_window_id);
    println!("AppLogic.on_main_window_created called; initial commands enqueued.");

    let app_event_handler = Arc::new(Mutex::new(my_app_logic));

    println!("Starting platform event loop...");
    let run_result = platform_interface.run(app_event_handler);

    match run_result {
        Ok(()) => println!("Application exited cleanly."),
        Err(e) => {
            eprintln!("Application exited with an error: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}
