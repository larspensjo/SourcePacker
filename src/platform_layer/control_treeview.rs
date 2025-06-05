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
// use super::window_common::NativeWindowData; // No longer needed here directly for NativeWindowData struct

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, WPARAM},
        UI::Controls::{
            HTREEITEM, NM_CLICK, NMHDR, NMTREEVIEWW, TVI_LAST, TVIF_CHILDREN, TVIF_PARAM,
            TVIF_STATE, TVIF_TEXT, TVINSERTSTRUCTW, TVINSERTSTRUCTW_0, TVIS_STATEIMAGEMASK,
            TVITEMEXW, TVITEMEXW_CHILDREN, TVM_DELETEITEM, TVM_GETITEMW, TVM_INSERTITEMW,
            TVM_SETITEMW, TVN_ITEMCHANGEDW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PWSTR},
};

use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const ID_TREEVIEW_CTRL: i32 = 1001;

#[derive(Debug)]
pub(crate) struct TreeViewInternalState {
    pub(crate) item_id_to_htreeitem: HashMap<TreeItemId, HTREEITEM>,
    pub(crate) htreeitem_to_item_id: HashMap<isize, TreeItemId>,
}

impl TreeViewInternalState {
    pub(crate) fn new() -> Self {
        Self {
            item_id_to_htreeitem: HashMap::new(),
            htreeitem_to_item_id: HashMap::new(),
        }
    }

    fn clear_items_impl(&mut self, hwnd_treeview: HWND) {
        if hwnd_treeview.is_invalid() {
            log::error!("TreeViewInternalState::clear_items_impl called with invalid HWND");
            return;
        }
        unsafe {
            SendMessageW(
                hwnd_treeview,
                TVM_DELETEITEM,
                Some(WPARAM(0)),
                Some(LPARAM(HTREEITEM(0).0)),
            );
        }
        self.item_id_to_htreeitem.clear();
        self.htreeitem_to_item_id.clear();
        log::debug!(
            "TreeViewInternalState::clear_items_impl completed for HWND {:?}",
            hwnd_treeview
        );
    }

    fn add_item_recursive_impl(
        &mut self,
        hwnd_treeview: HWND,
        h_parent_native: HTREEITEM,
        item_desc: &TreeItemDescriptor,
    ) -> PlatformResult<()> {
        if hwnd_treeview.is_invalid() {
            log::error!("TreeViewInternalState::add_item_recursive_impl called with invalid HWND");
            return Err(PlatformError::InvalidHandle(
                "Invalid TreeView HWND".to_string(),
            ));
        }

        let mut text_buffer: Vec<u16> = item_desc.text.encode_utf16().collect();
        text_buffer.push(0);

        let image_index = match item_desc.state {
            CheckState::Checked => 2,
            CheckState::Unchecked => 1,
        };

        let tv_item = TVITEMEXW {
            mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN | TVIF_STATE,
            hItem: HTREEITEM::default(),
            pszText: PWSTR(text_buffer.as_mut_ptr()),
            cchTextMax: text_buffer.len() as i32,
            lParam: LPARAM(item_desc.id.0 as isize),
            cChildren: TVITEMEXW_CHILDREN(if item_desc.is_folder { 1 } else { 0 }),
            state: (image_index as u32) << 12,
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
                    hwnd_treeview,
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

        self.item_id_to_htreeitem
            .insert(item_desc.id, h_current_item_native);
        self.htreeitem_to_item_id
            .insert(h_current_item_native.0, item_desc.id);

        if item_desc.is_folder && !item_desc.children.is_empty() {
            for child_desc in &item_desc.children {
                self.add_item_recursive_impl(hwnd_treeview, h_current_item_native, child_desc)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    items: Vec<TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "Platform: control_treeview::populate_treeview called for WinID {:?}",
        window_id
    );

    let hwnd_treeview: HWND;
    let mut taken_tv_state: Option<TreeViewInternalState>; // To hold the state outside the lock

    // Phase 1: Lock, get HWND, take tv_state
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to lock windows map for populate_treeview (phase 1)".into(),
            )
        })?;

