//! Original Lumi vector artwork, rendered by Windows Direct2D SVG.
//! Vector paths for the native interface.
pub const PAGE_PATHS: [&str; 12] = [
    "M10 17 V5 L20 3 V15 M10 17 C10 21 3 21 3 18 C3 15 10 14 10 17 M20 15 C20 19 13 19 13 16 C13 13 20 12 20 15",
    "M20 13 A8 8 0 1 0 4 13 A8 8 0 1 0 20 13 M12 8 V13 M9 2 H15 M12 2 V5 M18 4 L20 6",
    "M12 2 C10 6 4 12 4 16 A8 7 0 0 0 20 16 C20 12 14 6 12 2 Z",
    "M5 5 H19 L22 14 V21 H2 V14 Z M2 14 H8 L10 17 H14 L16 14 H22",
    "M3 5 L5 7 L8 3 M3 13 L5 15 L8 11 M12 5 H21 M12 13 H21 M3 21 H21",
    "M4 14 V10 A8 8 0 0 1 20 10 V14 M4 12 H7 V20 H4 Z M17 12 H20 V20 H17 Z",
    "M3 6 H20 V18 H3 Z M21 10 H23 V14 H21 M7 12 H16 M12 8 V16",
    "M3 7 H21 M3 17 H21 M8 4 V10 M16 14 V20",
    "M4 5 H20 V21 H4 Z M4 10 H20 M8 2 V8 M16 2 V8 M8 14 H10 M14 14 H16",
    "M20 12 A8 8 0 1 1 18 6 M20 3 V8 H15",
    "M6 4 L20 12 L6 20 Z",
    "M8 4 V20 M16 4 V20",
];
pub const SETTINGS_PATHS: [&str; 11] = [
    "M9 3 L15 3 L16 6 L19 7 L21 11 L19 13 L19 17 L15 20 L12 19 L8 20 L5 17 L5 13 L3 11 L5 7 L8 6 Z M15 11 A3 3 0 1 0 9 11 A3 3 0 1 0 15 11",
    "M3 4 H21 V16 H3 Z M12 16 V20 M8 20 H16",
    "M21 12 A9 9 0 1 0 3 12 A9 9 0 1 0 21 12 M12 3 V21",
    "M10 17 V5 L20 3 V15 M10 17 C10 21 3 21 3 18 C3 15 10 14 10 17 M20 15 C20 19 13 19 13 16 C13 13 20 12 20 15",
    "M21 12 A9 9 0 1 0 3 12 A9 9 0 1 0 21 12 M7 12 L10 15 L17 8",
    "M4 5 H20 V21 H4 Z M4 10 H20 M8 2 V8 M16 2 V8 M8 14 H10 M14 14 H16 M8 18 H10",
    "M20 13 A8 8 0 1 0 4 13 A8 8 0 1 0 20 13 M12 9 V13 L15 15 M9 2 H15 M12 2 V5",
    "M12 2 C10 6 4 12 4 16 A8 7 0 0 0 20 16 C20 12 14 6 12 2 Z",
    "M5 5 H19 L22 14 V21 H2 V14 Z M2 14 H8 L10 17 H14 L16 14 H22",
    "M5 17 L7 14 V9 C7 2 17 2 17 9 V14 L19 17 Z M10 21 H14",
    "M21 12 A9 9 0 1 0 3 12 A9 9 0 1 0 21 12 M12 11 V18 M12 6 V8",
];
pub const ICON_PREV: (&str, f32, f32, bool, f32) = (
    "M 11 6 L 3 12 L 11 18 Z M 21 6 L 13 12 L 21 18 Z",
    24.0,
    24.0,
    true,
    0.0,
);
pub const ICON_PLAY: (&str, f32, f32, bool, f32) =
    ("M 7 4.5 L 7 19.5 L 19 12 Z", 24.0, 24.0, true, 0.0);
