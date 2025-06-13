/*
 * This module is responsible for handling platform-specific logic related to
 * label controls (STATIC controls in Win32). It encapsulates the creation,
 * updating, and custom drawing aspects of labels, making the main command
 * executor and window procedure cleaner.
 *
 * It provides functions to execute label-related PlatformCommands and to handle
 * relevant Win32 messages like WM_CTLCOLORSTATIC for custom appearance.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{LabelClass, MessageSeverity, WindowId};
use crate::platform_layer::window_common::{SS_LEFT, WC_STATIC}; // Import common constants

use std::sync::Arc;
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{
            COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, GetSysColorBrush, HDC, InvalidateRect,
            SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, GetDlgCtrlID, SendMessageW, SetWindowTextW, WINDOW_EX_STYLE,
            WM_SETFONT, WS_CHILD, WS_VISIBLE,
        },
    },
    core::HSTRING,
};

/*
 * Handles the creation of a native label (STATIC) control.
 * This function takes the necessary parameters to create a label, including its parent,
 * logical ID, initial text, and class. It registers the new label's HWND with the
 * NativeWindowData and sets its initial severity.
 *
 * Parameters:
 * - internal_state: Shared state of the Win32 platform layer.
 * - window_id: The logical ID of the window where the label will be created.
 * - parent_panel_id: The logical ID of the panel that will host this label.
 * - label_id: The logical ID to assign to this new label.
 * - initial_text: The text to display on the label initially.
 * - class: The class of the label, used for potential specific styling.
 *
 * Returns:
 * - Ok(()) if creation is successful.
 * - PlatformError if creation fails (e.g., parent not found, ID conflict).
 */
pub(crate) fn handle_create_label_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    parent_panel_id: i32,
    label_id: i32,
    initial_text: String,
    class: LabelClass,
) -> PlatformResult<()> {
    log::debug!(
        "LabelHandler: handle_create_label_command for WinID {:?}, LabelID: {}, ParentPanelID: {}, Text: '{}', Class: {:?}",
        window_id,
        label_id,
        parent_panel_id,
        initial_text,
        class,
    );

    let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
        log::error!(
            "LabelHandler: Failed to lock windows map for CreateLabel: {:?}",
            e
        );
        PlatformError::OperationFailed("Failed to lock windows map for CreateLabel".into())
    })?;

    let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
        log::warn!(
            "LabelHandler: WindowId {:?} not found for CreateLabel.",
            window_id
        );
        PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for CreateLabel",
            window_id
        ))
    })?;

    if window_data.has_control(label_id) {
        log::warn!(
            "LabelHandler: Label with logical ID {} already exists for window {:?}.",
            label_id,
            window_id
        );
        return Err(PlatformError::OperationFailed(format!(
            "Label with logical ID {} already exists for window {:?}",
            label_id, window_id
        )));
    }

    let hwnd_parent_panel = window_data
        .get_control_hwnd(parent_panel_id)
        .ok_or_else(|| {
            log::warn!(
                "LabelHandler: Parent panel with logical ID {} not found for CreateLabel in WinID {:?}.",
                parent_panel_id, window_id
            );
            PlatformError::InvalidHandle(format!(
                "Parent panel with logical ID {} not found for CreateLabel in WinID {:?}",
                parent_panel_id, window_id
            ))
        })?;

    let hwnd_label = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WC_STATIC, // Use constant for "STATIC"
            &HSTRING::from(initial_text.as_str()),
            WS_CHILD | WS_VISIBLE | SS_LEFT, // Basic label styles
            0,
            0,
            10,
            10, // Dummy position/size, layout rules will adjust
            Some(hwnd_parent_panel),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                label_id as *mut _,
            )), // Use logical ID for the HMENU
            Some(internal_state.h_instance),
            None,
        )?
    };

    // Apply custom font if this is a status bar label and font exists
    if class == LabelClass::StatusBar {
        if let Some(h_font) = window_data.get_status_bar_font() {
            if !h_font.is_invalid() {
                unsafe {
                    SendMessageW(
                        hwnd_label,
                        WM_SETFONT,
                        Some(WPARAM(h_font.0 as usize)),
                        Some(LPARAM(1)),
                    )
                }; // LPARAM(1) to redraw
                log::debug!(
                    "LabelHandler: Applied status bar font to label ID {}",
                    label_id
                );
            }
        }
    }

    window_data.register_control_hwnd(label_id, hwnd_label);
    window_data.set_label_severity(label_id, MessageSeverity::Information); // Default to Information
    log::debug!(
        "LabelHandler: Created label '{}' (LogicalID {}) for WinID {:?} with HWND {:?}",
        initial_text,
        label_id,
        window_id,
        hwnd_label
    );
    Ok(())
}

/*
 * Handles the update of a label's text and its associated severity.
 * This function retrieves the label's HWND using its logical ID and then
 * updates its text content and stores the new severity in NativeWindowData.
 * It also invalidates the label to trigger a repaint, allowing WM_CTLCOLORSTATIC
 * to apply any severity-based coloring.
 *
 * Parameters:
 * - internal_state: Shared state of the Win32 platform layer.
 * - window_id: The logical ID of the window containing the label.
 * - label_id: The logical ID of the label to update.
 * - text: The new text to display on the label.
 * - severity: The new message severity associated with the text.
 *
 * Returns:
 * - Ok(()) if the update is successful.
 * - PlatformError if the update fails (e.g., label not found).
 */
