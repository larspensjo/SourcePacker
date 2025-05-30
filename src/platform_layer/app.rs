use super::command_executor;
use super::control_treeview;
use super::dialog_handler;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{
    AppEvent, CheckState, MenuAction, MessageSeverity, PlatformCommand, PlatformEventHandler,
    TreeItemId, WindowConfig, WindowId,
};
use super::{types::MenuItemConfig, window_common};

use windows::{
    Win32::{
        Foundation::{FALSE, GetLastError, HINSTANCE, HWND, LPARAM, RECT, TRUE, WPARAM},
        System::Com::{CoInitializeEx, CoUninitialize}, // Only CoInitializeEx and CoUninitialize remain
        System::LibraryLoader::GetModuleHandleW,
        UI::Controls::{
            // Only common control initialization items remain
            ICC_TREEVIEW_CLASSES,
            INITCOMMONCONTROLSEX,
            InitCommonControlsEx,
        },
        // UI::Shell items are all removed
        UI::WindowsAndMessaging::*, // Main windowing messages and types remain
    },
    core::{HSTRING, PCWSTR}, // PWSTR removed
};

use std::collections::HashMap;
use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf; // Kept as some commands might still pass PathBufs
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
 */
pub(crate) struct Win32ApiInternalState {
    pub(crate) h_instance: HINSTANCE,
    pub(crate) next_window_id_counter: AtomicUsize,
    // Maps platform-agnostic `WindowId` to native `HWND` and associated window data.
    pub(crate) window_map: RwLock<HashMap<WindowId, window_common::NativeWindowData>>,
    // A weak reference to the event handler provided by the application logic.
    // Stored to be accessible by the WndProc. Weak to avoid cycles if event_handler holds PlatformInterface.
    pub(crate) event_handler: Mutex<Option<Weak<Mutex<dyn PlatformEventHandler>>>>,
    // The application name, used for window class registration.
    pub(crate) app_name_for_class: String,
    // Keeps track of active top-level windows. When this count reaches zero,
    // and `is_quitting` is true, the application exits.
    active_windows_count: AtomicUsize,
    is_quitting: AtomicUsize, // 0 = false, 1 = true (using usize for Atomic)
}

