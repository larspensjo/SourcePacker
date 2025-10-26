# Plan: Transforming `platform_layer` into the `CommanDuctUI` Library

**Goal:** Refactor the existing `platform_layer` into a distinct, reusable library named `CommanDuctUI`. This will be done by creating a separate Git repository for the library and integrating it back into SourcePacker as a **Git submodule**.

**Strategy:** This plan follows a highly incremental and safe process. We will first add the new library as an empty submodule, allowing the old and new locations to coexist temporarily. We will then migrate the code and update the application to use the new library, ensuring the project remains in a buildable state at all times.

---

### **Phase 1: Integrate the Empty Submodule**

**Goal:** Add the `CommanDuctUI` repository to the `SourcePacker` project as an empty, non-disruptive submodule.

*   **Prerequisites:** You have already created an empty Git repository for `CommanDuctUI`.

*   **Step 1.1: Add the Submodule to SourcePacker**
    *   **Action:** From the root of your `source_packer` repository, run the `git submodule add` command. Crucially, we will place it at `src/CommanDuctUI`, which does not conflict with the existing `src/platform_layer`.
        ```bash
        cd /path/to/source_packer
        git submodule add <url_to_your_new_CommanDuctUI_repo> src/CommanDuctUI
        ```
    *   **Result:** This creates a `.gitmodules` file and an empty `src/CommanDuctUI` directory. Your `source_packer` repository now tracks a specific (the initial) commit of `CommanDuctUI`.

*   **Step 1.2: Verify the Checkpoint**
    *   **Action:** Run `cargo check` in your `source_packer` project.
    *   **Expected Result:** The project will build successfully. The new submodule is just an empty directory and is not yet referenced by any code or by `Cargo.toml`, so it has no effect.

*   **Step 1.3: Commit this Structural Change**
    *   **Action:** Commit the addition of the submodule. This creates a safe, stable checkpoint you can always return to.
        ```bash
        git add .gitmodules src/CommanDuctUI
        git commit -m "chore: Add empty CommanDuctUI as a submodule"
        git push
        ```

---

### **Phase 2: Migrate Code and Re-wire the Application**

**Goal:** Move the code from `platform_layer` into the `CommanDuctUI` submodule, configure it as a proper library crate, and update SourcePacker to use it.

*   **Step 2.1: Move the Code Using Git**
    *   **Action:** Use `git mv` to move the files. This preserves their history.
        ```bash
        # Move the contents of the platform_layer directory
        git mv src/platform_layer/* src/CommanDuctUI/src/

        # Move the module root file and rename it to the library root
        git mv src/platform_layer.rs src/CommanDuctUI/src/lib.rs
        ```
    *   **Result:** The old `src/platform_layer` directory is now empty. All the platform code now lives inside `src/CommanDuctUI/src/`.

*   **Step 2.2: Configure the `CommanDuctUI` Crate**
    *   **Action:** Create `src/CommanDuctUI/Cargo.toml`. Add the necessary package information, license, and dependencies.
        ```toml
        # In src/CommanDuctUI/Cargo.toml
        [package]
        name = "commanductui"
        version = "0.1.0"
        edition = "2021"
        license = "MIT OR Apache-2.0"
        description = "A declarative, command-driven Rust library for native Windows (Win32) UI development..."

        [dependencies]
        windows = { version = "...", features = [...] }
        log = "0.4"
        ```
    *   **Action:** Open `src/CommanDuctUI/src/lib.rs`. Review the module declarations and ensure the public API is correctly exposed with `pub use`.

*   **Step 2.3: Fix Paths and Build the Library**
    *   **Action:** Go through the files in `src/CommanDuctUI/src/` and update the `use` statements (e.g., `use crate::platform_layer::types` becomes `use crate::types`).
    *   **Action:** Verify the library builds correctly **in isolation**.
        ```bash
        cd src/CommanDuctUI
        cargo check
        # Fix any errors until the library is self-contained and builds cleanly.
        cd ../..
        ```

*   **Step 2.4: Update SourcePacker to Use the New Crate**
    *   **Action:** In `source_packer/Cargo.toml`, add the dependency on the local `CommanDuctUI` crate.
        ```toml
        # In source_packer/Cargo.toml
        [dependencies]
        # ... other dependencies
        commanductui = { path = "src/CommanDuctUI" }
        ```
    *   **Action:** In `source_packer/src/main.rs`, remove the old module declaration: `mod platform_layer;`.
    *   **Action:** Throughout the `source_packer` codebase, change all `use platform_layer::...` statements to `use commanductui::...`.

*   **Step 2.5: Final Verification**
    *   **Action:** From the root of your `source_packer` project, run `cargo check` and `cargo test`.
    *   **Expected Result:** Everything should now compile and pass tests using the new `commanductui` crate via the submodule path.

---

### **Phase 3: Finalize with Atomic Commits**

**Goal:** Save the completed refactoring in a clean, atomic way across both repositories.

*   **Step 3.1: Commit the `CommanDuctUI` Library Changes**
    *   **Action:** First, commit the new code within the submodule.
        ```bash
        cd src/CommanDuctUI
        git add .
        git commit -m "feat: Initial implementation of the library"
        git push
        cd ../..
        ```

*   **Step 3.2: Commit the SourcePacker Refactoring**
    *   **Action:** Now, commit the changes in the main project. `git status` will show the file moves, the `Cargo.toml` update, and the updated commit pointer for `src/CommanDuctUI`.
        ```bash
        git status
        # Should show:
        #   renamed:    src/platform_layer.rs -> src/CommanDuctUI/src/lib.rs
        #   renamed:    src/platform_layer/app.rs -> src/CommanDuctUI/src/app.rs
        #   ... (and other renames)
        #   modified:   Cargo.toml
        #   modified:   src/main.rs
        #   ... (and other source files)
        #   modified:   src/CommanDuctUI (new commits)

        git add .
        git commit -m "refactor: Migrate platform_layer to CommanDuctUI submodule"
        git push
        ```

*   **Step 3.3: Clean Up the Empty Directory**
    *   **Action:** The old `src/platform_layer` directory is now empty and can be removed.
        ```bash
        git rm src/platform_layer
        git commit -m "chore: Remove empty platform_layer directory"
        git push
        ```
