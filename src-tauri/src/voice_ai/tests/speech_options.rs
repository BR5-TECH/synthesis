//! The speed and the style of speech (VOP-FR-HQTQ .. VOP-FR-EMJY).

use serde_json::json;
use tokio::sync::watch;

use super::support::*;
use crate::voice_ai::pcm::{wav_of, FRAME_FILE};
use crate::voice_ai::registry::*;
use crate::voice_ai::speech::{speak_segment, SegmentVoice};
use crate::voice_ai::speech_options::*;
use crate::voice_ai::verify::{verify_integration, VerifyRequest};
use crate::voice_ai::*;

fn custom(base_url: &str, speed: Option<SpeechSpeed>, instructions: bool) -> ActiveVoice {
    let mut record = VoiceAiRecord::seeded(descriptor("custom").unwrap());
    record.base_url = base_url.to_string();
    record.tts_model = "tts".to_string();
    record.default_voice = "default-voice".to_string();
    record.speech_route = Some(SpeechRoute::OpenaiSpeech);
    record.capabilities = Some(VoiceCapabilities {
        openai_speech: true,
        speech_speed: speed,
        speech_instructions: instructions,
        ..Default::default()
    });
    ActiveVoice { record, key: None, language: None }
}

async fn speak_with(
    voice: &VoiceAi,
    config: &ActiveVoice,
    asked: SegmentVoice<'_>,
) -> (Result<(), String>, Vec<Vec<u8>>) {
    let (_stop, mut cancel) = watch::channel(false);
    let mut frames = Vec::new();
    let mut emit = |frame: Vec<u8>| frames.push(frame);
    let sink = RecordingSink::default();
    let result = speak_segment(voice, config, "Hello", asked, &mut cancel, &mut emit, &sink).await;
    (result, frames)
}

fn bodies(server: &Fake) -> Vec<serde_json::Value> {
    server.http.lock().unwrap().iter().map(|r| serde_json::from_slice(&r.body).unwrap()).collect()
}

fn rate_of(frame: &[u8]) -> u32 {
    u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]])
}

// VOP-FR-STVA: the server speed is the requested speed in the range of the
// route, and playback makes up the rest.
#[test]
fn the_speed_plan_splits_the_speed_between_server_and_playback() {
    let none = speed_plan(None, 150);
    assert_eq!(none, SpeedPlan { server: None, playback: 1.5 });
    let wide = speed_plan(Some((0.25, 4.0)), 150);
    assert_eq!(wide, SpeedPlan { server: Some(1.5), playback: 1.0 });
    let narrow = speed_plan(Some((0.25, 1.5)), 200);
    assert_eq!(narrow.server, Some(1.5));
    assert!((narrow.playback - 200.0 / 150.0).abs() < 0.001);
    assert_eq!(speed_plan(Some((0.25, 4.0)), 100), SpeedPlan { server: None, playback: 1.0 });
}

// VOP-FR-STVA: which route has a server speed.
#[test]
fn only_a_detected_server_speed_or_openai_realtime_has_a_range() {
    let base = custom("http://x", Some(SpeechSpeed::Server), false).record;
    assert_eq!(server_range(&base, SpeechRoute::OpenaiSpeech), Some((0.25, 4.0)));
    let client = custom("http://x", Some(SpeechSpeed::Client), false).record;
    assert_eq!(server_range(&client, SpeechRoute::OpenaiSpeech), None);
    let unknown = custom("http://x", None, false).record;
    assert_eq!(server_range(&unknown, SpeechRoute::OpenaiSpeech), None);
    assert_eq!(server_range(&base, SpeechRoute::LlamaTts), None);
    let openai = VoiceAiRecord::seeded(descriptor("openai").unwrap());
    assert_eq!(server_range(&openai, SpeechRoute::Realtime), Some((0.25, 1.5)));
}

