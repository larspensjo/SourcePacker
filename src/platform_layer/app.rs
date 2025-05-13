use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, PlatformCommand, PlatformEventHandler, WindowConfig, WindowId};
use super::window_common; // For window creation and WndProc // For TreeView specific command handling

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Controls::{
                Dialogs::{
                    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER,
                    OFN_EXTENSIONDIFFERENT, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
                    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
                },
                ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
            },
            WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, MSG, PostQuitMessage, TranslateMessage,
            },
        },
    },
    core::{HSTRING, PCWSTR},
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, RwLock, Weak,
    atomic::{AtomicUsize, Ordering},
};

/// Internal state for the Win32 platform layer.
///
/// This struct holds all necessary Win32 handles and mappings required to manage
/// the application's lifecycle and UI elements. It is managed by `PlatformInterface`
/// and accessed by the `WndProc` and command handlers.
/// It is passed as user data to some functions.
pub(crate) struct Win32ApiInternalState {
    pub(crate) h_instance: HINSTANCE,
    pub(crate) next_window_id_counter: AtomicUsize,
    /// Maps platform-agnostic `WindowId` to native `HWND` and associated window data.
    pub(crate) window_map: RwLock<HashMap<WindowId, window_common::NativeWindowData>>,
    /// A weak reference to the event handler provided by the application logic.
    /// Stored to be accessible by the WndProc. Weak to avoid cycles if event_handler holds PlatformInterface.
    pub(crate) event_handler: Mutex<Option<Weak<Mutex<dyn PlatformEventHandler>>>>,
    /// The application name, used for window class registration.
    pub(crate) app_name_for_class: String,
    /// Keeps track of active top-level windows. When this count reaches zero,
    /// and `is_quitting` is true, the application exits.
    active_windows_count: AtomicUsize,
    is_quitting: AtomicUsize, // 0 = false, 1 = true (using usize for Atomic)
}

