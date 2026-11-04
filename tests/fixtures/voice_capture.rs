use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::application::events::AppEvent;
use crate::application::presenter::VoiceState;
use crate::application::voice_capture::VoiceCapture;
use crate::domain::ports::{AudioRecorder, PcmAudio, PortError, PortFuture, SpeechToText};

struct EmptyCapture;

impl AudioRecorder for EmptyCapture {
    fn start(&self) -> Result<(), PortError> {
        Ok(())
    }

    fn stop(&self) -> Result<PcmAudio, PortError> {
        Err(PortError::new(
            "voice.error.emptyTranscript",
            "no speech in microphone capture",
        ))
    }
}

struct SpeechCallCounter(Arc<AtomicUsize>);

impl SpeechToText for SpeechCallCounter {
    fn transcribe(&self, _audio: PcmAudio) -> PortFuture<Result<String, PortError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(String::new()) })
    }
}

#[test]
fn voice_empty_capture_failure_does_not_call_the_speech_provider() {
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();
    let calls = Arc::new(AtomicUsize::new(0));
    let speech = Arc::new(SpeechCallCounter(Arc::clone(&calls)));
    let mut capture = VoiceCapture::new(Arc::new(EmptyCapture), speech, events);
    capture.configure(true);

    capture.toggle();
    assert_eq!(capture.state(), VoiceState::Recording);
    capture.toggle();

    assert_eq!(
        capture.state(),
        VoiceState::Failed("voice.error.emptyTranscript")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(received.try_recv().is_err());
}
