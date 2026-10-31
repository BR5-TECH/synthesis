//! Realtime URLs, events, dialects, and the transcript.

use serde_json::json;

use crate::voice_ai::realtime::*;
use crate::voice_ai::Dialect;

// VOP-FR-CVXJ: the scheme and the path of each kind of provider.
#[test]
fn the_realtime_url_follows_the_base_url() {
    assert_eq!(
        realtime_url("openai", "https://api.openai.com/v1", &[]),
        "wss://api.openai.com/v1/realtime"
    );
    assert_eq!(
        realtime_url("custom", "http://localhost:8080", &[]),
        "ws://localhost:8080/v1/realtime"
    );
    assert_eq!(
        realtime_url("custom", "http://localhost:8080/", &[("a", "b")]),
        "ws://localhost:8080/v1/realtime?a=b"
    );
}

// VOP-FR-HAXX: `custom` sends the model, and `openai` does not.
#[test]
fn a_transcription_url_holds_a_model_only_on_custom() {
    assert_eq!(
        transcription_url("openai", "https://api.openai.com/v1", "gpt-live-transcribe"),
        "wss://api.openai.com/v1/realtime?intent=transcription"
    );
    assert_eq!(
        transcription_url("custom", "http://h:1", "org/model:Q4"),
        "ws://h:1/v1/realtime?intent=transcription&model=org%2Fmodel%3AQ4"
    );
    assert_eq!(
        transcription_url("custom", "http://h:1", ""),
        "ws://h:1/v1/realtime?intent=transcription"
    );
}

// VOP-FR-FHDE, VOP-FR-USQZ: the rates of `session.created`.
#[test]
fn session_created_gives_its_rates_and_type() {
    let event = json!({
        "type": "session.created",
        "session": { "type": "transcription", "audio": {
            "input": { "format": { "rate": 16000 } },
            "output": { "format": { "rate": 24000 } },
        }},
    });
    assert_eq!(
        parse_event(&event.to_string()),
        ServerEvent::SessionCreated(SessionInfo {
            session_type: Some("transcription".into()),
            input_rate: Some(16000),
            output_rate: Some(24000),
        })
    );
}

