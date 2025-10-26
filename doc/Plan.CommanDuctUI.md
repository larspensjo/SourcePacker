# Plan: Transforming `platform_layer` into the `CommanDuctUI` Library

**Goal:** Refactor the existing `platform_layer` into a distinct, reusable library named `CommanDuctUI`. This will be done by creating a separate Git repository for the library and integrating it back into SourcePacker as a **Git submodule**. This approach allows for rapid, coordinated development while establishing a clean architectural boundary.

## **Phase 1: New Repository Setup and Code Migration**

**Goal:** Create a dedicated home for `CommanDuctUI` and move the platform-specific code into it.

*   **Step 1.1: Create the `CommanDuctUI` Git Repository**
    *   **Action:** On your Git hosting service (e.g., GitHub), create a new, empty repository named `CommanDuctUI`.
    *   **Action:** Clone this empty repository to a location **outside** your SourcePacker project folder.
        ```bash
        git clone <url_to_your_new_CommanDuctUI_repo>
        ```

*   **Step 1.2: Structure the New Library Crate**
    *   **Action:** In the newly cloned `CommanDuctUI` folder, create the basic Rust library structure.
        1.  Copy the entire contents of `source_packer/src/platform_layer/` into `CommanDuctUI/src/`.
        2.  Create `CommanDuctUI/Cargo.toml`. Copy the `[dependencies]` for `windows` and `log` from SourcePacker's `Cargo.toml` into this new file. Define the package as a library.
        3.  Create `CommanDuctUI/src/lib.rs`. Declare all the modules you just copied (e.g., `pub mod app;`, `pub mod types;`) and re-export the public API (e.g., `pub use types::{PlatformCommand, AppEvent};`).

*   **Step 1.3: Clean Up and Initial Commit**
    *   **Action:** Go through the files in `CommanDuctUI/src/` and fix the module paths. For example, change `use crate::platform_layer::types;` to `use crate::types;`.
    *   **Action:** Run `cargo check` inside the `CommanDuctUI` directory to resolve all compilation errors.
    *   **Action:** Once it compiles cleanly, make the first commit to establish a baseline.
        ```bash
        cd CommanDuctUI
        git add .
        git commit -m "feat: Initial migration of platform_layer to CommanDuctUI"
        git push origin main
        ```

## **Phase 2: Submodule Integration into SourcePacker**

**Goal:** Replace the old `platform_layer` directory with the new `CommanDuctUI` submodule.

*   **Step 2.1: Remove the Old `platform_layer`**
    *   **Action:** In your `SourcePacker` repository, **delete the original `src/platform_layer` directory**. This is a critical step to avoid Git conflicts.
        ```bash
        cd /path/to/source_packer
        git rm -r src/platform_layer
        git commit -m "refactor: Remove old platform_layer in preparation for submodule"
        ```

*   **Step 2.2: Add the `CommanDuctUI` Submodule**
    *   **Action:** Add the `CommanDuctUI` repository as a submodule. We will place it in the same location as the old directory to keep paths familiar.
        ```bash
        git submodule add <url_to_your_new_CommanDuctUI_repo> src/platform_layer
        ```
    *   **Result:** This command creates a `.gitmodules` file and a `src/platform_layer` directory that is a clone of your `CommanDuctUI` repository.

*   **Step 2.3: Update SourcePacker's `Cargo.toml`**
    *   **Action:** Edit `source_packer/Cargo.toml`.
    *   **Action:** Add `CommanDuctUI` as a local `path` dependency.
        ```toml
        [dependencies]
        # ... other dependencies
        commanductui = { path = "src/platform_layer" }
        ```

*   **Step 2.4: Update SourcePacker's Code**
    *   **Action:** Throughout the `source_packer` codebase (e.g., in `main.rs`, `app_logic/`, `ui_description_layer/`), change all `use platform_layer::...` statements to `use commanductui::...`.
    *   **Action:** Run `cargo check` in the `source_packer` directory to find and fix any remaining path errors.

*   **Step 2.5: Commit the Integration**
    *   **Action:** Add the changes to your `SourcePacker` repository and commit.
        ```bash
        git add .gitmodules src/platform_layer Cargo.toml src/
        git commit -m "feat: Integrate CommanDuctUI as a submodule"
        git push
        ```

## **Phase 3: Requirement and API Refinement**

**Goal:** Establish `CommanDuctUI` as a true library with its own requirements and a safer, more expressive API.

