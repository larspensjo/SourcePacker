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

### Initial Project Configuration

1.  **Clone the repository (or create a new project):**
    ```bash
    # If cloning
    git clone <repository-url>
    cd source-packer

    # If starting new
    cargo new source_packer --bin
    cd source_packer
    ```

2.  **Add Dependencies to `Cargo.toml`:**
    Open your `Cargo.toml` file and add the following dependencies under the `[dependencies]` section. Versions should be checked for the latest compatible ones.

    ```toml
    [dependencies]
    windows = { version = "0.52.0", features = [
        "Win32_Foundation",
        "Win32_System_LibraryLoader",
        "Win32_UI_WindowsAndMessaging",
        "Win32_UI_Controls",
        "Win32_Graphics_Gdi",
        "Win32_System_Com", # If using COM components like FileSaveDialog
        "Win32_Storage_FileSystem",
        "System_Threading", # For DispatcherQueue if using XAML islands or modern WinRT async
        "UI_Xaml_Controls", # Example if you were to use XAML, not strictly needed for pure Win32
        "Win32_UI_Shell", # For SHGetKnownFolderPath or dialogs
    ]}
    serde = { version = "1.0", features = ["derive"] }
    serde_json = "1.0"
    directories-rs = "5.0" # For system directories like %APPDATA%
    walkdir = "2.4"        # For directory traversal
    glob = "0.3"           # For file pattern matching

    # Optional, for token counting (example, choose one that fits your AI)
    # tiktoken-rs = "0.5"
    ```
    *Note: The exact features for `windows-rs` will depend on the specific Win32/WinRT APIs you end up using. Start with a minimal set and add as needed.*

3.  **Configure `build.rs` (Optional but often useful for `windows-rs`):**
    Sometimes, for `windows-rs` to work smoothly, especially with WinRT components or if you want to generate bindings more explicitly, you might use a `build.rs` script. For basic Win32, it might not be strictly necessary if `Cargo.toml` features are sufficient.
    If needed, create `build.rs` in the project root:
    ```rust
    // build.rs
    fn main() {
        windows::build!(
            // List specific types or modules you want bindings for if not covered by Cargo.toml features
            // Example:
            // Windows::Win32::UI::WindowsAndMessaging::MessageBoxW,
            // Windows::Win32::Foundation::HWND,
        );
    }
    ```
    And add `windows-build = "0.52.0"` (match your `windows` crate version) to `[build-dependencies]` in `Cargo.toml`.

4.  **Set Rust Edition (in `Cargo.toml`):**
    Ensure your `Cargo.toml` specifies a recent Rust edition:
    ```toml
    [package]
    name = "source_packer"
    version = "0.1.0"
    edition = "2021" # Or "2024" when stable
    ```

### Building and Running

*   **Build:** `cargo build`
*   **Run:** `cargo run`
*   **Check (lints):** `cargo clippy`
*   **Format:** `cargo fmt`
*   **Test:** `cargo test`

### Recommended Crates & Libraries Summary

*   **`windows-rs`**: For all Windows API interactions (UI, file dialogs, etc.).
*   **`serde` / `serde_json`**: For serializing and deserializing profiles to/from JSON.
*   **`directories-rs`**: To reliably get the path to `%APPDATA%` or other standard system directories.
*   **`walkdir`**: For efficient recursive directory traversal.
*   **`glob`**: For matching filenames against whitelist patterns.
*   **`tiktoken-rs` (or similar)**: (Optional) For accurate token counting aligned with specific AI models (e.g., OpenAI's). A simpler word/character count can be a starting point.

### Development Focus

*   **Modularity:** Core logic (file system, profiles, archiving) will be separated from UI code to allow for easier unit testing.
*   **Error Handling:** Use Rust's `Result` type extensively for operations that can fail.
*   **Best Practices:** Adhere to Rust community best practices, including using `clippy` and `rustfmt`.
