use crate::application::events::{AppEvent, EventSender};
use crate::domain::ports::PortError;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{GlobalFree, HWND, POINTL};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{
    CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget,
    IDropTarget_Impl, OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard,
    OleUninitialize, RegisterDragDrop, ReleaseStgMedium,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP, SHCreateDataObject};
use windows::core::{Ref, implement};

fn format(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    }
}
struct Medium(STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

pub fn text_data_object(text: &str) -> windows::core::Result<IDataObject> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let object: IDataObject = SHCreateDataObject(None, None, None::<&IDataObject>)?;
        let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
        let pointer = GlobalLock(memory).cast::<u16>();
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(windows::core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
        let _ = GlobalUnlock(memory);
        let mut medium = Medium(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: memory },
            ..Default::default()
        });
        object.SetData(&format(CF_UNICODETEXT.0), &medium.0, true)?;
        // Successful SetData transfers ownership of HGLOBAL to the Shell data object.
        medium.0 = STGMEDIUM::default();
        Ok(object)
    }
}

fn read_text(object: &IDataObject) -> windows::core::Result<String> {
    unsafe {
        let medium = Medium(object.GetData(&format(CF_UNICODETEXT.0))?);
        if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        let memory = medium.0.u.hGlobal;
        let size = GlobalSize(memory) / 2;
        let pointer = GlobalLock(memory).cast::<u16>();
        if pointer.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        let wide = std::slice::from_raw_parts(pointer, size);
        let end = wide.iter().position(|value| *value == 0).unwrap_or(size);
        let text = String::from_utf16(&wide[..end]);
        let _ = GlobalUnlock(memory);
        text.map_err(|_| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)
        })
    }
}

pub fn clipboard_text() -> Result<String, PortError> {
    unsafe { OleGetClipboard().and_then(|object| read_text(&object)) }
        .map_err(|error| PortError::new("shelf.error.clipboard", error))
}

pub fn copy_text(text: &str) -> Result<(), PortError> {
    let text = text.to_owned();
    std::thread::spawn(move || unsafe {
        OleInitialize(None)?;
        let result = text_data_object(&text).and_then(|object| {
            OleSetClipboard(&object)?;
            OleFlushClipboard()
        });
        OleUninitialize();
        result
    })
    .join()
    .map_err(|_| PortError::new("shelf.error.clipboard", "clipboard thread failed"))?
    .map_err(|error| PortError::new("shelf.error.clipboard", error))
}

#[implement(IDropTarget)]
struct ShelfDropTarget {
    events: EventSender,
    accepts: AtomicBool,
}

impl IDropTarget_Impl for ShelfDropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let accepts = !super::shelf_drag::is_outbound_drag_active()
            && data.as_ref().is_some_and(|data| unsafe {
                data.QueryGetData(&format(CF_HDROP.0)).is_ok()
                    || data.QueryGetData(&format(CF_UNICODETEXT.0)).is_ok()
            });
        self.accepts.store(accepts, Ordering::Relaxed);
        unsafe {
            *effect = if accepts {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        let _ = self.events.send(AppEvent::FileDragHover(accepts));
        Ok(())
    }
    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = if self.accepts.load(Ordering::Relaxed) {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        Ok(())
    }
    fn DragLeave(&self) -> windows::core::Result<()> {
        self.accepts.store(false, Ordering::Relaxed);
        let _ = self.events.send(AppEvent::FileDragHover(false));
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = DROPEFFECT_NONE;
        }
        let accepts = self.accepts.swap(false, Ordering::Relaxed);
        let _ = self.events.send(AppEvent::FileDragHover(false));
        if !accepts {
            return Ok(());
        }
        let data = data.ok()?;
        unsafe {
            if data.QueryGetData(&format(CF_HDROP.0)).is_ok() {
                let medium = Medium(data.GetData(&format(CF_HDROP.0))?);
                let drop = HDROP(medium.0.u.hGlobal.0);
                let count = DragQueryFileW(drop, u32::MAX, None);
                let mut paths = Vec::new();
                for index in 0..count {
                    let length = DragQueryFileW(drop, index, None) as usize;
                    let mut path = vec![0; length + 1];
                    DragQueryFileW(drop, index, Some(&mut path));
                    use std::os::windows::ffi::OsStringExt;
                    paths.push(std::path::PathBuf::from(std::ffi::OsString::from_wide(
                        &path[..length],
                    )));
                }
                let _ = self.events.send(AppEvent::FilesDropped(paths));
            } else {
                let _ = self
                    .events
                    .send(AppEvent::ShelfTextDropped(read_text(data)?));
            }
            *effect = DROPEFFECT_COPY;
        }
        Ok(())
    }
}

pub fn install_drop_target(hwnd: isize, events: EventSender) -> Result<(), PortError> {
    let handle = HWND(hwnd as *mut _);
    let target: IDropTarget = ShelfDropTarget {
        events,
        accepts: AtomicBool::new(false),
    }
    .into();
    unsafe {
        RegisterDragDrop(handle, &target)
            .map_err(|error| PortError::new("shelf.error.dropTarget", error))
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/shelf_text.rs"]
mod tests;
