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
    AppEvent, CheckState, TreeItemDescriptor, TreeItemId, WindowId,
};

use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{HGDIOBJ, HFONT, InvalidateRect, ScreenToClient, SelectObject},
        UI::Controls::{
            CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT, CDRF_NEWFONT,
            CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTPAINT, HTREEITEM, NMHDR, NMTVCUSTOMDRAW,
            TVHITTESTINFO, TVHT_ONITEMSTATEICON, TVI_LAST, TVIF_CHILDREN, TVIF_PARAM, TVIF_STATE,
            TVIF_TEXT, TVINSERTSTRUCTW, TVINSERTSTRUCTW_0, TVIS_STATEIMAGEMASK, TVITEMEXW,
            TVITEMEXW_CHILDREN, TVM_DELETEITEM, TVM_GETITEMRECT, TVM_GETITEMW, TVM_HITTEST,
            TVM_INSERTITEMW, TVM_SETITEMW, TVS_CHECKBOXES, TVS_HASBUTTONS, TVS_HASLINES,
            TVS_LINESATROOT, TVS_SHOWSELALWAYS, WC_TREEVIEWW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::PWSTR,
};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

/*
 * --- DEACTIVATED ---
 * The values below supported the manual blue-dot drawing for "New" items. They
 * are preserved for future experimentation but are no longer part of the active
 * rendering path now that font styling drives the indicator.
 *
 * const CIRCLE_DIAMETER: i32 = 6;
 * const CIRCLE_COLOR_BLUE: windows::Win32::Foundation::COLORREF =
 *     windows::Win32::Foundation::COLORREF(0x00FF0000); // BGR format for Blue
 */

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
            mask: TVIF_TEXT | TVIF_PARAM | TVIF_CHILDREN,
            hItem: HTREEITEM::default(), // Will be filled by the system if successful
            pszText: PWSTR(text_buffer.as_mut_ptr()),
            cchTextMax: text_buffer.len() as i32,
            lParam: LPARAM(item_desc.id.0 as isize), // Store app-specific TreeItemId
            cChildren: TVITEMEXW_CHILDREN(if item_desc.is_folder { 1 } else { 0 }), // Hint if it has children
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

        // Explicitly set the state after insertion. This ensures the built-in
        // state image list for checkboxes is attached before we request a
        // particular check state.
        let mut tv_item_update = TVITEMEXW {
            mask: TVIF_STATE,
            hItem: h_current_item_native,
            state: (image_index as u32) << 12,
            stateMask: TVIS_STATEIMAGEMASK.0,
            ..Default::default()
        };

        unsafe {
            SendMessageW(
                hwnd_treeview,
                TVM_SETITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tv_item_update as *mut _ as isize)),
            );
        }

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
 * This function uses a read-create-write pattern to minimize lock contention.
 * It first checks for conflicts using a read lock, then creates the native
 * control, and finally uses a write lock to register the new control and its state.
 */
pub(crate) fn handle_create_treeview_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    parent_control_id: Option<i32>,
    control_id: i32,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: handle_create_treeview_command for WinID {:?}, ParentID {:?}, ControlID {}",
        window_id,
        parent_control_id,
        control_id
    );

    // Phase 1: Read-only pre-checks.
    let hwnd_parent_for_creation =
        internal_state.with_window_data_read(window_id, |window_data| {
            if window_data.has_control(control_id) || window_data.has_treeview_state() {
                return Err(PlatformError::ControlCreationFailed(format!(
                    "TreeView with ID {} or existing treeview state already present for window {:?}",
                    control_id, window_id
                )));
            }

            let hwnd = match parent_control_id {
                Some(id) => window_data.get_control_hwnd(id).ok_or_else(|| {
                    PlatformError::InvalidHandle(format!(
                        "Parent control with ID {} not found for CreateTreeView in WinID {:?}",
                        id, window_id
                    ))
                })?,
                None => window_data.get_hwnd(),
            };

            if hwnd.is_invalid() {
                return Err(PlatformError::InvalidHandle(format!(
                    "Parent HWND for CreateTreeView is invalid (WinID: {:?})",
                    window_id
                )));
            }
            Ok(hwnd)
        })?;

    // Phase 2: Create the window without holding a lock.
    let h_instance_for_creation = internal_state.h_instance();
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

    // Phase 3: Acquire write lock to update NativeWindowData.
    internal_state.with_window_data_write(window_id, |window_data| {
        // Re-check for race conditions.
        if window_data.has_control(control_id) || window_data.has_treeview_state() {
            log::warn!(
                "TreeViewHandler: TreeView (ID {}) created concurrently. Destroying new one.",
                control_id
            );
            unsafe { DestroyWindow(hwnd_tv).ok() };
            return Err(PlatformError::ControlCreationFailed(format!(
                "TreeView with ID {} was concurrently created for window {:?}",
                control_id, window_id
            )));
        }

        window_data.register_control_hwnd(control_id, hwnd_tv);
        window_data.init_treeview_state();
        log::debug!(
            "TreeViewHandler: Created TreeView (ID {}) for window {:?} with HWND {:?}",
            control_id,
            window_id,
            hwnd_tv
        );
        Ok(())
    })
}

