use super::app::App;
use super::error::{Result as UiResult, UiError};
use crate::core::{FileNode, FileState};
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
            TVGN_CHILD,
            TVGN_NEXT,
            TVIF_CHILDREN,
            TVIF_PARAM,
            TVIF_STATE,
            TVIF_TEXT,
            TVINSERTSTRUCTW,
            TVINSERTSTRUCTW_0,
            TVIS_STATEIMAGEMASK,
            TVITEMEXW,
            TVITEMEXW_CHILDREN,
            TVM_DELETEITEM,
            TVM_GETITEMW,
            TVM_GETNEXTITEM,
            TVM_INSERTITEMW,
            TVM_SETITEMW,
            TVN_ITEMCHANGEDW,
            TVS_CHECKBOXES,
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
    file_nodes: Vec<FileNode>,
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

    pub fn populate_treeview_with_data(&self, nodes_data: Vec<FileNode>) {
        let state_ptr = self.get_state_ptr();
        if state_ptr.is_null() {
            eprintln!("Window state is null, cannot populate treeview");
            return;
        }
        let window_state = unsafe { &mut *state_ptr };

        window_state.file_nodes = nodes_data; // Move the data into WindowState

        if let Some(hwnd_tv) = window_state.hwnd_treeview {
            unsafe {
                // Clear all existing items. TVI_ROOT is HTREEITEM(0)
                let _ = SendMessageW(
                    hwnd_tv,
                    TVM_DELETEITEM,
                    Some(WPARAM(0)),
                    Some(LPARAM(HTREEITEM(0).0)),
                );
                // Add new items from the stored file_nodes
                // Iterate over window_state.file_nodes which now owns the data
                for node_ref in &window_state.file_nodes {
                    // HTREEITEM(0) for h_parent means add to root (TVI_ROOT)
                    self.add_treeview_item(hwnd_tv, HTREEITEM(0), node_ref);
                }
            }
        } else {
            eprintln!("TreeView HWND not found in window state for population.");
        }
    }

    unsafe fn add_treeview_item(&self, hwnd_tv: HWND, h_parent: HTREEITEM, file_node: &FileNode) {
        let mut text_buffer: Vec<u16> = file_node.name.encode_utf16().collect();
        text_buffer.push(0); // Null-terminate

        let image_index = match file_node.state {
            FileState::Selected => 2, // Checked state image index
            _ => 1,                   // Unchecked state image index (for Deselected or Unknown)
        };

        let mut tv_item = TVITEMEXW {
            // Use TVITEMEXW for lParam if not already
            mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN | TVIF_STATE, // Add TVIF_STATE
            hItem: HTREEITEM::default(), // System fills this on insert, or use for TVM_SETITEM
            pszText: PWSTR(text_buffer.as_mut_ptr()),
            cchTextMax: text_buffer.len() as i32,
            lParam: LPARAM(file_node as *const FileNode as isize), // Pointer to node in WindowState.file_nodes
            cChildren: TVITEMEXW_CHILDREN(if file_node.is_dir { 1 } else { 0 }),
            state: (image_index as u32) << 12, // INDEXTOSTATEIMAGEMASK(image_index)
            stateMask: TVIS_STATEIMAGEMASK.0,  // Mask to indicate state image is being set/read
            ..Default::default()
        };

        let tv_insert_struct = TVINSERTSTRUCTW {
            hParent: h_parent,
            hInsertAfter: HTREEITEM(1),                       // TVI_LAST
            Anonymous: TVINSERTSTRUCTW_0 { itemex: tv_item }, // Use itemex if TVITEMEXW
        };

        let h_item_lresult = SendMessageW(
            hwnd_tv,
            TVM_INSERTITEMW,
            Some(WPARAM(0)), // flags, not used here
            Some(LPARAM(&tv_insert_struct as *const _ as isize)),
        );
        let h_current_item = HTREEITEM(h_item_lresult.0);

        if h_current_item.0 == 0 {
            eprintln!("Failed to insert TreeView item: {}", file_node.name);
            return;
        }

        if file_node.is_dir && !file_node.children.is_empty() {
            for child_node in &file_node.children {
                self.add_treeview_item(hwnd_tv, h_current_item, child_node);
            }
        }
    }

    // Updates the checkbox for a given h_item based on its node's state,
    // and recursively updates children's visual states.
    // The 'node' parameter must point to the up-to-date model state.
    pub fn update_treeview_item_visual_state(
        &self,
        hwnd_tv: HWND,
        h_item: HTREEITEM,
        node: &FileNode,
    ) {
        let image_index = match node.state {
            FileState::Selected => 2,
            _ => 1, // Deselected or Unknown
        };

        let mut tv_item_update = TVITEMEXW {
            mask: TVIF_STATE,
            hItem: h_item,
            state: (image_index as u32) << 12, // INDEXTOSTATEIMAGEMASK
            stateMask: TVIS_STATEIMAGEMASK.0,
            ..Default::default()
        };

        unsafe {
            let _ = SendMessageW(
                hwnd_tv,
                TVM_SETITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tv_item_update as *mut _ as isize)),
            );
        }

        // If it's a directory, recurse for children
        if node.is_dir {
            unsafe {
                // Get the first child item in the TreeView
                let mut h_child_tv_item = HTREEITEM(
                    SendMessageW(
                        hwnd_tv,
                        TVM_GETNEXTITEM,
                        Some(WPARAM(TVGN_CHILD as _)),
                        Some(LPARAM(h_item.0)),
                    )
                    .0,
                );

                while h_child_tv_item.0 != 0 {
                    // Get the FileNode associated with this child TreeView item
                    let mut child_item_data = TVITEMEXW {
                        mask: TVIF_PARAM,
                        hItem: h_child_tv_item,
                        ..Default::default()
                    };
                    if SendMessageW(
                        hwnd_tv,
                        TVM_GETITEMW,
                        Some(WPARAM(0)),
                        Some(LPARAM(&mut child_item_data as *mut _ as isize)),
                    )
                    .0 == 0
                    {
                        break; // Error fetching child item data
                    }

                    let child_node_ptr = child_item_data.lParam.0 as *const FileNode;
                    if !child_node_ptr.is_null() {
                        let child_node_ref = &*child_node_ptr;
                        // Recursively call to update this child's visual state
                        self.update_treeview_item_visual_state(
                            hwnd_tv,
                            h_child_tv_item,
                            child_node_ref,
                        );
                    }

                    // Get the next sibling item in the TreeView
                    h_child_tv_item = HTREEITEM(
                        SendMessageW(
                            hwnd_tv,
                            TVM_GETNEXTITEM,
                            Some(WPARAM(TVGN_NEXT as _)),
                            Some(LPARAM(h_child_tv_item.0)),
                        )
                        .0,
                    );
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
            file_nodes: Vec::new(),
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

                    let tvs_styles_u32 = TVS_HASLINES
                        | TVS_LINESATROOT
                        | TVS_HASBUTTONS
                        | TVS_SHOWSELALWAYS
                        | TVS_CHECKBOXES;
                    // WS_xyz are also structs WINDOW_STYLE(u32)
                    let combined_style_u32 =
                        WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(tvs_styles_u32);

                    let hwnd_tv_result = CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        WC_TREEVIEWW,
                        PCWSTR::null(),
                        combined_style_u32,
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

        WM_NOTIFY => {
            let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) };
            let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };

            if state_ptr.is_null() {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            let window_state = unsafe { &mut *state_ptr }; // Now we can use window_state

            if let Some(hwnd_tv) = window_state.hwnd_treeview {
                if nmhdr.hwndFrom == hwnd_tv && nmhdr.code == TVN_ITEMCHANGEDW {
                    let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };

                    // Check if the state change involves the state image (checkbox)
                    // itemNew.mask tells what changed. itemNew.stateMask tells what part of .state is valid.
                    if (nmtv.itemNew.mask.0 & TVIF_STATE.0) != 0
                        && (nmtv.itemNew.stateMask.0 & TVIS_STATEIMAGEMASK.0) != 0
                    {
                        // The state image bits might have changed.
                        // Extract old and new state image indices (1 for unchecked, 2 for checked).
                        let old_state_idx = (nmtv.itemOld.state.0 & TVIS_STATEIMAGEMASK.0) >> 12;
                        let new_state_idx = (nmtv.itemNew.state.0 & TVIS_STATEIMAGEMASK.0) >> 12;

                        if old_state_idx != new_state_idx {
                            // A genuine toggle of the checkbox state
                            let h_item = nmtv.itemNew.hItem;
                            // lParam in itemNew points to our FileNode.
                            let current_item_ptr = nmtv.itemNew.lParam.0 as *mut FileNode; // From TVITEMEXW.lParam

                            if !current_item_ptr.is_null() {
                                let current_node = unsafe { &mut *current_item_ptr }; // Mutable access to the node in WindowState.file_nodes

                                let new_file_state = if new_state_idx == 2 {
                                    // Checked
                                    FileState::Selected
                                } else {
                                    // Unchecked
                                    FileState::Deselected
                                };

                                // Update internal model state.
                                // If it's a folder, propagate to children's model state.
                                if current_node.is_dir {
                                    crate::core::state_manager::update_folder_selection(
                                        current_node,
                                        new_file_state,
                                    );
                                } else {
                                    current_node.state = new_file_state;
                                }

                                // After model update, refresh the UI for the clicked item and its children
                                // This function call handles the recursive UI update.
                                let main_window_ref = Window { hwnd }; // Create a temporary Window wrapper
                                main_window_ref.update_treeview_item_visual_state(
                                    hwnd_tv,
                                    h_item,
                                    current_node,
                                );

                                // If you had a status bar or other UI elements to update based on selection, do it here.
                                // E.g., count selected files from window_state.file_nodes and update status bar.
                            }
                        }
                    }
                    return LRESULT(0); // Indicate message was processed
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
    }
}
