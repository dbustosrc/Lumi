/// Separate enter/leave thresholds keep small contrast changes from reversing a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentSample {
    Clear,
    Uncertain,
    Detailed,
}

impl ContentSample {
    pub fn from_edges(edges: usize) -> Self {
        match edges {
            0..6 => Self::Clear,
            6..12 => Self::Uncertain,
            _ => Self::Detailed,
        }
    }

    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Detailed, _) | (_, Self::Detailed) => Self::Detailed,
            (Self::Uncertain, _) | (_, Self::Uncertain) => Self::Uncertain,
            _ => Self::Clear,
        }
    }
}

/// Confirms both appearance and disappearance with two consecutive decisive samples.
#[derive(Default)]
pub struct ContentStability {
    detected: bool,
    candidate: Option<bool>,
}

impl ContentStability {
    pub fn observe(&mut self, sample: Option<ContentSample>) -> Option<bool> {
        let sample = match sample {
            Some(ContentSample::Detailed) => true,
            Some(ContentSample::Clear) => false,
            Some(ContentSample::Uncertain) | None => {
                self.candidate = None;
                return None;
            }
        };
        if sample == self.detected {
            self.candidate = None;
        } else if self.candidate == Some(sample) {
            self.detected = sample;
            self.candidate = None;
            return Some(sample);
        } else {
            self.candidate = Some(sample);
        }
        None
    }

    /// A previous window's detection cannot apply to a new window or geometry.
    pub fn reset(&mut self) -> Option<bool> {
        let was_detected = self.detected;
        *self = Self::default();
        was_detected.then_some(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundObservation<C> {
    /// No reliable context could be read; this is not a window change.
    Unavailable,
    /// A known desktop/system/minimized window cannot contribute nearby content.
    Excluded,
    Window(C),
}

/// Owns the confirmed result and the context it belongs to; native probes do not reset it.
pub struct BackgroundDetection<C> {
    context: Option<C>,
    stability: ContentStability,
}

impl<C: PartialEq> Default for BackgroundDetection<C> {
    fn default() -> Self {
        Self {
            context: None,
            stability: ContentStability::default(),
        }
    }
}

impl<C: PartialEq> BackgroundDetection<C> {
    pub fn observe_context(&mut self, observation: BackgroundObservation<C>) -> Option<bool> {
        let next = match observation {
            BackgroundObservation::Unavailable => return self.stability.observe(None),
            BackgroundObservation::Excluded => None,
            BackgroundObservation::Window(context) => Some(context),
        };
        if self.context != next {
            self.context = next;
            self.stability.reset()
        } else {
            None
        }
    }

    pub fn observe_sample(&mut self, context: &C, sample: Option<ContentSample>) -> Option<bool> {
        if self.context.as_ref() != Some(context) {
            return None;
        }
        self.stability.observe(sample)
    }
}

/// BGRA pixels; alpha is irrelevant to visible contrast. Never compare across rows.
pub fn nearby_detail(pixels: &[[u8; 4]], width: usize) -> ContentSample {
    if width < 2 {
        return ContentSample::Uncertain;
    }
    let edges = pixels
        .chunks_exact(width)
        .flat_map(|row| row.windows(2))
        .filter(|pair| (0..3).any(|channel| pair[0][channel].abs_diff(pair[1][channel]) >= 14))
        .take(12)
        .count();
    ContentSample::from_edges(edges)
}
