//! Translation of Music Assistant JSON payloads into domain entities.

use std::time::Duration;

use serde_json::Value;

use crate::domain::media::{PlaybackStatus, Track};

pub const MIN_SCHEMA_VERSION: u64 = 31;
const ARTWORK_REQUEST_SIZE: u32 = 256;

/// Information sent by the server right after the WebSocket handshake.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub schema_version: u64,
    pub base_url: String,
}

impl ServerInfo {
    pub fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            schema_version: value.get("schema_version")?.as_u64()?,
            base_url: value
                .get("base_url")?
                .as_str()?
                .trim_end_matches('/')
                .to_owned(),
        })
    }
}

/// The subset of a `PlayerQueue` the notch needs.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueState {
    pub queue_id: String,
    pub is_active: bool,
    pub status: PlaybackStatus,
    pub elapsed: Duration,
    pub track: Option<Track>,
}

pub fn parse_queue(value: &Value, server: &ServerInfo) -> Option<QueueState> {
    let status = match value.get("state").and_then(Value::as_str) {
        Some("playing") => PlaybackStatus::Playing,
        Some("paused") => PlaybackStatus::Paused,
        _ => PlaybackStatus::Idle,
    };
    Some(QueueState {
        queue_id: value.get("queue_id")?.as_str()?.to_owned(),
        is_active: value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status,
        elapsed: parse_seconds(value.get("elapsed_time")),
        track: value
            .get("current_item")
            .and_then(|item| parse_track(item, server)),
    })
}

pub fn parse_seconds(value: Option<&Value>) -> Duration {
    let seconds = value.and_then(Value::as_f64).unwrap_or(0.0);
    Duration::from_secs_f64(seconds.max(0.0))
}

fn parse_track(item: &Value, server: &ServerInfo) -> Option<Track> {
    let media = item.get("media_item");
    let title = media
        .and_then(|media| media.get("name"))
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)?
        .to_owned();
    Some(Track {
        id: item.get("queue_item_id")?.as_str()?.to_owned(),
        title,
        artist: media.map(join_artists).unwrap_or_default(),
        album: media
            .and_then(|media| media.pointer("/album/name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        duration: parse_seconds(item.get("duration")),
        artwork_url: find_image(item).and_then(|image| artwork_url(image, server)),
    })
}

fn join_artists(media: &Value) -> String {
    let Some(artists) = media.get("artists").and_then(Value::as_array) else {
        return String::new();
    };
    artists
        .iter()
        .filter_map(|artist| artist.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_image(item: &Value) -> Option<&Value> {
    item.get("image")
        .filter(|image| !image.is_null())
        .or_else(|| item.pointer("/media_item/metadata/images/0"))
        .or_else(|| item.pointer("/media_item/album/image"))
        .filter(|image| !image.is_null())
}

/// Mirrors `MusicAssistantClient.get_image_url` from the official client.
fn artwork_url(image: &Value, server: &ServerInfo) -> Option<String> {
    let path = image.get("path")?.as_str()?;
    if image
        .get("remotely_accessible")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some(path.to_owned());
    }
    let proxy_id = image.get("proxy_id").and_then(Value::as_str)?;
    Some(format!(
        "{}/imageproxy/{proxy_id}?size={ARTWORK_REQUEST_SIZE}",
        server.base_url
    ))
}

/// Converts the configured base URL into the control WebSocket endpoint.
pub fn websocket_url(server_url: &str) -> String {
    let base = normalize_ws_base(server_url);
    format!("{base}/ws")
}

/// Converts the configured base URL into the Sendspin audio proxy WebSocket endpoint.
pub fn sendspin_url(server_url: &str) -> String {
    let base = normalize_ws_base(server_url);
    format!("{base}/sendspin")
}

fn normalize_ws_base(server_url: &str) -> String {
    let trimmed = server_url.trim().trim_end_matches('/');
    let without_suffix = trimmed
        .strip_suffix("/ws")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    without_suffix
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}
