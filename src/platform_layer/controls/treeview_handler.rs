/*
 * This module provides platform-specific (Win32) implementations for TreeView control
 * operations. It handles the creation, population, manipulation, custom drawing,
 * and event handling for native TreeView items based on platform-agnostic commands
 * and descriptors. It also defines the internal state (`TreeViewInternalState`)
 * required to manage a TreeView control.
 *
 * It centralizes all TreeView-related Win32 API interactions, making other parts
 * of the platform layer (like command_executor and window_common) less coupled
 * to specific TreeView details.
 */
use crate::platform_layer::app::Win32ApiInternalState;
use crate::platform_layer::error::{PlatformError, Result as PlatformResult};
use crate::platform_layer::types::{
    AppEvent, CheckState, PlatformEventHandler, TreeItemDescriptor, TreeItemId, WindowId,
};

use windows::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            CreateSolidBrush, DeleteObject, Ellipse, HGDIOBJ, InvalidateRect, ScreenToClient,
            SelectObject,
        },
        UI::Controls::{
            CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT,
            CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTPAINT, HTREEITEM, NMHDR, NMTVCUSTOMDRAW,
            TVHITTESTINFO, TVHITTESTINFO_FLAGS, TVHT_ONITEMSTATEICON, TVI_LAST, TVIF_CHILDREN,
            TVIF_PARAM, TVIF_STATE, TVIF_TEXT, TVINSERTSTRUCTW, TVINSERTSTRUCTW_0,
            TVIS_STATEIMAGEMASK, TVITEMEXW, TVITEMEXW_CHILDREN, TVM_DELETEITEM, TVM_GETITEMRECT,
            TVM_GETITEMW, TVM_HITTEST, TVM_INSERTITEMW, TVM_SETITEMW, TVS_CHECKBOXES,
            TVS_HASBUTTONS, TVS_HASLINES, TVS_LINESATROOT, TVS_SHOWSELALWAYS, WC_TREEVIEWW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::PWSTR,
};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Constants from window_common, now perhaps better placed here or in a shared consts module if widely used.
// TODO: This shouldn't be hardcoded. Create some mecchanism for this.
const CIRCLE_DIAMETER: i32 = 6;
const CIRCLE_COLOR_BLUE: windows::Win32::Foundation::COLORREF =
    windows::Win32::Foundation::COLORREF(0x00FF0000); // BGR format for Blue

/*
 * Holds internal state specific to a TreeView control instance.
 * This includes mappings between application-defined `TreeItemId`s and native
 * `HTREEITEM` handles, which are essential for translating commands and events.
 */
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
                Some(LPARAM(HTREEITEM(0).0)), // Passing TVI_ROOT (0) or NULL deletes all items
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
        text_buffer.push(0); // Null terminator

        // Determine the state image index for checkbox (1-based: 1 for unchecked, 2 for checked)
        let image_index = match item_desc.state {
            CheckState::Checked => 2,
            CheckState::Unchecked => 1,
        };

        let tv_item = TVITEMEXW {
            mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN | TVIF_STATE,
            hItem: HTREEITEM::default(), // Will be filled by the system if successful
            pszText: PWSTR(text_buffer.as_mut_ptr()),
            cchTextMax: text_buffer.len() as i32,
            lParam: LPARAM(item_desc.id.0 as isize), // Store app-specific TreeItemId
            cChildren: TVITEMEXW_CHILDREN(if item_desc.is_folder { 1 } else { 0 }), // Hint if it has children
            state: (image_index as u32) << 12, // Set state image index (shifted by 12 bits)
            stateMask: TVIS_STATEIMAGEMASK.0,  // Mask to indicate we are setting state image
            ..Default::default()
        };

        let tv_insert_struct = TVINSERTSTRUCTW {
            hParent: h_parent_native,
            hInsertAfter: TVI_LAST, // Insert at the end of the parent's children
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
            // TVM_INSERTITEMW returns NULL on failure
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

        // Recursively add children if this item is a folder and has children
        if item_desc.is_folder && !item_desc.children.is_empty() {
            for child_desc in &item_desc.children {
                self.add_item_recursive_impl(hwnd_treeview, h_current_item_native, child_desc)?;
            }
        }
        Ok(())
    }
}

/*
 * Handles the creation of a native TreeView control.
 * This function takes the window ID and a logical control ID, creates the
 * TreeView using CreateWindowExW, and initializes its internal state within
 * the corresponding NativeWindowData.
 */
