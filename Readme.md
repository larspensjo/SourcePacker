# SourcePacker

SourcePacker is a Windows desktop tool for managing and packaging evolving source code projects into text archives, primarily for AI prompt context. It actively monitors your file hierarchy, detects changes (additions, removals, modifications), and helps you maintain curated subsets of files for different archives using profiles. Newly detected files are marked as "unknown," requiring user classification. The tool also notifies you when your selected files have changed more recently than their corresponding archive, prompting for an update.

## Project-Based Workflow

SourcePacker now works on a **per-project** basis. The first action in each session is to choose a project folder via **File → Open Folder**. All profile data lives alongside the project in a hidden `.sourcepacker` directory to keep projects self contained and portable.

What gets stored under `.sourcepacker/`:

* `profiles/` – profile JSON files for this project only.
* `last_profile.txt` – remembers the last profile used for this project.

Profiles can still point to arbitrary scan roots and archive destinations, but the application state is anchored to the opened project folder. The `.sourcepacker` directory is automatically ignored by the scanner so it never appears in the file tree.

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

### Git Ignore Recommendation

Add the following to your project `.gitignore` if you want to avoid checking in local SourcePacker state:

```
/.sourcepacker/
```

If you prefer finer control, ignore at least `/.sourcepacker/profiles/` and `/.sourcepacker/last_profile.txt`.

### Development Focus

*   **Modularity:** Core logic (file system, profiles, archiving) will be separated from UI code to allow for easier unit testing.
*   **Error Handling:** Use Rust's `Result` type extensively for operations that can fail.
*   **Best Practices:** Adhere to Rust community best practices, including using `clippy` and `rustfmt`.
