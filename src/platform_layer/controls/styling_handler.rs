/*
 * This module is responsible for parsing platform-agnostic style descriptions
 * into native-ready formats. It encapsulates the logic for converting
 * theme definitions (like fonts and colors) into resources that the Win32
 * API can use directly, such as HFONT and HBRUSH handles.
 */

use crate::platform_layer::{
    error::{PlatformError, Result as PlatformResult},
    styling::{Color, ControlStyle, FontWeight, ParsedControlStyle},
};
use windows::{
    Win32::{
        Foundation::{COLORREF, GetLastError},
        Graphics::Gdi::{
            CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_QUALITY,
            FF_DONTCARE, FW_BOLD, FW_NORMAL, GetDC, GetDeviceCaps, HBRUSH, HFONT, LOGPIXELSY,
            OUT_DEFAULT_PRECIS, ReleaseDC,
        },
        System::WindowsProgramming::MulDiv,
    },
    core::HSTRING,
};

/*
 * Creates a Win32 COLORREF from the platform-agnostic `Color` struct.
 * Win32 expects colors in BGR format, so this function handles the conversion.
 */
pub(crate) fn color_to_colorref(color: &Color) -> COLORREF {
    COLORREF((color.r as u32) | ((color.g as u32) << 8) | ((color.b as u32) << 16))
}

/*
 * Parses a platform-agnostic `ControlStyle` into a `ParsedControlStyle`.
 *
 * This function handles the "heavy lifting" of style conversion. It takes a
 * platform-agnostic description and creates any necessary native GDI resources,
 * such as creating an `HFONT` from a `FontDescription` and an `HBRUSH` from
 * a `background_color`. The resulting `ParsedControlStyle` can then be stored
 * by the platform layer for later use in rendering controls.
 */
pub(crate) fn parse_style(style: ControlStyle) -> PlatformResult<ParsedControlStyle> {
    // --- Parse FontDescription into HFONT ---
    let font_handle: Option<HFONT> = if let Some(font_desc) = &style.font {
        // To correctly calculate font height in logical units from point size,
        // we need the screen's DPI (dots per inch). We can get a temporary
        // device context for the entire screen for this purpose.
        let hdc_screen = unsafe { GetDC(None) };
        if hdc_screen.is_invalid() {
            log::error!("StylingHandler: Could not get screen DC for font creation.");
            return Err(PlatformError::OperationFailed(
                "Could not get screen DC for font creation".to_string(),
            ));
        }

        // The formula for nHeight is: -MulDiv(pointSize, GetDeviceCaps(hDC, LOGPIXELSY), 72)
        // The negative sign requests the font mapper to choose a font based on character height.
        let logical_font_height = if let Some(point_size) = font_desc.size {
            -unsafe { MulDiv(point_size, GetDeviceCaps(Some(hdc_screen), LOGPIXELSY), 72) }
        } else {
            0 // A value of 0 lets the font mapper choose a default height.
        };

        // Release the DC as soon as we're done with it.
        unsafe { ReleaseDC(None, hdc_screen) };

        let weight = match font_desc.weight {
            Some(FontWeight::Bold) => FW_BOLD.0 as i32,
            _ => FW_NORMAL.0 as i32, // Default to Normal for None or Some(Normal)
        };

        // Use a safe default font name if none is provided.
        let font_name = font_desc.name.as_deref().unwrap_or("MS Shell Dlg 2");
        let font_name_hstring = HSTRING::from(font_name);

        let h_font = unsafe {
            CreateFontW(
                logical_font_height,
                0,      // nWidth
                0,      // nEscapement
                0,      // nOrientation
                weight, // fnWeight
                0,      // fdwItalic
                0,      // fdwUnderline
                0,      // fdwStrikeOut
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                FF_DONTCARE.0 as u32,
                &font_name_hstring,
            )
        };

        if h_font.is_invalid() {
            log::error!("StylingHandler: CreateFontW failed: {:?}", unsafe {
                GetLastError()
            });
            return Err(PlatformError::OperationFailed(
                "CreateFontW failed".to_string(),
            ));
        }
        Some(h_font)
    } else {
        None
    };

    // --- Parse background_color into HBRUSH ---
    let background_brush: Option<HBRUSH> = if let Some(color) = &style.background_color {
        let color_ref = color_to_colorref(color);
        let h_brush = unsafe { CreateSolidBrush(color_ref) };
        if h_brush.is_invalid() {
            log::error!("StylingHandler: CreateSolidBrush failed: {:?}", unsafe {
                GetLastError()
            });
            return Err(PlatformError::OperationFailed(
                "CreateSolidBrush failed".to_string(),
            ));
        }
        Some(h_brush)
    } else {
        None
    };

    // --- Create the ParsedControlStyle ---
    let parsed_style = ParsedControlStyle {
        font_handle,
        text_color: style.text_color,
        background_color: style.background_color,
        background_brush,
    };

    Ok(parsed_style)
}
