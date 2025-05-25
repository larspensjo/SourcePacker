use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{
    AppEvent, MessageSeverity, PlatformCommand, PlatformEventHandler, WindowConfig, WindowId,
};
use super::window_common;

use windows::{
    Win32::{
        Foundation::{FALSE, GetLastError, HINSTANCE, HWND, LPARAM, TRUE, WPARAM},
        Graphics::Gdi::InvalidateRect,
        System::Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        System::LibraryLoader::GetModuleHandleW,
        System::SystemServices::SS_LEFTNOWORDWRAP,
        UI::Controls::{
            Dialogs::*, ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
        },
        UI::Input::KeyboardAndMouse::EnableWindow,
        UI::Shell::{
            FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
            SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

use std::collections::HashMap;
use std::ffi::{OsStr, OsString, c_void};
use std::mem::{align_of, size_of};
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

    fn get_hwnd_owner(&self, window_id: WindowId) -> PlatformResult<HWND> {
        let windows_guard = self.window_map.read().map_err(|_| {
            PlatformError::OperationFailed("Failed to acquire read lock on windows map".into())
        })?;
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
            |win_id, res_path| AppEvent::FileOpenProfileDialogCompleted {
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
        if let Some(handler_arc) = self
            .event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|wh| wh.upgrade())
        {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event);
            } else {
                eprintln!("Platform: Failed to lock event handler after dialog completion.");
            }
        } else {
            eprintln!("Platform: Event handler not available after dialog completion.");
        }
        Ok(())
    }

    fn _handle_show_profile_selection_dialog_stub_impl(
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
                handler_guard.handle_event(event);
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
        println!("Platform: Showing Input Dialog. Title: '{}'", title);
        let hwnd_owner = self.get_hwnd_owner(window_id)?;
        let mut dialog_data = InputDialogData {
            prompt_text: prompt,
            input_text: default_text.unwrap_or_default(),
            context_tag: context_tag.clone(),
            success: false,
        };
        let mut template_bytes = Vec::<u8>::new();
        build_input_dialog_template(&mut template_bytes, &title, &dialog_data.prompt_text)?;
        let dialog_result = unsafe {
            DialogBoxIndirectParamW(
                Some(self.h_instance),
                template_bytes.as_ptr() as *const DLGTEMPLATE,
                Some(hwnd_owner),
                Some(input_dialog_proc),
                LPARAM(&mut dialog_data as *mut _ as isize),
            )
        };
        let final_text_result = if dialog_result != 0 && dialog_data.success {
            Some(dialog_data.input_text)
        } else {
            println!(
                "Platform: Input dialog cancelled or failed. Result: {:?}, Success flag: {}",
                dialog_result, dialog_data.success
            );
            None
        };
        let event = AppEvent::GenericInputDialogCompleted {
            window_id,
            text: final_text_result,
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
        _initial_dir: Option<PathBuf>, // _initial_dir is not used in this impl yet
    ) -> PlatformResult<()> {
        println!(
            "Platform: Showing real Folder Picker Dialog. Title: '{}'",
            title
        );
        let hwnd_owner = self.get_hwnd_owner(window_id)?;
        let mut path_result: Option<PathBuf> = None;
        let file_dialog_result: Result<IFileOpenDialog, windows::core::Error> =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) };

        if let Ok(file_dialog) = file_dialog_result {
            unsafe {
                let _ = file_dialog.SetOptions(FOS_PICKFOLDERS);
                let h_title = HSTRING::from(title);
                let _ = file_dialog.SetTitle(&h_title);
                if let Some(dir_path) = &_initial_dir {
                    let dir_hstring = HSTRING::from(dir_path.as_os_str());
                    match SHCreateItemFromParsingName::<_, _, IShellItem>(&dir_hstring, None) {
                        Ok(item) => {
                            if let Err(e_sdf) = file_dialog.SetDefaultFolder(&item) {
                                eprintln!(
                                    "Platform: IFileOpenDialog::SetDefaultFolder failed: {:?}",
                                    e_sdf
                                );
                            }
                        }
                        Err(e_csipn) => {
                            eprintln!(
                                "Platform: SHCreateItemFromParsingName for initial_dir {:?} failed: {:?}",
                                dir_path, e_csipn
                            );
                        }
                    }
                }
                if file_dialog.Show(Some(hwnd_owner)).is_ok() {
                    if let Ok(shell_item) = file_dialog.GetResult() {
                        if let Ok(pwstr_path) = shell_item.GetDisplayName(SIGDN_FILESYSPATH) {
                            let path_string = pwstr_path.to_string().unwrap_or_default();
                            CoTaskMemFree(Some(pwstr_path.as_ptr() as *const c_void));
                            if !path_string.is_empty() {
                                path_result = Some(PathBuf::from(path_string));
                            }
                        }
                    }
                }
            }
        } else if let Err(e) = file_dialog_result {
            let err_msg = format!(
                "Platform: CoCreateInstance for IFileOpenDialog failed: {:?}",
                e
            );
            eprintln!("{}", err_msg);
            // Send completion event even on creation failure so AppLogic isn't stuck
            let event = AppEvent::FolderPickerDialogCompleted {
                window_id,
                path: None,
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
            return Err(PlatformError::OperationFailed(err_msg));
        }

        let event = AppEvent::FolderPickerDialogCompleted {
            window_id,
            path: path_result,
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

    fn _handle_update_status_bar_text_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        text: String,
        severity: MessageSeverity,
    ) -> PlatformResult<()> {
        // 1. Logging based on severity
        match severity {
            MessageSeverity::Error => {
                eprintln!("Platform Status (WinID {:?} ERROR): {}", window_id, text)
            }
            MessageSeverity::Warning => {
                eprintln!("Platform Status (WinID {:?} WARN):  {}", window_id, text)
            }
            MessageSeverity::Information => {
                println!("Platform Status (WinID {:?} INFO): {}", window_id, text)
            }
            MessageSeverity::Debug => {
                println!("Platform Status (WinID {:?} DEBUG): {}", window_id, text)
            }
            MessageSeverity::None => {
                println!("Platform Status (WinID {:?} CLEAR)", window_id)
            }
        }

        let hwnd_status_bar_opt: Option<HWND>;
        let final_text_for_setwindowtext: String;

        // 2. Scope for the write lock to update NativeWindowData
        {
            let mut windows_map_guard = self.window_map.write().map_err(|_| {
                PlatformError::OperationFailed(
                    "Failed to lock windows map for status update".into(),
                )
            })?;

            if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                if severity == MessageSeverity::None {
                    // Clear status
                    window_data.status_bar_current_text.clear();
                    window_data.status_bar_current_severity = MessageSeverity::None;
                    final_text_for_setwindowtext = "".to_string();
                } else if severity >= window_data.status_bar_current_severity {
                    // Update with new higher-or-equal severity message
                    window_data.status_bar_current_text = text.clone();
                    window_data.status_bar_current_severity = severity;
                    final_text_for_setwindowtext = text; // text is moved here
                } else {
                    // Incoming message is lower severity, do not update UI, but still log it (already done above)
                    println!(
                        "Platform Status (WinID {:?} IGNORED_LOWER_SEVERITY): {} (Severity: {:?}, Current: {:?})",
                        window_id, text, severity, window_data.status_bar_current_severity
                    );
                    return Ok(()); // Early exit, no UI update needed
                }
                hwnd_status_bar_opt = window_data.hwnd_status_bar; // Copy HWND
            } else {
                // WindowId not found
                return Err(PlatformError::InvalidHandle(format!(
                    "WindowId {:?} not found for status bar update",
                    window_id
                )));
            }
        } // Write lock on window_map is released here

        // 3. Perform WinAPI calls without holding the lock
        if let Some(hwnd_status) = hwnd_status_bar_opt {
            unsafe {
                // Set the text on the status bar control
                if SetWindowTextW(hwnd_status, &HSTRING::from(final_text_for_setwindowtext))
                    .is_err()
                {
                    return Err(PlatformError::OperationFailed(format!(
                        "SetWindowTextW for status bar failed: {:?}",
                        GetLastError()
                    )));
                }
                // Invalidate the status bar control to force a repaint.
                InvalidateRect(Some(hwnd_status), None, true); // true to erase background
            }
            Ok(())
        } else {
            // This case implies hwnd_status_bar was None in NativeWindowData
            Err(PlatformError::InvalidHandle(format!(
                "Status bar HWND not found for WindowId {:?} (after lock release)",
                window_id
            )))
        }
    }

    /*
     * Executes a single platform command directly.
     * This method centralizes the handling of all platform commands, whether
     * they originate from the initial setup or from event handling by MyAppLogic.
     * It's called by the main run loop after dequeuing commands from MyAppLogic.
     */
    fn _execute_platform_command(self: &Arc<Self>, command: PlatformCommand) -> PlatformResult<()> {
        println!("Platform: Executing command: {:?}", command);
        match command {
            PlatformCommand::SetWindowTitle { window_id, title } => {
                window_common::set_window_title(self, window_id, &title)
            }
            PlatformCommand::ShowWindow { window_id } => {
                window_common::show_window(self, window_id, true)
            }
            PlatformCommand::CloseWindow { window_id } => {
                // window_common::destroy_native_window(self, window_id) // Original direct destroy
                window_common::send_close_message(self, window_id) // Changed to send WM_CLOSE first
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
                severity,
            } => self._handle_update_status_bar_text_impl(window_id, text, severity), // Refactored call
            PlatformCommand::ShowProfileSelectionDialog {
                window_id,
                available_profiles,
                title,
                prompt,
                emphasize_create_new,
            } => self._handle_show_profile_selection_dialog_stub_impl(
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
            PlatformCommand::SetControlEnabled {
                window_id,
                control_id,
                enabled,
            } => self._handle_set_control_enabled_impl(window_id, control_id, enabled),
            PlatformCommand::QuitApplication => self._handle_quit_application_impl(),
        }
    }

    fn _handle_set_control_enabled_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
        enabled: bool,
    ) -> PlatformResult<()> {
        let windows_guard = self.window_map.read().map_err(|_| {
            PlatformError::OperationFailed("Failed to acquire read lock on windows map".into())
        })?;

        if let Some(window_data) = windows_guard.get(&window_id) {
            let hwnd_ctrl_result = unsafe { GetDlgItem(Some(window_data.hwnd), control_id) };
            let hwnd_ctrl = match hwnd_ctrl_result {
                Ok(hwnd) => hwnd,
                Err(_) => {
                    return Err(PlatformError::InvalidHandle(format!(
                        "Control ID {} not found in window {:?}",
                        control_id, window_id
                    )));
                }
            };
            unsafe {
                EnableWindow(hwnd_ctrl, enabled);
            }
            println!(
                "Platform: Control ID {} in window {:?} set to enabled: {}",
                control_id, window_id, enabled
            );
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for SetControlEnabled",
                window_id
            )))
        }
    }
}

