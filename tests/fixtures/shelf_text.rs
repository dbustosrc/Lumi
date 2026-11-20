use super::*;

use std::cell::Cell;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread;

use crate::application::events::{AppEvent, DropHover, FileDropDestination, create_event_channel};
use crate::application::settings::AppSettings;
use windows::Win32::Foundation::{POINTL, RECT};
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, OleInitialize,
    OleUninitialize,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, ILCreateFromPathW, ILFree, SHCreateShellItemArrayFromIDLists,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IsWindowVisible, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

struct OleApartment;

impl OleApartment {
    fn initialize() -> Self {
        unsafe { OleInitialize(None).expect("initialize dedicated OLE STA") };
        Self
    }
}

impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

struct HiddenDropWindow(windows::Win32::Foundation::HWND);

impl Drop for HiddenDropWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct FixtureTempDir(PathBuf);

impl Drop for FixtureTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct OwnedPidls(Vec<*mut ITEMIDLIST>);

impl Drop for OwnedPidls {
    fn drop(&mut self) {
        for pidl in self.0.drain(..) {
            unsafe { ILFree(Some(pidl as *const ITEMIDLIST)) };
        }
    }
}

fn shell_files_data_object(
    paths: &[PathBuf],
) -> windows::core::Result<windows::Win32::System::Com::IDataObject> {
    let mut pidls = Vec::with_capacity(paths.len());
    for path in paths {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let pidl = unsafe { ILCreateFromPathW(PCWSTR(wide.as_ptr())) };
        assert!(!pidl.is_null(), "create Shell item for {}", path.display());
        pidls.push(pidl);
    }
    let pidls = OwnedPidls(pidls);
    let const_pidls: Vec<_> = pidls
        .0
        .iter()
        .map(|pidl| *pidl as *const ITEMIDLIST)
        .collect();
    let items = unsafe { SHCreateShellItemArrayFromIDLists(&const_pidls)? };
    let data = unsafe { items.BindToHandler(None::<&IBindCtx>, &BHID_DataObject)? };
    Ok(data)
}

#[test]
fn shell_data_object_round_trips_unicode_and_advertises_unicode_text() {
    let expected = "línea uno\r\n雪と😀\nfinal\n".to_owned();
    let for_sta = expected.clone();
    let actual = thread::spawn(move || {
        let _apartment = OleApartment::initialize();
        let object = text_data_object(&for_sta).expect("create text data object with no items");
        unsafe {
            assert!(object.QueryGetData(&format(CF_UNICODETEXT.0)).is_ok());
        }
        read_text(&object).expect("read text from data object")
    })
    .join()
    .expect("STA worker should not panic");

    assert_eq!(actual, expected);
}

#[test]
fn clipboard_data_objects_route_text_and_files_to_both_destinations() -> windows::core::Result<()> {
    let _apartment = OleApartment::initialize();
    let text = "Selected text: línea uno, 雪と😀, https://example.test";
    let text_data = text_data_object(text)?;
    unsafe {
        assert!(text_data.QueryGetData(&format(CF_HDROP.0)).is_ok());
    }

    let root = std::env::temp_dir().join(format!("lumi-clipboard-data-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let _root = FixtureTempDir(root.clone());
    let paths = vec![root.join("selected-a.txt"), root.join("selected-b.txt")];
    for path in &paths {
        std::fs::write(path, "fixture file").unwrap();
    }
    let canonical_paths: Vec<_> = paths
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap())
        .collect();
    let files_data = shell_files_data_object(&paths)?;

    for destination in [FileDropDestination::Shelf, FileDropDestination::QuickShare] {
        let text_event = content_event(&text_data, destination)?.unwrap();
        assert!(matches!(
            text_event,
            AppEvent::TextDropped {
                text: actual,
                destination: actual_destination
            } if actual == text && actual_destination == destination
        ));

        let files_event = content_event(&files_data, destination)?.unwrap();
        assert!(
            matches!(
                &files_event,
                AppEvent::FilesDropped {
                    paths: actual,
                    destination: actual_destination
                } if actual.iter().map(|path| std::fs::canonicalize(path).unwrap()).collect::<Vec<_>>()
                    == canonical_paths && *actual_destination == destination
            ),
            "unexpected file event: {files_event:?}; expected {paths:?}"
        );
    }

    Ok(())
}

#[test]
fn selected_text_drop_routes_to_shelf_and_quick_share_without_showing_a_window()
-> windows::core::Result<()> {
    let _apartment = OleApartment::initialize();
    let text = "Selected text: línea uno, 雪と😀, https://example.test";
    let data = text_data_object(text)?;
    unsafe {
        assert!(data.QueryGetData(&format(CF_HDROP.0)).is_ok());
    }
    let module = unsafe { GetModuleHandleW(None)? };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Lumi OLE drop target fixture"),
            WS_POPUP,
            0,
            0,
            240,
            120,
            None,
            None,
            Some(module.into()),
            None,
        )?
    };
    let _window = HiddenDropWindow(hwnd);
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());

    let zones = Rc::new(Cell::new([None, None]));
    let (events, mut received_events) = create_event_channel();
    let (_settings_sender, settings) = tokio::sync::watch::channel(AppSettings::default());
    let target: IDropTarget = ShelfDropTarget {
        hwnd,
        events,
        file_zones: zones.clone(),
        settings,
        hover: Cell::new(None),
    }
    .into();

    for (destination, x) in [
        (FileDropDestination::Shelf, 50),
        (FileDropDestination::QuickShare, 150),
    ] {
        zones.set([None, None]);
        let point = POINTL { x, y: 50 };
        let mut effect = DROPEFFECT_COPY;
        unsafe {
            target.DragEnter(&data, MODIFIERKEYS_FLAGS(0), point, &mut effect)?;
        }
        assert_eq!(effect, DROPEFFECT_NONE);
        let initial_hover = received_events.try_recv();
        assert!(
            matches!(
                initial_hover,
                Ok(AppEvent::FileDragHover(Some(DropHover::Text(None))))
            ),
            "unexpected hover before drop zones exist: {initial_hover:?}"
        );

        zones.set([
            Some((
                FileDropDestination::Shelf,
                RECT {
                    left: 10,
                    top: 10,
                    right: 90,
                    bottom: 90,
                },
            )),
            Some((
                FileDropDestination::QuickShare,
                RECT {
                    left: 110,
                    top: 10,
                    right: 190,
                    bottom: 90,
                },
            )),
        ]);
        effect = DROPEFFECT_COPY;
        unsafe {
            target.DragOver(MODIFIERKEYS_FLAGS(0), point, &mut effect)?;
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        assert!(matches!(
            received_events.try_recv(),
            Ok(AppEvent::FileDragHover(Some(DropHover::Text(Some(actual)))))
                if actual == destination
        ));

        unsafe {
            target.Drop(&data, MODIFIERKEYS_FLAGS(0), point, &mut effect)?;
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        assert!(matches!(
            received_events.try_recv(),
            Ok(AppEvent::TextDropped { text: actual, destination: actual_destination })
                if actual == text && actual_destination == destination
        ));
    }

    assert!(received_events.try_recv().is_err());
    Ok(())
}
