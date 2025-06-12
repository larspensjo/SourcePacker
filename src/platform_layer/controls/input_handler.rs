/*
 * Provides handling for input (EDIT) controls, specifically custom background
 * colors via WM_CTLCOLOREDIT. State is stored in the window data so the command
 * executor can update colors without direct Win32 calls.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::WindowId;
use std::sync::Arc;
use windows::Win32::{
    Foundation::{COLORREF, GetLastError, HWND, LRESULT},
    Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH, SetBkColor},
    UI::WindowsAndMessaging::GetDlgCtrlID,
};

#[derive(Debug)]
pub(crate) struct InputColorState {
    pub color: COLORREF,
    pub brush: HBRUSH,
}

pub(crate) fn set_input_background_color(
    window_data: &mut crate::platform_layer::window_common::NativeWindowData,
    control_id: i32,
    color: Option<u32>,
) -> PlatformResult<()> {
    if let Some(existing) = window_data.input_bg_colors.remove(&control_id) {
        unsafe {
            let _ = DeleteObject(existing.brush.into());
        }
    }
    if let Some(c) = color {
        let colorref = COLORREF(c);
        let brush = unsafe { CreateSolidBrush(colorref) };
        if brush.is_invalid() {
            return Err(PlatformError::OperationFailed(format!(
                "CreateSolidBrush failed: {:?}",
                unsafe { GetLastError() }
            )));
        }
        window_data.input_bg_colors.insert(
            control_id,
            InputColorState {
                color: colorref,
                brush,
            },
        );
    }
    Ok(())
}

pub(crate) fn handle_wm_ctlcoloredit(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    hdc_edit: windows::Win32::Graphics::Gdi::HDC,
    hwnd_edit: HWND,
) -> Option<LRESULT> {
    let windows_map_guard = internal_state.active_windows.read().ok()?;
    let window_data = windows_map_guard.get(&window_id)?;
    let control_id = unsafe { GetDlgCtrlID(hwnd_edit) };
    if control_id == 0 {
        return None;
    }
    if let Some(state) = window_data.input_bg_colors.get(&control_id) {
        unsafe {
            SetBkColor(hdc_edit, state.color);
        }
        return Some(LRESULT(state.brush.0 as isize));
    }
    None
}

pub(crate) fn cleanup(window_data: &mut crate::platform_layer::window_common::NativeWindowData) {
    for (_, state) in window_data.input_bg_colors.drain() {
        unsafe {
            let _ = DeleteObject(state.brush.into());
        }
    }
}