*   **Step 3.1: Migrate Requirements**
    *   **Action:** Create a new requirements file at `CommanDuctUI/doc/Requirements.md`.
    *   **Action:** Go through `source_packer/doc/Requirements.md` and identify all requirements that are the responsibility of the UI platform layer. These are typically `[Tech...]` and some `[Ui...]` tags related to rendering and native controls.
    *   **Action:** **Move** these requirements from SourcePacker's document to `CommanDuctUI`'s document.
        *   **Example to Move:** `[TechUiFrameworkWindowsRsV1]` belongs in `CommanDuctUI`.
        *   **Example to Keep:** `[UiMenuProfileManagementV2]` stays in `SourcePacker`, as it defines application behavior that *uses* the library's menu capabilities.
    *   **Justification:** This creates a clean "separation of concerns" for requirements. SourcePacker specifies *what* it needs to do, and CommanDuctUI specifies the UI capabilities it *must provide*.

*   **Step 3.2: Implement Type-Safe `ControlId`**
    *   **Goal:** Replace the raw `i32` for control IDs with a type-safe `ControlId` newtype.
    *   **Action:** Follow the plan from **Step 2.1** of the original `Plan.CommanDuctUI.md`.
        1.  Define `pub struct ControlId(pub i32);` in `CommanDuctUI/src/types.rs`.
        2.  Update all `PlatformCommand` and `AppEvent` variants in `CommanDuctUI` to use `ControlId` instead of `i32`.
        3.  Update all internal code within `CommanDuctUI` to use the new type, accessing the raw value with `.0` only for Win32 API calls.
        4.  Run `cargo check` within `CommanDuctUI` to fix all resulting type errors.
        5.  Commit and push these changes from *within the `CommanDuctUI` directory*.
    *   **Action:** In the `SourcePacker` project, run `cargo check`. It will now fail because it's still using `i32`. Update `ui_constants.rs` and any other usage in `app_logic` to use the new `commanductui::ControlId` type.
    *   **Action:** Commit the changes in both repositories (see workflow below).

## **Phase 4: Documentation and Workflow**

**Goal:** Document the new structure and workflow to ensure the project is easy to manage.

*   **Step 4.1: Final Folder Hierarchy**
    Your final project structure will look like this:
    ```
    source_packer/
    ├── .git/
    ├── .gitmodules         <-- Defines the submodule relationship
    ├── Cargo.toml          <-- Depends on `commanductui` via path
    └── src/
        ├── app_logic/
        ├── core/
        ├── main.rs
        └── platform_layer/   <-- This is now the CommanDuctUI submodule
            ├── .git/         <-- It has its own independent Git repository
            ├── Cargo.toml
            └── src/
                ├── app.rs
                ├── types.rs
                └── lib.rs
                └── ... (rest of the library code)
    ```

*   **Step 4.2: Add Developer Workflow Documentation**
    *   **Action:** Create a new document in `source_packer/doc/Workflow.md` or add a section to your main `README.md` explaining how to work with the submodule. It should include the following:

    > ### Submodule Workflow
    >
    > The `src/platform_layer` directory is a Git submodule for our UI library, `CommanDuctUI`.
    >
    > **1. Cloning the Repository for the First Time:**
    > To clone the project and all its submodules, use the `--recurse-submodules` flag:
    > ```bash
    > git clone --recurse-submodules <source_packer_repo_url>
    > ```    > If you already cloned without it, run this to initialize the submodule:
    > ```bash
    > git submodule update --init --recursive
    > ```
    >
    > **2. Making Changes to `CommanDuctUI`:**
    > This is a two-step process:
    > 1.  Make your code changes inside `src/platform_layer`.
    > 2.  Commit and push those changes *from within the submodule directory*:
    >     ```bash
    >     cd src/platform_layer
    >     git add .
    >     git commit -m "feat: Add new feature to CommanDuctUI"
    >     git push
    >     ```
    > 3.  Go back to the main `SourcePacker` project. `git status` will show `src/platform_layer` as modified. This is Git noting that the submodule's commit pointer has changed.
    > 4.  Commit this pointer update in the main project:
    >     ```bash
    >     cd ../..
    >     git add src/platform_layer
    >     git commit -m "chore: Update CommanDuctUI to latest commit"
    >     git push
    >     ```
    >
    > **3. Pulling Updates from Remote:**
    > When you pull changes in the `SourcePacker` repository, you also need to update the submodule to match the new commit pointer:
    > ```bash
    > git pull
    > git submodule update --recursive --remote
    > ```

## **Phase 5: Future Transition to a Published Crate**

**Goal:** Outline the path to making `CommanDuctUI` a fully independent, versioned library on `crates.io`.

*   **Future Action:** Once the `CommanDuctUI` API is stable and mature, the transition is simple:
    1.  Ensure the `CommanDuctUI/Cargo.toml` has all necessary metadata (license, description, repository, etc.).
    2.  From within the `CommanDuctUI` directory, run `cargo publish`.
    3.  In `source_packer/Cargo.toml`, change the dependency from `path = "..."` to `version = "0.1.0"` (or the published version).
    4.  The submodule can then be removed if desired, or kept for development of future versions.