pub(crate) fn handle_update_label_text_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    label_id: i32,
    text: String,
    severity: MessageSeverity,
) -> PlatformResult<()> {
    log::debug!(
        "LabelHandler: handle_update_label_text_command for WinID {:?}, LabelID: {}, Text: '{}', Severity: {:?}",
        window_id,
        label_id,
        text,
        severity
    );

    let hwnd_label_for_api_call: Option<HWND>;

    // Scope for the write lock on window_map to update label_severities
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
            log::error!(
                "LabelHandler: Failed to lock windows map for UpdateLabelText: {:?}",
                e
            );
            PlatformError::OperationFailed("Failed to lock windows map for UpdateLabelText".into())
        })?;

        let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
            log::warn!(
                "LabelHandler: WindowId {:?} not found for UpdateLabelText.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for UpdateLabelText",
                window_id
            ))
        })?;

        hwnd_label_for_api_call = window_data.get_control_hwnd(label_id);
        if hwnd_label_for_api_call.is_none() {
            log::warn!(
                "LabelHandler: Label with logical ID {} not found for UpdateLabelText in WinID {:?}.",
                label_id,
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Label with logical ID {} not found for UpdateLabelText in WinID {:?}",
                label_id, window_id
            )));
        }
        window_data.set_label_severity(label_id, severity);
    } // Write lock released

    // Now make WinAPI calls without holding the lock
    if let Some(hwnd_label) = hwnd_label_for_api_call {
        unsafe {
            if SetWindowTextW(hwnd_label, &HSTRING::from(text)).is_err() {
                let last_error = GetLastError();
                log::error!(
                    "LabelHandler: SetWindowTextW for label ID {} failed: {:?}",
                    label_id,
                    last_error
                );
                return Err(PlatformError::OperationFailed(format!(
                    "SetWindowTextW for label ID {} failed: {:?}",
                    label_id, last_error
                )));
            }
            // Trigger repaint for WM_CTLCOLORSTATIC to apply new severity color
            _ = InvalidateRect(Some(hwnd_label), None, true);
        }
        Ok(())
    } else {
        // This case should ideally be caught by the check above, but as a safeguard:
        log::error!(
            "LabelHandler: Label HWND for logical ID {} became invalid before API call in WinID {:?}.",
            label_id,
            window_id
        );
        Err(PlatformError::InvalidHandle(format!(
            "Label HWND for logical ID {} became invalid before API call in WinID {:?}",
            label_id, window_id
        )))
    }
}

/*
 * Handles the WM_CTLCOLORSTATIC message specifically for label controls.
 * This function is called when a label (STATIC control) is about to be drawn.
 * It determines the appropriate text color based on the label's stored severity
 * and sets the background to transparent.
 *
 * Parameters:
 * - internal_state: Shared state of the Win32 platform layer.
 * - window_id: The logical ID of the window containing the label.
 * - hdc_static_ctrl: The device context handle for the static control (from WPARAM).
 * - hwnd_static_ctrl: The window handle of the static control (from LPARAM).
 *
 * Returns:
 * - Some(LRESULT) containing the handle to the background brush if the message was
 *   handled for a known label.
 * - None if the control is not a known label or an error occurs, allowing default
 *   processing.
 */
pub(crate) fn handle_wm_ctlcolorstatic(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    hdc_static_ctrl: HDC,   // Directly pass HDC from WPARAM
    hwnd_static_ctrl: HWND, // Directly pass HWND from LPARAM
) -> Option<LRESULT> {
    log::trace!(
        "LabelHandler: handle_wm_ctlcolorstatic for WinID {:?}, HDC {:?}, HWND {:?}",
        window_id,
        hdc_static_ctrl,
        hwnd_static_ctrl
    );

    // No need to lock internal_state.active_windows for read here if we get all necessary info from window_data first.
    // However, we need to access window_data.label_severities.

    let windows_map_guard = internal_state.active_windows.read().ok()?;
    let window_data = windows_map_guard.get(&window_id)?;

    // Get the control ID from the HWND of the static control.
    // It's important that WM_CTLCOLORSTATIC's LPARAM is indeed the HWND of the control.
    let control_id_of_static = unsafe { GetDlgCtrlID(hwnd_static_ctrl) };

    if control_id_of_static == 0 {
        // Not a dialog control, or an error occurred.
        // This can happen for static text not created with a dialog ID.
        log::trace!(
            "LabelHandler: WM_CTLCOLORSTATIC for HWND {:?} which has no control ID or is not a dialog control. Defaulting.",
            hwnd_static_ctrl
        );
        return None;
    }

    if let Some(severity) = window_data.get_label_severity(control_id_of_static) {
        log::trace!(
            "LabelHandler: Found severity {:?} for label ID {} (HWND {:?})",
            severity,
            control_id_of_static,
            hwnd_static_ctrl
        );
        unsafe {
            let color = match severity {
                MessageSeverity::Error => windows::Win32::Foundation::COLORREF(0x000000FF), // Red
                MessageSeverity::Warning => windows::Win32::Foundation::COLORREF(0x0000A5FF), // Orange-ish
                _ => windows::Win32::Foundation::COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            };
            SetTextColor(hdc_static_ctrl, color);
            SetBkMode(hdc_static_ctrl, TRANSPARENT);
            // Return the brush for the parent window's background
            return Some(LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize));
        }
    } else {
        log::trace!(
            "LabelHandler: No severity found for label ID {} (HWND {:?}). Defaulting.",
            control_id_of_static,
            hwnd_static_ctrl
        );
    }
    None // Default processing
}
