use super::app::Win32ApiInternalState;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, TreeItemDescriptor, TreeItemId, WindowId};
use super::window_common::{BUTTON_AREA_HEIGHT, NativeWindowData};

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::Controls::{
            HTREEITEM, NMHDR, NMTREEVIEWW, TVGN_CHILD, TVGN_NEXT, TVI_LAST, TVIF_CHILDREN,
            TVIF_PARAM, TVIF_STATE, TVIF_TEXT, TVINSERTSTRUCTW, TVINSERTSTRUCTW_0,
            TVIS_STATEIMAGEMASK, TVITEMEXW, TVITEMEXW_CHILDREN, TVM_DELETEITEM, TVM_GETITEMW,
            TVM_GETNEXTITEM, TVM_INSERTITEMW, TVM_SETITEMW, TVN_ITEMCHANGEDW, TVS_CHECKBOXES,
            TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS, WC_TREEVIEWW,
        },
        UI::WindowsAndMessaging::*, // For CreateWindowExW, GetDlgItem, SendMessageW etc.
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

// Define a constant for the TreeView control ID
pub(crate) const ID_TREEVIEW_CTRL: i32 = 1001;

/// Holds the native state specific to a TreeView control within a window.
/// This includes its handle (`HWND`) and mappings between application-level
/// `TreeItemId`s and native `HTREEITEM`s.
#[derive(Debug)]
pub(crate) struct TreeViewInternalState {
    pub(crate) hwnd: HWND,
    /// Maps application-provided `TreeItemId` to the native `HTREEITEM`.
    item_id_to_htreeitem: HashMap<TreeItemId, HTREEITEM>,
    /// Maps native `HTREEITEM` back to the application-provided `TreeItemId`.
    /// Used for processing notifications.
    htreeitem_to_item_id: HashMap<isize, TreeItemId>,
}

impl TreeViewInternalState {
    fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            item_id_to_htreeitem: HashMap::new(),
            htreeitem_to_item_id: HashMap::new(),
        }
    }

    fn clear_items(&mut self) {
        unsafe {
            // TVI_ROOT is HTREEITEM(0)
            SendMessageW(
                self.hwnd,
                TVM_DELETEITEM,
                Some(WPARAM(0)),
                Some(LPARAM(HTREEITEM(0).0)),
            );
        }
        self.item_id_to_htreeitem.clear();
        self.htreeitem_to_item_id.clear();
    }
}

/// Ensures a TreeView control exists for the given window, creating it if necessary.
/// Returns a mutable reference to the `TreeViewInternalState`.
fn ensure_treeview_exists_and_get_state<'a>(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    window_data: &'a mut NativeWindowData, // Mutable access to parent window's data
) -> PlatformResult<&'a mut TreeViewInternalState> {
    if window_data.treeview_state.is_none() {
        // TreeView doesn't exist yet, create it.
        let mut client_rect = RECT::default();
        unsafe { GetClientRect(window_data.hwnd, &mut client_rect)? };

        let tvs_styles =
            TVS_HASLINES | TVS_LINESATROOT | TVS_HASBUTTONS | TVS_SHOWSELALWAYS | TVS_CHECKBOXES;
        let combined_style = WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(tvs_styles);

        let tv_width = client_rect.right - client_rect.left;
        let tv_height = client_rect.bottom - client_rect.top - BUTTON_AREA_HEIGHT;

        let hwnd_tv = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),     // dwExStyle
                WC_TREEVIEWW,           // lpClassName
                PCWSTR::null(),         // lpWindowName (no title for a control)
                combined_style,         // dwStyle
                0,                      // X
                0,                      // Y
                tv_width,               // nWidth
                tv_height,              // nHeight (adjusted for button area)
                Some(window_data.hwnd), // hWndParent
                Some(HMENU(ID_TREEVIEW_CTRL as *mut c_void)),
                Some(internal_state.h_instance), // hInstance
                None,                            // lpParam
            )?
        };
        println!("Platform: TreeView created with HWND {:?}", hwnd_tv);
        window_data.treeview_state = Some(TreeViewInternalState::new(hwnd_tv));
    }
    // This unwrap is safe because we just created it if it was None.
    Ok(window_data.treeview_state.as_mut().unwrap())
}

