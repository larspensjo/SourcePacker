# Plan: Transforming `platform_layer` into the `CommanDuctUI` Library

**Goal:** Create a high-quality, reusable Rust library named `CommanDuctUI` from the existing `platform_layer` code, suitable for publishing to `crates.io` and use in other projects.

---

## Phase 1: Library Creation and Code Migration

### Step 1.1: Initialize New Library Crate (`CommanDuctUI`)
   - Outside your SourcePacker project, create a new Rust library:
     ```bash
     cargo new CommanDuctUI --lib
     cd CommanDuctUI
     ```

### Step 1.2: Configure `Cargo.toml` for `CommanDuctUI`
   - Edit `CommanDuctUI/Cargo.toml`:
     - **`[package]` section:**
       - `name = "CommanDuctUI"`
       - `version = "0.1.0"` (or your initial version)
       - `edition = "2021"` (or your current Rust edition)
       - `authors = ["Your Name <your.email@example.com>"]`
       - `description = "A command-driven Rust library for declarative native Windows UI development."`
       - `license = "MIT OR Apache-2.0"` (or your chosen OSI-approved license)
       - `repository = "https://github.com/your_username/CommanDuctUI"` (update with actual URL)
       - `readme = "README.md"`
       - `keywords = ["gui", "ui", "windows", "native", "win32", "command-pattern", "declarative-ui"]`
       - `categories = ["gui", "os::windows-apis"]`
     - **`[dependencies]` section:**
       - Copy necessary dependencies from SourcePacker's `platform_layer` (e.g., `windows`, `log`).
       - Example `windows` dependency:
         ```toml
         [dependencies]
         windows = { version = "0.56.0", features = [ # Ensure this list is exhaustive for platform_layer's needs
             "Win32_Foundation",
             "Win32_Graphics_Gdi",
             "Win32_System_Com",
             "Win32_System_LibraryLoader",
             "Win32_UI_Controls",
             "Win32_UI_Controls_Dialogs",
             "Win32_UI_Shell",
             "Win32_UI_Input_KeyboardAndMouse",
             "Win32_UI_WindowsAndMessaging",
             # Add any other Win32 features used by platform_layer
         ]}
         log = "0.4"
         # Add other dependencies like once_cell if used by platform_layer
         ```
     - **`[lib]` section** (optional, but good for clarity):
       ```toml
       [lib]
       name = "commanductui" # Typically snake_case for the lib name attribute
       crate-type = ["lib", "rlib"]
       ```

### Step 1.3: Move `platform_layer` Code into `CommanDuctUI`
   - Copy the contents of SourcePacker's `src/platform_layer/` directory into `CommanDuctUI/src/`.
   - Your `CommanDuctUI/src/` structure might look like:
     ```
     CommanDuctUI/
     └── src/
         ├── app.rs
         ├── command_executor.rs
         ├── controls/             # (Previously platform_layer/controls)
         │   ├── mod.rs
         │   ├── button_handler.rs (if created)
         │   ├── label_handler.rs  (if created)
         │   └── treeview_handler.rs
         ├── dialog_handler.rs
         ├── error.rs
         ├── lib.rs                # (New, or adapt existing platform_layer/mod.rs)
         ├── types.rs
         └── window_common.rs
     ```
   - Adapt `CommanDuctUI/src/lib.rs` (or rename `platform_layer/mod.rs` to `lib.rs` and adapt it) to correctly declare modules and re-export the public API.

### Step 1.4: Adjust Internal Module Paths and Visibility
   - Go through the moved code and update module paths:
     - `super::` might need to change to `crate::` or be removed if items are in the same module.
     - References to `crate::app_logic::ui_constants` (if any were in `platform_layer`) must be removed. The library should not know about application-specific constants. Control IDs it works with should be passed in via commands.
   - Review `pub`, `pub(crate)`, and private visibility. Ensure only the intended public API is exposed from `lib.rs`.

### Step 1.5: Initial Build and Dependency Resolution
   - Run `cargo check` and `cargo build` within the `CommanDuctUI` directory.
   - Resolve any compilation errors related to path changes, visibility, or missing dependencies.
   - **Crucially**: Remove any logic that is specific to SourcePacker's UI (e.g., hardcoded layout fallbacks, specific status bar segmenting logic within `handle_wm_size`, assumptions about `ID_TREEVIEW_CTRL` being the only TreeView ID) as outlined in the previous analysis. The library must be generic.

