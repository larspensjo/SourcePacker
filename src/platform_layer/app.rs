use crate::platform_layer::{
    command_executor,
    controls::{
        button_handler, dialog_handler, label_handler, menu_handler, panel_handler,
        styling_handler, treeview_handler,
    },
    error::{PlatformError, Result as PlatformResult},
    styling::{ControlStyle, ParsedControlStyle, StyleId},
    types::{
        AppEvent, PlatformCommand, PlatformEventHandler, UiStateProvider, WindowConfig, WindowId,
    },
    window_common,
};

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, WPARAM},
        Graphics::Gdi::InvalidateRect,
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
    h_instance: HINSTANCE,
    next_window_id_counter: AtomicUsize, // For generating unique WindowIds
    // Central registry for all active windows, mapping WindowId to its native state.
    active_windows: RwLock<HashMap<WindowId, window_common::NativeWindowData>>,
    application_event_handler: Mutex<Option<Weak<Mutex<dyn PlatformEventHandler>>>>,
    ui_state_provider: Mutex<Option<Weak<Mutex<dyn UiStateProvider>>>>,
    // Stores processed, native-ready style definitions, keyed by a semantic ID.
    defined_styles: RwLock<HashMap<StyleId, Arc<ParsedControlStyle>>>,
    // The application name, used for window class registration.
    app_name_for_class: String,
    is_quitting: AtomicUsize, // 0 = false, 1 = true
}

// SAFETY: All fields are Send + Sync or wrapped in thread-safe containers, and trait objects are required to be Send + Sync.
unsafe impl Send for Win32ApiInternalState {}
unsafe impl Sync for Win32ApiInternalState {}

impl Win32ApiInternalState {
    /*
     * Generates a new unique `WindowId`.
     */
    pub(crate) fn generate_unique_window_id(&self) -> WindowId {
        WindowId(self.next_window_id_counter.fetch_add(1, Ordering::Relaxed))
    }

    /*
     * Retrieves the application's instance handle.
     * Control and window creation functions use this value when calling Win32 APIs.
     */
    pub(crate) fn h_instance(&self) -> HINSTANCE {
        self.h_instance
    }

    #[cfg(test)]
    pub(crate) fn active_windows(
        &self,
    ) -> &RwLock<HashMap<WindowId, window_common::NativeWindowData>> {
        &self.active_windows
    }

    pub(crate) fn ui_state_provider(&self) -> &Mutex<Option<Weak<Mutex<dyn UiStateProvider>>>> {
        &self.ui_state_provider
    }

    pub(crate) fn app_name_for_class(&self) -> &str {
        &self.app_name_for_class
    }

