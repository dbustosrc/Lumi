use crate::application::events::{AppEvent, EventSender};
use crate::domain::device::WifiState;
use crate::domain::ports::PortError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Networking::Connectivity::{
    NetworkConnectivityLevel, NetworkInformation, NetworkStatusChangedEventHandler,
};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::Globalization::{GetLocaleInfoW, LOCALE_SLOCALIZEDDISPLAYNAME};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{VARIANT, VariantClear, VariantToUInt32};
use windows::Win32::System::Wmi::{
    IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WbemLocator,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, HHOOK, KBDLLHOOKSTRUCT,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};
use windows::core::{BSTR, PCWSTR, w};

static CAPS_EVENTS: Mutex<Option<EventSender>> = Mutex::new(None);
static CAPS_ON: AtomicBool = AtomicBool::new(false);
static CAPS_DOWN: AtomicBool = AtomicBool::new(false);

pub struct CapsLockWatcher(HHOOK);
impl Drop for CapsLockWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
        if let Ok(mut events) = CAPS_EVENTS.lock() {
            *events = None;
        }
    }
}
impl CapsLockWatcher {
    pub fn install(events: EventSender) -> Result<Self, PortError> {
        CAPS_ON.store(
            unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 != 0,
            Ordering::Relaxed,
        );
        CAPS_DOWN.store(false, Ordering::Relaxed);
        let hook = unsafe {
            let module = GetModuleHandleW(None)
                .map_err(|error| PortError::new("hud.error.keyboard", error))?;
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(caps_hook), Some(module.into()), 0)
                .map_err(|error| PortError::new("hud.error.keyboard", error))?
        };
        if let Ok(mut sender) = CAPS_EVENTS.lock() {
            *sender = Some(events);
        }
        Ok(Self(hook))
    }
}
unsafe extern "system" fn caps_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    // Only the Caps Lock key is observed; all keyboard input is forwarded unchanged.
    if code >= 0 && data.0 != 0 {
        let key = unsafe { &*(data.0 as *const KBDLLHOOKSTRUCT) };
        if key.vkCode == u32::from(VK_CAPITAL.0) {
            match message.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN if !CAPS_DOWN.swap(true, Ordering::Relaxed) => {
                    let enabled = !CAPS_ON.fetch_xor(true, Ordering::Relaxed);
                    if let Ok(events) = CAPS_EVENTS.lock()
                        && let Some(events) = &*events
                    {
                        let _ = events.send(AppEvent::CapsLock(enabled));
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    CAPS_DOWN.store(false, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }
    unsafe { CallNextHookEx(None, code, message, data) }
}

pub fn input_language() -> Option<u32> {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_invalid() {
            return None;
        }
        let thread = GetWindowThreadProcessId(foreground, None);
        let language = (GetKeyboardLayout(thread).0 as usize & 0xffff) as u32;
        (language != 0).then_some(language)
    }
}

pub fn language_name(language: u32) -> Option<String> {
    let mut name = [0; 128];
    let length = unsafe { GetLocaleInfoW(language, LOCALE_SLOCALIZEDDISPLAYNAME, Some(&mut name)) };
    (length > 0).then(|| String::from_utf16_lossy(&name[..length as usize - 1]))
}

pub struct WifiWatcher(i64);

impl WifiWatcher {
    pub fn start(events: EventSender) -> windows::core::Result<Self> {
        let network_events = events.clone();
        let token = NetworkInformation::NetworkStatusChanged(
            &NetworkStatusChangedEventHandler::new(move |_| {
                if let Ok(state) = wifi_state() {
                    let _ = network_events.send(AppEvent::Wifi(state));
                }
                Ok(())
            }),
        )?;
        if let Ok(state) = wifi_state() {
            let _ = events.send(AppEvent::Wifi(state));
        }
        Ok(Self(token))
    }
}

impl Drop for WifiWatcher {
    fn drop(&mut self) {
        let _ = NetworkInformation::RemoveNetworkStatusChanged(self.0);
    }
}

pub struct BrightnessWatcher(Arc<AtomicBool>);

impl BrightnessWatcher {
    pub fn start(events: EventSender) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        std::thread::Builder::new()
            .name("display-brightness".into())
            .spawn(move || {
                if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() } {
                    tracing::warn!(event = "log.hud.initializeFailed", error = %error);
                    return;
                }
                if let Err(error) = observe_brightness(&events, &worker_stop) {
                    tracing::debug!(event = "log.hud.initializeFailed", error = %error);
                }
                unsafe { CoUninitialize() };
            })?;
        Ok(Self(stop))
    }
}

impl Drop for BrightnessWatcher {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn observe_brightness(events: &EventSender, stop: &AtomicBool) -> windows::core::Result<()> {
    let service = connect_brightness()?;
    let changes = unsafe {
        service.ExecNotificationQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT * FROM WmiMonitorBrightnessEvent"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )?
    };
    let mut last_brightness = read_brightness(&service);
    if let Some(value) = last_brightness {
        let _ = events.send(AppEvent::Brightness(value));
    }
    while !events.is_closed() && !stop.load(Ordering::Relaxed) {
        let mut objects = [None];
        let mut returned = 0;
        // The provider blocks until a change. The timeout only checks for shutdown.
        unsafe { changes.Next(1000, &mut objects, &mut returned).ok()? };
        if returned != 0
            && !stop.load(Ordering::Relaxed)
            && let Some(value) = read_brightness(&service)
            && last_brightness != Some(value)
        {
            last_brightness = Some(value);
            let _ = events.send(AppEvent::Brightness(value));
        }
    }
    Ok(())
}

fn wifi_state() -> windows::core::Result<WifiState> {
    for profile in NetworkInformation::GetConnectionProfiles()? {
        if profile.IsWlanConnectionProfile()?
            && profile.GetNetworkConnectivityLevel()? != NetworkConnectivityLevel::None
        {
            return Ok(WifiState::Connected(profile.ProfileName()?.to_string()));
        }
    }
    Ok(WifiState::Disconnected)
}

fn connect_brightness() -> windows::core::Result<IWbemServices> {
    unsafe {
        let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
        let service = locator.ConnectServer(
            &BSTR::from("ROOT\\WMI"),
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            0,
            &BSTR::new(),
            None,
        )?;
        CoSetProxyBlanket(
            &service,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )?;
        Ok(service)
    }
}
fn read_brightness(service: &IWbemServices) -> Option<u8> {
    unsafe {
        let result = service
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from("SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE"),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .ok()?;
        let mut objects = [None];
        let mut returned = 0;
        result.Next(250, &mut objects, &mut returned).ok().ok()?;
        let object = objects[0].as_ref()?;
        let mut value = VARIANT::default();
        object
            .Get(w!("CurrentBrightness"), 0, &mut value, None, None)
            .ok()?;
        let number = VariantToUInt32(&value).ok();
        let _ = VariantClear(&mut value);
        number.and_then(|value| u8::try_from(value).ok())
    }
}