---

## Phase 2: API Design and Refinement

### Step 2.1: Define the Public API (`CommanDuctUI/src/lib.rs`)
   - Explicitly re-export all types, traits, and functions that will form the public interface of `CommanDuctUI`.
   - This typically includes:
     - `PlatformInterface`
     - `WindowConfig`
     - `PlatformCommand` (and its variants)
     - `AppEvent` (and its variants)
     - `PlatformEventHandler` trait
     - Key identifiers like `WindowId`, `TreeItemId`, `MenuAction`.
     - Layout types like `DockStyle`, `LayoutRule`.
     - Error types like `PlatformError`.
     - Any other necessary public enums or structs (e.g., `MessageSeverity`, `CheckState`).
   - Example `lib.rs` structure:
     ```rust
     // CommanDuctUI/src/lib.rs
     mod app;
     mod command_executor;
     mod controls;
     mod dialog_handler;
     mod error;
     mod types;
     mod window_common;

     pub use app::PlatformInterface;
     pub use error::{PlatformError, Result as PlatformResult}; // Assuming Result is a common pattern
     pub use types::{
         AppEvent, CheckState, DockStyle, LayoutRule, MenuAction, MenuItemConfig,
         MessageSeverity, PlatformCommand, PlatformEventHandler, TreeItemDescriptor,
         TreeItemId, WindowConfig, WindowId,
         // Potentially ControlType, ControlProperties if you go that route
     };
     // Potentially other specific exports if needed by users
     ```

### Step 2.2: Refine Internal Structure and Visibility
   - Ensure modules like `command_executor`, `dialog_handler`, `window_common`, and the `controls` sub-module are `pub(crate)` or private unless they contain items intended for direct public use (unlikely for most of their contents).
   - Clean up any `use` statements that are no longer needed or are incorrect after the move.

### Step 2.3: Error Handling Strategy for the Library
   - Ensure `PlatformError` comprehensively covers all error conditions the library can produce.
   - Make sure errors are descriptive and help users of the library diagnose problems.
   - Avoid panics in library code; return `Result` types.

### Step 2.4: Configuration Options (if any)
   - Consider if `CommanDuctUI` needs any library-level configuration (e.g., for logging behavior specific to the library, advanced Win32 settings).
   - If so, design a clear way to pass this configuration during `PlatformInterface::new()` or via other methods. (For now, it seems `app_name_for_class` in `PlatformInterface::new` is the main one).

---

## Phase 3: Documentation

### Step 3.1: Write Comprehensive Inline Documentation (Doc Comments)
   - Add `///` doc comments to all public items in `lib.rs` (structs, enums, traits, functions, methods).
   - Explain what each item is, how to use it, its parameters, return values, and any panics or errors it might produce.
   - Document important `pub(crate)` items as well for internal maintainability.
   - Provide usage examples directly within the doc comments where appropriate (`# Examples`).
   - Run `cargo doc --open` frequently to preview the documentation.

### Step 3.2: Create a High-Quality `README.md`
   - In `CommanDuctUI/README.md`:
     - **Project Title and Badge(s):** `CommanDuctUI`, crates.io version, license, build status.
     - **Brief Description:** What the library is and its main purpose.
     - **Features:** Key capabilities (command-driven, declarative layout, native Windows UI, etc.).
     - **Getting Started/Usage:**
       - How to add it to `Cargo.toml`.
       - A minimal, complete example of how to create a window and handle a simple event.
     - **Core Concepts:** Briefly explain `PlatformCommand`, `AppEvent`, `PlatformEventHandler`, and the layout system.
     - **License:** State the license.
     - **Contributing:** Link to `CONTRIBUTING.md` if you have one.
     - **(Optional) Roadmap/Future Plans.**

### Step 3.3: Add Usage Examples (`CommanDuctUI/examples/`)
   - Create a `CommanDuctUI/examples/` directory.
   - Add small, runnable example programs demonstrating key features:
     - `basic_window.rs`: Creating a window, showing it.
     - `button_click.rs`: Creating a button, handling its click event.
     - `simple_layout.rs`: Using `DefineLayout` for a few controls.
     - `treeview_example.rs`: Populating and interacting with a TreeView.
     - `menu_example.rs`: Creating and handling a main menu.
   - Each `.rs` file in `examples/` can be run with `cargo run --example <example_name>`.
   - These examples also serve as integration tests.

