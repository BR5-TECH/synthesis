//! Audio shapes: PCM16 bytes, resampling, and WAV (VOP-FR-GZOC, VOP-FR-EYHP,
//! VOP-FR-TSVV).

use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Indexing, Resampler};

/// The chunk that the streaming resampler takes, in input frames.
const CHUNK_FRAMES: usize = 1024;

/// VOP-FR-DXPK: little-endian PCM16 bytes as samples. A last odd byte is not
/// a sample, so it is dropped.
pub(crate) fn samples_of(bytes: &[u8]) -> Vec<i16> {
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

pub(crate) fn bytes_of(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn to_float(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|s| f32::from(*s) / 32768.0).collect()
}

fn to_pcm(samples: &[f32]) -> Vec<i16> {
    // `as` saturates, so a filter overshoot cannot wrap around.
    samples.iter().map(|s| (s * 32768.0) as i16).collect()
}

fn new_resampler(from: u32, to: u32) -> Result<Fft<f32>, String> {
    Fft::<f32>::new(from as usize, to as usize, CHUNK_FRAMES, 1, FixedSync::Input)
        .map_err(|e| e.to_string())
}

/// VOP-FR-GZOC: a whole recording, with the delay of the filter removed.
pub(crate) fn resample_all(samples: &[i16], from: u32, to: u32) -> Result<Vec<i16>, String> {
    if from == to || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let mut resampler = new_resampler(from, to)?;
    let input = vec![to_float(samples)];
    let adapter =
        SequentialSliceOfVecs::new(&input, 1, samples.len()).map_err(|e| e.to_string())?;
    let output = resampler.process_all(&adapter, samples.len(), None).map_err(|e| e.to_string())?;
    Ok(to_pcm(&output.take_data()))
}

/// VOP-FR-GYKP: the resampler of a live session. Audio arrives in chunks of
/// any size, so this holds what does not yet fill a chunk of the filter.
pub(crate) struct StreamResampler {
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    /// Output frames of the filter delay that are still to drop.
    skip: usize,
}

impl StreamResampler {
    pub(crate) fn new(from: u32, to: u32) -> Result<Self, String> {
        if from == to {
            return Ok(Self { inner: None, pending: Vec::new(), skip: 0 });
        }
        let inner = new_resampler(from, to)?;
        let skip = inner.output_delay();
        Ok(Self { inner: Some(inner), pending: Vec::new(), skip })
    }

    pub(crate) fn push(&mut self, samples: &[i16]) -> Result<Vec<i16>, String> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(samples.to_vec());
        };
        self.pending.extend(to_float(samples));
        let mut out = Vec::new();
        loop {
            let need = inner.input_frames_next();
            if self.pending.len() < need {
                break;
            }
            let chunk: Vec<Vec<f32>> = vec![self.pending.drain(..need).collect()];
            let adapter = SequentialSliceOfVecs::new(&chunk, 1, need).map_err(|e| e.to_string())?;
            let produced = inner.process(&adapter, None).map_err(|e| e.to_string())?;
            out.extend(produced.take_data());
        }
        Ok(self.drop_delay(out))
    }

    /// The end of input: the rest of the audio, with the filter drained.
    pub(crate) fn finish(&mut self) -> Result<Vec<i16>, String> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(Vec::new());
        };
        let rest = std::mem::take(&mut self.pending);
        let need = inner.input_frames_next();
        let mut padded = rest.clone();
        padded.resize(need.max(rest.len()), 0.0);
        let chunk = vec![padded];
        let adapter =
            SequentialSliceOfVecs::new(&chunk, 1, chunk[0].len()).map_err(|e| e.to_string())?;
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: Some(rest.len()),
            active_channels_mask: None,
        };
        let produced = inner.process(&adapter, Some(&indexing)).map_err(|e| e.to_string())?;
        Ok(self.drop_delay(produced.take_data()))
    }

    fn drop_delay(&mut self, mut out: Vec<f32>) -> Vec<i16> {
        let drop = self.skip.min(out.len());
        out.drain(..drop);
        self.skip -= drop;
        to_pcm(&out)
    }
}

/// VOP-FR-EYHP: a mono PCM16 WAV file.
pub(crate) fn wav_of(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&bytes_of(samples));
    wav
}

/// VOP-FR-TSVV: set the RIFF and `data` sizes to the real length. A server
/// that streams writes its header before it knows the length.
pub(crate) fn fix_riff_sizes(mut wav: Vec<u8>) -> Vec<u8> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return wav;
    }
    let total = wav.len();
    wav[4..8].copy_from_slice(&((total - 8) as u32).to_le_bytes());
    let mut at = 12;
    while at + 8 <= total {
        let size = u32::from_le_bytes([wav[at + 4], wav[at + 5], wav[at + 6], wav[at + 7]]);
        if &wav[at..at + 4] == b"data" {
            let real = (total - at - 8) as u32;
            wav[at + 4..at + 8].copy_from_slice(&real.to_le_bytes());
            return wav;
        }
        at = at.saturating_add(8).saturating_add(size as usize + (size as usize & 1));
    }
    wav
}

/// VOP-FR-XZET: one frame of a `Channel`.
pub(crate) const FRAME_PCM: u8 = 0;
pub(crate) const FRAME_FILE: u8 = 1;
/// VOP-FR-MOSY: a frame of 1 KB or more reaches the WebView after the command
/// resolves, so this frame, which comes last, shows that all audio arrived.
pub(crate) const FRAME_END: u8 = 2;

pub(crate) fn frame(kind: u8, rate: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(kind);
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(payload);
    out
}
