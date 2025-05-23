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
                    COMMON_DLG_ERRORS, CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW,
                    OFN_EXPLORER, OFN_EXTENSIONDIFFERENT, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
                    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPEN_FILENAME_FLAGS, OPENFILENAMEW,
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
use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStringExt;
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
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        default_filename: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        self._show_common_file_dialog(
            window_id,
            title,
            Some(default_filename),
            filter_spec,
            initial_dir,
            OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR,
            |ofn_ptr| unsafe { GetSaveFileNameW(ofn_ptr) },
            |win_id, res_path| AppEvent::FileSaveDialogCompleted {
                window_id: win_id,
                result: res_path,
            },
        )
    }

    fn _handle_show_open_file_dialog_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        self._show_common_file_dialog(
            window_id,
            title,
            None,
            filter_spec,
            initial_dir,
            OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
            |ofn_ptr| unsafe { GetOpenFileNameW(ofn_ptr) },
            |win_id, res_path| AppEvent::FileOpenDialogCompleted {
                window_id: win_id,
                result: res_path,
            },
        )
    }

    fn _show_common_file_dialog<FDialog, FEvent>(
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        default_filename: Option<String>,
        filter_spec: String,
        initial_dir: Option<PathBuf>,
        specific_flags: OPEN_FILENAME_FLAGS,
        dialog_fn: FDialog,
        event_constructor: FEvent,
    ) -> PlatformResult<()>
    where
        FDialog: FnOnce(&mut OPENFILENAMEW) -> windows::core::BOOL,
        FEvent: FnOnce(WindowId, Option<PathBuf>) -> AppEvent,
    {
        let hwnd_owner = self.get_hwnd_owner(window_id)?;

        let mut file_buffer: Vec<u16> = vec![0; 2048];
        if let Some(fname) = default_filename {
            if !fname.is_empty() {
                let default_name_utf16: Vec<u16> = fname.encode_utf16().collect();
                let len_to_copy = std::cmp::min(default_name_utf16.len(), file_buffer.len() - 1);
                file_buffer[..len_to_copy].copy_from_slice(&default_name_utf16[..len_to_copy]);
            }
        }

        let title_hstring = HSTRING::from(title);
        let filter_utf16: Vec<u16> = filter_spec.encode_utf16().collect();
        let initial_dir_hstring = initial_dir.map(|p| HSTRING::from(p.to_string_lossy().as_ref()));
        let initial_dir_pcwstr = initial_dir_hstring
            .as_ref()
            .map_or(PCWSTR::null(), |h_str| PCWSTR(h_str.as_ptr()));

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd_owner,
            lpstrFile: windows::core::PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: PCWSTR(filter_utf16.as_ptr()),
            lpstrTitle: PCWSTR(title_hstring.as_ptr()),
            lpstrInitialDir: initial_dir_pcwstr,
            Flags: OFN_EXPLORER | specific_flags,
            ..Default::default()
        };

        let dialog_succeeded = dialog_fn(&mut ofn).as_bool();
        let mut path_result: Option<PathBuf> = None;

        if dialog_succeeded {
            path_result = Some(pathbuf_from_buf(&file_buffer));
            println!(
                "Platform: Dialog function succeeded. Path: {:?}",
                path_result.as_ref().unwrap()
            );
        } else {
            let error_code = unsafe { CommDlgExtendedError() };
            if error_code != COMMON_DLG_ERRORS(0) {
                eprintln!(
                    "Platform: Dialog function failed or was cancelled with error. CommDlgExtendedError: {:?}",
                    error_code
                );
            } else {
                println!("Platform: Dialog cancelled by user (no error).");
            }
        }

        let event = event_constructor(window_id, path_result);

        // Send the event to AppLogic. MyAppLogic will enqueue commands.
        // The main loop in PlatformInterface::run() will pick them up.
        if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event); // This now returns ()
            } else {
                eprintln!("Platform: Failed to lock event handler after dialog completion.");
            }
        } else {
            eprintln!("Platform: Event handler not available after dialog completion.");
        }
        // Removed: self.process_commands_from_event_handler(commands_from_dialog_completion);
        Ok(())
    }

    fn _handle_show_profile_selection_dialog_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        available_profiles: Vec<String>,
        title: String,
        prompt: String,
        emphasize_create_new: bool,
    ) -> PlatformResult<()> {
        println!(
            "Platform (STUB): Showing Profile Selection Dialog. Title: '{}', Prompt: '{}', Emphasize Create: {}, Profiles: {:?}",
            title, prompt, emphasize_create_new, available_profiles
        );

        let (chosen_profile_name, create_new_requested, cancelled) =
            if !available_profiles.is_empty() && !emphasize_create_new {
                (Some(available_profiles[0].clone()), false, false)
            } else if emphasize_create_new || available_profiles.is_empty() {
                (None, true, false)
            } else {
                (None, false, true)
            };

        println!(
            "Platform (STUB): Simulating dialog result: chosen='{:?}', create_new={}, cancelled={}",
            chosen_profile_name, create_new_requested, cancelled
        );

        let event = AppEvent::ProfileSelectionDialogCompleted {
            window_id,
            chosen_profile_name,
            create_new_requested,
            user_cancelled: cancelled,
        };

        if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event); // MyAppLogic enqueues commands
            } else {
                eprintln!(
                    "Platform: Failed to lock event handler for ProfileSelectionDialogCompleted."
                );
            }
        } else {
            eprintln!("Platform: Event handler not available for ProfileSelectionDialogCompleted.");
        }
        Ok(())
    }
    fn _handle_show_input_dialog_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        prompt: String,
        default_text: Option<String>,
        context_tag: Option<String>,
    ) -> PlatformResult<()> {
        println!(
            "Platform (STUB): Showing Input Dialog. Title: '{}', Prompt: '{}', Default: {:?}, Tag: {:?}",
            title, prompt, default_text, context_tag
        );
        let event = AppEvent::InputDialogCompleted {
            window_id,
            text: Some("TestProfileNameFromStub".to_string()),
            context_tag,
        };
        if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event);
            }
        }
        Ok(())
    }

    fn _handle_show_folder_picker_dialog_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        title: String,
        initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        println!(
            "Platform (STUB): Showing Folder Picker Dialog. Title: '{}', Initial Dir: {:?}",
            title, initial_dir
        );
        let event = AppEvent::FolderPickerDialogCompleted {
            window_id,
            path: Some(PathBuf::from("./mock_picked_folder_stub")),
        };
        if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event);
            }
        }
        Ok(())
    }

    fn _handle_quit_application_impl(self: &Arc<Self>) -> PlatformResult<()> {
        println!("Platform: Received QuitApplication command. Posting WM_QUIT.");
        unsafe { PostQuitMessage(0) };
        Ok(())
    }

    /*
     * Executes a single platform command directly.
     * This method centralizes the handling of all platform commands, whether
     * they originate from the initial setup or from event handling by MyAppLogic.
     * It's called by the main run loop after dequeuing commands from MyAppLogic.
     */
    fn _execute_platform_command(self: &Arc<Self>, command: PlatformCommand) -> PlatformResult<()> {
        println!("Platform: Executing command: {:?}", command); // Basic logging
        match command {
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
                initial_dir,
            } => self._handle_show_save_file_dialog_impl(
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
            } => {
                self._handle_show_open_file_dialog_impl(window_id, title, filter_spec, initial_dir)
            }
            PlatformCommand::UpdateStatusBarText {
                window_id,
                text,
                is_error,
            } => window_common::update_status_bar_text(self, window_id, &text, is_error),
            PlatformCommand::ShowProfileSelectionDialog {
                window_id,
                available_profiles,
                title,
                prompt,
                emphasize_create_new,
            } => self._handle_show_profile_selection_dialog_impl(
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
            } => self._handle_show_input_dialog_impl(
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
            } => self._handle_show_folder_picker_dialog_impl(window_id, title, initial_dir),
            PlatformCommand::QuitApplication => self._handle_quit_application_impl(),
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
            hwnd: HWND(std::ptr::null_mut()),
            id: window_id,
            treeview_state: None,
            hwnd_button_generate: None,
            hwnd_status_bar: None,      // Initialize new field
            status_bar_is_error: false, // Initialize new field
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
                        "Platform: Updated HWND in NativeWindowData for WindowId {:?}. Button HWND is {:?}, Status HWND is {:?}.",
                        window_id, window_data.hwnd_button_generate, window_data.hwnd_status_bar
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
                initial_dir,
            } => self.internal_state._handle_show_save_file_dialog_impl(
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
            } => self.internal_state._handle_show_open_file_dialog_impl(
                window_id,
                title,
                filter_spec,
                initial_dir,
            ),
            PlatformCommand::UpdateStatusBarText {
                window_id,
                text,
                is_error,
            } => window_common::update_status_bar_text(
                &self.internal_state,
                window_id,
                &text,
                is_error,
            ),
            PlatformCommand::ShowProfileSelectionDialog {
                window_id,
                available_profiles,
                title,
                prompt,
                emphasize_create_new,
            } => self
                .internal_state
                ._handle_show_profile_selection_dialog_impl(
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
            } => self.internal_state._handle_show_input_dialog_impl(
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
            } => self.internal_state._handle_show_folder_picker_dialog_impl(
                window_id,
                title,
                initial_dir,
            ),
            PlatformCommand::QuitApplication => self.internal_state._handle_quit_application_impl(),
        }
    }

    /*
     * Starts the platform's main event loop.
     * This method takes ownership of the `event_handler` (MyAppLogic) and continuously
     * processes messages. Before checking for OS messages, it drains and executes
     * any commands enqueued in MyAppLogic. It only returns when the application
     * is quitting.
     */
    pub fn run(&self, event_handler: Arc<Mutex<dyn PlatformEventHandler>>) -> PlatformResult<()> {
        *self.internal_state.event_handler.lock().unwrap() = Some(Arc::downgrade(&event_handler));

        let app_logic_ref = event_handler; // Keep a strong reference for the loop

        unsafe {
            let mut msg = MSG::default();
            loop {
                // 1. Drain MyAppLogic's command queue and execute commands
                loop {
                    let command_opt = if let Ok(mut logic_guard) = app_logic_ref.lock() {
                        // Downcast to access MyAppLogic specific methods
                        // Note: This assumes event_handler always IS a Mutex<MyAppLogic>
                        // If it could be other PlatformEventHandlers, a more robust downcast is needed
                        // or try_dequeue_command would need to be part of the trait.
                        // For this project structure, it's MyAppLogic.
                        if let Some(my_app_logic_concrete) = logic_guard
                            .as_any_mut() // Requires PlatformEventHandler to have `as_any_mut`
                            .downcast_mut::<crate::app_logic::handler::MyAppLogic>()
                        // Fully qualified path
                        {
                            my_app_logic_concrete.try_dequeue_command()
                        } else {
                            eprintln!(
                                "Platform: Failed to downcast event_handler to MyAppLogic for dequeuing."
                            );
                            None
                        }
                    } else {
                        eprintln!("Platform: Failed to lock MyAppLogic to dequeue command.");
                        None
                    };

                    if let Some(command) = command_opt {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            eprintln!("Platform: Error executing command from queue: {:?}", e);
                        }
                    } else {
                        break;
                    }
                }

                // 2. Process OS messages
                let result = GetMessageW(&mut msg, None, 0, 0);

                if result.0 > 0 {
                    // Message received
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg); // This will call facade_wnd_proc_router
                } else if result.0 == 0 {
                    // WM_QUIT
                    println!("Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop.");
                    break;
                } else {
                    // Error
                    let last_error = GetLastError();
                    eprintln!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    // If WM_QUIT was posted due to last window closing, and then GetMessageW fails,
                    // this could lead to an error return even if shutdown was intended.
                    // Check if quitting was intended.
                    if self.internal_state.is_quitting.load(Ordering::Relaxed) == 1
                        && self
                            .internal_state
                            .active_windows_count
                            .load(Ordering::Relaxed)
                            == 0
                    {
                        println!(
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

        // Call on_quit on MyAppLogic
        if let Ok(mut handler_guard) = app_logic_ref.lock() {
            handler_guard.on_quit();
        } else {
            eprintln!("Platform: Failed to lock MyAppLogic for on_quit call.");
        }

        *self.internal_state.event_handler.lock().unwrap() = None;
        println!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

/// Given a slice of UTF-16 code units (with a trailing 0), produce a PathBuf.
///
/// It:
/// 1. Finds the first 0 (or uses the full buffer if none),
/// 2. Constructs an OsString via `from_wide`,
/// 3. Converts that to `PathBuf`.
pub fn pathbuf_from_buf(buffer: &[u16]) -> PathBuf {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let path_str = String::from_utf16_lossy(&buffer[..len]);
    PathBuf::from(path_str)
}

#[cfg(test)]
mod app_tests {
    use super::*;

    #[test]
    fn roundtrip_simple() {
        // "C:\temp\file.txt" in UTF-16 (with trailing 0)
        let mut wide: Vec<u16> = "C:\\temp\\file.txt".encode_utf16().collect();
        wide.push(0);
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"C:\temp\file.txt"));
    }

    #[test]
    fn no_null_terminator() {
        // if there's no 0, we still consume the whole buffer
        let wide: Vec<u16> = "D:\\data\\incomplete".encode_utf16().collect();
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"D:\data\incomplete"));
    }
}
