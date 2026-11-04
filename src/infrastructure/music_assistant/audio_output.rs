//! Real-time PCM audio output sink backed by `cpal` (WASAPI shared mode on Windows).
//! Pre-warms `IAudioClient` in paused state (`0%` idle CPU) and triggers playback on
//! the very first PCM chunk so sample `0:00.000` plays with sub-millisecond latency.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{DeviceId, FromSample, Sample, SampleFormat, SizedSample};

const MAX_BUFFERED_SECONDS: usize = 30;

type StereoQueue = Arc<Mutex<VecDeque<(f32, f32)>>>;

/// Format negotiated for incoming Sendspin PCM chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmStreamFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
}

struct OutputDeviceHandle {
    device_id: Option<DeviceId>,
    device_sample_rate: u32,
    is_invalidated: Arc<AtomicBool>,
    stream: cpal::Stream,
}

/// Pre-warmed audio output sink converting Sendspin PCM frames into the default
/// OS audio output device without cold-open delay or truncated track intros.
pub struct AudioOutputSink {
    queue: StereoQueue,
    volume: Arc<AtomicU8>,
    is_muted: Arc<AtomicBool>,
    stream_format: Option<PcmStreamFormat>,
    output: Option<OutputDeviceHandle>,
    is_stream_playing: bool,
}

impl AudioOutputSink {
    pub fn new() -> Self {
        let mut sink = Self {
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(96_000))),
            volume: Arc::new(AtomicU8::new(100)),
            is_muted: Arc::new(AtomicBool::new(false)),
            stream_format: None,
            output: None,
            is_stream_playing: false,
        };
        sink.ensure_warm_output();
        sink
    }

    pub fn volume(&self) -> u8 {
        self.volume.load(Ordering::Relaxed)
    }

    pub fn is_muted(&self) -> bool {
        self.is_muted.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume.min(100), Ordering::Relaxed);
    }

    pub fn set_muted(&self, is_muted: bool) {
        self.is_muted.store(is_muted, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.queue.lock() {
            guard.clear();
        }
    }

    pub fn start_stream(&mut self, format: PcmStreamFormat) {
        self.pause_active_stream();
        self.clear();
        self.stream_format = Some(format);
        self.ensure_warm_output();
    }

    pub fn stop_stream(&mut self) {
        self.pause_active_stream();
        self.clear();
        self.stream_format = None;
    }

    pub fn push_pcm(&mut self, payload: &[u8]) {
        let (Some(format), Some(out)) = (self.stream_format, self.output.as_ref()) else {
            return;
        };
        if out.is_invalidated.load(Ordering::Relaxed) {
            return;
        }
        let decoded = decode_stereo_frames(payload, format.channels, format.bit_depth);
        if decoded.is_empty() {
            return;
        }
        let frames = resample_stereo(&decoded, format.sample_rate, out.device_sample_rate);
        let max_frames = (out.device_sample_rate as usize) * MAX_BUFFERED_SECONDS;
        if let Ok(mut guard) = self.queue.lock() {
            let overflow = (guard.len() + frames.len()).saturating_sub(max_frames);
            if overflow > 0 {
                let drain_count = overflow.min(guard.len());
                guard.drain(..drain_count);
            }
            guard.extend(frames);
        }
        self.activate_stream_if_needed();
    }

    fn activate_stream_if_needed(&mut self) {
        if self.is_stream_playing {
            return;
        }
        let Some(out) = self.output.as_ref() else {
            return;
        };
        match out.stream.play() {
            Ok(()) => self.is_stream_playing = true,
            Err(error) => {
                out.is_invalidated.store(true, Ordering::Relaxed);
                tracing::error!(event = "log.music.audioStreamError", error = %error);
            }
        }
    }

    fn pause_active_stream(&mut self) {
        if !self.is_stream_playing {
            return;
        }
        self.is_stream_playing = false;
        if let Some(out) = self.output.as_ref()
            && let Err(error) = out.stream.pause()
        {
            out.is_invalidated.store(true, Ordering::Relaxed);
            tracing::error!(event = "log.music.audioStreamError", error = %error);
        }
    }

    fn ensure_warm_output(&mut self) {
        let Some(device) = cpal::default_host().default_output_device() else {
            self.is_stream_playing = false;
            self.output = None;
            self.clear();
            return;
        };
        let target_id = device.id().ok();
        let is_reusable = self.output.as_ref().is_some_and(|out| {
            !out.is_invalidated.load(Ordering::Relaxed) && out.device_id == target_id
        });
        if is_reusable {
            return;
        }
        self.is_stream_playing = false;
        self.output = None;
        self.clear();
        match open_paused_output(
            &device,
            target_id,
            Arc::clone(&self.queue),
            Arc::clone(&self.volume),
            Arc::clone(&self.is_muted),
        ) {
            Ok(handle) => self.output = Some(handle),
            Err(reason) => {
                tracing::error!(event = "log.music.audioOutputFailed", reason = %reason);
            }
        }
    }

    /// Called by the existing one-second Sendspin clock tick, never per PCM packet.
    pub fn refresh_output(&mut self) {
        self.ensure_warm_output();
    }
}