        let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for populate_treeview",
                window_id
            ))
        })?;

        hwnd_treeview = window_data
            .get_control_hwnd(ID_TREEVIEW_CTRL)
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for window ID {:?} before populating.",
                    window_id
                ))
            })?;

        if hwnd_treeview.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "TreeView HWND is invalid for window ID {:?} before populating.",
                window_id
            )));
        }
        // Take the state out of window_data. If it's None, create a new one.
        taken_tv_state = window_data.treeview_state.take();
        if taken_tv_state.is_none() {
            // This case should ideally not happen if CreateTreeView command ensures state is created.
            log::warn!(
                "TreeView state was None for WinID {:?}, creating new for population.",
                window_id
            );
            taken_tv_state = Some(TreeViewInternalState::new());
        }
    } // Write lock on window_map released

    // Phase 2: Perform operations on taken_tv_state and HWND, NO window_map lock held
    if let Some(mut tv_state_actual) = taken_tv_state {
        tv_state_actual.clear_items_impl(hwnd_treeview);
        log::debug!(
            "Platform: Cleared existing items from TreeView (HWND {:?}) for WinID {:?}",
            hwnd_treeview,
            window_id
        );

        for item_desc in items {
            if let Err(e) =
                tv_state_actual.add_item_recursive_impl(hwnd_treeview, HTREEITEM(0), &item_desc)
            {
                // If adding an item fails, we should put the state back before returning the error.
                // Re-acquire lock to put state back.
                {
                    let mut windows_map_guard =
                        internal_state.active_windows.write().map_err(|_| {
                            PlatformError::OperationFailed(
                                "Failed to lock windows map for populate_treeview (error recovery)"
                                    .into(),
                            )
                        })?;
                    if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
                        window_data.treeview_state = Some(tv_state_actual);
                    } else {
                        log::error!(
                            "Failed to put back tv_state for {:?} after error: window not found",
                            window_id
                        );
                    }
                }
                return Err(e);
            }
        }
        log::debug!(
            "Platform: Finished populating TreeView (HWND {:?}) for WinID {:?}",
            hwnd_treeview,
            window_id
        );
        // Update taken_tv_state with the modified state
        taken_tv_state = Some(tv_state_actual);
    } else {
        // This should not be reached if taken_tv_state was initialized above.
        return Err(PlatformError::OperationFailed(
            "TreeView state was unexpectedly None after take".to_string(),
        ));
    }

    // Phase 3: Lock, put tv_state back
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to lock windows map for populate_treeview (phase 3)".into(),
            )
        })?;
        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            window_data.treeview_state = taken_tv_state;
        } else {
            // This would be a serious issue if the window disappeared between phases.
            log::error!(
                "WindowId {:?} disappeared while TreeView was being populated.",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found when trying to restore TreeView state.",
                window_id
            )));
        }
    } // Write lock released

    Ok(())
}

pub(crate) fn update_treeview_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    item_id: TreeItemId,
    new_check_state: CheckState,
) -> PlatformResult<()> {
    let hwnd_treeview: HWND;
    let h_item_native: HTREEITEM;

    {
        let windows_guard = internal_state.active_windows.read().map_err(|_| {
            PlatformError::OperationFailed(
                "Failed to acquire read lock for windows map (update visual)".into(),
            )
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for UpdateVisualState",
                window_id
            ))
        })?;

        hwnd_treeview = window_data
            .get_control_hwnd(ID_TREEVIEW_CTRL)
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for window ID {:?} during UpdateVisualState.",
                    window_id
                ))
            })?;

        let tv_state = window_data.treeview_state.as_ref().ok_or_else(|| {
            PlatformError::OperationFailed(format!(
                "No TreeView state exists in window {:?} for UpdateVisualState",
                window_id
            ))
        })?;

        h_item_native = tv_state
            .item_id_to_htreeitem
            .get(&item_id)
            .copied()
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeItemId {:?} not found in window {:?} for UpdateVisualState",
                    item_id, window_id
                ))
            })?;
    }

    if hwnd_treeview.is_invalid() {
        return Err(PlatformError::InvalidHandle(
            "Invalid TreeView HWND for visual update".to_string(),
        ));
    }

    let image_index = match new_check_state {
        CheckState::Checked => 2,
        CheckState::Unchecked => 1,
    };

    let mut tv_item_update = TVITEMEXW {
        mask: TVIF_STATE,
        hItem: h_item_native,
        state: (image_index as u32) << 12,
        stateMask: TVIS_STATEIMAGEMASK.0,
        ..Default::default()
    };

    let send_result = unsafe {
        SendMessageW(
            hwnd_treeview,
            TVM_SETITEMW,
            Some(WPARAM(0)),
            Some(LPARAM(&mut tv_item_update as *mut _ as isize)),
        )
    };

    if send_result.0 == 0 {
        return Err(PlatformError::OperationFailed(format!(
            "TVM_SETITEMW failed for item {:?}: {:?}",
            item_id,
            unsafe { GetLastError() }
        )));
    }
    Ok(())
}

pub(crate) fn handle_treeview_itemchanged_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam: LPARAM,
) -> Option<AppEvent> {
    let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) };

    if nmhdr.idFrom as i32 != ID_TREEVIEW_CTRL {
        return None;
    }

    let tv_state_exists;
    {
        let windows_guard = internal_state.active_windows.read().ok()?;
        let window_data = windows_guard.get(&window_id)?;
        tv_state_exists = window_data.treeview_state.is_some();
    }

    if !tv_state_exists {
        log::warn!(
            "handle_treeview_itemchanged_notification: tv_state does not exist for WinID {:?}",
            window_id
        );
        return None;
    }

    // Actual handling of TVN_ITEMCHANGEDW (if any) would go here.
    // For example, to get item details if needed:
    // let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
    // log::debug!("TVN_ITEMCHANGEDW: uOldState: {:#X}, uNewState: {:#X}, action: {:#X}, item id via param: {}",
    //    nmtv.itemOld.state, nmtv.itemNew.state, nmtv.action, nmtv.itemNew.lParam);

    None
}
