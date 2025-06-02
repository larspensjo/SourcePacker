# Step-by-Step Plan: Implementing Defined Styles (Method 2) & UI Enhancements

**Goal:** Implement a foundational styling system where styles are defined with an ID and then applied to controls. Subsequently, extend this system with panel-specific backgrounds, border controls, and tooltip functionality. This plan assumes the "UI Descriptive Layer plan" is completed.

**Legend:**
*   **(PL-Types):** Changes in `platform_layer/types.rs` (or a new `platform_layer/styling.rs`)
*   **(PL-State):** Changes in `platform_layer/app.rs` (Win32ApiInternalState) or `platform_layer/window_common.rs` (NativeWindowData)
*   **(PL-Exec):** Changes in `platform_layer/command_executor.rs`
*   **(PL-WndProc):** Changes in `platform_layer/window_common.rs` (WndProc message handling)
*   **(UIDL):** Changes in `ui_description_layer/mod.rs`
*   **(AppLogic):** Potential future changes in `app_logic/handler.rs` for dynamic styling

---

## Phase 1: Define Core Styling Types and Commands (Foundation)

**Objective:** Establish the necessary structs for describing styles (initially font & color) and the `PlatformCommand`s to manage them.

1.  **Create `platform_layer/styling.rs` (Optional but Recommended):**
    *   **Action:** Create `src/platform_layer/styling.rs`.
    *   **Action:** Add `pub mod styling;` to `src/platform_layer/mod.rs` and `pub use styling::*;` to re-export.

