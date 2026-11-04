use std::sync::Arc;
use std::time::{Duration, Instant};

use super::events::{AppEvent, EventSender};
use super::presenter::VoiceState;
use crate::domain::ports::{AudioRecorder, PortError, SpeechToText};

/// How long a failure stays visible before returning to idle.
const FAILURE_VISIBLE_FOR: Duration = Duration::from_secs(4);

/// Voice-to-task use case: record from the microphone until stopped, transcribe
/// with the speech port and hand the text back through the event bus.
pub struct VoiceCapture {
    recorder: Arc<dyn AudioRecorder>,
    speech: Arc<dyn SpeechToText>,
    events: EventSender,
    state: VoiceState,
    failure_deadline: Option<Instant>,
    is_enabled: bool,
}

#[cfg(test)]
#[path = "../../tests/fixtures/voice_capture.rs"]
mod tests;

impl VoiceCapture {
    pub fn new(
        recorder: Arc<dyn AudioRecorder>,
        speech: Arc<dyn SpeechToText>,
        events: EventSender,
    ) -> Self {
        Self {
            recorder,
            speech,
            events,
            state: VoiceState::Idle,
            failure_deadline: None,
            is_enabled: false,
        }
    }

    pub fn configure(&mut self, is_enabled: bool) {
        self.is_enabled = is_enabled;
    }

    pub fn state(&self) -> VoiceState {
        self.state
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state, VoiceState::Recording | VoiceState::Transcribing)
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.failure_deadline
    }

    /// Starts or stops the recording depending on the current state.
    pub fn toggle(&mut self) {
        match self.state {
            VoiceState::Recording => self.finish_recording(),
            VoiceState::Transcribing => {}
            VoiceState::Idle | VoiceState::Failed(_) => self.start_recording(),
        }
    }

    /// Clears a visible failure banner once its display window expires.
    pub fn on_deadline(&mut self, now: Instant) {
        if self
            .failure_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            self.set_state(VoiceState::Idle, None);
        }
    }

    /// Returns the transcript to turn into a task, or records the failure.
    pub fn on_transcribed(&mut self, result: Result<String, PortError>) -> Option<String> {
        match result.map(|text| text.trim().to_owned()) {
            Ok(text) if !text.is_empty() => {
                self.set_state(VoiceState::Idle, None);
                Some(text)
            }
            Ok(_) => self.fail("voice.error.emptyTranscript"),
            Err(error) => {
                tracing::error!(event = "log.voice.transcriptionFailed", error = %error);
                self.fail(error.key)
            }
        }
    }

    fn start_recording(&mut self) {
        if !self.is_enabled {
            self.fail("voice.error.notConfigured");
            return;
        }
        match self.recorder.start() {
            Ok(()) => self.set_state(VoiceState::Recording, None),
            Err(error) => {
                tracing::error!(event = "log.voice.recordingStartFailed", error = %error);
                self.fail(error.key);
            }
        }
    }

    fn finish_recording(&mut self) {
        let audio = match self.recorder.stop() {
            Ok(audio) => audio,
            Err(error) => {
                if error.key != "voice.error.emptyTranscript" {
                    tracing::error!(event = "log.voice.recordingStopFailed", error = %error);
                }
                self.fail(error.key);
                return;
            }
        };
        self.set_state(VoiceState::Transcribing, None);
        let transcription = self.speech.transcribe(audio);
        let events = self.events.clone();
        tokio::spawn(async move {
            let result = transcription.await;
            if events.send(AppEvent::VoiceTranscribed(result)).is_err() {
                tracing::warn!(event = "log.events.receiverClosed");
            }
        });
    }

    fn fail(&mut self, key: &'static str) -> Option<String> {
        self.set_state(
            VoiceState::Failed(key),
            Some(Instant::now() + FAILURE_VISIBLE_FOR),
        );
        None
    }

    fn set_state(&mut self, state: VoiceState, failure_deadline: Option<Instant>) {
        self.state = state;
        self.failure_deadline = failure_deadline;
    }
}
