// The main function for the build script. This is compiled on ALL platforms.
fn main() {
    // This attribute ensures that the call to our Windows-specific helper function
    // is only included in the build script's code when compiling on a Windows target.
    // On Linux, this main function will be completely empty, which is valid and compiles.
    #[cfg(target_os = "windows")]
    build_for_windows();
}

// This helper function, and everything inside it, will only be defined and compiled
// when the target OS is Windows.
#[cfg(target_os = "windows")]
fn build_for_windows() {
    // Both `use` statements are now safely inside the conditionally compiled block.
    // The Linux compiler will never see these lines.
    use embed_resource::{CompilationResult, compile};

    // These println! statements will also only run on Windows.
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");

    // The compiler error clearly indicates that `compile` returns `CompilationResult`.
    // Your original logic using a `match` block was correct, and we'll use it here.
    let result = compile("app.rc", &[] as &[&str]);

    match result {
        CompilationResult::Ok => {
            println!("Resource file compiled successfully.");
        }
        // This case shouldn't be hit since we are inside a #[cfg(target_os = "windows")] block,
        // but handling it is exhaustive and good practice.
        CompilationResult::NotWindows => {
            eprintln!(
                "Build script warning: embed_resource reports not running on Windows, despite cfg flag."
            );
        }
        // Combine the failure arms as they lead to the same outcome.
        CompilationResult::Failed(e) | CompilationResult::NotAttempted(e) => {
            let error_message = format!("Failed to compile resource file: {}", e);
            eprintln!("{}", error_message);
            // It's critical to fail the build if resources can't be compiled.
            panic!("{}", error_message);
        }
    }
}
