/*
 * This module is responsible for handling platform-specific dialog interactions.
 * It implements the logic to display various dialogs (e.g., file open/save,
 * input, folder picker) based on commands received from the application logic.
 * It uses the Win32 API for dialog creation and management, and communicates
 * results back to the application logic via `AppEvent`s.
 */

use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{AppEvent, WindowId};
use crate::platform_layer::window_common; // For ID_DIALOG_INPUT_EDIT, ID_DIALOG_INPUT_PROMPT_STATIC etc.

use std::ffi::{OsString, c_void};
use std::mem::{align_of, size_of};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::Arc;

use windows::{
    Win32::{
        Foundation::{FALSE, HWND, LPARAM, TRUE, WPARAM},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
        UI::Controls::Dialogs::*, // Contains DLGTEMPLATE, DLGITEMTEMPLATE, OPENFILENAMEW etc.
        UI::Shell::{
            FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
            SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
        },
        UI::WindowsAndMessaging::*, // Contains GetSaveFileNameW, DialogBoxIndirectParamW etc.
    },
    core::{HSTRING, PCWSTR},
};

/*
 * Creates a `PathBuf` from a null-terminated or unterminated slice of UTF-16 code units.
 *
 * This utility function is used to convert wide-character string buffers,
 * often received from Win32 API calls (like file dialogs), into Rust's `PathBuf`.
 * It searches for the first null terminator to determine the string's length;
 * if no null terminator is found, the entire buffer is used.
 */
pub(crate) fn pathbuf_from_buf(buffer: &[u16]) -> PathBuf {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let path_os_string = OsString::from_wide(&buffer[..len]);
    PathBuf::from(path_os_string)
}

/*
 * Retrieves the owner HWND for a given WindowId.
 * This is a helper function that uses the with_window_data_read pattern to
 * safely access the native window handle, encapsulating the locking and
 * error handling logic.
 */
pub(crate) fn get_hwnd_owner(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
) -> PlatformResult<HWND> {
    internal_state.with_window_data_read(window_id, |window_data| {
        let hwnd = window_data.get_hwnd();
        if hwnd.is_invalid() {
            log::warn!(
                "get_hwnd_owner found an invalid HWND for WindowId {:?}",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "HWND for WindowId {:?} is invalid",
                window_id
            )));
        }
        Ok(hwnd)
    })
}

/*
 * Displays a standard Win32 file dialog (Open or Save As).
 * This is a generic helper function used by `handle_show_open_file_dialog_command`
 * and `handle_show_save_file_dialog_command`. It handles the common setup
 * for `OPENFILENAMEW` and processes the dialog result, then sends an
 * appropriate `AppEvent` constructed by `event_constructor`.
 */
fn show_common_file_dialog<FDialog, FEvent>(
    internal_state: &Arc<Win32ApiInternalState>,
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
    // Retrieve the owner HWND for the dialog.
    let hwnd_owner = get_hwnd_owner(internal_state, window_id)?;

    // Prepare buffer for the file path.
    let mut file_buffer: Vec<u16> = vec![0; 2048]; // Buffer for the path.
    if let Some(fname) = default_filename {
        if !fname.is_empty() {
            let default_name_utf16: Vec<u16> = fname.encode_utf16().collect();
            let len_to_copy = std::cmp::min(default_name_utf16.len(), file_buffer.len() - 1);
            file_buffer[..len_to_copy].copy_from_slice(&default_name_utf16[..len_to_copy]);
        }
    }

    // Prepare strings for the OPENFILENAMEW struct.
    let title_hstring = HSTRING::from(title);
    let filter_utf16: Vec<u16> = filter_spec.encode_utf16().collect(); // Ensure null termination is handled by spec.
    let initial_dir_hstring = initial_dir.map(|p| HSTRING::from(p.to_string_lossy().as_ref()));
    let initial_dir_pcwstr = initial_dir_hstring
        .as_ref()
        .map_or(PCWSTR::null(), |h_str| PCWSTR(h_str.as_ptr()));

    // Initialize OPENFILENAMEW struct.
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd_owner,
        lpstrFile: windows::core::PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrFilter: PCWSTR(filter_utf16.as_ptr()),
        lpstrTitle: PCWSTR(title_hstring.as_ptr()),
        lpstrInitialDir: initial_dir_pcwstr,
        Flags: OFN_EXPLORER | specific_flags, // Base flags + dialog-specific flags.
        ..Default::default()
    };

    // Call the appropriate dialog function (GetOpenFileNameW or GetSaveFileNameW).
    let dialog_succeeded = dialog_fn(&mut ofn).as_bool();
    let mut path_result: Option<PathBuf> = None;

    if dialog_succeeded {
        path_result = Some(pathbuf_from_buf(&file_buffer)); // Use our helper.
        log::debug!(
            "DialogHandler: Dialog function succeeded. Path: {:?}",
            path_result.as_ref().unwrap()
        );
    } else {
        // Check for errors if the dialog didn't succeed.
        // CommDlgExtendedError returns 0 if the user cancelled.
        let error_code = unsafe { CommDlgExtendedError() };
        if error_code != COMMON_DLG_ERRORS(0) {
            log::error!(
                "DialogHandler: Dialog function failed or was cancelled with error. CommDlgExtendedError: {:?}",
                error_code
            );
        } else {
            log::debug!("DialogHandler: Dialog cancelled by user (no error).");
        }
    }

    // Construct and send the event to the application logic.
    let event = event_constructor(window_id, path_result);
    internal_state.send_event(event);
    Ok(())
}

