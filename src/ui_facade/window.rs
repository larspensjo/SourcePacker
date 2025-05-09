use super::app::App;
use super::error::{Result as UiResult, UiError};
use crate::core::FileNode;
use std::ffi::c_void;

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::UpdateWindow,
        Graphics::Gdi::{BeginPaint, COLOR_WINDOW, EndPaint, HBRUSH, PAINTSTRUCT},
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        System::Ole, // Import the Ole module to qualify S_FALSE
        UI::Controls::{
            HTREEITEM,
            ICC_TREEVIEW_CLASSES,
            INITCOMMONCONTROLSEX,
            InitCommonControlsEx,
            NMHDR,
            NMTREEVIEWW, // Keep NMHDR, NMTREEVIEWW for later WM_NOTIFY
            TVIF_CHILDREN,
            TVIF_PARAM,
            TVIF_TEXT,
            TVINSERTSTRUCTW,
            TVINSERTSTRUCTW_0,
            TVITEMEXW,
            TVITEMEXW_CHILDREN,
            TVM_DELETEITEM,
            TVM_INSERTITEMW,
            TVS_HASBUTTONS,
            TVS_HASLINES,
            TVS_LINESATROOT,
            TVS_SHOWSELALWAYS,
            WC_TREEVIEWW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

const ID_TREEVIEW: isize = 1001;

struct WindowState {
    hwnd_treeview: Option<HWND>,
    on_destroy: Option<Box<dyn Fn()>>,
}

#[derive(Clone, Copy)]
pub struct Window {
    pub hwnd: HWND,
}

impl Window {
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = UpdateWindow(self.hwnd);
        }
    }

    fn get_state_ptr(&self) -> *mut WindowState {
        unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowState }
    }

    pub fn populate_treeview(&self, nodes: &[FileNode]) {
        let state_ptr = self.get_state_ptr();
        if state_ptr.is_null() {
            eprintln!("Window state is null, cannot populate treeview");
            return;
        }
        let window_state = unsafe { &*state_ptr };

        if let Some(hwnd_tv) = window_state.hwnd_treeview {
            unsafe {
                let _ = SendMessageW(
                    hwnd_tv,
                    TVM_DELETEITEM,
                    Some(WPARAM(0)),
                    Some(LPARAM(HTREEITEM(0).0)),
                );
                for node in nodes {
                    self.add_treeview_item(hwnd_tv, HTREEITEM::default(), node);
                }
            }
        } else {
            eprintln!("TreeView HWND not found in window state.");
        }
    }

    unsafe fn add_treeview_item(&self, hwnd_tv: HWND, h_parent: HTREEITEM, file_node: &FileNode) {
        let mut text_buffer: Vec<u16> = file_node.name.encode_utf16().collect();
        text_buffer.push(0);

        let tv_item = TVITEMEXW {
            mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN,
            hItem: HTREEITEM::default(),
            pszText: PWSTR(text_buffer.as_mut_ptr()),
            cchTextMax: text_buffer.len() as i32,
            lParam: LPARAM(file_node as *const FileNode as isize),
            cChildren: TVITEMEXW_CHILDREN(if file_node.is_dir { 1 } else { 0 }),
            ..Default::default()
        };

        let tv_insert_struct = TVINSERTSTRUCTW {
            hParent: h_parent,
            hInsertAfter: HTREEITEM(1), // TVI_LAST
            Anonymous: TVINSERTSTRUCTW_0 { itemex: tv_item },
        };

        let h_item_lresult = unsafe {
            SendMessageW(
                hwnd_tv,
                TVM_INSERTITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&tv_insert_struct as *const _ as isize)),
            )
        };
        let h_current_item = HTREEITEM(h_item_lresult.0);

        if h_current_item.0 == 0 {
            eprintln!("Failed to insert TreeView item: {}", file_node.name);
            return;
        }

        if file_node.is_dir && !file_node.children.is_empty() {
            for child_node in &file_node.children {
                unsafe {
                    self.add_treeview_item(hwnd_tv, h_current_item, child_node);
                }
            }
        }
    }
}

pub struct WindowBuilder {
    app: App,
    title: String,
    width: i32,
    height: i32,
    on_destroy_callback: Option<Box<dyn Fn()>>,
}