pub(crate) fn handle_create_treeview_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: handle_create_treeview_command for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    let hwnd_parent_for_creation: HWND;
    let h_instance_for_creation: HINSTANCE;

    // Phase 1: Acquire read lock, perform checks, and get necessary data for CreateWindowExW
    {
        let windows_map_guard = internal_state.active_windows.read().map_err(|e|{
            log::error!("TreeViewHandler: Failed to lock windows map (read) for TreeView creation pre-check: {:?}", e);
            PlatformError::OperationFailed("Failed to lock windows map (read) for TreeView creation pre-check".into())
        })?;

        let window_data = windows_map_guard.get(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for CreateTreeView pre-check.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for CreateTreeView pre-check",
                window_id
            ))
        })?;

        if window_data.control_hwnd_map.contains_key(&control_id)
            || window_data.treeview_state.is_some()
        {
            log::warn!(
                "TreeViewHandler: TreeView with ID {} or existing TreeView state already present for window {:?}.",
                control_id,
                window_id
            );
            return Err(PlatformError::ControlCreationFailed(format!(
                "TreeView with ID {} or existing TreeView state already present for window {:?}",
                control_id, window_id
            )));
        }
        hwnd_parent_for_creation = window_data.this_window_hwnd;
        h_instance_for_creation = internal_state.h_instance;

        if hwnd_parent_for_creation.is_invalid() {
            log::warn!(
                "TreeViewHandler: Parent HWND for CreateTreeView is invalid (WinID: {:?})",
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "Parent HWND for CreateTreeView is invalid (WinID: {:?})",
                window_id
            )));
        }
    } // Read lock released

    // Phase 2: Create the window without holding the lock
    let tvs_style = WINDOW_STYLE(
        TVS_HASLINES | TVS_LINESATROOT | TVS_HASBUTTONS | TVS_SHOWSELALWAYS | TVS_CHECKBOXES,
    );
    let combined_style = WS_CHILD | WS_VISIBLE | WS_BORDER | tvs_style;
    let hwnd_tv = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WC_TREEVIEWW, // Standard class name for TreeView
            None,         // No window text/title for a control
            combined_style,
            0,
            0,
            10,
            10, // Dummy position/size, layout rules will adjust
            Some(hwnd_parent_for_creation),
            Some(HMENU(control_id as *mut _)), // Use logical ID for HMENU
            Some(h_instance_for_creation),
            None, // No extra creation parameters
        )?
    };

    // Phase 3: Re-acquire write lock to update NativeWindowData
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
            log::error!(
                "TreeViewHandler: Failed to re-acquire write lock for TreeView creation post-update: {:?}",
                e
            );
            unsafe { DestroyWindow(hwnd_tv).ok(); } // Try to clean up orphaned window
            PlatformError::OperationFailed(
                "Failed to re-acquire write lock for TreeView creation post-update".into(),
            )
        })?;

        let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} no longer exists for CreateTreeView post-update. Destroying orphaned control.",
                window_id
            );
            unsafe { DestroyWindow(hwnd_tv).ok(); }
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} no longer exists for CreateTreeView post-update",
                window_id
            ))
        })?;

        // Check again in case the window was destroyed or control created by another thread
        if window_data.control_hwnd_map.contains_key(&control_id)
            || window_data.treeview_state.is_some()
        {
            log::warn!(
                "TreeViewHandler: TreeView (ID {}) or state for window {:?} was created concurrently or window was altered. Destroying newly created one.",
                control_id,
                window_id
            );
            unsafe {
                DestroyWindow(hwnd_tv).ok();
            }
            return Err(PlatformError::ControlCreationFailed(format!(
                "TreeView with ID {} or state was concurrently created for window {:?}",
                control_id, window_id
            )));
        }

        window_data.control_hwnd_map.insert(control_id, hwnd_tv);
        window_data.treeview_state = Some(TreeViewInternalState::new());
        log::debug!(
            "TreeViewHandler: Created TreeView (ID {}) for window {:?} with HWND {:?}",
            control_id,
            window_id,
            hwnd_tv
        );
    } // Write lock is released

    Ok(())
}

/*
 * Populates a TreeView control with a given set of item descriptors.
 * This function clears any existing items in the TreeView and then recursively
 * adds the new items. It manages the internal `TreeViewInternalState` for
 * mapping application item IDs to native handles.
 */
