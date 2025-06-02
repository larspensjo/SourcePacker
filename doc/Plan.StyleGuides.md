# Step-by-Step Plan: Implementing Defined Styles

**Goal:** Implement a styling system where styles are defined with an ID and then applied to controls. This facilitates reusability, easier theme management, and allows future style extensions with minimal changes to the `ui_description_layer`. This plan assumes the "UI Descriptive Layer plan" is completed.

**Legend:**
*   **(PL-Types):** Changes in `platform_layer/types.rs` (or a new `platform_layer/styling.rs`)
*   **(PL-State):** Changes in `platform_layer/app.rs` (Win32ApiInternalState) or `platform_layer/window_common.rs` (NativeWindowData)
*   **(PL-Exec):** Changes in `platform_layer/command_executor.rs`
*   **(PL-WndProc):** Changes in `platform_layer/window_common.rs` (WndProc message handling)
*   **(UIDL):** Changes in `ui_description_layer/mod.rs`
*   **(AppLogic):** Potential future changes in `app_logic/handler.rs` for dynamic styling

---

## Phase 1: Define Core Styling Types and Commands

**Objective:** Establish the necessary structs for describing styles and the `PlatformCommand`s to manage them.

1.  **Create `platform_layer/styling.rs` (Optional but Recommended):**
    *   **Action:** Create a new file `src/platform_layer/styling.rs`.
    *   **Action:** Add `pub mod styling;` to `src/platform_layer/mod.rs` and `pub use styling::*;` to re-export.
    *   **Rationale:** Keeps styling-related type definitions organized.

2.  **(PL-Types) Define Basic Styling Primitives:**
    *   **File:** `platform_layer/styling.rs` (or `platform_layer/types.rs` if not using a separate file).
    *   **Action:** Define `Color`, `FontWeight`, and `FontDescription` structs.
        ```rust
        #[derive(Debug, Clone, Default, PartialEq, Eq)] // Eq if all fields are Eq
        pub struct Color {
            pub r: u8,
            pub g: u8,
            pub b: u8,
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub enum FontWeight {
            #[default]
            Normal,
            Bold,
            // Consider others like Light, SemiBold later if needed
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct FontDescription {
            pub name: Option<String>, // e.g., "Segoe UI"
            pub size: Option<i32>,    // In points or logical units
            pub weight: Option<FontWeight>,
            pub italic: Option<bool>,
            pub underline: Option<bool>,
            pub strikeout: Option<bool>,
        }
        ```

3.  **(PL-Types) Define `ControlStyle` Struct:**
    *   **File:** `platform_layer/styling.rs` (or `types.rs`).
    *   **Action:** Define the main `ControlStyle` struct that aggregates various style aspects.
        ```rust
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct ControlStyle {
            pub font: Option<FontDescription>,
            pub text_color: Option<Color>,
            pub background_color: Option<Color>,
            // Add other properties like border_style, border_color later
        }
        ```

4.  **(PL-Types) Define `StyleId` Enum:**
    *   **File:** `platform_layer/styling.rs` (or `types.rs`).
    *   **Action:** Define an enum for named styles. These will be used by `ui_description_layer`.
        ```rust
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum StyleId {
            // General Purpose
            DefaultText,
            DefaultButton,
            DefaultPanel,

            // Specific Labels (examples)
            StatusLabelNormal,
            StatusLabelWarning,
            StatusLabelError,
            StatusLabelArchive,
            StatusLabelTokens,

            // TreeView
            TreeViewDefault,
            // Add more as needed
        }
        ```

5.  **(PL-Types) Add New `PlatformCommand` Variants:**
    *   **File:** `platform_layer/types.rs` (in `PlatformCommand` enum).
    *   **Action:** Add commands for defining and applying styles.
        ```rust
        // ... existing commands
        DefineStyle {
            style_id: StyleId, // Use the new enum
            style_definition: ControlStyle,
        },
        ApplyStyleToControl {
            window_id: WindowId, // Target window for the control
            control_id: i32,     // Logical ID of the control
            style_id: StyleId,   // Style to apply
        },
        ```

