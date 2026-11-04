use super::{
    CMD_TOGGLE_MOUSE_MOVE, TrayMenuAction, TrayMenuChecks, TrayMenuLabels, build_popup_menu,
    map_command_to_action,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyMenu, GetMenuState, MF_BYCOMMAND, MF_CHECKED,
};

#[test]
fn mouse_move_tray_toggle_has_checked_state_and_command_mapping() {
    let labels = TrayMenuLabels {
        tooltip: "Lumi".into(),
        settings: "Settings".into(),
        minimal_notch: "Minimal".into(),
        mascot: "Mascot".into(),
        mouse_move: "Cursor activity".into(),
        quit: "Quit".into(),
    };
    let menu_state = |enabled| {
        let menu = unsafe {
            build_popup_menu(
                &labels,
                TrayMenuChecks {
                    is_minimal_notch: false,
                    is_mascot_enabled: true,
                    is_mouse_move_enabled: enabled,
                },
            )
        }
        .expect("create an unshown native tray menu");
        let state = unsafe { GetMenuState(menu, CMD_TOGGLE_MOUSE_MOVE as u32, MF_BYCOMMAND) };
        unsafe { DestroyMenu(menu) }.unwrap();
        assert_ne!(
            state,
            u32::MAX,
            "mouse move command exists in the tray menu"
        );
        state
    };

    assert_ne!(
        menu_state(true) & MF_CHECKED.0 as u32,
        0,
        "enabled mouse move preference is checked"
    );
    assert_eq!(
        menu_state(false) & MF_CHECKED.0 as u32,
        0,
        "disabled mouse move preference is unchecked"
    );
    assert_eq!(
        map_command_to_action(CMD_TOGGLE_MOUSE_MOVE),
        Some(TrayMenuAction::ToggleMouseMove)
    );
}
