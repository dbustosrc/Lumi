use crate::application::events::{AppEvent, DropHover, EventSender, FileDropDestination};
use crate::application::settings::AppSettings;
use crate::domain::notch::NotchPage;
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

pub fn clipboard_event(destination: FileDropDestination) -> Result<Option<AppEvent>, PortError> {
    unsafe { OleGetClipboard().and_then(|object| content_event(&object, destination)) }
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
    settings: tokio::sync::watch::Receiver<AppSettings>,
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

fn with_file_drop<T>(
    data: &IDataObject,
    read: impl FnOnce(HDROP) -> T,
) -> windows::core::Result<T> {
    unsafe {
        let medium = Medium(data.GetData(&format(CF_HDROP.0))?);
        if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        Ok(read(HDROP(medium.0.u.hGlobal.0)))
    }
}

fn read_file_paths(data: &IDataObject) -> windows::core::Result<Vec<PathBuf>> {
    with_file_drop(data, |drop| unsafe {
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
        paths
    })
}

fn content_hover(
    data: &IDataObject,
    destination: Option<FileDropDestination>,
) -> Option<DropHover> {
    let (files, text) = unsafe {
        (
            data.QueryGetData(&format(CF_HDROP.0)).is_ok(),
            data.QueryGetData(&format(CF_UNICODETEXT.0)).is_ok(),
        )
    };
    // Shell data objects can advertise CF_HDROP without containing files.
    if files
        && (!text
            || with_file_drop(
                data,
                |drop| unsafe { DragQueryFileW(drop, u32::MAX, None) } > 0,
            )
            .unwrap_or(false))
    {
        Some(DropHover::Files(destination))
    } else if text {
        Some(DropHover::Text(destination))
    } else {
        None
    }
}

fn event_for_hover(
    data: &IDataObject,
    hover: DropHover,
) -> windows::core::Result<Option<AppEvent>> {
    match hover {
        DropHover::Files(Some(destination)) => {
            let paths = read_file_paths(data)?;
            Ok((!paths.is_empty()).then_some(AppEvent::FilesDropped { paths, destination }))
        }
        DropHover::Text(Some(destination)) => {
            let text = read_text(data)?;
            Ok((!text.trim().is_empty()).then_some(AppEvent::TextDropped { text, destination }))
        }
        _ => Ok(None),
    }
}

fn content_event(
    data: &IDataObject,
    destination: FileDropDestination,
) -> windows::core::Result<Option<AppEvent>> {
    content_hover(data, Some(destination)).map_or(Ok(None), |hover| event_for_hover(data, hover))
}

impl IDropTarget_Impl for ShelfDropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let (shelf, share) = {
            let settings = self.settings.borrow();
            let pages = &settings.desktop.pages;
            (
                pages.contains(NotchPage::Shelf),
                pages.contains(NotchPage::QuickShare),
            )
        };
        let hover = if !(shelf || share)
            || !allows_copy(unsafe { *effect })
            || super::shelf_drag::is_outbound_drag_active()
        {
            None
        } else {
            data.as_ref()
                .and_then(|data| content_hover(data, self.destination_at(point)))
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
        let destination = allows_copy(unsafe { *effect })
            .then(|| self.destination_at(point))
            .flatten();
        let hover = self.hover.get().map(|hover| hover_at(hover, destination));
        self.update_hover(hover);
        unsafe {
            *effect = drop_effect(hover);
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
        let event = if copy_allowed && let Some(destination) = self.destination_at(point) {
            data.ok()
                .and_then(|data| event_for_hover(data, hover_at(hover, Some(destination))))
        } else {
            Ok(None)
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

fn hover_at(hover: DropHover, destination: Option<FileDropDestination>) -> DropHover {
    match hover {
        DropHover::Files(_) => DropHover::Files(destination),
        DropHover::Text(_) => DropHover::Text(destination),
    }
}

fn drop_effect(hover: Option<DropHover>) -> DROPEFFECT {
    if matches!(
        hover,
        Some(DropHover::Files(Some(_)) | DropHover::Text(Some(_)))
    ) {
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
    settings: tokio::sync::watch::Receiver<AppSettings>,
) -> Result<(), PortError> {
    let target: IDropTarget = ShelfDropTarget {
        hwnd,
        events,
        file_zones,
        settings,
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
