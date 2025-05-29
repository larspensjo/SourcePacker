use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{
    AppEvent, MenuAction, MessageSeverity, PlatformCommand, PlatformEventHandler, WindowConfig,
    WindowId,
};
use super::{types::MenuItemConfig, window_common};

use windows::{
    Win32::{
        Foundation::{FALSE, GetLastError, HINSTANCE, HWND, LPARAM, RECT, TRUE, WPARAM},
        Graphics::Gdi::InvalidateRect,
        System::Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        System::LibraryLoader::GetModuleHandleW,
        System::SystemServices::SS_LEFTNOWORDWRAP,
        UI::Controls::{
            Dialogs::*, ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
            TVS_CHECKBOXES, TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS,
            WC_TREEVIEWW,
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

use crate::platform_layer::window_common::{
    BUTTON_AREA_HEIGHT, ID_BUTTON_GENERATE_ARCHIVE, ID_STATUS_BAR_CTRL, SS_LEFT, WC_BUTTON,
    WC_STATIC,
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

    /*
     * Displays a standard Win32 file dialog (Open or Save As).
     * This is a generic helper function used by `_handle_show_open_file_dialog_impl`
     * and `_handle_show_save_file_dialog_impl`. It handles the common setup
     * for `OPENFILENAMEW` and processes the dialog result, then sends an
     * appropriate `AppEvent` constructed by `event_constructor`.
     */
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
            log::debug!(
                "Platform: Dialog function succeeded. Path: {:?}",
                path_result.as_ref().unwrap()
            );
        } else {
            let error_code = unsafe { CommDlgExtendedError() };
            if error_code != COMMON_DLG_ERRORS(0) {
                log::error!(
                    "Platform: Dialog function failed or was cancelled with error. CommDlgExtendedError: {:?}",
                    error_code
                );
            } else {
                log::debug!("Platform: Dialog cancelled by user (no error).");
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
                log::error!("Platform: Failed to lock event handler after dialog completion.");
            }
        } else {
            log::error!("Platform: Event handler not available after dialog completion.");
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
        log::debug!(
            "Platform (STUB): Showing Profile Selection Dialog. Title: '{}', Prompt: '{}', Emphasize Create: {}, Profiles: {:?}",
            title,
            prompt,
            emphasize_create_new,
            available_profiles
        );

        let (chosen_profile_name, create_new_requested, cancelled) =
            if !available_profiles.is_empty() && !emphasize_create_new {
                (Some(available_profiles[0].clone()), false, false)
            } else if emphasize_create_new || available_profiles.is_empty() {
                (None, true, false)
            } else {
                (None, false, true)
            };
        log::debug!(
            "Platform (STUB): Simulating dialog result: chosen='{:?}', create_new={}, cancelled={}",
            chosen_profile_name,
            create_new_requested,
            cancelled
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
                log::error!(
                    "Platform: Failed to lock event handler for ProfileSelectionDialogCompleted."
                );
            }
        } else {
            log::error!(
                "Platform: Event handler not available for ProfileSelectionDialogCompleted."
            );
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
        log::debug!("Platform: Showing Input Dialog. Title: '{}'", title);
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
            log::debug!(
                "Platform: Input dialog cancelled or failed. Result: {:?}, Success flag: {}",
                dialog_result,
                dialog_data.success
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
        _initial_dir: Option<PathBuf>,
    ) -> PlatformResult<()> {
        log::debug!(
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
                                log::error!(
                                    "Platform: IFileOpenDialog::SetDefaultFolder failed: {:?}",
                                    e_sdf
                                );
                            }
                        }
                        Err(e_csipn) => {
                            log::error!(
                                "Platform: SHCreateItemFromParsingName for initial_dir {:?} failed: {:?}",
                                dir_path,
                                e_csipn
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
            log::error!("{}", err_msg);
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
        log::debug!("Platform: Received QuitApplication command. Posting WM_QUIT.");
        unsafe { PostQuitMessage(0) };
        Ok(())
    }

    /*
     * Handles the `PlatformCommand::UpdateStatusBarText` command.
     * It updates the stored text and severity in `NativeWindowData` and then
     * calls the WinAPI functions to update the visual appearance of the status bar.
     * The lock on `window_map` is released before making WinAPI calls that might
     * trigger synchronous messages (like WM_CTLCOLORSTATIC) to prevent deadlocks.
     */
    fn _handle_update_status_bar_text_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        text: String,
        severity: MessageSeverity,
    ) -> PlatformResult<()> {
        match severity {
            MessageSeverity::Error => {
                log::error!("Platform Status (WinID {:?} ERROR): {}", window_id, text)
            }
            MessageSeverity::Warning => {
                log::warn!("Platform Status (WinID {:?} WARN):  {}", window_id, text)
            }
            MessageSeverity::Information => {
                log::info!("Platform Status (WinID {:?} INFO): {}", window_id, text)
            }
            MessageSeverity::Debug => {
                log::debug!("Platform Status (WinID {:?} DEBUG): {}", window_id, text)
            }
            MessageSeverity::None => {
                log::debug!("Platform Status (WinID {:?} CLEAR)", window_id)
            }
        }

        let mut text_to_set_on_control: Option<String> = None;
        let mut hwnd_status_bar_for_api_call: Option<HWND> = None;

        // Scope for the write lock on window_map
        {
            let mut windows_map_guard = self.window_map.write().map_err(|_| {
                PlatformError::OperationFailed(
                    "Failed to lock windows map for status update".into(),
                )
            })?;

            if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                // Logic to decide if the status bar UI needs updating
                if severity == MessageSeverity::None {
                    window_data.status_bar_current_text.clear();
                    window_data.status_bar_current_severity = MessageSeverity::None;
                    text_to_set_on_control = Some("".to_string());
                } else if severity >= window_data.status_bar_current_severity {
                    window_data.status_bar_current_text = text.clone(); // Store full text
                    window_data.status_bar_current_severity = severity;
                    text_to_set_on_control = Some(text.clone()); // Text to actually set on the control
                } else {
                    // Incoming message is lower severity, do not update UI, but it was already logged.
                    log::debug!(
                        "Platform Status (WinID {:?} IGNORED_LOWER_SEVERITY_UI_UPDATE): current severity {:?}, incoming {:?})",
                        window_id,
                        window_data.status_bar_current_severity,
                        severity
                    );
                    return Ok(()); // Early exit, no UI update needed for this command
                }

                // Get the HWND for the status bar.
                hwnd_status_bar_for_api_call = window_data.get_control_hwnd(ID_STATUS_BAR_CTRL);
            } else {
                return Err(PlatformError::InvalidHandle(format!(
                    "WindowId {:?} not found for status bar update",
                    window_id
                )));
            }
        } // Write lock on window_map (windows_map_guard) is released here.

        // Perform WinAPI calls *after* releasing the lock.
        if let Some(hwnd_status) = hwnd_status_bar_for_api_call {
            if let Some(final_text) = text_to_set_on_control {
                // This text was determined while the lock was held.
                unsafe {
                    if SetWindowTextW(hwnd_status, &HSTRING::from(final_text)).is_err() {
                        return Err(PlatformError::OperationFailed(format!(
                            "SetWindowTextW for status bar failed: {:?}",
                            GetLastError()
                        )));
                    }
                    // Invalidate the status bar control to force a repaint.
                    // WM_CTLCOLORSTATIC will then use the (already updated) severity for color.
                    InvalidateRect(Some(hwnd_status), None, true); // true to erase background
                }
                Ok(())
            } else {
                // This case means no UI update was needed (e.g., lower severity message)
                Ok(())
            }
        } else {
            // HWND not found in the controls map.
            log::error!(
                "Platform Warning: Status bar HWND (ID {}) not found for WindowId {:?} during UI update. Text was: '{}'",
                ID_STATUS_BAR_CTRL,
                window_id,
                text,
            );
            Ok(())
        }
    }

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
                window_common::set_window_title(self, window_id, &title)
            }
            PlatformCommand::ShowWindow { window_id } => {
                window_common::show_window(self, window_id, true)
            }
            PlatformCommand::CloseWindow { window_id } => {
                window_common::send_close_message(self, window_id)
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
            } => self._handle_update_status_bar_text_impl(window_id, text, severity),
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
            PlatformCommand::CreateMainMenu {
                window_id,
                menu_items,
            } => self._handle_create_main_menu_impl(window_id, menu_items),
            PlatformCommand::CreateButton {
                window_id,
                control_id,
                text,
            } => self._handle_create_button_impl(window_id, control_id, text),
            PlatformCommand::CreateStatusBar {
                window_id,
                control_id,
                initial_text,
            } => self._handle_create_status_bar_impl(window_id, control_id, initial_text),
            PlatformCommand::CreateTreeView {
                window_id,
                control_id,
            } => self._handle_create_treeview_impl(window_id, control_id),
        }
    }

    fn _handle_create_main_menu_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        menu_items: Vec<MenuItemConfig>,
    ) -> PlatformResult<()> {
        let h_main_menu = unsafe { CreateMenu()? };
        let mut hwnd_owner_opt: Option<HWND> = None;

        // Scope for the write lock to modify NativeWindowData
        {
            let mut windows_map_guard = self.window_map.write().map_err(|_| {
                PlatformError::OperationFailed(
                    "Failed to lock windows map for main menu creation (data population)".into(),
                )
            })?;

            if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                hwnd_owner_opt = Some(window_data.hwnd);
                if window_data.hwnd.is_invalid() {
                    // Should not happen if create_window was called and completed successfully
                    unsafe {
                        DestroyMenu(h_main_menu).unwrap_or_default();
                    }
                    return Err(PlatformError::InvalidHandle(format!(
                        "HWND not yet valid for WindowId {:?} during menu data population",
                        window_id
                    )));
                }

                for item_config in menu_items {
                    // Pass mutable reference to window_data for ID generation and mapping
                    unsafe {
                        self.add_menu_item_recursive(h_main_menu, &item_config, window_data)?;
                    }
                }
            } else {
                unsafe {
                    DestroyMenu(h_main_menu).unwrap_or_default();
                }
                return Err(PlatformError::InvalidHandle(format!(
                    "WindowId {:?} not found for CreateMainMenu (data population)",
                    window_id
                )));
            }
            // windows_map_guard is dropped here, releasing the write lock.
        }

        // Now call SetMenu AFTER releasing the lock
        if let Some(hwnd_owner) = hwnd_owner_opt {
            if unsafe { SetMenu(hwnd_owner, Some(h_main_menu)) }.is_err() {
                unsafe {
                    DestroyMenu(h_main_menu).unwrap_or_default();
                }
                return Err(PlatformError::OperationFailed(format!(
                    "SetMenu failed for main menu on WindowId {:?}: {:?}",
                    window_id,
                    unsafe { GetLastError() }
                )));
            }
            log::debug!(
                "Platform: Main menu created and set for WindowId {:?}",
                window_id
            );
            Ok(())
        } else {
            // This case should ideally be caught earlier if window_data wasn't found
            unsafe {
                DestroyMenu(h_main_menu).unwrap_or_default();
            }
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} became invalid or HWND was not set before SetMenu",
                window_id
            )))
        }
    }

    /*
     * Recursively adds menu items (and submenus) to a parent menu handle.
     * This function is called by `_handle_create_main_menu_impl` to build
     * the menu structure defined by `MenuItemConfig`. If an item has a `MenuAction`,
     * a unique `i32` ID is generated, stored in `NativeWindowData.menu_action_map`,
     * and used with `AppendMenuW`. For items that are popups (no action), the
     * submenu handle is used.
     */
    unsafe fn add_menu_item_recursive(
        self: &Arc<Self>, // Added self to potentially access h_instance if ever needed, though not currently
        parent_menu_handle: HMENU,
        item_config: &MenuItemConfig,
        window_data: &mut window_common::NativeWindowData, // For ID generation and map
    ) -> PlatformResult<()> {
        if item_config.children.is_empty() {
            // This is a command item
            if let Some(action) = item_config.action {
                let generated_id = window_data.generate_menu_item_id();
                window_data.menu_action_map.insert(generated_id, action);
                log::trace!(
                    "Platform: Mapping menu action {:?} to ID {} for window {:?}",
                    action,
                    generated_id,
                    window_data.id
                );
                unsafe {
                    AppendMenuW(
                        parent_menu_handle,
                        MF_STRING,
                        generated_id as usize, // Use the generated ID
                        &HSTRING::from(item_config.text.as_str()),
                    )?;
                }
            } else {
                // This is a simple menu item with no action (e.g., a separator if MF_SEPARATOR was used)
                // Or an item that should have had an action but doesn't - log a warning.
                log::warn!(
                    "Platform: Menu item '{}' has no children and no action. It will be non-functional.",
                    item_config.text
                );
                // Add it as a disabled item or an item that does nothing.
                // For now, let's treat it like an item with an action but use a placeholder ID that won't be in map.
                // Or, more simply, just append it with ID 0 if that's acceptable for non-action items.
                // Current structure means it's only appended if it has an action.
                // If it is intended to be a non-actionable label, AppendMenuW requires an ID.
                // Let's assume items without children *must* have an action to be useful.
                // So, if action is None here, we might skip it or log an error.
                // The current ui_description_layer ensures items without children have an action.
            }
        } else {
            // This is a popup item (has children)
            let h_submenu = unsafe { CreatePopupMenu()? };
            for child_config in &item_config.children {
                unsafe {
                    self.add_menu_item_recursive(h_submenu, child_config, window_data)?;
                }
            }
            unsafe {
                AppendMenuW(
                    parent_menu_handle,
                    MF_POPUP,
                    h_submenu.0 as usize, // ID for MF_POPUP is the submenu handle
                    &HSTRING::from(item_config.text.as_str()),
                )?;
            }
        }
        Ok(())
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
            let hwnd_ctrl_opt = window_data.get_control_hwnd(control_id);

            let hwnd_ctrl = match hwnd_ctrl_opt {
                Some(hwnd) => hwnd,
                None => {
                    return Err(PlatformError::InvalidHandle(format!(
                        "Control ID {} not found in window {:?}",
                        control_id, window_id
                    )));
                }
            };

            unsafe {
                EnableWindow(hwnd_ctrl, enabled);
            }
            log::debug!(
                "Platform: Control ID {} in window {:?} set to enabled: {}",
                control_id,
                window_id,
                enabled
            );
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for SetControlEnabled",
                window_id
            )))
        }
    }

    /*
     * Handles the `PlatformCommand::CreateButton` command.
     * This method creates a native button control within the specified window
     * and stores its HWND in the window's `NativeWindowData.controls` map.
     * The button's position and size will be managed by `WM_SIZE`.
     */
    fn _handle_create_button_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
        text: String,
    ) -> PlatformResult<()> {
        let mut windows_map_guard = self.window_map.write().map_err(|_| {
            PlatformError::OperationFailed("Failed to lock windows map for button creation".into())
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            if window_data.controls.contains_key(&control_id) {
                return Err(PlatformError::OperationFailed(format!(
                    "Button with ID {} already exists for window {:?}",
                    control_id, window_id
                )));
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
                    10, // Dummies, WM_SIZE will adjust
                    Some(window_data.hwnd),
                    Some(HMENU(control_id as *mut c_void)),
                    Some(self.h_instance),
                    None,
                )?
            };
            window_data.controls.insert(control_id, hwnd_button);
            log::debug!(
                "Platform: Created button '{}' (ID {}) for window {:?} with HWND {:?}",
                text,
                control_id,
                window_id,
                hwnd_button
            );
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateButton",
                window_id
            )))
        }
    }

    /*
     * Handles the `PlatformCommand::CreateStatusBar` command.
     * This method creates a native status bar control (using a STATIC control)
     * within the specified window. It stores its HWND in
     * `NativeWindowData.controls`.
     * The status bar's position and size will be managed by `WM_SIZE`.
     */
    fn _handle_create_status_bar_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
        initial_text: String,
    ) -> PlatformResult<()> {
        log::debug!(
            "Platform: Handling CreateStatusBar command for WinID {:?}, CtrlID {}, Text: '{}'",
            window_id,
            control_id,
            initial_text
        );
        let mut windows_map_guard = self.window_map.write().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to lock windows map for status bar creation".into(),
            )
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            if window_data.controls.contains_key(&control_id) {
                return Err(PlatformError::OperationFailed(format!(
                    "Status bar with ID {} already present for window {:?}",
                    control_id, window_id
                )));
            }

            let hwnd_status_bar = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    WC_STATIC,
                    &HSTRING::from(initial_text.as_str()),
                    WS_CHILD | WS_VISIBLE | SS_LEFT,
                    0,
                    0,
                    0,
                    0, // Dummies, WM_SIZE will adjust
                    Some(window_data.hwnd),
                    Some(HMENU(control_id as *mut c_void)),
                    Some(self.h_instance),
                    None,
                )?
            };
            window_data.controls.insert(control_id, hwnd_status_bar);
            window_data.status_bar_current_text = initial_text.clone();
            window_data.status_bar_current_severity = MessageSeverity::Information;

            log::debug!(
                "Platform: Created status bar (ID {}) for window {:?} with HWND {:?}, initial text: '{}'",
                control_id,
                window_id,
                hwnd_status_bar,
                initial_text
            );
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateStatusBar",
                window_id
            )))
        }
    }

    /*
     * Handles the `PlatformCommand::CreateTreeView` command.
     * This method creates a native TreeView control within the specified window.
     * It stores its HWND in `NativeWindowData.controls` and initializes
     * `NativeWindowData.treeview_state` with the `TreeViewInternalState`.
     * The TreeView's position and size will be managed by `WM_SIZE`.
     */
    fn _handle_create_treeview_impl(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
    ) -> PlatformResult<()> {
        log::debug!(
            "Platform: Handling CreateTreeView command for WinID {:?}, CtrlID {}",
            window_id,
            control_id
        );
        let mut windows_map_guard = self.window_map.write().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to lock windows map for TreeView creation".into(),
            )
        })?;

        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            if window_data.controls.contains_key(&control_id)
                || window_data.treeview_state.is_some()
            {
                return Err(PlatformError::ControlCreationFailed(format!(
                    "TreeView with ID {} or existing TreeView state already present for window {:?}",
                    control_id, window_id
                )));
            }

            // Get initial dimensions based on parent window's client area (dummy values if not available yet)
            let mut client_rect = RECT::default();
            if !window_data.hwnd.is_invalid() {
                unsafe { GetClientRect(window_data.hwnd, &mut client_rect)? };
            } else {
                // Fallback if parent HWND not fully ready (should not happen if create_window is called first)
                client_rect.right = 600; // Arbitrary default
                client_rect.bottom = 400; // Arbitrary default
            }

            let tvs_style = WINDOW_STYLE(
                TVS_HASLINES
                    | TVS_LINESATROOT
                    | TVS_HASBUTTONS
                    | TVS_SHOWSELALWAYS
                    | TVS_CHECKBOXES,
            );
            let combined_style = WS_CHILD | WS_VISIBLE | WS_BORDER | tvs_style;

            // Initial size, WM_SIZE will adjust it correctly later.
            // Subtract placeholder for button area and status bar.
            let tv_width = client_rect.right - client_rect.left;
            let tv_height = client_rect.bottom
                - client_rect.top
                - BUTTON_AREA_HEIGHT
                - window_common::STATUS_BAR_HEIGHT;

            let hwnd_tv = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    WC_TREEVIEWW,
                    PCWSTR::null(),
                    combined_style,
                    0,                 // X
                    0,                 // Y
                    tv_width.max(10),  // nWidth, ensure non-zero
                    tv_height.max(10), // nHeight, ensure non-zero
                    Some(window_data.hwnd),
                    Some(HMENU(control_id as *mut c_void)),
                    Some(self.h_instance),
                    None,
                )?
            };

            window_data.controls.insert(control_id, hwnd_tv);
            window_data.treeview_state = Some(control_treeview::TreeViewInternalState::new()); // HWND no longer passed

            log::debug!(
                "Platform: Created TreeView (ID {}) for window {:?} with HWND {:?}",
                control_id,
                window_id,
                hwnd_tv
            );
            Ok(())
        } else {
            Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateTreeView",
                window_id
            )))
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
            menu_action_map: HashMap::new(),  // Initialize new field
            next_menu_item_id_counter: 30000, // Initialize new field
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
                        window_data.get_control_hwnd(ID_STATUS_BAR_CTRL)
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
                    // Step 1: Dequeue a command. The lock is held only for this operation.
                    let command_to_execute: Option<PlatformCommand> = {
                        // Scope for the MutexGuard
                        match app_logic_ref.lock() {
                            Ok(mut logic_guard) => logic_guard.try_dequeue_command(),
                            Err(e) => {
                                log::error!(
                                    "Platform: Failed to lock MyAppLogic to dequeue command: {:?}. Skipping command processing for this cycle.",
                                    e
                                );
                                None // Treat as no command available this iteration
                            }
                        }
                    }; // MutexGuard (logic_guard) is dropped here, releasing the lock.

                    // Step 2: Execute the command if one was dequeued. MyAppLogic is NOT locked here.
                    if let Some(command) = command_to_execute {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            log::error!("Platform: Error executing command from queue: {:?}", e);
                            // Decide on error handling: continue, break, or return?
                            // For now, log and continue processing other commands/messages.
                        }
                    } else {
                        // No more commands in the queue.
                        break;
                    }
                }

                // Process Windows messages. This will block until a message is received.
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                } else if result.0 == 0 {
                    log::debug!(
                        "Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop."
                    );
                    break; // WM_QUIT
                } else {
                    let last_error = GetLastError();
                    log::error!(
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

// Given a slice of UTF-16 code units (with a trailing 0), produce a PathBuf.
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
    use crate::platform_layer::types::MenuAction; // For test
    use crate::platform_layer::types::MenuItemConfig; // For test
    use std::ptr; // Added for null_mut

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

    #[test]
    fn test_menu_id_generation_and_mapping() {
        let internal_state_arc = Win32ApiInternalState::new("TestApp".to_string()).unwrap();
        let window_id = internal_state_arc.generate_window_id();
        let mut native_window_data = window_common::NativeWindowData {
            hwnd: HWND(ptr::null_mut()), // Dummy HWND for this test
            id: window_id,
            treeview_state: None,
            controls: HashMap::new(),
            status_bar_current_text: String::new(),
            status_bar_current_severity: MessageSeverity::None,
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 30000,
        };

        let menu_items = vec![
            MenuItemConfig {
                action: Some(MenuAction::LoadProfile),
                text: "Load".to_string(),
                children: vec![],
            },
            MenuItemConfig {
                action: None, // This is a popup
                text: "File".to_string(),
                children: vec![MenuItemConfig {
                    action: Some(MenuAction::SaveProfileAs),
                    text: "Save As".to_string(),
                    children: vec![],
                }],
            },
            MenuItemConfig {
                action: Some(MenuAction::RefreshFileList),
                text: "Refresh".to_string(),
                children: vec![],
            },
        ];

        unsafe {
            let h_main_menu = CreateMenu().unwrap();
            for item_config in &menu_items {
                internal_state_arc
                    .add_menu_item_recursive(h_main_menu, item_config, &mut native_window_data)
                    .unwrap();
            }
            DestroyMenu(h_main_menu).unwrap(); // Clean up
        }

        assert_eq!(native_window_data.menu_action_map.len(), 3); // Load, Save As, Refresh
        assert_eq!(native_window_data.next_menu_item_id_counter, 30003);

        // Check if the actions are correctly mapped to generated IDs
        let mut found_load = false;
        let mut found_save_as = false;
        let mut found_refresh = false;

        for (id, action) in &native_window_data.menu_action_map {
            assert!(*id >= 30000 && *id < 30003); // IDs should be in the generated range
            match action {
                MenuAction::LoadProfile => found_load = true,
                MenuAction::SaveProfileAs => found_save_as = true,
                MenuAction::RefreshFileList => found_refresh = true,
                _ => panic!("Unexpected action in map"),
            }
        }
        assert!(found_load, "LoadProfile action not found in map");
        assert!(found_save_as, "SaveProfileAs action not found in map");
        assert!(found_refresh, "RefreshFileList action not found in map");
    }
}
