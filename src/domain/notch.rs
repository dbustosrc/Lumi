/// Every visual state the notch can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotchView {
    /// Unobtrusive thin top-edge bezel strip hiding time and battery.
    Minimal,
    /// Mascot eyes shown after a quiet period.
    Resting,
    /// Compact pill with battery, clock and equalizer.
    #[default]
    Closed,
    Music,
    Lyrics,
    /// Temporary drag-in / drag-out file holding area.
    Shelf,
    QuickShare,
    FileDrop,
    Todo,
    Pomodoro,
    Water,
    Widgets,
    Hud,
    /// Expanded interactive wireless headphones status page.
    Headphones,
    /// Expanded interactive battery & charging status page.
    Charging,
    /// Short peek when wireless headphones connect while closed.
    HeadphonesPeek,
    /// Short peek when the charger is plugged in while closed.
    ChargingPeek,
}

/// Pages reachable while the notch is expanded (page dots & loop arrows order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotchPage {
    #[default]
    Music,
    Shelf,
    QuickShare,
    Todo,
    Headphones,
    Charging,
    Pomodoro,
    Water,
    Widgets,
}

impl NotchPage {
    pub const ALL: [NotchPage; 9] = [
        NotchPage::Music,
        NotchPage::Pomodoro,
        NotchPage::Water,
        NotchPage::Shelf,
        NotchPage::QuickShare,
        NotchPage::Todo,
        NotchPage::Headphones,
        NotchPage::Charging,
        NotchPage::Widgets,
    ];

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|page| *page == self)
            .unwrap_or_default()
    }

    pub fn available(widgets_enabled: bool) -> &'static [Self] {
        &Self::ALL[..Self::ALL.len() - usize::from(!widgets_enabled)]
    }

    pub fn step(self, direction: isize, widgets_enabled: bool) -> Self {
        let pages = Self::available(widgets_enabled);
        if self == Self::Widgets && !widgets_enabled {
            return if direction < 0 {
                Self::Charging
            } else {
                Self::Music
            };
        }
        pages[(self.index() as isize + direction).rem_euclid(pages.len() as isize) as usize]
    }

    pub fn view(self) -> NotchView {
        match self {
            Self::Music => NotchView::Music,
            Self::Shelf => NotchView::Shelf,
            Self::QuickShare => NotchView::QuickShare,
            Self::Todo => NotchView::Todo,
            Self::Headphones => NotchView::Headphones,
            Self::Charging => NotchView::Charging,
            Self::Pomodoro => NotchView::Pomodoro,
            Self::Water => NotchView::Water,
            Self::Widgets => NotchView::Widgets,
        }
    }
}

/// Transient notification shown for a few seconds while closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Peek {
    Charging,
    Headphones,
    Hud,
}

/// Facts needed to decide which view the notch must display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotchContext {
    pub is_hovered: bool,
    /// Active inbound OS file drag hovering over the notch window.
    pub is_file_drag_over: bool,
    pub is_choosing_drop_destination: bool,
    /// Keeps the notch expanded while the user interacts (e.g. typing a task).
    pub is_interaction_locked: bool,
    /// Active voice recording or transcription session.
    pub is_voice_busy: bool,
    pub page: NotchPage,
    pub is_lyrics_mode: bool,
    pub has_lyrics: bool,
    pub is_playing: bool,
    pub peek: Option<Peek>,
    pub is_actionable_alert: bool,
    pub is_quiet: bool,
    pub is_mascot_enabled: bool,
    pub is_minimal_notch: bool,
    pub has_nearby_content: bool,
}

/// Resolves the notch view from hover, playback, and system events. Hover opens the notch,
/// leaving closes it (to `Minimal`, `Resting` or `Closed`), peeks interrupt
/// briefly and lyrics stay pinned while a song with synced lyrics is playing.
pub fn resolve_view(context: &NotchContext) -> NotchView {
    let is_open = context.is_hovered || context.is_interaction_locked;
    if context.is_choosing_drop_destination {
        return NotchView::FileDrop;
    }
    if context.is_file_drag_over {
        return if context.is_mascot_enabled && !is_open {
            NotchView::Closed
        } else {
            NotchView::Shelf
        };
    }
    if context.peek == Some(Peek::Hud)
        && context.is_actionable_alert
        && !context.is_interaction_locked
    {
        return NotchView::Hud;
    }

    if let (Some(peek), false) = (context.peek, is_open) {
        return match peek {
            Peek::Charging => NotchView::ChargingPeek,
            Peek::Headphones => NotchView::HeadphonesPeek,
            Peek::Hud => NotchView::Hud,
        };
    }
    if is_open {
        return if context.page == NotchPage::Music && context.is_lyrics_mode {
            NotchView::Lyrics
        } else {
            context.page.view()
        };
    }
    if context.is_mascot_enabled && context.is_voice_busy {
        return NotchView::Closed;
    }
    if context.is_lyrics_mode && context.has_lyrics && context.is_playing {
        return NotchView::Lyrics;
    }
    if context.is_minimal_notch || context.has_nearby_content {
        return NotchView::Minimal;
    }
    if context.is_mascot_enabled && context.is_quiet && !context.is_playing {
        return NotchView::Resting;
    }
    NotchView::Closed
}
