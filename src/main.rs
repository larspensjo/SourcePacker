#![allow(unused)]

mod app_logic;
mod core;
mod platform_layer;

use app_logic::handler::MyAppLogic;
use platform_layer::{PlatformInterface, PlatformResult, WindowConfig}; // Re-export key types
use std::sync::{Arc, Mutex};

fn main() -> PlatformResult<()> {
    println!("Application Starting...");

    // 1. Initialize the Platform Layer
    // The app name is used for things like the window class name. It has to match APP_NAME_FOR_PROFILES :-(
    let platform_interface = match PlatformInterface::new("SourcePackerApp".to_string()) {
        Ok(pi) => pi,
        Err(e) => {
            // If platform init fails, we can't show a GUI error box easily without the platform.
            // So, eprintln is the best we can do here.
            eprintln!("Fatal: Failed to initialize the platform layer: {:?}", e);
            // Convert PlatformError to a generic error or just return it if main can.
            // For simplicity, we'll just exit if this critical step fails.
            return Err(e);
        }
    };
    println!("Platform interface initialized.");

    // 2. Initialize the Application Logic
    let mut my_app_logic = MyAppLogic::new();
    println!("Application logic initialized.");

    // 3. Create the main window
    // The application logic requests window creation via the platform interface.
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

    // Notify app_logic about the created window and get initial commands (like ShowWindow, PopulateTreeView)
    let initial_commands = my_app_logic.on_main_window_created(main_window_id);
    println!(
        "AppLogic generated {} initial command(s).",
        initial_commands.len()
    );

    for cmd in initial_commands {
        if let Err(e) = platform_interface.execute_command(cmd) {
            // Log error, decide if fatal. For now, just log.
            eprintln!("Error executing initial command: {:?}", e);
        }
    }
    println!("Initial commands executed.");

    // 4. Prepare the event handler for the platform layer's run loop
    // The event handler (MyAppLogic) needs to be shareable and mutable, hence Arc<Mutex<>>.
    let app_event_handler = Arc::new(Mutex::new(my_app_logic));

    // 5. Run the platform's event loop
    // This will block until the application quits.
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
