#![allow(unused)]

mod app_logic;
mod core;
mod platform_layer;

use app_logic::handler::MyAppLogic;
use core::CoreConfigManager;
use platform_layer::{PlatformInterface, PlatformResult, WindowConfig};
use std::sync::{Arc, Mutex};

fn main() -> PlatformResult<()> {
    println!("Application Starting...");

    // 1. Initialize the Platform Layer
    // The app name is used for things like the window class name. It has to match APP_NAME_FOR_PROFILES :-(
    let platform_interface = match PlatformInterface::new("SourcePackerApp".to_string()) {
        Ok(pi) => pi,
        Err(e) => {
            eprintln!("Fatal: Failed to initialize the platform layer: {:?}", e);
            return Err(e);
        }
    };
    println!("Platform interface initialized.");

    // 2. Initialize the Application Logic
    // Create an instance of the CoreConfigManager
    let core_config_manager = Arc::new(CoreConfigManager::new());
    // Pass the CoreConfigManager to MyAppLogic::new
    let mut my_app_logic = MyAppLogic::new(core_config_manager);
    println!("Application logic initialized.");

    // 3. Create the main window
    let main_window_config = WindowConfig {
        title: "SourcePacker - Refactored",
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

    // Notify app_logic about the created window and get initial commands
    let initial_commands = my_app_logic.on_main_window_created(main_window_id);
    println!(
        "AppLogic generated {} initial command(s).",
        initial_commands.len()
    );

    for cmd in initial_commands {
        if let Err(e) = platform_interface.execute_command(cmd) {
            eprintln!("Error executing initial command: {:?}", e);
        }
    }
    println!("Initial commands executed.");

    // 4. Prepare the event handler for the platform layer's run loop
    let app_event_handler = Arc::new(Mutex::new(my_app_logic));

    // 5. Run the platform's event loop
    println!("Starting platform event loop...");
    let run_result = platform_interface.run(app_event_handler);

    match run_result {
        Ok(()) => println!("Application exited cleanly."),
        Err(e) => {
            eprintln!("Application exited with an error: {:?}", e);
            return Err(e); // Propagate the error
        }
    }

    Ok(())
}
