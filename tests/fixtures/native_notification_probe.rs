use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, HWINEVENTHOOK, IUIAutomation5, IUIAutomationElement, IUIAutomationEventHandler,
    IUIAutomationEventHandler_Impl, IUIAutomationNotificationEventHandler,
    IUIAutomationNotificationEventHandler_Impl, NotificationKind, NotificationProcessing,
    SetWinEventHook, TreeScope_Subtree, UIA_EVENT_ID, UIA_NotificationEventId,
    UIA_SystemAlertEventId, UnhookWinEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_SHOW, EVENT_SYSTEM_ALERT, GetClassNameW,
    GetWindowThreadProcessId, PM_REMOVE, PeekMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT,
};
use windows::Win32::UI::Shell::{
    QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT,
    QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::core::{BSTR, Ref, Result as WinResult, implement};

const CLASSIC_POWERSHELL_AUMID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
const TOAST_SCRIPT: &str = r#"
[void][Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
[void][Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType = WindowsRuntime]
[void][Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]
$aumid = '__AUMID__'
$notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($aumid)
$history = [Windows.UI.Notifications.ToastNotificationManager]::History
$historyBefore = @($history.GetHistory($aumid))
Write-Output ("toast_notifier_setting=" + $notifier.Setting)
Write-Output ("toast_history_count_before=" + $historyBefore.Count)
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml('<toast duration="long"><visual><binding template="ToastGeneric"><text>Lumi event probe</text><text>Local test notification</text></binding></visual></toast>')
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
$toast.ExpirationTime = [DateTimeOffset]::Now.AddSeconds(15)
$notifier.Show($toast)
Start-Sleep -Milliseconds 500
$historyAfter = @($history.GetHistory($aumid))
Write-Output ("toast_history_count_after=" + $historyAfter.Count)
"#;

static WINEVENT_SYSTEM_COUNT: AtomicUsize = AtomicUsize::new(0);
static WINEVENT_OBJECT_COUNT: AtomicUsize = AtomicUsize::new(0);
static WINEVENT_SOURCES: Mutex<Vec<WinEventSource>> = Mutex::new(Vec::new());
static UIA_LIVE_SOURCES: Mutex<Vec<UiaEventSource>> = Mutex::new(Vec::new());
static UIA_OPEN_SOURCES: Mutex<Vec<UiaEventSource>> = Mutex::new(Vec::new());

#[derive(Clone, Debug, PartialEq, Eq)]
struct WinEventSource {
    class_name: String,
    object_id: i32,
    process_id: u32,
    event_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UiaEventSource {
    event_id: i32,
    process_id: i32,
    control_type: i32,
    hwnd_class_name: String,
    class_name: String,
    automation_id: String,
    children: Vec<UiaChildSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UiaChildSource {
    depth: u8,
    process_id: i32,
    control_type: i32,
    class_name: String,
    automation_id: String,
}

#[implement(IUIAutomationNotificationEventHandler)]
struct NotificationCounter {
    event_count: Arc<AtomicUsize>,
    source_process: Arc<AtomicI32>,
    source_control_type: Arc<AtomicI32>,
}

impl IUIAutomationNotificationEventHandler_Impl for NotificationCounter_Impl {
    fn HandleNotificationEvent(
        &self,
        sender: Ref<'_, IUIAutomationElement>,
        _kind: NotificationKind,
        _processing: NotificationProcessing,
        _display_string: &BSTR,
        _activity_id: &BSTR,
    ) -> WinResult<()> {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        record_uia_source(sender, &self.source_process, &self.source_control_type);
        Ok(())
    }
}

#[implement(IUIAutomationEventHandler)]
struct SystemAlertCounter {
    event_count: Arc<AtomicUsize>,
    event_id: Arc<AtomicI32>,
    source_process: Arc<AtomicI32>,
    source_control_type: Arc<AtomicI32>,
}

#[implement(IUIAutomationEventHandler)]
struct MetadataEventCounter {
    event_count: Arc<AtomicUsize>,
    event_id: i32,
    sources: &'static Mutex<Vec<UiaEventSource>>,
    child_walker: Option<windows::Win32::UI::Accessibility::IUIAutomationTreeWalker>,
}

impl IUIAutomationEventHandler_Impl for MetadataEventCounter_Impl {
    fn HandleAutomationEvent(
        &self,
        sender: Ref<'_, IUIAutomationElement>,
        _event_id: UIA_EVENT_ID,
    ) -> WinResult<()> {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        if let Some(mut source) = uia_source(&sender, self.event_id) {
            if source.class_name == "XamlExplorerHostIslandWindow" {
                if let Some(walker) = &self.child_walker {
                    source.children = collect_child_metadata(&sender, walker);
                }
            }
            record_metadata(self.sources, source);
        }
        Ok(())
    }
}

impl IUIAutomationEventHandler_Impl for SystemAlertCounter_Impl {
    fn HandleAutomationEvent(
        &self,
        sender: Ref<'_, IUIAutomationElement>,
        event_id: UIA_EVENT_ID,
    ) -> WinResult<()> {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        self.event_id.store(event_id.0, Ordering::Relaxed);
        record_uia_source(sender, &self.source_process, &self.source_control_type);
        Ok(())
    }
}

fn record_uia_source(
    sender: Ref<'_, IUIAutomationElement>,
    process_id: &AtomicI32,
    control_type: &AtomicI32,
) {
    if let Some(sender) = sender.as_ref() {
        if let Ok(value) = unsafe { sender.CurrentProcessId() } {
            process_id.store(value, Ordering::Relaxed);
        }
        if let Ok(value) = unsafe { sender.CurrentControlType() } {
            control_type.store(value.0, Ordering::Relaxed);
        }
    }
}

fn uia_source(sender: &Ref<'_, IUIAutomationElement>, event_id: i32) -> Option<UiaEventSource> {
    let sender = sender.as_ref()?;
    let hwnd_class_name =
        window_class_name(unsafe { sender.CurrentNativeWindowHandle().ok()? });
    Some(UiaEventSource {
        event_id,
        process_id: unsafe { sender.CurrentProcessId().ok()? },
        control_type: unsafe { sender.CurrentControlType().ok()?.0 },
        hwnd_class_name,
        class_name: safe_metadata(unsafe { sender.CurrentClassName().ok()? }.to_string()),
        automation_id: safe_metadata(unsafe { sender.CurrentAutomationId().ok()? }.to_string()),
        children: Vec::new(),
    })
}

fn window_class_name(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return String::new();
    }
    let mut class_name = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, &mut class_name) }.max(0) as usize;
    safe_metadata(String::from_utf16_lossy(&class_name[..length]))
}

fn collect_child_metadata(
    sender: &Ref<'_, IUIAutomationElement>,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
) -> Vec<UiaChildSource> {
    let Some(sender) = sender.as_ref() else {
        return Vec::new();
    };
    let mut children = Vec::new();
    collect_children(walker, sender, 1, &mut children);
    children
}

fn collect_children(
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    depth: u8,
    children: &mut Vec<UiaChildSource>,
) {
    if depth > 4 || children.len() >= 64 {
        return;
    }
    let mut child = unsafe { walker.GetFirstChildElement(parent) }.ok();
    while let Some(element) = child {
        if children.len() >= 64 {
            break;
        }
        if let (Ok(process_id), Ok(control_type), Ok(class_name), Ok(automation_id)) = unsafe {
            (
                element.CurrentProcessId(),
                element.CurrentControlType(),
                element.CurrentClassName(),
                element.CurrentAutomationId(),
            )
        } {
            children.push(UiaChildSource {
                depth,
                process_id,
                control_type: control_type.0,
                class_name: safe_metadata(class_name.to_string()),
                automation_id: safe_metadata(automation_id.to_string()),
            });
        }
        collect_children(walker, &element, depth + 1, children);
        child = unsafe { walker.GetNextSiblingElement(&element) }.ok();
    }
}

fn safe_metadata(value: String) -> String {
    if value.len() > 64
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        "<redacted>".into()
    } else {
        value
    }
}

