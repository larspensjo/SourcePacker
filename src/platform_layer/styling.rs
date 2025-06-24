/*
 * This module defines the core data structures for a flexible, themeable
 * styling system for UI controls. It separates the definition of a style
 * (e.g., colors, fonts) from its application to a control, enabling
 * centralized theme management.
 *
 * Key components include `ControlStyle`, which describes the look of a
 * control, and `StyleId`, which acts as a unique key for a defined style.
 * The `ui_description_layer` uses these to define a theme, and the
 * `platform_layer` uses them to render controls. It also defines internal
 * `ParsedControlStyle` for holding native handles.
 */

use windows::Win32::Graphics::Gdi::{DeleteObject, HBRUSH, HFONT, HGDIOBJ};
// --- Public Styling Primitives and Descriptors ---

/*
 * A simple platform-agnostic representation of an RGB color.
 */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/*
 * Defines the weight (e.g., boldness) of a font.
 */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

/*
 * Describes the properties of a font in a platform-agnostic way.
 * All fields are optional, allowing styles to override only specific
 * aspects of a control's default font.
 */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontDescription {
    pub name: Option<String>,
    pub size: Option<i32>,
    pub weight: Option<FontWeight>,
    // italic, underline, etc. can be added here
}

/*
 * The master struct that holds all possible style properties for a control.
 * This is the public-facing description of a style, intended to be created
 * by the `ui_description_layer`. It will be translated into a
 * `ParsedControlStyle` by the platform layer.
 */
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlStyle {
    pub font: Option<FontDescription>,
    pub text_color: Option<Color>,
    pub background_color: Option<Color>,
    // Properties for border, hover, etc., will be added in later phases.
}

/*
 * A unique, semantic identifier for a specific, reusable style.
 * This is the equivalent of a style key (like `x:Key` in WPF or a CSS class name)
 * and is used to define a style and later apply it to one or more controls.
 */
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
    DefaultInputError,
    // Specific elements
    StatusLabelNormal,
    StatusLabelWarning,
    StatusLabelError,
}

// --- Internal (Crate-Private) Parsed Style Representation ---

/*
 * The `platform_layer`'s internal, processed representation of a `ControlStyle`.
 * This struct holds native resource handles (like `HFONT` and `HBRUSH`) that are
 * created from the platform-agnostic descriptions. This encapsulates Win32-specific
 * types and handles their cleanup via the `Drop` trait.
 */
#[derive(Debug, Clone)]
pub(crate) struct ParsedControlStyle {
    pub(crate) font_handle: Option<HFONT>,
    pub(crate) text_color: Option<Color>,
    pub(crate) background_color: Option<Color>,
    pub(crate) background_brush: Option<HBRUSH>,
    // ... other parsed properties ...
}

impl Drop for ParsedControlStyle {
    /*
     * Ensures that native GDI resources, such as HFONTs and HBRUSHes, are properly
     * released when a `ParsedControlStyle` is no longer in use. This prevents
     * resource leaks.
     */
    fn drop(&mut self) {
        if let Some(hfont) = self.font_handle.take() {
            if !hfont.is_invalid() {
                // It's safe to call DeleteObject on a font handle.
                unsafe {
                    DeleteObject(HGDIOBJ(hfont.0));
                }
            }
        }
        if let Some(hbrush) = self.background_brush.take() {
            if !hbrush.is_invalid() {
                // It's also safe to call DeleteObject on a brush handle.
                unsafe {
                    DeleteObject(HGDIOBJ(hbrush.0));
                }
            }
        }
    }
}
