use super::command_executor;
use super::controls::treeview_handler;
use super::dialog_handler;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{PlatformCommand, PlatformEventHandler, WindowConfig, WindowId};
use super::window_common;

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE},
        System::Com::{CoInitializeEx, CoUninitialize},
        System::LibraryLoader::GetModuleHandleW,
        UI::Controls::{ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx},
        UI::WindowsAndMessaging::*,
    },
    core::PCWSTR,
};

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, RwLock, Weak,
    atomic::{AtomicUsize, Ordering},
};

/*
 * Internal state for the Win32 platform layer.
 *
 * This struct holds all necessary Win32 handles and mappings required to manage
 * the application's lifecycle and UI elements. It is managed by `PlatformInterface`
 * and accessed by the `WndProc` and command handlers.
 * It is passed as user data to some functions.
 * TOOD: I think all member should be made private here. Instead, accessor functions should be provided.
 */
pub(crate) struct Win32ApiInternalState {
    pub(crate) h_instance: HINSTANCE,
    pub(crate) next_window_id_counter: AtomicUsize, // For generating unique WindowIds
    // Central registry for all active windows, mapping WindowId to its native state.
    pub(crate) active_windows: RwLock<HashMap<WindowId, window_common::NativeWindowData>>,
    pub(crate) application_event_handler: Mutex<Option<Weak<Mutex<dyn PlatformEventHandler>>>>,
    // The application name, used for window class registration.
    pub(crate) app_name_for_class: String,
    is_quitting: AtomicUsize, // 0 = false, 1 = true
}

