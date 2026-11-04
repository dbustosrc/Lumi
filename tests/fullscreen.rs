#[path = "../src/domain/fullscreen.rs"]
mod fullscreen;

use fullscreen::covers_monitor;

#[test]
fn captionless_window_covering_the_whole_monitor_is_fullscreen() {
    assert!(covers_monitor(
        [0, 0, 1920, 1080],
        [0, 0, 1920, 1080],
        false
    ));
}

#[test]
fn captionless_window_may_extend_past_monitor_edges() {
    assert!(covers_monitor(
        [-1, -1, 1921, 1081],
        [0, 0, 1920, 1080],
        false
    ));
}

#[test]
fn partial_monitor_coverage_is_not_fullscreen() {
    let monitor = [0, 0, 1920, 1080];
    for window in [
        [1, 0, 1920, 1080],
        [0, 1, 1920, 1080],
        [0, 0, 1919, 1080],
        [0, 0, 1920, 1079],
    ] {
        assert!(!covers_monitor(window, monitor, false), "window {window:?}");
    }
}

#[test]
fn captioned_maximized_window_is_not_fullscreen() {
    assert!(!covers_monitor(
        [0, 0, 1920, 1080],
        [0, 0, 1920, 1080],
        true
    ));
}

#[test]
fn fullscreen_detection_uses_monitor_coordinates_on_negative_origins() {
    assert!(covers_monitor(
        [-1920, 0, 0, 1080],
        [-1920, 0, 0, 1080],
        false
    ));
    assert!(!covers_monitor(
        [-1919, 0, 0, 1080],
        [-1920, 0, 0, 1080],
        false
    ));
}