pub(crate) fn populate_treeview(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    items: Vec<TreeItemDescriptor>,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: populate_treeview called for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    let hwnd_treeview: HWND;
    let mut taken_tv_state: Option<TreeViewInternalState>;

    // Phase 1: Lock, get HWND, take tv_state
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
            log::error!(
                "TreeViewHandler: Failed to lock windows map for populate_treeview (phase 1): {:?}",
                e
            );
            PlatformError::OperationFailed(
                "Failed to lock windows map for populate_treeview (phase 1)".into(),
            )
        })?;

        let window_data = windows_map_guard.get_mut(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for populate_treeview.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for populate_treeview",
                window_id
            ))
        })?;

        hwnd_treeview = window_data
            .get_control_hwnd(control_id)
            .ok_or_else(|| {
                log::warn!("TreeViewHandler: TreeView HWND not found for ControlID {} in WinID {:?} before populating.", control_id, window_id);
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {} in WinID {:?} before populating.",
                    control_id, window_id
                ))
            })?;

        if hwnd_treeview.is_invalid() {
            log::warn!(
                "TreeViewHandler: TreeView HWND is invalid for ControlID {} in WinID {:?} before populating.",
                control_id,
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "TreeView HWND is invalid for ControlID {} in WinID {:?} before populating.",
                control_id, window_id
            )));
        }
        taken_tv_state = window_data.treeview_state.take();
        if taken_tv_state.is_none() {
            log::warn!(
                "TreeViewHandler: TreeView state was None for WinID {:?}/ControlID {}, creating new for population.",
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
            "TreeViewHandler: Cleared existing items from TreeView (HWND {:?}) for WinID {:?}/ControlID {}.",
            hwnd_treeview,
            window_id,
            control_id
        );

        for item_desc in items {
            if let Err(e) =
                tv_state_actual.add_item_recursive_impl(hwnd_treeview, HTREEITEM(0), &item_desc)
            {
                // Attempt to put state back on error
                let mut windows_map_guard = internal_state.active_windows.write().map_err(|re_lock_err| {
                    log::error!("TreeViewHandler: Failed to re-lock windows map for populate_treeview (error recovery): {:?}", re_lock_err);
                    // Original error `e` is more important, but log this too.
                    e.clone() // Return original error
                })?;
                if let Some(window_data_err_case) = windows_map_guard.get_mut(&window_id) {
                    window_data_err_case.treeview_state = Some(tv_state_actual);
                } else {
                    log::error!(
                        "TreeViewHandler: Failed to put back tv_state for {:?} after error: window not found",
                        window_id
                    );
                }
                return Err(e);
            }
        }
        log::debug!(
            "TreeViewHandler: Finished populating TreeView (HWND {:?}) for WinID {:?}/ControlID {}.",
            hwnd_treeview,
            window_id,
            control_id
        );
        taken_tv_state = Some(tv_state_actual); // tv_state_actual is moved back
    } else {
        // This should not happen if the logic in Phase 1 is correct
        log::error!(
            "TreeViewHandler: TreeView state was unexpectedly None after take in populate_treeview."
        );
        return Err(PlatformError::OperationFailed(
            "TreeView state was unexpectedly None after take".to_string(),
        ));
    }

    // Phase 3: Lock, put tv_state back
    {
        let mut windows_map_guard = internal_state.active_windows.write().map_err(|e| {
            log::error!(
                "TreeViewHandler: Failed to lock windows map for populate_treeview (phase 3): {:?}",
                e
            );
            PlatformError::OperationFailed(
                "Failed to lock windows map for populate_treeview (phase 3)".into(),
            )
        })?;
        if let Some(window_data) = windows_map_guard.get_mut(&window_id) {
            window_data.treeview_state = taken_tv_state;
        } else {
            log::error!(
                "TreeViewHandler: WindowId {:?} disappeared while TreeView (ControlID {}) was being populated.",
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

/*
 * Updates the visual state (specifically the checkbox) of a single TreeView item.
 * It maps the application-defined `TreeItemId` to its native `HTREEITEM` and sends
 * a `TVM_SETITEMW` message to change its state image index.
 */
pub(crate) fn update_treeview_item_visual_state(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    item_id: TreeItemId,
    new_check_state: CheckState,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: update_treeview_item_visual_state for WinID {:?}, ControlID {}, ItemID {:?}",
        window_id,
        control_id,
        item_id
    );

    let hwnd_treeview: HWND;
    let h_item_native: HTREEITEM;

    {
        // Read lock scope
        let windows_guard = internal_state.active_windows.read().map_err(|e|{
            log::error!("TreeViewHandler: Failed to acquire read lock for windows map (update visual): {:?}",e);
            PlatformError::OperationFailed("Failed to acquire read lock for windows map (update visual)".into())
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for UpdateVisualState.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for UpdateVisualState",
                window_id
            ))
        })?;

        hwnd_treeview = window_data
            .get_control_hwnd(control_id)
            .ok_or_else(|| {
                log::warn!("TreeViewHandler: TreeView HWND not found for ControlID {} in WinID {:?} during UpdateVisualState.", control_id, window_id);
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {} in WinID {:?} during UpdateVisualState.",
                    control_id, window_id
                ))
            })?;

        let tv_state = window_data.treeview_state.as_ref().ok_or_else(|| {
            log::warn!("TreeViewHandler: No TreeView state exists in window {:?} for UpdateVisualState (ControlID {})", window_id, control_id);
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
                 log::warn!("TreeViewHandler: TreeItemId {:?} not found in window {:?}/ControlID {} for UpdateVisualState", item_id, window_id, control_id);
                PlatformError::InvalidHandle(format!(
                    "TreeItemId {:?} not found in window {:?}/ControlID {} for UpdateVisualState",
                    item_id, window_id, control_id
                ))
            })?;
    } // Read lock released

    if hwnd_treeview.is_invalid() {
        log::warn!(
            "TreeViewHandler: Invalid TreeView HWND for ControlID {} in visual update",
            control_id
        );
        return Err(PlatformError::InvalidHandle(format!(
            "Invalid TreeView HWND for ControlID {} in visual update",
            control_id
        )));
    }

    let image_index = match new_check_state {
        CheckState::Checked => 2,   // Index for checked state image
        CheckState::Unchecked => 1, // Index for unchecked state image
    };

    let mut tv_item_update = TVITEMEXW {
        mask: TVIF_STATE,
        hItem: h_item_native,
        state: (image_index as u32) << 12, // State image index is bits 12-15 of state
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
        // TVM_SETITEMW returns 0 on failure
        let last_error = unsafe { GetLastError() };
        log::error!(
            "TreeViewHandler: TVM_SETITEMW failed for item {:?} in ControlID {}: {:?}",
            item_id,
            control_id,
            last_error
        );
        return Err(PlatformError::OperationFailed(format!(
            "TVM_SETITEMW failed for item {:?} in ControlID {}: {:?}",
            item_id, control_id, last_error
        )));
    }
    Ok(())
}

/*
 * Handles the TVN_ITEMCHANGEDW notification for a TreeView.
 * This notification is sent for various item state changes, but this handler
 * currently only logs the event. More specific handling (e.g., for selection
 * changes if needed by app logic) could be added here.
 */