impl Drop for Win32ApiInternalState {
    fn drop(&mut self) {
        println!("Platform: Win32ApiInternalState dropped, calling CoUninitialize.");
        unsafe { CoUninitialize() };
    }
}

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
            hwnd: HWND(std::ptr::null_mut()), // Will be set after successful creation
            id: window_id,
            treeview_state: None,
            hwnd_button_generate: None,
            hwnd_status_bar: None,
            status_bar_current_text: String::new(), // Initialize
            status_bar_current_severity: MessageSeverity::None, // Initialize
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
                    window_data.hwnd = hwnd; // Set the actual HWND
                    // Button and status bar HWNDs are set in WM_CREATE of window_common
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
        self.internal_state._execute_platform_command(command)
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
        let app_logic_ref = event_handler;
        unsafe {
            let mut msg = MSG::default();
            loop {
                // Reset current highest severity for windows before processing new commands/events
                // This ensures that a new highest severity for the current "cycle" can be determined.
                if let Ok(mut windows_map_guard) = self.internal_state.window_map.write() {
                    for (_id, window_data) in windows_map_guard.iter_mut() {
                        // Reset to a baseline that allows new info messages to show,
                        // but errors/warnings would still take precedence.
                        // If MessageSeverity::None, then only higher severity messages will appear
                        // until explicitly cleared. If MessageSeverity::Information, then new Info
                        // messages will replace old ones.
                        window_data.status_bar_current_severity = MessageSeverity::Information;
                        if window_data.status_bar_current_text.is_empty() {
                            // If it was cleared, set to None
                            window_data.status_bar_current_severity = MessageSeverity::None;
                        }
                    }
                }

                loop {
                    let command_opt = if let Ok(mut logic_guard) = app_logic_ref.lock() {
                        logic_guard.try_dequeue_command()
                    } else {
                        eprintln!("Platform: Failed to lock MyAppLogic to dequeue command.");
                        None
                    };

                    if let Some(command) = command_opt {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            eprintln!("Platform: Error executing command from queue: {:?}", e);
                        }
                    } else {
                        break; // No more commands from app logic queue
                    }
                }

                // Process Windows messages. This will block until a message is received.
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if result.0 == 0 {
                    println!("Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop.");
                    break; // WM_QUIT
                } else {
                    // Error
                    let last_error = GetLastError();
                    eprintln!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    // Check if we are already in a quit sequence
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
        // Application quit
        if let Ok(mut handler_guard) = app_logic_ref.lock() {
            handler_guard.on_quit();
        } else {
            eprintln!("Platform: Failed to lock MyAppLogic for on_quit call.");
        }
        *self.internal_state.event_handler.lock().unwrap() = None; // Clear handler ref
        println!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

/// Given a slice of UTF-16 code units (with a trailing 0), produce a PathBuf.
pub fn pathbuf_from_buf(buffer: &[u16]) -> PathBuf {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let path_os_string = OsString::from_wide(&buffer[..len]);
    PathBuf::from(path_os_string)
}

struct InputDialogData {
    prompt_text: String,
    input_text: String,
    context_tag: Option<String>,
    success: bool,
}

fn loword_from_wparam(wparam: WPARAM) -> u16 {
    (wparam.0 & 0xFFFF) as u16
}

unsafe extern "system" fn input_dialog_proc(
    hdlg: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match msg {
        WM_INITDIALOG => {
            unsafe {
                SetWindowLongPtrW(hdlg, GWLP_USERDATA, lparam.0);
            }
            let dialog_data = unsafe { &*(lparam.0 as *const InputDialogData) };
            let h_prompt = HSTRING::from(dialog_data.prompt_text.as_str());
            unsafe {
                SetDlgItemTextW(
                    hdlg,
                    window_common::ID_DIALOG_INPUT_PROMPT_STATIC,
                    &h_prompt,
                )
                .unwrap_or_default();
            }
            if !dialog_data.input_text.is_empty() {
                let h_edit_text = HSTRING::from(dialog_data.input_text.as_str());
                unsafe {
                    SetDlgItemTextW(hdlg, window_common::ID_DIALOG_INPUT_EDIT, &h_edit_text)
                        .unwrap_or_default();
                }
            }
            // SetFocus logic if needed
            TRUE.0 as isize
        }
        WM_COMMAND => {
            let command_id = loword_from_wparam(wparam);
            match command_id {
                x if x == IDOK.0 as u16 => {
                    let dialog_data_ptr =
                        unsafe { GetWindowLongPtrW(hdlg, GWLP_USERDATA) } as *mut InputDialogData;
                    if !dialog_data_ptr.is_null() {
                        let dialog_data = unsafe { &mut *dialog_data_ptr };
                        if let Ok(hwnd_edit_ok) =
                            unsafe { GetDlgItem(Some(hdlg), window_common::ID_DIALOG_INPUT_EDIT) }
                        {
                            let mut buffer: [u16; 256] = [0; 256];
                            let len = unsafe { GetWindowTextW(hwnd_edit_ok, &mut buffer) };
                            dialog_data.input_text = if len > 0 {
                                String::from_utf16_lossy(&buffer[0..len as usize])
                            } else {
                                String::new()
                            };
                        }
                        dialog_data.success = true;
                    }
                    unsafe {
                        EndDialog(hdlg, IDOK.0 as isize).unwrap_or_default();
                    }
                    return TRUE.0 as isize;
                }
                x if x == IDCANCEL.0 as u16 => {
                    let dialog_data_ptr =
                        unsafe { GetWindowLongPtrW(hdlg, GWLP_USERDATA) } as *mut InputDialogData;
                    if !dialog_data_ptr.is_null() {
                        unsafe { (&mut *dialog_data_ptr).success = false };
                    }
                    unsafe { EndDialog(hdlg, IDCANCEL.0 as isize).unwrap_or_default() };
                    return TRUE.0 as isize;
                }
                _ => FALSE.0 as isize,
            }
        }
        _ => FALSE.0 as isize,
    }
}

fn push_word(vec: &mut Vec<u8>, word: u16) {
    vec.extend_from_slice(&word.to_le_bytes());
}

fn push_dword(vec: &mut Vec<u8>, dword: u32) {
    vec.extend_from_slice(&dword.to_le_bytes());
}

fn push_str_utf16(vec: &mut Vec<u8>, s: &str) {
    for c in s.encode_utf16() {
        push_word(vec, c);
    }
    push_word(vec, 0);
}

fn align_to_dword(vec: &mut Vec<u8>) {
    while vec.len() % align_of::<u32>() != 0 {
        vec.push(0);
    }
}

fn build_input_dialog_template(
    template_bytes: &mut Vec<u8>,
    title_str: &str,
    _prompt_str: &str,
) -> PlatformResult<()> {
    let style = DS_CENTER | DS_MODALFRAME | DS_SETFONT;
    let dlg_template = DLGTEMPLATE {
        style: style as u32 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_POPUP.0,
        dwExtendedStyle: 0,
        cdit: 4,
        x: 0,
        y: 0,
        cx: 200,
        cy: 80,
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(dlg_template) as *const [u8; size_of::<DLGTEMPLATE>()])
    });

    push_word(template_bytes, 0); // No menu
    push_word(template_bytes, 0); // Default dialog class
    push_str_utf16(template_bytes, title_str); // Title

    push_word(template_bytes, 8); // Pointsize
    push_str_utf16(template_bytes, "MS Shell Dlg"); // Font

    align_to_dword(template_bytes);
    let static_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | SS_LEFTNOWORDWRAP.0,
        dwExtendedStyle: 0,
        x: 10,
        y: 10,
        cx: 180,
        cy: 10,
        id: window_common::ID_DIALOG_INPUT_PROMPT_STATIC as u16,
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(static_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Static");
    push_str_utf16(template_bytes, "Prompt text here"); // Placeholder text set via SetDlgItemText
    push_word(template_bytes, 0); // No creation data

    align_to_dword(template_bytes);
    let edit_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32,
        dwExtendedStyle: 0,
        x: 10,
        y: 25,
        cx: 180,
        cy: 12,
        id: window_common::ID_DIALOG_INPUT_EDIT as u16,
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(edit_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Edit");
    push_word(template_bytes, 0); // No text here (set via SetDlgItemText)
    push_word(template_bytes, 0); // No creation data

    align_to_dword(template_bytes);
    let ok_button_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | BS_DEFPUSHBUTTON as u32,
        dwExtendedStyle: 0,
        x: 40,
        y: 50,
        cx: 50,
        cy: 14,
        id: IDOK.0 as u16,
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(ok_button_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Button");
    push_str_utf16(template_bytes, "OK");
    push_word(template_bytes, 0); // No creation data

    align_to_dword(template_bytes);
    let cancel_button_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32,
        dwExtendedStyle: 0,
        x: 110,
        y: 50,
        cx: 50,
        cy: 14,
        id: IDCANCEL.0 as u16,
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(cancel_button_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Button");
    push_str_utf16(template_bytes, "Cancel");
    push_word(template_bytes, 0); // No creation data

    Ok(())
}

#[cfg(test)]
mod app_tests {
    use super::*;

    #[test]
    fn roundtrip_simple() {
        let mut wide: Vec<u16> = "C:\\temp\\file.txt".encode_utf16().collect();
        wide.push(0);
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"C:\temp\file.txt"));
    }

    #[test]
    fn no_null_terminator() {
        let wide: Vec<u16> = "D:\\data\\incomplete".encode_utf16().collect();
        let path = pathbuf_from_buf(&wide);
        assert_eq!(path, PathBuf::from(r"D:\\data\\incomplete"));
    }
}
