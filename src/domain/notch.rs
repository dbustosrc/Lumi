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
    Todo,
    Pomodoro,
    Water,
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
    Todo,
    Headphones,
    Charging,
    Pomodoro,
    Water,
}

impl NotchPage {
    pub const ALL: [NotchPage; 7] = [
        NotchPage::Music,
        NotchPage::Pomodoro,
        NotchPage::Water,
        NotchPage::Shelf,
        NotchPage::Todo,
        NotchPage::Headphones,
        NotchPage::Charging,
    ];

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|page| *page == self)
            .unwrap_or_default()
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
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
        return match context.page {
            NotchPage::Music if context.is_lyrics_mode => NotchView::Lyrics,
            NotchPage::Music => NotchView::Music,
            NotchPage::Shelf => NotchView::Shelf,
            NotchPage::Todo => NotchView::Todo,
            NotchPage::Headphones => NotchView::Headphones,
            NotchPage::Charging => NotchView::Charging,
            NotchPage::Pomodoro => NotchView::Pomodoro,
            NotchPage::Water => NotchView::Water,
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