pub(crate) fn handle_treeview_itemchanged_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam: LPARAM,
    control_id_from_notify: i32,
) -> Option<AppEvent> {
    log::trace!(
        "TreeViewHandler: handle_treeview_itemchanged_notification received for WinID {:?}, ControlID {}, lparam {:?}",
        window_id,
        control_id_from_notify,
        lparam
    );

    // Ensure TreeView state exists for this control_id
    let windows_guard = match internal_state.active_windows.read() {
        Ok(g) => g,
        Err(e) => {
            log::error!(
                "TreeViewHandler: Failed to get read lock on active_windows in handle_treeview_itemchanged_notification: {:?}",
                e
            );
            return None;
        }
    };
    let window_data = match windows_guard.get(&window_id) {
        Some(wd) => wd,
        None => {
            log::warn!(
                "TreeViewHandler: WindowData not found for WinID {:?} in handle_treeview_itemchanged_notification",
                window_id
            );
            return None;
        }
    };
    if window_data.treeview_state.is_none() {
        log::warn!(
            "TreeViewHandler: handle_treeview_itemchanged_notification: tv_state does not exist for WinID {:?}/ControlID {}.",
            window_id,
            control_id_from_notify
        );
        return None;
    }
    // let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
    // log::debug!("TVN_ITEMCHANGEDW: uOldState: {:#X}, uNewState: {:#X}, action: {:#X}, item id via param: {} for ControlID {}",
    //    nmtv.itemOld.state, nmtv.itemNew.state, nmtv.action, nmtv.itemNew.lParam, control_id_from_notify);
    None // No AppEvent generated from this notification directly for now
}

/*
 * Executes the `RedrawTreeItem` command by invalidating the rectangle of a specific item.
 * This function retrieves the native `HTREEITEM` for the given `TreeItemId` and
 * uses `TVM_GETITEMRECT` to find its bounding box. It then calls `InvalidateRect`
 * to force a repaint of that item, which is crucial for updating custom-drawn
 * elements like the "New" item indicator.
 */
pub(crate) fn handle_redraw_tree_item_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    item_id: TreeItemId,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: handle_redraw_tree_item_command for WinID {:?}, ControlID {}, ItemID {:?}",
        window_id,
        control_id,
        item_id
    );

    let hwnd_treeview: HWND;
    let htreeitem: HTREEITEM;

    {
        // Read lock scope
        let windows_guard = internal_state.active_windows.read().map_err(|e| {
            log::error!("TreeViewHandler: Failed to acquire read lock on windows map for RedrawTreeItem: {:?}", e);
            PlatformError::OperationFailed("Failed to lock active_windows map for RedrawTreeItem".into())
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for RedrawTreeItem.",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for RedrawTreeItem",
                window_id
            ))
        })?;

        hwnd_treeview = window_data
            .get_control_hwnd(control_id)
            .ok_or_else(|| {
                log::warn!("TreeViewHandler: TreeView control (ID {}) not found for WinID {:?} during RedrawTreeItem.", control_id, window_id);
                PlatformError::InvalidHandle(format!(
                    "TreeView control (ID {}) not found for WinID {:?} during RedrawTreeItem.",
                    control_id, window_id
                ))
            })?;

        if hwnd_treeview.is_invalid() {
            log::warn!(
                "TreeViewHandler: TreeView HWND for ControlID {} is invalid for WinID {:?} during RedrawTreeItem.",
                control_id,
                window_id
            );
            return Err(PlatformError::InvalidHandle(format!(
                "TreeView HWND for ControlID {} is invalid for WinID {:?} during RedrawTreeItem.",
                control_id, window_id
            )));
        }

        let tv_state = window_data.treeview_state.as_ref().ok_or_else(|| {
            log::warn!("TreeViewHandler: TreeView state not found for WinID {:?} (ControlID {}) during RedrawTreeItem.", window_id, control_id);
            PlatformError::OperationFailed(format!(
                "TreeView state not found for WinID {:?} (ControlID {}) during RedrawTreeItem.",
                window_id, control_id
            ))
        })?;

        htreeitem = tv_state.item_id_to_htreeitem.get(&item_id).copied().ok_or_else(|| {
            log::warn!("TreeViewHandler: HTREEITEM not found for ItemID {:?} (ControlID {}) during RedrawTreeItem. Cannot invalidate.", item_id, control_id);
            PlatformError::InvalidHandle(format!(
                "HTREEITEM not found for ItemID {:?} (ControlID {}) during RedrawTreeItem.",
                item_id, control_id
            ))
        })?;
    } // Read lock released

    let mut item_rect = RECT::default();
    unsafe {
        *((&mut item_rect as *mut RECT) as *mut HTREEITEM) = htreeitem;
    }

    let get_rect_success = unsafe {
        SendMessageW(
            hwnd_treeview,
            TVM_GETITEMRECT,
            Some(WPARAM(0)), // TRUE for text-only, FALSE for whole item
            Some(LPARAM(&mut item_rect as *mut _ as isize)),
        )
    };

    if get_rect_success.0 != 0 {
        // Non-zero indicates success
        unsafe {
            _ = InvalidateRect(Some(hwnd_treeview), Some(&item_rect), true); // TRUE for bErase
        }
        log::debug!(
            "TreeViewHandler: Invalidated rect {:?} for item ID {:?} (HTREEITEM {:?}, ControlID {})",
            item_rect,
            item_id,
            htreeitem,
            control_id
        );
    } else {
        let last_error = unsafe { GetLastError() };
        log::warn!(
            "TreeViewHandler: TVM_GETITEMRECT failed for item ID {:?} (HTREEITEM {:?}, ControlID {}) during RedrawTreeItem. Invalidating whole control. Error: {:?}",
            item_id,
            htreeitem,
            control_id,
            last_error
        );
        unsafe {
            _ = InvalidateRect(Some(hwnd_treeview), None, true); // Invalidate the whole TreeView
        }
    }
    Ok(())
}