/*
 * Populates a TreeView control with a given set of item descriptors.
 * This function clears any existing items in the TreeView and then recursively
 * adds the new items. It uses the specialized `with_treeview_state_mut` helper
 * to ensure the main window map is not locked during the potentially lengthy
 * population process.
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

    internal_state.with_treeview_state_mut(window_id, control_id, |hwnd_treeview, tv_state| {
        log::debug!(
            "TreeViewHandler: Populating TreeView (HWND {:?}). Clearing existing items.",
            hwnd_treeview
        );
        tv_state.clear_items_impl(hwnd_treeview);

        for item_desc in items {
            tv_state.add_item_recursive_impl(hwnd_treeview, HTREEITEM(0), &item_desc)?;
        }

        log::debug!(
            "TreeViewHandler: Finished populating TreeView (HWND {:?}).",
            hwnd_treeview
        );
        Ok(())
    })
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

    // Get all necessary handles and data within a single read lock.
    let (hwnd_treeview, h_item_native) =
        internal_state.with_window_data_read(window_id, |window_data| {
            let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {}",
                    control_id
                ))
            })?;

            let tv_state = window_data.get_treeview_state().ok_or_else(|| {
                PlatformError::OperationFailed(format!(
                    "No TreeView state exists in window {:?}",
                    window_id
                ))
            })?;

            let h_item = tv_state
                .item_id_to_htreeitem
                .get(&item_id)
                .copied()
                .ok_or_else(|| {
                    PlatformError::InvalidHandle(format!("TreeItemId {:?} not found", item_id))
                })?;

            Ok((hwnd, h_item))
        })?;

    if hwnd_treeview.is_invalid() {
        return Err(PlatformError::InvalidHandle("Invalid TreeView HWND".into()));
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
        let last_error = unsafe { GetLastError() };
        return Err(PlatformError::OperationFailed(format!(
            "TVM_SETITEMW failed for item {:?}: {:?}",
            item_id, last_error
        )));
    }
    Ok(())
}

/*
 * Updates the rendered text for a single TreeView item. This reuses the stored
 * `HTREEITEM` mapping to send a `TVM_SETITEMW` call with a new UTF-16 buffer,
 * allowing the application logic to append or remove the indicator glyph without
 * rebuilding the entire control.
 */
pub(crate) fn update_treeview_item_text(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    item_id: TreeItemId,
    text: String,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: update_treeview_item_text for WinID {:?}, ControlID {}, ItemID {:?}",
        window_id,
        control_id,
        item_id
    );

    let (hwnd_treeview, h_item_native) =
        internal_state.with_window_data_read(window_id, |window_data| {
            let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView HWND not found for ControlID {}",
                    control_id
                ))
            })?;

            let tv_state = window_data.get_treeview_state().ok_or_else(|| {
                PlatformError::OperationFailed(format!(
                    "No TreeView state exists in window {:?}",
                    window_id
                ))
            })?;

            let h_item = tv_state
                .item_id_to_htreeitem
                .get(&item_id)
                .copied()
                .ok_or_else(|| {
                    PlatformError::InvalidHandle(format!("TreeItemId {:?} not found", item_id))
                })?;

            Ok((hwnd, h_item))
        })?;

    if hwnd_treeview.is_invalid() {
        return Err(PlatformError::InvalidHandle("Invalid TreeView HWND".into()));
    }

    let mut text_buffer: Vec<u16> = text.encode_utf16().collect();
    text_buffer.push(0);

    let mut tv_item_update = TVITEMEXW {
        mask: TVIF_TEXT,
        hItem: h_item_native,
        pszText: PWSTR(text_buffer.as_mut_ptr()),
        cchTextMax: text_buffer.len() as i32,
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
        let last_error = unsafe { GetLastError() };
        return Err(PlatformError::OperationFailed(format!(
            "TVM_SETITEMW (text update) failed for item {:?}: {:?}",
            item_id, last_error
        )));
    }
    Ok(())
}