6.  **(PL-State) Define `ParsedControlStyle` (Internal):**
    *   **File:** `platform_layer/styling.rs` or `platform_layer/app.rs` (where `Win32ApiInternalState` is).
    *   **Action:** Define a struct for the platform layer's internal representation of a style, holding GDI resources.
        ```rust
        // In platform_layer/styling.rs (or app.rs if kept internal to Win32ApiInternalState module)
        use windows::Win32::Graphics::Gdi::HFONT; // Add necessary import

        #[derive(Debug, Clone)] // HFONT is not Clone by default, handle carefully
        pub(crate) struct ParsedControlStyle {
            pub(crate) font_handle: Option<HFONT>, // Native font handle
            pub(crate) text_color: Option<Color>,
            pub(crate) background_color: Option<Color>,
            // Original FontDescription kept for re-creation if needed, or for comparison
            pub(crate) original_font_desc: Option<FontDescription>,
        }

        impl Drop for ParsedControlStyle {
            fn drop(&mut self) {
                if let Some(hfont) = self.font_handle.take() {
                    if !hfont.is_invalid() {
                        unsafe {
                            // Important: Ensure DeleteObject is correctly used for HFONT
                            windows::Win32::Graphics::Gdi::DeleteObject(hfont);
                        }
                        log::trace!("Dropped HFONT {:?}", hfont);
                    }
                }
            }
        }
        ```
    *   **Note:** `HFONT` cannot be simply cloned. If `ParsedControlStyle` needs to be `Clone`, `font_handle` might need to be an `Arc<HFONTWrapper>` or handled differently. For now, if it's only stored and replaced, `Option<HFONT>` with manual `Drop` is okay.

---

## Phase 2: `platform_layer` - Implement `DefineStyle`

**Objective:** Enable the platform layer to receive style definitions, create GDI resources (like fonts), and store them.

1.  **(PL-State) Add Storage for Defined Styles:**
    *   **File:** `platform_layer/app.rs` (in `Win32ApiInternalState` struct).
    *   **Action:** Add a field to store the defined styles.
        ```rust
        // ... existing fields
        pub(crate) defined_styles: RwLock<HashMap<StyleId, ParsedControlStyle>>,
        ```
    *   **Action:** Initialize `defined_styles: RwLock::new(HashMap::new())` in `Win32ApiInternalState::new()`.