pub(crate) fn expand_visible_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: expand_visible_tree_items for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    let hwnd_treeview = {
        let windows_guard = internal_state.active_windows.read().map_err(|e| {
            log::error!(
                "TreeViewHandler: Failed to lock windows map (expand_visible_tree_items): {:?}",
                e
            );
            PlatformError::OperationFailed(
                "Failed to lock windows map for expand_visible_tree_items".into(),
            )
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for expand_visible_tree_items",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for expand_visible_tree_items",
                window_id
            ))
        })?;

        let hwnd = window_data
            .get_control_hwnd(control_id)
            .ok_or_else(|| {
                log::warn!(
                    "TreeViewHandler: Control ID {} not found for expand_visible_tree_items in WinID {:?}",
                    control_id,
                    window_id
                );
                PlatformError::InvalidHandle(format!(
                    "Control ID {} not found for expand_visible_tree_items in WinID {:?}",
                    control_id, window_id
                ))
            })?;
        hwnd
    };

    if hwnd_treeview.is_invalid() {
        log::warn!(
            "TreeViewHandler: HWND invalid for expand_visible_tree_items in ControlID {}",
            control_id
        );
        return Err(PlatformError::InvalidHandle(
            "Invalid TreeView HWND for expand_visible_tree_items".to_string(),
        ));
    }

    use windows::Win32::UI::Controls::{
        TVE_EXPAND, TVGN_FIRSTVISIBLE, TVGN_NEXTVISIBLE, TVM_EXPAND, TVM_GETNEXTITEM,
    };

    unsafe {
        let mut item = SendMessageW(
            hwnd_treeview,
            TVM_GETNEXTITEM,
            Some(WPARAM(TVGN_FIRSTVISIBLE as usize)),
            Some(LPARAM(0)),
        );

        while item.0 != 0 {
            _ = SendMessageW(
                hwnd_treeview,
                TVM_EXPAND,
                Some(WPARAM(TVE_EXPAND.0 as usize)),
                Some(LPARAM(item.0)),
            );

            item = SendMessageW(
                hwnd_treeview,
                TVM_GETNEXTITEM,
                Some(WPARAM(TVGN_NEXTVISIBLE as usize)),
                Some(LPARAM(item.0)),
            );
        }
    }

    Ok(())
}

pub(crate) fn expand_all_tree_items(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: expand_all_tree_items for WinID {:?}, ControlID {}",
        window_id,
        control_id
    );

    let hwnd_treeview = {
        let windows_guard = internal_state.active_windows.read().map_err(|e| {
            log::error!(
                "TreeViewHandler: Failed to lock windows map (expand_all_tree_items): {:?}",
                e
            );
            PlatformError::OperationFailed(
                "Failed to lock windows map for expand_all_tree_items".into(),
            )
        })?;

        let window_data = windows_guard.get(&window_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: WindowId {:?} not found for expand_all_tree_items",
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "WindowId {:?} not found for expand_all_tree_items",
                window_id
            ))
        })?;

        let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
            log::warn!(
                "TreeViewHandler: Control ID {} not found for expand_all_tree_items in WinID {:?}",
                control_id,
                window_id
            );
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found for expand_all_tree_items in WinID {:?}",
                control_id, window_id
            ))
        })?;
        hwnd
    };

    if hwnd_treeview.is_invalid() {
        log::warn!(
            "TreeViewHandler: HWND invalid for expand_all_tree_items in ControlID {}",
            control_id
        );
        return Err(PlatformError::InvalidHandle(
            "Invalid TreeView HWND for expand_all_tree_items".to_string(),
        ));
    }

    use windows::Win32::UI::Controls::{
        TVE_EXPAND, TVGN_CHILD, TVGN_NEXT, TVGN_ROOT, TVM_EXPAND, TVM_GETNEXTITEM,
    };

    unsafe fn recurse(hwnd: HWND, item: HTREEITEM) {
        if item.0 == 0 {
            return;
        }
        unsafe {
            let _ = SendMessageW(
                hwnd,
                TVM_EXPAND,
                Some(WPARAM(TVE_EXPAND.0 as usize)),
                Some(LPARAM(item.0)),
            );
        }
        let mut child = unsafe {
            SendMessageW(
                hwnd,
                TVM_GETNEXTITEM,
                Some(WPARAM(TVGN_CHILD as usize)),
                Some(LPARAM(item.0)),
            )
        };
        unsafe {
            while child.0 != 0 {
                recurse(hwnd, HTREEITEM(child.0));
                child = SendMessageW(
                    hwnd,
                    TVM_GETNEXTITEM,
                    Some(WPARAM(TVGN_NEXT as usize)),
                    Some(LPARAM(child.0)),
                );
            }
        }
    }

    unsafe {
        let mut root = SendMessageW(
            hwnd_treeview,
            TVM_GETNEXTITEM,
            Some(WPARAM(TVGN_ROOT as usize)),
            Some(LPARAM(0)),
        );
        while root.0 != 0 {
            recurse(hwnd_treeview, HTREEITEM(root.0));
            root = SendMessageW(
                hwnd_treeview,
                TVM_GETNEXTITEM,
                Some(WPARAM(TVGN_NEXT as usize)),
                Some(LPARAM(root.0)),
            );
        }
    }

    Ok(())
}