/// Populates the TreeView control in the specified window with new items.
/// Any existing items are cleared first.
pub(crate) fn populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    items: Vec<TreeItemDescriptor>,
) -> PlatformResult<()> {
    let mut windows_guard = internal_state.windows.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to acquire write lock for windows map".into())
    })?;

    if let Some(window_data) = windows_guard.get_mut(&window_id) {
        let tv_state =
            ensure_treeview_exists_and_get_state(internal_state, window_id, window_data)?;
        tv_state.clear_items();

        for item_desc in items {
            add_treeview_item_recursive(tv_state, HTREEITEM(0), &item_desc)?;
        }
        Ok(())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for PopulateTreeView",
            window_id
        )))
    }
}

/// Recursively adds a `TreeItemDescriptor` and its children to the TreeView.
fn add_treeview_item_recursive(
    tv_state: &mut TreeViewInternalState,
    h_parent_native: HTREEITEM,
    item_desc: &TreeItemDescriptor,
) -> PlatformResult<()> {
    let mut text_buffer: Vec<u16> = item_desc.text.encode_utf16().collect();
    text_buffer.push(0); // Null-terminate

    let image_index = match item_desc.state {
        CheckState::Checked => 2, // Checked state image index (1-based for TVS_CHECKBOXES)
        CheckState::Unchecked => 1, // Unchecked state image index
    };

    let mut tv_item = TVITEMEXW {
        mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN | TVIF_STATE,
        hItem: HTREEITEM::default(), // Will be filled by TVM_INSERTITEMW
        pszText: PWSTR(text_buffer.as_mut_ptr()),
        cchTextMax: text_buffer.len() as i32,
        lParam: LPARAM(item_desc.id.0 as isize), // Store app-specific TreeItemId.0 here
        cChildren: TVITEMEXW_CHILDREN(if item_desc.is_folder { 1 } else { 0 }), // Has children affordance
        state: (image_index as u32) << 12, // INDEXTOSTATEIMAGEMASK(image_index)
        stateMask: TVIS_STATEIMAGEMASK.0,
        ..Default::default()
    };

    let tv_insert_struct = TVINSERTSTRUCTW {
        hParent: h_parent_native,
        hInsertAfter: TVI_LAST,
        Anonymous: TVINSERTSTRUCTW_0 { itemex: tv_item },
    };

    let h_current_item_native = HTREEITEM(
        unsafe {
            SendMessageW(
                tv_state.hwnd,
                TVM_INSERTITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&tv_insert_struct as *const _ as isize)),
            )
        }
        .0,
    );

    if h_current_item_native.0 == 0 {
        return Err(PlatformError::ControlCreationFailed(format!(
            "Failed to insert TreeView item '{}': {:?}",
            item_desc.text,
            unsafe { GetLastError() }
        )));
    }

    // Store mappings
    tv_state
        .item_id_to_htreeitem
        .insert(item_desc.id, h_current_item_native);
    tv_state
        .htreeitem_to_item_id
        .insert(h_current_item_native.0, item_desc.id);

    // Recursively add children
    if item_desc.is_folder && !item_desc.children.is_empty() {
        for child_desc in &item_desc.children {
            add_treeview_item_recursive(tv_state, h_current_item_native, child_desc)?;
        }
    }
    Ok(())
}

