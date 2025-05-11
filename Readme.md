# SourcePacker

SourcePacker is a Windows desktop tool designed to help you select and package source code files into a single text archive. This is particularly useful for preparing context for AI prompts, where token limits are a concern.

The tool allows you to:
*   Browse a directory structure in a tree view.
*   Select/deselect individual files and folders.
*   Define whitelist patterns to include only relevant files (e.g., `*.rs`, `src/**/*.md`).
*   Save and load selection configurations as "profiles."
*   Generate a concatenated text file of selected files, with clear headers for each file.
*   Get an estimated token count for your selection.

## Project Setup

This project is built using Rust and the `windows-rs` crate for interacting with the native Windows API.

### Prerequisites

1.  **Rust:** Install Rust and Cargo from [rustup.rs](https://rustup.rs/). The latest stable version is recommended.
2.  **Windows Development Tools:** Ensure you have the necessary components for Windows development. This typically includes the Windows SDK. When installing Visual Studio, select the "Desktop development with C++" workload and ensure the latest Windows SDK is included. `windows-rs` relies on these.

### Building and Running

*   **Build:** `cargo build`
*   **Run:** `cargo run`
*   **Check (lints):** `cargo clippy`
*   **Format:** `cargo fmt`
*   **Test:** `cargo test`

### Development Focus

*   **Modularity:** Core logic (file system, profiles, archiving) will be separated from UI code to allow for easier unit testing.
*   **Error Handling:** Use Rust's `Result` type extensively for operations that can fail.
*   **Best Practices:** Adhere to Rust community best practices, including using `clippy` and `rustfmt`.
