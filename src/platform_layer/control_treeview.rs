/*
 * This module provides platform-specific (Win32) implementations for TreeView control
 * operations. It handles the creation (now delegated), population, and manipulation
 * of native TreeView items based on platform-agnostic commands and descriptors.
 * It also defines the internal state (`TreeViewInternalState`) required to manage
 * a TreeView control, such as its handle and item ID mappings.
 */
use super::app::Win32ApiInternalState;
use super::error::{PlatformError, Result as PlatformResult};
use super::types::{AppEvent, CheckState, TreeItemDescriptor, TreeItemId, WindowId};
use super::window_common::NativeWindowData; // Removed BUTTON_AREA_HEIGHT, not needed here anymore

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::Controls::{
            HTREEITEM, NM_CLICK, NMHDR, NMTREEVIEWW, TVGN_CHILD, TVGN_NEXT, TVI_LAST,
            TVIF_CHILDREN, TVIF_PARAM, TVIF_STATE, TVIF_TEXT, TVINSERTSTRUCTW, TVINSERTSTRUCTW_0,
            TVIS_STATEIMAGEMASK, TVITEMEXW, TVITEMEXW_CHILDREN, TVM_DELETEITEM, TVM_GETITEMW,
            TVM_GETNEXTITEM, TVM_INSERTITEMW, TVM_SETITEMW, TVN_ITEMCHANGEDW,
        },
        UI::WindowsAndMessaging::*,
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
/// `TreeItemId`s and native `HTREEITEM`s. This state is expected to be
/// initialized by an explicit `CreateTreeView` command.
#[derive(Debug)]
pub(crate) struct TreeViewInternalState {
    // pub(crate) hwnd: HWND, // Removed: HWND will be stored in NativeWindowData.controls
    /// Maps application-provided `TreeItemId` to the native `HTREEITEM`.
    item_id_to_htreeitem: HashMap<TreeItemId, HTREEITEM>,
    /// Maps native `HTREEITEM` back to the application-provided `TreeItemId`.
    /// Used for processing notifications.
    pub(crate) htreeitem_to_item_id: HashMap<isize, TreeItemId>,
}

impl TreeViewInternalState {
    /*
     * Creates a new `TreeViewInternalState`.
     * This is typically called after the TreeView control has been created
     * by a `PlatformCommand::CreateTreeView` handler, and its HWND stored
     * in `NativeWindowData.controls`.
     */
    pub(crate) fn new() -> Self {
        Self {
            // hwnd, // Removed
            item_id_to_htreeitem: HashMap::new(),
            htreeitem_to_item_id: HashMap::new(),
        }
    }

    fn clear_items(&mut self, hwnd_treeview: HWND) {
        unsafe {
            // TVI_ROOT is HTREEITEM(0)
            SendMessageW(
                hwnd_treeview,
                TVM_DELETEITEM,
                Some(WPARAM(0)),
                Some(LPARAM(HTREEITEM(0).0)),
            );
        }
        self.item_id_to_htreeitem.clear();
        self.htreeitem_to_item_id.clear();
    }
}

/*
 * Retrieves a mutable reference to the `TreeViewInternalState` for a given window.
 * This function assumes the TreeView and its state have already been created
 * via a `PlatformCommand::CreateTreeView`. If the state is not found,
 * it returns an error, as on-demand creation is no longer supported here.
 */
fn get_existing_treeview_state_mut<'a>(
    window_data: &'a mut NativeWindowData,
) -> PlatformResult<&'a mut TreeViewInternalState> {
    if window_data.treeview_state.is_some() {
        // This unwrap is safe because we just checked it's Some.
        Ok(window_data.treeview_state.as_mut().unwrap())
    } else {
        Err(PlatformError::InvalidHandle(format!(
            "TreeView state not found for window ID {:?}. TreeView must be created before population.",
            window_data.id
        )))
    }
}

