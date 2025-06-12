/*
 * Encapsulates Win32-specific operations for button controls.
 * Provides creation of push buttons and translation of button click
 * notifications into platform-agnostic `AppEvent`s.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{AppEvent, WindowId};
use crate::platform_layer::window_common::WC_BUTTON;

use std::sync::Arc;
use windows::Win32::{
    Foundation::{HWND, HINSTANCE},
    UI::WindowsAndMessaging::{
        CreateWindowExW, HMENU, WINDOW_EX_STYLE, WS_CHILD, WS_VISIBLE, BS_PUSHBUTTON,
        WINDOW_STYLE,
    },
};
use windows::core::HSTRING;

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
            PlatformError::InvalidHandle(format!("WindowId {:?} not found for CreateButton", window_id))
        })?;

        if window_data.control_hwnd_map.contains_key(&control_id) {
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

        if window_data.this_window_hwnd.is_invalid() {
            log::error!(
                "ButtonHandler: Parent HWND invalid for CreateButton (WinID: {:?})",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND invalid for CreateButton (WinID: {:?})",
                window_id
            )));
        }

        hwnd_parent_for_creation = window_data.this_window_hwnd;
        h_instance = internal_state.h_instance;
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
        if window_data.control_hwnd_map.contains_key(&control_id) {
            log::warn!(
                "ButtonHandler: Control ID {} was created concurrently for window {:?}. Destroying new HWND.",
                control_id,
                window_id
            );
            unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd_button).ok(); }
            return Err(PlatformError::OperationFailed(format!(
                "Button with ID {} already exists for window {:?}",
                control_id, window_id
            )));
        }
        window_data.control_hwnd_map.insert(control_id, hwnd_button);
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
        unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd_button).ok(); }
        return Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found after CreateButton", window_id
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_layer::{window_common, app::Win32ApiInternalState};

    use std::collections::HashMap;

    // --- Arrange helpers ---
    fn setup_state() -> (Arc<Win32ApiInternalState>, WindowId, window_common::NativeWindowData) {
        let internal_state = Win32ApiInternalState::new("TestApp".into()).unwrap();
        let win_id = internal_state.generate_window_id();
        let window_data = window_common::NativeWindowData {
            this_window_hwnd: window_common::HWND_INVALID,
            logical_window_id: win_id,
            treeview_state: None,
            control_hwnd_map: HashMap::new(),
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 0,
            layout_rules: None,
            label_severities: HashMap::new(),
            input_bg_colors: HashMap::new(),
            status_bar_font: None,
        };
        (internal_state, win_id, window_data)
    }

    #[test]
    fn test_handle_bn_clicked_returns_event() {
        // Arrange
        let win_id = WindowId(1);
        let hwnd = HWND(0x1234 as _);

        // Act
        let evt = handle_bn_clicked(win_id, 42, hwnd);

        // Assert
        assert_eq!(evt, AppEvent::ButtonClicked { window_id: win_id, control_id: 42 });
    }

    #[test]
    fn test_create_button_missing_window_returns_error() {
        // Arrange
        let (state, win_id, _data) = setup_state();
        // Do not insert window data into map

        // Act
        let result = handle_create_button_command(&state, win_id, 1, "Test".into());

        // Assert
        assert!(result.is_err());
    }
}
