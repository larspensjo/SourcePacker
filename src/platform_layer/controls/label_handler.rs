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
 * NativeWindowData and sets its initial severity, all within a single write transaction.
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

    internal_state.with_window_data_write(window_id, |window_data| {
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

        let h_instance = internal_state.h_instance();
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
                Some(h_instance),
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
    })
}

/*
 * Handles the update of a label's text and its associated severity.
 * This function is carefully structured to avoid deadlocks. It first acquires a
 * write lock to update the internal severity state and retrieve the label's HWND.
 * It then RELEASES the lock before making any Win32 API calls that could
 * synchronously dispatch messages (like SetWindowTextW), preventing a re-entrant
 * lock on the same thread.
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

    let hwnd_label =
        internal_state.with_window_data_write(window_id, |window_data| {
            let hwnd = window_data.get_control_hwnd(label_id).ok_or_else(|| {
                log::warn!(
                    "LabelHandler: Label with logical ID {} not found for UpdateLabelText in WinID {:?}.",
                    label_id,
                    window_id
                );
                PlatformError::InvalidHandle(format!(
                    "Label with logical ID {} not found for UpdateLabelText in WinID {:?}",
                    label_id, window_id
                ))
            })?;

            // Update the severity state.
            window_data.set_label_severity(label_id, severity);

            // Return the HWND for use outside the lock.
            Ok(hwnd)
        })?;

    unsafe {
        if SetWindowTextW(hwnd_label, &HSTRING::from(text.as_str())).is_err() {
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
}

/*
 * Handles the WM_CTLCOLORSTATIC message specifically for label controls.
 * This function is called when a label (STATIC control) is about to be drawn.
 * It determines the appropriate text color based on the label's stored severity
 * and sets the background to transparent.
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

    let result = internal_state.with_window_data_read(window_id, |window_data| {
        let control_id_of_static = unsafe { GetDlgCtrlID(hwnd_static_ctrl) };
        if control_id_of_static == 0 {
            log::trace!(
                "LabelHandler: WM_CTLCOLORSTATIC for HWND {:?} which has no control ID. Defaulting.",
                hwnd_static_ctrl
            );
            return Ok(None);
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
                let brush_result = LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
                return Ok(Some(brush_result));
            }
        }

        log::trace!(
            "LabelHandler: No severity found for label ID {} (HWND {:?}). Defaulting.",
            control_id_of_static,
            hwnd_static_ctrl
        );
        Ok(None) // Default processing
    });

    // Convert PlatformResult<Option<LRESULT>> to Option<LRESULT>
    result.ok().flatten()
}
