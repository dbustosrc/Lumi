use crate::application::events::{AppEvent, DropHover, EventSender, FileDropDestination};
use crate::domain::ports::PortError;
use std::cell::Cell;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;
use windows::Win32::Foundation::{GlobalFree, HWND, POINT, POINTL, RECT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
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
    hwnd: HWND,
    file_zones: FileDropZones,
    hover: Cell<Option<DropHover>>,
}

pub type FileDropZones = Rc<Cell<[Option<(FileDropDestination, RECT)>; 2]>>;

impl ShelfDropTarget_Impl {
    fn destination_at(&self, point: &POINTL) -> Option<FileDropDestination> {
        let mut point = POINT {
            x: point.x,
            y: point.y,
        };
        if !unsafe { ScreenToClient(self.hwnd, &mut point) }.as_bool() {
            return None;
        }
        self.file_zones
            .get()
            .into_iter()
            .flatten()
            .find_map(|(destination, rect)| {
                (point.x >= rect.left
                    && point.x < rect.right
                    && point.y >= rect.top
                    && point.y < rect.bottom)
                    .then_some(destination)
            })
    }

    fn update_hover(&self, hover: Option<DropHover>) {
        if self.hover.replace(hover) != hover {
            let _ = self.events.send(AppEvent::FileDragHover(hover));
        }
    }
}

fn read_file_paths(data: &IDataObject) -> windows::core::Result<Vec<PathBuf>> {
    unsafe {
        let medium = Medium(data.GetData(&format(CF_HDROP.0))?);
        if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        let drop = HDROP(medium.0.u.hGlobal.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        let mut paths = Vec::new();
        for index in 0..count {
            let length = DragQueryFileW(drop, index, None) as usize;
            let mut path = vec![0; length + 1];
            let read = DragQueryFileW(drop, index, Some(&mut path)) as usize;
            if read > 0 {
                paths.push(PathBuf::from(std::ffi::OsString::from_wide(&path[..read])));
            }
        }
        Ok(paths)
    }
}

impl IDropTarget_Impl for ShelfDropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let hover =
            if !allows_copy(unsafe { *effect }) || super::shelf_drag::is_outbound_drag_active() {
                None
            } else {
                data.as_ref().and_then(|data| unsafe {
                    if data.QueryGetData(&format(CF_HDROP.0)).is_ok() {
                        Some(DropHover::Files(self.destination_at(point)))
                    } else if data.QueryGetData(&format(CF_UNICODETEXT.0)).is_ok() {
                        Some(DropHover::Text)
                    } else {
                        None
                    }
                })
            };
        self.update_hover(hover);
        unsafe {
            *effect = drop_effect(hover);
        }
        Ok(())
    }
    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let hover = self.hover.get().map(|hover| match hover {
            DropHover::Files(_) if allows_copy(unsafe { *effect }) => {
                DropHover::Files(self.destination_at(point))
            }
            DropHover::Files(_) => DropHover::Files(None),
            DropHover::Text => DropHover::Text,
        });
        self.update_hover(hover);
        unsafe {
            *effect = if allows_copy(*effect) {
                drop_effect(hover)
            } else {
                DROPEFFECT_NONE
            };
        }
        Ok(())
    }
    fn DragLeave(&self) -> windows::core::Result<()> {
        self.update_hover(None);
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let copy_allowed = allows_copy(unsafe { *effect });
        unsafe {
            *effect = DROPEFFECT_NONE;
        }
        let Some(hover) = self.hover.take() else {
            return Ok(());
        };
        let event =
            match (copy_allowed, hover) {
                (false, _) => Ok(None),
                (true, DropHover::Files(_)) => {
                    self.destination_at(point).map_or(Ok(None), |destination| {
                        let paths = read_file_paths(data.ok()?)?;
                        Ok((!paths.is_empty())
                            .then_some(AppEvent::FilesDropped { paths, destination }))
                    })
                }
                (true, DropHover::Text) => data
                    .ok()
                    .and_then(read_text)
                    .map(|text| Some(AppEvent::ShelfTextDropped(text))),
            };
        match event {
            Ok(Some(event)) => {
                if self.events.send(event).is_ok() {
                    unsafe {
                        *effect = DROPEFFECT_COPY;
                    }
                }
                Ok(())
            }
            other => {
                let _ = self.events.send(AppEvent::FileDragHover(None));
                other.map(|_| ())
            }
        }
    }
}

fn drop_effect(hover: Option<DropHover>) -> DROPEFFECT {
    if matches!(hover, Some(DropHover::Files(Some(_)) | DropHover::Text)) {
        DROPEFFECT_COPY
    } else {
        DROPEFFECT_NONE
    }
}

fn allows_copy(effect: DROPEFFECT) -> bool {
    effect.0 & DROPEFFECT_COPY.0 != 0
}

pub fn install_drop_target(
    hwnd: HWND,
    events: EventSender,
    file_zones: FileDropZones,
) -> Result<(), PortError> {
    let target: IDropTarget = ShelfDropTarget {
        hwnd,
        events,
        file_zones,
        hover: Cell::default(),
    }
    .into();
    unsafe {
        RegisterDragDrop(hwnd, &target)
            .map_err(|error| PortError::new("shelf.error.dropTarget", error))
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/shelf_text.rs"]
mod tests;