pub const ICON_PAUSE: (&str, f32, f32, bool, f32) = (
    "M 7 5 L 11 5 L 11 19 L 7 19 Z M 13 5 L 17 5 L 17 19 L 13 19 Z",
    24.0,
    24.0,
    true,
    0.0,
);
pub const ICON_NEXT: (&str, f32, f32, bool, f32) = (
    "M 13 6 L 21 12 L 13 18 Z M 3 6 L 11 12 L 3 18 Z",
    24.0,
    24.0,
    true,
    0.0,
);
pub const ICON_LYRICS_BUBBLE: (&str, f32, f32, bool, f32) = (
    "M 4 0.8 L 14 0.8 A 3.2 3.2 0 0 1 17.2 4 L 17.2 8.6 A 3.2 3.2 0 0 1 14 11.8 L 8.4 11.8 L 4.6 15 L 5.1 11.8 L 4 11.8 A 3.2 3.2 0 0 1 0.8 8.6 L 0.8 4 A 3.2 3.2 0 0 1 4 0.8 Z",
    18.0,
    16.0,
    true,
    1.3,
);
pub const ICON_CHEVRON_LEFT: (&str, f32, f32, bool, f32) =
    ("M 6.5 1.5 L 3 5 L 6.5 8.5", 10.0, 10.0, false, 2.2);
pub const ICON_CHEVRON_RIGHT: (&str, f32, f32, bool, f32) =
    ("M 3.5 1.5 L 7 5 L 3.5 8.5", 10.0, 10.0, false, 2.2);
pub const ICON_PLUS_CIRCLE: (&str, f32, f32, bool, f32) = (
    "M 12 2 A 10 10 0 1 0 12 22 A 10 10 0 1 0 12 2 Z M 13 7 L 13 11 L 17 11 L 17 13 L 13 13 L 13 17 L 11 17 L 11 13 L 7 13 L 7 11 L 11 11 L 11 7 Z",
    24.0,
    24.0,
    true,
    0.0,
);
pub const ICON_EDIT_PENCIL: (&str, f32, f32, bool, f32) = (
    "M 4 20 L 11 20 M 14 5 L 19 10 L 10 19 L 5 19 L 5 14 Z",
    24.0,
    24.0,
    false,
    2.0,
);
pub const ICON_MIC: (&str, f32, f32, bool, f32) = (
    "M 12 2 A 3 3 0 0 1 15 5 L 15 11 A 3 3 0 0 1 9 11 L 9 5 A 3 3 0 0 1 12 2 Z M 19 11 A 7 7 0 0 1 5 11 M 12 18 L 12 22 M 8 22 L 16 22",
    24.0,
    24.0,
    false,
    2.0,
);
pub const ICON_CHECK: (&str, f32, f32, bool, f32) =
    ("M 2 5.2 L 4 7.2 L 8 2.8", 10.0, 10.0, false, 2.4);
pub const ICON_TRASH: (&str, f32, f32, bool, f32) = (
    "M 4 7 L 20 7 M 9 7 L 9 4 L 15 4 L 15 7 M 7 7 L 8 20 L 16 20 L 17 7",
    24.0,
    24.0,
    false,
    2.1,
);
pub const ICON_BLUETOOTH: (&str, f32, f32, bool, f32) = (
    "M 7 7 L 17 17 L 12 21 L 12 3 L 17 7 L 7 17",
    24.0,
    24.0,
    false,
    2.3,
);
pub const ICON_HEADPHONES: (&str, f32, f32, bool, f32) = (
    "M 4 15 L 4 12 A 8 8 0 0 1 20 12 L 20 15 M 4 15 L 7 15 L 7 20 L 5 20 A 1 1 0 0 1 4 19 Z M 20 15 L 17 15 L 17 20 L 19 20 A 1 1 0 0 0 20 19 Z",
    24.0,
    24.0,
    false,
    2.0,
);
pub const ICON_BOLT: (&str, f32, f32, bool, f32) = (
    "M 6 0 L 0 8 L 4 8 L 3 14 L 9 6 L 5 6 Z",
    10.0,
    14.0,
    true,
    0.0,
);
pub const ICON_SETTINGS_SLIDERS: (&str, f32, f32, bool, f32) =
    ("M 4 8 L 20 8 M 4 16 L 20 16", 24.0, 24.0, false, 2.2);
pub const ICON_REMOVE_CROSS: (&str, f32, f32, bool, f32) =
    ("M 2 2 L 8 8 M 8 2 L 2 8", 10.0, 10.0, false, 2.1);