2.  **(PL-Exec) Implement `execute_define_style`:**
    *   **File:** `platform_layer/command_executor.rs`.
    *   **Action:** Create the function to handle `PlatformCommand::DefineStyle`.
        ```rust
        use crate::platform_layer::styling::{StyleId, ControlStyle, ParsedControlStyle, FontDescription, FontWeight, Color as StyleColor}; // Assuming Color is aliased
        use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, DeleteObject, HFONT, LOGFONTW, FW_NORMAL, FW_BOLD /*, other FW_ constants */};
        use windows::core::HSTRING;
        // ... other imports

        pub(crate) fn execute_define_style(
            internal_state: &Arc<Win32ApiInternalState>,
            style_id: StyleId,
            style_definition: ControlStyle,
        ) -> PlatformResult<()> {
            log::debug!("CommandExecutor: execute_define_style for StyleID: {:?}", style_id);
            let mut new_font_handle: Option<HFONT> = None;
            let mut original_font_desc_for_parsed_style: Option<FontDescription> = None;

            if let Some(font_desc) = &style_definition.font {
                original_font_desc_for_parsed_style = Some(font_desc.clone());
                let mut lf = LOGFONTW::default();
                if let Some(name) = &font_desc.name {
                    let face_name_hstring = HSTRING::from(name.as_str());
                    let face_name_wide: Vec<u16> = face_name_hstring.as_wide().to_vec();
                    // Ensure null termination for lpszFaceName if not already handled by as_wide() followed by copy.
                    // LOGFONTW.lfFaceName is [u16; 32]
                    let len_to_copy = std::cmp::min(face_name_wide.len(), lf.lfFaceName.len() -1);
                    lf.lfFaceName[..len_to_copy].copy_from_slice(&face_name_wide[..len_to_copy]);
                    if len_to_copy < lf.lfFaceName.len() { // Ensure null termination
                        lf.lfFaceName[len_to_copy] = 0;
                    }
                } else {
                    // Default font name if needed, or rely on system default
                    let default_face_name = HSTRING::from("Segoe UI"); // Example default
                     let face_name_wide: Vec<u16> = default_face_name.as_wide().to_vec();
                    let len_to_copy = std::cmp::min(face_name_wide.len(), lf.lfFaceName.len() -1);
                    lf.lfFaceName[..len_to_copy].copy_from_slice(&face_name_wide[..len_to_copy]);
                     if len_to_copy < lf.lfFaceName.len() { lf.lfFaceName[len_to_copy] = 0; }
                }

                lf.lfHeight = font_desc.size.map_or(-12, |s| -s); // Example: -points for height
                lf.lfWeight = match font_desc.weight {
                    Some(FontWeight::Bold) => FW_BOLD.0,
                    _ => FW_NORMAL.0,
                };
                lf.lfItalic = font_desc.italic.unwrap_or(false) as u8;
                lf.lfUnderline = font_desc.underline.unwrap_or(false) as u8;
                lf.lfStrikeOut = font_desc.strikeout.unwrap_or(false) as u8;
                // Set other LOGFONT fields as needed (e.g., lfCharSet)

                unsafe {
                    let hfont = CreateFontIndirectW(&lf);
                    if hfont.is_invalid() {
                        log::error!("Failed to create font for style {:?}: {:?}", style_id, windows::core::Error::from_win32());
                        // Optionally return an error or proceed without a font
                    } else {
                        new_font_handle = Some(hfont);
                    }
                }
            }

            let parsed_style = ParsedControlStyle {
                font_handle: new_font_handle,
                text_color: style_definition.text_color.clone(),
                background_color: style_definition.background_color.clone(),
                original_font_desc: original_font_desc_for_parsed_style,
            };

            let mut styles_guard = internal_state.defined_styles.write().map_err(|_| {
                PlatformError::OperationFailed("Failed to lock defined_styles for writing".into())
            })?;

            // If replacing an existing style, the old HFONT in ParsedControlStyle's Drop will be cleaned.
            let old_style = styles_guard.insert(style_id, parsed_style);
            if old_style.is_some() {
                log::debug!("Replaced existing style definition for {:?}", style_id);
            }

            Ok(())
        }
        ```
    *   **Action:** Add the call to `execute_define_style` in `Win32ApiInternalState::_execute_platform_command`.

3.  **(PL-State) Implement `Drop` for `Win32ApiInternalState` (or ensure `defined_styles` is handled):**
    *   **File:** `platform_layer/app.rs`.
    *   **Action:** The `Drop` impl for `ParsedControlStyle` should handle `DeleteObject` for its `HFONT`. When `Win32ApiInternalState` is dropped, its `defined_styles` `HashMap` will be dropped, and each `ParsedControlStyle` within it will also be dropped, triggering their `HFONT` cleanup. Ensure this chain of `Drop` calls works.

---

## Phase 3: `platform_layer` - Implement `ApplyStyleToControl` and Rendering

**Objective:** Enable applying defined styles to controls and ensure they render correctly.

1.  **(PL-State) Add Storage for Applied Styles per Control:**
    *   **File:** `platform_layer/window_common.rs` (in `NativeWindowData` struct).
    *   **Action:** Add a field to map `control_id` to `StyleId`.
        ```rust
        // ... existing fields
        pub(crate) applied_styles: HashMap<i32, StyleId>,
        ```
    *   **Action:** Initialize `applied_styles: HashMap::new()` in `NativeWindowData`'s creation.

