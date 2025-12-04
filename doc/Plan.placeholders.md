This is a strategic feature that addresses a very common pain point in AI-assisted development: context window exhaustion. By treating subfolders as "black boxes" described by a contract (the Markdown summary), you significantly increase the density of high-value information in your prompt.

Here is a brainstorming and architectural breakdown for implementing **Folder Summaries (Placeholders)** in SourcePacker.

---

### 1. Architectural Impact

#### The Data Model (`core`)
Currently, `FileNode` has a `SelectionState` (Selected, Deselected, New). We need to introduce a concept of "Summarized".

*   **Option A: New SelectionState.** Add `SelectionState::Summarized`.
    *   *Pros:* Simple state machine. A folder is either fully included, excluded, or summarized.
    *   *Cons:* Implicitly assumes a summary file exists.
*   **Option B: Separate "Inclusion Mode".** Keep `Selected` (meaning "part of the output") but add a metadata field to `Profile` that maps `PathBuf -> InclusionMode (FullContent | Summary)`.
    *   *Recommendation:* **Option A** is likely cleaner for your current recursive archiver logic. If a node is `Summarized`, the recursion stops, and the summary is emitted.

#### Profile Persistence
The `Profile` struct in `src/core/file_node.rs` needs to track which folders are placeholders.
*   We should add a `summary_paths: HashSet<PathBuf>` similar to `selected_paths`.
*   **Migration:** Old profiles won't have this field. `#[serde(default)]` handles backward compatibility easily.

#### Storage Strategy
You suggested `.sourcepacker`. This is the right place.
*   **Location:** `<ProjectRoot>/.sourcepacker/summaries/`
*   **Naming Strategy:**
    1.  **Mirror Structure:** If you summarize `src/ui/controls`, the placeholder is at `.sourcepacker/summaries/src/ui/controls.md`.
        *   *Pro:* Human readable, easy to edit externally.
        *   *Con:* Deep paths could hit OS limits (rare but possible on Windows).
    2.  **Flat Hash:** `sha256(rel_path).md`.
        *   *Pro:* Flat directory.
        *   *Con:* Hard for humans (and external tools) to identify which file maps to which folder without a lookup table.
    *   *Recommendation:* **Mirror Structure**. Since SourcePacker is for developers, having a readable file structure for documentation is valuable.

---

### 2. User Experience (UX)

#### Visual Indication
The user needs to know *at a glance* if a folder is full-content or summary-only.
*   **Current UI:** Checkboxes and text.
*   **Proposal:** Use a distinct visual style for summarized folders.
    *   **Icon/Glyph:** Prefix the text with a book icon or similar (e.g., `📖 src`).
    *   **Color:** Use the styling system (`StyleId`) to render summarized folder names in a distinct color (e.g., Cyan or Purple in the "Neon Night" theme).

#### Interaction Flow
How does a user toggle this?
1.  **Selection:** User selects a folder in the TreeView.
2.  **Action:** A new button or menu item "Toggle Summary Mode".
    *   *If Normal:* Checks for existing summary. If missing, creates a template. Sets state to `Summarized`.
    *   *If Summarized:* Sets state back to `Selected` (Full).
3.  **External Editing:** Since generation is external, SourcePacker could offer a button "Edit Summary" which launches the default system editor for the `.md` file.

---

### 3. CommanDuctUI Extensions

To support this cleanly without breaking changes, `CommanDuctUI` needs minor enhancements:

1.  **Expanded Visual States:**
    Currently, `TreeItemDescriptor` has `CheckState`. You might need `CheckState::Partial` or a generic `icon_id`.
    *   *Low Impact:* repurpose `CheckState::Indeterminate` (if Windows supports it on TreeViews, which usually implies "some children selected").
    *   *Better:* Add `style_override: Option<StyleId>` to `TreeItemDescriptor`. This allows the App Logic to tell the Platform Layer: "Render this specific tree item using `StyleId::SummaryItem`". This leverages your existing `NM_CUSTOMDRAW` logic in `treeview_handler.rs`.

2.  **Context Menus (Right Click):**
    While you have buttons, a right-click context menu on a TreeView item is the standard pattern for "Treat this folder differently."
    *   *Extension:* Handle `NM_RCLICK` in `treeview_handler.rs`. Send an `AppEvent::TreeViewContextMenuRequested { item_id, screen_x, screen_y }`.
    *   *App Logic:* Responds by sending a `ShowContextMenu` command.

---

### 4. Archive Generation Logic (`CoreArchiver`)

The `ArchiverOperations::create_content` method needs modification:

```rust
// Pseudo-code logic flow
fn create_content(...) {
    for node in nodes {
        if node.state == SelectionState::Summarized {
            // 1. Resolve path to summary file in .sourcepacker/summaries
            // 2. Read that markdown file
            // 3. Emit Header: "--- SUMMARY: src/ui (Content Omitted) ---"
            // 4. Emit Markdown content
            // 5. Emit Footer
            // 6. Do NOT recurse into node.children
        } else if node.is_selected() {
            // Standard behavior
        }
    }
}
```

---

### 5. Robustness & Edge Cases

1.  **Missing Summary:** What if a folder is marked `Summarized` in the profile, but the `.md` file is deleted?
    *   *Solution:* Fallback behavior. During archive generation, insert a warning in the text file: `[WARNING: Summary file for 'src/ui' missing. Folder content omitted.]`. Or, automatically revert to full scan (risky for context window).
2.  **Stale Summaries:** The code in the folder changes, but the summary doesn't.
    *   *Idea:* Compare the modification timestamp of the `.md` file vs the latest modification timestamp of any file inside the summarized folder.
    *   *UI:* Show a warning icon (like the "New" dot, maybe an amber warning) on the folder if `Summary_Mod_Time < Folder_Max_Mod_Time`.
3.  **Empty Folders:** Can an empty folder be summarized? Yes, it explains why it exists.

---

### 6. Testing Strategy

1.  **Unit Test (`Archiver`):**
    *   Create a mock file system with a folder `data/` and a summary `summaries/data.md`.
    *   Set up a `FileNode` for `data` with `SelectionState::Summarized`.
    *   Run `create_content`.
    *   Assert that files *inside* `data/` are NOT in the output, but the content of `summaries/data.md` IS.

2.  **Integration Test (`Profile`):**
    *   Save a profile with summary paths.
    *   Load it back.
    *   Ensure the `SelectionState` is preserved.

---

### 7. Future Extensions (Brainstorming)

*   **Auto-Generation Hook:** Even if the generation is "external", SourcePacker could allow configuring a command line string (e.g., `python gen_summary.py {target_path} {output_path}`). SourcePacker runs this when you click "Update Summary".
*   **Inline Editing:** Use the `ID_VIEWER_EDIT_CTRL` (currently read-only) to allow editing the Markdown summary directly inside SourcePacker when a summarized folder is selected.
*   **Token Budgeting:** Since you care about context windows, the UI could show: "Current Folder: 50k tokens. Summary: 500 tokens. Savings: 99%".

### Next Steps Recommendation

1.  **Phase 1 (Core):** Implement the `SelectionState::Summarized`, update `Profile` serialization, and implement the "Mirror Structure" storage logic in a new `SummaryManager` core module.
2.  **Phase 2 (UI - Read):** Update `CommanDuctUI` to support per-item styling in TreeView. Update `MyAppLogic` to render summarized nodes in a different color.
3.  **Phase 3 (UI - Write):** Add the button/logic to toggle state and create the default placeholder file if missing.
4.  **Phase 4 (Archiver):** Update the archiver to consume the summary instead of recursion.
