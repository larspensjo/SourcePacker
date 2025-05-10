use super::control_treeview;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, PlatformCommand, PlatformEventHandler, WindowConfig, WindowId};
use super::window_common; // For window creation and WndProc // For TreeView specific command handling

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Controls::{ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx},
            WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, MSG, PostQuitMessage, TranslateMessage,
            },
        },
    },
    core::PCWSTR,
};

use std::collections::HashMap;
use std::ffi::c_void;
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
    pub(crate) windows: RwLock<HashMap<WindowId, window_common::NativeWindowData>>,
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
                windows: RwLock::new(HashMap::new()),
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

    /// Decrements the active window count and posts WM_QUIT if it reaches zero
    /// and the application is in a quitting state.
    pub(crate) fn decrement_active_windows(&self) {
        let prev_count = self.active_windows_count.fetch_sub(1, Ordering::Relaxed);
        if prev_count == 1 && self.is_quitting.load(Ordering::Relaxed) == 1 {
            // Last window closed and we are trying to quit
            println!("Platform: Last active window closed, posting WM_QUIT.");
            unsafe { PostQuitMessage(0) };
        }
    }

    /// Marks the application as attempting to quit.
    /// If no windows are active, posts WM_QUIT immediately.
    pub(crate) fn signal_quit_intent(&self) {
        self.is_quitting.store(1, Ordering::Relaxed);
        if self.active_windows_count.load(Ordering::Relaxed) == 0 {
            println!("Platform: Quit signaled with no active windows, posting WM_QUIT.");
            unsafe { PostQuitMessage(0) };
        }
    }

    /// Internally processes a list of platform commands.
    /// This is called from the WndProc after the app logic's event handler returns commands.
    /// It avoids re-acquiring locks unnecessarily if called from the UI thread.
    pub fn process_commands_from_event_handler(
        self: &Arc<Self>, // Takes Arc<Self> just like handle_window_message
        commands: Vec<PlatformCommand>,
    ) {
        for cmd in commands {
            // We need to match on the command and call the appropriate
            // existing helper functions from window_common or control_treeview.
            // These helpers already take `&Arc<Win32ApiInternalState>`.
            let result = match cmd {
                PlatformCommand::SetWindowTitle { window_id, title } => {
                    window_common::set_window_title(self, window_id, &title)
                }
                PlatformCommand::ShowWindow { window_id } => {
                    window_common::show_window(self, window_id, true)
                }
                PlatformCommand::CloseWindow { window_id } => {
                    // This command from AppLogic means "yes, actually destroy the window now"
                    // It's different from send_close_message which just posts WM_CLOSE.
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
                ), // Add other commands here as they are implemented
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
        // Uninitialize COM when the platform state is dropped.
        // This should happen when the PlatformInterface is dropped, effectively at app exit.
        println!("Platform: Win32ApiInternalState dropped, calling CoUninitialize.");
        unsafe { CoUninitialize() };
    }
}

/// The primary interface to the platform abstraction layer.
///
/// It allows the application logic to create and manage native UI elements
/// (like windows and controls) and to run the application's main event loop.
/// This struct encapsulates the platform-specific state and operations.
pub struct PlatformInterface {
    /// Holds the internal state of the Win32 platform layer, shared via an Arc.
    internal_state: Arc<Win32ApiInternalState>,
}

impl PlatformInterface {
    /// Creates a new instance of the platform interface.
    ///
    /// This initializes necessary platform components (like COM and Common Controls)
    /// and prepares the layer for creating UI elements. It should typically be
    /// called once at application startup.
    /// The `app_name_for_class` is used to create a unique window class name.
    pub fn new(app_name_for_class: String) -> PlatformResult<Self> {
        let internal_state = Win32ApiInternalState::new(app_name_for_class)?;

        // Register the window class. This should ideally happen only once.
        // We pass the Arc'd internal_state to window_common::register_window_class
        // so it can set it as GWLP_USERDATA for the class if needed, or so that
        // create_native_window can pass it via lpCreateParams.
        window_common::register_window_class(&internal_state)?;
        println!("Platform: Window class registration attempted during PlatformInterface::new().");

        Ok(PlatformInterface { internal_state })
    }

    /// Creates a new native window based on the provided configuration.
    ///
    /// The application logic specifies desired properties like title and size.
    /// The platform layer handles the actual Win32 window creation and returns
    /// a `WindowId` that the application logic can use to refer to this window.
    pub fn create_window(&self, config: WindowConfig) -> PlatformResult<WindowId> {
        let window_id = self.internal_state.generate_window_id();
        let hwnd = window_common::create_native_window(
            &self.internal_state, // Pass Arc<Win32ApiInternalState>
            window_id,
            &config.title,
            config.width,
            config.height,
        )?;

        // Store native window data
        let native_data = window_common::NativeWindowData {
            hwnd,
            id: window_id,
            treeview_state: None, // Initially no treeview state
        };
        self.internal_state
            .windows
            .write()
            .unwrap()
            .insert(window_id, native_data);
        self.internal_state
            .active_windows_count
            .fetch_add(1, Ordering::Relaxed);

        Ok(window_id)
    }

    /// Executes a platform-agnostic command sent by the application logic.
    ///
    /// This method translates `PlatformCommand`s into specific native UI operations.
    /// For example, it can change a window's title, populate a tree view, or show a window.
    pub fn execute_command(&self, command: PlatformCommand) -> PlatformResult<()> {
        match command {
            PlatformCommand::SetWindowTitle { window_id, title } => {
                window_common::set_window_title(&self.internal_state, window_id, &title)
            }
            PlatformCommand::ShowWindow { window_id } => {
                window_common::show_window(&self.internal_state, window_id, true)
            }
            PlatformCommand::CloseWindow { window_id } => {
                // This sends WM_CLOSE. The app logic will then get WindowCloseRequested.
                // If app logic confirms, it should issue another CloseWindow or similar,
                // or platform directly proceeds with DestroyWindow in WndProc on WM_CLOSE.
                // For now, WM_CLOSE directly leads to DestroyWindow in our current WndProc.
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
        }
    }

    /// Runs the main application event loop.
    ///
    /// This method takes control and processes native OS messages. It continuously
    /// dispatches events to the provided `event_handler` (from the application logic)
    /// until the application quits. This function will block the current thread.
    pub fn run(&self, event_handler: Arc<Mutex<dyn PlatformEventHandler>>) -> PlatformResult<()> {
        // Store a weak reference to the event handler in the internal state
        // so the WndProc can access it without creating a reference cycle.
        *self.internal_state.event_handler.lock().unwrap() = Some(Arc::downgrade(&event_handler));

        // Main message loop
        // This is taken from your original ui_facade/app.rs
        unsafe {
            let mut msg = MSG::default();
            // HWND::default() (None for GetMessageW) retrieves messages for any window
            // belonging to the current thread.
            loop {
                let result: windows::core::BOOL = GetMessageW(&mut msg, None, 0, 0);

                if result.0 > 0 {
                    // Message retrieved (not WM_QUIT)
                    // result.as_bool() would also work here if result.0 is guaranteed non-negative on success
                    let _ = TranslateMessage(&msg); // This is unsafe
                    DispatchMessageW(&msg); // This is unsafe
                } else if result.0 == 0 {
                    // WM_QUIT received
                    println!("Platform: GetMessageW returned 0 (WM_QUIT), exiting message loop.");
                    break;
                } else {
                    // result.0 == -1, an error occurred
                    let last_error = GetLastError();
                    eprintln!(
                        "Platform: GetMessageW failed with return -1. LastError: {:?}",
                        last_error
                    );
                    // Convert the HRESULT from GetLastError into a windows::core::Error
                    let win_error = windows::core::Error::from_win32();
                    return Err(PlatformError::OperationFailed(format!(
                        "GetMessageW failed: {}",
                        win_error // This is windows::core::Error
                    )));
                }
            }
        }

        // Application is quitting, call on_quit on the event handler.
        if let Ok(mut handler_guard) = event_handler.lock() {
            handler_guard.on_quit();
        }

        // Clear the event handler reference from internal state
        *self.internal_state.event_handler.lock().unwrap() = None;
        println!("Platform: Message loop exited cleanly.");
        Ok(())
    }
}