2.  **(PL-Exec) Implement `execute_apply_style_to_control`:**
    *   **File:** `platform_layer/command_executor.rs`.
    *   **Action:** Create the function to handle `PlatformCommand::ApplyStyleToControl`.
        ```rust
        use windows::Win32::UI::WindowsAndMessaging::{InvalidateRect, WM_SETFONT};
        // ... other imports

        pub(crate) fn execute_apply_style_to_control(
            internal_state: &Arc<Win32ApiInternalState>,
            window_id: WindowId,
            control_id: i32,
            style_id: StyleId,
        ) -> PlatformResult<()> {
            log::debug!("CommandExecutor: execute_apply_style_to_control for WinID: {:?}, ControlID: {}, StyleID: {:?}", window_id, control_id, style_id);

            let control_hwnd: HWND;
            let font_to_apply: Option<HFONT>;

            // Scope for read lock on defined_styles
            {
                let styles_guard = internal_state.defined_styles.read().map_err(|_| {
                    PlatformError::OperationFailed("Failed to lock defined_styles for reading".into())
                })?;
                let parsed_style = styles_guard.get(&style_id).ok_or_else(|| {
                    PlatformError::OperationFailed(format!("StyleId {:?} not defined", style_id))
                })?;
                font_to_apply = parsed_style.font_handle; // HFONT is Copy
            }

            // Scope for write lock on active_windows
            {
                let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
                    PlatformError::OperationFailed("Failed to lock active_windows for writing".into())
                })?;
                let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
                    PlatformError::InvalidHandle(format!("WindowId {:?} not found", window_id))
                })?;

                control_hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
                    PlatformError::InvalidHandle(format!("ControlId {} not found in window {:?}", control_id, window_id))
                })?;

                if control_hwnd.is_invalid() {
                     return Err(PlatformError::InvalidHandle(format!("ControlId {} HWND is invalid in window {:?}", control_id, window_id)));
                }
                window_data.applied_styles.insert(control_id, style_id);
            } // Write lock released

            // Apply font if one is defined for the style
            if let Some(hfont) = font_to_apply {
                if !hfont.is_invalid() {
                    unsafe {
                        SendMessageW(control_hwnd, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1)); // LPARAM(1) to redraw
                    }
                }
            } else {
                // If the style has no font, or font creation failed, apply system default font
                unsafe {
                     SendMessageW(control_hwnd, WM_SETFONT, WPARAM(0), LPARAM(1)); // WPARAM(0) for system font
                }
            }

            // Invalidate control to trigger WM_CTLCOLOR* for color changes
            unsafe { InvalidateRect(Some(control_hwnd), None, true); }
            Ok(())
        }
        ```
    *   **Action:** Add the call to `execute_apply_style_to_control` in `Win32ApiInternalState::_execute_platform_command`.

3.  **(PL-WndProc) Modify `WM_CTLCOLORSTATIC` (and others like `WM_CTLCOLORBTN` as needed):**
    *   **File:** `platform_layer/window_common.rs` (in `Win32ApiInternalState::handle_window_message`).
    *   **Action:** Update the handler to use the new styling system.
        ```rust
        // Inside WM_CTLCOLORSTATIC handler:
        // ...
        let control_id_of_static = unsafe { GetDlgCtrlID(hwnd_static_ctrl_from_msg) };
        let mut style_to_apply: Option<&ParsedControlStyle> = None; // Use a temporary borrow

        if let Some(window_data) = windows_map_guard.get(&window_id) { // Assuming windows_map_guard is a read lock
            if let Some(applied_style_id) = window_data.applied_styles.get(&control_id_of_static) {
                // Need to access internal_state.defined_styles here.
                // This might require passing internal_state to handle_wm_ctlcolorstatic or restructuring.
                // For simplicity here, assume internal_state.defined_styles can be read:
                if let Ok(defined_styles_guard) = self.defined_styles.read() { // 'self' refers to Win32ApiInternalState
                    style_to_apply = defined_styles_guard.get(applied_style_id);
                }
            }
        }
        // Drop windows_map_guard before potentially calling DefWindowProcW

        if let Some(parsed_style) = style_to_apply {
            if let Some(text_color) = &parsed_style.text_color {
                unsafe { SetTextColor(hdc_static_ctrl, windows::Win32::Foundation::COLORREF(RGB(text_color.r, text_color.g, text_color.b))); }
            }
            // For background color:
            // If parsed_style.background_color is Some, create/get a brush and return its handle.
            // Otherwise, SetBkMode(TRANSPARENT) and return GetStockObject(NULL_BRUSH) or default background.
            // This is more complex for generic controls. For labels, transparency often works.
            unsafe {
                SetBkMode(hdc_static_ctrl, TRANSPARENT.0); // Example for labels
                lresult_override = Some(LRESULT(GetStockObject(NULL_BRUSH).0 as isize)); // Example
            }
            handled = true;
        } else {
            // Fallback to old severity-based logic for labels IF no explicit style is applied,
            // or remove old logic if styles are mandatory.
            // For now, assume if no style, use default processing.
            // handled = false; // This would cause it to fall through
        }
        // ...
        ```
    *   **Refinement:** The `WM_CTLCOLOR*` logic needs careful access to both `NativeWindowData` (for the applied `StyleId`) and `Win32ApiInternalState` (for the `ParsedControlStyle` definition). Ensure locks are managed correctly to avoid deadlocks or holding locks for too long. Consider helper functions on `Win32ApiInternalState` or `NativeWindowData` to encapsulate this lookup.

