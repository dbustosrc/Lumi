use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IBindCtx, IDataObject,
};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, IDropSource};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FileOpenDialog,
    IFileOpenDialog, ILCreateFromPathW, ILFree, IShellItemArray, SHCreateShellItemArrayFromIDLists,
    SHDoDragDrop, SIGDN_FILESYSPATH, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, PostMessageW, SW_SHOWNORMAL, WM_MOUSEMOVE,
};
use windows::core::{HRESULT, PCWSTR, PWSTR, w};

use crate::domain::ports::PortError;

/// Suppresses Winit's `IDropTarget` events on `NotchWindow` while Lumi is
/// itself the drag source, so dragging out never triggers self-drop overlays.
static IS_SELF_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn is_outbound_drag_active() -> bool {
    IS_SELF_DRAG_ACTIVE.load(Ordering::Acquire)
}

struct SelfDragGuard;

impl SelfDragGuard {
    fn activate() -> Self {
        IS_SELF_DRAG_ACTIVE.store(true, Ordering::Release);
        Self
    }
}

impl Drop for SelfDragGuard {
    fn drop(&mut self) {
        IS_SELF_DRAG_ACTIVE.store(false, Ordering::Release);
    }
}

struct OwnedPidlList(Vec<*mut ITEMIDLIST>);

impl OwnedPidlList {
    fn from_paths(paths: &[PathBuf]) -> Result<Self, PortError> {
        let mut list = Vec::with_capacity(paths.len());
        for path in paths {
            let wide = encode_wide(path);
            // SAFETY: `wide` is null-terminated UTF-16; returned PIDL is freed in `Drop`.
            let pidl = unsafe { ILCreateFromPathW(PCWSTR(wide.as_ptr())) };
            if !pidl.is_null() {
                list.push(pidl);
            }
        }
        if list.is_empty() {
            return Err(PortError::new(
                "shelf.error.inspectFailed",
                "no valid shell PIDL",
            ));
        }
        Ok(Self(list))
    }

    fn as_const_ptrs(&self) -> Vec<*const ITEMIDLIST> {
        self.0.iter().map(|ptr| *ptr as *const ITEMIDLIST).collect()
    }
}

impl Drop for OwnedPidlList {
    fn drop(&mut self) {
        for pidl in self.0.drain(..) {
            // SAFETY: Each `pidl` was allocated by `ILCreateFromPathW`.
            unsafe { ILFree(Some(pidl as *const ITEMIDLIST)) };
        }
    }
}

/// Runs a native Windows Shell drag-and-drop loop (`SHDoDragDrop`) with Windows 11
/// drag visuals so shelf items drop cleanly into Explorer, browsers and chat apps.
pub fn start_outbound_file_drag(
    hwnd_raw: Option<isize>,
    paths: &[PathBuf],
) -> Result<(), PortError> {
    let _guard = SelfDragGuard::activate();
    let pidls = OwnedPidlList::from_paths(paths)?;
    let const_pidls = pidls.as_const_ptrs();
    let owner = hwnd_raw.map(|raw| HWND(raw as *mut _));

    // SAFETY: Runs on the STA UI thread initialized with OLE by Winit; `const_pidls`
    // remain valid across `SHCreateShellItemArrayFromIDLists`.
    unsafe {
        let array = SHCreateShellItemArrayFromIDLists(&const_pidls)
            .map_err(|err| PortError::new("shelf.error.openFailed", err))?;
        let data_obj: IDataObject = array
            .BindToHandler(None::<&IBindCtx>, &BHID_DataObject)
            .map_err(|err| PortError::new("shelf.error.openFailed", err))?;
        let effects = DROPEFFECT_COPY | DROPEFFECT_LINK;
        let _ = SHDoDragDrop(owner, &data_obj, None::<&IDropSource>, effects);
    }
    Ok(())
}

pub fn start_outbound_text_drag(hwnd_raw: Option<isize>, text: &str) -> Result<(), PortError> {
    let _guard = SelfDragGuard::activate();
    let data = super::shelf_text::text_data_object(text)
        .map_err(|error| PortError::new("shelf.error.clipboard", error))?;
    unsafe {
        let _ = SHDoDragDrop(
            hwnd_raw.map(|raw| HWND(raw as *mut _)),
            &data,
            None::<&IDropSource>,
            DROPEFFECT_COPY,
        );
    }
    Ok(())
}

