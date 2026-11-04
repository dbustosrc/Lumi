use crate::application::presenter::{NotchPresenter, ViewUpdate};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_UPDATES: u32 = WM_APP + 0x10;
#[derive(Default)]
pub struct NativePresenter {
    queue: Mutex<VecDeque<ViewUpdate>>,
    window: AtomicIsize,
    notified: AtomicBool,
}
impl NativePresenter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn attach(&self, hwnd: HWND) {
        self.window.store(hwnd.0 as isize, Ordering::Release);
        self.notify();
    }
    pub fn detach(&self) {
        self.window.store(0, Ordering::Release);
    }
    pub fn take(&self) -> Vec<ViewUpdate> {
        let mut queue = self.queue.lock().expect("UI update queue poisoned");
        self.notified.store(false, Ordering::Release);
        queue.drain(..).collect()
    }
    fn notify(&self) {
        let raw = self.window.load(Ordering::Acquire);
        if raw != 0 && !self.notified.swap(true, Ordering::AcqRel) {
            unsafe {
                if let Err(error) =
                    PostMessageW(Some(HWND(raw as *mut _)), WM_UPDATES, WPARAM(0), LPARAM(0))
                {
                    self.notified.store(false, Ordering::Release);
                    tracing::error!(event="log.window.showFailed",error=%error);
                }
            }
        }
    }
}
impl NotchPresenter for NativePresenter {
    fn apply(&self, update: ViewUpdate) {
        self.queue
            .lock()
            .expect("UI update queue poisoned")
            .push_back(update);
        self.notify();
    }
}