/*
 * Handles the NM_CUSTOMDRAW notification for a TreeView control.
 * This function orchestrates the custom drawing stages necessary to render a "New"
 * item indicator (a blue circle) next to items identified as new by the application logic.
 * It communicates with the `PlatformEventHandler` to query the "new" status of items.
 */
pub(crate) fn handle_nm_customdraw(
    _internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam_nmcustomdraw: LPARAM, // This is NMTVCUSTOMDRAW*
    event_handler_opt: Option<&Arc<Mutex<dyn PlatformEventHandler>>>,
    control_id_of_treeview: i32,
) -> LRESULT {
    let nmtvcd = unsafe { &*(lparam_nmcustomdraw.0 as *const NMTVCUSTOMDRAW) };

    match nmtvcd.nmcd.dwDrawStage {
        CDDS_PREPAINT => {
            log::trace!(
                "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): CDDS_PREPAINT. Requesting CDRF_NOTIFYITEMDRAW.",
                window_id,
                control_id_of_treeview
            );
            return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
        }
        CDDS_ITEMPREPAINT => {
            let tree_item_id_val = nmtvcd.nmcd.lItemlParam; // App-specific ID from lParam
            let tree_item_id = TreeItemId(tree_item_id_val.0 as u64);
            log::trace!(
                "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): CDDS_ITEMPREPAINT for AppTreeItemId {:?}",
                window_id,
                control_id_of_treeview,
                tree_item_id
            );

            if let Some(handler_arc) = event_handler_opt {
                if let Ok(handler_guard) = handler_arc.lock() {
                    if handler_guard.is_tree_item_new(window_id, tree_item_id) {
                        log::debug!(
                            "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Item {:?} IS NEW. Requesting CDRF_NOTIFYPOSTPAINT.",
                            window_id,
                            control_id_of_treeview,
                            tree_item_id
                        );
                        return LRESULT(CDRF_NOTIFYPOSTPAINT as isize);
                    } else {
                        log::trace!(
                            "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Item {:?} IS NOT NEW. Requesting CDRF_DODEFAULT.",
                            window_id,
                            control_id_of_treeview,
                            tree_item_id
                        );
                    }
                } else {
                    log::warn!(
                        "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Failed to lock event handler for ITEMPREPAINT. Defaulting for item {:?}.",
                        window_id,
                        control_id_of_treeview,
                        tree_item_id
                    );
                }
            } else {
                log::warn!(
                    "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Event handler not available for ITEMPREPAINT. Defaulting for item {:?}.",
                    window_id,
                    control_id_of_treeview,
                    tree_item_id
                );
            }
            return LRESULT(CDRF_DODEFAULT as isize);
        }
        CDDS_ITEMPOSTPAINT => {
            let hdc = nmtvcd.nmcd.hdc;
            let h_item_native = HTREEITEM(nmtvcd.nmcd.dwItemSpec as isize); // Native HTREEITEM
            let hwnd_treeview = nmtvcd.nmcd.hdr.hwndFrom;
            let tree_item_id_val = nmtvcd.nmcd.lItemlParam; // App-specific ID
            let tree_item_id = TreeItemId(tree_item_id_val.0 as u64);

            let mut item_rect_text_part = RECT::default();
            // To get rect for a specific item for drawing, it's common to put HTREEITEM in rect.left
            // when calling TVM_GETITEMRECT with wParam = TRUE (text part only).
            // Or, more simply, pass the HTREEITEM as part of the structure.
            // For TVM_GETITEMRECT, rect.left seems to be an input for *which* item if item is not selected.
            // The problem P3.2 description suggests TVM_GETITEMRECT with wParam=FALSE (full item)
            // was returning narrow rects, and wParam=TRUE (text-only) might be better.

            // Let's try to get the text-only rectangle as per P3.2 action plan.
            // The HTREEITEM needs to be communicated to TVM_GETITEMRECT.
            // A common way: copy h_item_native into the RECT's first field(s).
            unsafe {
                *((&mut item_rect_text_part as *mut RECT) as *mut HTREEITEM) = h_item_native;
            }

            let get_rect_success = unsafe {
                SendMessageW(
                    hwnd_treeview,
                    TVM_GETITEMRECT,
                    Some(WPARAM(1)), // TRUE for text-only part of the item
                    Some(LPARAM(&mut item_rect_text_part as *mut _ as isize)),
                )
            };

            if get_rect_success.0 != 0 {
                // Non-zero means success
                // Position circle slightly to the left of the text rectangle's left edge
                let circle_offset_x = -(CIRCLE_DIAMETER + 3); // Offset to the left of text, plus a small gap
                let x1 = item_rect_text_part.left + circle_offset_x;
                // Vertically center the circle with the text rectangle
                let y1 = item_rect_text_part.top
                    + (item_rect_text_part.bottom - item_rect_text_part.top - CIRCLE_DIAMETER) / 2;
                let x2 = x1 + CIRCLE_DIAMETER;
                let y2 = y1 + CIRCLE_DIAMETER;

                log::debug!(
                    "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): ITEMPOSTPAINT for AppTreeItemId {:?} (HTREEITEM {:?}). TextRect: {:?}. Drawing circle at ({},{},{},{})",
                    window_id,
                    control_id_of_treeview,
                    tree_item_id,
                    h_item_native,
                    item_rect_text_part,
                    x1,
                    y1,
                    x2,
                    y2
                );

                unsafe {
                    let h_brush = CreateSolidBrush(CIRCLE_COLOR_BLUE);
                    if !h_brush.is_invalid() {
                        let old_brush = SelectObject(hdc, HGDIOBJ(h_brush.0));
                        _ = Ellipse(hdc, x1, y1, x2, y2);
                        SelectObject(hdc, old_brush); // Restore original brush
                        _ = DeleteObject(HGDIOBJ(h_brush.0)); // Delete created brush
                    } else {
                        log::error!(
                            "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Failed to create brush for 'New' indicator. LastError: {:?}",
                            window_id,
                            control_id_of_treeview,
                            GetLastError()
                        );
                    }
                }
            } else {
                log::warn!(
                    // Changed from error to warn based on P3.2 (failure might be expected for non-visible)
                    "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): TVM_GETITEMRECT (text-only) FAILED for HTREEITEM {:?} (AppTreeItemId {:?}). GetLastError: {:?}. Indicator not drawn.",
                    window_id,
                    control_id_of_treeview,
                    h_item_native,
                    tree_item_id,
                    unsafe { GetLastError() }
                );
            }
            return LRESULT(CDRF_DODEFAULT as isize); // Finished custom drawing for this item
        }
        _ => {
            log::trace!(
                "TreeViewHandler NM_CUSTOMDRAW (WinID {:?}/CtrlID {}): Unhandled dwDrawStage: {:?}",
                window_id,
                control_id_of_treeview,
                nmtvcd.nmcd.dwDrawStage
            );
        }
    }
    LRESULT(CDRF_DODEFAULT as isize) // Default for unhandled stages
}

