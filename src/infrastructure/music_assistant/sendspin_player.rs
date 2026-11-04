//! Native Sendspin (`player@v1`) local audio player connecting through Music
//! Assistant's authenticated `/sendspin` WebSocket proxy.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::audio_output::{AudioOutputSink, PcmStreamFormat, query_preferred_pcm_format};
use super::protocol::sendspin_url;
use crate::application::settings::{AppSettings, MusicAssistantSettings};

const BINARY_PLAYER_AUDIO: u8 = 0x04;
const AUDIO_HEADER_BYTES: usize = 9;
// ~2.0s of 48kHz 16-bit stereo PCM: tight enough for instant start/skip without underruns.
const BUFFER_CAPACITY_BYTES: u32 = 384_000;
const MIN_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const CLOCK_SYNC_INTERVAL: Duration = Duration::from_secs(1);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Deterministic Sendspin `client_id` / Music Assistant `queue_id` for this PC.
pub fn local_player_id() -> String {
    let raw = std::env::var("COMPUTERNAME").unwrap_or_default();
    let slug: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .take(20)
        .collect();
    if slug.is_empty() {
        "lumi-local".into()
    } else {
        format!("lumi-{slug}")
    }
}

/// Supervises the local Sendspin audio player alongside the control WebSocket.
pub async fn supervise_sendspin_player(mut settings: watch::Receiver<AppSettings>) {
    let player_id = local_player_id();
    let mut retry_delay = MIN_RETRY_DELAY;
    loop {
        let config = settings.borrow_and_update().music_assistant.clone();
        if !is_ready_config(&config) {
            if !wait_for_ma_change(&mut settings, &config).await {
                return;
            }
            continue;
        }
        tokio::select! {
            result = run_player_session(&config, &player_id) => {
                if let Err(reason) = result {
                    tracing::warn!(event = "log.music.audioPlayerDisconnected", reason = %reason);
                }
            }
            changed = wait_for_ma_change(&mut settings, &config) => {
                if !changed { return; }
                retry_delay = MIN_RETRY_DELAY;
                continue;
            }
        }
        tokio::select! {
            () = tokio::time::sleep(retry_delay) => {
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
            }
            changed = wait_for_ma_change(&mut settings, &config) => {
                if !changed { return; }
                retry_delay = MIN_RETRY_DELAY;
            }
        }
    }
}

fn is_ready_config(config: &MusicAssistantSettings) -> bool {
    config.is_enabled && !config.server_url.trim().is_empty() && !config.token.trim().is_empty()
}

async fn wait_for_ma_change(
    settings: &mut watch::Receiver<AppSettings>,
    current: &MusicAssistantSettings,
) -> bool {
    while settings.changed().await.is_ok() {
        if settings.borrow().music_assistant != *current {
            return true;
        }
    }
    false
}

async fn run_player_session(
    config: &MusicAssistantSettings,
    player_id: &str,
) -> Result<(), String> {
    let mut socket = tokio::time::timeout(super::HANDSHAKE_TIMEOUT, async {
        let url = sendspin_url(&config.server_url);
        let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .map_err(|err| err.to_string())?;
        authenticate_proxy(&mut socket, config.token.trim(), player_id).await?;
        perform_sendspin_hello(&mut socket, player_id).await?;
        Ok::<_, String>(socket)
    })
    .await
    .map_err(|_| "Sendspin handshake timed out".to_owned())??;
    let mut sink = AudioOutputSink::new();
    send_player_state(&mut socket, &sink).await?;
    let epoch = Instant::now();
    send_clock_burst(&mut socket, epoch).await?;
    tracing::info!(event = "log.music.audioPlayerReady", player_id = %player_id);
    drive_player_loop(&mut socket, &mut sink, epoch).await
}

async fn authenticate_proxy(
    socket: &mut Socket,
    token: &str,
    player_id: &str,
) -> Result<(), String> {
    let auth = json!({ "type": "auth", "token": token, "client_id": player_id });
    socket
        .send(Message::text(auth.to_string()))
        .await
        .map_err(|err| err.to_string())?;
    let reply = read_next_text_json(socket).await?;
    if reply.get("type").and_then(Value::as_str) == Some("auth_ok") {
        Ok(())
    } else {
        Err("sendspin proxy rejected token".into())
    }
}

async fn perform_sendspin_hello(socket: &mut Socket, player_id: &str) -> Result<(), String> {
    let hello = build_client_hello(player_id);
    socket
        .send(Message::text(hello.to_string()))
        .await
        .map_err(|err| err.to_string())?;
    loop {
        let message = read_next_text_json(socket).await?;
        if message.get("type").and_then(Value::as_str) == Some("server/hello") {
            return Ok(());
        }
    }
}

fn build_client_hello(player_id: &str) -> Value {
    let (preferred_rate, preferred_ch) = query_preferred_pcm_format();
    json!({
        "type": "client/hello",
        "payload": {
            "client_id": player_id,
            "name": "Lumi",
            "version": 1,
            "supported_roles": ["player@v1"],
            "device_info": {
                "product_name": "Web Player",
                "manufacturer": "Music Assistant",
                "software_version": env!("CARGO_PKG_VERSION")
            },
            "player@v1_support": {
                "supported_formats": build_pcm_formats(preferred_rate, preferred_ch),
                "buffer_capacity": BUFFER_CAPACITY_BYTES,
                "supported_commands": ["volume", "mute"]
            }
        }
    })
}