/// Updates the visual state (checkbox) of a single TreeView item.
pub(crate) fn update_treeview_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    item_id: TreeItemId,
    new_check_state: CheckState,
) -> PlatformResult<()> {
    let windows_guard = internal_state.windows.read().map_err(|_| {
        PlatformError::OperationFailed("Failed to acquire read lock for windows map".into())
    })?;

    if let Some(window_data) = windows_guard.get(&window_id) {
        if let Some(ref tv_state) = window_data.treeview_state {
            if let Some(h_item_native) = tv_state.item_id_to_htreeitem.get(&item_id) {
                let image_index = match new_check_state {
                    CheckState::Checked => 2,
                    CheckState::Unchecked => 1,
                };

                let mut tv_item_update = TVITEMEXW {
                    mask: TVIF_STATE,
                    hItem: *h_item_native,
                    state: (image_index as u32) << 12, // INDEXTOSTATEIMAGEMASK
                    stateMask: TVIS_STATEIMAGEMASK.0,
                    ..Default::default()
                };

                let send_result = unsafe {
                    SendMessageW(
                        tv_state.hwnd,
                        TVM_SETITEMW,
                        Some(WPARAM(0)),
                        Some(LPARAM(&mut tv_item_update as *mut _ as isize)),
                    )
                };
                if send_result.0 == 0 {
                    // TVM_SETITEM returns 0 on failure
                    return Err(PlatformError::OperationFailed(format!(
                        "TVM_SETITEMW failed for item {:?}: {:?}",
                        item_id,
                        unsafe { GetLastError() }
                    )));
                }
                Ok(())
            } else {
                Err(PlatformError::InvalidHandle(format!(
                    "TreeItemId {:?} not found in window {:?} for UpdateVisualState",
                    item_id, window_id
                )))
            }
        } else {
            Err(PlatformError::OperationFailed(format!(
                "No TreeView exists in window {:?} for UpdateVisualState",
                window_id
            )))
        }
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "WindowId {:?} not found for UpdateVisualState",
            window_id
        )))
    }
}

/// Handles `WM_NOTIFY` messages specifically for TreeView controls.
/// If the notification is relevant (e.g., item check state changed),
/// it translates it into an `AppEvent` and returns it.
pub(crate) fn handle_treeview_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam: LPARAM, // LPARAM of WM_NOTIFY, which is LPNMHDR
) -> Option<AppEvent> {
    let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) };

    if nmhdr.idFrom as i32 != ID_TREEVIEW_CTRL {
        return None;
    }

    let windows_guard = internal_state.windows.read().ok()?;
    let window_data = windows_guard.get(&window_id)?;
    let tv_state = window_data.treeview_state.as_ref()?;

    if nmhdr.hwndFrom != tv_state.hwnd {
        return None;
    }

    if nmhdr.code == TVN_ITEMCHANGEDW {
        let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };

        if (nmtv.itemNew.mask.0 & TVIF_STATE.0) != 0
            && (nmtv.itemNew.stateMask.0 & TVIS_STATEIMAGEMASK.0) != 0
        {
            let old_state_idx = (nmtv.itemOld.state.0 & TVIS_STATEIMAGEMASK.0) >> 12;
            let new_state_idx = (nmtv.itemNew.state.0 & TVIS_STATEIMAGEMASK.0) >> 12;

            if old_state_idx != new_state_idx {
                let item_app_id_val = nmtv.itemNew.lParam.0 as u64;
                let toggled_item_id = TreeItemId(item_app_id_val);

                if !tv_state.item_id_to_htreeitem.contains_key(&toggled_item_id) {
                    eprintln!(
                        "Platform TreeView: Received toggle for an unknown TreeItemId {:?} from lParam.",
                        toggled_item_id
                    );
                    return None;
                }

                let new_check_state = if new_state_idx == 2 {
                    CheckState::Checked
                } else {
                    CheckState::Unchecked
                };
                println!(
                    "Platform TreeView: Item {:?} toggled to {:?}",
                    toggled_item_id, new_check_state
                );
                return Some(AppEvent::TreeViewItemToggled {
                    window_id,
                    item_id: toggled_item_id,
                    new_state: new_check_state,
                });
            }
        }
    }
    None
}
