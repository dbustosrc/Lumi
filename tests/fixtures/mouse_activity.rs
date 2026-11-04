use super::movement_input;
use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT_MOUSE, MOUSEEVENTF_MOVE};

#[test]
fn mouse_movement_input_is_zero_delta_without_invoking_windows_input() {
    let input = movement_input();
    assert_eq!(input.r#type, INPUT_MOUSE);

    // Inspect the value only. The fixture never calls SendInput or moves the real cursor.
    let mouse = unsafe { &input.Anonymous.mi };
    assert_eq!(mouse.dx, 0);
    assert_eq!(mouse.dy, 0);
    assert_eq!(mouse.mouseData, 0);
    assert_eq!(mouse.dwFlags, MOUSEEVENTF_MOVE);
    assert_eq!(mouse.time, 0);
    assert_eq!(mouse.dwExtraInfo, 0);
}
