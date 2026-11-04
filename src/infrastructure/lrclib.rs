use serde::Deserialize;

use crate::domain::lyrics::SyncedLyrics;
use crate::domain::media::Track;
use crate::domain::ports::{LyricsProvider, PortError, PortFuture};

const LRCLIB_GET_URL: &str = "https://lrclib.net/api/get";
const LRCLIB_SEARCH_URL: &str = "https://lrclib.net/api/search";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibRecord {
    synced_lyrics: Option<String>,
    duration: Option<f64>,
}

/// Retrieves synced lyrics from LRCLIB.
pub struct LrclibLyricsProvider {
    client: reqwest::Client,
}

impl LrclibLyricsProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl LyricsProvider for LrclibLyricsProvider {
    fn find(&self, track: Track) -> PortFuture<Result<Option<SyncedLyrics>, PortError>> {
        let client = self.client.clone();
        Box::pin(async move {
            if !track.album.trim().is_empty()
                && let Some(lyrics) = fetch_exact_match(&client, &track).await?
            {
                return Ok(Some(lyrics));
            }
            search_best_match(&client, &track).await
        })
    }
}

async fn fetch_exact_match(
    client: &reqwest::Client,
    track: &Track,
) -> Result<Option<SyncedLyrics>, PortError> {
    let duration_secs = track.duration.as_secs().to_string();
    let query = [
        ("track_name", track.title.as_str()),
        ("artist_name", track.artist.as_str()),
        ("album_name", track.album.as_str()),
        ("duration", duration_secs.as_str()),
    ];
    let response = client
        .get(LRCLIB_GET_URL)
        .query(&query)
        .send()
        .await
        .map_err(|error| PortError::new("lyrics.error.request", error))?;
    if matches!(
        response.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::BAD_REQUEST
    ) {
        return Ok(None);
    }
    let record: LrclibRecord = response
        .error_for_status()
        .map_err(|error| PortError::new("lyrics.error.request", error))?
        .json()
        .await
        .map_err(|error| PortError::new("lyrics.error.parse", error))?;
    Ok(parse_non_empty_lrc(record.synced_lyrics.as_deref()))
}

async fn search_best_match(
    client: &reqwest::Client,
    track: &Track,
) -> Result<Option<SyncedLyrics>, PortError> {
    let query = [
        ("track_name", track.title.as_str()),
        ("artist_name", track.artist.as_str()),
    ];
    let records: Vec<LrclibRecord> = client
        .get(LRCLIB_SEARCH_URL)
        .query(&query)
        .send()
        .await
        .map_err(|error| PortError::new("lyrics.error.request", error))?
        .error_for_status()
        .map_err(|error| PortError::new("lyrics.error.request", error))?
        .json()
        .await
        .map_err(|error| PortError::new("lyrics.error.parse", error))?;
    Ok(select_closest_synced(
        &records,
        track.duration.as_secs_f64(),
    ))
}

fn select_closest_synced(records: &[LrclibRecord], target_secs: f64) -> Option<SyncedLyrics> {
    records
        .iter()
        .filter_map(|rec| {
            let lyrics = parse_non_empty_lrc(rec.synced_lyrics.as_deref())?;
            let delta = rec
                .duration
                .filter(|_| target_secs > 0.0)
                .map_or(0.0, |dur| (dur - target_secs).abs());
            Some((delta, lyrics))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, lyrics)| lyrics)
}

fn parse_non_empty_lrc(raw: Option<&str>) -> Option<SyncedLyrics> {
    let parsed = SyncedLyrics::parse_lrc(raw?);
    (!parsed.is_empty()).then_some(parsed)
}