/*
 * Handles the custom WM_APP_TREEVIEW_CHECKBOX_CLICKED message.
 * This message is posted by the NM_CLICK handler when a click occurs on a TreeView
 * item's state icon (checkbox). This function retrieves the item's current checkbox
 * state and the application-specific TreeItemId, then constructs an
 * AppEvent::TreeViewItemToggledByUser to notify the application logic.
 */
pub(crate) fn handle_wm_app_treeview_checkbox_clicked(
    internal_state: &Arc<Win32ApiInternalState>,
    _parent_hwnd: HWND, // Unused for now, but was part of original signature
    window_id: WindowId,
    wparam_htreeitem: WPARAM,
    lparam_control_id: LPARAM,
) -> Option<AppEvent> {
    let h_item_val = wparam_htreeitem.0 as isize;
    let control_id_of_treeview = lparam_control_id.0 as i32;

    if h_item_val == 0 {
        log::warn!(
            "TreeViewHandler: WM_APP_TREEVIEW_CHECKBOX_CLICKED with null HTREEITEM in WPARAM for ControlID {}. Ignoring.",
            control_id_of_treeview
        );
        return None;
    }
    if control_id_of_treeview == 0 {
        log::warn!(
            "TreeViewHandler: WM_APP_TREEVIEW_CHECKBOX_CLICKED with null ControlID in LPARAM for HTREEITEM {:?}. Ignoring.",
            HTREEITEM(h_item_val)
        );
        return None;
    }

    let h_item_clicked = HTREEITEM(h_item_val);
    log::debug!(
        "TreeViewHandler: handle_wm_app_treeview_checkbox_clicked for WinID {:?}, ControlID {}, HTREEITEM {:?}",
        window_id,
        control_id_of_treeview,
        h_item_clicked
    );

    // Retrieve the HWND of the TreeView and its state
    let windows_guard = internal_state.active_windows.read().ok()?;
    let window_data = windows_guard.get(&window_id)?;
    let hwnd_treeview = window_data.get_control_hwnd(control_id_of_treeview)?;

    let tv_state = window_data.treeview_state.as_ref()?; // Ensure TreeView state exists

    // Get the item's current state (including checkbox state)
    let mut tv_item_get = TVITEMEXW {
        mask: TVIF_STATE | TVIF_PARAM, // Need state for checkbox and lParam for AppTreeItemId
        hItem: h_item_clicked,
        stateMask: TVIS_STATEIMAGEMASK.0,
        lParam: LPARAM(0), // To retrieve the app-specific ID
        ..Default::default()
    };

    let get_item_result = unsafe {
        SendMessageW(
            hwnd_treeview,
            TVM_GETITEMW,
            Some(WPARAM(0)), // Must be 0
            Some(LPARAM(&mut tv_item_get as *mut _ as isize)),
        )
    };

    if get_item_result.0 == 0 {
        // TVM_GETITEMW returns 0 on failure
        log::error!(
            "TreeViewHandler: TVM_GETITEMW FAILED for HTREEITEM {:?} in ControlID {}. Error: {:?}",
            h_item_clicked,
            control_id_of_treeview,
            unsafe { GetLastError() }
        );
        return None;
    }

    // State image index: 1 for unchecked, 2 for checked.
    let state_image_idx = (tv_item_get.state & TVIS_STATEIMAGEMASK.0) >> 12;
    let new_check_state = if state_image_idx == 2 {
        // Item IS now checked
        CheckState::Checked
    } else {
        // Item IS now unchecked (or indeterminate, which we map to unchecked)
        CheckState::Unchecked
    };

    // Retrieve the application-specific TreeItemId stored in lParam.
    // If lParam is 0 (e.g. not set during creation), try to map back from h_item_clicked.
    let app_item_id_from_lparam = tv_item_get.lParam.0 as u64;
    let app_item_id: TreeItemId;

    if app_item_id_from_lparam != 0 {
        app_item_id = TreeItemId(app_item_id_from_lparam);
    } else {
        // Fallback: try to find TreeItemId from htreeitem_to_item_id map
        if let Some(mapped_id) = tv_state.htreeitem_to_item_id.get(&(h_item_clicked.0)) {
            app_item_id = *mapped_id;
            log::warn!(
                "TreeViewHandler: AppTreeItemId was 0 from TVM_GETITEMW's lParam for HTREEITEM {:?}, but found {:?} via map.",
                h_item_clicked,
                app_item_id
            );
        } else {
            log::error!(
                "TreeViewHandler: AppTreeItemId is 0 from TVM_GETITEMW's lParam AND HTREEITEM {:?} not found in htreeitem_to_item_id map for ControlID {}. Cannot generate event.",
                h_item_clicked,
                control_id_of_treeview
            );
            return None;
        }
    }
    log::debug!(
        "TreeViewHandler: Checkbox toggle processed. AppTreeItemId: {:?}, New UI CheckState: {:?}, ControlID: {}",
        app_item_id,
        new_check_state,
        control_id_of_treeview
    );

    Some(AppEvent::TreeViewItemToggledByUser {
        window_id,
        item_id: app_item_id,
        new_state: new_check_state,
    })
}

