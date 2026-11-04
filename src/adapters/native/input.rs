//! Native EDIT controls retain Windows Unicode, IME, caret, undo and clipboard behavior.
use super::scene::{Field, Input};
use std::collections::BTreeMap;
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DeleteObject, FW_NORMAL, HDC, HFONT, OPAQUE, OUT_DEFAULT_PRECIS, SetBkColor, SetBkMode,
    SetTextColor,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{CAccPropServices, IAccPropServices, PROPID_ACC_NAME};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};
pub const EDIT_STYLE_PROPERTY: windows::core::PCWSTR = w!("LumiEditStyle");
pub const EDIT_BACKGROUNDS: [u32; 4] = [0x080909, 0x242729, 0x212124, 0x333336];
pub const WM_EDIT_COMMIT: u32 = WM_APP + 0x13;
pub const WM_EDIT_CANCEL: u32 = WM_APP + 0x14;
pub const WM_EDIT_TAB: u32 = WM_APP + 0x15;

pub fn paint_background(dc: HDC, hwnd: HWND) -> Option<usize> {
    let style = unsafe { GetPropW(hwnd, EDIT_STYLE_PROPERTY).0 as usize };
    let rgb = *EDIT_BACKGROUNDS.get(style.checked_sub(1)?)?;
    let bgr = ((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255);
    unsafe {
        let _ = SetTextColor(dc, COLORREF(0x00ffffff));
        let _ = SetBkColor(dc, COLORREF(bgr));
        let _ = SetBkMode(dc, OPAQUE);
    }
    Some(style - 1)
}
struct Entry {
    hwnd: HWND,
    font: HFONT,
    scale: f32,
    label: String,
}
#[derive(Default)]
pub struct Inputs {
    entries: BTreeMap<Field, Entry>,
    next_id: usize,
    accessible_properties: Option<IAccPropServices>,
}
impl Inputs {
    pub fn enable(&self, enabled: bool) {
        for entry in self.entries.values() {
            unsafe {
                let _ =
                    windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(entry.hwnd, enabled);
            }
        }
    }
    pub fn hwnd(&self, key: Field) -> Option<HWND> {
        self.entries.get(&key).map(|e| e.hwnd)
    }
    pub fn sync(
        &mut self,
        parent: HWND,
        inputs: &[Input],
        scale: f32,
        focus_new: bool,
    ) -> Result<()> {
        unsafe {
            let removed: Vec<_> = self
                .entries
                .keys()
                .filter(|key| !inputs.iter().any(|i| &i.key == *key))
                .copied()
                .collect();
            for key in removed {
                if let Some(e) = self.entries.remove(&key) {
                    self.dispose(e);
                }
            }
            if !inputs.is_empty() && self.accessible_properties.is_none() {
                self.accessible_properties = Some(CoCreateInstance(
                    &CAccPropServices,
                    None,
                    CLSCTX_INPROC_SERVER,
                )?);
            }
            let mut first_new = None;
            for input in inputs {
                if !self.entries.contains_key(&input.key) {
                    self.next_id += 1;
                    let style = WS_CHILD
                        | WS_VISIBLE
                        | WS_TABSTOP
                        | WINDOW_STYLE(ES_AUTOHSCROLL as u32)
                        | if input.multiline {
                            WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32)
                        } else {
                            WINDOW_STYLE(0)
                        }
                        | if input.secret {
                            WINDOW_STYLE(ES_PASSWORD as u32)
                        } else {
                            WINDOW_STYLE(0)
                        };
                    let text: Vec<_> = input.text.encode_utf16().chain(Some(0)).collect();
                    let hwnd = CreateWindowExW(
                        WS_EX_NOPARENTNOTIFY | WS_EX_LAYERED,
                        w!("EDIT"),
                        PCWSTR(text.as_ptr()),
                        style,
                        0,
                        0,
                        1,
                        1,
                        Some(parent),
                        Some(HMENU((2000 + self.next_id) as *mut _)),
                        Some(GetModuleHandleW(None)?.into()),
                        None,
                    )?;
                    // GDI text needs its own redirection surface over the composition-only parent.
                    if let Err(error) =
                        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)
                    {
                        let _ = DestroyWindow(hwnd);
                        return Err(error);
                    }
                    if !SetWindowSubclass(hwnd, Some(edit_proc), 1, usize::from(input.multiline))
                        .as_bool()
                    {
                        let _ = DestroyWindow(hwnd);
                        return Err(windows::core::Error::from_thread());
                    }
                    let style = match input.key {
                        Field::TaskTitle => 0,
                        Field::TaskTime => 1,
                        Field::ShelfText => 2,
                        _ => 3,
                    };
                    SetPropW(
                        hwnd,
                        EDIT_STYLE_PROPERTY,
                        Some(HANDLE((style + 1) as *mut _)),
                    )?;
                    let font = font(scale, input.key);
                    let cue: Vec<_> = input.label.encode_utf16().chain(Some(0)).collect();
                    SendMessageW(
                        hwnd,
                        windows::Win32::UI::Controls::EM_SETCUEBANNER,
                        Some(WPARAM(1)),
                        Some(LPARAM(cue.as_ptr() as isize)),
                    );
                    SendMessageW(
                        hwnd,
                        WM_SETFONT,
                        Some(WPARAM(font.0 as usize)),
                        Some(LPARAM(0)),
                    );
                    self.entries.insert(
                        input.key,
                        Entry {
                            hwnd,
                            font,
                            scale,
                            label: String::new(),
                        },
                    );
                    if first_new.is_none() {
                        first_new = Some(hwnd);
                    }
                }
                let entry = self.entries.get_mut(&input.key).expect("input inserted");
                if entry.label != input.label {
                    if let Some(properties) = &self.accessible_properties {
                        let label: Vec<_> = input.label.encode_utf16().chain(Some(0)).collect();
                        properties.SetHwndPropStr(
                            entry.hwnd,
                            OBJID_CLIENT.0 as u32,
                            CHILDID_SELF,
                            PROPID_ACC_NAME,
                            PCWSTR(label.as_ptr()),
                        )?;
                        entry.label = input.label.clone();
                    }
                }

                if entry.scale != scale {
                    let old = entry.font;
                    entry.font = font(scale, input.key);
                    entry.scale = scale;
                    SendMessageW(
                        entry.hwnd,
                        WM_SETFONT,
                        Some(WPARAM(entry.font.0 as usize)),
                        Some(LPARAM(0)),
                    );
                    let _ = DeleteObject(old.into());
                }
                let r = input.rect;
                SetWindowPos(
                    entry.hwnd,
                    None,
                    (r.x * scale).round() as i32,
                    (r.y * scale).round() as i32,
                    (r.w * scale).ceil() as i32,
                    (r.h * scale).ceil() as i32,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )?;
                if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() != entry.hwnd
                    && read_text(entry.hwnd) != input.text
                {
                    let text: Vec<_> = input.text.encode_utf16().chain(Some(0)).collect();
                    SetWindowTextW(entry.hwnd, PCWSTR(text.as_ptr()))?;
                }
            }
            if focus_new {
                if let Some(hwnd) = first_new {
                    let _ = SetFocus(Some(hwnd));
                }
            }
            Ok(())
        }
    }
    fn dispose(&self, entry: Entry) {
        unsafe {
            if let Some(properties) = &self.accessible_properties {
                let _ = properties.ClearHwndProps(
                    entry.hwnd,
                    OBJID_CLIENT.0 as u32,
                    CHILDID_SELF,
                    &[PROPID_ACC_NAME],
                );
            }
            let _ = RemoveWindowSubclass(entry.hwnd, Some(edit_proc), 1);
            let _ = DestroyWindow(entry.hwnd);
            let _ = DeleteObject(entry.font.into());
        }
    }
    pub fn clear(&mut self) {
        for entry in std::mem::take(&mut self.entries).into_values() {
            self.dispose(entry);
        }
        self.accessible_properties = None;
    }
    pub fn field(&self, hwnd: HWND) -> Option<Field> {
        self.entries
            .iter()
            .find_map(|(key, e)| (e.hwnd == hwnd).then_some(*key))
    }
    pub fn collect(&self) -> Vec<(Field, String)> {
        self.entries
            .iter()
            .map(|(key, e)| (*key, read_text(e.hwnd)))
            .collect()
    }
}
impl Drop for Inputs {
    fn drop(&mut self) {
        self.clear();
    }
}

