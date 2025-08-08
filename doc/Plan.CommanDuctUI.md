# Plan: Transforming `platform_layer` into the `CommanDuctUI` Library

**Goal:** Create a high-quality, reusable Rust library named `CommanDuctUI` from the existing `platform_layer` code, suitable for publishing and use in other projects. Completing this plan is the central goal of **Phase 1** of the Master Development Plan.

**Dependency:** The UI Descriptive Layer plan, which defined the initial structure via commands, is considered **complete**.

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
       - `version = "0.1.0"`
       - `edition = "2021"`
       - `authors = ["Your Name <your.email@example.com>"]`
       - `description = "A command-driven Rust library for declarative native Windows UI development."`
       - `license = "MIT OR Apache-2.0"`
       - `repository = "https://github.com/your_username/CommanDuctUI"`
       - `readme = "README.md"`
       - `keywords = ["gui", "ui", "windows", "native", "win32", "command-pattern", "declarative-ui"]`
       - `categories = ["gui", "os::windows-apis"]`
     - **`[dependencies]` section:**
       - Copy necessary dependencies from SourcePacker's `platform_layer` (e.g., `windows`, `log`).
       - Example `windows` dependency:
         ```toml
         [dependencies]
         windows = { version = "0.56.0", features = [
             "Win32_Foundation",
             "Win32_Graphics_Gdi",
             "Win32_System_Com",
             "Win32_System_LibraryLoader",
             "Win32_UI_Controls",
             "Win32_UI_Controls_Dialogs",
             "Win32_UI_Shell",
             "Win32_UI_Input_KeyboardAndMouse",
             "Win32_UI_WindowsAndMessaging",
             "Win32_System_WindowsProgramming", // For MulDiv, etc.
         ]}
         log = "0.4"
         ```
     - **`[lib]` section** (optional, but good for clarity):
       ```toml
       [lib]
       name = "commanductui"
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
         ├── controls/
         │   ├── mod.rs
         │   ├── button_handler.rs
         │   ├── label_handler.rs
         │   └── treeview_handler.rs
         ├── dialog_handler.rs
         ├── error.rs
         ├── lib.rs                # (New, or adapt existing platform_layer/mod.rs)
         ├── types.rs
         └── window_common.rs
     ```
   - Adapt `CommanDuctUI/src/lib.rs` to correctly declare modules and re-export the public API.

### Step 1.4: Adjust Internal Module Paths and Visibility
   - Go through the moved code and update module paths (e.g., `super::` to `crate::`).
   - Remove any logic specific to SourcePacker's UI. The library must be generic.

### Step 1.5: Initial Build and Dependency Resolution
   - Run `cargo check` and `cargo build` within the `CommanDuctUI` directory.
   - Resolve any compilation errors related to path changes, visibility, or missing dependencies.

## Phase 2: API Design and Refinement (Key Task)

### Step 2.1: Implement Type-Safe `ControlId`

   - **Goal:** Replace the raw `i32` for control IDs with a type-safe `ControlId` newtype to prevent accidental misuse and improve API clarity.

   - **Action 1: Define `ControlId` in `types.rs`**
     - In `CommanDuctUI/src/types.rs`, define the new public type:
       ```rust
       /// An opaque, type-safe identifier for a UI control.
       ///
       /// The application is responsible for assigning a unique `i32` value
       /// for each control. This wrapper ensures that control IDs are not
       /// accidentally confused with other integer types.
       #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
       pub struct ControlId(pub i32);
       ```

   - **Action 2: Update `PlatformCommand` and Event Signatures**
     - In `CommanDuctUI/src/types.rs`, update all command and event variants that use a control ID from `i32` to `ControlId`.
       ```rust
       // Example in PlatformCommand enum
       CreateButton {
           window_id: WindowId,
           control_id: ControlId, // Was: i32
           text: String,
       },
       ButtonClicked {
           window_id: WindowId,
           control_id: ControlId, // Was: i32
       },
       // ... and so on for all relevant commands and events.
       ```

   - **Action 3: Update Internal Library Implementation**
     - Throughout the `CommanDuctUI` crate, find all internal usages of `control_id: i32` and update them to `control_id: ControlId`.
     - When a Win32 API call needs the raw `i32`, use the `.0` accessor.
       - **Example:** `HMENU(control_id.0 as *mut _)`
     - Update `NativeWindowData.control_hwnd_map` from `HashMap<i32, HWND>` to `HashMap<ControlId, HWND>`.
     - Update `LayoutRule.control_id` and `parent_control_id` to use `ControlId`.

   - **Action 4: Re-compile and Fix Errors**
     - Run `cargo check` in `CommanDuctUI` and fix all the type errors that will now appear. This is a good thing—it's the compiler enforcing your new, safer API.

### Step 2.2: Define the Public API (`CommanDuctUI/src/lib.rs`)
   - Explicitly re-export all types, traits, and functions that will form the public interface of `CommanDuctUI`.
   - This now includes the new `ControlId` type.
   - Example `lib.rs` structure:
     ```rust
     // CommanDuctUI/src/lib.rs
     mod app;
     mod command_executor;
     mod controls;
     // ... other modules

     pub use app::PlatformInterface;
     pub use error::{PlatformError, Result as PlatformResult};
     pub use types::{
         AppEvent, CheckState, ControlId, DockStyle, LayoutRule, MenuAction, MenuItemConfig,
         MessageSeverity, PlatformCommand, PlatformEventHandler, TreeItemDescriptor,
         TreeItemId, WindowConfig, WindowId,
     };
     ```

### Step 2.3: Error Handling Strategy for the Library
   - Ensure `PlatformError` comprehensively covers all error conditions the library can produce.
   - Make sure errors are descriptive and help users of the library diagnose problems.
   - Avoid panics in library code; return `Result` types.

