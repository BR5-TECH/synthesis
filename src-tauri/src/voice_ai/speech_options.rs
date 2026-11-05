//! The speed and the style of speech (VOP-FR-HQTQ, VOP-FR-STVA, VOP-FR-EMJY).

use super::pcm::frame;
use super::{SpeechRoute, SpeechSpeed, VoiceAiRecord};

/// VOP-FR-HQTQ: the range of a speed, in percent.
pub(crate) const SPEED_MIN_PERCENT: u16 = 50;
pub(crate) const SPEED_MAX_PERCENT: u16 = 200;
/// VOP-FR-HQTQ: the longest style, in characters.
pub(crate) const STYLE_LIMIT: usize = 200;
/// VOP-FR-XZET: the frame that gives the playback rate of a segment.
pub(crate) const FRAME_RATE: u8 = 3;

/// VOP-FR-HQTQ, AGR-FR-EPYX: a speed and a style that a record can hold.
pub(crate) fn valid_options(speed_percent: Option<u16>, style: &str) -> bool {
    speed_percent.is_none_or(|p| (SPEED_MIN_PERCENT..=SPEED_MAX_PERCENT).contains(&p))
        && style.chars().count() <= STYLE_LIMIT
}

/// VOP-FR-STVA: how one segment gets its speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpeedPlan {
    /// The speed that the request carries. `None` sends no speed.
    pub(crate) server: Option<f32>,
    /// The rate at which the WebView plays the audio.
    pub(crate) playback: f32,
}

/// VOP-FR-STVA: the speed range of a route, or `None` where the server does
/// not change the speed.
pub(crate) fn server_range(record: &VoiceAiRecord, route: SpeechRoute) -> Option<(f32, f32)> {
    let server_speed = record
        .capabilities
        .as_ref()
        .and_then(|c| c.speech_speed)
        .is_some_and(|s| s == SpeechSpeed::Server);
    match route {
        SpeechRoute::OpenaiSpeech if record.provider == "custom" && server_speed => {
            Some((0.25, 4.0))
        }
        SpeechRoute::Realtime if record.provider == "openai" => Some((0.25, 1.5)),
        _ => None,
    }
}

/// VOP-FR-STVA: the server speed is the requested speed in the range of the
/// route, and playback makes up the rest. A speed of 1 needs no field.
pub(crate) fn speed_plan(range: Option<(f32, f32)>, percent: u16) -> SpeedPlan {
    let requested = f32::from(percent) / 100.0;
    match range {
        Some((low, high)) => {
            let server = requested.clamp(low, high);
            SpeedPlan {
                server: ((server - 1.0).abs() > f32::EPSILON).then_some(server),
                playback: requested / server,
            }
        }
        None => SpeedPlan { server: None, playback: requested },
    }
}

/// The speed as the request carries it, to two decimals.
pub(crate) fn wire_speed(speed: f32) -> f64 {
    (f64::from(speed) * 100.0).round() / 100.0
}

/// VOP-FR-EMJY: the rate frame of a segment, or `None` at rate 1.
pub(crate) fn rate_frame(playback: f32) -> Option<Vec<u8>> {
    if (playback - 1.0).abs() < 0.001 {
        return None;
    }
    Some(frame(FRAME_RATE, (playback * 1000.0).round() as u32, &[]))
}

/// VOP-FR-COMJ, VOP-FR-TSVV: the style that a request carries, or `None`.
pub(crate) fn style_for<'a>(
    record: &'a VoiceAiRecord,
    requested: Option<&'a str>,
) -> Option<&'a str> {
    let takes = record.capabilities.as_ref().is_some_and(|c| c.speech_instructions);
    let style = requested.filter(|s| !s.trim().is_empty()).unwrap_or(&record.speech_style).trim();
    (takes && !style.is_empty()).then_some(style)
}

/// VOP-FR-TGIB: the length of a WAV in milliseconds, from its `fmt ` and
/// `data` chunks. `None` for bytes that are no WAV.
pub(crate) fn wav_duration_ms(wav: &[u8]) -> Option<u64> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12usize;
    let mut byte_rate: Option<u32> = None;
    while at + 8 <= wav.len() {
        let id = &wav[at..at + 4];
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().ok()?) as usize;
        if id == b"fmt " && at + 20 <= wav.len() {
            byte_rate = Some(u32::from_le_bytes(wav[at + 16..at + 20].try_into().ok()?));
        }
        if id == b"data" {
            let rate = u64::from(byte_rate.filter(|r| *r > 0)?);
            let size = size.min(wav.len().saturating_sub(at + 8)) as u64;
            return Some(size * 1000 / rate);
        }
        at = at.saturating_add(8).saturating_add(size + (size & 1));
    }
    None
}