impl Win32ApiInternalState {
    fn new(app_name_for_class: String) -> PlatformResult<Arc<Self>> {
        unsafe {
            // Initialize COM for the current thread.
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err()
                && hr != windows::Win32::Foundation::S_FALSE
                && hr != windows::Win32::Foundation::RPC_E_CHANGED_MODE
            {
                return Err(PlatformError::InitializationFailed(format!(
                    "CoInitializeEx failed: {:?}",
                    hr
                )));
            }

            // Initialize Common Controls (specifically for TreeView).
            let icex = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_TREEVIEW_CLASSES,
            };
            if !InitCommonControlsEx(&icex).as_bool() {
                // This is not necessarily fatal, but good to log.
                eprintln!(
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
        println!(
            "Platform: Active window count decremented, was {}, now {}.",
            prev_count,
            prev_count - 1
        );

        if prev_count == 1 {
            println!("Platform: Last active window closed (or being destroyed), posting WM_QUIT.");
            unsafe { PostQuitMessage(0) };
        }
    }

    pub(crate) fn signal_quit_intent(&self) {
        self.is_quitting.store(1, Ordering::Relaxed);
        if self.active_windows_count.load(Ordering::Relaxed) == 0 {
            println!("Platform: Quit signaled with no active windows, posting WM_QUIT.");
            unsafe { PostQuitMessage(0) };
        }
    }

    /// Look up the HWND for a given WindowId, or return an error if not found.
    fn get_hwnd_owner(&self, window_id: WindowId) -> PlatformResult<HWND> {
        // 1) Try to acquire a read-lock on the windows map
        let windows_guard = self.window_map.read().map_err(|_| {
            PlatformError::OperationFailed("Failed to acquire read lock on windows map".into())
        })?;

        // 2) Find the entry, or return InvalidHandle if absent
        windows_guard
            .get(&window_id)
            .map(|data| data.hwnd)
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "WindowId {:?} not found for get_hwnd_owner",
                    window_id
                ))
            })
    }

    /// Centralized logic for showing the save file dialog.
    /// This method is called by both PlatformInterface::execute_command and by
    /// Win32ApiInternalState::process_commands_from_event_handler.
    /// It takes `&Arc<Self>` because `process_commands_from_event_handler` might be called recursively.
    fn _handle_show_save_file_dialog_impl(
        self: &Arc<Self>, // match how process_commands_from_event_handler is called
        window_id: WindowId,
        title: String,
        default_filename: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        let hwnd_owner = self.get_hwnd_owner(window_id)?;

        let mut file_buffer: Vec<u16> = vec![0; 2048];
        if !default_filename.is_empty() {
            let default_name_utf16: Vec<u16> = default_filename.encode_utf16().collect();
            let len_to_copy = std::cmp::min(default_name_utf16.len(), file_buffer.len() - 1);
            file_buffer[..len_to_copy].copy_from_slice(&default_name_utf16[..len_to_copy]);
        }

        let title_hstring = HSTRING::from(title);
        let filter_utf16: Vec<u16> = filter_spec.encode_utf16().collect();

        let initial_dir_hstring = initial_dir.map(|p| HSTRING::from(p.to_string_lossy().as_ref()));
        let initial_dir_pcwstr = initial_dir_hstring
            .as_ref()
            .map_or(PCWSTR::null(), |h| PCWSTR(h.as_ptr()));

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd_owner,
            lpstrFile: windows::core::PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: PCWSTR(filter_utf16.as_ptr()),
            lpstrTitle: PCWSTR(title_hstring.as_ptr()),
            lpstrInitialDir: initial_dir_pcwstr, // Use it here
            Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR,
            ..Default::default()
        };

        let save_result = unsafe { GetSaveFileNameW(&mut ofn) }.as_bool();
        let mut path_result: Option<PathBuf> = None;

        if save_result {
            let len = file_buffer
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_buffer.len());
            let path_str = String::from_utf16_lossy(&file_buffer[..len]);
            path_result = Some(PathBuf::from(path_str));
            println!(
                "Platform: Save dialog returned path: {:?}",
                path_result.as_ref().unwrap()
            );
        } else {
            let error_code = unsafe { CommDlgExtendedError() };
            if error_code != windows::Win32::UI::Controls::Dialogs::COMMON_DLG_ERRORS(0) {
                eprintln!(
                    "Platform: GetSaveFileNameW failed. CommDlgExtendedError: {:?}",
                    error_code
                );
            } else {
                println!("Platform: Save dialog cancelled by user.");
            }
        }

        let event = AppEvent::FileSaveDialogCompleted {
            window_id,
            result: path_result,
        };

        // Send the event to AppLogic and process any commands it returns
        let commands_from_dialog_completion = if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event)
            } else {
                eprintln!("Platform: Failed to lock event handler after save dialog.");
                vec![]
            }
        } else {
            eprintln!(
                "Platform: Event handler not available after save dialog (it might not have been set yet if called very early, or already dropped)."
            );
            vec![]
        };

        if !commands_from_dialog_completion.is_empty() {
            // Call process_commands_from_event_handler on Arc<Self>
            self.process_commands_from_event_handler(commands_from_dialog_completion);
        }
        Ok(())
    }

    fn _handle_show_open_file_dialog_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        let hwnd_owner = self.get_hwnd_owner(window_id)?;

        let mut file_buffer: Vec<u16> = vec![0; 2048]; // Buffer for the selected file path

        let title_hstring = HSTRING::from(title);
        let filter_utf16: Vec<u16> = filter_spec.encode_utf16().collect();
        let initial_dir_hstring = initial_dir.map(|p| HSTRING::from(p.to_string_lossy().as_ref()));
        let initial_dir_pcwstr = initial_dir_hstring
            .as_ref()
            .map_or(PCWSTR::null(), |h| PCWSTR(h.as_ptr()));

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd_owner,
            lpstrFile: windows::core::PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: PCWSTR(filter_utf16.as_ptr()),
            lpstrTitle: PCWSTR(title_hstring.as_ptr()),
            lpstrInitialDir: initial_dir_pcwstr,
            Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };

        let open_result = unsafe { GetOpenFileNameW(&mut ofn) }.as_bool();
        let mut path_result: Option<PathBuf> = None;

        if open_result {
            let len = file_buffer
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_buffer.len());
            let path_str = String::from_utf16_lossy(&file_buffer[..len]);
            path_result = Some(PathBuf::from(path_str));
            println!(
                "Platform: Open dialog returned path: {:?}",
                path_result.as_ref().unwrap()
            );
        } else {
            let error_code = unsafe { CommDlgExtendedError() };
            if error_code != windows::Win32::UI::Controls::Dialogs::COMMON_DLG_ERRORS(0) {
                eprintln!(
                    "Platform: GetOpenFileNameW failed. CommDlgExtendedError: {:?}",
                    error_code
                );
            } else {
                println!("Platform: Open dialog cancelled by user.");
            }
        }

        let event = AppEvent::FileOpenDialogCompleted {
            // Use the new specific event
            window_id,
            result: path_result,
        };

        // Send the event to AppLogic and process any commands it returns
        let commands_from_dialog_completion = if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event)
            } else {
                eprintln!("Platform: Failed to lock event handler after open dialog.");
                vec![]
            }
        } else {
            eprintln!("Platform: Event handler not available after open dialog.");
            vec![]
        };

        if !commands_from_dialog_completion.is_empty() {
            self.process_commands_from_event_handler(commands_from_dialog_completion);
        }
        Ok(())
    }

    pub fn process_commands_from_event_handler(self: &Arc<Self>, commands: Vec<PlatformCommand>) {
        for cmd in commands {
            let result = match cmd {
                PlatformCommand::SetWindowTitle { window_id, title } => {
                    window_common::set_window_title(self, window_id, &title)
                }
                PlatformCommand::ShowWindow { window_id } => {
                    window_common::show_window(self, window_id, true)
                }
                PlatformCommand::CloseWindow { window_id } => {
                    window_common::destroy_native_window(self, window_id)
                }
                PlatformCommand::PopulateTreeView { window_id, items } => {
                    control_treeview::populate_treeview(self, window_id, items)
                }
                PlatformCommand::UpdateTreeItemVisualState {
                    window_id,
                    item_id,
                    new_state,
                } => control_treeview::update_treeview_item_visual_state(
                    self, window_id, item_id, new_state,
                ),
                PlatformCommand::ShowSaveFileDialog {
                    window_id,
                    title,
                    default_filename,
                    filter_spec,
                    initial_dir, // Add initial_dir here
                } => self._handle_show_save_file_dialog_impl(
                    window_id,
                    title,
                    default_filename,
                    filter_spec,
                    initial_dir, // Pass it along
                ),
                PlatformCommand::ShowOpenFileDialog {
                    // New case
                    window_id,
                    title,
                    filter_spec,
                    initial_dir,
                } => self._handle_show_open_file_dialog_impl(
                    window_id,
                    title,
                    filter_spec,
                    initial_dir,
                ),
            };

            if let Err(e) = result {
                eprintln!(
                    "Platform: Error executing command from event handler: {:?}",
                    e
                );
            }
        }
    }
}

