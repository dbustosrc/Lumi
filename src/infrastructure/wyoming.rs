//! Minimal Wyoming protocol client for speech-to-text (e.g. wyoming-faster-whisper).
//! Wire format: a JSON header line, then optional `data_length` bytes of JSON
//! data and `payload_length` bytes of binary payload.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::watch;

use crate::application::settings::{AppSettings, WyomingSettings};
use crate::domain::ports::{PcmAudio, PortError, PortFuture, SpeechToText};

const PROTOCOL_VERSION: &str = "1.5.2";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(90);
const SAMPLES_PER_CHUNK: usize = 1024;
const SAMPLE_WIDTH_BYTES: u32 = 2;

/// Speech-to-text through a Wyoming server configured in the settings.
pub struct WyomingSpeechToText {
    settings: watch::Receiver<AppSettings>,
}

impl WyomingSpeechToText {
    pub fn new(settings: watch::Receiver<AppSettings>) -> Self {
        Self { settings }
    }
}

impl SpeechToText for WyomingSpeechToText {
    fn transcribe(&self, audio: PcmAudio) -> PortFuture<Result<String, PortError>> {
        let settings = self.settings.borrow().wyoming.clone();
        Box::pin(async move {
            tokio::time::timeout(TRANSCRIBE_TIMEOUT, transcribe(settings, audio))
                .await
                .map_err(|_| PortError::new("voice.error.timeout", "transcription timed out"))?
        })
    }
}

async fn transcribe(settings: WyomingSettings, audio: PcmAudio) -> Result<String, PortError> {
    let address = (settings.host.as_str(), settings.port);
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| PortError::new("voice.error.connect", "connection timed out"))?
        .map_err(|error| PortError::new("voice.error.connect", error))?;
    let (reader, mut writer) = stream.into_split();
    send_audio(&mut writer, &settings.language, &audio).await?;
    read_transcript(&mut BufReader::new(reader)).await
}

async fn send_audio(
    writer: &mut (impl AsyncWriteExt + Unpin),
    language: &str,
    audio: &PcmAudio,
) -> Result<(), PortError> {
    let format = json!({ "rate": audio.sample_rate, "width": SAMPLE_WIDTH_BYTES, "channels": 1 });
    write_event(writer, "transcribe", &json!({ "language": language }), &[]).await?;
    write_event(writer, "audio-start", &format, &[]).await?;
    for chunk in audio.samples.chunks(SAMPLES_PER_CHUNK) {
        let payload: Vec<u8> = chunk
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        write_event(writer, "audio-chunk", &format, &payload).await?;
    }
    write_event(writer, "audio-stop", &json!({}), &[]).await?;
    writer
        .flush()
        .await
        .map_err(|error| PortError::new("voice.error.send", error))
}

async fn write_event(
    writer: &mut (impl AsyncWriteExt + Unpin),
    event_type: &str,
    data: &Value,
    payload: &[u8],
) -> Result<(), PortError> {
    let data_bytes = match data.as_object() {
        Some(fields) if !fields.is_empty() => serde_json::to_vec(data).unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut header = json!({ "type": event_type, "version": PROTOCOL_VERSION });
    if !data_bytes.is_empty() {
        header["data_length"] = json!(data_bytes.len());
    }
    if !payload.is_empty() {
        header["payload_length"] = json!(payload.len());
    }
    let mut message = serde_json::to_vec(&header).unwrap_or_default();
    message.push(b'\n');
    message.extend_from_slice(&data_bytes);
    message.extend_from_slice(payload);
    writer
        .write_all(&message)
        .await
        .map_err(|error| PortError::new("voice.error.send", error))
}

async fn read_transcript(reader: &mut (impl AsyncBufReadExt + Unpin)) -> Result<String, PortError> {
    loop {
        let (event_type, data) = read_event(reader).await?;
        if event_type == "transcript" {
            let text = data.get("text").and_then(Value::as_str).unwrap_or_default();
            return Ok(text.to_owned());
        }
        tracing::debug!(event = "log.voice.wyomingEventIgnored", wyoming_type = %event_type);
    }
}

async fn read_event(
    reader: &mut (impl AsyncBufReadExt + Unpin),
) -> Result<(String, Value), PortError> {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .await
        .map_err(|error| PortError::new("voice.error.receive", error))?;
    if read == 0 {
        return Err(PortError::new(
            "voice.error.receive",
            "connection closed by server",
        ));
    }
    let header: Value = serde_json::from_str(&line)
        .map_err(|error| PortError::new("voice.error.protocol", error))?;
    let mut data = header.get("data").cloned().unwrap_or_else(|| json!({}));
    if let Some(extra) = read_block(reader, &header, "data_length").await? {
        let extra: Value = serde_json::from_slice(&extra)
            .map_err(|error| PortError::new("voice.error.protocol", error))?;
        merge_objects(&mut data, extra);
    }
    read_block(reader, &header, "payload_length").await?;
    let event_type = header
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok((event_type, data))
}

async fn read_block(
    reader: &mut (impl AsyncReadExt + Unpin),
    header: &Value,
    length_field: &str,
) -> Result<Option<Vec<u8>>, PortError> {
    let length = header
        .get(length_field)
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    if length == 0 {
        return Ok(None);
    }
    let mut buffer = vec![0; length];
    reader
        .read_exact(&mut buffer)
        .await
        .map_err(|error| PortError::new("voice.error.receive", error))?;
    Ok(Some(buffer))
}

fn merge_objects(target: &mut Value, extra: Value) {
    if let (Some(target), Value::Object(extra)) = (target.as_object_mut(), extra) {
        target.extend(extra);
    }
}