/*
 * Handles general NM_CLICK notifications for a TreeView.
 * This function is called from window_common's WM_NOTIFY handler.
 * Its primary purpose is to detect clicks on a TreeView item's state icon (checkbox)
 * and, if detected, post a custom WM_APP_TREEVIEW_CHECKBOX_CLICKED message to the
 * parent window. This decouples the immediate click detection from the state update logic.
 */
pub(crate) fn handle_nm_click(
    _internal_state: &Arc<Win32ApiInternalState>,
    parent_hwnd: HWND,
    window_id: WindowId,
    nmhdr: &NMHDR,
) {
    let hwnd_tv_from_notify = nmhdr.hwndFrom; // HWND of the TreeView control
    if hwnd_tv_from_notify.is_invalid() {
        log::warn!("TreeViewHandler: NM_CLICK from invalid HWND. Ignoring.");
        return;
    }

    let control_id_from_notify = nmhdr.idFrom as i32;
    log::trace!(
        "TreeViewHandler: handle_nm_click for WinID {:?}, TreeView HWND {:?}, ControlID {}",
        window_id,
        hwnd_tv_from_notify,
        control_id_from_notify
    );

    // Get cursor position in client coordinates of the TreeView
    let mut screen_pt_of_click = POINT::default();
    if unsafe { GetCursorPos(&mut screen_pt_of_click) }.is_err() {
        log::warn!("TreeViewHandler: GetCursorPos failed in NM_CLICK. Cannot perform hit-test.");
        return;
    }
    let mut client_pt_for_hittest = screen_pt_of_click;
    if unsafe { ScreenToClient(hwnd_tv_from_notify, &mut client_pt_for_hittest) }.as_bool() == false
    {
        log::warn!(
            "TreeViewHandler: ScreenToClient failed in NM_CLICK. Cannot perform hit-test. Error: {:?}",
            unsafe { GetLastError() }
        );
        return;
    }

    // Perform hit-test
    let mut tvht_info = TVHITTESTINFO {
        pt: client_pt_for_hittest,
        flags: TVHITTESTINFO_FLAGS(0), // Will be filled by TVM_HITTEST
        hItem: HTREEITEM(0),           // Will be filled by TVM_HITTEST
    };

    let h_item_hit = HTREEITEM(
        unsafe {
            SendMessageW(
                hwnd_tv_from_notify,
                TVM_HITTEST,
                Some(WPARAM(0)), // Must be 0
                Some(LPARAM(&mut tvht_info as *mut _ as isize)),
            )
        }
        .0,
    );

    if h_item_hit.0 != 0 && (tvht_info.flags.0 & TVHT_ONITEMSTATEICON.0) != 0 {
        // Click was on a state icon (checkbox)
        log::debug!(
            "TreeViewHandler: NM_CLICK on state icon detected for HTREEITEM {:?} in ControlID {}. Posting WM_APP_TREEVIEW_CHECKBOX_CLICKED.",
            h_item_hit,
            control_id_from_notify
        );
        unsafe {
            // Post message to parent window for deferred processing
            if PostMessageW(
                Some(parent_hwnd), // Post to the main window that received WM_NOTIFY
                crate::platform_layer::window_common::WM_APP_TREEVIEW_CHECKBOX_CLICKED,
                WPARAM(h_item_hit.0 as usize), // Pass HTREEITEM in WPARAM
                LPARAM(control_id_from_notify as isize), // Pass ControlID in LPARAM
            )
            .is_err()
            {
                log::error!(
                    "TreeViewHandler: Failed to post WM_APP_TREEVIEW_CHECKBOX_CLICKED message: {:?}",
                    GetLastError()
                );
            }
        }
    } else {
        log::trace!(
            "TreeViewHandler: NM_CLICK was not on a state icon (flags: {:?}, hItem: {:?}).",
            tvht_info.flags,
            h_item_hit
        );
    }
}