impl Win32ApiInternalState {
    /*
     * Creates a new instance of `Win32ApiInternalState`.
     * Initializes COM, common controls, and retrieves the application instance handle.
     */
    fn new(app_name_for_class: String) -> PlatformResult<Arc<Self>> {
        unsafe {
            let hr = CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            if hr.is_err()
                && hr != windows::Win32::Foundation::S_FALSE
                && hr != windows::Win32::Foundation::RPC_E_CHANGED_MODE
            {
                return Err(PlatformError::InitializationFailed(format!(
                    "CoInitializeEx failed: {:?}",
                    hr
                )));
            }

            let icex = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_TREEVIEW_CLASSES,
            };
            if !InitCommonControlsEx(&icex).as_bool() {
                log::error!(
                    "Warning: InitCommonControlsEx for TreeView failed. Error: {:?}",
                    GetLastError()
                );
            }

            let h_instance = HINSTANCE(GetModuleHandleW(PCWSTR::null())?.0);
            Ok(Arc::new(Self {
                h_instance,
                next_window_id_counter: AtomicUsize::new(1),
                active_windows: RwLock::new(HashMap::new()),
                application_event_handler: Mutex::new(None),
                app_name_for_class,
                is_quitting: AtomicUsize::new(0),
            }))
        }
    }

    /*
     * Generates a new unique `WindowId`.
     */
    pub(crate) fn generate_unique_window_id(&self) -> WindowId {
        WindowId(self.next_window_id_counter.fetch_add(1, Ordering::Relaxed))
    }

    /*
     * Called typically after a window is removed from the `active_windows` map
     * (e.g., during WM_DESTROY processing). If no windows remain active and a quit
     * has been signaled or if this was the last window, it posts WM_QUIT.
     * The `is_quitting` flag ensures that if `QuitApplication` was called
     * when multiple windows were open, the app quits when the *last* one closes.
     */
    pub(crate) fn check_if_should_quit_after_window_close(&self) {
        let no_active_windows = self.active_windows.read().map_or_else(
            |poisoned_err| {
                log::error!("Win32ApiInternalState: Poisoned RwLock on active_windows during quit check: {:?}", poisoned_err);
                false
            },
            |guard| guard.is_empty()
        );

        if no_active_windows {
            log::debug!(
                "Platform: Last active window closed (or was already closed and quit signaled). Posting WM_QUIT."
            );
            unsafe { PostQuitMessage(0) };
        }
    }

    /*
     * Executes a single platform command.
     * Delegates to specific handlers in `command_executor`, `dialog_handler`,
     * or now directly to control handlers (e.g., `treeview_handler`).
     */
    fn _execute_platform_command(self: &Arc<Self>, command: PlatformCommand) -> PlatformResult<()> {
        log::trace!("Platform: Executing command: {:?}", command);
        match command {
            PlatformCommand::SetWindowTitle { window_id, title } => {
                command_executor::execute_set_window_title(self, window_id, &title)
            }
            PlatformCommand::ShowWindow { window_id } => {
                command_executor::execute_show_window(self, window_id, true)
            }
            PlatformCommand::CloseWindow { window_id } => {
                command_executor::execute_close_window(self, window_id)
            }
            PlatformCommand::PopulateTreeView {
                window_id,
                control_id,
                items,
            } => command_executor::execute_populate_treeview(self, window_id, control_id, items),
            PlatformCommand::UpdateTreeItemVisualState {
                window_id,
                control_id,
                item_id,
                new_state,
            } => command_executor::execute_update_tree_item_visual_state(
                self, window_id, control_id, item_id, new_state,
            ),
            PlatformCommand::ShowSaveFileDialog {
                window_id,
                title,
                default_filename,
                filter_spec,
                initial_dir,
            } => dialog_handler::handle_show_save_file_dialog_command(
                self,
                window_id,
                title,
                default_filename,
                filter_spec,
                initial_dir,
            ),
            PlatformCommand::ShowOpenFileDialog {
                window_id,
                title,
                filter_spec,
                initial_dir,
            } => dialog_handler::handle_show_open_file_dialog_command(
                self,
                window_id,
                title,
                filter_spec,
                initial_dir,
            ),
            PlatformCommand::ShowProfileSelectionDialog {
                window_id,
                available_profiles,
                title,
                prompt,
                emphasize_create_new,
            } => dialog_handler::handle_show_profile_selection_dialog_command(
                self,
                window_id,
                available_profiles,
                title,
                prompt,
                emphasize_create_new,
            ),
            PlatformCommand::ShowInputDialog {
                window_id,
                title,
                prompt,
                default_text,
                context_tag,
            } => dialog_handler::handle_show_input_dialog_command(
                self,
                window_id,
                title,
                prompt,
                default_text,
                context_tag,
            ),
            PlatformCommand::ShowFolderPickerDialog {
                window_id,
                title,
                initial_dir,
            } => dialog_handler::handle_show_folder_picker_dialog_command(
                self,
                window_id,
                title,
                initial_dir,
            ),
            PlatformCommand::SetControlEnabled {
                window_id,
                control_id,
                enabled,
            } => {
                command_executor::execute_set_control_enabled(self, window_id, control_id, enabled)
            }
            PlatformCommand::QuitApplication => {
                // Set an atomic flag indicating that a quit has been requested.
                // This helps the message loop make a final decision if GetMessageW itself errors out
                // but there are no windows left and a quit was intended.
                self.is_quitting.store(1, Ordering::Relaxed);
                command_executor::execute_quit_application()
            }
            PlatformCommand::CreateMainMenu {
                window_id,
                menu_items,
            } => command_executor::execute_create_main_menu(self, window_id, menu_items),
            PlatformCommand::CreateButton {
                window_id,
                control_id,
                text,
            } => super::controls::button_handler::handle_create_button_command(
                self,
                window_id,
                control_id,
                text,
            ),
            PlatformCommand::CreateTreeView {
                window_id,
                control_id,
            } => treeview_handler::handle_create_treeview_command(self, window_id, control_id),
            PlatformCommand::SignalMainWindowUISetupComplete { window_id } => {
                command_executor::execute_signal_main_window_ui_setup_complete(self, window_id)
            }
            PlatformCommand::DefineLayout { window_id, rules } => {
                command_executor::execute_define_layout(self, window_id, rules)
            }
            PlatformCommand::CreatePanel {
                window_id,
                parent_control_id,
                control_id: panel_id,
            } => {
                command_executor::execute_create_panel(self, window_id, parent_control_id, panel_id)
            }
            PlatformCommand::CreateLabel {
                window_id,
                parent_panel_id,
                control_id: label_id,
                initial_text,
                class,
            } => super::controls::label_handler::handle_create_label_command(
                self,
                window_id,
                parent_panel_id,
                label_id,
                initial_text,
                class,
            ),
            PlatformCommand::UpdateLabelText {
                window_id,
                control_id: label_id,
                text,
                severity,
            } => super::controls::label_handler::handle_update_label_text_command(
                self, window_id, label_id, text, severity,
            ),
            PlatformCommand::RedrawTreeItem {
                window_id,
                control_id,
                item_id,
            } => treeview_handler::handle_redraw_tree_item_command(
                self, window_id, control_id, item_id,
            ),
            PlatformCommand::ExpandVisibleTreeItems { window_id, control_id } => {
                command_executor::execute_expand_visible_tree_items(
                    self,
                    window_id,
                    control_id,
                )
            }
            PlatformCommand::ExpandAllTreeItems { window_id, control_id } => {
                command_executor::execute_expand_all_tree_items(
                    self,
                    window_id,
                    control_id,
                )
            }
            PlatformCommand::CreateInput {
                window_id,
                parent_control_id,
                control_id,
                initial_text,
            } => command_executor::execute_create_input(
                self,
                window_id,
                parent_control_id,
                control_id,
                initial_text,
            ),
            PlatformCommand::SetInputText {
                window_id,
                control_id,
                text,
            } => command_executor::execute_set_input_text(self, window_id, control_id, text),
            PlatformCommand::SetInputBackgroundColor {
                window_id,
                control_id,
                color,
            } => command_executor::execute_set_input_background_color(self, window_id, control_id, color),
        }
    }
}

