use super::*;

use std::thread;

use windows::Win32::System::Ole::{CF_UNICODETEXT, OleInitialize, OleUninitialize};

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