// VOP-FR-XZET, VOP-FR-EMJY: a rate frame holds the rate times 1000, and a
// rate of 1 needs no frame.
#[test]
fn a_rate_frame_holds_the_rate_and_rate_one_has_none() {
    assert_eq!(rate_frame(1.0), None);
    let frame = rate_frame(1.5).unwrap();
    assert_eq!(frame[0], FRAME_RATE);
    assert_eq!(rate_of(&frame), 1500);
    assert_eq!(frame.len(), 5, "a rate frame has no payload");
}

// VOP-FR-HQTQ, AGR-FR-EPYX: the range of a speed and the length of a style.
#[test]
fn options_out_of_range_are_not_valid() {
    assert!(valid_options(None, ""));
    assert!(valid_options(Some(50), "calm"));
    assert!(valid_options(Some(200), &"x".repeat(200)));
    assert!(!valid_options(Some(49), ""));
    assert!(!valid_options(Some(201), ""));
    assert!(!valid_options(None, &"x".repeat(201)));
}

// VOP-FR-TGIB: the length of a WAV comes from its header.
#[test]
fn the_length_of_a_wav_comes_from_its_header() {
    assert_eq!(wav_duration_ms(&wav_of(&vec![0i16; 24_000], 24_000)), Some(1000));
    assert_eq!(wav_duration_ms(&wav_of(&vec![0i16; 12_000], 24_000)), Some(500));
    assert_eq!(wav_duration_ms(b"not a wav"), None);
}

// VOP-FR-TSVV, VOP-FR-STVA, VOP-FR-COMJ: a server that changes the speed and
// takes `instructions` gets both, and playback stays at rate 1.
#[tokio::test]
async fn a_capable_server_gets_the_speed_and_the_style() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000)), None).await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Server), true);
    config.record.speech_speed_percent = Some(150);
    config.record.speech_style = "calm".into();
    let (result, frames) = speak_with(&bench.voice, &config, SegmentVoice::default()).await;
    result.unwrap();
    let body = &bodies(&server)[0];
    assert_eq!(body["speed"], json!(1.5));
    assert_eq!(body["instructions"], "calm");
    assert_eq!(frames.len(), 1, "no rate frame at rate 1");
    assert_eq!(frames[0][0], FRAME_FILE);
}

// VOP-FR-STVA, VOP-FR-EMJY, VOP-FR-COMJ: a server that does not change the
// speed gets no speed and no style, and the rate frame comes first.
#[tokio::test]
async fn a_client_speed_sends_a_rate_frame_before_the_audio() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000)), None).await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Client), false);
    config.record.speech_speed_percent = Some(150);
    config.record.speech_style = "calm".into();
    let (result, frames) = speak_with(&bench.voice, &config, SegmentVoice::default()).await;
    result.unwrap();
    let body = &bodies(&server)[0];
    assert!(body.get("speed").is_none());
    assert!(body.get("instructions").is_none());
    assert_eq!(frames[0][0], FRAME_RATE);
    assert_eq!(rate_of(&frames[0]), 1500);
    assert_eq!(frames[1][0], FRAME_FILE);
}

// VOP-FR-RABZ: a 400 for a speed repeats once with neither the speed nor the
// style, and playback then carries the whole speed.
#[tokio::test]
async fn a_refused_speed_repeats_without_it_and_moves_to_playback() {
    let bench = Bench::new();
    let server = fake(
        |_, body| {
            let body: serde_json::Value = serde_json::from_slice(body).unwrap();
            if body.get("speed").is_some() {
                HttpReply::status(400)
            } else {
                HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000))
            }
        },
        None,
    )
    .await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Server), true);
    config.record.speech_style = "calm".into();
    let asked = SegmentVoice { speed_percent: Some(180), ..Default::default() };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    result.unwrap();
    let seen = bodies(&server);
    assert_eq!(seen.len(), 2);
    assert!(seen[1].get("speed").is_none() && seen[1].get("instructions").is_none());
    assert_eq!(seen[1]["voice"], "default-voice");
    assert_eq!(rate_of(&frames[0]), 1800);
    assert_eq!(frames[1][0], FRAME_FILE);
}