impl Drop for Win32ApiInternalState {
    fn drop(&mut self) {
        log::debug!("Platform: Win32ApiInternalState dropped, calling CoUninitialize.");
        unsafe { CoUninitialize() };
    }
}

/*
 * Provides the main interface for the application to interact with the
 * underlying Win32 platform. It handles window creation, command execution,
 * and running the main event loop.
 */
pub struct PlatformInterface {
    internal_state: Arc<Win32ApiInternalState>,
}

impl PlatformInterface {
    /*
     * Creates a new `PlatformInterface`.
     * Initializes the internal Win32 state and registers the main window class.
     */
    pub fn new(app_name_for_class: String) -> PlatformResult<Self> {
        let internal_state = Win32ApiInternalState::new(app_name_for_class)?;
        window_common::register_window_class(&internal_state)?;
        log::debug!(
            "Platform: Window class registration attempted during PlatformInterface::new()."
        );
        Ok(PlatformInterface { internal_state })
    }

    /*
     * A `WindowId` is generated and associated with the native window's state.
     * The window is not shown until a `PlatformCommand::ShowWindow` is received.
     */
    pub fn create_window(&self, config: WindowConfig) -> PlatformResult<WindowId> {
        let window_id = self.internal_state.generate_unique_window_id();

        // Create a preliminary NativeWindowData. It will be fully populated after HWND creation.
        let preliminary_native_data = window_common::NativeWindowData::new(window_id);

        // Insert preliminary data before creating the native window.
        // This ensures that if WM_NCCREATE is processed for this window_id,
        // its NativeWindowData entry already exists.
        self.internal_state
            .active_windows
            .write()
            .map_err(|e| {
                log::error!(
                    "Platform: Failed to lock active_windows for preliminary insert: {:?}",
                    e
                );
                PlatformError::OperationFailed(
                    "Failed to lock active_windows map for preliminary insert".into(),
                )
            })?
            .insert(window_id, preliminary_native_data);
        log::debug!(
            "Platform: Inserted preliminary NativeWindowData for WindowId {:?}",
            window_id
        );

        // Now, create the actual native window.
        let hwnd = match window_common::create_native_window(
            &self.internal_state, // Pass Arc<Win32ApiInternalState>
            window_id,            // Pass the generated WindowId
            &config.title,
            config.width,
            config.height,
        ) {
            Ok(h) => h,
            Err(e) => {
                // If native window creation fails, remove the preliminary data.
                if let Ok(mut guard) = self.internal_state.active_windows.write() {
                    guard.remove(&window_id);
                } else {
                    log::error!(
                        "Platform: Failed to lock active_windows for cleanup after window creation failure for WinID {:?}",
                        window_id
                    );
                }
                log::debug!(
                    "Platform: Removed (or attempted to remove) preliminary NativeWindowData for WindowId {:?} due to creation failure.",
                    window_id
                );
                return Err(e);
            }
        };
        log::debug!(
            "Platform: Native window created with HWND {:?} for WindowId {:?}",
            hwnd,
            window_id
        );

        // Update the NativeWindowData with the actual HWND.
        // This is done after create_native_window returns successfully.
        match self.internal_state.active_windows.write() {
            Ok(mut windows_map_guard) => {
                if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                    window_data.set_hwnd(hwnd); // Set the actual HWND
                    log::debug!(
                        "Platform: Updated HWND in NativeWindowData for WindowId {:?}.",
                        window_id,
                    );
                } else {
                    // This should ideally not happen if preliminary insert was successful
                    // and no other thread removed it.
                    log::error!(
                        "Platform: CRITICAL - Preliminary NativeWindowData for WindowId {:?} vanished before HWND update.",
                        window_id
                    );
                    // Attempt to destroy the orphaned window if HWND is valid
                    if !hwnd.is_invalid() {
                        unsafe {
                            DestroyWindow(hwnd).ok();
                        }
                    }
                    return Err(PlatformError::WindowCreationFailed(
                        "Failed to update HWND for preliminary window data: entry missing"
                            .to_string(),
                    ));
                }
            }
            Err(e) => {
                log::error!(
                    "Platform: Failed to lock active_windows for HWND update: {:?}",
                    e
                );
                // Attempt to destroy the orphaned window if HWND is valid
                if !hwnd.is_invalid() {
                    unsafe {
                        DestroyWindow(hwnd).ok();
                    }
                }
                return Err(PlatformError::OperationFailed(
                    "Failed to lock active_windows map for HWND update".into(),
                ));
            }
        }
        Ok(window_id)
    }

    /*
     * Takes the application's event handler and a list of initial commands.
     * Processes initial commands, then enters the message loop, dequeuing and
     * executing commands from the event handler before processing OS messages.
     * Returns when the application quits.
     */
    pub fn main_event_loop(
        &self,
        event_handler_param: Arc<Mutex<dyn PlatformEventHandler>>,
        initial_commands_to_execute: Vec<PlatformCommand>,
    ) -> PlatformResult<()> {
        *self
            .internal_state
            .application_event_handler
            .lock()
            .map_err(|e| {
                log::error!(
                    "Platform: Failed to lock application_event_handler to set it: {:?}",
                    e
                );
                PlatformError::OperationFailed("Failed to set application event handler".into())
            })? = Some(Arc::downgrade(&event_handler_param));

        log::debug!(
            "Platform: run() called. Processing {} initial UI commands before event loop.",
            initial_commands_to_execute.len()
        );

        for command in initial_commands_to_execute {
            log::debug!("Platform: Executing initial command: {:?}", command);
            if let Err(e) = self.internal_state._execute_platform_command(command) {
                log::error!(
                    "Platform: Error executing initial UI command: {:?}. Halting initialization.",
                    e
                );
                return Err(e);
            }
        }
        log::debug!("Platform: Initial UI commands processed successfully.");

        let app_logic_ref_for_loop = event_handler_param;
        unsafe {
            let mut msg = MSG::default();
            loop {
                // Process all pending commands from app logic first
                loop {
                    let command_to_execute: Option<PlatformCommand> = {
                        match app_logic_ref_for_loop.lock() {
                            Ok(mut logic_guard) => logic_guard.try_dequeue_command(),
                            Err(e) => {
                                log::error!(
                                    "Platform: Failed to lock application logic to dequeue command: {:?}. Skipping command processing for this cycle.",
                                    e
                                );
                                None // Avoid panic, try again next cycle
                            }
                        }
                    };

                    if let Some(command) = command_to_execute {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            log::error!("Platform: Error executing command from queue: {:?}", e);
                            // Decide if error is fatal. For now, continue.
                        }
                    } else {
                        break; // No more commands from app logic, proceed to OS messages
                    }
                }

                // Then process OS messages
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    // Regular message
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if result.0 == 0 {
                    // WM_QUIT
                    log::debug!(
                        "Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop."
                    );
                    break;
                } else {
                    // Error from GetMessageW (result.0 == -1)
                    let last_error = GetLastError();
                    log::error!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    // Check if we should break despite error (e.g., if quitting and no windows)
                    let should_still_break =
                        self.internal_state.is_quitting.load(Ordering::Relaxed) == 1
                            && self
                                .internal_state
                                .active_windows
                                .read()
                                .map_or(false, |g| g.is_empty());
                    if should_still_break {
                        log::debug!(
                            "Platform: GetMessageW error during intended quit sequence with no windows, treating as clean exit."
                        );
                        break;
                    }
                    return Err(PlatformError::OperationFailed(format!(
                        "GetMessageW failed: {}",
                        windows::core::Error::from_win32() // Converts last error to windows::core::Error
                    )));
                }
            }
        }
        // Call on_quit on the event handler
        if let Ok(mut handler_guard) = app_logic_ref_for_loop.lock() {
            handler_guard.on_quit();
        } else {
            log::error!("Platform: Failed to lock application logic for on_quit call.");
        }
        // Clear the event handler reference
        match self.internal_state.application_event_handler.lock() {
            Ok(mut guard) => *guard = None,
            Err(e) => {
                log::error!(
                    "Platform: Failed to lock application_event_handler to clear it (poisoned): {:?}",
                    e
                );
            }
        }
        log::debug!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

#[cfg(test)]
mod app_tests {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    // Helper function to create PathBuf from a slice of u16 (wide char buffer)
    // This is useful when dealing with paths from Win32 API calls.
    pub fn pathbuf_from_buf(buffer: &[u16]) -> PathBuf {
        // Find the first null terminator, or use the whole buffer if none is found.
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let path_os_string = OsString::from_wide(&buffer[..len]);
        PathBuf::from(path_os_string)
    }

    #[test]
    fn roundtrip_simple() {
        let mut wide: Vec<u16> = "C:\\temp\\file.txt".encode_utf16().collect();
        wide.push(0); // Add null terminator
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"C:\\temp\\file.txt"));
    }

    #[test]
    fn no_null_terminator() {
        let wide: Vec<u16> = "D:\\data\\incomplete".encode_utf16().collect();
        // No null terminator added
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"D:\\data\\incomplete"));
    }
}
