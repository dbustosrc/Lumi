use std::time::Duration;

/// A single time-synced lyric line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub starts_at: Duration,
    pub text: String,
}

/// Lyrics synchronized to the track timeline, sorted by start time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncedLyrics {
    lines: Vec<LyricLine>,
}

impl SyncedLyrics {
    /// Parses LRC formatted text (`[mm:ss.xx] text`). Lines may carry several
    /// timestamps; metadata tags such as `[ar:...]` are ignored.
    pub fn parse_lrc(source: &str) -> Self {
        let mut lines: Vec<LyricLine> = source.lines().flat_map(parse_lrc_line).collect();
        lines.sort_by_key(|line| line.starts_at);
        Self { lines }
    }

    pub fn lines(&self) -> &[LyricLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Index of the line being sung at `position`, if any has started.
    pub fn active_index(&self, position: Duration) -> Option<usize> {
        let upcoming = self
            .lines
            .partition_point(|line| line.starts_at <= position);
        upcoming.checked_sub(1)
    }
}

fn parse_lrc_line(raw: &str) -> Vec<LyricLine> {
    let mut timestamps = Vec::new();
    let mut rest = raw.trim();
    while let Some(stripped) = rest.strip_prefix('[') {
        let Some(end) = stripped.find(']') else { break };
        match parse_timestamp(&stripped[..end]) {
            Some(timestamp) => timestamps.push(timestamp),
            None => return Vec::new(),
        }
        rest = stripped[end + 1..].trim_start();
    }
    let text = rest.trim().to_owned();
    timestamps
        .into_iter()
        .map(|starts_at| LyricLine {
            starts_at,
            text: text.clone(),
        })
        .collect()
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (minutes, seconds) = tag.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let seconds: f64 = seconds.trim().parse().ok()?;
    if !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some(Duration::from_secs(minutes * 60) + Duration::from_secs_f64(seconds))
}