/*
 * Handles the `ShowSaveFileDialog` platform command.
 * It uses `show_common_file_dialog` to display a Win32 "Save As" dialog and
 * sends an `AppEvent::FileSaveDialogCompleted` upon completion. This function
 * is called by `Win32ApiInternalState::_execute_platform_command`.
 */
pub(crate) fn handle_show_save_file_dialog_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: String,
    default_filename: String,
    filter_spec: String,
    initial_dir: Option<PathBuf>,
) -> PlatformResult<()> {
    show_common_file_dialog(
        internal_state,
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

/*
 * Handles the `ShowOpenFileDialog` platform command.
 * It uses `show_common_file_dialog` to display a Win32 "Open" dialog and
 * sends an `AppEvent::FileOpenProfileDialogCompleted` upon completion. This function
 * is called by `Win32ApiInternalState::_execute_platform_command`.
 */
pub(crate) fn handle_show_open_file_dialog_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: String,
    filter_spec: String,
    initial_dir: Option<PathBuf>,
) -> PlatformResult<()> {
    show_common_file_dialog(
        internal_state,
        window_id,
        title,
        None, // No default filename for open dialog
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
 * Handles the `ShowProfileSelectionDialog` platform command (currently a stub).
 * This function simulates the display of a profile selection dialog. In a full
 * implementation, it would create a custom dialog window. For now, it logs
 * the request and immediately sends a predefined `AppEvent::ProfileSelectionDialogCompleted`
 * to the application logic, simulating a user choice or cancellation based on
 * the input parameters. This function is called by `Win32ApiInternalState::_execute_platform_command`.
 */
pub(crate) fn handle_show_profile_selection_dialog_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    available_profiles: Vec<String>,
    title: String,
    prompt: String,
    emphasize_create_new: bool,
) -> PlatformResult<()> {
    log::debug!(
        "DialogHandler (STUB): Showing Profile Selection Dialog. Title: '{}', Prompt: '{}', Emphasize Create: {}, Profiles: {:?}",
        title,
        prompt,
        emphasize_create_new,
        available_profiles
    );

    let (chosen_profile_name, create_new_requested, cancelled) = if !available_profiles.is_empty()
        && !emphasize_create_new
    {
        (Some(available_profiles[0].clone()), false, false)
    } else if emphasize_create_new || available_profiles.is_empty() {
        (None, true, false)
    } else {
        log::warn!(
            "DialogHandler (STUB): ProfileSelectionDialog in unexpected state, simulating cancel."
        );
        (None, false, true)
    };
    log::debug!(
        "DialogHandler (STUB): Simulating dialog result: chosen='{:?}', create_new={}, cancelled={}",
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

    internal_state.send_event(event);
    Ok(())
}

// --- Input Dialog Implementation ---

/*
 * Internal data structure passed to the `input_dialog_proc`.
 * It holds the text for the prompt, the current input text (which can be
 * a default value and is updated by the user), an optional context tag
 * to identify the purpose of the dialog, and a success flag.
 */
struct InputDialogData {
    prompt_text: String,
    input_text: String,
    success: bool, // True if OK was pressed, false if Cancel or closed
}

// Helper to extract the low word from WPARAM, typically a command ID.
fn loword_from_wparam_input_dlg(wparam: WPARAM) -> u16 {
    (wparam.0 & 0xFFFF) as u16
}

/*
 * Dialog procedure for the custom input dialog.
 * Handles messages like WM_INITDIALOG to set initial text and WM_COMMAND
 * to process OK/Cancel button clicks, retrieving the entered text.
 */
unsafe extern "system" fn input_dialog_proc(
    hdlg: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match msg {
        WM_INITDIALOG => {
            // Store the InputDialogData pointer in the dialog's user data.
            unsafe {
                SetWindowLongPtrW(hdlg, GWLP_USERDATA, lparam.0);
            }
            // Retrieve the data and set initial text for prompt and edit control.
            let dialog_data = unsafe { &*(lparam.0 as *const InputDialogData) };
            let h_prompt = HSTRING::from(dialog_data.prompt_text.as_str());
            unsafe {
                SetDlgItemTextW(
                    hdlg,
                    window_common::ID_DIALOG_INPUT_PROMPT_STATIC, // Use ID from window_common
                    &h_prompt,
                )
                .unwrap_or_default();
            }
            if !dialog_data.input_text.is_empty() {
                let h_edit_text = HSTRING::from(dialog_data.input_text.as_str());
                unsafe {
                    SetDlgItemTextW(hdlg, window_common::ID_DIALOG_INPUT_EDIT, &h_edit_text) // Use ID
                        .unwrap_or_default();
                }
            }
            TRUE.0 as isize // Indicates message was processed.
        }
        WM_COMMAND => {
            let command_id = loword_from_wparam_input_dlg(wparam);
            match command_id {
                x if x == IDOK.0 as u16 => {
                    // Retrieve InputDialogData pointer.
                    let dialog_data_ptr =
                        unsafe { GetWindowLongPtrW(hdlg, GWLP_USERDATA) } as *mut InputDialogData;
                    if !dialog_data_ptr.is_null() {
                        let dialog_data = unsafe { &mut *dialog_data_ptr };
                        // Get text from edit control.
                        if let Ok(hwnd_edit_ok) =
                            unsafe { GetDlgItem(Some(hdlg), window_common::ID_DIALOG_INPUT_EDIT) }
                        // Use ID
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
                    TRUE.0 as isize
                }
                x if x == IDCANCEL.0 as u16 => {
                    let dialog_data_ptr =
                        unsafe { GetWindowLongPtrW(hdlg, GWLP_USERDATA) } as *mut InputDialogData;
                    if !dialog_data_ptr.is_null() {
                        unsafe { (*dialog_data_ptr).success = false };
                    }
                    unsafe { EndDialog(hdlg, IDCANCEL.0 as isize).unwrap_or_default() };
                    TRUE.0 as isize
                }
                _ => FALSE.0 as isize, // Message not processed.
            }
        }
        _ => FALSE.0 as isize, // Message not processed.
    }
}

// Helper to push a u16 word (little-endian) to a byte vector.
fn push_word(vec: &mut Vec<u8>, word: u16) {
    vec.extend_from_slice(&word.to_le_bytes());
}

// Helper to push a null-terminated UTF-16 string to a byte vector.
fn push_str_utf16(vec: &mut Vec<u8>, s: &str) {
    for c in s.encode_utf16() {
        push_word(vec, c);
    }
    push_word(vec, 0); // Null terminator
}

// Helper to align a byte vector to a DWORD (4-byte) boundary.
fn align_to_dword(vec: &mut Vec<u8>) {
    while vec.len() % align_of::<u32>() != 0 {
        vec.push(0);
    }
}

/*
 * Builds a Win32 dialog template in memory for the input dialog.
 * This function constructs the binary representation of a DLGTEMPLATE
 * and its associated DLGITEMTEMPLATEs for the prompt, edit box, and buttons.
 */
fn build_input_dialog_template(
    template_bytes: &mut Vec<u8>,
    title_str: &str,
    _prompt_str: &str, // Prompt string is set via SetDlgItemText in WM_INITDIALOG
) -> PlatformResult<()> {
    // --- DLGTEMPLATE ---
    let style = DS_CENTER | DS_MODALFRAME | DS_SETFONT; // Dialog styles
    let dlg_template = DLGTEMPLATE {
        style: style as u32 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_POPUP.0,
        dwExtendedStyle: 0,
        cdit: 4, // Number of controls: Prompt, Edit, OK, Cancel
        x: 0,    // Centered by DS_CENTER
        y: 0,    // Centered by DS_CENTER
        cx: 200, // Dialog units
        cy: 80,  // Dialog units
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(dlg_template) as *const [u8; size_of::<DLGTEMPLATE>()])
    });

    // Menu (none), Class (default), Title
    push_word(template_bytes, 0);
    push_word(template_bytes, 0);
    push_str_utf16(template_bytes, title_str);

    // Font (Pointsize, Name) if DS_SETFONT is used
    push_word(template_bytes, 8);
    push_str_utf16(template_bytes, "MS Shell Dlg");

    // --- DLGITEMTEMPLATE for Prompt Static Text ---
    align_to_dword(template_bytes);
    let static_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | window_common::SS_LEFT.0, // Use SS_LEFT from window_common
        dwExtendedStyle: 0,
        x: 10,
        y: 10,
        cx: 180,
        cy: 10,
        id: window_common::ID_DIALOG_INPUT_PROMPT_STATIC as u16, // Use ID from window_common
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(static_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Static"); // Control class
    push_str_utf16(template_bytes, "Placeholder for prompt"); // Text set via SetDlgItemText
    push_word(template_bytes, 0); // No creation data

    // --- DLGITEMTEMPLATE for Edit Control ---
    align_to_dword(template_bytes);
    let edit_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32,
        dwExtendedStyle: 0,
        x: 10,
        y: 25,
        cx: 180,
        cy: 12,
        id: window_common::ID_DIALOG_INPUT_EDIT as u16, // Use ID from window_common
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(edit_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Edit");
    push_word(template_bytes, 0); // No initial text here
    push_word(template_bytes, 0); // No creation data

    // --- DLGITEMTEMPLATE for OK Button ---
    align_to_dword(template_bytes);
    let ok_button_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | BS_DEFPUSHBUTTON as u32,
        dwExtendedStyle: 0,
        x: 40,
        y: 50,
        cx: 50,
        cy: 14,
        id: IDOK.0 as u16, // Standard OK ID
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(ok_button_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Button");
    push_str_utf16(template_bytes, "OK");
    push_word(template_bytes, 0);

    // --- DLGITEMTEMPLATE for Cancel Button ---
    align_to_dword(template_bytes);
    let cancel_button_item = DLGITEMTEMPLATE {
        style: WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32,
        dwExtendedStyle: 0,
        x: 110,
        y: 50,
        cx: 50,
        cy: 14,
        id: IDCANCEL.0 as u16, // Standard Cancel ID
    };
    template_bytes.extend_from_slice(unsafe {
        &*(std::ptr::addr_of!(cancel_button_item) as *const [u8; size_of::<DLGITEMTEMPLATE>()])
    });
    push_str_utf16(template_bytes, "Button");
    push_str_utf16(template_bytes, "Cancel");
    push_word(template_bytes, 0);

    Ok(())
}

/*
 * Handles the `ShowInputDialog` platform command.
 * This function creates and displays a modal input dialog using a dynamically
 * constructed dialog template. It allows the user to enter text.
 * Upon completion (OK or Cancel), it sends an `AppEvent::GenericInputDialogCompleted`
 * with the entered text (if any) and the original context tag. This function
 * is called by `Win32ApiInternalState::_execute_platform_command`.
 */
pub(crate) fn handle_show_input_dialog_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: String,
    prompt: String,
    default_text: Option<String>,
    context_tag: Option<String>,
) -> PlatformResult<()> {
    log::debug!("DialogHandler: Showing Input Dialog. Title: '{}'", title);
    let hwnd_owner = get_hwnd_owner(internal_state, window_id)?;

    // Data to be passed to and modified by the dialog proc.
    let mut dialog_data = InputDialogData {
        prompt_text: prompt,
        input_text: default_text.unwrap_or_default(),
        success: false,
    };

    // Build the dialog template in memory.
    let mut template_bytes = Vec::<u8>::new();
    build_input_dialog_template(&mut template_bytes, &title, &dialog_data.prompt_text)?;

    // Show the modal dialog.
    let dialog_result = unsafe {
        DialogBoxIndirectParamW(
            Some(internal_state.h_instance()),
            template_bytes.as_ptr() as *const DLGTEMPLATE,
            Some(hwnd_owner),
            Some(input_dialog_proc),
            LPARAM(&mut dialog_data as *mut _ as isize),
        )
    };

    // Process the result.
    let final_text_result = if dialog_result != 0 && dialog_data.success {
        Some(dialog_data.input_text)
    } else {
        log::debug!(
            "DialogHandler: Input dialog cancelled or failed. Result: {:?}, Success flag: {}",
            dialog_result,
            dialog_data.success
        );
        None
    };

    // Send completion event.
    let event = AppEvent::GenericInputDialogCompleted {
        window_id,
        text: final_text_result,
        context_tag,
    };

    internal_state.send_event(event);
    Ok(())
}

/*
 * Handles the `ShowFolderPickerDialog` platform command.
 * This function uses the modern `IFileOpenDialog` with `FOS_PICKFOLDERS`
 * option to display a folder selection dialog.
 * Upon completion, it sends an `AppEvent::FolderPickerDialogCompleted` with the
 * selected folder path, if any. This function is called by
 * `Win32ApiInternalState::_execute_platform_command`.
 */
pub(crate) fn handle_show_folder_picker_dialog_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    title: String,
    initial_dir: Option<PathBuf>,
) -> PlatformResult<()> {
    log::debug!(
        "DialogHandler: Showing real Folder Picker Dialog. Title: '{}', Initial Dir: {:?}",
        title,
        initial_dir
    );
    let hwnd_owner = get_hwnd_owner(internal_state, window_id)?;
    let mut path_result: Option<PathBuf> = None;

    let file_dialog_result: Result<IFileOpenDialog, windows::core::Error> =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) };

    if let Ok(file_dialog) = file_dialog_result {
        unsafe {
            if let Err(e_opts) = file_dialog.SetOptions(FOS_PICKFOLDERS) {
                log::error!(
                    "DialogHandler: IFileOpenDialog::SetOptions failed: {:?}",
                    e_opts
                );
            }

            let h_title = HSTRING::from(title.as_str());
            if let Err(e_title) = file_dialog.SetTitle(&h_title) {
                log::error!(
                    "DialogHandler: IFileOpenDialog::SetTitle failed: {:?}",
                    e_title
                );
            }

            if let Some(dir_path) = &initial_dir {
                let dir_hstring = HSTRING::from(dir_path.as_os_str());
                match SHCreateItemFromParsingName::<_, _, IShellItem>(&dir_hstring, None) {
                    Ok(item) => {
                        if let Err(e_sdf) = file_dialog.SetDefaultFolder(&item) {
                            log::error!(
                                "DialogHandler: IFileOpenDialog::SetDefaultFolder failed: {:?}",
                                e_sdf
                            );
                        }
                    }
                    Err(e_csipn) => {
                        log::error!(
                            "DialogHandler: SHCreateItemFromParsingName for initial_dir {:?} failed: {:?}",
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
                    } else {
                        log::warn!(
                            "DialogHandler: GetDisplayName failed for folder picker result."
                        );
                    }
                } else {
                    log::debug!(
                        "DialogHandler: GetResult failed or no item selected in folder picker."
                    );
                }
            } else {
                log::debug!(
                    "DialogHandler: Folder picker dialog was cancelled or an error occurred during Show."
                );
            }
        }
    } else if let Err(e) = file_dialog_result {
        let err_msg = format!(
            "DialogHandler: CoCreateInstance for IFileOpenDialog failed: {:?}",
            e
        );
        log::error!("{}", err_msg);
        let event = AppEvent::FolderPickerDialogCompleted {
            window_id,
            path: None,
        };
        internal_state.send_event(event);
        return Err(PlatformError::OperationFailed(err_msg));
    }

    let event = AppEvent::FolderPickerDialogCompleted {
        window_id,
        path: path_result,
    };
    internal_state.send_event(event);
    Ok(())
}
