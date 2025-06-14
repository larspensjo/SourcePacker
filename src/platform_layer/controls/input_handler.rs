/*
 * Provides handling for input (EDIT) controls, specifically custom background
 * colors via WM_CTLCOLOREDIT. State is stored in the window data so the command
 * executor can update colors without direct Win32 calls.
 */

use crate::platform_layer::PlatformResult;
use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::types::WindowId;
use std::sync::Arc;
use windows::Win32::{
    Foundation::{COLORREF, HWND, LRESULT},
    Graphics::Gdi::{HBRUSH, SetBkColor},
    UI::WindowsAndMessaging::GetDlgCtrlID,
};

#[derive(Debug)]
pub(crate) struct InputColorState {
    pub color: COLORREF,
    pub brush: HBRUSH,
}

/*
 * Handles the WM_CTLCOLOREDIT message for input controls.
 * This function is called when an EDIT control is about to be drawn. It uses
 * the with_window_data_read helper to safely look up if a custom background
 * color has been set for the specific control. If so, it sets the background
 * color and returns the corresponding brush handle.
 */
pub(crate) fn handle_wm_ctlcoloredit(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    hdc_edit: windows::Win32::Graphics::Gdi::HDC,
    hwnd_edit: HWND,
) -> Option<LRESULT> {
    let result: PlatformResult<Option<LRESULT>> =
        internal_state.with_window_data_read(window_id, |window_data| {
            let control_id = unsafe { GetDlgCtrlID(hwnd_edit) };
            if control_id == 0 {
                // Not a control with an ID, or an error occurred. Let the system handle it.
                return Ok(None);
            }

            if let Some(state) = window_data.get_input_background_color(control_id) {
                unsafe {
                    SetBkColor(hdc_edit, state.color);
                }
                // Return the brush handle for the system to use.
                return Ok(Some(LRESULT(state.brush.0 as isize)));
            }

            // We found the window, but this specific control doesn't have a custom color.
            Ok(None)
        });
    // Convert the PlatformResult<Option<LRESULT>> to Option<LRESULT>.
    // - Ok(Some(lresult)) becomes Some(lresult).
    // - Ok(None) becomes None.
    // - Err(_) becomes None.
    // This correctly mirrors the original function's behavior where any failure to
    // find the window data resulted in default processing (returning None).
    result.ok().flatten()
}