fn record_metadata(sources: &'static Mutex<Vec<UiaEventSource>>, source: UiaEventSource) {
    let mut sources = sources
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if sources.len() < 16 && !sources.contains(&source) {
        sources.push(source);
    }
}

unsafe extern "system" fn winevent_callback(
    _hook: HWINEVENTHOOK,
    event_id: u32,
    hwnd: HWND,
    object_id: i32,
    _child_id: i32,
    _thread_id: u32,
    _event_time: u32,
) {
    if event_id == EVENT_SYSTEM_ALERT {
        WINEVENT_SYSTEM_COUNT.fetch_add(1, Ordering::Relaxed);
    } else if event_id == EVENT_OBJECT_SHOW {
        WINEVENT_OBJECT_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    let mut process_id = 0;
    if !hwnd.0.is_null() {
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    }
    let mut class_name = [0u16; 128];
    let class_len = if hwnd.0.is_null() {
        0
    } else {
        unsafe { GetClassNameW(hwnd, &mut class_name) }.max(0) as usize
    };
    let class_name = safe_metadata(String::from_utf16_lossy(&class_name[..class_len]));
    let mut sources = WINEVENT_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let source = WinEventSource {
        class_name,
        object_id,
        process_id,
        event_id,
    };
    if sources.len() < 16 && !sources.contains(&source) {
        sources.push(source);
    }
}

struct WinEventHook(HWINEVENTHOOK);

impl Drop for WinEventHook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.0);
        }
    }
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[test]
#[ignore = "explicit local toast delivery probe"]
fn windows_toast_event_sources_probe_metadata_only()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let _com = ComApartment;
    let (app_events, mut app_event_receiver) = crate::application::events::create_event_channel();
    let _notification_watcher =
        crate::infrastructure::windows::notifications::NotificationWatcher::start(app_events)?;
    thread::sleep(Duration::from_millis(250));
    WINEVENT_SYSTEM_COUNT.store(0, Ordering::Relaxed);
    WINEVENT_OBJECT_COUNT.store(0, Ordering::Relaxed);
    WINEVENT_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    UIA_LIVE_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    UIA_OPEN_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();

    let automation = unsafe {
        CoCreateInstance::<_, IUIAutomation5>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?
    };
    let root = unsafe { automation.GetRootElement()? };
    let notification_count = Arc::new(AtomicUsize::new(0));
    let notification_process = Arc::new(AtomicI32::new(0));
    let notification_control = Arc::new(AtomicI32::new(0));
    let notification_handler: IUIAutomationNotificationEventHandler = NotificationCounter {
        event_count: notification_count.clone(),
        source_process: notification_process.clone(),
        source_control_type: notification_control.clone(),
    }
    .into();
    unsafe {
        automation.AddNotificationEventHandler(
            &root,
            TreeScope_Subtree,
            None,
            &notification_handler,
        )?;
    }

    let alert_count = Arc::new(AtomicUsize::new(0));
    let alert_event_id = Arc::new(AtomicI32::new(0));
    let alert_process = Arc::new(AtomicI32::new(0));
    let alert_control = Arc::new(AtomicI32::new(0));
    let alert_handler: IUIAutomationEventHandler = SystemAlertCounter {
        event_count: alert_count.clone(),
        event_id: alert_event_id.clone(),
        source_process: alert_process.clone(),
        source_control_type: alert_control.clone(),
    }
    .into();
    unsafe {
        automation.AddAutomationEventHandler(
            UIA_SystemAlertEventId,
            &root,
            TreeScope_Subtree,
            None,
            &alert_handler,
        )?;
    }

    let live_count = Arc::new(AtomicUsize::new(0));
    let live_handler: IUIAutomationEventHandler = MetadataEventCounter {
        event_count: live_count.clone(),
        event_id: windows::Win32::UI::Accessibility::UIA_LiveRegionChangedEventId.0,
        sources: &UIA_LIVE_SOURCES,
        child_walker: None,
    }
    .into();
    unsafe {
        automation.AddAutomationEventHandler(
            windows::Win32::UI::Accessibility::UIA_LiveRegionChangedEventId,
            &root,
            TreeScope_Subtree,
            None,
            &live_handler,
        )?;
    }

    let window_open_count = Arc::new(AtomicUsize::new(0));
    let window_open_handler: IUIAutomationEventHandler = MetadataEventCounter {
        event_count: window_open_count.clone(),
        event_id: windows::Win32::UI::Accessibility::UIA_Window_WindowOpenedEventId.0,
        sources: &UIA_OPEN_SOURCES,
        child_walker: Some(unsafe { automation.RawViewWalker()? }),
    }
    .into();
    unsafe {
        automation.AddAutomationEventHandler(
            windows::Win32::UI::Accessibility::UIA_Window_WindowOpenedEventId,
            &root,
            TreeScope_Subtree,
            None,
            &window_open_handler,
        )?;
    }

    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_ALERT,
            EVENT_SYSTEM_ALERT,
            None,
            Some(winevent_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    assert!(!hook.0.is_null(), "WinEvent SystemAlert hook installed");
    let _hook = WinEventHook(hook);

    let object_show_hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(winevent_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    assert!(
        !object_show_hook.0.is_null(),
        "WinEvent ObjectShow hook installed"
    );
    let _object_show_hook = WinEventHook(object_show_hook);

    let toast_script = TOAST_SCRIPT.replace("__AUMID__", CLASSIC_POWERSHELL_AUMID);
    let toast_status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &toast_script,
        ])
        .creation_flags(0x0800_0000)
        .output();
    pump_messages_for(Duration::from_secs(8));
    let mut notification_arrived_count = 0;
    while let Ok(event) = app_event_receiver.try_recv() {
        if matches!(event, crate::application::events::AppEvent::NotificationArrived) {
            notification_arrived_count += 1;
        }
    }

    unsafe {
        automation.RemoveNotificationEventHandler(&root, &notification_handler)?;
        automation.RemoveAutomationEventHandler(UIA_SystemAlertEventId, &root, &alert_handler)?;
        automation.RemoveAutomationEventHandler(
            windows::Win32::UI::Accessibility::UIA_LiveRegionChangedEventId,
            &root,
            &live_handler,
        )?;
        automation.RemoveAutomationEventHandler(
            windows::Win32::UI::Accessibility::UIA_Window_WindowOpenedEventId,
            &root,
            &window_open_handler,
        )?;
    }

    let toast_exit_code = toast_status
        .as_ref()
        .ok()
        .and_then(|output| output.status.code())
        .unwrap_or(-1);
    let shell_notification_state = unsafe { SHQueryUserNotificationState() }
        .map(|state| state.0)
        .unwrap_or(-1);
    let win_events = WINEVENT_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let object_show_sources: Vec<_> = win_events
        .iter()
        .filter(|source| source.event_id == EVENT_OBJECT_SHOW)
        .cloned()
        .collect();
    let live_sources = UIA_LIVE_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let open_sources = UIA_OPEN_SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    println!("toast_sender_aumid={CLASSIC_POWERSHELL_AUMID}");
    println!("toast_sender_exit_code={toast_exit_code}");
    println!("lumi_notification_arrived_count={notification_arrived_count}");
    if let Some(output) = toast_status.as_ref().ok() {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.starts_with("toast_notifier_setting=")
                || line.starts_with("toast_history_count_")
            {
                println!("{line}");
            }
        }
    }
    println!(
        "shell_user_notification_state={} ({})",
        shell_notification_state,
        match shell_notification_state {
            x if x == QUNS_NOT_PRESENT.0 => "not_present",
            x if x == QUNS_BUSY.0 => "busy",
            x if x == QUNS_RUNNING_D3D_FULL_SCREEN.0 => "fullscreen",
            x if x == QUNS_PRESENTATION_MODE.0 => "presentation_mode",
            x if x == QUNS_ACCEPTS_NOTIFICATIONS.0 => "accepts_notifications",
            x if x == QUNS_QUIET_TIME.0 => "quiet_time",
            x if x == QUNS_APP.0 => "app",
            _ => "unknown",
        }
    );
    println!(
        "uia_notification_event_id={} count={} sender_pid={} sender_control_type={}",
        UIA_NotificationEventId.0,
        notification_count.load(Ordering::Relaxed),
        notification_process.load(Ordering::Relaxed),
        notification_control.load(Ordering::Relaxed),
    );
    println!(
        "uia_system_alert_event_id={} delivered_event_id={} count={} sender_pid={} sender_control_type={}",
        UIA_SystemAlertEventId.0,
        alert_event_id.load(Ordering::Relaxed),
        alert_count.load(Ordering::Relaxed),
        alert_process.load(Ordering::Relaxed),
        alert_control.load(Ordering::Relaxed),
    );
    println!(
        "winevent_system_alert_id={} count={}",
        EVENT_SYSTEM_ALERT,
        WINEVENT_SYSTEM_COUNT.load(Ordering::Relaxed),
    );
    println!(
        "uia_live_region_event_id={} count={} metadata={:?}",
        windows::Win32::UI::Accessibility::UIA_LiveRegionChangedEventId.0,
        live_count.load(Ordering::Relaxed),
        live_sources,
    );
    println!(
        "uia_window_opened_event_id={} count={} metadata={:?}",
        windows::Win32::UI::Accessibility::UIA_Window_WindowOpenedEventId.0,
        window_open_count.load(Ordering::Relaxed),
        open_sources,
    );
    println!(
        "winevent_object_show_id={} count={} metadata={:?}",
        EVENT_OBJECT_SHOW,
        WINEVENT_OBJECT_COUNT.load(Ordering::Relaxed),
        object_show_sources,
    );
    assert!(
        notification_arrived_count > 0,
        "NotificationWatcher did not send NotificationArrived"
    );
    Ok(())
}

fn pump_messages_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        unsafe {
            let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}