// VOM-FR-UALP, VOP-FR-STVA: the speed and the style of the agent come before
// the values of the provider.
#[tokio::test]
async fn the_agent_values_win_over_the_provider() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000)), None).await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Server), true);
    config.record.speech_speed_percent = Some(150);
    config.record.speech_style = "calm".into();
    let asked = SegmentVoice { name: None, speed_percent: Some(70), style: Some("bright") };
    speak_with(&bench.voice, &config, asked).await.0.unwrap();
    let body = &bodies(&server)[0];
    assert_eq!(body["speed"], json!(0.7));
    assert_eq!(body["instructions"], "bright");
}

// VOP-FR-UPRG, VOP-FR-STVA: OpenAI Realtime speech sets the speed in the
// session, and the style goes into the instructions.
#[tokio::test]
async fn openai_realtime_speech_carries_the_speed_and_the_style() {
    let bench = Bench::new();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(|_, mut socket| async move {
            send(
                &mut socket,
                json!({ "type": "session.created", "session": { "type": "realtime" } }),
            )
            .await;
            let update = recv_type(&mut socket, "session.update").await.unwrap();
            assert_eq!(update["session"]["audio"]["output"]["speed"], json!(1.2));
            let request = recv_type(&mut socket, "response.create").await.unwrap();
            let instructions = request["response"]["instructions"].as_str().unwrap().to_string();
            assert!(instructions.contains("Speak in this style: warm"), "{instructions}");
            send(&mut socket, json!({ "type": "response.done" })).await;
        }),
    )
    .await;
    let mut record = VoiceAiRecord::seeded(descriptor("openai").unwrap());
    record.base_url = format!("{}/v1", server.url);
    record.tts_model = "tts".into();
    record.speech_route = Some(SpeechRoute::Realtime);
    record.capabilities = Some(VoiceCapabilities {
        realtime_speech: true,
        speech_speed: Some(SpeechSpeed::Server),
        speech_instructions: true,
        ..Default::default()
    });
    record.speech_style = "warm".into();
    let config = ActiveVoice { record, key: Some("k".into()), language: None };
    let asked = SegmentVoice { speed_percent: Some(120), ..Default::default() };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    result.unwrap();
    assert!(frames.is_empty(), "no rate frame at rate 1");
}

fn verify_custom(base: &str) -> VerifyRequest<'_> {
    VerifyRequest {
        provider: "custom",
        base_url: base,
        api_key: None,
        stt_model: "stt",
        tts_model: "tts",
    }
}

/// A custom HTTP server whose speech answers with `answer(body)`.
async fn speech_server(
    answer: impl Fn(&serde_json::Value) -> HttpReply + Send + Sync + 'static,
) -> Fake {
    fake(
        move |path, body| match path {
            "/v1/audio/transcriptions" => transcribed(""),
            "/v1/audio/speech" => answer(&serde_json::from_slice(body).unwrap()),
            _ => HttpReply::status(404),
        },
        None,
    )
    .await
}

fn wav_ms(ms: usize) -> HttpReply {
    HttpReply::audio("audio/wav", wav_of(&vec![0i16; ms * 24], 24_000))
}

// VOP-FR-TGIB, VOP-FR-COMJ: a fast phrase that is shorter marks a server
// speed, and an answer to `instructions` marks a style.
#[tokio::test]
async fn a_shorter_fast_phrase_marks_a_server_speed() {
    let bench = Bench::new();
    let server = speech_server(|body| match body.get("speed") {
        Some(_) => wav_ms(400),
        None => wav_ms(1000),
    })
    .await;
    let record =
        verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    let capabilities = record.capabilities.unwrap();
    assert_eq!(capabilities.speech_speed, Some(SpeechSpeed::Server));
    assert!(capabilities.speech_instructions);
}