/// Populates the TreeView control in the specified window with new items.
/// Any existing items are cleared first. This function now assumes the
/// TreeView control has already been created by a `CreateTreeView` command.
pub(crate) fn populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    items: Vec<TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "Platform: control_treeview::populate_treeview called for WinID {:?}",
        window_id
    );
    let mut windows_guard = internal_state.window_map.write().map_err(|_| {
        PlatformError::OperationFailed("Failed to acquire write lock for windows map".into())
    })?;

    if let Some(window_data) = windows_guard.get_mut(&window_id) {
        let hwnd_treeview = window_data.get_control_hwnd(ID_TREEVIEW_CTRL).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "TreeView HWND not found in controls map for window ID {:?}. TreeView must be created before population.",
                window_id
            ))
        })?;

        // Get the TreeView state, assuming it already exists.
        let tv_state = get_existing_treeview_state_mut(window_data)?;
        tv_state.clear_items(hwnd_treeview);
        log::debug!(
            "Platform: Cleared existing items from TreeView for WinID {:?}",
            window_id
        );

        for item_desc in items {
            add_treeview_item_recursive(tv_state, hwnd_treeview, HTREEITEM(0), &item_desc)?;
        }
        log::debug!(
            "Platform: Finished populating TreeView for WinID {:?}",
            window_id
        );
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
    hwnd_treeview: HWND,
    h_parent_native: HTREEITEM,
    item_desc: &TreeItemDescriptor,
) -> PlatformResult<()> {
    let mut text_buffer: Vec<u16> = item_desc.text.encode_utf16().collect();
    text_buffer.push(0); // Null-terminate

    let image_index = match item_desc.state {
        CheckState::Checked => 2, // Checked state image index (1-based for TVS_CHECKBOXES)
        CheckState::Unchecked => 1, // Unchecked state image index
    };

    let tv_item = TVITEMEXW {
        // No mut needed here as it's copied into TVINSERTSTRUCTW
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
                hwnd_treeview, // Use passed HWND
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
            add_treeview_item_recursive(
                tv_state,
                hwnd_treeview,
                h_current_item_native,
                child_desc,
            )?;
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
    let windows_guard = internal_state.window_map.read().map_err(|_| {
        PlatformError::OperationFailed("Failed to acquire read lock for windows map".into())
    })?;

    if let Some(window_data) = windows_guard.get(&window_id) {
        let hwnd_treeview = window_data.get_control_hwnd(ID_TREEVIEW_CTRL).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "TreeView HWND not found in controls map for window ID {:?} during UpdateVisualState.",
                window_id
            ))
        })?;

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
                        hwnd_treeview, // Use HWND from controls map
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
                "No TreeView state exists in window {:?} for UpdateVisualState",
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

/// Notice: There is no `TVN_ITEMCHANGEDW` notifications for TreeView controls.
/// This will NOT be reliably called for checkbox state changes
/// when using TVS_CHECKBOXES. Intsead, that is handled by NM_CLICK -> custom message.
/// This function could handle other item changes like selection, if needed.
/// it translates it into an `AppEvent` and returns it.
pub(crate) fn handle_treeview_itemchanged_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam: LPARAM, // LPARAM of WM_NOTIFY, which is LPNMHDR
) -> Option<AppEvent> {
    let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) };

    // Ensure notification is from our TreeView control by checking the ID.
    // We no longer check nmhdr.hwndFrom against tv_state.hwnd here, as tv_state no longer holds hwnd.
    // The check against ID_TREEVIEW_CTRL (done by the caller, WM_NOTIFY handler) should be sufficient
    // if there's only one TreeView per window with this ID.
    // If multiple treeviews were possible, a more robust check linking nmhdr.hwndFrom to the specific
    // treeview instance's HWND stored in window_data.controls would be needed.
    // For now, assuming ID_TREEVIEW_CTRL is unique for the main treeview.

    if nmhdr.idFrom as i32 != ID_TREEVIEW_CTRL {
        return None;
    }

    // Accessing tv_state just to see if it exists, not for HWND.
    let windows_guard = internal_state.window_map.read().ok()?;
    let window_data = windows_guard.get(&window_id)?;
    let _tv_state = window_data.treeview_state.as_ref()?; // Ensure tv_state exists

    None
    // let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
    // Currently, no AppEvents are generated from TVN_ITEMCHANGEDW.
    // If selection change events were needed, they would be handled here.
    // Checkbox changes are handled via NM_CLICK -> custom message.
}