---

## Phase 3: Documentation and Examples

### Step 3.1: Write Comprehensive Inline Documentation
   - Add `///` doc comments to all public items.
   - Specifically document `ControlId` and explain that the *user of the library* is responsible for managing the uniqueness of the inner `i32` values.
   - Run `cargo doc --open` frequently to preview the documentation.

### Step 3.2: Create a High-Quality `README.md`
   - In `CommanDuctUI/README.md`, provide a minimal "getting started" example that shows manual `ControlId` definition. This demonstrates the simplest use case.
     ```markdown
     ## Getting Started

     Here is a minimal example of creating a window with a button:

     ```rust
     use commanductui::*;

     // The application defines its own unique control IDs.
     const BTN_CLICK_ME: ControlId = ControlId(101);

     // ... rest of the example ...
     ```

### Step 3.3: Add Usage Examples (`CommanDuctUI/examples/`)
   - Create a `CommanDuctUI/examples/` directory.
   - Add small, runnable example programs demonstrating key features. These examples should use the manual `const ControlId(...)` pattern, as it's the simplest way to use the library.

---

## Phase 4: Testing

### Step 4.1: Adapt and Enhance Unit Tests
   - Move/adapt existing unit tests.
   - Update tests to use the new `ControlId` type instead of raw `i32`.

### Step 4.2: Add Integration Tests
   - The example programs in `CommanDuctUI/examples/` serve as basic integration tests.
   - Create more formal integration tests in `CommanDuctUI/tests/`.

---

## Phase 5: Publishing and Maintenance

### Step 5.1: Prepare for Publishing
   - **Final API Review:** Ensure the public API with `ControlId` is stable for a `0.1.0` release.
   - **`Cargo.toml` Check:** Double-check all metadata.
   - **Local Package Check:** Run `cargo package`.

### Step 5.2: Publish to `crates.io`
   - When ready: `cargo publish` (consider `--dry-run` first).

### Step 5.3: Set up CI/CD
   - Use GitHub Actions (or similar) to automate checks and tests.

---

## Phase 6: Integration into SourcePacker

### Step 6.1: Implement Optional `build.rs` ID Generation in SourcePacker

   - **Goal:** In the SourcePacker application, automate the generation of `ControlId` constants to avoid manual management of magic numbers. This is an *application-level* choice, not a library feature.

   - **Action 1: Create `control_ids.txt` in SourcePacker**
     - In `source_packer/src/app_logic/`, create a new file named `control_ids.txt`.
     - List the *names* of all UI constants, one per line.
       ```text
       ID_TREEVIEW_CTRL
       STATUS_BAR_PANEL_ID
       STATUS_LABEL_GENERAL_ID
       // ... etc.
       ```

   - **Action 2: Update SourcePacker's `build.rs`**
     - Add logic to your existing `source_packer/build.rs` to read `control_ids.txt` and generate a Rust source file.
       ```rust
       // In source_packer/build.rs, inside your main() or build_for_windows()
       use std::env;
       use std::fs;
       use std::path::Path;

       println!("cargo:rerun-if-changed=src/app_logic/control_ids.txt");

       let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
       let dest_path = Path::new(&out_dir).join("generated_ui_constants.rs");

       let mut generated_code = String::new();
       // Import the ControlId type from the new library
       generated_code.push_str("use commanductui::ControlId;\n\n");

       let control_names = fs::read_to_string("src/app_logic/control_ids.txt")
           .expect("Failed to read control_ids.txt");

       // Assign IDs starting from a base value
       let base_id = 1001;
       for (i, name) in control_names.lines().filter(|l| !l.trim().is_empty()).enumerate() {
           let id = base_id + i as i32;
           generated_code.push_str(&format!(
               "pub const {}: ControlId = ControlId({});\n",
               name.trim(), id
           ));
       }

       fs::write(&dest_path, generated_code).expect("Failed to write generated constants file");
       ```

   - **Action 3: Update `ui_constants.rs` in SourcePacker**
     - Replace the manual `const` definitions in `source_packer/src/app_logic/ui_constants.rs` with a single `include!` macro.
       ```rust
       // In app_logic/ui_constants.rs

       // Include the constants generated by the build script.
       // The `ControlId` type will be resolved via `use commanductui::ControlId;`
       // which is included inside the generated file.
       include!(concat!(env!("OUT_DIR"), "/generated_ui_constants.rs"));

       // You can still define other non-ID constants manually here.
       pub const FILTER_COLOR_ACTIVE: u32 = 0x00FFFFE0;
       pub const FILTER_COLOR_NO_MATCH: u32 = 0x00E0E0FF;
       ```

### Step 6.2: Remove Old `platform_layer` from SourcePacker
   - Delete the `src/platform_layer/` directory from your SourcePacker project.
   - Remove its module declaration from SourcePacker's `src/main.rs` or `src/lib.rs`.

### Step 6.3: Add `CommanDuctUI` as a Dependency in SourcePacker
   - In SourcePacker's `Cargo.toml`:
     ```toml
     [dependencies]
     commanductui = { path = "../CommanDuctUI" }
     # ... other SourcePacker dependencies
     ```

### Step 6.4: Update SourcePacker Code to Use `CommanDuctUI`
   - Change `use platform_layer::...` to `use commanductui::...` throughout SourcePacker.
   - The code should already be compatible with `ControlId` because the generated constants file uses it.
   - Run `cargo check` in SourcePacker and resolve any remaining path or type errors.

### Step 6.5: Thoroughly Test SourcePacker
   - Run all SourcePacker tests.
   - Manually test all UI interactions in SourcePacker to ensure it behaves as expected with `CommanDuctUI` as a library and the new auto-generated IDs.
