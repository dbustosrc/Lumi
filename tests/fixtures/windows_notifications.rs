use super::matches_toast_metadata;
use windows::Win32::UI::Accessibility::{
    UIA_ButtonControlTypeId, UIA_SystemAlertEventId, UIA_Window_WindowOpenedEventId,
    UIA_WindowControlTypeId,
};

#[test]
fn toast_opened_requires_only_cached_shell_metadata() {
    assert!(matches_toast_metadata(
        UIA_Window_WindowOpenedEventId,
        UIA_WindowControlTypeId,
        "Windows.UI.Core.CoreWindow",
        "ToastView"
    ));
    assert!(matches_toast_metadata(
        UIA_Window_WindowOpenedEventId,
        UIA_WindowControlTypeId,
        "Windows.UI.Core.CoreWindow",
        "ToastView_3"
    ));

    assert!(!matches_toast_metadata(
        UIA_SystemAlertEventId,
        UIA_WindowControlTypeId,
        "Windows.UI.Core.CoreWindow",
        "ToastView"
    ));
    assert!(!matches_toast_metadata(
        UIA_Window_WindowOpenedEventId,
        UIA_ButtonControlTypeId,
        "Windows.UI.Core.CoreWindow",
        "ToastView"
    ));
    assert!(!matches_toast_metadata(
        UIA_Window_WindowOpenedEventId,
        UIA_WindowControlTypeId,
        "Chrome_WidgetWin_1",
        "ToastView"
    ));
    assert!(!matches_toast_metadata(
        UIA_Window_WindowOpenedEventId,
        UIA_WindowControlTypeId,
        "Windows.UI.Core.CoreWindow",
        "OtherView"
    ));
}