// VOP-FR-TGIB, VOP-FR-COMJ: a server that ignores `speed` marks a client
// speed, and a 400 for `instructions` marks no style.
#[tokio::test]
async fn an_ignored_speed_marks_a_client_speed() {
    let bench = Bench::new();
    let server = speech_server(|body| match body.get("instructions") {
        Some(_) => HttpReply::status(400),
        None => wav_ms(1000),
    })
    .await;
    let record =
        verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    let capabilities = record.capabilities.unwrap();
    assert_eq!(capabilities.speech_speed, Some(SpeechSpeed::Client));
    assert!(!capabilities.speech_instructions);
}

// VOP-FR-TGIB: a 400 for `speed` marks a client speed.
#[tokio::test]
async fn a_refused_speed_marks_a_client_speed() {
    let bench = Bench::new();
    let server = speech_server(|body| match body.get("speed") {
        Some(_) => HttpReply::status(400),
        None => wav_ms(1000),
    })
    .await;
    let record =
        verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    assert_eq!(record.capabilities.unwrap().speech_speed, Some(SpeechSpeed::Client));
}

// VOP-FR-HQTQ: the options save, a value out of range is refused, and a
// verification keeps them.
#[tokio::test]
async fn speech_options_save_and_survive_a_verification() {
    let bench = Bench::new();
    let server = speech_server(|_| wav_ms(1000)).await;
    verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    assert_eq!(
        set_speech_options(&bench.store, &bench.voice, "custom", Some(201), ""),
        Err(ERR_INVALID_SPEECH_OPTIONS.to_string()),
    );
    let saved =
        set_speech_options(&bench.store, &bench.voice, "custom", Some(130), " warm ").unwrap();
    assert_eq!(saved.speech_speed_percent, Some(130));
    assert_eq!(saved.speech_style, "warm");
    let again =
        verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    assert_eq!(again.speech_speed_percent, Some(130));
    assert_eq!(again.speech_style, "warm");
    let reset = set_speech_options(&bench.store, &bench.voice, "custom", Some(100), "").unwrap();
    assert_eq!(reset.speech_speed_percent, None, "100 is stored as no speed");
}

// VOP-FR-HQTQ, VOP-FR-SURK: a record written before the options existed loads
// with no speed and no style.
#[test]
fn an_old_record_loads_with_no_options() {
    let old = json!({ "provider": "custom", "baseUrl": "http://x", "defaultVoice": "v" });
    let record: VoiceAiRecord = serde_json::from_value(old).unwrap();
    assert_eq!(record.speech_speed_percent, None);
    assert_eq!(record.speech_style, "");
    let caps: VoiceCapabilities = serde_json::from_value(json!({ "openaiSpeech": true })).unwrap();
    assert_eq!(caps.speech_speed, None);
    assert!(!caps.speech_instructions);
}

// VOP-FR-TGIB, VOP-FR-COMJ: OpenAI Realtime speech has a server speed and a
// style by its descriptor, and no other provider gets them so.
#[test]
fn openai_realtime_speech_has_its_options_by_descriptor() {
    use crate::voice_ai::detect::descriptor_speech_options;
    let mut openai = VoiceCapabilities { realtime_speech: true, ..Default::default() };
    descriptor_speech_options("openai", &mut openai);
    assert_eq!(openai.speech_speed, Some(SpeechSpeed::Server));
    assert!(openai.speech_instructions);
    let mut silent = VoiceCapabilities::default();
    descriptor_speech_options("openai", &mut silent);
    assert_eq!(silent.speech_speed, None);
    let mut custom = VoiceCapabilities { realtime_speech: true, ..Default::default() };
    descriptor_speech_options("custom", &mut custom);
    assert_eq!(custom.speech_speed, None);
    assert!(!custom.speech_instructions);
}

