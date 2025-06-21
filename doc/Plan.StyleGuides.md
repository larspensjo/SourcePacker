# Refactored Plan: A Comprehensive Styling and Theming System

**Goal:** To refactor the UI system to support a flexible, ID-based styling mechanism. This will decouple style definitions from control creation, enabling the implementation of themes like "Neon Night" and making future look-and-feel changes easier. The system will be built in small, verifiable increments where the application remains functional after each phase.

**Legend:**
*   **(PL-Types):** Changes in `platform_layer/types.rs` or a new `platform_layer/styling.rs`.
*   **(PL-State):** Changes in `platform_layer/app.rs` or `platform_layer/window_common.rs`.
*   **(PL-Exec):** Changes in `platform_layer/command_executor.rs` or control handlers.
*   **(PL-WndProc):** Changes in `platform_layer/window_common.rs` (message handling).
*   **(UIDL):** Changes in `ui_description_layer.rs` or a new `ui_description_layer/theme.rs`.

---

## Phase 1: Foundational Types for a Definable Style System

**Objective:** Establish the core data structures for defining styles and the commands to manage them. This phase involves no visual changes but lays the entire groundwork.

1.  **Create a New Module for Styling (Recommended):**
    *   **Action:** Create `src/platform_layer/styling.rs` to house all style-related definitions.
    *   **Action:** Add `pub mod styling;` to `src/platform_layer/mod.rs` and `pub use styling::*;` to re-export its contents.

2.  **(PL-Types) Define Core Styling Primitives:**
    *   **File:** `platform_layer/styling.rs`
    *   **Action:** Define the basic building blocks for styles.
        ```rust
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct Color { pub r: u8, pub g: u8, pub b: u8 }

        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub enum FontWeight { #[default] Normal, Bold }

        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct FontDescription {
            pub name: Option<String>,
            pub size: Option<i32>,
            pub weight: Option<FontWeight>,
            // italic, underline, etc. can be added here
        }
        ```

3.  **(PL-Types) Define the Master `ControlStyle` Struct:**
    *   **File:** `platform_layer/styling.rs`
    *   **Action:** Define the struct that holds all possible style properties. We will start simple and add more properties in later phases.
        ```rust
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct ControlStyle {
            pub font: Option<FontDescription>,
            pub text_color: Option<Color>,
            pub background_color: Option<Color>,
            // Properties for border, hover, etc., will be added later.
        }
        ```

4.  **(PL-Types) Define `StyleId` Enum:**
    *   **File:** `platform_layer/styling.rs`
    *   **Action:** Create an enum to act as the unique identifier for each defined style. This is the equivalent of `x:Key` in WPF.
        ```rust
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum StyleId {
            // General Controls
            DefaultText,
            DefaultButton,
            DefaultInput,
            // Panels & Regions
            MainWindowBackground,
            PanelBackground,
            StatusBarBackground,
            // Specific elements
            StatusLabelNormal,
            StatusLabelWarning,
            StatusLabelError,
        }
        ```

5.  **(PL-Types) Add New `PlatformCommand` Variants:**
    *   **File:** `platform_layer/types.rs`
    *   **Action:** Add the two key commands for our new system into the `PlatformCommand` enum.
        ```rust
        // ... existing commands
        DefineStyle {
            style_id: StyleId,
            style: ControlStyle,
        },
        ApplyStyleToControl {
            window_id: WindowId,
            control_id: i32,
            style_id: StyleId,
        },
        ```

6.  **(PL-State) Define the Internal `ParsedControlStyle`:**
    *   **File:** `platform_layer/styling.rs`
    *   **Action:** Create a struct for the `platform_layer`'s internal use, which will hold processed data like native `HFONT` handles. This encapsulates Win32 types.
        ```rust
        use windows::Win32::Graphics::Gdi::{HFONT, DeleteObject};

        #[derive(Debug, Clone)]
        pub(crate) struct ParsedControlStyle {
            pub(crate) font_handle: Option<HFONT>,
            pub(crate) text_color: Option<Color>,
            pub(crate) background_color: Option<Color>,
            // ... other parsed properties ...
        }

        impl Drop for ParsedControlStyle { // Handles HFONT cleanup
            fn drop(&mut self) {
                if let Some(hfont) = self.font_handle.take() {
                    if !hfont.is_invalid() {
                        unsafe { DeleteObject(hfont); }
                    }
                }
            }
        }
        ```

