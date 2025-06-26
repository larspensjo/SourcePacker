/*
 * This module defines the visual themes for the application. It centralizes
 * the definition of colors, fonts, and other style properties into reusable
 * themes. The primary function, `define_neon_night_theme`, constructs a series
 * of `PlatformCommand::DefineStyle` commands that the platform layer can use
 * to configure the application's appearance.
 *
 * By separating theme definition from UI layout (which is in
 * ui_description_layer.rs), we can easily switch or modify the entire
 * application's look and feel from a single location.
 */
use crate::platform_layer::{
    Color, ControlStyle, FontDescription, FontWeight, PlatformCommand, StyleId,
};

/*
 * Creates and returns a vector of `PlatformCommand::DefineStyle` commands
 * that constitute the "Neon Night" theme. This theme uses a dark background
 * with bright, readable text and accent colors for interactive elements.
 */
pub fn define_neon_night_theme() -> Vec<PlatformCommand> {
    let mut commands = Vec::new();

    // --- Color Palette ---
    let bg_main = Color {
        r: 30,
        g: 30,
        b: 30,
    };
    let bg_panel = Color {
        r: 45,
        g: 45,
        b: 45,
    };
    let bg_input = Color {
        r: 60,
        g: 60,
        b: 60,
    };
    let text_light = Color {
        r: 220,
        g: 220,
        b: 220,
    };
    let bg_error = Color {
        r: 80,
        g: 40,
        b: 40,
    }; // Dark red background for error states
    let text_error = Color {
        r: 255,
        g: 100,
        b: 100,
    };
    let text_warning = Color {
        r: 255,
        g: 165,
        b: 0,
    }; // Orange

    // --- Font Definitions ---
    let default_font = FontDescription {
        name: Some("Segoe UI".to_string()),
        size: Some(9),
        weight: Some(FontWeight::Normal),
    };

    // --- Style Definitions ---

    // General window background
    let main_window_style = ControlStyle {
        background_color: Some(bg_main.clone()),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::MainWindowBackground,
        style: main_window_style,
    });

    // Panel backgrounds (e.g., status bar, filter bar)
    let panel_style = ControlStyle {
        background_color: Some(bg_panel.clone()),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::PanelBackground,
        style: panel_style.clone(),
    });
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::StatusBarBackground,
        style: panel_style,
    });

    // Default text label style
    let default_text_style = ControlStyle {
        text_color: Some(text_light.clone()),
        font: Some(default_font.clone()),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::DefaultText,
        style: default_text_style,
    });

    // Default button style
    let button_style = ControlStyle {
        text_color: Some(text_light.clone()),
        background_color: Some(bg_input.clone()),
        font: Some(default_font.clone()),
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::DefaultButton,
        style: button_style,
    });

    // Default input/edit control style
    let input_style = ControlStyle {
        text_color: Some(text_light.clone()),
        background_color: Some(bg_input.clone()),
        font: Some(default_font.clone()),
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::DefaultInput,
        style: input_style,
    });

    // Style for input controls when in an error state (e.g., filter no match)
    let input_error_style = ControlStyle {
        text_color: Some(text_light.clone()),
        background_color: Some(bg_error),
        font: Some(default_font.clone()),
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::DefaultInputError,
        style: input_error_style,
    });

    // Status label styles, matching legacy colors for now
    let status_normal_style = ControlStyle {
        text_color: Some(text_light),
        font: Some(default_font.clone()),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::StatusLabelNormal,
        style: status_normal_style,
    });

    let status_warning_style = ControlStyle {
        text_color: Some(text_warning),
        font: Some(default_font.clone()),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::StatusLabelWarning,
        style: status_warning_style,
    });

    let status_error_style = ControlStyle {
        text_color: Some(text_error),
        font: Some(default_font),
        ..Default::default()
    };
    commands.push(PlatformCommand::DefineStyle {
        style_id: StyleId::StatusLabelError,
        style: status_error_style,
    });

    commands
}
