//! The model that ships with the application. These tests run the real file
//! through the real runtime, so they find a wrong tensor name or shape.

use crate::voice_ai::activity::detector::FRAME_SAMPLES;
use crate::voice_ai::activity::silero::{SileroModel, SpeechModel};

// VOP-FR-SMSX, VOP-FR-JXQG: the embedded model loads with no file and no network,
// and silence is not speech.
#[test]
fn the_embedded_model_loads_and_silence_is_not_speech() {
    let mut model = SileroModel::load().unwrap();
    let silence = vec![0.0; FRAME_SAMPLES];
    for _ in 0..30 {
        let probability = model.probability(&silence).unwrap();
        assert!((0.0..0.2).contains(&probability), "{probability}");
    }
}

/// Three seconds of speech at 16 kHz, cut from `tests/data/test.wav` of
/// silero-vad v6.2.3 (MIT).
static SPEECH: &[u8] = include_bytes!("fixtures/speech-16k.pcm");

fn speech_frames() -> Vec<Vec<f32>> {
    SPEECH
        .chunks_exact(FRAME_SAMPLES * 2)
        .map(|frame| {
            frame
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                .collect()
        })
        .collect()
}

// VOP-FR-SMSX, VOP-FR-OORS: real speech passes the start threshold of each level.
#[test]
fn real_speech_passes_the_start_threshold() {
    let mut model = SileroModel::load().unwrap();
    let probabilities: Vec<f32> =
        speech_frames().iter().map(|f| model.probability(f).unwrap()).collect();
    let loud = probabilities.iter().filter(|p| **p >= 0.70).count();
    assert!(loud * 2 > probabilities.len(), "{loud} of {}", probabilities.len());
}

// VOP-FR-SMSX: the recurrent state and the context go from frame to frame, so
// one frame after speech differs from the same frame after silence.
#[test]
fn the_state_goes_from_frame_to_frame() {
    let frames = speech_frames();
    let probe = &frames[frames.len() / 2];

    let mut cold = SileroModel::load().unwrap();
    let first = cold.probability(probe).unwrap();

    let mut warm = SileroModel::load().unwrap();
    for frame in &frames[..frames.len() / 2] {
        warm.probability(frame).unwrap();
    }
    let after_speech = warm.probability(probe).unwrap();
    assert!((first - after_speech).abs() > 1e-4, "{first} {after_speech}");
}

// VOP-FR-SMSX: a frame of another size is refused, not padded.
#[test]
fn a_frame_of_another_size_is_refused() {
    let mut model = SileroModel::load().unwrap();
    assert!(model.probability(&[0.0; 10]).is_err());
}

// VOP-FR-SMSX: a second session works, as a reopened VAD session needs.
#[test]
fn a_second_model_loads_in_one_process() {
    let mut first = SileroModel::load().unwrap();
    let mut second = SileroModel::load().unwrap();
    let silence = vec![0.0; FRAME_SAMPLES];
    assert!(first.probability(&silence).is_ok());
    assert!(second.probability(&silence).is_ok());
}