---

## Phase 4: `ui_description_layer` Integration

**Objective:** Modify the `ui_description_layer` to define and apply styles.

1.  **(UIDL) Define Styles:**
    *   **File:** `ui_description_layer/mod.rs`.
    *   **Action:** At the beginning of `build_main_window_static_layout`, add `PlatformCommand::DefineStyle` commands for all common styles your application will use.
        ```rust
        // Example definitions
        commands.push(PlatformCommand::DefineStyle {
            style_id: StyleId::StatusLabelNormal,
            style_definition: ControlStyle {
                font: Some(FontDescription { name: Some("Segoe UI".to_string()), size: Some(9), ..Default::default() }),
                text_color: Some(Color { r: 0, g: 0, b: 0 }), // Black
                ..Default::default()
            },
        });
        commands.push(PlatformCommand::DefineStyle {
            style_id: StyleId::StatusLabelError,
            style_definition: ControlStyle {
                font: Some(FontDescription { name: Some("Segoe UI".to_string()), size: Some(9), weight: Some(FontWeight::Bold), ..Default::default() }),
                text_color: Some(Color { r: 200, g: 0, b: 0 }), // Dark Red
                ..Default::default()
            },
        });
        // ... more style definitions
        ```

2.  **(UIDL) Apply Styles to Controls:**
    *   **File:** `ui_description_layer/mod.rs`.
    *   **Action:** After each relevant `CreateLabel`, `CreateButton`, etc. command, add a `PlatformCommand::ApplyStyleToControl` command.
        ```rust
        // Example applying to a status label
        commands.push(PlatformCommand::CreateLabel {
            window_id,
            parent_panel_id: ui_constants::STATUS_BAR_PANEL_ID,
            label_id: ui_constants::STATUS_LABEL_GENERAL_ID,
            initial_text: "Status: Initial".to_string(),
        });
        commands.push(PlatformCommand::ApplyStyleToControl {
            window_id,
            control_id: ui_constants::STATUS_LABEL_GENERAL_ID,
            style_id: StyleId::StatusLabelNormal, // Apply the defined normal style
        });
        ```
    *   **Decision:** The old `MessageSeverity` field in `UpdateLabelText` and `label_severities` in `NativeWindowData` becomes partially redundant if all color styling for labels is handled by `StyleId`s like `StatusLabelError`, `StatusLabelWarning`. You might:
        *   Keep `MessageSeverity` in `UpdateLabelText` and have `app_logic` decide which `StyleId` to apply based on severity (e.g., by sending an `ApplyStyleToControl` command). This is flexible.
        *   Or, `UpdateLabelText` only updates text, and a separate `ApplyStyleToControl` is always used for color changes.

---

## Phase 5: Testing and Refinement

**Objective:** Ensure the styling system works correctly and robustly.

1.  **Unit Tests:**
    *   Test `execute_define_style`: verify `HFONT` creation (mock or check handle validity), `defined_styles` map content.
    *   Test `execute_apply_style_to_control`: verify `WM_SETFONT` is (conceptually) sent, `applied_styles` map in `NativeWindowData` is updated, `InvalidateRect` is called.
2.  **Manual Verification:**
    *   Run the application. Check if fonts are applied correctly to labels, buttons, etc.
    *   Check if text colors and background colors (if implemented) are applied.
    *   Test with different style definitions.