impl Win32ApiInternalState {
    /*
     * Creates a new instance of `Win32ApiInternalState`.
     * Initializes COM, common controls (specifically for TreeView), and
     * retrieves the application instance handle (`HINSTANCE`).
     * Returns a `PlatformResult` wrapping an `Arc` to the new state.
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
                window_map: RwLock::new(HashMap::new()),
                event_handler: Mutex::new(None),
                app_name_for_class,
                active_windows_count: AtomicUsize::new(0),
                is_quitting: AtomicUsize::new(0),
            }))
        }
    }

    pub(crate) fn generate_window_id(&self) -> WindowId {
        WindowId(self.next_window_id_counter.fetch_add(1, Ordering::Relaxed))
    }

    pub(crate) fn decrement_active_windows(&self) {
        let prev_count = self.active_windows_count.fetch_sub(1, Ordering::Relaxed);
        log::debug!(
            "Platform: Active window count decremented, was {}, now {}.",
            prev_count,
            prev_count - 1
        );
        if prev_count == 1 {
            log::debug!(
                "Platform: Last active window closed (or being destroyed), posting WM_QUIT."
            );
            unsafe { PostQuitMessage(0) };
        }
    }

    pub(crate) fn signal_quit_intent(&self) {
        self.is_quitting.store(1, Ordering::Relaxed);
        if self.active_windows_count.load(Ordering::Relaxed) == 0 {
            log::error!("Platform: Quit signaled with no active windows, posting WM_QUIT.");
            unsafe { PostQuitMessage(0) };
        }
    }

    // All _handle_..._dialog_impl methods have been moved to dialog_handler.rs

    /*
     * Executes a single platform command directly.
     * This method centralizes the handling of all platform commands, whether
     * they originate from the initial setup or from event handling by MyAppLogic.
     * It's called by the main run loop after dequeuing commands from MyAppLogic,
     * and can also be called directly for initial UI setup commands.
     */
    fn _execute_platform_command(self: &Arc<Self>, command: PlatformCommand) -> PlatformResult<()> {
        log::debug!("Platform: Executing command: {:?}", command);
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
            PlatformCommand::PopulateTreeView { window_id, items } => {
                command_executor::execute_populate_treeview(self, window_id, items)
            }
            PlatformCommand::UpdateTreeItemVisualState {
                window_id,
                item_id,
                new_state,
            } => command_executor::execute_update_tree_item_visual_state(
                self, window_id, item_id, new_state,
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
            PlatformCommand::UpdateStatusBarText {
                window_id,
                text,
                severity,
            } => command_executor::execute_update_status_bar_text(self, window_id, text, severity),
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
            PlatformCommand::QuitApplication => command_executor::execute_quit_application(self),
            PlatformCommand::CreateMainMenu {
                window_id,
                menu_items,
            } => command_executor::execute_create_main_menu(self, window_id, menu_items),
            PlatformCommand::CreateButton {
                window_id,
                control_id,
                text,
            } => command_executor::execute_create_button(self, window_id, control_id, text),
            PlatformCommand::CreateStatusBar {
                window_id,
                control_id,
                initial_text,
            } => command_executor::execute_create_status_bar(
                self,
                window_id,
                control_id,
                initial_text,
            ),
            PlatformCommand::CreateTreeView {
                window_id,
                control_id,
            } => command_executor::execute_create_treeview(self, window_id, control_id),
            PlatformCommand::SignalMainWindowUISetupComplete { window_id } => {
                command_executor::execute_signal_main_window_ui_setup_complete(self, window_id)
            }
            PlatformCommand::DefineLayout { window_id, rules } => {
                command_executor::execute_define_layout(self, window_id, rules)
            }
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
     * Returns a `PlatformResult` wrapping the new interface.
     */
    pub fn new(app_name_for_class: String) -> PlatformResult<Self> {
        let internal_state = Win32ApiInternalState::new(app_name_for_class)?;
        window_common::register_window_class(&internal_state)?;
        log::debug!(
            "Platform: Window class registration attempted during PlatformInterface::new()."
        );
        Ok(PlatformInterface { internal_state })
    }

    pub fn create_window(&self, config: WindowConfig) -> PlatformResult<WindowId> {
        let window_id = self.internal_state.generate_window_id();
        let preliminary_native_data = window_common::NativeWindowData {
            hwnd: HWND(std::ptr::null_mut()),
            id: window_id,
            treeview_state: None,
            controls: HashMap::new(),
            status_bar_current_text: String::new(),
            status_bar_current_severity: MessageSeverity::None,
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 30000,
            layout_rules: None,
        };
        self.internal_state
            .window_map
            .write()
            .map_err(|_| {
                PlatformError::OperationFailed(
                    "Failed to lock windows map for preliminary insert".into(),
                )
            })?
            .insert(window_id, preliminary_native_data);
        log::debug!(
            "Platform: Inserted preliminary NativeWindowData for WindowId {:?}",
            window_id
        );
        let hwnd = match window_common::create_native_window(
            &self.internal_state,
            window_id,
            &config.title,
            config.width,
            config.height,
        ) {
            Ok(h) => h,
            Err(e) => {
                self.internal_state
                    .window_map
                    .write()
                    .unwrap()
                    .remove(&window_id);
                log::debug!(
                    "Platform: Removed preliminary NativeWindowData for WindowId {:?} due to creation failure.",
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
        match self.internal_state.window_map.write() {
            Ok(mut windows_map_guard) => {
                if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                    window_data.hwnd = hwnd;
                    log::debug!(
                        "Platform: Updated HWND in NativeWindowData for WindowId {:?}. Status HWND is {:?}.",
                        window_id,
                        window_data.get_control_hwnd(window_common::ID_STATUS_BAR_CTRL)
                    );
                } else {
                    log::error!(
                        "Platform: CRITICAL - Preliminary NativeWindowData for WindowId {:?} vanished before HWND update.",
                        window_id
                    );
                    return Err(PlatformError::WindowCreationFailed(
                        "Failed to update HWND for preliminary window data: entry missing"
                            .to_string(),
                    ));
                }
            }
            Err(_) => {
                return Err(PlatformError::OperationFailed(
                    "Failed to lock windows map for HWND update".into(),
                ));
            }
        }
        self.internal_state
            .active_windows_count
            .fetch_add(1, Ordering::Relaxed);
        Ok(window_id)
    }

    pub fn execute_command(&self, command: PlatformCommand) -> PlatformResult<()> {
        self.internal_state._execute_platform_command(command)
    }

    /*
     * Starts the platform's main event loop.
     * This method takes ownership of the `event_handler` (e.g., MyAppLogic) and
     * continuously processes messages. Before checking for OS messages, it drains
     * and executes any commands enqueued in the event handler. It only returns
     * when the application is quitting (e.g., after `WM_QUIT` is posted).
     */
    pub fn run(
        &self,
        event_handler: Arc<Mutex<dyn PlatformEventHandler>>,
        initial_commands_to_execute: Vec<PlatformCommand>,
    ) -> PlatformResult<()> {
        *self.internal_state.event_handler.lock().unwrap() = Some(Arc::downgrade(&event_handler));

        log::debug!(
            "Platform: run() called. Processing {} initial UI commands before event loop.",
            initial_commands_to_execute.len()
        );

        // Execute initial UI setup commands before starting the message loop.
        for command in initial_commands_to_execute {
            log::trace!("Platform: Executing initial command: {:?}", command);
            if let Err(e) = self.internal_state._execute_platform_command(command) {
                log::error!(
                    "Platform: Error executing initial UI command: {:?}. Halting initialization.",
                    e
                );
                return Err(e);
            }
        }
        log::debug!("Platform: Initial UI commands processed successfully.");

        let app_logic_ref = event_handler;
        unsafe {
            let mut msg = MSG::default();
            loop {
                // Reset current highest severity for windows before processing new commands/events
                if let Ok(mut windows_map_guard) = self.internal_state.window_map.write() {
                    for (_id, window_data) in windows_map_guard.iter_mut() {
                        window_data.status_bar_current_severity = MessageSeverity::Information;
                        if window_data.status_bar_current_text.is_empty() {
                            window_data.status_bar_current_severity = MessageSeverity::None;
                        }
                    }
                }

                loop {
                    // Step 1: Dequeue a command.
                    let command_to_execute: Option<PlatformCommand> = {
                        match app_logic_ref.lock() {
                            Ok(mut logic_guard) => logic_guard.try_dequeue_command(),
                            Err(e) => {
                                log::error!(
                                    "Platform: Failed to lock MyAppLogic to dequeue command: {:?}. Skipping command processing for this cycle.",
                                    e
                                );
                                None
                            }
                        }
                    };

                    // Step 2: Execute the command if one was dequeued.
                    if let Some(command) = command_to_execute {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            log::error!("Platform: Error executing command from queue: {:?}", e);
                        }
                    } else {
                        // No more commands in the queue.
                        break;
                    }
                }

                // Process Windows messages.
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if result.0 == 0 {
                    log::debug!(
                        "Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop."
                    );
                    break;
                } else {
                    let last_error = GetLastError();
                    log::error!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    if self.internal_state.is_quitting.load(Ordering::Relaxed) == 1
                        && self
                            .internal_state
                            .active_windows_count
                            .load(Ordering::Relaxed)
                            == 0
                    {
                        log::debug!(
                            "Platform: GetMessageW error during intended quit sequence, treating as clean exit."
                        );
                        break;
                    }
                    return Err(PlatformError::OperationFailed(format!(
                        "GetMessageW failed: {}",
                        windows::core::Error::from_win32()
                    )));
                }
            }
        }
        // Application quit
        if let Ok(mut handler_guard) = app_logic_ref.lock() {
            handler_guard.on_quit();
        } else {
            log::error!("Platform: Failed to lock MyAppLogic for on_quit call.");
        }
        *self.internal_state.event_handler.lock().unwrap() = None;
        log::debug!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

// All dialog-specific structs (like InputDialogData) and their associated procs/helpers
// have been moved to dialog_handler.rs.

#[cfg(test)]
mod app_tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    // This helper was specific to app.rs, but its functionality is now in dialog_handler.rs.
    // For tests that might have relied on it being in app.rs scope, it's kept here.
    // In a real scenario, tests would be updated or new ones created for dialog_handler.
    pub fn pathbuf_from_buf(buffer: &[u16]) -> PathBuf {
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let path_os_string = OsString::from_wide(&buffer[..len]);
        PathBuf::from(path_os_string)
    }

    #[test]
    fn roundtrip_simple() {
        let mut wide: Vec<u16> = "C:\\temp\\file.txt".encode_utf16().collect();
        wide.push(0);
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"C:\\temp\\file.txt"));
    }

    #[test]
    fn no_null_terminator() {
        let wide: Vec<u16> = "D:\\data\\incomplete".encode_utf16().collect();
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"D:\\data\\incomplete"));
    }
}