    /*
     * Creates a new instance of `Win32ApiInternalState`.
     * Initializes COM, common controls, and retrieves the application instance handle.
     */
    pub(crate) fn new(app_name_for_class: String) -> PlatformResult<Arc<Self>> {
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
                ui_state_provider: Mutex::new(None),
                defined_styles: RwLock::new(HashMap::new()),
                app_name_for_class,
                is_quitting: AtomicUsize::new(0),
            }))
        }
    }

    // Sends an AppEvent to the registered application event handler.
    // This centralizes the logic for locking, upgrading the weak reference,
    // and calling the handler.
    pub(crate) fn send_event(self: &Arc<Self>, event: AppEvent) {
        let event_handler_opt = self
            .application_event_handler
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|weak_handler| weak_handler.upgrade());

        if let Some(handler_arc) = event_handler_opt {
            if let Ok(mut handler_guard) = handler_arc.lock() {
                handler_guard.handle_event(event);
            } else {
                log::error!("Platform: Failed to lock event handler to send event.");
            }
        } else {
            log::warn!("Platform: Event handler not available to send event.");
        }
    }

    /*
     * Removes the data for a given window ID from the active windows map.
     * This is a map-level operation that acquires a write lock, removes the
     * entry, and then calls the necessary cleanup functions on the removed data
     * to release any associated GDI resources.
     */
    pub(crate) fn remove_window_data(&self, window_id: WindowId) {
        let mut windows_map_guard = match self.active_windows.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "Failed to acquire write lock to remove WinID {:?}: {:?}",
                    window_id,
                    e
                );
                return; // Exit if the lock is poisoned.
            }
        };

        if let Some(mut removed_data) = windows_map_guard.remove(&window_id) {
            // Perform cleanup on the GDI objects owned by the window data.
            removed_data.cleanup_status_bar_font();
            log::debug!("Removed and cleaned up data for WindowId {:?}.", window_id);
        } else {
            log::warn!("Attempted to remove non-existent WindowId {:?}.", window_id);
        }
    }
    /*
     * A specialized helper for mutating the TreeView's internal state.
     * This function safely takes the TreeView state out of the `NativeWindowData`,
     * executes the provided closure on it *without* holding the main window map lock,
     * and then puts the (potentially modified) state back. This is critical for
     * long-running operations like populating the tree, as it prevents deadlocks
     * and avoids blocking other UI commands.
     */
    pub(crate) fn with_treeview_state_mut<F>(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
        f: F,
    ) -> PlatformResult<()>
    where
        F: FnOnce(HWND, &mut treeview_handler::TreeViewInternalState) -> PlatformResult<()>,
    {
        // Phase 1: Lock, get HWND, and take the treeview state out.
        let (hwnd_treeview, mut taken_tv_state) =
        self.with_window_data_write(window_id, |window_data| {
            let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for control ID {}",
                    control_id
                ))
            })?;

            // Take the state. If it doesn't exist, create a new one for the operation.
            let state = window_data.take_treeview_state().unwrap_or_else(|| {
                log::warn!(
                    "TreeView state was None for WinID {:?}/ControlID {}. Creating new for operation.",
                    window_id,
                    control_id
                );
                treeview_handler::TreeViewInternalState::new()
            });

            Ok((hwnd, state))
        })?;

        // Phase 2: Perform the long-running operation on the state without holding the map lock.
        let result = f(hwnd_treeview, &mut taken_tv_state);

        // Phase 3: Lock again to put the state back, regardless of whether the operation succeeded.
        // This ensures the state is never lost.
        if let Err(e) = self.with_window_data_write(window_id, |window_data| {
            window_data.set_treeview_state(Some(taken_tv_state));
            Ok(())
        }) {
            log::error!(
                "CRITICAL: Failed to put back TreeView state for WinID {:?}. State may be lost. Error: {:?}",
                window_id,
                e
            );
        }

        // Return the original result from the operation.
        result
    }

    // Provides safe, read-only access to a specific window's data.
    // Handles locking and error checking.
    pub(crate) fn with_window_data_read<F, R>(&self, window_id: WindowId, f: F) -> PlatformResult<R>
    where
        F: FnOnce(&window_common::NativeWindowData) -> PlatformResult<R>,
    {
        let windows_map = self.active_windows.read().map_err(|e| {
            log::error!("Failed to acquire read lock on active_windows: {:?}", e);
            PlatformError::OperationFailed("RwLock poisoned".into())
        })?;

        let window_data = windows_map.get(&window_id).ok_or_else(|| {
            log::warn!("Attempted to access non-existent WindowId {:?}", window_id);
            PlatformError::InvalidHandle(format!("WindowId {:?} not found", window_id))
        })?;

        f(window_data)
    }

    // Provides safe, writeable access to a specific window's data.
    // Handles locking and error checking.
    pub(crate) fn with_window_data_write<F, R>(
        &self,
        window_id: WindowId,
        f: F,
    ) -> PlatformResult<R>
    where
        F: FnOnce(&mut window_common::NativeWindowData) -> PlatformResult<R>,
    {
        let mut windows_map = self.active_windows.write().map_err(|e| {
            log::error!("Failed to acquire write lock on active_windows: {:?}", e);
            PlatformError::OperationFailed("RwLock poisoned".into())
        })?;

        let window_data = windows_map.get_mut(&window_id).ok_or_else(|| {
            log::warn!("Attempted to access non-existent WindowId {:?}", window_id);
            PlatformError::InvalidHandle(format!("WindowId {:?} not found", window_id))
        })?;

        f(window_data)
    }

    /*
     * Returns `true` if no active windows remain. This pure check can be unit
     * tested without side effects and drives the quit logic when a window
     * closes.
     */
    fn should_quit_on_last_window_close(&self) -> bool {
        self.active_windows.read().map_or_else(
            |poisoned_err| {
                log::error!(
                    "Win32ApiInternalState: Poisoned RwLock on active_windows during quit check: {:?}",
                    poisoned_err
                );
                false
            },
            |guard| guard.is_empty(),
        )
    }

    /*
     * Called after a window is removed from the `active_windows` map. If this
     * was the last window, posts `WM_QUIT` so the message loop exits. The
     * `is_quitting` flag ensures we honor a prior quit request once all windows
     * have closed.
     */
    pub(crate) fn check_if_should_quit_after_window_close(&self) {
        if self.should_quit_on_last_window_close() {
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
    fn execute_platform_command(self: &Arc<Self>, command: PlatformCommand) -> PlatformResult<()> {
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
            } => menu_handler::handle_create_main_menu_command(self, window_id, menu_items),
            PlatformCommand::CreateButton {
                window_id,
                control_id,
                text,
            } => button_handler::handle_create_button_command(self, window_id, control_id, text),
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
            } => panel_handler::handle_create_panel_command(
                self,
                window_id,
                parent_control_id,
                panel_id,
            ),
            PlatformCommand::CreateLabel {
                window_id,
                parent_panel_id,
                control_id: label_id,
                initial_text,
                class,
            } => label_handler::handle_create_label_command(
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
            } => label_handler::handle_update_label_text_command(
                self, window_id, label_id, text, severity,
            ),
            PlatformCommand::RedrawTreeItem {
                window_id,
                control_id,
                item_id,
            } => treeview_handler::handle_redraw_tree_item_command(
                self, window_id, control_id, item_id,
            ),
            PlatformCommand::ExpandVisibleTreeItems {
                window_id,
                control_id,
            } => command_executor::execute_expand_visible_tree_items(self, window_id, control_id),
            PlatformCommand::ExpandAllTreeItems {
                window_id,
                control_id,
            } => command_executor::execute_expand_all_tree_items(self, window_id, control_id),
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
            PlatformCommand::DefineStyle { style_id, style } => {
                self.execute_define_style(style_id, style)
            }
            PlatformCommand::ApplyStyleToControl {
                window_id,
                control_id,
                style_id,
            } => self.execute_apply_style_to_control(window_id, control_id, style_id),
        }
    }

    /*
     * Executes the `DefineStyle` command.
     *
     * This method orchestrates the process of defining a style. It delegates the
     * complex parsing of the `ControlStyle` to the `styling_handler`, and then
     * performs the state modification by inserting the resulting `ParsedControlStyle`
     * into its own `defined_styles` map. This maintains proper encapsulation.
     */
    fn execute_define_style(
        self: &Arc<Self>,
        style_id: StyleId,
        style: ControlStyle,
    ) -> PlatformResult<()> {
        log::debug!(
            "Win32ApiInternalState: execute_define_style for StyleId::{:?}",
            style_id
        );

        // 1. Delegate parsing to the specialized handler.
        let parsed_style = styling_handler::parse_style(style)?;

        // 2. Modify the internal state.
        match self.defined_styles.write() {
            Ok(mut styles_map) => {
                styles_map.insert(style_id, Arc::new(parsed_style));
                log::debug!(
                    "Successfully stored parsed style for StyleId::{:?}",
                    style_id
                );
            }
            Err(e) => {
                log::error!(
                    "Failed to acquire write lock on defined_styles map: {:?}",
                    e
                );
                return Err(PlatformError::OperationFailed(
                    "RwLock poisoned on defined_styles map".to_string(),
                ));
            }
        }
        Ok(())
    }

    /*
     * Executes the `ApplyStyleToControl` command.
     *
     * This method applies a previously defined style to a specific control. It
     * updates the window's internal mapping, sends a `WM_SETFONT` message if a
     * font is part of the style, and invalidates the control to force a repaint,
     * which will trigger color changes via `WM_CTLCOLOR...` messages.
     */
    fn execute_apply_style_to_control(
        self: &Arc<Self>,
        window_id: WindowId,
        control_id: i32,
        style_id: StyleId,
    ) -> PlatformResult<()> {
        log::debug!(
            "Applying style {:?} to ControlID {} in WinID {:?}",
            style_id,
            control_id,
            window_id
        );

        // Get the control's HWND and store the style association in the window's data.
        let control_hwnd = self.with_window_data_write(window_id, |window_data| {
            window_data.apply_style_to_control(control_id, style_id);
            window_data.get_control_hwnd(control_id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "Control ID {} not found in WinID {:?}",
                    control_id, window_id
                ))
            })
        })?;

        if let Some(parsed_style) = self.get_parsed_style(style_id) {
            // Apply the font if one is defined in the style.
            if let Some(hfont) = parsed_style.font_handle {
                if !hfont.is_invalid() {
                    unsafe {
                        // SendMessageW is synchronous. The LPARAM(1) tells the control to redraw immediately.
                        SendMessageW(
                            control_hwnd,
                            WM_SETFONT,
                            Some(WPARAM(hfont.0 as usize)),
                            Some(LPARAM(1)),
                        );
                    }
                }
            }

            // Invalidate the control's rectangle to force a repaint.
            // This is crucial for `WM_CTLCOLOR...` messages to be sent, which will apply
            // the new text and background colors.
            unsafe {
                InvalidateRect(Some(control_hwnd), None, true);
            }
        } else {
            log::warn!("Attempted to apply undefined StyleId::{:?}", style_id);
        }

        Ok(())
    }

    /*
     * Retrieves a shared reference to a parsed style definition.
     *
     * This method provides safe, read-only access to the central style map.
     * It returns an `Arc<ParsedControlStyle>`, allowing multiple components to
     * share ownership of the style data (including native GDI handles) without
     * risking double-free errors.
     */
    pub(crate) fn get_parsed_style(&self, style_id: StyleId) -> Option<Arc<ParsedControlStyle>> {
        self.defined_styles
            .read()
            .ok()
            .and_then(|styles_map| styles_map.get(&style_id).cloned())
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
            config.title,
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
        ui_state_provider_param: Arc<Mutex<dyn UiStateProvider>>,
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

        *self.internal_state.ui_state_provider.lock().map_err(|e| {
            log::error!(
                "Platform: Failed to lock ui_state_provider to set it: {:?}",
                e
            );
            PlatformError::OperationFailed("Failed to set ui_state_provider".into())
        })? = Some(Arc::downgrade(&ui_state_provider_param));

        log::debug!(
            "Platform: run() called. Processing {} initial UI commands before event loop.",
            initial_commands_to_execute.len()
        );

        for command in initial_commands_to_execute {
            log::debug!("Platform: Executing initial command: {:?}", command);
            if let Err(e) = self.internal_state.execute_platform_command(command) {
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
                        if let Err(e) = self.internal_state.execute_platform_command(command) {
                            log::error!("Platform: Error executing command from queue: {:?}", e);
                            // Decide if error is fatal. For now, continue.
                        }
                    } else {
                        break; // No more commands from app logic, proceed to OS messages
                    }
                }

                // Then process OS messages
                let result = GetMessageW(&mut msg, None, 0, 0);
                match result.0 {
                    n if n > 0 => {
                        // Regular message
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    0 => {
                        // WM_QUIT
                        log::debug!(
                            "Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop."
                        );
                        break;
                    }
                    _ => {
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
                                    .is_ok_and(|g| g.is_empty());
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
        // Clear the ui state provider reference
        match self.internal_state.ui_state_provider.lock() {
            Ok(mut guard) => *guard = None,
            Err(e) => {
                log::error!(
                    "Platform: Failed to lock ui_state_provider to clear it (poisoned): {:?}",
                    e
                );
            }
        }
        log::debug!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::platform_layer::controls::treeview_handler::TreeViewInternalState;
    use crate::platform_layer::types::TreeItemId;
    use crate::platform_layer::window_common::NativeWindowData;
    use windows::Win32::{Foundation::HWND, UI::Controls::HTREEITEM};

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

    // Arrange common state for Win32ApiInternalState tests
    fn setup_state() -> (Arc<Win32ApiInternalState>, WindowId, NativeWindowData) {
        let state = Win32ApiInternalState::new("TestState".to_string()).unwrap();
        let window_id = state.generate_unique_window_id();
        let native = NativeWindowData::new(window_id);
        (state, window_id, native)
    }

    #[test]
    fn generate_unique_window_id_produces_unique_values() {
        // Arrange
        let state = Win32ApiInternalState::new("UIDTest".to_string()).unwrap();
        // Act
        let id1 = state.generate_unique_window_id();
        let id2 = state.generate_unique_window_id();
        // Assert
        assert_ne!(id1, id2);
    }

    #[test]
    fn remove_window_data_removes_entry() {
        // Arrange
        let (state, window_id, data) = setup_state();
        {
            let mut guard = state.active_windows().write().unwrap();
            guard.insert(window_id, data);
        }
        // Act
        state.remove_window_data(window_id);
        // Assert
        let guard = state.active_windows().read().unwrap();
        assert!(!guard.contains_key(&window_id));
    }

    #[test]
    fn with_treeview_state_mut_preserves_state_on_success() {
        // Arrange
        let (state, window_id, mut data) = setup_state();
        data.register_control_hwnd(1, HWND(1 as *mut std::ffi::c_void));
        data.init_treeview_state();
        {
            let mut guard = state.active_windows().write().unwrap();
            guard.insert(window_id, data);
        }
        // Act
        let result = state.with_treeview_state_mut(window_id, 1, |_hwnd, tv_state| {
            tv_state
                .item_id_to_htreeitem
                .insert(TreeItemId(7), HTREEITEM(7));
            Ok(())
        });
        // Assert
        assert!(result.is_ok());
        let guard = state.active_windows().read().unwrap();
        let window = guard.get(&window_id).unwrap();
        let tv_state = window.get_treeview_state().expect("treeview state");
        assert!(tv_state.item_id_to_htreeitem.contains_key(&TreeItemId(7)));
    }

    #[test]
    fn with_treeview_state_mut_preserves_state_on_error() {
        // Arrange
        let (state, window_id, mut data) = setup_state();
        data.register_control_hwnd(1, HWND(1 as *mut std::ffi::c_void));
        data.init_treeview_state();
        {
            let mut guard = state.active_windows().write().unwrap();
            guard.insert(window_id, data);
        }
        // Act
        let result = state.with_treeview_state_mut(window_id, 1, |_hwnd, tv_state| {
            tv_state
                .item_id_to_htreeitem
                .insert(TreeItemId(9), HTREEITEM(9));
            Err(PlatformError::OperationFailed("fail".into()))
        });
        // Assert
        assert!(result.is_err());
        let guard = state.active_windows().read().unwrap();
        let window = guard.get(&window_id).unwrap();
        let tv_state = window.get_treeview_state().expect("treeview state");
        assert!(tv_state.item_id_to_htreeitem.contains_key(&TreeItemId(9)));
    }

    #[test]
    fn should_quit_on_last_window_close_false_when_windows_exist() {
        // Arrange
        let (state, window_id, data) = setup_state();
        {
            let mut guard = state.active_windows().write().unwrap();
            guard.insert(window_id, data);
        }
        // Act
        let result = state.should_quit_on_last_window_close();
        // Assert
        assert!(!result);
    }

    #[test]
    fn should_quit_on_last_window_close_true_when_no_windows() {
        // Arrange
        let (state, _, _) = setup_state();
        // Act
        let result = state.should_quit_on_last_window_close();
        // Assert
        assert!(result);
    }
}
