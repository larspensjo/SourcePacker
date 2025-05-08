// Declare the ui_facade module
mod ui_facade;

// Use the facade's components
use ui_facade::{App, UiResult, WindowBuilder};
// Still need PostQuitMessage for the on_destroy callback
use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

fn main() -> UiResult<()> {
    // main now returns the facade's Result type
    // 1. Initialize the application facade
    let app = App::new()?;
    println!("App instance: {:?}", app.instance);

    // 2. Use the WindowBuilder to configure and create the main window
    let main_window = WindowBuilder::new(app)
        .title("SourcePacker - UI Facade")
        .size(800, 600) // Explicit size
        .on_destroy(|| {
            // This callback is executed when WM_DESTROY is received
            println!("Main window on_destroy callback: Quitting application.");
            unsafe {
                PostQuitMessage(0); // Signal the message loop to terminate
            }
        })
        .build()?; // This creates the window

    println!("Window HWND: {:?}", main_window.hwnd);

    // 3. Show the window
    main_window.show();

    // 4. Run the application's message loop
    println!("Running app loop...");
    let run_result = app.run();
    println!("App loop exited.");

    run_result // Return the result of the app run
}