// VOP-FR-YRQN: the `vllm` events have the meaning of the shared ones.
#[test]
fn the_vllm_events_read_as_the_shared_events() {
    assert_eq!(
        parse_event(r#"{"type":"transcription.delta","delta":"he"}"#),
        ServerEvent::Delta { item_id: None, text: "he".into() }
    );
    assert_eq!(
        parse_event(r#"{"type":"transcription.done","text":"hello"}"#),
        ServerEvent::Completed { item_id: None, text: "hello".into() }
    );
}

// VOP-FR-AVXY: the code and the message of an `error` event, in both of the
// shapes that servers send.
#[test]
fn an_error_event_gives_its_code_and_message() {
    assert_eq!(
        parse_event(r#"{"type":"error","error":{"code":"x","message":"no model"}}"#),
        ServerEvent::Error { code: Some("x".into()), message: "no model".into() }
    );
    assert_eq!(
        parse_event(r#"{"type":"error","error":"plain text"}"#),
        ServerEvent::Error { code: None, message: "plain text".into() }
    );
}

// VOP-FR-TZDV: audio deltas decode, and bad input is not an event.
#[test]
fn audio_deltas_decode_and_bad_input_is_ignored() {
    assert_eq!(
        parse_event(r#"{"type":"response.output_audio.delta","delta":"AQI="}"#),
        ServerEvent::AudioDelta(vec![1, 2])
    );
    assert_eq!(
        parse_event(r#"{"type":"response.output_audio.delta","delta":"!"}"#),
        ServerEvent::Other
    );
    assert_eq!(parse_event("not json"), ServerEvent::Other);
    assert_eq!(parse_event(r#"{"type":"unknown"}"#), ServerEvent::Other);
}

// VOP-FR-TLGM: the provider and the shape of `session.created` select it.
#[test]
fn the_dialect_comes_from_the_provider_and_the_session() {
    let with_rate = SessionInfo { input_rate: Some(16000), ..SessionInfo::default() };
    let bare = SessionInfo::default();
    assert_eq!(dialect_of("openai", &bare), Dialect::Openai);
    assert_eq!(dialect_of("custom", &with_rate), Dialect::Llamacpp);
    assert_eq!(dialect_of("custom", &bare), Dialect::Vllm);
    assert_eq!(dialect_of("openrouter", &with_rate), Dialect::Llamacpp);
}

// VOP-FR-FHDE, VOP-FR-GDVD, VOP-FR-CSWH, VOP-FR-YRQN: the rate of each
// dialect. `openai` is fixed, whatever the session reports.
#[test]
fn each_dialect_has_its_input_rate() {
    let session = SessionInfo { input_rate: Some(16000), ..SessionInfo::default() };
    assert_eq!(input_rate_of(Dialect::Openai, &session), 24_000);
    assert_eq!(input_rate_of(Dialect::Llamacpp, &session), 16_000);
    assert_eq!(input_rate_of(Dialect::Vllm, &SessionInfo::default()), 16_000);
}

// VOP-FR-BTDW: only a stated type other than `transcription` is turn-based.
#[test]
fn only_a_stated_other_type_is_turn_based() {
    let of = |t: Option<&str>| SessionInfo {
        session_type: t.map(str::to_string),
        ..SessionInfo::default()
    };
    assert!(is_turn_based(&of(Some("realtime"))));
    assert!(!is_turn_based(&of(Some("transcription"))));
    assert!(!is_turn_based(&of(None)));
}

// VOP-FR-XXAB: a final text replaces its deltas, and items join on new lines.
#[test]
fn items_join_on_new_lines() {
    let mut t = Transcript::default();
    t.delta(Some("a"), "hel");
    t.delta(Some("a"), "lo");
    assert_eq!(t.text(), "hello");
    assert!(!t.all_completed());
    t.completed(Some("a"), "Hello.");
    t.delta(Some("b"), "again");
    assert_eq!(t.text(), "Hello.\nagain");
    t.completed(Some("b"), "Again.");
    assert!(t.all_completed());
    assert_eq!(t.text(), "Hello.\nAgain.");
}

// VOP-FR-XXAB: `completed` events out of order keep the commit order.
#[test]
fn commit_order_wins_over_arrival_order() {
    let mut t = Transcript::default();
    t.completed(Some("second"), "two");
    t.completed(Some("first"), "one");
    t.committed(Some("first"), None);
    t.committed(Some("second"), Some("first"));
    assert_eq!(t.text(), "one\ntwo");
}

// VOP-FR-XXAB: a dialect with no item id has one item.
#[test]
fn a_dialect_with_no_ids_has_one_item() {
    let mut t = Transcript::default();
    t.delta(None, "a");
    t.delta(None, "b");
    assert_eq!(t.text(), "ab");
    t.completed(None, "final");
    assert_eq!(t.text(), "final");
    assert!(t.all_completed());
}

// VOP-FR-XXAB: commit events out of order give the commit order.
#[test]
fn a_commit_chain_out_of_order_gives_the_commit_order() {
    let mut t = Transcript::default();
    t.completed(Some("c"), "three");
    t.committed(Some("c"), Some("b"));
    t.completed(Some("b"), "two");
    t.committed(Some("b"), Some("a"));
    t.completed(Some("a"), "one");
    t.committed(Some("a"), None);
    assert_eq!(t.text(), "one\ntwo\nthree");
}

// VOP-FR-XXAB: a loop in bad server data does not hang, and no text is lost.
#[test]
fn a_loop_in_the_commit_chain_keeps_the_text() {
    let mut t = Transcript::default();
    t.completed(Some("a"), "one");
    t.completed(Some("b"), "two");
    t.committed(Some("a"), Some("b"));
    t.committed(Some("b"), Some("a"));
    let text = t.text();
    assert!(text.contains("one") && text.contains("two"), "{text}");
}