/// Queries the default Windows output device's preferred sample rate and stereo channel count.
pub fn query_preferred_pcm_format() -> (u32, u8) {
    let Some(device) = cpal::default_host().default_output_device() else {
        return (48_000, 2);
    };
    let Ok(config) = device.default_output_config() else {
        return (48_000, 2);
    };
    (config.sample_rate(), config.channels().clamp(1, 2) as u8)
}

fn open_paused_output(
    device: &cpal::Device,
    device_id: Option<DeviceId>,
    queue: StereoQueue,
    volume: Arc<AtomicU8>,
    is_muted: Arc<AtomicBool>,
) -> Result<OutputDeviceHandle, String> {
    let supported = device
        .default_output_config()
        .map_err(|err| err.to_string())?;
    let device_sample_rate = supported.sample_rate();
    let channels = usize::from(supported.channels());
    let config: cpal::StreamConfig = supported.config();
    let is_invalidated = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&is_invalidated);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => {
            build_out::<f32>(device, config, channels, queue, volume, is_muted, flag)
        }
        SampleFormat::I16 => {
            build_out::<i16>(device, config, channels, queue, volume, is_muted, flag)
        }
        SampleFormat::U16 => {
            build_out::<u16>(device, config, channels, queue, volume, is_muted, flag)
        }
        other => Err(format!("unsupported output sample format: {other:?}")),
    }?;
    let _ = stream.pause();
    Ok(OutputDeviceHandle {
        device_id,
        device_sample_rate,
        is_invalidated,
        stream,
    })
}

fn build_out<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    queue: StereoQueue,
    volume: Arc<AtomicU8>,
    is_muted: Arc<AtomicBool>,
    is_invalidated: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let on_data = move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
        fill_output_buffer(out, channels, &queue, &volume, &is_muted);
    };
    let on_err = move |error: cpal::Error| {
        is_invalidated.store(true, Ordering::Relaxed);
        tracing::error!(event = "log.music.audioStreamError", error = %error);
    };
    device
        .build_output_stream(config, on_data, on_err, None)
        .map_err(|err| err.to_string())
}

fn fill_output_buffer<T>(
    out: &mut [T],
    channels: usize,
    queue: &StereoQueue,
    volume: &AtomicU8,
    is_muted: &AtomicBool,
) where
    T: Sample + FromSample<f32>,
{
    if channels == 0 {
        return;
    }
    let gain = if is_muted.load(Ordering::Relaxed) {
        0.0
    } else {
        f32::from(volume.load(Ordering::Relaxed)) / 100.0
    };
    let Ok(mut guard) = queue.lock() else {
        out.fill(T::from_sample(0.0));
        return;
    };
    for frame in out.chunks_mut(channels) {
        let (left, right) = guard.pop_front().unwrap_or((0.0, 0.0));
        write_device_frame(frame, left * gain, right * gain);
    }
}

fn write_device_frame<T: Sample + FromSample<f32>>(frame: &mut [T], left: f32, right: f32) {
    if frame.len() == 1 {
        frame[0] = T::from_sample(((left + right) * 0.5).clamp(-1.0, 1.0));
        return;
    }
    frame[0] = T::from_sample(left.clamp(-1.0, 1.0));
    frame[1] = T::from_sample(right.clamp(-1.0, 1.0));
    for extra in &mut frame[2..] {
        *extra = T::from_sample(0.0);
    }
}

fn decode_stereo_frames(payload: &[u8], channels: u8, bit_depth: u8) -> Vec<(f32, f32)> {
    let ch = usize::from(channels.max(1));
    match bit_depth {
        16 => payload
            .chunks_exact(2 * ch)
            .map(|frame| {
                let left = pcm16_to_f32(&frame[0..2]);
                let right = if ch >= 2 {
                    pcm16_to_f32(&frame[2..4])
                } else {
                    left
                };
                (left, right)
            })
            .collect(),
        24 => payload
            .chunks_exact(3 * ch)
            .map(|frame| {
                let left = pcm24_to_f32(&frame[0..3]);
                let right = if ch >= 2 {
                    pcm24_to_f32(&frame[3..6])
                } else {
                    left
                };
                (left, right)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn pcm16_to_f32(bytes: &[u8]) -> f32 {
    f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0
}

fn pcm24_to_f32(bytes: &[u8]) -> f32 {
    let signed = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
    (signed as f32) / 8_388_608.0
}

fn resample_stereo(frames: &[(f32, f32)], src_rate: u32, dst_rate: u32) -> Vec<(f32, f32)> {
    if src_rate == dst_rate || src_rate == 0 || dst_rate == 0 || frames.is_empty() {
        return frames.to_vec();
    }
    let ratio = f64::from(src_rate) / f64::from(dst_rate);
    let out_len = ((frames.len() as f64) / ratio) as usize;
    (0..out_len)
        .map(|idx| {
            let pos = (idx as f64) * ratio;
            let base = (pos as usize).min(frames.len() - 1);
            let next = (base + 1).min(frames.len() - 1);
            let w = (pos - (base as f64)) as f32;
            let (l0, r0) = frames[base];
            let (l1, r1) = frames[next];
            (l0 + (l1 - l0) * w, r0 + (r1 - r0) * w)
        })
        .collect()
}