/// Checks whether the cursor remained inside the overlay window after an OLE drag
/// and re-posts `WM_MOUSEMOVE` when inside so Slint hover state stays accurate.
pub fn sync_cursor_after_drag(hwnd_raw: Option<isize>) -> bool {
    let Some(raw) = hwnd_raw else { return false };
    let handle = HWND(raw as *mut _);
    let mut cursor = POINT::default();
    let Some(rect) = super::window_region::overlay_region_bounds(raw) else {
        return false;
    };
    // SAFETY: `handle` is the live `NotchWindow` Win32 handle on the UI thread.
    let is_inside = unsafe {
        GetCursorPos(&mut cursor).is_ok()
            && cursor.x >= rect.left
            && cursor.x < rect.right
            && cursor.y >= rect.top
            && cursor.y < rect.bottom
    };
    if is_inside {
        post_client_mousemove(handle, cursor);
    }
    is_inside
}

fn post_client_mousemove(handle: HWND, mut client_pt: POINT) {
    // SAFETY: `ScreenToClient` and `PostMessageW` operate on a valid UI-thread `HWND`.
    unsafe {
        if ScreenToClient(handle, &mut client_pt).as_bool() {
            let packed = ((client_pt.y & 0xffff) << 16) | (client_pt.x & 0xffff);
            let _ = PostMessageW(
                Some(handle),
                WM_MOUSEMOVE,
                WPARAM(0),
                LPARAM(packed as isize),
            );
        }
    }
}

/// Opens a file or folder with its default Windows handler (`ShellExecuteW`).
pub fn open_shell_path(path: &Path) -> Result<(), PortError> {
    let wide = encode_wide(path);
    // SAFETY: `wide` is null-terminated UTF-16; `ShellExecuteW` returns `> 32` on success.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        return Err(PortError::new(
            "shelf.error.openFailed",
            format!("ShellExecuteW={code}"),
        ));
    }
    Ok(())
}

struct StaComGuard(bool);

impl StaComGuard {
    fn init() -> Result<Self, PortError> {
        // SAFETY: Called on a dedicated short-lived thread for `IFileOpenDialog`.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_err() {
            return Err(PortError::new("shelf.error.pickerFailed", hr.message()));
        }
        Ok(Self(true))
    }
}

impl Drop for StaComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Balances `CoInitializeEx` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// Opens the native Windows 11 multi-select file dialog on a dedicated STA thread.
pub fn pick_files_dialog() -> Result<Vec<PathBuf>, PortError> {
    if super::fullscreen::foreground_is_fullscreen() {
        return Err(PortError::new(
            "shelf.error.fullscreenProtected",
            "cannot activate a file dialog over fullscreen",
        ));
    }
    let _com = StaComGuard::init()?;
    let cancel_hr = HRESULT::from_win32(ERROR_CANCELLED.0);
    // SAFETY: STA COM is initialized on this dedicated thread.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
        let opts = dialog
            .GetOptions()
            .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
        let flags = opts | FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM;
        dialog
            .SetOptions(flags)
            .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
        match dialog.Show(None) {
            Ok(()) => {}
            Err(err) if err.code() == cancel_hr => return Ok(Vec::new()),
            Err(err) => return Err(PortError::new("shelf.error.pickerFailed", err)),
        }
        let results = dialog
            .GetResults()
            .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
        collect_shell_item_paths(&results)
    }
}

unsafe fn collect_shell_item_paths(results: &IShellItemArray) -> Result<Vec<PathBuf>, PortError> {
    let count = unsafe { results.GetCount() }
        .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        if let Ok(item) = unsafe { results.GetItemAt(index) }
            && let Ok(pwstr) = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
        {
            let text = unsafe { take_com_pwstr(pwstr) };
            if !text.is_empty() {
                paths.push(PathBuf::from(text));
            }
        }
    }
    Ok(paths)
}

unsafe fn take_com_pwstr(pwstr: PWSTR) -> String {
    if pwstr.is_null() {
        return String::new();
    }
    let text = unsafe { pwstr.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(pwstr.0 as *const _)) };
    text
}

fn encode_wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
