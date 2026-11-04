pub fn covers_monitor(window: [i32; 4], monitor: [i32; 4], has_caption: bool) -> bool {
    !has_caption
        && window[0] <= monitor[0]
        && window[1] <= monitor[1]
        && window[2] >= monitor[2]
        && window[3] >= monitor[3]
}
