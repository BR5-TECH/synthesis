//! PCM, WAV, resampling, and frames.

use crate::voice_ai::pcm::*;

fn sine(frequency: f64, rate: u32, seconds: f64) -> Vec<i16> {
    (0..(f64::from(rate) * seconds) as usize)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            ((t * frequency * std::f64::consts::TAU).sin() * 8000.0) as i16
        })
        .collect()
}

fn zero_crossings(samples: &[i16]) -> usize {
    samples.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count()
}

// VOP-FR-DXPK: raw bytes become samples, and a last odd byte is dropped.
#[test]
fn raw_bytes_become_samples() {
    assert_eq!(samples_of(&[1, 0, 0xFF, 0xFF, 9]), vec![1, -1]);
    assert_eq!(bytes_of(&[1, -1]), vec![1, 0, 0xFF, 0xFF]);
}

// VOP-FR-EYHP: a WAV file with a correct header for mono PCM16.
#[test]
fn a_wav_file_has_a_correct_header() {
    let wav = wav_of(&[0i16; 100], 16_000);
    assert_eq!(wav.len(), 244);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]), 236);
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
    assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 16_000);
    assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 200);
}

// VOP-FR-TSVV: maximum values in the sizes, as Ollama writes them, become
// the real length.
#[test]
fn riff_sizes_become_the_real_length() {
    let mut wav = wav_of(&[7i16; 10], 24_000);
    wav[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    wav[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    let fixed = fix_riff_sizes(wav.clone());
    assert_eq!(u32::from_le_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]), 56);
    assert_eq!(u32::from_le_bytes([fixed[40], fixed[41], fixed[42], fixed[43]]), 20);
    assert_eq!(&fixed[44..], &wav[44..], "the audio does not change");
}

// VOP-FR-TSVV: a `data` chunk after another chunk is found, and a body that
// is not a WAV file passes through.
#[test]
fn riff_rewrite_walks_chunks_and_ignores_other_bodies() {
    let plain = wav_of(&[1i16; 4], 24_000);
    let mut wav = plain[..36].to_vec();
    wav.extend_from_slice(b"LIST");
    wav.extend_from_slice(&3u32.to_le_bytes());
    wav.extend_from_slice(&[0, 0, 0, 0]);
    wav.extend_from_slice(&plain[36..]);
    let at = 36 + 12;
    wav[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    let fixed = fix_riff_sizes(wav);
    assert_eq!(u32::from_le_bytes([fixed[at + 4], fixed[at + 5], fixed[at + 6], fixed[at + 7]]), 8);
    assert_eq!(fix_riff_sizes(b"ID3 not a wav".to_vec()), b"ID3 not a wav");
}

// VOP-FR-GZOC: a whole recording keeps its length in time and its pitch.
#[test]
fn a_whole_recording_keeps_length_and_pitch() {
    let input = sine(440.0, 48_000, 2.0);
    let out = resample_all(&input, 48_000, 16_000).unwrap();
    let expected = 32_000usize;
    assert!(out.len().abs_diff(expected) < 64, "length {}", out.len());
    let crossings = zero_crossings(&out[1000..out.len() - 1000]);
    let seconds = (out.len() - 2000) as f64 / 16_000.0;
    let pitch = crossings as f64 / 2.0 / seconds;
    assert!((pitch - 440.0).abs() < 5.0, "pitch {pitch}");
}

// VOP-FR-GZOC: equal rates change nothing, and empty audio stays empty.
#[test]
fn equal_rates_and_empty_audio_pass_through() {
    assert_eq!(resample_all(&[1, 2, 3], 16_000, 16_000).unwrap(), vec![1, 2, 3]);
    assert!(resample_all(&[], 48_000, 16_000).unwrap().is_empty());
}

// VOP-FR-GZOC, VOP-FR-GYKP: chunks of any size give the audio of the whole.
#[test]
fn a_stream_of_uneven_chunks_keeps_length_and_pitch() {
    let input = sine(440.0, 48_000, 2.0);
    let mut resampler = StreamResampler::new(48_000, 24_000).unwrap();
    let mut out = Vec::new();
    let mut at = 0;
    for size in [1, 480, 4800, 7, 2048].iter().cycle() {
        if at >= input.len() {
            break;
        }
        let end = (at + size).min(input.len());
        out.extend(resampler.push(&input[at..end]).unwrap());
        at = end;
    }
    out.extend(resampler.finish().unwrap());
    let expected = 48_000usize;
    assert!(out.len().abs_diff(expected) < 1024, "length {}", out.len());
    let middle = &out[2000..out.len() - 2000];
    let pitch = zero_crossings(middle) as f64 / 2.0 / (middle.len() as f64 / 24_000.0);
    assert!((pitch - 440.0).abs() < 5.0, "pitch {pitch}");
}

// VOP-FR-GZOC: a stream at the route's own rate is not filtered.
#[test]
fn a_stream_at_the_route_rate_passes_through() {
    let mut resampler = StreamResampler::new(24_000, 24_000).unwrap();
    assert_eq!(resampler.push(&[5, 6]).unwrap(), vec![5, 6]);
    assert!(resampler.finish().unwrap().is_empty());
}

// VOP-FR-XZET: the kind, the little-endian rate, and the payload.
#[test]
fn a_frame_has_its_kind_rate_and_payload() {
    assert_eq!(frame(FRAME_PCM, 24_000, &[9, 8]), vec![0, 0xC0, 0x5D, 0, 0, 9, 8]);
    assert_eq!(frame(FRAME_FILE, 0, &[1]), vec![1, 0, 0, 0, 0, 1]);
}

// VOP-FR-XZET, VOP-FR-MOSY: an end frame has the header and no payload.
#[test]
fn a_segment_that_ends_sends_one_end_frame() {
    let mut frames = Vec::new();
    crate::voice_ai::commands::end_segment(&Ok(()), &mut |frame| frames.push(frame));
    assert_eq!(frames, vec![vec![2, 0, 0, 0, 0]]);
}

// VOP-FR-MOSY: a failed segment sends none.
#[test]
fn a_failed_segment_sends_no_end_frame() {
    let mut frames = Vec::new();
    let failed = Err("speech_failed".to_string());
    crate::voice_ai::commands::end_segment(&failed, &mut |frame| frames.push(frame));
    assert!(frames.is_empty());
}