**Verification:** The application compiles successfully. No functional changes are expected.

---

## Phase 2: Backend - Implementing `DefineStyle`

**Objective:** Implement the logic that allows the platform layer to receive style definitions and store them in a parsed, ready-to-use format. Still no visual changes.

1.  **(PL-State) Add Storage for Defined Styles:**
    *   **File:** `platform_layer/app.rs`
    *   **Action:** In `Win32ApiInternalState`, add a map to store the defined styles.
        ```rust
        // in Win32ApiInternalState
        defined_styles: RwLock<HashMap<StyleId, ParsedControlStyle>>,
        ```

2.  **(PL-Exec) Implement `execute_define_style`:**
    *   **Action:** Create a function to handle the `DefineStyle` command (e.g., in `command_executor.rs` or a new `styling_handler.rs`). It will:
        *   Parse the incoming `ControlStyle` (e.g., create an `HFONT` from a `FontDescription`).
        *   Create a `ParsedControlStyle` instance.
        *   Store it in the `defined_styles` map in `Win32ApiInternalState`, replacing any existing style with the same ID.

**Verification:** The application compiles. Unit tests can be written to verify that calling `DefineStyle` results in a correctly parsed style being added to the internal state map.

---

## Phase 3: Backend - Implementing `ApplyStyleToControl` and Basic Rendering

**Objective:** Make the styles appear on screen. This is the first phase with a visible result, focusing on fonts and colors.

1.  **(PL-State) Add Storage for Applied Styles:**
    *   **File:** `platform_layer/window_common.rs`
    *   **Action:** In `NativeWindowData`, add a map to track which style is applied to which control.
        ```rust
        // in NativeWindowData
        applied_styles: HashMap<i32, StyleId>,
        ```

2.  **(PL-Exec) Implement `execute_apply_style_to_control`:**
    *   **Action:** Create a function to handle `ApplyStyleToControl`. It will:
        *   Store the `control_id -> style_id` mapping in the window's `NativeWindowData`.
        *   Look up the `ParsedControlStyle` from the global `defined_styles` map.
        *   If a font is present in the style, send a `WM_SETFONT` message to the control's `HWND`.
        *   Call `InvalidateRect` on the control to force it to repaint with the new style properties.

3.  **(PL-WndProc) Modify `WndProc` for Color Handling:**
    *   **File:** `platform_layer/window_common.rs` (or relevant control handlers).
    *   **Action:** Modify the handlers for `WM_CTLCOLORSTATIC`, `WM_CTLCOLOREDIT`, etc.
        *   When a message arrives, get the `control_id` from the `HWND`.
        *   Look up the `StyleId` in the `applied_styles` map for that control.
        *   If found, get the `ParsedControlStyle` from the global map.
        *   Use the `text_color` and `background_color` from the parsed style to set the text color and return the appropriate background brush.
        *   This will **replace** the existing hardcoded `MessageSeverity` logic in `label_handler.rs`.

**Verification:** At the end of this phase, you can define a simple style in the UIDL, apply it to a label, and it should appear with the specified font and colors.

---

## Phase 4: UI Descriptive Layer Integration and Theming

**Objective:** Define the entire "Neon Night" theme using the new styling system.

1.  **Create a Theme Definition Module:**
    *   **Action:** Create a new file: `src/ui_description_layer/theme.rs`.
    *   **Action:** Create a function `pub fn define_neon_night_theme() -> Vec<PlatformCommand>`. This function will return a list of `PlatformCommand::DefineStyle` commands for every `StyleId`.
    *   **Rationale:** This separates theme definition (colors, fonts) from UI layout (which controls exist and where).

2.  **(UIDL) Define the "Neon Night" Styles:**
    *   **File:** `ui_description_layer/theme.rs`
    *   **Action:** Translate the colors from `DarkTheme.xaml` into `Color { r, g, b }` structs in Rust.
    *   **Action:** Implement `define_neon_night_theme()` to create `DefineStyle` commands for all your `StyleId`s (`DefaultButton`, `PanelBackground`, etc.), setting their `ControlStyle` properties.

