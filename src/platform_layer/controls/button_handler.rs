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
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        BS_PUSHBUTTON, CreateWindowExW, DestroyWindow, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE,
        WS_CHILD, WS_VISIBLE,
    },
};
use windows::core::{HSTRING, PCWSTR};

const WC_BUTTON: PCWSTR = windows::core::w!("BUTTON");

/*
 * Creates a native push button and registers the resulting HWND in the
 * window's `NativeWindowData`. Fails if the window or control ID are
 * invalid or already in use. This function uses a read-create-write pattern
 * to minimize lock contention on the global window map.
 *
 * First, it acquires a read lock to verify that the control doesn't already
 * exist and to get the parent HWND. Then, it creates the native button
 * control without holding any locks. Finally, it acquires a write lock briefly
 * to register the new control, checking for race conditions.
 */
pub(crate) fn handle_create_button_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    text: String,
) -> PlatformResult<()> {
    log::debug!(
        "ButtonHandler: handle_create_button_command for WinID {window_id:?}, ControlID {control_id}, Text: '{text}'"
    );

    // Phase 1: Read-only pre-checks.
    // Get the parent HWND for creation while holding only a read lock.
    let hwnd_parent_for_creation =
        internal_state.with_window_data_read(window_id, |window_data| {
            if window_data.has_control(control_id) {
                log::warn!(
                    "ButtonHandler: Button with ID {control_id} already exists for window {window_id:?}."
                );
                return Err(PlatformError::OperationFailed(format!(
                    "Button with ID {control_id} already exists for window {window_id:?}"
                )));
            }

            let hwnd_parent = window_data.get_hwnd();
            if hwnd_parent.is_invalid() {
                log::error!(
                    "ButtonHandler: Parent HWND invalid for CreateButton (WinID: {window_id:?})"
                );
                return Err(PlatformError::InvalidHandle(format!(
                    "Parent HWND invalid for CreateButton (WinID: {window_id:?})"
                )));
            }
            Ok(hwnd_parent)
        })?;

    // Phase 2: Create the native control without holding any locks.
    let h_instance = internal_state.h_instance();
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

    // Phase 3: Acquire a write lock only to register the new HWND.
    internal_state.with_window_data_write(window_id, |window_data| {
        // Re-check for a race condition where another thread created the control
        // while we were not holding a lock.
        if window_data.has_control(control_id) {
            log::warn!(
                "ButtonHandler: Control ID {control_id} was created concurrently for window {window_id:?}. Destroying new HWND."
            );
            unsafe {
                // Safely ignore error if window is already gone.
                DestroyWindow(hwnd_button).ok();
            }
            return Err(PlatformError::OperationFailed(format!(
                "Button with ID {control_id} was created concurrently for window {window_id:?}"
            )));
        }

        window_data.register_control_hwnd(control_id, hwnd_button);
        log::debug!(
            "ButtonHandler: Created button '{text}' (ID {control_id}) for window {window_id:?} with HWND {hwnd_button:?}"
        );
        Ok(())
    })
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
        "ButtonHandler: BN_CLICKED for ID {control_id} (HWND {hwnd_control:?}) in WinID {window_id:?}"
    );
    AppEvent::ButtonClicked {
        window_id,
        control_id,
    }
}
