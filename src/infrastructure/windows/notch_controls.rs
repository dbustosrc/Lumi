//! Read-only control overlap queries run off the rendering thread.

use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, RECT, S_OK};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationElement, UIA_BoundingRectanglePropertyId,
    UIA_ButtonControlTypeId, UIA_CONTROLTYPE_ID, UIA_CheckBoxControlTypeId,
    UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
    UIA_HyperlinkControlTypeId, UIA_IsEnabledPropertyId, UIA_IsOffscreenPropertyId,
    UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_RadioButtonControlTypeId,
    UIA_SliderControlTypeId, UIA_SpinnerControlTypeId, UIA_SplitButtonControlTypeId,
    UIA_TabItemControlTypeId,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ControlQuery {
    pub window: isize,
    pub window_bounds: [i32; 4],
    pub notch: [i32; 4],
}

type ProbeResult = (ControlQuery, Option<bool>);
// Match the sampling period; one slow provider must not build up a request backlog.
const QUERY_BUDGET: Duration = Duration::from_secs(1);

pub(super) struct ControlProbe {
    requests: SyncSender<ControlQuery>,
    results: Receiver<ProbeResult>,
    pending: bool,
}

impl ControlProbe {
    pub fn new() -> std::io::Result<Self> {
        let (requests, incoming) = sync_channel::<ControlQuery>(1);
        let (outgoing, results) = sync_channel(1);
        std::thread::Builder::new()
            .name("notch-controls".into())
            .spawn(move || {
                // SAFETY: COM and every UIA interface remain on this dedicated MTA thread.
                if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
                    tracing::warn!(event = "log.notch.controlProbeFailed", reason = "com");
                    return;
                }
                {
                    let automation = unsafe {
                        CoCreateInstance::<_, IUIAutomation2>(
                            &CUIAutomation8,
                            None,
                            CLSCTX_INPROC_SERVER,
                        )
                    };
                    match automation {
                        Ok(automation) => {
                            let configured = unsafe {
                                automation
                                    .SetConnectionTimeout(QUERY_BUDGET.as_millis() as u32)
                                    .and_then(|_| {
                                        automation
                                            .SetTransactionTimeout(QUERY_BUDGET.as_millis() as u32)
                                    })
                            };
                            if configured.is_ok() {
                                while let Ok(query) = incoming.recv() {
                                    let result = query_controls(&automation, query).unwrap_or_else(
                                        |error| {
                                            tracing::debug!(
                                                event = "log.notch.controlQueryFailed",
                                                code = error.code().0
                                            );
                                            None
                                        },
                                    );
                                    if outgoing.send((query, result)).is_err() {
                                        break;
                                    }
                                }
                            } else {
                                tracing::warn!(
                                    event = "log.notch.controlProbeFailed",
                                    reason = "timeouts"
                                );
                            }
                        }
                        Err(error) => {
                            tracing::warn!(event = "log.notch.controlProbeFailed", error = %error)
                        }
                    }
                }
                unsafe { CoUninitialize() };
            })?;
        Ok(Self {
            requests,
            results,
            pending: false,
        })
    }

    /// A result is consumed once and applies only to its original window and geometry.
    pub fn sample(&mut self, query: ControlQuery) -> Option<bool> {
        let mut evidence = None;
        while let Ok((completed, result)) = self.results.try_recv() {
            self.pending = false;
            if completed == query {
                evidence = result;
            }
        }
        if !self.pending && self.requests.try_send(query).is_ok() {
            self.pending = true;
        }
        evidence
    }
}

pub(super) fn control_overlaps_notch(
    kind: UIA_CONTROLTYPE_ID,
    enabled: bool,
    offscreen: bool,
    bounds: RECT,
    notch: RECT,
) -> bool {
    enabled
        && !offscreen
        && [
            UIA_ButtonControlTypeId,
            UIA_CheckBoxControlTypeId,
            UIA_ComboBoxControlTypeId,
            UIA_EditControlTypeId,
            UIA_HyperlinkControlTypeId,
            UIA_ListItemControlTypeId,
            UIA_MenuItemControlTypeId,
            UIA_RadioButtonControlTypeId,
            UIA_SliderControlTypeId,
            UIA_SpinnerControlTypeId,
            UIA_SplitButtonControlTypeId,
            UIA_TabItemControlTypeId,
        ]
        .contains(&kind)
        && overlaps(bounds, notch)
}

fn overlaps(a: RECT, b: RECT) -> bool {
    a.right > a.left
        && a.bottom > a.top
        && a.left < b.right
        && b.left < a.right
        && a.top < b.bottom
        && b.top < a.bottom
}

fn optional_element(
    result: windows::core::Result<IUIAutomationElement>,
) -> windows::core::Result<Option<IUIAutomationElement>> {
    match result {
        Ok(element) => Ok(Some(element)),
        // windows-core represents a successful null COM interface as Error::empty (S_OK).
        Err(error) if error.code() == S_OK => Ok(None),
        Err(error) => Err(error),
    }
}

fn query_controls(
    automation: &IUIAutomation2,
    query: ControlQuery,
) -> windows::core::Result<Option<bool>> {
    let started = Instant::now();
    let [left, top, right, bottom] = query.notch;
    let notch = RECT {
        left,
        top,
        right,
        bottom,
    };
    // Start at the background HWND, never at the desktop or the foreground overlay.
    let cache = unsafe { automation.CreateCacheRequest() }?;
    for property in [
        UIA_BoundingRectanglePropertyId,
        UIA_ControlTypePropertyId,
        UIA_IsEnabledPropertyId,
        UIA_IsOffscreenPropertyId,
    ] {
        unsafe { cache.AddProperty(property) }?;
    }
    let root =
        unsafe { automation.ElementFromHandleBuildCache(HWND(query.window as *mut _), &cache) }?;
    let walker = unsafe { automation.ControlViewWalker() }?;
    enum Step {
        Visit(IUIAutomationElement),
        SiblingOf(IUIAutomationElement),
    }
    let mut stack = vec![Step::Visit(root)];
    while let Some(step) = stack.pop() {
        // Bound the work even when a provider has an unusually large subtree.
        if started.elapsed() >= QUERY_BUDGET {
            tracing::debug!(event = "log.notch.controlQueryTimedOut");
            return Ok(None);
        }
        let element = match step {
            Step::Visit(element) => element,
            Step::SiblingOf(element) => {
                if let Some(next) = optional_element(unsafe {
                    walker.GetNextSiblingElementBuildCache(&element, &cache)
                })? {
                    stack.push(Step::SiblingOf(next.clone()));
                    stack.push(Step::Visit(next));
                }
                continue;
            }
        };
        let bounds = unsafe { element.CachedBoundingRectangle() }?;
        if !overlaps(bounds, notch) {
            continue;
        }
        let kind = unsafe { element.CachedControlType() }?;
        let enabled = unsafe { element.CachedIsEnabled() }?.as_bool();
        let offscreen = unsafe { element.CachedIsOffscreen() }?.as_bool();
        if control_overlaps_notch(kind, enabled, offscreen, bounds, notch) {
            return Ok(Some(true));
        }
        let child =
            optional_element(unsafe { walker.GetFirstChildElementBuildCache(&element, &cache) })?;
        if let Some(child) = child {
            // Inspect a child before asking for all its siblings, so a matching control ends early.
            stack.push(Step::SiblingOf(child.clone()));
            stack.push(Step::Visit(child));
        }
    }
    Ok(Some(false))
}