3.  **(UIDL) Apply Styles to Controls:**
    *   **File:** `ui_description_layer.rs`
    *   **Action:** In `build_main_window_static_layout`:
        *   First, call `theme::define_neon_night_theme()` and add all the returned commands to the command list.
        *   After each `CreateButton`, `CreatePanel`, etc. command, add a `PlatformCommand::ApplyStyleToControl` command to apply the desired `StyleId` to that control.

**Verification:** Relaunch the application. The entire UI should now have the basic colors and fonts of the Neon Night theme.

---

## Phase 5: Advanced Styling - Borders and Focus Effects

**Objective:** Implement custom borders and the focus "glow" effect, which are crucial for the Neon Night theme's aesthetic.

1.  **(PL-Types) Extend `ControlStyle`:**
    *   **File:** `platform_layer/styling.rs`
    *   **Action:** Add border and focus properties to `ControlStyle` and `ParsedControlStyle`.
        ```rust
        // in ControlStyle
        pub border_color: Option<Color>,
        pub border_thickness: Option<u32>,
        pub focus_border_color: Option<Color>, // Used to simulate the "glow"
        ```

2.  **(PL-Exec & PL-WndProc) Implement Border and Focus Drawing:**
    *   **The Challenge:** True glow effects are difficult in native Win32. We will simulate it by changing the border color on focus.
    *   **Recommended Approach:** Handle `WM_NCPAINT` (non-client area paint). This is the most robust way to draw custom borders around standard controls.
    *   **Action (WM_NCPAINT):**
        *   When a control with a custom border style gets this message, call the default window procedure first.
        *   Then, get the window's `RECT` and a device context (`HDC`) for the non-client area.
        *   Check if the control is focused (`GetFocus() == hwnd`).
        *   Select the `focus_border_color` if focused and the style defines it; otherwise, use the normal `border_color`.
        *   Draw a frame around the control using this color.
    *   **Action (Focus Handling):**
        *   In the main `WndProc`, handle `WM_SETFOCUS` and `WM_KILLFOCUS`. When a control receives or loses focus, call `RedrawWindow` with the `RDW_FRAME` flag to force a `WM_NCPAINT` message, triggering the border color change.

3.  **(UIDL) Update Theme Definition:**
    *   **Action:** Add border and focus color properties to your style definitions in `ui_description_layer/theme.rs`.

**Verification:** Panels and controls should have custom-colored borders. When you tab to a `TextBox` or `Button`, its border color should change to the accent color, simulating the focus glow.

---

## Phase 6: UI Enhancement - Tooltip Support

**Objective:** Implement styled tooltips, reusing the logic from the previous plan but integrating it with the new theme.

1.  **(PL-Types) Add Tooltip Command:**
    *   **File:** `platform_layer/types.rs`
    *   **Action:** Add `SetControlTooltip { window_id: WindowId, control_id: i32, tooltip_text: Option<String> }`.

2.  **(PL-Exec & PL-State) Implement Tooltip Logic:**
    *   **Action:** Implement the logic to create a tooltip control (`TOOLTIPS_CLASSW`) per window and use `TTM_ADDTOOLW` to associate text with a control's `HWND`.
    *   **Styling:** To style the tooltip itself, it must be created with the `TTS_OWNERDRAW` style. This requires handling `WM_DRAWITEM` for the tooltip, which is an advanced step.
    *   **Simpler Alternative:** A simpler first step is to accept the default system tooltip appearance. Full custom styling can be a follow-on task.

3.  **(UIDL) Add Tooltips:**
    *   **Action:** After creating a button or other control, send a `SetControlTooltip` command.

**Verification:** Hovering over a control with a configured tooltip displays the text.

---

## Phase 7: Review and Further Enhancements

1.  **Comprehensive Testing:** Test all styling and tooltip features in combination.
2.  **Performance Review:** Assess the performance impact of custom drawing, especially `WM_NCPAINT` handlers.
3.  **API and `StyleId` Refinement:** Based on usage, adjust `StyleId`s for clarity and add more `ControlStyle` properties if needed (e.g., hover-state colors).
4.  **Consider Hover Effects:** Implement hover state changes by handling `WM_MOUSEMOVE` and `WM_MOUSELEAVE` (`TrackMouseEvent`) to trigger repaints, similar to the focus handling logic.
