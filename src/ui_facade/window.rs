// The core of the window abstraction: builder, window struct, and the facade's WndProc

use super::app::App;
use super::error::{Result, UiError};
use std::ffi::c_void;
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT, UpdateWindow},
        Graphics::Gdi::{COLOR_WINDOW, HBRUSH},
        UI::WindowsAndMessaging::GWLP_USERDATA,
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR, w},
};

// This struct will hold the data associated with each window instance,
// including callbacks. A pointer to this will be stored in GWLP_USERDATA.
struct WindowState {
    on_destroy: Option<Box<dyn Fn()>>,
    // Future: on_paint, on_command, etc.
}

// The public Window struct, primarily holding the HWND.
#[derive(Clone, Copy)] // HWND is Copy
pub struct Window {
    pub hwnd: HWND,
}

impl Window {
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWDEFAULT);
            let _ = UpdateWindow(self.hwnd); // Ensure WM_PAINT is sent if needed
        }
    }
    // Add more methods like set_title, add_control etc. later
}

pub struct WindowBuilder {
    app: App, // To get HINSTANCE
    title: String,
    width: i32,
    height: i32,
    on_destroy_callback: Option<Box<dyn Fn()>>,
}

impl WindowBuilder {
    pub fn new(app: App) -> Self {
        WindowBuilder {
            app,
            title: "SourcePacker".to_string(),
            width: CW_USEDEFAULT, // Use CW_USEDEFAULT for default size/pos
            height: CW_USEDEFAULT,
            on_destroy_callback: None,
        }
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn on_destroy<F: Fn() + 'static>(mut self, callback: F) -> Self {
        self.on_destroy_callback = Some(Box::new(callback));
        self
    }

    pub fn build(self) -> Result<Window> {
        let window_class_name = w!("SourcePackerFacadeWindowClass");

        // Prepare the state to be associated with the window.
        // This Box will be converted to a raw pointer and passed to CreateWindowExW.
        // It will be reclaimed in WM_NCDESTROY.
        let window_state_boxed = Box::new(WindowState {
            on_destroy: self.on_destroy_callback,
        });
        let window_state_ptr = Box::into_raw(window_state_boxed) as *mut c_void;

        unsafe {
            // Register the window class if it hasn't been registered yet.
            // A more robust way would be to use a static OnceCell for class registration.
            let mut wc_test = WNDCLASSEXW::default();
            if GetClassInfoExW(Some(self.app.instance), window_class_name, &mut wc_test).is_err() {
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                    lpfnWndProc: Some(facade_wnd_proc), // Use the facade's WndProc
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: self.app.instance,
                    hIcon: LoadIconW(None, IDI_APPLICATION)?,
                    hCursor: LoadCursorW(None, IDC_ARROW)?,
                    // HBRUSH is an isize internally. COLOR_WINDOW.0 is u32.
                    hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
                    lpszMenuName: PCWSTR::null(),
                    lpszClassName: window_class_name,
                    hIconSm: LoadIconW(None, IDI_APPLICATION)?,
                };
                if RegisterClassExW(&wc) == 0 {
                    // If registration fails, reclaim the Box immediately.
                    let _ = Box::from_raw(window_state_ptr as *mut WindowState);
                    return Err(UiError::ClassRegistrationFailed);
                }
            }

            // Create the window.
            // Pass the window_state_ptr as the lpParam (last argument).
            // This will be retrieved in WM_NCCREATE.
            let hwnd_result = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                window_class_name,
                &HSTRING::from(self.title), // HSTRING is often preferred for new APIs
                WS_OVERLAPPEDWINDOW,        // Note: WS_VISIBLE is removed; call window.show()
                self.width,
                self.height,
                self.width, // Re-using for nWidth, nHeight; CW_USEDEFAULT for x,y
                self.height,
                None, // Parent window
                None, // Menu
                Some(self.app.instance),
                Some(window_state_ptr), // Pass our boxed state pointer
            );

            match hwnd_result {
                Ok(hwnd_handle) => {
                    if hwnd_handle.0 as usize == 0usize {
                        // This case should ideally be covered by Err from CreateWindowExW
                        let _ = Box::from_raw(window_state_ptr as *mut WindowState); // Reclaim
                        Err(UiError::WindowCreationFailed)
                    } else {
                        Ok(Window { hwnd: hwnd_handle })
                    }
                }
                Err(e) => {
                    let _ = Box::from_raw(window_state_ptr as *mut WindowState); // Reclaim
                    Err(UiError::from(e))
                }
            }
        }
    }
}

// The facade's main window procedure.
// It dispatches messages to the appropriate WindowState callbacks.
extern "system" fn facade_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Retrieve the WindowState pointer stored in GWLP_USERDATA.
        // This is set during WM_NCCREATE.
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;

        match msg {
            WM_NCCREATE => {
                // This is the first message. lpCreateParams has the pointer we passed.
                let create_struct = lparam.0 as *const CREATESTRUCTW;
                let window_state_ptr_from_param =
                    (*create_struct).lpCreateParams as *mut WindowState;
                // Store this pointer with the window instance.
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_state_ptr_from_param as isize);
                // Fall through to DefWindowProcW for default WM_NCCREATE processing.
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NCDESTROY => {
                // This is the (almost) last message. Time to clean up our WindowState.
                if !state_ptr.is_null() {
                    // Convert the raw pointer back into a Box and let it drop,
                    // deallocating the memory and running any Drop impl for WindowState.
                    let _boxed_state = Box::from_raw(state_ptr);
                    // Clear the pointer from GWLP_USERDATA to prevent dangling pointers.
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    println!("WindowState for HWND {:?} cleaned up.", hwnd);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => {
                // For other messages, if we have a state pointer, use it.
                if !state_ptr.is_null() {
                    let window_state = &*state_ptr; // Get a reference to the state

                    match msg {
                        WM_DESTROY => {
                            println!("WM_DESTROY for HWND {:?}", hwnd);
                            if let Some(ref on_destroy_cb) = window_state.on_destroy {
                                on_destroy_cb();
                            }
                            // Note: Don't PostQuitMessage here by default in the facade.
                            // The on_destroy callback is responsible if this is the main window.
                            return LRESULT(0); // Message handled
                        }
                        WM_PAINT => {
                            // Basic WM_PAINT handling, can be expanded with a callback
                            println!("WM_PAINT for HWND {:?}", hwnd);
                            let mut ps = PAINTSTRUCT::default();
                            let _hdc = BeginPaint(hwnd, &mut ps);
                            // FillRect(hdc, &ps.rcPaint, HBRUSH((COLOR_WINDOW.0 + 1) as isize)); // Example
                            let _ = EndPaint(hwnd, &ps);
                            return LRESULT(0);
                        }
                        // Handle other messages like WM_COMMAND, WM_NOTIFY, WM_SIZE etc.
                        // by calling callbacks stored in window_state.
                        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                    }
                } else {
                    // No state associated (e.g., messages before WM_NCCREATE fully processed,
                    // or after WM_NCDESTROY).
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
        }
    }
}