pub fn read_text(hwnd: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut text = vec![0; length + 1];
        let read = GetWindowTextW(hwnd, &mut text) as usize;
        String::from_utf16_lossy(&text[..read])
    }
}
unsafe fn font(scale: f32, key: Field) -> HFONT {
    unsafe {
        CreateFontW(
            -((match key {
                Field::TaskTitle => 11.,
                Field::TaskTime => 11.,
                Field::Preference(_) => 13.,
                _ => 12.,
            }) * scale)
                .round() as i32,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI Variable Text"),
        )
    }
}
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_KEYDOWN {
            let parent = GetParent(hwnd).unwrap_or_default();
            if wparam.0 == windows::Win32::UI::Input::KeyboardAndMouse::VK_TAB.0 as usize {
                let _ = PostMessageW(
                    Some(parent),
                    WM_EDIT_TAB,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(0),
                );
                return LRESULT(0);
            }
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = PostMessageW(
                    Some(parent),
                    WM_EDIT_CANCEL,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(0),
                );
                return LRESULT(0);
            }
            if wparam.0 == VK_RETURN.0 as usize
                && (data == 0 || GetKeyState(VK_CONTROL.0 as i32) < 0)
            {
                let _ = PostMessageW(
                    Some(parent),
                    WM_EDIT_COMMIT,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(0),
                );
                return LRESULT(0);
            }
        }
        if msg == WM_KILLFOCUS {
            if let Ok(parent) = GetParent(hwnd) {
                let _ = PostMessageW(
                    Some(parent),
                    WM_EDIT_COMMIT,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(1),
                );
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/native_input.rs"]
mod tests;
