use crate::domain::ports::{MouseActivity, PortError};
use std::mem::size_of;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};

pub struct WindowsMouseActivity;
impl MouseActivity for WindowsMouseActivity {
    fn pulse(&self) -> Result<(), PortError> {
        let input = movement_input();
        if unsafe { SendInput(&[input], size_of::<INPUT>() as i32) } != 1 {
            return Err(PortError::new(
                "mouseMove.error.blocked",
                "Windows did not accept the mouse activity event",
            ));
        }
        Ok(())
    }
}

fn movement_input() -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: MOUSEEVENTF_MOVE,
                ..Default::default()
            },
        },
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/mouse_activity.rs"]
mod tests;