/*
 * Handles the TVN_ITEMCHANGEDW notification for a TreeView.
 * This notification is sent for various item state changes, but this handler
 * currently only logs the event.
 */
pub(crate) fn handle_treeview_itemchanged_notification(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    _lparam: LPARAM,
    control_id_from_notify: i32,
) -> Option<AppEvent> {
    log::trace!(
        "TreeViewHandler: TVN_ITEMCHANGEDW received for WinID {:?}, ControlID {}",
        window_id,
        control_id_from_notify,
    );
    // Check if TreeView state exists for this window.
    if let Ok(false) =
        internal_state.with_window_data_read(window_id, |wd| Ok(wd.has_treeview_state()))
    {
        log::warn!("Received TVN_ITEMCHANGEDW for a window without treeview state.");
        return None;
    }

    // `lparam` could be used here to get more details if needed.
    // let nmtv = unsafe { &*(lparam.0 as *const NMTREEVIEWW) };
    None // No AppEvent generated from this notification directly for now
}

/*
 * Executes the `RedrawTreeItem` command by invalidating the rectangle of a specific item.
 * This function retrieves the native `HTREEITEM` for the given `TreeItemId` and
 * uses `TVM_GETITEMRECT` to find its bounding box, then forces a repaint.
 */
pub(crate) fn handle_redraw_tree_item_command(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
    item_id: TreeItemId,
) -> PlatformResult<()> {
    log::debug!(
        "TreeViewHandler: handle_redraw_tree_item_command for WinID {:?}, ItemID {:?}",
        window_id,
        item_id
    );

    let (hwnd_treeview, htreeitem) =
        internal_state.with_window_data_read(window_id, |window_data| {
            let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
                PlatformError::InvalidHandle(format!(
                    "TreeView control (ID {}) not found",
                    control_id
                ))
            })?;

            let tv_state = window_data.get_treeview_state().ok_or_else(|| {
                PlatformError::OperationFailed(format!(
                    "TreeView state not found for WinID {:?}",
                    window_id
                ))
            })?;

            let h_item = tv_state
                .item_id_to_htreeitem
                .get(&item_id)
                .copied()
                .ok_or_else(|| {
                    PlatformError::InvalidHandle(format!(
                        "HTREEITEM not found for ItemID {:?}",
                        item_id
                    ))
                })?;
            Ok((hwnd, h_item))
        })?;

    if hwnd_treeview.is_invalid() {
        return Err(PlatformError::InvalidHandle("Invalid TreeView HWND".into()));
    }

    let mut item_rect = RECT::default();
    unsafe {
        *((&mut item_rect as *mut RECT) as *mut HTREEITEM) = htreeitem;
    }

    let get_rect_success = unsafe {
        SendMessageW(
            hwnd_treeview,
            TVM_GETITEMRECT,
            Some(WPARAM(0)), // FALSE for whole item
            Some(LPARAM(&mut item_rect as *mut _ as isize)),
        )
    };

    if get_rect_success.0 != 0 {
        unsafe {
            _ = InvalidateRect(Some(hwnd_treeview), Some(&item_rect), true);
        }
    } else {
        log::warn!(
            "TVM_GETITEMRECT failed for item ID {:?}, invalidating whole control. Error: {:?}",
            item_id,
            unsafe { GetLastError() }
        );
        unsafe {
            _ = InvalidateRect(Some(hwnd_treeview), None, true);
        }
    }
    Ok(())
}