### Step 3.4: Consider `CONTRIBUTING.md` and `CODE_OF_CONDUCT.md`
   - If you plan for others to contribute:
     - `CONTRIBUTING.md`: Guidelines for contributions (code style, PR process, etc.).
     - `CODE_OF_CONDUCT.md`: Adopt a standard code of conduct (e.g., Contributor Covenant).

---

## Phase 4: Testing

### Step 4.1: Adapt and Enhance Unit Tests
   - Move/adapt existing unit tests from SourcePacker's `platform_layer` (like those in `command_executor_tests.rs` or `window_common_tests.rs` if they exist) into `CommanDuctUI`.
   - Ensure tests are self-contained within the library and do not depend on SourcePacker-specific logic.
   - Add more unit tests for individual functions and modules, especially for the layout engine and command executors.
   - Use mocks for the `PlatformEventHandler` trait when testing parts of `CommanDuctUI` that interact with it.

### Step 4.2: Add Integration Tests
   - The example programs in `CommanDuctUI/examples/` also serve as basic integration tests.
   - You can create more formal integration tests in `CommanDuctUI/tests/`. These tests would use `CommanDuctUI` as a library user would.
     - `tests/api_tests.rs`: Test the public API functions, window creation, command processing, and event flow.

### Step 4.3: Test on Different Windows Versions (if feasible)
   - If you have access, test the library on different Windows versions (e.g., Windows 10, Windows 11) to catch platform-specific issues.

---

## Phase 5: Publishing and Maintenance

### Step 5.1: Prepare for Publishing
   - **Final API Review:** Ensure the public API is stable and ergonomic for a `0.1.0` release.
   - **Versioning:** Follow Semantic Versioning (SemVer). Start with `0.1.0`.
   - **`Cargo.toml` Check:** Double-check all metadata (description, license, repository, keywords, categories).
   - **Local Package Check:** Run `cargo package` to see if Cargo can successfully package your crate and to identify any issues (like uncommitted changes or files that shouldn't be included). Check the generated `.crate` file.
   - **Login to `crates.io`:** `cargo login` (you'll need an account on `crates.io`).

### Step 5.2: Publish to `crates.io`
   - When ready:
     ```bash
     cargo publish
     ```
   - (Optional) Use `--dry-run` first: `cargo publish --dry-run` to check for warnings/errors without actually publishing.

### Step 5.3: Set up CI/CD (Optional but Recommended)
   - Use GitHub Actions (or similar) to:
     - Run `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check` on every push/PR.
     - (Optional) Automatically publish to `crates.io` when a new tag is pushed.

### Step 5.4: Plan for Future Maintenance and Issue Tracking
   - Use your GitHub repository's issue tracker.
   - Be prepared to respond to issues and review pull requests if you encourage contributions.

---

## Phase 6: Integration into SourcePacker

### Step 6.1: Remove Old `platform_layer` from SourcePacker
   - Delete the `src/platform_layer/` directory from your SourcePacker project.
   - Remove its module declaration from SourcePacker's `src/main.rs` or `src/lib.rs`.

### Step 6.2: Add `CommanDuctUI` as a Dependency in SourcePacker
   - In SourcePacker's `Cargo.toml`:
     ```toml
     [dependencies]
     CommanDuctUI = "0.1.0" # Or use a path for local development:
     # CommanDuctUI = { path = "../CommanDuctUI" }
     # ... other SourcePacker dependencies
     ```

### Step 6.3: Update SourcePacker Code to Use `CommanDuctUI`
   - Change `use platform_layer::...` to `use commanductui::...` throughout SourcePacker.
   - Resolve any breaking changes if the API of `CommanDuctUI` diverged slightly from the old `platform_layer`.
   - Ensure `app_logic::ui_constants` is the source of truth for control IDs used by `ui_description_layer` and `app_logic`.

### Step 6.4: Thoroughly Test SourcePacker
   - Run all SourcePacker tests.
   - Manually test all UI interactions in SourcePacker to ensure it behaves as expected with `CommanDuctUI` as a library.
