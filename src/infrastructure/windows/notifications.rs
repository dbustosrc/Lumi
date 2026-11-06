//! Observe only the Shell's visible toast window metadata, never notification text.
use crate::application::events::{AppEvent, EventSender};
use std::sync::mpsc::{Sender, channel};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_None, CUIAutomation8, IUIAutomation2, IUIAutomationElement,
    IUIAutomationEventHandler, IUIAutomationEventHandler_Impl, TreeScope_Element,
    TreeScope_Subtree, UIA_AutomationIdPropertyId, UIA_CONTROLTYPE_ID, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_EVENT_ID, UIA_Window_WindowOpenedEventId,
    UIA_WindowControlTypeId,
};
use windows::core::{Ref, Result, implement};

pub struct NotificationWatcher {
    stop: Sender<()>,
}
impl Drop for NotificationWatcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}
impl NotificationWatcher {
    pub fn start(events: EventSender) -> std::io::Result<Self> {
        let (stop, stopped) = channel();
        std::thread::Builder::new()
            .name("windows-toast-events".into())
            .spawn(move || {
                if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() } {
                    tracing::warn!(
                        event = "log.notifications.subscribeFailed",
                        code = error.code().0
                    );
                    return;
                }
                let result = (|| -> Result<()> {
                    let automation: IUIAutomation2 =
                        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
                    unsafe {
                        automation.SetConnectionTimeout(1000)?;
                        automation.SetTransactionTimeout(1000)?;
                        let root = automation.GetRootElement()?;
                        let cache = automation.CreateCacheRequest()?;
                        cache.SetTreeScope(TreeScope_Element)?;
                        cache.SetAutomationElementMode(AutomationElementMode_None)?;
                        for property in [
                            UIA_ClassNamePropertyId,
                            UIA_AutomationIdPropertyId,
                            UIA_ControlTypePropertyId,
                        ] {
                            cache.AddProperty(property)?;
                        }
                        let handler: IUIAutomationEventHandler = ToastOpened { events }.into();
                        automation.AddAutomationEventHandler(
                            UIA_Window_WindowOpenedEventId,
                            &root,
                            TreeScope_Subtree,
                            &cache,
                            &handler,
                        )?;
                        let _ = stopped.recv();
                        automation.RemoveAutomationEventHandler(
                            UIA_Window_WindowOpenedEventId,
                            &root,
                            &handler,
                        )?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    tracing::warn!(
                        event = "log.notifications.subscribeFailed",
                        code = error.code().0
                    );
                }
                unsafe { CoUninitialize() };
            })?;
        Ok(Self { stop })
    }
}

#[implement(IUIAutomationEventHandler)]
struct ToastOpened {
    events: EventSender,
}
impl IUIAutomationEventHandler_Impl for ToastOpened_Impl {
    fn HandleAutomationEvent(
        &self,
        sender: Ref<'_, IUIAutomationElement>,
        event_id: UIA_EVENT_ID,
    ) -> Result<()> {
        if event_id != UIA_Window_WindowOpenedEventId {
            return Ok(());
        }
        let Some(sender) = sender.as_ref() else {
            return Ok(());
        };
        let toast = (|| -> Result<bool> {
            unsafe {
                Ok(matches_toast_metadata(
                    event_id,
                    sender.CachedControlType()?,
                    &sender.CachedClassName()?.to_string(),
                    &sender.CachedAutomationId()?.to_string(),
                ))
            }
        })()
        .unwrap_or(false);
        if toast {
            let _ = self.events.send(AppEvent::NotificationArrived);
        }
        Ok(())
    }
}

fn matches_toast_metadata(
    event_id: UIA_EVENT_ID,
    kind: UIA_CONTROLTYPE_ID,
    class: &str,
    id: &str,
) -> bool {
    event_id == UIA_Window_WindowOpenedEventId
        && kind == UIA_WindowControlTypeId
        && matches!(class, "Windows.UI.Core.CoreWindow" | "FlexibleToastView")
        && id.contains("ToastView")
}

#[cfg(test)]
#[path = "../../../tests/fixtures/windows_notifications.rs"]
mod tests;
