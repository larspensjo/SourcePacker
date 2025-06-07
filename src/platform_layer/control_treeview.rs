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

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, WPARAM},
        UI::Controls::{
            HTREEITEM, NMHDR, TVI_LAST, TVIF_CHILDREN, TVIF_PARAM, TVIF_STATE, TVIF_TEXT,
            TVINSERTSTRUCTW, TVINSERTSTRUCTW_0, TVIS_STATEIMAGEMASK, TVITEMEXW, TVITEMEXW_CHILDREN,
            TVM_DELETEITEM, TVM_INSERTITEMW, TVM_SETITEMW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::PWSTR,
};

use std::collections::HashMap;
use std::sync::Arc;

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
    control_id: i32, /* ID of the TreeView control to populate */
    items: Vec<TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "Platform: control_treeview::populate_treeview called for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    let hwnd_treeview: HWND;
    let mut taken_tv_state: Option<TreeViewInternalState>;

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
            .get_control_hwnd(control_id) // Use control_id to get HWND
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {} in WinID {:?} before populating.",
                    control_id, window_id
                ))
            })?;

        if hwnd_treeview.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "TreeView HWND is invalid for ControlID {} in WinID {:?} before populating.",
                control_id, window_id
            )));
        }
        // Take the state out of window_data. If it's None, create a new one.
        // This assumes treeview_state in NativeWindowData is for the TreeView identified by `control_id``.
        // If multiple treeviews were supported, treeview_state would need to be a map from control_id to TreeViewInternalState.
        taken_tv_state = window_data.treeview_state.take();
        if taken_tv_state.is_none() {
            log::warn!(
                "TreeView state was None for WinID {:?}/ControlID {}, creating new for population.",
                window_id,
                control_id
            );
            taken_tv_state = Some(TreeViewInternalState::new());
        }
    } // Write lock on window_map released

    // Phase 2: Perform operations on taken_tv_state and HWND, NO window_map lock held
    if let Some(mut tv_state_actual) = taken_tv_state {
        tv_state_actual.clear_items_impl(hwnd_treeview);
        log::debug!(
            "Platform: Cleared existing items from TreeView (HWND {:?}) for WinID {:?}/ControlID {}.",
            hwnd_treeview,
            window_id,
            control_id
        );

        for item_desc in items {
            if let Err(e) =
                tv_state_actual.add_item_recursive_impl(hwnd_treeview, HTREEITEM(0), &item_desc)
            {
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
            "Platform: Finished populating TreeView (HWND {:?}) for WinID {:?}/ControlID {}.",
            hwnd_treeview,
            window_id,
            control_id
        );
        taken_tv_state = Some(tv_state_actual);
    } else {
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
            log::error!(
                "WindowId {:?} disappeared while TreeView (ControlID {}) was being populated.",
                window_id,
                control_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found when trying to restore TreeView state for ControlID {}.",
                window_id, control_id
            )));
        }
    }

    Ok(())
}

pub(crate) fn update_treeview_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32, /* ID of the TreeView control */
    item_id: TreeItemId,
    new_check_state: CheckState,
) -> PlatformResult<()> {
    log::debug!(
        "Platform: control_treeview::update_treeview_item_visual_state called for WinID {:?}, ControlID {}, ItemID {:?}",
        window_id,
        control_id,
        item_id
    );

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
            .get_control_hwnd(control_id) // Use control_id to get HWND
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {} in WinID {:?} during UpdateVisualState.",
                    control_id, window_id
                ))
            })?;

        let tv_state = window_data.treeview_state.as_ref().ok_or_else(|| {
            PlatformError::OperationFailed(format!(
                "No TreeView state exists in window {:?} for UpdateVisualState (ControlID {})",
                window_id, control_id
            ))
        })?;

        h_item_native = tv_state
            .item_id_to_htreeitem
            .get(&item_id)
            .copied()
            .ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeItemId {:?} not found in window {:?}/ControlID {} for UpdateVisualState",
                    item_id, window_id, control_id
                ))
            })?;
    }

    if hwnd_treeview.is_invalid() {
        return Err(PlatformError::InvalidHandle(format!(
            "Invalid TreeView HWND for ControlID {} in visual update",
            control_id
        )));
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
            "TVM_SETITEMW failed for item {:?} in ControlID {}: {:?}",
            item_id,
            control_id,
            unsafe { GetLastError() }
        )));
    }
    Ok(())
}

pub(crate) fn handle_treeview_itemchanged_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam: LPARAM,
    control_id_from_notify: i32, /* ID of the TreeView that sent the notification */
) -> Option<AppEvent> {
    // let nmhdr = unsafe { &*(lparam.0 as *const NMHDR) }; // Already checked by caller normally

    // The 'control_id_from_notify' is the ID of the TreeView control that sent this notification.
    log::trace!(
        "control_treeview::handle_treeview_itemchanged_notification received for WinID {:?}, ControlID {}, lparam {:?}",
        window_id,
        control_id_from_notify,
        lparam
    );

    // At this point, we know the notification is TVN_ITEMCHANGEDW for the
    // TreeView identified by `control_id_from_notify`.

    let tv_state_exists;
    {
        let windows_guard = match internal_state.active_windows.read() {
            Ok(g) => g,
            Err(_) => {
                log::error!(
                    "Failed to get read lock on active_windows in handle_treeview_itemchanged_notification"
                );
                return None;
            }
        };
        let window_data = match windows_guard.get(&window_id) {
            Some(wd) => wd,
            None => {
                log::warn!(
                    "WindowData not found for WinID {:?} in handle_treeview_itemchanged_notification",
                    window_id
                );
                return None;
            }
        };

        // Assuming treeview_state in NativeWindowData is the one for the TreeView
        // identified by control_id_from_notify. If multiple TreeViews were supported,
        // TreeViewInternalState would need to be stored in a map keyed by control_id.
        // For now, we check if *any* treeview_state exists for the window.
        tv_state_exists = window_data.treeview_state.is_some();
        if !tv_state_exists {
            log::warn!(
                "handle_treeview_itemchanged_notification: tv_state does not exist for WinID {:?}/ControlID {}.",
                window_id,
                control_id_from_notify
            );
            return None;
        }
    }

    // Actual handling of TVN_ITEMCHANGEDW (if any) would go here.
    // For example, to get item details if needed:
    // let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
    // log::debug!("TVN_ITEMCHANGEDW: uOldState: {:#X}, uNewState: {:#X}, action: {:#X}, item id via param: {} for ControlID {}",
    //    nmtv.itemOld.state, nmtv.itemNew.state, nmtv.action, nmtv.itemNew.lParam, control_id_from_notify);

    None
}