fn get_treeview_hwnd(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    control_id: i32,
) -> PlatformResult<HWND> {
    internal_state.with_window_data_read(window_id, |window_data| {
        let hwnd = window_data.get_control_hwnd(control_id).ok_or_else(|| {
            PlatformError::InvalidHandle(format!(
                "Control ID {} not found in WinID {:?}",
                control_id, window_id
            ))
        })?;

        if hwnd.is_invalid() {
            return Err(PlatformError::InvalidHandle(format!(
                "HWND for control ID {} is invalid",
                control_id
            )));
        }
        Ok(hwnd)
    })
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
    let hwnd_treeview = get_treeview_hwnd(internal_state, window_id, control_id)?;

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
    let hwnd_treeview = get_treeview_hwnd(internal_state, window_id, control_id)?;

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
 * Determines whether a given TreeView item should display the "new item" styling.
 * Relies on the application logic's `UiStateProvider` to keep the decision centralized.
 */
fn is_item_new_for_display(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    tree_item_id: TreeItemId,
) -> bool {
    let provider_opt = internal_state
        .ui_state_provider()
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|weak_handler| weak_handler.upgrade());

    if let Some(handler_arc) = provider_opt {
        if let Ok(handler_guard) = handler_arc.lock() {
            return handler_guard.is_tree_item_new(window_id, tree_item_id);
        }
    }

    false
}

/*
 * Handles the NM_CUSTOMDRAW notification for a TreeView control.
 * Applies a bold/italic font to "New" items via NM_CUSTOMDRAW, replacing the former
 * hand-drawn blue circle indicator. The old drawing logic is preserved in comments
 * for potential future reference.
 */
pub(crate) fn handle_nm_customdraw(
    internal_state: &Arc<Win32ApiInternalState>,
    window_id: WindowId,
    lparam_nmcustomdraw: LPARAM, // This is NMTVCUSTOMDRAW*
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
            let tree_item_id = TreeItemId(nmtvcd.nmcd.lItemlParam.0 as u64);
            if !is_item_new_for_display(internal_state, window_id, tree_item_id) {
                return LRESULT(CDRF_DODEFAULT as isize);
            }

            let mut indicator_font: Option<HFONT> = internal_state
                .with_window_data_read(window_id, |window_data| {
                    Ok(window_data.get_treeview_new_item_font())
                })
                .unwrap_or(None);

            if indicator_font.is_none() {
                if let Ok(font_opt) =
                    internal_state.with_window_data_write(window_id, |window_data| {
                        window_data.ensure_treeview_new_item_font();
                        Ok(window_data.get_treeview_new_item_font())
                    })
                {
                    indicator_font = font_opt;
                }
            }

            if let Some(font_handle) = indicator_font {
                unsafe {
                    SelectObject(nmtvcd.nmcd.hdc, HGDIOBJ(font_handle.0));
                }
            }

            return LRESULT((CDRF_NOTIFYPOSTPAINT | CDRF_NEWFONT) as isize);
        }
        CDDS_ITEMPOSTPAINT => {
            let hdc = nmtvcd.nmcd.hdc;
            let hwnd_treeview = nmtvcd.nmcd.hdr.hwndFrom;
            let tree_item_id = TreeItemId(nmtvcd.nmcd.lItemlParam.0 as u64);

            if is_item_new_for_display(internal_state, window_id, tree_item_id) {
                /*
                 * --- DEACTIVATED ---
                 * Historical manual drawing code retained for reference:
                 *
                 * let h_item_native = HTREEITEM(nmtvcd.nmcd.dwItemSpec as isize);
                 * let mut item_rect_text_part = RECT::default();
                 * unsafe {
                 *     *((&mut item_rect_text_part as *mut RECT) as *mut HTREEITEM) = h_item_native;
                 * }
                 * let get_rect_success = unsafe {
                 *     SendMessageW(
                 *         hwnd_treeview,
                 *         TVM_GETITEMRECT,
                 *         Some(WPARAM(1)),
                 *         Some(LPARAM(&mut item_rect_text_part as *mut _ as isize)),
                 *     )
                 * };
                 * if get_rect_success.0 != 0 {
                 *     // ellipse drawing omitted
                 * }
                */
                let default_font_lresult = unsafe {
                    SendMessageW(hwnd_treeview, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0)))
                };
                let default_font = HFONT(default_font_lresult.0 as usize as *mut c_void);
                if !default_font.0.is_null() {
                    unsafe {
                        SelectObject(hdc, HGDIOBJ(default_font.0));
                    }
                }
            }
            return LRESULT(CDRF_DODEFAULT as isize);
        }
        _ => {}
    }
    LRESULT(CDRF_DODEFAULT as isize)
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
    _parent_hwnd: HWND,
    window_id: WindowId,
    wparam_htreeitem: WPARAM,
    lparam_control_id: LPARAM,
) -> Option<AppEvent> {
    let h_item_clicked = HTREEITEM(wparam_htreeitem.0 as isize);
    let control_id_of_treeview = lparam_control_id.0 as i32;

    if h_item_clicked.0 == 0 || control_id_of_treeview == 0 {
        return None;
    }

    let result = internal_state.with_window_data_read(window_id, |window_data| {
        let hwnd_treeview = window_data
            .get_control_hwnd(control_id_of_treeview)
            .ok_or_else(|| PlatformError::InvalidHandle("Control not found".into()))?;
        let tv_state = window_data
            .get_treeview_state()
            .ok_or_else(|| PlatformError::OperationFailed("TreeView state not found".into()))?;

        let mut tv_item_get = TVITEMEXW {
            mask: TVIF_STATE | TVIF_PARAM,
            hItem: h_item_clicked,
            stateMask: TVIS_STATEIMAGEMASK.0,
            ..Default::default()
        };

        if unsafe {
            SendMessageW(
                hwnd_treeview,
                TVM_GETITEMW,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tv_item_get as *mut _ as isize)),
            )
        }
        .0 == 0
        {
            return Err(PlatformError::OperationFailed("TVM_GETITEMW failed".into()));
        }

        let state_image_idx = (tv_item_get.state & TVIS_STATEIMAGEMASK.0) >> 12;
        let new_check_state = if state_image_idx == 2 {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        };

        let app_item_id = if tv_item_get.lParam.0 != 0 {
            TreeItemId(tv_item_get.lParam.0 as u64)
        } else {
            // Fallback to map lookup
            tv_state
                .htreeitem_to_item_id
                .get(&(h_item_clicked.0))
                .copied()
                .ok_or_else(|| PlatformError::InvalidHandle("HTREEITEM not found in map".into()))?
        };

        Ok(AppEvent::TreeViewItemToggledByUser {
            window_id,
            item_id: app_item_id,
            new_state: new_check_state,
        })
    });

    match result {
        Ok(event) => Some(event),
        Err(e) => {
            log::error!(
                "Failed to handle checkbox click for HTREEITEM {:?}: {:?}",
                h_item_clicked,
                e
            );
            None
        }
    }
}