fn build_pcm_formats(preferred_rate: u32, preferred_ch: u8) -> Vec<Value> {
    let mut specs = vec![(preferred_rate, preferred_ch), (48_000, 2), (44_100, 2)];
    specs.dedup();
    specs
        .into_iter()
        .map(|(rate, ch)| {
            json!({ "codec": "pcm", "channels": ch, "sample_rate": rate, "bit_depth": 16 })
        })
        .collect()
}

async fn send_clock_burst(socket: &mut Socket, epoch: Instant) -> Result<(), String> {
    for step in 0..4 {
        if step > 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        send_client_time(socket, epoch).await?;
    }
    Ok(())
}

async fn send_client_time(socket: &mut Socket, epoch: Instant) -> Result<(), String> {
    let micros = i64::try_from(epoch.elapsed().as_micros()).unwrap_or(i64::MAX);
    let msg = json!({ "type": "client/time", "payload": { "client_transmitted": micros } });
    socket
        .send(Message::text(msg.to_string()))
        .await
        .map_err(|err| err.to_string())
}

async fn send_player_state(socket: &mut Socket, sink: &AudioOutputSink) -> Result<(), String> {
    let msg = json!({
        "type": "client/state",
        "payload": {
            "state": "synchronized",
            "player": { "volume": sink.volume(), "muted": sink.is_muted() }
        }
    });
    socket
        .send(Message::text(msg.to_string()))
        .await
        .map_err(|err| err.to_string())
}

async fn drive_player_loop(
    socket: &mut Socket,
    sink: &mut AudioOutputSink,
    epoch: Instant,
) -> Result<(), String> {
    let mut ticker = tokio::time::interval(CLOCK_SYNC_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                sink.refresh_output();
                send_client_time(socket, epoch).await?;
            },
            frame = socket.next() => match frame {
                Some(Ok(message)) => {
                    if !handle_ws_frame(socket, sink, message).await? {
                        return Ok(());
                    }
                }
                Some(Err(err)) => return Err(err.to_string()),
                None => return Ok(()),
            }
        }
    }
}

async fn handle_ws_frame(
    socket: &mut Socket,
    sink: &mut AudioOutputSink,
    frame: Message,
) -> Result<bool, String> {
    match frame {
        Message::Binary(bytes) => {
            if bytes.len() > AUDIO_HEADER_BYTES && bytes[0] == BINARY_PLAYER_AUDIO {
                sink.push_pcm(&bytes[AUDIO_HEADER_BYTES..]);
            }
            Ok(true)
        }
        Message::Text(text) => {
            if let Ok(parsed) = serde_json::from_str::<Value>(text.as_str()) {
                handle_json_control(socket, sink, &parsed).await?;
            }
            Ok(true)
        }
        Message::Ping(payload) => {
            socket
                .send(Message::Pong(payload))
                .await
                .map_err(|err| err.to_string())?;
            Ok(true)
        }
        Message::Close(_) => Ok(false),
        _ => Ok(true),
    }
}

async fn handle_json_control(
    socket: &mut Socket,
    sink: &mut AudioOutputSink,
    msg: &Value,
) -> Result<(), String> {
    match msg.get("type").and_then(Value::as_str) {
        Some("stream/start") => {
            if let Some(format) = parse_stream_format(msg) {
                sink.start_stream(format);
            }
        }
        Some("stream/clear") => sink.clear(),
        Some("stream/end") => sink.stop_stream(),
        Some("server/command") if apply_player_command(sink, msg) => {
            send_player_state(socket, sink).await?;
        }
        _ => {}
    }
    Ok(())
}

fn parse_stream_format(msg: &Value) -> Option<PcmStreamFormat> {
    let player = msg.pointer("/payload/player")?;
    if player.get("codec").and_then(Value::as_str) != Some("pcm") {
        return None;
    }
    Some(PcmStreamFormat {
        sample_rate: u32::try_from(player.get("sample_rate")?.as_u64()?).ok()?,
        channels: u8::try_from(player.get("channels")?.as_u64()?).ok()?,
        bit_depth: u8::try_from(player.get("bit_depth")?.as_u64()?).ok()?,
    })
}

fn apply_player_command(sink: &AudioOutputSink, msg: &Value) -> bool {
    let Some(player) = msg.pointer("/payload/player") else {
        return false;
    };
    match player.get("command").and_then(Value::as_str) {
        Some("volume") => {
            if let Some(vol) = player.get("volume").and_then(Value::as_u64) {
                sink.set_volume(u8::try_from(vol.min(100)).unwrap_or(100));
                return true;
            }
        }
        Some("mute") => {
            if let Some(muted) = player.get("mute").and_then(Value::as_bool) {
                sink.set_muted(muted);
                return true;
            }
        }
        _ => {}
    }
    false
}

async fn read_next_text_json(socket: &mut Socket) -> Result<Value, String> {
    while let Some(frame) = socket.next().await {
        match frame.map_err(|err| err.to_string())? {
            Message::Text(text) => {
                return serde_json::from_str(text.as_str()).map_err(|err| err.to_string());
            }
            Message::Close(_) => return Err("closed during sendspin handshake".into()),
            _ => {}
        }
    }
    Err("eof during sendspin handshake".into())
}
