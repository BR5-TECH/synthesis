//! The Silero VAD model, through ONNX Runtime (VOP-FR-SMSX, VOP-FR-JXQG).
//!
//! The model is compiled into the executable, so detection reads no file and
//! works offline.

use ort::session::Session;
use ort::value::Tensor;

use super::detector::FRAME_SAMPLES;
use crate::onnx_runtime::shared_environment;

/// VOP-FR-JXQG: silero-vad v6.2.3, `silero_vad_16k_op15.onnx`. MIT licence, in
/// `resources/models/silero_vad.LICENSE`.
static MODEL: &[u8] = include_bytes!("../../../../resources/models/silero_vad_16k_op15.onnx");

const SAMPLE_RATE: i64 = 16_000;
/// The model reads the end of the previous frame before each frame.
const CONTEXT_SAMPLES: usize = 64;
/// The recurrent state: two layers, one batch, 128 values.
const STATE_SHAPE: [usize; 3] = [2, 1, 128];
const STATE_VALUES: usize = 2 * 128;

/// What gives the probability of speech in one frame. A trait, so a test scripts
/// an utterance with no model.
pub(crate) trait SpeechModel: Send {
    fn probability(&mut self, frame: &[f32]) -> Result<f32, String>;
}

pub(crate) struct SileroModel {
    session: Session,
    state: Vec<f32>,
    context: [f32; CONTEXT_SAMPLES],
}

impl SileroModel {
    pub(crate) fn load() -> Result<Self, String> {
        let environment = shared_environment()?;
        let session = Session::builder(&environment)
            .map_err(|e| e.to_string())?
            .with_intra_threads(1)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_memory(MODEL)
            .map_err(|e| e.to_string())?;
        Ok(Self { session, state: vec![0.0; STATE_VALUES], context: [0.0; CONTEXT_SAMPLES] })
    }
}

impl SpeechModel for SileroModel {
    fn probability(&mut self, frame: &[f32]) -> Result<f32, String> {
        if frame.len() != FRAME_SAMPLES {
            return Err(format!("a frame holds {} samples", frame.len()));
        }
        let mut input = Vec::with_capacity(CONTEXT_SAMPLES + FRAME_SAMPLES);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(frame);
        let audio = Tensor::from_array(([1, CONTEXT_SAMPLES + FRAME_SAMPLES], input))
            .map_err(|e| e.to_string())?;
        let state =
            Tensor::from_array((STATE_SHAPE, self.state.clone())).map_err(|e| e.to_string())?;
        let rate =
            Tensor::from_array(([0usize; 0], vec![SAMPLE_RATE])).map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs! { "input" => audio, "state" => state, "sr" => rate })
            .map_err(|e| e.to_string())?;
        let (_, probability) =
            outputs["output"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let probability = *probability.first().ok_or("the model gave no probability")?;
        let (_, next) = outputs["stateN"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        self.state.copy_from_slice(next);
        self.context.copy_from_slice(&frame[FRAME_SAMPLES - CONTEXT_SAMPLES..]);
        Ok(probability)
    }
}