impl WindowBuilder {
    pub fn new(app: App) -> Self {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            // Compare HRESULT structs directly, or their .0 fields (i32 error codes)
            if hr.is_err()
                && hr != windows::Win32::Foundation::S_FALSE
                && hr != windows::Win32::Foundation::RPC_E_CHANGED_MODE
            {
                eprintln!("CoInitializeEx failed with HRESULT: {:#010X}", hr.0); // Access HRESULT's inner i32 value
            }

            let icex = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_TREEVIEW_CLASSES, // Pass the struct directly
            };
            if !InitCommonControlsEx(&icex).as_bool() {
                eprintln!(
                    "InitCommonControlsEx for TreeView failed! Error: {:?}",
                    GetLastError()
                );
            }
        }

        WindowBuilder {
            app,
            title: "SourcePacker".to_string(),
            width: 800,
            height: 600,
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

    pub fn build(self) -> UiResult<Window> {
        let window_class_name = w!("SourcePackerFacadeWindowClass");
        let window_state_boxed = Box::new(WindowState {
            hwnd_treeview: None,
            on_destroy: self.on_destroy_callback,
        });
        let window_state_ptr = Box::into_raw(window_state_boxed) as *mut c_void;

        unsafe {
            let mut wc_test = WNDCLASSEXW::default();
            if GetClassInfoExW(Some(self.app.instance), window_class_name, &mut wc_test).is_err() {
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                    lpfnWndProc: Some(facade_wnd_proc),
                    hInstance: self.app.instance,
                    hIcon: LoadIconW(None, IDI_APPLICATION)?,
                    hCursor: LoadCursorW(None, IDC_ARROW)?,
                    hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
                    lpszClassName: window_class_name,
                    ..Default::default()
                };
                if RegisterClassExW(&wc) == 0 {
                    let _ = Box::from_raw(window_state_ptr as *mut WindowState);
                    return Err(UiError::ClassRegistrationFailed);
                }
            }

            let main_hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                window_class_name,
                &HSTRING::from(self.title),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                self.width,
                self.height,
                None,
                None,
                Some(self.app.instance),
                Some(window_state_ptr),
            )?;
            Ok(Window { hwnd: main_hwnd })
        }
    }
}

extern "system" fn facade_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };

    match msg {
        WM_NCCREATE => {
            unsafe {
                let create_struct = lparam.0 as *const CREATESTRUCTW;
                let window_state_param_ptr = (*create_struct).lpCreateParams as *mut WindowState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_state_param_ptr as isize);
            }
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        WM_CREATE => {
            if !state_ptr.is_null() {
                let window_state = unsafe { &mut *state_ptr };
                unsafe {
                    let h_instance_isize = GetWindowLongPtrW(hwnd, GWLP_HINSTANCE);
                    let h_instance = HINSTANCE(h_instance_isize as *mut c_void);

                    let mut client_rect = RECT::default();
                    GetClientRect(hwnd, &mut client_rect);

                    let tvs_styles_u32 =
                        TVS_HASLINES | TVS_LINESATROOT | TVS_HASBUTTONS | TVS_SHOWSELALWAYS;
                    // WS_xyz are also structs WINDOW_STYLE(u32)
                    let combined_style_u32 = WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(tvs_styles_u32);
                    let final_style = combined_style_u32;

                    let hwnd_tv_result = CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        WC_TREEVIEWW,
                        PCWSTR::null(),
                        final_style, // dwStyle
                        0,
                        0,
                        client_rect.right,
                        client_rect.bottom,
                        Some(hwnd),
                        Some(HMENU(ID_TREEVIEW as *mut c_void)),
                        Some(h_instance),
                        None,
                    );

                    match hwnd_tv_result {
                        Ok(tv_handle) => {
                            if tv_handle.0.is_null() {
                                eprintln!(
                                    "Failed to create TreeView control (handle is null). Error: {:?}",
                                    GetLastError()
                                );
                            } else {
                                window_state.hwnd_treeview = Some(tv_handle);
                                println!("TreeView Created: {:?}", tv_handle);
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Failed to create TreeView control (CreateWindowExW error): {:?}",
                                e
                            );
                        }
                    }
                }
            }
            return LRESULT(0);
        }
        WM_SIZE => {
            if !state_ptr.is_null() {
                let window_state = unsafe { &*state_ptr };
                if let Some(hwnd_tv) = window_state.hwnd_treeview {
                    unsafe {
                        let width = (lparam.0 & 0x0000FFFF) as i32;
                        let height = ((lparam.0 >> 16) & 0x0000FFFF) as i32;
                        let _ = SetWindowPos(
                            hwnd_tv,
                            Some(HWND::default()),
                            0,
                            0,
                            width,
                            height,
                            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            if !state_ptr.is_null() {
                let window_state = unsafe { &*state_ptr };
                println!("WM_DESTROY for HWND {:?}", hwnd);
                if let Some(ref on_destroy_cb) = window_state.on_destroy {
                    on_destroy_cb();
                }
            }
            return LRESULT(0);
        }
        WM_NCDESTROY => {
            if !state_ptr.is_null() {
                let _boxed_state = unsafe { Box::from_raw(state_ptr) };
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                println!("WindowState for HWND {:?} cleaned up.", hwnd);
            }
            unsafe {
                CoUninitialize();
            }
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        _ => {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
    }
}