pub fn svg(key: &str, tint: u32) -> String {
    if key == "focus-glow" || key == "eye-halo" {
        return format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"><defs><radialGradient id=\"g\"><stop offset=\"0\" stop-color=\"#{tint:06x}\" stop-opacity=\"0.7\"/><stop offset=\"0.7\" stop-color=\"#{tint:06x}\" stop-opacity=\"0.55\"/><stop offset=\"1\" stop-color=\"#{tint:06x}\" stop-opacity=\"0\"/></radialGradient></defs><ellipse cx=\"50\" cy=\"50\" rx=\"50\" ry=\"50\" fill=\"url(#g)\"/></svg>"
        );
    }

    if key == "drop-zone-border" {
        return format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 336 90\"><path d=\"M16 1 h4 M16 89 h4 M24 1 h4 M24 89 h4 M32 1 h4 M32 89 h4 M40 1 h4 M40 89 h4 M48 1 h4 M48 89 h4 M56 1 h4 M56 89 h4 M64 1 h4 M64 89 h4 M72 1 h4 M72 89 h4 M80 1 h4 M80 89 h4 M88 1 h4 M88 89 h4 M96 1 h4 M96 89 h4 M104 1 h4 M104 89 h4 M112 1 h4 M112 89 h4 M120 1 h4 M120 89 h4 M128 1 h4 M128 89 h4 M136 1 h4 M136 89 h4 M144 1 h4 M144 89 h4 M152 1 h4 M152 89 h4 M160 1 h4 M160 89 h4 M168 1 h4 M168 89 h4 M176 1 h4 M176 89 h4 M184 1 h4 M184 89 h4 M192 1 h4 M192 89 h4 M200 1 h4 M200 89 h4 M208 1 h4 M208 89 h4 M216 1 h4 M216 89 h4 M224 1 h4 M224 89 h4 M232 1 h4 M232 89 h4 M240 1 h4 M240 89 h4 M248 1 h4 M248 89 h4 M256 1 h4 M256 89 h4 M264 1 h4 M264 89 h4 M272 1 h4 M272 89 h4 M280 1 h4 M280 89 h4 M288 1 h4 M288 89 h4 M296 1 h4 M296 89 h4 M304 1 h4 M304 89 h4 M312 1 h4 M312 89 h4 M1 16 v4 M335 16 v4 M1 24 v4 M335 24 v4 M1 32 v4 M335 32 v4 M1 40 v4 M335 40 v4 M1 48 v4 M335 48 v4 M1 56 v4 M335 56 v4 M1 64 v4 M335 64 v4 M1 72 v4 M335 72 v4 M1 13 Q1 1 13 1 M323 1 Q335 1 335 13 M335 77 Q335 89 323 89 M13 89 Q1 89 1 77\" fill=\"none\" stroke=\"#{tint:06x}\" stroke-width=\"1.2\"/></svg>"
        );
    }
    if key == "folder" {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 36 32\"><path d=\"M3 6H13L16 9H33A2 2 0 0 1 35 11V27A3 3 0 0 1 32 30H4A3 3 0 0 1 1 27V8A2 2 0 0 1 3 6Z\" fill=\"#d89617\"/><path d=\"M2 13H34L32 29A2 2 0 0 1 30 30.5H6A2 2 0 0 1 4 29Z\" fill=\"#ffca43\"/></svg>".into();
    }
    if key == "lyrics-on" || key == "lyrics-off" {
        let active = key == "lyrics-on";
        return format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 18 16\"><path d=\"M4 0.8H14A3.2 3.2 0 0 1 17.2 4V8.6A3.2 3.2 0 0 1 14 11.8H8.4L4.6 15L5.1 11.8H4A3.2 3.2 0 0 1 0.8 8.6V4A3.2 3.2 0 0 1 4 0.8Z\" fill=\"{}\" stroke=\"#{tint:06x}\" stroke-width=\"1.3\"/><circle cx=\"6.25\" cy=\"5.55\" r=\"1.25\" fill=\"{}\"/><circle cx=\"11.05\" cy=\"5.55\" r=\"1.25\" fill=\"{}\"/></svg>",
            if active { "white" } else { "none" },
            if active { "black" } else { "white" },
            if active { "black" } else { "white" }
        );
    }
    let (path, w, h, filled, stroke) = match key {
        "\u{e189}" => (PAGE_PATHS[0], 24., 24., false, 1.8),
        "\u{e916}" => (PAGE_PATHS[1], 24., 24., false, 1.8),
        "water-drop" => (PAGE_PATHS[2], 24., 24., false, 1.8),
        "\u{e7c3}" => (PAGE_PATHS[3], 24., 24., false, 1.8),
        "\u{e9d5}" => (PAGE_PATHS[4], 24., 24., false, 1.8),
        "\u{e7f6}" => (PAGE_PATHS[5], 24., 24., false, 1.8),
        "\u{e850}" => (PAGE_PATHS[6], 24., 24., false, 1.8),
        "page-settings" => (PAGE_PATHS[7], 24., 24., false, 1.8),
        "\u{e787}" => (PAGE_PATHS[8], 24., 24., false, 1.8),
        "\u{e777}" => (PAGE_PATHS[9], 24., 24., false, 1.8),
        "\u{e768}" => (PAGE_PATHS[10], 24., 24., false, 1.8),
        "\u{e769}" => (PAGE_PATHS[11], 24., 24., false, 1.8),
        "\u{e713}" => (SETTINGS_PATHS[0], 24., 24., false, 1.7),
        "\u{e7f4}" => (SETTINGS_PATHS[1], 24., 24., false, 1.7),
        "\u{e790}" => (SETTINGS_PATHS[2], 24., 24., false, 1.7),
        "bluetooth" => ICON_BLUETOOTH,
        "headphones" => ICON_HEADPHONES,
        "mouse-move" => (
            "M7 10V7A5 5 0 0 1 17 7V10 M12 2V8 M7 10H17V17A5 5 0 0 1 7 17Z M12 11V13",
            24.,
            24.,
            false,
            1.7,
        ),
        "sliders" => ICON_SETTINGS_SLIDERS,
        "lyrics" => ICON_LYRICS_BUBBLE,
        "\u{e946}" => (SETTINGS_PATHS[10], 24., 24., false, 1.7),
        "\u{e892}" => ICON_PREV,
        "\u{e893}" => ICON_NEXT,
        "transport-play" => ICON_PLAY,
        "transport-pause" => ICON_PAUSE,
        "spin-up" => ("M2 8L6 4L10 8", 12., 12., false, 1.2),
        "spin-down" => ("M2 4L6 8L10 4", 12., 12., false, 1.2),
        "\u{e76b}" => ICON_CHEVRON_LEFT,
        "\u{e76c}" => ICON_CHEVRON_RIGHT,
        "\u{e70f}" => ICON_EDIT_PENCIL,
        "\u{e720}" => ICON_MIC,
        "\u{e73e}" => ICON_CHECK,
        "\u{e74d}" => ICON_TRASH,
        "\u{e945}" => ICON_BOLT,
        "\u{e710}" => ("M12 5V19 M5 12H19", 24., 24., false, 2.),
        "plus-circle" => ICON_PLUS_CIRCLE,
        "\u{e77f}" => (
            "M9 4H15V17H9Z M7 2H17V5H7 M7 8H17 M7 12H17",
            24.,
            24.,
            false,
            1.8,
        ),
        "\u{e7c4}" => ("M8 3H20V17H8Z M4 7H8 M4 7V21H16V17", 24., 24., false, 1.8),
        "\u{e738}" => ("M5 12H19", 24., 24., false, 2.),
        "\u{e711}" => ICON_REMOVE_CROSS,
        "\u{e767}" | "\u{e74f}" => ("M3 9L7 9L12 4L12 20L7 15L3 15Z", 24., 24., true, 0.),
        "\u{e8a5}" => (
            "M4 1H14A3 3 0 0 1 17 4V9A3 3 0 0 1 14 12H8L4 15L5 12H4A3 3 0 0 1 1 9V4A3 3 0 0 1 4 1Z M6 5H7 M11 5H12",
            18.,
            16.,
            false,
            1.3,
        ),
        "\u{e896}" => (
            "M12 4V14M8.5 10.5L12 14L15.5 10.5 M4 12H8L9.8 15.2H14.2L16 12H20V19A1.5 1.5 0 0 1 18.5 20.5H5.5A1.5 1.5 0 0 1 4 19Z",
            24.,
            24.,
            false,
            1.9,
        ),
        "\u{e8b7}" => (
            "M3 6H13L16 9H33A2 2 0 0 1 35 11V27A3 3 0 0 1 32 30H4A3 3 0 0 1 1 27V8A2 2 0 0 1 3 6Z",
            36.,
            32.,
            true,
            0.,
        ),
        _ if key.starts_with("settings-") => {
            let i = key[9..].parse::<usize>().expect("settings icon index");
            (SETTINGS_PATHS[i], 24., 24., false, 1.7)
        }
        _ => panic!("Unregistered native vector icon: {key:?}"),
    };
    let color = format!("#{tint:06x}");
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\"><path d=\"{path}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{stroke}\"/></svg>",
        if filled { &color } else { "none" },
        if stroke > 0. { &color } else { "none" }
    )
}