/*
 * Handles general NM_CLICK notifications for a TreeView.
 * This function's primary purpose is to detect clicks on a TreeView item's state
 * icon (checkbox) and post a custom message for deferred processing.
 */
pub(crate) fn handle_nm_click(
    _internal_state: &Arc<Win32ApiInternalState>,
    parent_hwnd: HWND,
    _window_id: WindowId,
    nmhdr: &NMHDR,
) {
    let hwnd_tv_from_notify = nmhdr.hwndFrom;
    if hwnd_tv_from_notify.is_invalid() {
        return;
    }

    let control_id_from_notify = nmhdr.idFrom as i32;

    let mut screen_pt_of_click = POINT::default();
    if unsafe { GetCursorPos(&mut screen_pt_of_click) }.is_err() {
        return;
    }
    let mut client_pt_for_hittest = screen_pt_of_click;
    if unsafe { !ScreenToClient(hwnd_tv_from_notify, &mut client_pt_for_hittest) }.as_bool() {
        return;
    }

    let mut tvht_info = TVHITTESTINFO {
        pt: client_pt_for_hittest,
        ..Default::default()
    };

    let h_item_hit = HTREEITEM(
        unsafe {
            SendMessageW(
                hwnd_tv_from_notify,
                TVM_HITTEST,
                Some(WPARAM(0)),
                Some(LPARAM(&mut tvht_info as *mut _ as isize)),
            )
        }
        .0,
    );

    if h_item_hit.0 != 0 && (tvht_info.flags.0 & TVHT_ONITEMSTATEICON.0) != 0 {
        log::debug!(
            "NM_CLICK on state icon detected for HTREEITEM {:?}. Posting deferred message.",
            h_item_hit,
        );
        unsafe {
            if PostMessageW(
                Some(parent_hwnd),
                crate::platform_layer::window_common::WM_APP_TREEVIEW_CHECKBOX_CLICKED,
                WPARAM(h_item_hit.0 as usize),
                LPARAM(control_id_from_notify as isize),
            )
            .is_err()
            {
                log::error!(
                    "Failed to post WM_APP_TREEVIEW_CHECKBOX_CLICKED message: {:?}",
                    GetLastError()
                );
            }
        }
    }
}