impl Drop for Win32ApiInternalState {
    fn drop(&mut self) {
        println!("Platform: Win32ApiInternalState dropped, calling CoUninitialize.");
        unsafe { CoUninitialize() };
    }
}

/// The primary interface to the platform abstraction layer.
pub struct PlatformInterface {
    internal_state: Arc<Win32ApiInternalState>,
}

impl PlatformInterface {
    pub fn new(app_name_for_class: String) -> PlatformResult<Self> {
        let internal_state = Win32ApiInternalState::new(app_name_for_class)?;
        window_common::register_window_class(&internal_state)?;
        println!("Platform: Window class registration attempted during PlatformInterface::new().");
        Ok(PlatformInterface { internal_state })
    }

    pub fn create_window(&self, config: WindowConfig) -> PlatformResult<WindowId> {
        let window_id = self.internal_state.generate_window_id();

        let preliminary_native_data = window_common::NativeWindowData {
            hwnd: HWND(std::ptr::null_mut()), // Placeholder, will be updated after creation
            id: window_id,
            treeview_state: None,
            hwnd_button_generate: None,
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

        println!(
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
                // If window creation fails, remove the preliminary entry
                self.internal_state
                    .window_map
                    .write()
                    .unwrap()
                    .remove(&window_id);
                println!(
                    "Platform: Removed preliminary NativeWindowData for WindowId {:?} due to creation failure.",
                    window_id
                );
                return Err(e);
            }
        };

        println!(
            "Platform: Native window created with HWND {:?} for WindowId {:?}",
            hwnd, window_id
        );

        match self.internal_state.window_map.write() {
            Ok(mut windows_map_guard) => {
                if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                    window_data.hwnd = hwnd;
                    println!(
                        "Platform: Updated HWND in NativeWindowData for WindowId {:?}. Button HWND is {:?}.",
                        window_id, window_data.hwnd_button_generate
                    );
                } else {
                    eprintln!(
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
        match command {
            PlatformCommand::SetWindowTitle { window_id, title } => {
                window_common::set_window_title(&self.internal_state, window_id, &title)
            }
            PlatformCommand::ShowWindow { window_id } => {
                window_common::show_window(&self.internal_state, window_id, true)
            }
            PlatformCommand::CloseWindow { window_id } => {
                window_common::send_close_message(&self.internal_state, window_id)
            }
            PlatformCommand::PopulateTreeView { window_id, items } => {
                control_treeview::populate_treeview(&self.internal_state, window_id, items)
            }
            PlatformCommand::UpdateTreeItemVisualState {
                window_id,
                item_id,
                new_state,
            } => control_treeview::update_treeview_item_visual_state(
                &self.internal_state,
                window_id,
                item_id,
                new_state,
            ),
            PlatformCommand::ShowSaveFileDialog {
                window_id,
                title,
                default_filename,
                filter_spec,
                initial_dir, // Add initial_dir here
            } => {
                // Call the centralized internal handler via the internal_state Arc
                self.internal_state._handle_show_save_file_dialog_impl(
                    window_id,
                    title,
                    default_filename,
                    filter_spec,
                    initial_dir, // Pass it along
                )
            }
            PlatformCommand::ShowOpenFileDialog {
                // New case
                window_id,
                title,
                filter_spec,
                initial_dir,
            } => self.internal_state._handle_show_open_file_dialog_impl(
                window_id,
                title,
                filter_spec,
                initial_dir,
            ),
        }
    }

    // Removed PlatformInterface::handle_show_save_file_dialog as its logic is now in Win32ApiInternalState::_handle_show_save_file_dialog_impl

    pub fn run(&self, event_handler: Arc<Mutex<dyn PlatformEventHandler>>) -> PlatformResult<()> {
        *self.internal_state.event_handler.lock().unwrap() = Some(Arc::downgrade(&event_handler));
        unsafe {
            let mut msg = MSG::default();
            loop {
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if result.0 == 0 {
                    println!("Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop.");
                    break;
                } else {
                    let last_error = GetLastError();
                    eprintln!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    return Err(PlatformError::OperationFailed(format!(
                        "GetMessageW failed: {}",
                        windows::core::Error::from_win32()
                    )));
                }
            }
        }
        if let Ok(mut handler_guard) = event_handler.lock() {
            handler_guard.on_quit();
        }
        *self.internal_state.event_handler.lock().unwrap() = None;
        println!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}