// VOP-FR-TSVV, VOP-FR-STVA, VOP-FR-EMJY: `/tts` and OpenRouter take no speed
// and no style, and playback carries the speed.
#[tokio::test]
async fn llama_tts_and_openrouter_take_no_speed_and_no_style() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000)), None).await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Server), true);
    config.record.speech_route = Some(SpeechRoute::LlamaTts);
    config.record.speech_style = "calm".into();
    let asked = SegmentVoice { speed_percent: Some(150), ..Default::default() };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    result.unwrap();
    let body = &bodies(&server)[0];
    assert!(body.get("speed").is_none() && body.get("instructions").is_none(), "{body}");
    assert_eq!(rate_of(&frames[0]), 1500);

    let router = fake(|_, _| HttpReply::audio("audio/mpeg", vec![1, 2, 3]), None).await;
    let mut record = VoiceAiRecord::seeded(descriptor("openrouter").unwrap());
    record.base_url = format!("{}/api/v1", router.url);
    record.tts_model = "tts".into();
    record.speech_route = Some(SpeechRoute::OpenaiSpeech);
    record.capabilities = Some(VoiceCapabilities {
        openai_speech: true,
        speech_speed: Some(SpeechSpeed::Server),
        speech_instructions: true,
        ..Default::default()
    });
    record.speech_style = "calm".into();
    let config = ActiveVoice { record, key: Some("k".into()), language: None };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    result.unwrap();
    let body = &bodies(&router)[0];
    assert!(body.get("speed").is_none() && body.get("instructions").is_none(), "{body}");
    assert_eq!(frames[0][0], FRAME_RATE);
    assert_eq!(rate_of(&frames[0]), 1500);
}

// VOP-FR-RABZ: a 400 for a style alone repeats once with no style, and a
// second 400 fails with no frame.
#[tokio::test]
async fn a_refused_style_repeats_without_it_and_a_second_refusal_fails() {
    let bench = Bench::new();
    let server = fake(
        |_, body| {
            let body: serde_json::Value = serde_json::from_slice(body).unwrap();
            if body.get("instructions").is_some() {
                HttpReply::status(400)
            } else {
                HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000))
            }
        },
        None,
    )
    .await;
    let mut config = custom(&server.url, Some(SpeechSpeed::Server), true);
    config.record.speech_style = "calm".into();
    let (result, frames) = speak_with(&bench.voice, &config, SegmentVoice::default()).await;
    result.unwrap();
    assert_eq!(bodies(&server).len(), 2);
    assert!(bodies(&server)[1].get("instructions").is_none());
    assert_eq!(frames.len(), 1, "no rate frame at rate 1");
    assert_eq!(frames[0][0], FRAME_FILE);

    let refusing = fake(|_, _| HttpReply::status(400), None).await;
    let mut config = custom(&refusing.url, Some(SpeechSpeed::Server), true);
    config.record.speech_style = "calm".into();
    let asked = SegmentVoice { speed_percent: Some(150), ..Default::default() };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    assert!(result.is_err());
    assert_eq!(bodies(&refusing).len(), 2, "one repeat only");
    assert!(frames.is_empty(), "no frame for a failed segment");
}

