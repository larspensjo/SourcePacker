fn main() {
    // Tell Cargo to rerun this build script if app.rc or app.manifest changes.
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");

    // Compile app.rc and link it.
    // Pass an empty slice for options if no specific flags are needed.
    let _ = embed_resource::compile("app.rc", &[] as &[&str]);
    // Or, if embed_resource::NONE is indeed a valid constant for your version:
    // embed_resource::compile("app.rc", embed_resource::NONE);
}