2.  **(PL-Types) Define Basic Styling Primitives:**
    *   **File:** `platform_layer/styling.rs`.
    *   **Action:** Define `Color`, `FontWeight`, and `FontDescription` structs.
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
            pub italic: Option<bool>,
            pub underline: Option<bool>,
            pub strikeout: Option<bool>,
        }
        ```

3.  **(PL-Types) Define `ControlStyle` Struct (Initial Version):**
    *   **File:** `platform_layer/styling.rs`.
    *   **Action:** Define the main `ControlStyle` struct, starting with font and basic colors.
        ```rust
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct ControlStyle {
            pub font: Option<FontDescription>,
            pub text_color: Option<Color>,
            pub background_color: Option<Color>,
            // Border and other properties will be added in later phases
        }
        ```

4.  **(PL-Types) Define `StyleId` Enum (Initial Version):**
    *   **File:** `platform_layer/styling.rs`.
    *   **Action:** Define an enum for named styles. Start with essential ones.
        ```rust
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum StyleId {
            // General Purpose
            DefaultText,
            DefaultButton,
            DefaultPanel, // For generic panel backgrounds

            // Specific Labels (examples)
            StatusLabelNormal,
            StatusLabelWarning,
            StatusLabelError,
            // Add more specific StyleIds for panels in Phase 6
        }
        ```

5.  **(PL-Types) Add New `PlatformCommand` Variants for Styling:**
    *   **File:** `platform_layer/types.rs` (in `PlatformCommand` enum).
    *   **Action:** Add commands for defining and applying styles.
        ```rust
        // ... existing commands
        DefineStyle {
            style_id: StyleId,
            style_definition: ControlStyle,
        },
        ApplyStyleToControl {
            window_id: WindowId,
            control_id: i32,
            style_id: StyleId,
        },
        ```

6.  **(PL-State) Define `ParsedControlStyle` (Internal - Initial Version):**
    *   **File:** `platform_layer/styling.rs` (or `app.rs`).
    *   **Action:** Struct for platform layer's internal representation.
        ```rust
        use windows::Win32::Graphics::Gdi::HFONT;

        #[derive(Debug, Clone)]
        pub(crate) struct ParsedControlStyle {
            pub(crate) font_handle: Option<HFONT>,
            pub(crate) text_color: Option<Color>,
            pub(crate) background_color: Option<Color>,
            pub(crate) original_font_desc: Option<FontDescription>,
            // Border and other parsed properties will be added in later phases
        }

        impl Drop for ParsedControlStyle { // Handles HFONT cleanup
            fn drop(&mut self) {
                if let Some(hfont) = self.font_handle.take() {
                    if !hfont.is_invalid() {
                        unsafe { windows::Win32::Graphics::Gdi::DeleteObject(hfont); }
                        log::trace!("Dropped HFONT {:?}", hfont);
                    }
                }
            }
        }
        ```

---

## Phase 2: `platform_layer` - Implement `DefineStyle` (Foundation)

*(No changes from original plan for this phase, focuses on initial `ControlStyle` fields)*

1.  **(PL-State) Add Storage for Defined Styles in `Win32ApiInternalState`:** `defined_styles: RwLock<HashMap<StyleId, ParsedControlStyle>>`.
2.  **(PL-Exec) Implement `execute_define_style`:** Parses initial `ControlStyle` (font, text/bg color), creates `HFONT`, stores in `defined_styles`.

---

## Phase 3: `platform_layer` - Implement `ApplyStyleToControl` and Rendering (Foundation)

*(No changes from original plan for this phase, focuses on initial `ControlStyle` fields)*

1.  **(PL-State) Add Storage for Applied Styles per Control in `NativeWindowData`:** `applied_styles: HashMap<i32, StyleId>`.
2.  **(PL-Exec) Implement `execute_apply_style_to_control`:** Looks up style, sends `WM_SETFONT`, stores mapping, invalidates control.
3.  **(PL-WndProc) Modify `WM_CTLCOLORSTATIC` (and similar):** Retrieves applied style and uses its `text_color` and `background_color`.

---

## Phase 4: `ui_description_layer` Integration (Foundation)

*(No changes from original plan for this phase, focuses on initial `ControlStyle` fields)*

1.  **(UIDL) Define Styles:** Add `DefineStyle` commands using the initial `ControlStyle` definition.
2.  **(UIDL) Apply Styles to Controls:** Add `ApplyStyleToControl` commands.
3.  **(UIDL) Decide on `MessageSeverity` vs. `StyleId` for status label colors.**

---

## Phase 5: Testing and Refinement (Foundation)

*(No changes from original plan for this phase)*

1.  **Unit Tests & Manual Verification** for font and color application.
2.  **Resource Leak Check** for `HFONT`s.
3.  **API Review.**

---

## Phase 6: Advanced Styling - Panel/Section Specific Backgrounds

**Objective:** Allow `ui_description_layer` to define distinct background colors for different panels/sections by leveraging the existing styling mechanism with more specific `StyleId`s.

1.  **(PL-Types & UIDL) Define More Specific `StyleId`s for Panels:**
    *   **File (PL-Types):** `platform_layer/styling.rs` (in `StyleId` enum).
    *   **Action:** Add new `StyleId`s like:
        ```rust
        // ... existing StyleIds
        FiltersPaneBackground,
        EditorPaneBackground,
        StatusBarPanelBackground, // If not already granular enough
        // Add others as identified from your UI structure
        ```
    *   **File (UIDL):** `ui_description_layer/mod.rs`.
    *   **Action:** In `build_main_window_static_layout`, use `PlatformCommand::DefineStyle` to define these new `StyleId`s, primarily setting their `background_color` property in `ControlStyle`.
        ```rust
        commands.push(PlatformCommand::DefineStyle {
            style_id: StyleId::FiltersPaneBackground,
            style_definition: ControlStyle {
                background_color: Some(Color { r: 60, g: 60, b: 60 }), // Example dark gray
                ..Default::default() // Other properties can be default or inherited if/when implemented
            },
        });
        ```
    *   **Action (UIDL):** When creating panels (e.g., status bar panel, future sidebars), use `PlatformCommand::ApplyStyleToControl` to apply these new panel-specific `StyleId`s to the respective panel `control_id`s.

2.  **(PL-WndProc) Ensure Panel Backgrounds are Drawn:**
    *   **File:** `platform_layer/window_common.rs`.
    *   **Action:** The existing `WM_CTLCOLORSTATIC` handler (which panels are, as they are `WC_STATIC`) should already pick up the `background_color` from the applied `StyleId` if it's set. It needs to return an `HBRUSH` for that color.
        ```rust
        // Inside WM_CTLCOLORSTATIC, after retrieving 'parsed_style':
        if let Some(parsed_style) = style_to_apply {
            // ... existing text_color logic ...
            if let Some(bg_color) = &parsed_style.background_color {
                // IMPORTANT: Brushes created here need to be managed (cached or deleted).
                // For simplicity now, we might leak them or use GetSysColorBrush if it's a system color.
                // A better approach is a brush cache in Win32ApiInternalState.
                let hbrush = unsafe { CreateSolidBrush(RGB(bg_color.r, bg_color.g, bg_color.b)) };
                lresult_override = Some(LRESULT(hbrush.0 as isize)); // Return the brush
                // SetBkColor(hdc_static_ctrl, RGB(bg_color.r, bg_color.g, bg_color.b)); // Alternative for some controls
            } else {
                // Default background handling (e.g., transparent for labels, default for panels)
                unsafe { SetBkMode(hdc_static_ctrl, TRANSPARENT.0); }
                lresult_override = Some(LRESULT(GetStockObject(NULL_BRUSH).0 as isize));
            }
            handled = true;
        }
        ```
    *   **Refinement:** Brush management for `WM_CTLCOLOR*` is important. Creating brushes on every message is inefficient and leaks. Implement a brush cache in `Win32ApiInternalState` or `NativeWindowData` keyed by `Color`, or only support a fixed palette of system brushes initially.

3.  **Testing:**
    *   Verify that different panels defined by `ui_description_layer` now show their distinct background colors.

---

## Phase 7: Advanced Styling - Border Control (Thickness, Color, Visibility)

**Objective:** Extend `ControlStyle` and `platform_layer` to support basic border styling. "Shadow" is deferred as it's much more complex.

1.  **(PL-Types) Extend `ControlStyle` and `ParsedControlStyle` for Borders:**
    *   **File:** `platform_layer/styling.rs`.
    *   **Action:** Add border-related fields to both structs.
        ```rust
        // In ControlStyle
        // ... existing fields ...
        pub border_thickness: Option<u32>,     // e.g., 1, 2 pixels
        pub border_color: Option<Color>,
        pub border_visibility: Option<bool>,   // To explicitly show/hide standard WS_BORDER

        // In ParsedControlStyle
        // ... existing fields ...
        pub(crate) border_thickness: Option<u32>,
        pub(crate) border_color: Option<Color>,
        pub(crate) border_visibility: Option<bool>,
        ```

2.  **(PL-Exec) Update `execute_define_style`:**
    *   **Action:** Modify it to copy the new border properties from `style_definition` to `parsed_style`.

3.  **(PL-Exec & PL-WndProc) Implement Border Application:**
    *   **Standard Border Visibility (`border_visibility`):**
        *   **Action (PL-Exec):** In `execute_apply_style_to_control`, if the applied `ParsedControlStyle` has `border_visibility` set:
            *   Get current window styles using `GetWindowLongPtrW(hwnd, GWL_STYLE)`.
            *   If `border_visibility` is `true`, add `WS_BORDER` style. If `false`, remove `WS_BORDER`.
            *   Use `SetWindowLongPtrW(hwnd, GWL_STYLE, new_styles)` to apply.
            *   Call `SetWindowPos(hwnd, ..., SWP_FRAMECHANGED | ...)` to force redraw of the frame.
    *   **Custom Border Color/Thickness (Simple Initial Version - `border_color`, `border_thickness`):**
        *   This requires custom drawing. A simple approach for 1px borders on `WC_STATIC` (panels) or other simple controls:
        *   **(PL-WndProc) `WM_PAINT` Handler:**
            *   If a control has a style with `border_color` and `border_thickness: Some(1)`:
                *   After default painting (or instead of, for full custom), get client `RECT`.
                *   Create a pen with `border_color`.
                *   Select pen into `HDC`, draw rectangle frame inside the client rect.
                *   Restore old pen, delete created pen.
        *   **Alternative (WM_NCPAINT for non-client borders - more complex):**
            *   If a style has `border_color`:
                *   Handle `WM_NCPAINT`. Call default proc first.
                *   Get window DC (`GetWindowDC`).
                *   Draw a rectangle around the window's non-client frame using the `border_color`.
                *   Release DC.
        *   **Initial Focus:** Start with `border_visibility` (controlling `WS_BORDER`). Custom color/thickness can be a more advanced follow-up within this phase or deferred if too complex initially.

4.  **(UIDL) Update Style Definitions:**
    *   **Action:** Modify some `DefineStyle` commands to include `border_visibility`, `border_color`, `border_thickness` for relevant controls.

5.  **Testing:**
    *   Verify `WS_BORDER` can be toggled.
    *   If attempting custom border drawing, test visual correctness, performance, and ensure it doesn't interfere with standard control painting.

---

## Phase 8: UI Enhancement - Tooltip Support

**Objective:** Implement functionality for displaying tooltips on UI controls.

1.  **(PL-Types) Add `PlatformCommand` for Tooltips:**
    *   **File:** `platform_layer/types.rs`.
    *   **Action:**
        ```rust
        // In PlatformCommand enum
        SetControlTooltip {
            window_id: WindowId,
            control_id: i32,
            tooltip_text: Option<String>, // None to remove tooltip
        },
        ```

2.  **(PL-State) Manage Tooltip Control in `NativeWindowData`:**
    *   **File:** `platform_layer/window_common.rs`.
    *   **Action:** Add `pub(crate) hwnd_tooltip: Option<HWND>` to `NativeWindowData`.
    *   **Action:** Initialize to `None`. Ensure `DestroyWindow` is called on it if Some during `WM_DESTROY`.

3.  **(PL-Exec) Implement `execute_set_control_tooltip`:**
    *   **File:** `platform_layer/command_executor.rs`.
    *   **Action:**
        *   Retrieve `window_data` and `control_hwnd`.
        *   If `window_data.hwnd_tooltip` is `None`, create a tooltip control (`TOOLTIPS_CLASSW`) as a child of `window_data.hwnd`. Store its handle in `window_data.hwnd_tooltip`.
            *   Use styles like `WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX`.
            *   Optionally send `TTM_SETMAXTIPWIDTH`.
        *   Create a `TOOLINFOW` struct.
            *   `uFlags = TTF_IDISHWND | TTF_SUBCLASS` (identifies tool by `HWND`, subclasses the control).
            *   `hwnd = window_data.hwnd` (owner of the tooltip window).
            *   `uId = control_hwnd.0 as usize` (the `HWND` of the control getting the tooltip).
        *   If `tooltip_text` is `Some`:
            *   Set `ti.lpszText` to point to the (null-terminated) wide string of the text.
            *   Send `TTM_ADDTOOLW` message to `hwnd_tooltip_ctrl`. Handle potential errors if the tool already exists (log, or try `TTM_DELTOOLW` then `TTM_ADDTOOLW`).
        *   If `tooltip_text` is `None`:
            *   Send `TTM_DELTOOLW` message to `hwnd_tooltip_ctrl` to remove the tooltip for that `control_hwnd`.

4.  **(PL-WndProc) Message Relaying (Generally Automatic with `TTF_SUBCLASS`):**
    *   `TTF_SUBCLASS` usually handles message relaying. If issues arise with custom controls not showing tooltips, you might need to manually relay mouse messages using `TTM_RELAYEVENT`. For standard controls, this is often not needed.

5.  **(UIDL or AppLogic) Using Tooltips:**
    *   **Action (UIDL - for static tooltips):** After creating a control, send `SetControlTooltip` command.
        ```rust
        commands.push(PlatformCommand::CreateButton { /* ... */ control_id: MY_BUTTON_ID, /* ... */ });
        commands.push(PlatformCommand::SetControlTooltip {
            window_id,
            control_id: MY_BUTTON_ID,
            tooltip_text: Some("This button does X.".to_string()),
        });
        ```
    *   **Action (AppLogic - for dynamic tooltips):** `app_logic` can send `SetControlTooltip` at any time to change or remove a tooltip based on application state.

6.  **Testing:**
    *   Verify tooltips appear on hover for configured controls.
    *   Verify tooltips can be set, updated, and removed dynamically (if `AppLogic` sends commands).
    *   Test with multiple controls having tooltips on the same window.

---

## Phase 9: Review and Further Enhancements (Post-All-Features)

1.  **Comprehensive Testing:** Test all styling and tooltip features in combination.
2.  **Performance Review:** Especially for any custom drawing (borders) or frequent tooltip updates.
3.  **API and `StyleId` Refinement:** Based on usage, adjust `StyleId`s for clarity and add more `ControlStyle` properties if needed.
4.  **Consider "Shadows" (Major Future Work):** If desired, this would be a significant undertaking, likely involving layered windows or complex DWM manipulations.