// VOP-FR-STVA, VOP-FR-EMJY: on OpenAI Realtime, a speed over the range of the
// route goes partly to the server and the rest to playback, before the audio.
#[tokio::test]
async fn openai_realtime_speech_over_its_range_sends_a_rate_frame_first() {
    let bench = Bench::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
    let record_update = seen.clone();
    let server = fake(
        |_, _| HttpReply::status(404),
        ws_handler(move |_, mut socket| {
            let record_update = record_update.clone();
            async move {
                send(
                    &mut socket,
                    json!({ "type": "session.created", "session": { "type": "realtime" } }),
                )
                .await;
                let update = recv_type(&mut socket, "session.update").await.unwrap();
                *record_update.lock().unwrap() = Some(update);
                recv_type(&mut socket, "response.create").await.unwrap();
                send(
                    &mut socket,
                    json!({ "type": "response.output_audio.delta", "delta": "AQI=" }),
                )
                .await;
                send(&mut socket, json!({ "type": "response.done" })).await;
            }
        }),
    )
    .await;
    let mut record = VoiceAiRecord::seeded(descriptor("openai").unwrap());
    record.base_url = format!("{}/v1", server.url);
    record.tts_model = "tts".into();
    record.speech_route = Some(SpeechRoute::Realtime);
    let config = ActiveVoice { record, key: Some("k".into()), language: None };
    let asked = SegmentVoice { speed_percent: Some(200), ..Default::default() };
    let (result, frames) = speak_with(&bench.voice, &config, asked).await;
    result.unwrap();
    let update = seen.lock().unwrap().clone().unwrap();
    assert_eq!(update["session"]["audio"]["output"]["speed"], json!(1.5));
    assert_eq!(frames[0][0], FRAME_RATE);
    assert_eq!(rate_of(&frames[0]), 1333);
    assert_eq!(frames[1][0], crate::voice_ai::pcm::FRAME_PCM);
}

// VOP-FR-STVA: a stored speed out of range is clamped at speech time.
#[tokio::test]
async fn a_speed_out_of_range_is_clamped() {
    let bench = Bench::new();
    let server = fake(|_, _| HttpReply::audio("audio/wav", wav_of(&[1i16; 8], 24_000)), None).await;
    let config = custom(&server.url, Some(SpeechSpeed::Client), false);
    let asked = SegmentVoice { speed_percent: Some(1000), ..Default::default() };
    let (_, frames) = speak_with(&bench.voice, &config, asked).await;
    assert_eq!(rate_of(&frames[0]), 2000);
    let asked = SegmentVoice { speed_percent: Some(0), ..Default::default() };
    let (_, frames) = speak_with(&bench.voice, &config, asked).await;
    assert_eq!(rate_of(&frames[0]), 500);
}

// VOP-FR-TGIB: a fast phrase at exactly 75% of the length marks a server
// speed, and a longer one marks a client speed.
#[tokio::test]
async fn the_speed_probe_has_a_bound_of_three_quarters() {
    for (fast, expected) in [(750, SpeechSpeed::Server), (760, SpeechSpeed::Client)] {
        let bench = Bench::new();
        let server = speech_server(move |body| match body.get("speed") {
            Some(speed) => {
                assert_eq!(speed, &json!(2.0), "the probe asks for twice the speed");
                wav_ms(fast)
            }
            None => wav_ms(1000),
        })
        .await;
        let record = verify_integration(&bench.store, &bench.voice, verify_custom(&server.url))
            .await
            .unwrap();
        assert_eq!(record.capabilities.unwrap().speech_speed, Some(expected), "{fast} ms");
    }
}

// VOP-FR-HQTQ: a refused value leaves the stored record as it was.
#[tokio::test]
async fn a_refused_option_changes_nothing() {
    let bench = Bench::new();
    let server = speech_server(|_| wav_ms(1000)).await;
    verify_integration(&bench.store, &bench.voice, verify_custom(&server.url)).await.unwrap();
    set_speech_options(&bench.store, &bench.voice, "custom", Some(130), "warm").unwrap();
    assert!(set_speech_options(&bench.store, &bench.voice, "custom", Some(49), "x").is_err());
    let too_long = "x".repeat(201);
    assert!(set_speech_options(&bench.store, &bench.voice, "custom", None, &too_long).is_err());
    let padded = format!(" {} ", "x".repeat(200));
    assert!(set_speech_options(&bench.store, &bench.voice, "custom", None, &padded).is_ok());
    let (records, _) = bench.store.load_voice_ai_registry().unwrap();
    let custom = records.iter().find(|r| r.provider == "custom").unwrap();
    assert_eq!(custom.speech_style, "x".repeat(200));
    assert!(set_speech_options(&bench.store, &bench.voice, "nobody", None, "").is_err());
}
