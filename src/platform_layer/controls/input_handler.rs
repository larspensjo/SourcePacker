/*
 * Provides handling for input (EDIT) controls, specifically custom background
 * colors via WM_CTLCOLOREDIT. State is stored in the window data so the command
 * executor can update colors without direct Win32 calls.
 */

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


pub(crate) fn handle_wm_ctlcoloredit(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    hdc_edit: windows::Win32::Graphics::Gdi::HDC,
    hwnd_edit: HWND,
) -> Option<LRESULT> {
    let windows_map_guard = internal_state.active_windows().read().ok()?;
    let window_data = windows_map_guard.get(&window_id)?;
    let control_id = unsafe { GetDlgCtrlID(hwnd_edit) };
    if control_id == 0 {
        return None;
    }
    if let Some(state) = window_data.get_input_background_color(control_id) {
        unsafe {
            SetBkColor(hdc_edit, state.color);
        }
        return Some(LRESULT(state.brush.0 as isize));
    }
    None
}

