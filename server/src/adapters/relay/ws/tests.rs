//! The tests of the frame envelope.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//!
//! Envelope validation is a pure function of the frame bytes and the protocol
//! of the endpoint, so every case here is driven without a socket. The
//! behaviour of the three protocols over a real connection is
//! `tests/websocket.rs`.

use crate::adapters::relay::session::ReasonCode;
use crate::adapters::relay::ws::envelope::{
    self, Protocol, MAX_FIELD_CHARS, MAX_FRAME_BYTES, MAX_PROJECTS,
};

fn worker_frame(body: &str) -> String {
    format!(r#"{{"protocol":"worker","version":1,"type":"worker_hello","body":{body}}}"#)
}

// WSK-FR-VQPH, WSK-FR-WJIC, WSK-FR-QOZF: a well-formed envelope carries the
// five members the contract names, and a response copies the request
// identifier it answers.
#[test]
fn a_well_formed_envelope_reads_back_every_member() {
    let text = r#"{"protocol":"client","version":1,"type":"client_attach","request_id":"r-1","body":{"handle":"h-1","instance_id":"i-1"}}"#;
    let frame = envelope::parse(text, Protocol::Client).expect("the frame is well formed");

    assert_eq!(frame.protocol, Protocol::Client);
    assert_eq!(frame.frame_type, "client_attach");
    assert_eq!(frame.request_id.as_deref(), Some("r-1"));
    assert_eq!(frame.text("handle").expect("a member"), "h-1");
    assert_eq!(frame.text("instance_id").expect("a member"), "i-1");
    assert!(frame.only(&["handle", "instance_id"]).is_ok());
}

// WSK-FR-DMJT: exactly one UTF-8 JSON object. A trailing value, a document
// that is not an object, and a malformed document are each refused.
#[test]
fn only_one_json_object_is_a_frame() {
    for text in [
        r#"{"protocol":"worker","version":1,"type":"worker_hello","body":{}} {"a":1}"#,
        r#"[{"protocol":"worker","version":1,"type":"worker_hello","body":{}}]"#,
        "\"a string\"",
        "42",
        "",
        "{",
    ] {
        assert_eq!(
            envelope::parse(text, Protocol::Worker).expect_err("the frame is refused"),
            ReasonCode::InvalidFrameShape,
            "text {text}"
        );
    }
}

// WSK-FR-VQPH: a member the envelope does not define is refused.
#[test]
fn an_unknown_envelope_member_is_refused() {
    let text = r#"{"protocol":"worker","version":1,"type":"worker_hello","body":{},"extra":1}"#;
    assert_eq!(
        envelope::parse(text, Protocol::Worker).expect_err("the frame is refused"),
        ReasonCode::InvalidFrameShape
    );
}

// WSK-FR-CLYA, WSK-FR-ZWOE: the protocol must equal the endpoint's own, so a
// frame of one protocol is never read as a frame of another.
#[test]
fn a_frame_of_another_protocol_is_refused() {
    let text = r#"{"protocol":"registration","version":1,"type":"registration_start","body":{}}"#;
    for endpoint in [Protocol::Worker, Protocol::Client] {
        assert_eq!(
            envelope::parse(text, endpoint).expect_err("the frame is refused"),
            ReasonCode::ProtocolMismatch
        );
    }
    assert!(envelope::parse(text, Protocol::Registration).is_ok());

    // A protocol that is not one of the three is a mismatch as well.
    let unknown = r#"{"protocol":"admin","version":1,"type":"x","body":{}}"#;
    assert_eq!(
        envelope::parse(unknown, Protocol::Worker).expect_err("the frame is refused"),
        ReasonCode::ProtocolMismatch
    );
}

// WSK-FR-RUKN: V1 accepts the integer 1 alone.
#[test]
fn only_the_integer_version_one_is_accepted() {
    for version in ["2", "0", "-1", "1.5"] {
        let text = format!(r#"{{"protocol":"worker","version":{version},"type":"x","body":{{}}}}"#);
        assert_eq!(
            envelope::parse(&text, Protocol::Worker).expect_err("the version is refused"),
            ReasonCode::UnsupportedVersion,
            "version {version}"
        );
    }
    for version in ["\"1\"", "null", "true"] {
        let text = format!(r#"{{"protocol":"worker","version":{version},"type":"x","body":{{}}}}"#);
        assert_eq!(
            envelope::parse(&text, Protocol::Worker).expect_err("the version is refused"),
            ReasonCode::InvalidFrameShape,
            "version {version}"
        );
    }
}

// WSK-FR-EBTS, WSK-FR-WJIC, WSK-FR-QOZF: the type, the request identifier, and
// the body each hold the shape the envelope names.
#[test]
fn a_malformed_type_request_identifier_or_body_is_refused() {
    for text in [
        r#"{"protocol":"worker","version":1,"type":"","body":{}}"#,
        r#"{"protocol":"worker","version":1,"type":7,"body":{}}"#,
        r#"{"protocol":"worker","version":1,"body":{}}"#,
        r#"{"protocol":"worker","version":1,"type":"x","request_id":7,"body":{}}"#,
        r#"{"protocol":"worker","version":1,"type":"x","request_id":"","body":{}}"#,
        r#"{"protocol":"worker","version":1,"type":"x","body":[]}"#,
        r#"{"protocol":"worker","version":1,"type":"x"}"#,
    ] {
        assert_eq!(
            envelope::parse(text, Protocol::Worker).expect_err("the frame is refused"),
            ReasonCode::InvalidFrameShape,
            "text {text}"
        );
    }
}

// WSK-FR-AGVX, RSN-FR-CVUT, RSN-FR-VJIQ: an opaque value is an unpadded
// base64url string, and the relay reads the alphabet alone. It never decodes a
// payload, so what the payload holds reaches nothing here.
#[test]
fn an_opaque_payload_is_unpadded_base64url() {
    let accepted = worker_frame(r#"{"payload":"aGVsbG8-_9w"}"#);
    let frame = envelope::parse(&accepted, Protocol::Worker).expect("the frame is well formed");
    assert_eq!(frame.payload("payload").expect("a payload"), "aGVsbG8-_9w");

    for value in ["\"aGVsbG8=\"", "\"a+b/c\"", "\"\"", "7", "null"] {
        let text = worker_frame(&format!(r#"{{"payload":{value}}}"#));
        let frame = envelope::parse(&text, Protocol::Worker).expect("the envelope is well formed");
        assert_eq!(
            frame
                .payload("payload")
                .expect_err("the payload is refused"),
            ReasonCode::InvalidFrameShape,
            "value {value}"
        );
    }
}

// WSK-FR-QOZF: a body member the frame type does not define is refused.
#[test]
fn a_body_member_the_frame_type_does_not_define_is_refused() {
    let text = worker_frame(r#"{"handle":"h-1","surprise":1}"#);
    let frame = envelope::parse(&text, Protocol::Worker).expect("the envelope is well formed");
    assert_eq!(
        frame.only(&["handle"]).expect_err("the body is refused"),
        ReasonCode::InvalidFrameShape
    );
}

// RSN-FR-BPTL, RSN-FR-XNUH: a descriptor holds an opaque identifier and a
// display name, and two descriptors with one identifier are refused.
#[test]
fn an_announcement_refuses_a_duplicate_project_identifier() {
    let text = worker_frame(
        r#"{"projects":[{"project_id":"p1","display_name":"One"},{"project_id":"p1","display_name":"Two"}]}"#,
    );
    let frame = envelope::parse(&text, Protocol::Worker).expect("the envelope is well formed");
    assert_eq!(
        frame.projects().expect_err("the announcement is refused"),
        ReasonCode::InvalidFrameShape
    );

    let accepted = worker_frame(
        r#"{"projects":[{"project_id":"p1","display_name":"One"},{"project_id":"p2","display_name":"One"}]}"#,
    );
    let frame = envelope::parse(&accepted, Protocol::Worker).expect("the envelope is well formed");
    let projects = frame.projects().expect("the announcement is accepted");
    assert_eq!(projects.len(), 2);
    // The display name is presentation only, so two projects may share one.
    assert_eq!(projects[0].display_name, projects[1].display_name);
}

// RSN-FR-BPTL: a descriptor holds the two members alone, each bounded.
#[test]
fn a_malformed_project_descriptor_is_refused() {
    let long = "p".repeat(MAX_FIELD_CHARS + 1);
    for projects in [
        r#"[{"project_id":"p1"}]"#.to_string(),
        r#"[{"project_id":"p1","display_name":"One","extra":1}]"#.to_string(),
        r#"[{"project_id":"","display_name":"One"}]"#.to_string(),
        r#"[["p1","One"]]"#.to_string(),
        format!(r#"[{{"project_id":"{long}","display_name":"One"}}]"#),
    ] {
        let text = worker_frame(&format!(r#"{{"projects":{projects}}}"#));
        let frame = envelope::parse(&text, Protocol::Worker).expect("the envelope is well formed");
        assert_eq!(
            frame.projects().expect_err("the descriptor is refused"),
            ReasonCode::InvalidFrameShape,
            "projects {projects}"
        );
    }

    let many: Vec<String> = (0..=MAX_PROJECTS)
        .map(|index| format!(r#"{{"project_id":"p{index}","display_name":"One"}}"#))
        .collect();
    let text = worker_frame(&format!(r#"{{"projects":[{}]}}"#, many.join(",")));
    let frame = envelope::parse(&text, Protocol::Worker).expect("the envelope is well formed");
    assert_eq!(
        frame.projects().expect_err("the announcement is refused"),
        ReasonCode::InvalidFrameShape
    );
}

// WSK-FR-WVLC: a frame larger than the bound is refused before it is parsed.
#[test]
fn a_frame_over_the_bound_is_refused() {
    let payload = "a".repeat(MAX_FRAME_BYTES);
    let text = worker_frame(&format!(r#"{{"payload":"{payload}"}}"#));
    assert!(text.len() > MAX_FRAME_BYTES);
    assert_eq!(
        envelope::parse(&text, Protocol::Worker).expect_err("the frame is refused"),
        ReasonCode::InvalidFrameShape
    );
}

// WSK-FR-SYNB: the relay stamps the destination endpoint's protocol on the
// envelope it delivers, and carries the body it routes unchanged.
#[test]
fn an_outbound_frame_carries_the_destination_protocol() {
    let text = envelope::frame(
        Protocol::Client,
        "application_message",
        Some("r-9"),
        serde_json::json!({ "payload": "abc" }),
    );
    let value: serde_json::Value = serde_json::from_str(&text).expect("the frame is JSON");
    assert_eq!(value["protocol"], "client");
    assert_eq!(value["version"], 1);
    assert_eq!(value["type"], "application_message");
    assert_eq!(value["request_id"], "r-9");
    assert_eq!(value["body"]["payload"], "abc");

    // A notification carries no request identifier at all (WSK-FR-WJIC).
    let notification = envelope::frame(
        Protocol::Worker,
        "registration_start",
        None,
        serde_json::json!({}),
    );
    let value: serde_json::Value = serde_json::from_str(&notification).expect("the frame is JSON");
    assert!(value.get("request_id").is_none());
}

// WSK-FR-NQXD: the failure frame of the registration endpoint is named apart
// from the failure frame of the other two.
#[test]
fn each_endpoint_carries_its_own_failure_frame() {
    for (protocol, expected) in [
        (Protocol::Worker, "relay_failure"),
        (Protocol::Client, "relay_failure"),
        (Protocol::Registration, "registration_failed"),
    ] {
        let text = envelope::relay_failure(protocol, ReasonCode::WorkerNotFound, Some("r-1"));
        let value: serde_json::Value = serde_json::from_str(&text).expect("the frame is JSON");
        assert_eq!(value["type"], expected);
        assert_eq!(value["protocol"], protocol.as_str());
        assert_eq!(value["request_id"], "r-1");
        assert_eq!(value["body"]["reason"], "worker_not_found");
        // WSK-FR-SEBH: the failure names the reason and the request alone.
        assert_eq!(
            value["body"].as_object().expect("a body").len(),
            1,
            "the failure body holds the reason alone"
        );
    }
}

// WSK-FR-RCUY, WSK-FR-GZDU, RSN-FR-HZTV: a sink whose queue is full did not
// take the frame, so the caller is told rather than reporting a delivery that
// did not happen. A closed sink is the same answer.
#[tokio::test]
async fn a_full_or_closed_sink_reports_that_it_took_nothing() {
    use crate::adapters::relay::ws::hub::{deliver, Outbound};

    let (sink, mut receiver) = tokio::sync::mpsc::channel::<Outbound>(2);
    assert!(deliver(&sink, "one".to_string()));
    assert!(deliver(&sink, "two".to_string()));
    // The queue is full: the third frame is refused rather than held.
    assert!(!deliver(&sink, "three".to_string()));

    // Reading one frame makes room for exactly one more.
    let taken = receiver.recv().await.expect("a frame");
    assert!(matches!(taken, Outbound::Text(text) if text == "one"));
    assert!(deliver(&sink, "three".to_string()));

    // A sink whose connection has gone takes nothing at all.
    drop(receiver);
    assert!(!deliver(&sink, "four".to_string()));
}
