use super::{contains_speech, require_speech};
use crate::domain::ports::PcmAudio;

const SAMPLE_RATE: usize = 16_000;
const FRAME_SAMPLES: usize = SAMPLE_RATE / 50;

#[test]
fn voice_activity_rejects_silence_and_transient_noise_but_accepts_short_speech() {
    let silence = PcmAudio {
        samples: vec![0; SAMPLE_RATE / 4],
        sample_rate: SAMPLE_RATE as u32,
    };

    assert!(!contains_speech(&silence.samples).unwrap());
    let error = require_speech(silence).unwrap_err();
    assert_eq!(error.key, "voice.error.emptyTranscript");

    let mut samples = vec![0; FRAME_SAMPLES * 8];
    for (index, sample) in samples[FRAME_SAMPLES / 2..FRAME_SAMPLES / 2 + 64]
        .iter_mut()
        .enumerate()
    {
        *sample = if index % 2 == 0 { 18_000 } else { -18_000 };
    }

    assert!(!contains_speech(&samples).unwrap());

    let audio = PcmAudio {
        samples: speech_like_pcm(),
        sample_rate: SAMPLE_RATE as u32,
    };
    assert!(audio.samples.len() < SAMPLE_RATE);
    assert!(contains_speech(&audio.samples).unwrap());

    assert_eq!(require_speech(audio.clone()).unwrap(), audio);
}

fn speech_like_pcm() -> Vec<i16> {
    let duration = 0.24_f32;
    let sample_count = (duration * SAMPLE_RATE as f32) as usize;
    (0..sample_count)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            let pitch = 125. + 14. * (std::f32::consts::TAU * 3. * time).sin();
            let formant_one = 580. + 100. * (std::f32::consts::TAU * 2. * time).sin();
            let formant_two = 1_350. + 180. * (std::f32::consts::TAU * 1.5 * time).sin();
            let formant_three = 2_500. + 120. * (std::f32::consts::TAU * time).sin();
            let attack = (time / 0.018).clamp(0., 1.);
            let release = ((duration - time) / 0.025).clamp(0., 1.);
            let syllable = 0.58 + 0.24 * (std::f32::consts::TAU * 4. * time).sin();

            let mut voiced = 0.;
            for harmonic in 1..=24 {
                let frequency = harmonic as f32 * pitch;
                let width = 150. + frequency * 0.035;
                let spectral = (-((frequency - formant_one) / width).powi(2)).exp() * 0.9
                    + (-((frequency - formant_two) / (width * 1.4)).powi(2)).exp() * 0.7
                    + (-((frequency - formant_three) / (width * 1.8)).powi(2)).exp() * 0.4;
                voiced += spectral * (std::f32::consts::TAU * frequency * time).sin()
                    / (harmonic as f32).sqrt();
            }

            let sample =
                (voiced * attack * release * syllable * 0.3).clamp(-1., 1.) * f32::from(i16::MAX);
            sample as i16
        })
        .collect()
}