3.  **Resource Leak Check:**
    *   Use tools like Windows Task Manager (Details -> Add Columns -> GDI Objects) to monitor GDI object count while repeatedly creating/destroying windows or redefining styles (if that becomes dynamic) to ensure `HFONT`s are being released.
4.  **API Review:**
    *   Is `ControlStyle` comprehensive enough for initial needs?
    *   Is `StyleId` easy to use and manage?
    *   Is the interaction between `UpdateLabelText` (with its `severity`) and the new styling system clear, especially for status labels? Decide on the final approach.

---

## Phase 6: Future Extensions and Inventive Ideas

**Objective:** Explore more advanced styling capabilities.

1.  **Themes:**
    *   **Concept:** A theme is a collection of `StyleId` definitions.
    *   **Implementation:**
        *   `ui_description_layer` could have functions like `build_dark_theme_styles()` and `build_light_theme_styles()`, each returning `Vec<PlatformCommand::DefineStyle>`.
        *   `app_logic` could have a setting for the current theme. On theme change, it would instruct `ui_description_layer` (or a new theme manager module) to generate the `DefineStyle` commands for the new theme and send them.
        *   The `platform_layer` would re-process these `DefineStyle`s, replacing old definitions.
        *   To make existing controls update, `app_logic` might need to re-send `ApplyStyleToControl` for all relevant controls, or the `platform_layer` could iterate through `NativeWindowData.applied_styles` and re-apply them if their underlying `StyleId` definition changed.

2.  **Style Inheritance (Simplified):**
    *   **Concept:** A `ControlStyle` could optionally specify a `parent_style_id: Option<StyleId>`.
    *   **Implementation:** When resolving a style in `platform_layer`, if a property is not set in the current style, it would look up the parent style and use its property. This creates a chain. `CreateFontIndirectW` would still need a full `LOGFONTW`.

3.  **Dynamic Style Property Updates (More Granular):**
    *   **Concept:** Change individual aspects of an *applied* style or a *defined* style dynamically.
    *   **Commands:**
        *   `PlatformCommand::UpdateDefinedStyleProperty { style_id: StyleId, property: StylePropertyUpdate }` (e.g., `StylePropertyUpdate::TextColor(new_color)`). This would affect all controls currently using that `style_id`.
        *   `PlatformCommand::OverrideControlStyleProperty { window_id: WindowId, control_id: i32, property: StylePropertyUpdate }`. This applies a one-off override to a control without changing its base `StyleId`. `NativeWindowData` would need to store these overrides.

4.  **Predefined System Styles:**
    *   **Concept:** Expose system metrics and colors (e.g., `COLOR_WINDOWTEXT`, `COLOR_BTNFACE`) as special `StyleId`s.
    *   **Implementation:** `execute_apply_style_to_control` would recognize these and use `GetSysColor` or system font metrics instead of looking up a `ParsedControlStyle`.

5.  **Styling for Control States (Hover, Pressed, Disabled):**
    *   **Concept:** Define styles for different states of a control.
    *   **Implementation:** Very complex for standard Win32 controls. Often requires owner-drawing (`WM_DRAWITEM`) or subclassing and custom drawing.
        *   `ControlStyle` could expand: `hover_font: Option<FontDescription>`, `pressed_background_color: Option<Color>`.
        *   `platform_layer` would need significantly more logic in `WndProc` (e.g., tracking mouse hover, button down state) and potentially full custom drawing for the controls.

6.  **CSS-like String Definitions (Ambitious):**
    *   **Concept:** Allow defining styles via a simple CSS-like string: `font-family: Segoe UI; font-size: 10pt; color: #FF0000;`.
    *   **Implementation:** `PlatformCommand::DefineStyleFromString { style_id: StyleId, style_string: String }`. `platform_layer` would need a parser for this string to populate `ControlStyle`. High effort, low immediate ROI for Win32.

7.  **Integration with High Contrast Mode / System Themes:**
    *   **Concept:** Allow styles to optionally defer to system theme settings, especially for accessibility.
    *   **Implementation:** `ControlStyle` properties could have a "use system default" option. `platform_layer` would then use appropriate system calls.
