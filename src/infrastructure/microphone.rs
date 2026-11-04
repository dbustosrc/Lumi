//! Microphone capture with cpal. The cpal stream is not `Send`, so it lives on
//! a dedicated thread for the duration of each recording.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use webrtc_vad::{SampleRate, Vad, VadMode};

use crate::domain::ports::{AudioRecorder, PcmAudio, PortError};

/// Whisper models are trained on 16 kHz mono audio.
const TARGET_SAMPLE_RATE: u32 = 16_000;

type SharedBuffer = Arc<Mutex<Vec<f32>>>;
type RecordingResult = Result<(Vec<f32>, u32), PortError>;

struct ActiveRecording {
    stop: mpsc::Sender<()>,
    worker: JoinHandle<RecordingResult>,
}

/// Records from the default input device and returns 16 kHz mono PCM.
#[derive(Default)]
pub struct CpalAudioRecorder {
    active: Mutex<Option<ActiveRecording>>,
}

impl AudioRecorder for CpalAudioRecorder {
    fn start(&self) -> Result<(), PortError> {
        let mut active = lock(&self.active)?;
        if active.is_some() {
            return Err(PortError::new(
                "voice.error.alreadyRecording",
                "recording in progress",
            ));
        }
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let (stop, stop_receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("lumi-microphone".into())
            .spawn(move || record(&ready_sender, &stop_receiver))
            .map_err(|error| PortError::new("voice.error.microphone", error))?;
        match ready.recv() {
            Ok(Ok(())) => {
                *active = Some(ActiveRecording { stop, worker });
                Ok(())
            }
            Ok(Err(error)) => Err(error),
            Err(error) => Err(PortError::new("voice.error.microphone", error)),
        }
    }

    fn stop(&self) -> Result<PcmAudio, PortError> {
        let recording = lock(&self.active)?
            .take()
            .ok_or_else(|| PortError::new("voice.error.notRecording", "no active recording"))?;
        // A send error only means the worker already finished; join reports why.
        let _ = recording.stop.send(());
        let (samples, sample_rate) = recording
            .worker
            .join()
            .map_err(|_| PortError::new("voice.error.microphone", "recording thread panicked"))??;
        let audio = PcmAudio {
            samples: to_pcm16(&resample(&samples, sample_rate)),
            sample_rate: TARGET_SAMPLE_RATE,
        };
        require_speech(audio)
    }
}

fn require_speech(audio: PcmAudio) -> Result<PcmAudio, PortError> {
    if !contains_speech(&audio.samples)? {
        return Err(PortError::new(
            "voice.error.emptyTranscript",
            "no speech in microphone capture",
        ));
    }
    Ok(audio)
}

fn contains_speech(samples: &[i16]) -> Result<bool, PortError> {
    // WebRTC accepts 10/20/30 ms frames. Count 60 ms of consecutive speech
    // without treating its hangover through silence/DC bias as more speech.
    const FRAME_SAMPLES: usize = TARGET_SAMPLE_RATE as usize / 50;
    const MIN_SPEECH_FRAMES: usize = 3;
    let mut detector = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Aggressive);
    let mut speech_frames = 0;
    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        let classified = detector.is_voice_segment(frame).map_err(|()| {
            PortError::new("voice.error.microphone", "invalid voice activity frame")
        })?;
        let has_signal = frame.iter().any(|sample| *sample != frame[0]);
        if classified && has_signal {
            speech_frames += 1;
            if speech_frames >= MIN_SPEECH_FRAMES {
                return Ok(true);
            }
        } else {
            speech_frames = 0;
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "../../tests/fixtures/microphone.rs"]
mod tests;

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, PortError> {
    mutex
        .lock()
        .map_err(|error| PortError::new("voice.error.microphone", error))
}

fn record(ready: &SyncSender<Result<(), PortError>>, stop: &Receiver<()>) -> RecordingResult {
    let buffer: SharedBuffer = Arc::default();
    let started = open_stream(Arc::clone(&buffer));
    let (stream, sample_rate) = match started {
        Ok(started) => started,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let _ = ready.send(Ok(()));
    let _ = stop.recv();
    drop(stream);
    let samples = std::mem::take(&mut *lock(&buffer)?);
    Ok((samples, sample_rate))
}

fn open_stream(buffer: SharedBuffer) -> Result<(cpal::Stream, u32), PortError> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| PortError::new("voice.error.noMicrophone", "no default input device"))?;
    let config = device
        .default_input_config()
        .map_err(|error| PortError::new("voice.error.microphone", error))?;
    let channels = usize::from(config.channels());
    let sample_rate = config.sample_rate();
    let stream_config: cpal::StreamConfig = config.config();
    let stream = match config.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, channels, buffer),
        SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, channels, buffer),
        SampleFormat::I32 => build_stream::<i32>(&device, &stream_config, channels, buffer),
        SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, channels, buffer),
        format => Err(PortError::new("voice.error.unsupportedFormat", format)),
    }?;
    stream
        .play()
        .map_err(|error| PortError::new("voice.error.microphone", error))?;
    Ok((stream, sample_rate))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    buffer: SharedBuffer,
) -> Result<cpal::Stream, PortError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let on_data = move |data: &[T], _: &cpal::InputCallbackInfo| {
        let Ok(mut samples) = buffer.lock() else {
            return;
        };
        samples.extend(data.chunks(channels).map(|frame| {
            frame
                .iter()
                .map(|sample| sample.to_sample::<f32>())
                .sum::<f32>()
                / channels as f32
        }));
    };
    let on_error = |error: cpal::Error| {
        tracing::error!(event = "log.voice.microphoneStreamError", error = %error);
    };
    device
        .build_input_stream(*config, on_data, on_error, None)
        .map_err(|error| PortError::new("voice.error.microphone", error))
}

/// Linear interpolation is enough for speech recognition quality.
fn resample(samples: &[f32], source_rate: u32) -> Vec<f32> {
    if source_rate == TARGET_SAMPLE_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(source_rate) / f64::from(TARGET_SAMPLE_RATE);
    let output_len = (samples.len() as f64 / ratio) as usize;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let base = position as usize;
            let next = samples.get(base + 1).copied().unwrap_or(samples[base]);
            let weight = (position - base as f64) as f32;
            samples[base] * (1.0 - weight) + next * weight
        })
        .collect()
}

fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}
