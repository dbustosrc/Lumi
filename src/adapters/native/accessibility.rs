//! UIA providers expose the same live actions and bounds used by painting/input.
#![allow(non_upper_case_globals)] // Windows bindings expose UIA constants in PascalCase.
use super::host::WM_ACCESSIBLE_ACTION;
use super::model::{Rect, UiModel};
use super::scene::{Action, Hit, Paint, Scene};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{E_INVALIDARG, E_NOINTERFACE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PostMessageW};
use windows::core::{BOOL, IUnknown, IUnknownImpl, Interface, Result, implement};

#[derive(Clone)]
struct Node {
    id: u64,
    hit: Hit,
    bounds: Rect,
    value: Option<f64>,
    toggle: Option<bool>,
}
struct Snapshot {
    hwnd: isize,
    title: String,
    bounds: Rect,
    nodes: Vec<Node>,
    focused: Option<u64>,
    alive: bool,
    visible: bool,
}
pub enum Command {
    Invoke(Action),
    Focus(u64),
    Value(Action, f32),
}
pub struct AccessibleWindow {
    snapshot: Mutex<Snapshot>,
    commands: Mutex<VecDeque<Command>>,
}
thread_local! {static ROOTS:RefCell<HashMap<isize,IRawElementProviderSimple>>=RefCell::new(HashMap::new());}
impl AccessibleWindow {
    pub fn new(hwnd: HWND, title: &str) -> Arc<Self> {
        let access = Arc::new(Self {
            snapshot: Mutex::new(Snapshot {
                hwnd: hwnd.0 as isize,
                title: title.into(),
                bounds: Rect::new(0., 0., 0., 0.),
                nodes: vec![],
                focused: None,
                alive: true,
                visible: false,
            }),
            commands: Mutex::new(VecDeque::new()),
        });
        let root: IRawElementProviderSimple = Element {
            access: access.clone(),
            id: None,
        }
        .into();
        ROOTS.with(|roots| {
            roots.borrow_mut().insert(hwnd.0 as isize, root);
        });
        access
    }
    pub fn update(&self, scene: &Scene, model: &UiModel, scale: f32) {
        let mut snapshot = self.snapshot.lock().expect("accessibility state poisoned");
        let hwnd = HWND(snapshot.hwnd as *mut _);
        let mut window = windows::Win32::Foundation::RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
            return;
        }
        snapshot.bounds = Rect::new(
            window.left as f32,
            window.top as f32,
            (window.right - window.left) as f32,
            (window.bottom - window.top) as f32,
        );
        let mut occurrences = HashMap::new();
        snapshot.nodes = scene
            .hits
            .iter()
            .map(|hit| {
                let semantic = match &hit.action {
                    Action::Preference(key, _) => format!("Preference:{key:?}:{}", hit.label),
                    other => format!("{other:?}:{}", hit.label),
                };
                let ordinal = occurrences.entry(semantic.clone()).or_insert(0_u32);
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                semantic.hash(&mut hash);
                ordinal.hash(&mut hash);
                *ordinal += 1;
                let id = hash.finish();
                let r = hit.rect;
                let bounds = Rect::new(
                    window.left as f32 + r.x * scale,
                    window.top as f32 + r.y * scale,
                    r.w * scale,
                    r.h * scale,
                );
                let value = match hit.action {
                    Action::Volume => Some(f64::from(model.volume)),
                    Action::Seek => Some(if model.duration.is_zero() {
                        0.
                    } else {
                        model.position.as_secs_f64() / model.duration.as_secs_f64()
                    }),
                    _ => None,
                };
                let toggle = match &hit.action {
                    Action::Preference(key, _) if key.is_boolean() => {
                        Some(key.value(&model.settings) != 0)
                    }
                    Action::ToggleWater => Some(model.settings.water.enabled),
                    Action::Daily => model.draft.as_ref().map(|d| d.daily),
                    Action::Intent(crate::application::events::UserIntent::ToggleTask(id)) => {
                        model.tasks.iter().find(|t| t.id == *id).map(|t| t.is_done)
                    }
                    _ => None,
                };
                Node {
                    id,
                    hit: hit.clone(),
                    bounds,
                    value,
                    toggle,
                }
            })
            .collect();
        let mut clips = Vec::new();
        for (ordinal, paint) in scene.paint.iter().enumerate() {
            let (rect, text) = match paint {
                Paint::Clip(rect) => {
                    clips.push(*rect);
                    continue;
                }
                Paint::Unclip => {
                    clips.pop();
                    continue;
                }
                Paint::Text(rect, text, ..) | Paint::Paragraph(rect, text, ..) => {
                    (*rect, text.clone())
                }
                Paint::TextRun(rect, spans, ..) => (
                    *rect,
                    spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                Paint::ButtonContent(rect, text, ..) => (*rect, text.clone()),
                _ => continue,
            };
            let rect = clips.iter().fold(rect, |r, c| r.intersect(*c));
            if text.trim().is_empty()
                || rect.w <= 0.
                || rect.h <= 0.
                || scene
                    .hits
                    .iter()
                    .any(|h| h.label == text && h.rect.intersect(rect).w > 0.)
            {
                continue;
            }
            snapshot.nodes.push(Node {
                id: 0x8000_0000_0000_0000 | ordinal as u64,
                hit: Hit {
                    rect,
                    action: Action::None,
                    label: text,
                    enabled: true,
                },
                bounds: Rect::new(
                    window.left as f32 + rect.x * scale,
                    window.top as f32 + rect.y * scale,
                    rect.w * scale,
                    rect.h * scale,
                ),
                value: None,
                toggle: None,
            });
        }
    }
    pub fn take(&self) -> Vec<Command> {
        self.commands
            .lock()
            .expect("accessibility command queue poisoned")
            .drain(..)
            .collect()
    }
    pub fn focused_action(&self) -> Option<Action> {
        let snapshot = self.snapshot.lock().ok()?;
        snapshot
            .nodes
            .iter()
            .find(|n| Some(n.id) == snapshot.focused)
            .map(|n| n.hit.action.clone())
    }
    pub fn set_visible(&self, visible: bool) {
        self.snapshot
            .lock()
            .expect("accessibility state poisoned")
            .visible = visible;
    }
    pub fn focusable(&self) -> Vec<(u64, Rect)> {
        self.snapshot
            .lock()
            .expect("accessibility state poisoned")
            .nodes
            .iter()
            .filter(|n| n.hit.enabled && n.hit.action != Action::None)
            .map(|n| (n.id, n.hit.rect))
            .collect()
    }
    pub fn set_focus(&self, id: u64) -> Option<Rect> {
        let mut snapshot = self.snapshot.lock().ok()?;
        let rect = snapshot.nodes.iter().find(|n| n.id == id)?.hit.rect;
        snapshot.focused = Some(id);
        Some(rect)
    }
    pub fn clear_focus(&self) {
        self.snapshot
            .lock()
            .expect("accessibility state poisoned")
            .focused = None;
    }
    fn send(&self, command: Command) -> Result<()> {
        let hwnd = {
            let snapshot = self.snapshot.lock().expect("accessibility state poisoned");
            if !snapshot.alive || !snapshot.visible {
                return Err(unavailable());
            }
            snapshot.hwnd
        };
        self.commands
            .lock()
            .expect("accessibility command queue poisoned")
            .push_back(command);
        unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_ACCESSIBLE_ACTION,
                WPARAM(0),
                LPARAM(0),
            )
        }
    }
    pub fn close(&self) {
        let mut snapshot = self.snapshot.lock().expect("accessibility state poisoned");
        snapshot.alive = false;
        ROOTS.with(|roots| {
            roots.borrow_mut().remove(&snapshot.hwnd);
        });
    }
}
pub fn get_object(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if lparam.0 as i32 != UiaRootObjectId {
        return None;
    }
    let root = ROOTS.with(|roots| roots.borrow().get(&(hwnd.0 as isize)).cloned())?;
    Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &root) })
}
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IInvokeProvider,
    IRangeValueProvider,
    IToggleProvider
)]
struct Element {
    access: Arc<AccessibleWindow>,
    id: Option<u64>,
}
impl Element_Impl {
    fn node(&self) -> Result<Node> {
        let snapshot = self
            .access
            .snapshot
            .lock()
            .expect("accessibility state poisoned");
        if !snapshot.alive {
            return Err(unavailable());
        }
        snapshot
            .nodes
            .iter()
            .find(|node| Some(node.id) == self.id)
            .cloned()
            .ok_or_else(unavailable)
    }
    fn fragment(&self, id: Option<u64>) -> IRawElementProviderFragment {
        Element {
            access: self.access.clone(),
            id,
        }
        .into()
    }
}
impl IRawElementProviderSimple_Impl for Element_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        if self.id.is_none() {
            return Err(windows::core::Error::empty());
        }
        let node = self.node()?;
        match pattern {
            UIA_InvokePatternId if node.value.is_none() && node.hit.action != Action::None => {
                self.to_interface::<IInvokeProvider>().cast()
            }
            UIA_RangeValuePatternId if node.value.is_some() => {
                self.to_interface::<IRangeValueProvider>().cast()
            }
            UIA_TogglePatternId if node.toggle.is_some() => {
                self.to_interface::<IToggleProvider>().cast()
            }
            _ => Err(windows::core::Error::empty()),
        }
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        if self.id.is_none() {
            let s = self
                .access
                .snapshot
                .lock()
                .expect("accessibility state poisoned");
            return Ok(match property {
                UIA_NamePropertyId => VARIANT::from(s.title.as_str()),
                UIA_ControlTypePropertyId => VARIANT::from(UIA_PaneControlTypeId.0),
                UIA_FrameworkIdPropertyId => VARIANT::from("Lumi.Win32.Direct2D"),
                UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => {
                    VARIANT::from(true)
                }
                _ => VARIANT::default(),
            });
        }
        let node = self.node()?;
        Ok(match property {
            UIA_NamePropertyId => VARIANT::from(node.hit.label.as_str()),
            UIA_AutomationIdPropertyId => {
                VARIANT::from(format!("control-{:016x}", node.id).as_str())
            }
            UIA_ControlTypePropertyId => VARIANT::from(if node.hit.action == Action::None {
                UIA_TextControlTypeId.0
            } else if node.value.is_some() {
                UIA_SliderControlTypeId.0
            } else if node.toggle.is_some() {
                UIA_CheckBoxControlTypeId.0
            } else {
                UIA_ButtonControlTypeId.0
            }),
            UIA_IsEnabledPropertyId => VARIANT::from(node.hit.enabled),
            UIA_IsKeyboardFocusablePropertyId => VARIANT::from(node.hit.action != Action::None),
            UIA_IsControlElementPropertyId | UIA_IsContentElementPropertyId => VARIANT::from(true),
            UIA_IsOffscreenPropertyId => VARIANT::from(
                !self
                    .access
                    .snapshot
                    .lock()
                    .expect("accessibility state poisoned")
                    .visible,
            ),
            UIA_HasKeyboardFocusPropertyId => VARIANT::from(
                self.access
                    .snapshot
                    .lock()
                    .expect("accessibility state poisoned")
                    .focused
                    == self.id,
            ),
            UIA_FrameworkIdPropertyId => VARIANT::from("Lumi.Win32.Direct2D"),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        if self.id.is_some() {
            return Err(windows::core::Error::empty());
        }
        let hwnd = self
            .access
            .snapshot
            .lock()
            .expect("accessibility state poisoned")
            .hwnd;
        unsafe { UiaHostProviderFromHwnd(HWND(hwnd as *mut _)) }
    }
}
impl IRawElementProviderFragment_Impl for Element_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let s = self
            .access
            .snapshot
            .lock()
            .expect("accessibility state poisoned");
        if !s.alive {
            return Err(unavailable());
        }
        let id = match (self.id, direction) {
            (None, NavigateDirection_FirstChild) => s.nodes.first().map(|n| Some(n.id)),
            (None, NavigateDirection_LastChild) => s.nodes.last().map(|n| Some(n.id)),
            (Some(_), NavigateDirection_Parent) => Some(None),
            (Some(id), NavigateDirection_NextSibling) => s
                .nodes
                .iter()
                .position(|n| n.id == id)
                .and_then(|i| s.nodes.get(i + 1))
                .map(|n| Some(n.id)),
            (Some(id), NavigateDirection_PreviousSibling) => s
                .nodes
                .iter()
                .position(|n| n.id == id)
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| s.nodes.get(i))
                .map(|n| Some(n.id)),
            _ => None,
        };
        drop(s);
        id.map(|id| self.fragment(id))
            .ok_or_else(windows::core::Error::empty)
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        let Some(id) = self.id else {
            return Ok(std::ptr::null_mut());
        };
        unsafe {
            let array = SafeArrayCreateVector(VT_I4, 0, 3);
            if array.is_null() {
                return Err(windows::core::Error::from_thread());
            }
            for (i, value) in [UiaAppendRuntimeId as i32, id as i32, (id >> 32) as i32]
                .iter()
                .enumerate()
            {
                if let Err(e) =
                    SafeArrayPutElement(array, &(i as i32), (value as *const i32).cast())
                {
                    let _ = SafeArrayDestroy(array);
                    return Err(e);
                }
            }
            Ok(array)
        }
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let r = if self.id.is_some() {
            self.node()?.bounds
        } else {
            self.access
                .snapshot
                .lock()
                .expect("accessibility state poisoned")
                .bounds
        };
        Ok(UiaRect {
            left: f64::from(r.x),
            top: f64::from(r.y),
            width: f64::from(r.w),
            height: f64::from(r.h),
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        if let Some(id) = self.id {
            self.access.send(Command::Focus(id))?;
        }
        Ok(())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        self.fragment(None).cast()
    }
}
impl IRawElementProviderFragmentRoot_Impl for Element_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let id = self
            .access
            .snapshot
            .lock()
            .expect("accessibility state poisoned")
            .nodes
            .iter()
            .rev()
            .find(|n| n.bounds.contains(x as f32, y as f32))
            .map(|n| n.id);
        if id.is_some() {
            Ok(self.fragment(id))
        } else {
            Err(windows::core::Error::empty())
        }
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        let id = self
            .access
            .snapshot
            .lock()
            .expect("accessibility state poisoned")
            .focused;
        Ok(self.fragment(id))
    }
}
impl IInvokeProvider_Impl for Element_Impl {
    fn Invoke(&self) -> Result<()> {
        let node = self.node()?;
        if node.hit.action == Action::None {
            return Err(windows::core::Error::from_hresult(E_NOINTERFACE));
        }
        if !node.hit.enabled {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                UIA_E_ELEMENTNOTENABLED as i32,
            )));
        }
        self.access.send(Command::Invoke(node.hit.action))
    }
}
impl IRangeValueProvider_Impl for Element_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        let node = self.node()?;
        if node.value.is_none() {
            return Err(windows::core::Error::from_hresult(E_NOINTERFACE));
        }
        self.access
            .send(Command::Value(node.hit.action, value as f32))
    }
    fn Value(&self) -> Result<f64> {
        self.node()?
            .value
            .ok_or_else(|| windows::core::Error::from_hresult(E_NOINTERFACE))
    }
    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(BOOL(0))
    }
    fn Maximum(&self) -> Result<f64> {
        Ok(1.)
    }
    fn Minimum(&self) -> Result<f64> {
        Ok(0.)
    }
    fn LargeChange(&self) -> Result<f64> {
        Ok(0.1)
    }
    fn SmallChange(&self) -> Result<f64> {
        Ok(0.01)
    }
}
impl IToggleProvider_Impl for Element_Impl {
    fn Toggle(&self) -> Result<()> {
        IInvokeProvider_Impl::Invoke(self)
    }
    fn ToggleState(&self) -> Result<ToggleState> {
        self.node()?
            .toggle
            .map(|v| if v { ToggleState_On } else { ToggleState_Off })
            .ok_or_else(|| windows::core::Error::from_hresult(E_NOINTERFACE))
    }
}
fn unavailable() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}
