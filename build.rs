use embed_resource::CompilationResult;

fn main() {
    // Check if the target OS is Windows
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        println!("cargo:rerun-if-changed=app.rc");
        println!("cargo:rerun-if-changed=app.manifest");
        let result = embed_resource::compile("app.rc", &[] as &[&str]);
        match result {
            CompilationResult::NotWindows => {
                eprintln!("Windows only resource file found. Skipping...")
            }
            CompilationResult::Ok => {
                println!("Resource file compiled successfully.")
            }
            CompilationResult::Failed(e) => {
                eprintln!("Failed to compile resource file: {}", e);
            }
            CompilationResult::NotAttempted(e) => {
                eprintln!("Failed to compile resource file: {}", e);
            }
        }
    }
}
