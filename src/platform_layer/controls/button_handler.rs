/*
 * Encapsulates Win32-specific operations for button controls.
 * Provides creation of push buttons and translation of button click
 * notifications into platform-agnostic `AppEvent`s.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{AppEvent, WindowId};

use std::sync::Arc;
use windows::Win32::{
    Foundation::{HINSTANCE, HWND},
    UI::WindowsAndMessaging::{
        BS_PUSHBUTTON, CreateWindowExW, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_VISIBLE,
    },
};
use windows::core::{HSTRING, PCWSTR};

const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");

/*
 * Creates a native push button and registers the resulting HWND in the
 * window's `NativeWindowData`. Fails if the window or control ID are
 * invalid or already in use.
 */
pub(crate) fn handle_create_button_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    text: String,
) -> PlatformResult<()> {
    log::debug!(
        "ButtonHandler: handle_create_button_command for WinID {:?}, ControlID {}, Text: '{}'",
        window_id,
        control_id,
        text
    );

    let hwnd_parent_for_creation: HWND;
    let h_instance: HINSTANCE;
    {
        let mut windows_map = internal_state.active_windows.write().map_err(|e| {
            log::error!(
                "ButtonHandler: Failed to lock windows map for CreateButton: {:?}",
                e
            );
            PlatformError::OperationFailed("Failed to lock windows map for CreateButton".into())
        })?;

        let window_data = windows_map.get_mut(&window_id).ok_or_else(|| {
            log::warn!(
                "ButtonHandler: WindowId {:?} not found for CreateButton.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateButton",
                window_id
            ))
        })?;

        if window_data.has_control(control_id) {
            log::warn!(
                "ButtonHandler: Button with ID {} already exists for window {:?}.",
                control_id,
                window_id
            );
            return Err(PlatformError::OperationFailed(format!(
                "Button with ID {} already exists for window {:?}",
                control_id, window_id
            )));
        }

        if window_data.get_hwnd().is_invalid() {
            log::error!(
                "ButtonHandler: Parent HWND invalid for CreateButton (WinID: {:?})",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND invalid for CreateButton (WinID: {:?})",
                window_id
            )));
        }

        hwnd_parent_for_creation = window_data.get_hwnd();
        h_instance = internal_state.h_instance();
    }

    let hwnd_button = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WC_BUTTON,
            &HSTRING::from(text.as_str()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            0,
            0,
            10,
            10,
            Some(hwnd_parent_for_creation),
            Some(HMENU(control_id as *mut _)),
            Some(h_instance),
            None,
        )?
    };

    let mut windows_map = internal_state.active_windows.write().map_err(|e| {
        log::error!(
            "ButtonHandler: Failed to re-lock windows map after CreateButton: {:?}",
            e
        );
        PlatformError::OperationFailed("Failed to re-lock windows map after CreateButton".into())
    })?;

    if let Some(window_data) = windows_map.get_mut(&window_id) {
        if window_data.has_control(control_id) {
            log::warn!(
                "ButtonHandler: Control ID {} was created concurrently for window {:?}. Destroying new HWND.",
                control_id,
                window_id
            );
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd_button).ok();
            }
            return Err(PlatformError::OperationFailed(format!(
                "Button with ID {} already exists for window {:?}",
                control_id, window_id
            )));
        }
        window_data.register_control_hwnd(control_id, hwnd_button);
        log::debug!(
            "ButtonHandler: Created button '{}' (ID {}) for window {:?} with HWND {:?}",
            text,
            control_id,
            window_id,
            hwnd_button
        );
    } else {
        log::warn!(
            "ButtonHandler: WindowId {:?} disappeared before button insert. Destroying HWND.",
            window_id
        );
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd_button).ok();
        }
        return Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found after CreateButton",
            window_id
        )));
    }
    Ok(())
}

/*
 * Translates a BN_CLICKED notification into an `AppEvent::ButtonClicked`.
 */
pub(crate) fn handle_bn_clicked(
    window_id: WindowId,
    control_id: i32,
    hwnd_control: HWND,
) -> AppEvent {
    log::debug!(
        "ButtonHandler: BN_CLICKED for ID {} (HWND {:?}) in WinID {:?}",
        control_id,
        hwnd_control,
        window_id
    );
    AppEvent::ButtonClicked {
        window_id,
        control_id,
    }
}
