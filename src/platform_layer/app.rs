use super::command_executor;
use super::control_treeview;
use super::dialog_handler;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{PlatformCommand, PlatformEventHandler, TreeItemId, WindowConfig, WindowId};
use super::window_common;

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, RECT, WPARAM},
        Graphics::Gdi::InvalidateRect,
        System::Com::{CoInitializeEx, CoUninitialize},
        System::LibraryLoader::GetModuleHandleW,
        UI::Controls::{
            ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TVM_GETITEMRECT,
        },
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
                is_quitting: AtomicUsize::new(0), // Initialize is_quitting
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
                // Decide on a safe default. If poisoned, it's hard to know the true state.
                // Assuming not empty might prevent premature quit but could also lead to hanging if state is truly empty.
                // For now, let's default to false (i.e., assume not empty) to be cautious about premature quitting.
                false
            },
            |guard| guard.is_empty()
        );

        // Quit if no windows are left, OR if a quit was previously signaled and now no windows are left.
        // The `is_quitting` flag handles cases where QuitApplication is called before the last window closes.
        if no_active_windows {
            log::debug!(
                "Platform: Last active window closed (or was already closed and quit signaled). Posting WM_QUIT."
            );
            unsafe { PostQuitMessage(0) };
        }
    }

    /*
     * Executes the `RedrawTreeItem` command, triggering an immediate repaint
     * for a specific item in a TreeView. This is vital for updating custom-drawn
     * elements, like the "New" item indicator, promptly after its underlying
     * data state changes. The method translates the logical `item_id` to its
     * native TreeView and item handles.
     *
     * Using these native handles, it retrieves the item's bounding rectangle via
     * the `TVM_GETITEMRECT` message. A successful retrieval leads to invalidating
     * just that specific rectangle with `InvalidateRect`. This action signals
     * the OS to redraw the area, initiating the `NM_CUSTOMDRAW` sequence for the
     * item and allowing its visual state to be updated. If getting the specific
     * rectangle fails, the entire TreeView is invalidated as a fallback.
     */
    fn _execute_redraw_tree_item(
        self: &Arc<Self>,
        window_id: WindowId,
        item_id: TreeItemId,
    ) -> PlatformResult<()> {
        log::debug!(
            "Win32ApiInternalState: _execute_redraw_tree_item for WinID {:?}, ItemID {:?}",
            window_id,
            item_id
        );

        let windows_guard = self.active_windows.read().map_err(|e| {
            log::error!(
                "Failed to acquire read lock on windows map for RedrawTreeItem: {:?}",
                e
            );
            PlatformError::OperationFailed(
                "Failed to lock active_windows map for RedrawTreeItem".into(),
            )
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            log::warn!("WindowId {:?} not found for RedrawTreeItem.", window_id);
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for RedrawTreeItem",
                window_id
            ))
        })?;

        let hwnd_treeview = window_data
            .get_control_hwnd(control_treeview::ID_TREEVIEW_CTRL)
            .ok_or_else(|| {
                log::warn!(
                    "TreeView control not found for WinID {:?} during RedrawTreeItem.",
                    window_id
                );
                PlatformError::InvalidHandle(format!(
                    "TreeView control not found for WinID {:?} during RedrawTreeItem.",
                    window_id
                ))
            })?;

        if hwnd_treeview.is_invalid() {
            log::warn!(
                "TreeView HWND is invalid for WinID {:?} during RedrawTreeItem.",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "TreeView HWND is invalid for WinID {:?} during RedrawTreeItem.",
                window_id
            )));
        }

        let tv_state = window_data.treeview_state.as_ref().ok_or_else(|| {
            log::warn!(
                "TreeView state not found for WinID {:?} during RedrawTreeItem.",
                window_id
            );
            PlatformError::OperationFailed(format!(
                "TreeView state not found for WinID {:?} during RedrawTreeItem.",
                window_id
            ))
        })?;

        let htreeitem = tv_state.item_id_to_htreeitem.get(&item_id).ok_or_else(|| {
            log::warn!(
                "HTREEITEM not found for ItemID {:?} during RedrawTreeItem. Cannot invalidate.",
                item_id
            );
            PlatformError::InvalidHandle(format!(
                "HTREEITEM not found for ItemID {:?} during RedrawTreeItem.",
                item_id
            ))
        })?;

        let mut item_rect = RECT::default();

        // For TVM_GETITEMRECT, the HTREEITEM of the target item is passed by
        // setting it as the value of the `left` field of the RECT structure
        // pointed to by lParam. This is true whether wParam (fTextOnly) is
        // TRUE (for text-only part) or FALSE (for the entire item line).
        // We use wParam=0 (FALSE) to get the full line for invalidation.
        item_rect.left = htreeitem.0 as i32; // Set the HTREEITEM into the RECT.left

        let get_rect_success = unsafe {
            SendMessageW(
                hwnd_treeview,
                TVM_GETITEMRECT,
                Some(WPARAM(0)), // FALSE (0) for entire item line, not just text.
                Some(LPARAM(&mut item_rect as *mut _ as isize)),
            )
        };

        if get_rect_success.0 != 0 {
            // Non-zero means success, item_rect is now populated
            unsafe {
                _ = InvalidateRect(Some(hwnd_treeview), Some(&item_rect), true);
            }
            log::debug!(
                "Invalidated rect {:?} for item ID {:?} (HTREEITEM {:?})",
                item_rect,
                item_id,
                htreeitem
            );
        } else {
            log::warn!(
                "TVM_GETITEMRECT failed for item ID {:?} (HTREEITEM {:?}) during RedrawTreeItem. Invalidating whole control. Error: {:?}",
                item_id,
                htreeitem,
                unsafe { GetLastError() }
            );
            unsafe {
                _ = InvalidateRect(Some(hwnd_treeview), None, true);
            }
        }
        Ok(())
    }

    /*
     * Executes a single platform command.
     * Delegates to specific handlers in `command_executor` or `dialog_handler`.
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
            PlatformCommand::CreatePanel {
                window_id,
                parent_control_id,
                panel_id,
            } => {
                command_executor::execute_create_panel(self, window_id, parent_control_id, panel_id)
            }
            PlatformCommand::CreateLabel {
                window_id,
                parent_panel_id,
                label_id,
                initial_text,
                class,
            } => command_executor::execute_create_label(
                self,
                window_id,
                parent_panel_id,
                label_id,
                initial_text,
                class,
            ),
            PlatformCommand::UpdateLabelText {
                window_id,
                label_id,
                text,
                severity,
            } => command_executor::execute_update_label_text(
                self, window_id, label_id, text, severity,
            ),
            PlatformCommand::RedrawTreeItem { window_id, item_id } => {
                self._execute_redraw_tree_item(window_id, item_id) // Call the new method
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

        // Create preliminary data for the window map. HWND will be filled after creation.
        let preliminary_native_data = window_common::NativeWindowData {
            hwnd: HWND(std::ptr::null_mut()), // Invalid HWND initially
            id: window_id,
            treeview_state: None,
            controls: HashMap::new(),
            menu_action_map: HashMap::new(),
            next_menu_item_id_counter: 30000, // Default starting ID for menu items
            layout_rules: None,
            label_severities: HashMap::new(),
            status_bar_font: None,
        };

        // Insert preliminary data into the active_windows map.
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

        // Attempt to create the native window.
        let hwnd = match window_common::create_native_window(
            &self.internal_state,
            window_id,
            &config.title,
            config.width,
            config.height,
        ) {
            Ok(h) => h,
            Err(e) => {
                // If creation fails, attempt to remove the preliminary data.
                // Log any error during removal but return the original creation error.
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
                return Err(e); // Return the original creation error
            }
        };
        log::debug!(
            "Platform: Native window created with HWND {:?} for WindowId {:?}",
            hwnd,
            window_id
        );

        // Update the NativeWindowData with the actual HWND.
        match self.internal_state.active_windows.write() {
            Ok(mut windows_map_guard) => {
                if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                    window_data.hwnd = hwnd;
                    log::debug!(
                        "Platform: Updated HWND in NativeWindowData for WindowId {:?}.",
                        window_id,
                    );
                } else {
                    // This should ideally not happen if insert succeeded and no other thread removed it.
                    log::error!(
                        "Platform: CRITICAL - Preliminary NativeWindowData for WindowId {:?} vanished before HWND update.",
                        window_id
                    );
                    // Attempt to destroy the orphaned window if possible.
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
                // Attempt to destroy the orphaned window if possible.
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
        // Store a weak reference to the application's event handler.
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

        // Execute initial UI setup commands.
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

        // Keep a strong reference to the event handler for the duration of the run loop.
        let app_logic_ref_for_loop = event_handler_param;
        unsafe {
            let mut msg = MSG::default();
            loop {
                // Process commands from the application logic queue.
                loop {
                    let command_to_execute: Option<PlatformCommand> = {
                        match app_logic_ref_for_loop.lock() {
                            Ok(mut logic_guard) => logic_guard.try_dequeue_command(),
                            Err(e) => {
                                log::error!(
                                    "Platform: Failed to lock application logic to dequeue command: {:?}. Skipping command processing for this cycle.",
                                    e
                                );
                                None
                            }
                        }
                    };

                    if let Some(command) = command_to_execute {
                        if let Err(e) = self.internal_state._execute_platform_command(command) {
                            log::error!("Platform: Error executing command from queue: {:?}", e);
                            // Continue processing other commands/messages.
                        }
                    } else {
                        break; // No more commands in the queue.
                    }
                }

                // Process Windows messages.
                let result = GetMessageW(&mut msg, None, 0, 0);
                if result.0 > 0 {
                    // Message other than WM_QUIT
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
                    // Check if we are already in a quit sequence and no windows are left.
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
                        windows::core::Error::from_win32()
                    )));
                }
            }
        }
        // Application quit: notify the application logic.
        if let Ok(mut handler_guard) = app_logic_ref_for_loop.lock() {
            handler_guard.on_quit();
        } else {
            log::error!("Platform: Failed to lock application logic for on_quit call.");
        }
        match self.internal_state.application_event_handler.lock() {
            Ok(mut guard) => *guard = None,
            Err(e) => {
                log::error!(
                    "Platform: Failed to lock application_event_handler to clear it (poisoned): {:?}",
                    e
                );
                // If this fails due to poisoning, the program is exiting anyway.
                // We can't easily "fix" the poisoned lock at this stage for this specific operation.
            }
        }
        log::debug!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

#[cfg(test)]
mod app_tests {
    // use crate::platform_layer::window_common::HWND_INVALID; // Not directly used in these tests
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    // This test helper remains for historical reasons or if other parts of app_tests use it.
    // The actual pathbuf_from_buf used by dialogs is now in dialog_handler.rs.
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
