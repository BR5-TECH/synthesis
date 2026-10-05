//! The tests of the relay endpoint.
//!
//! Specification: `specifications/core/GSS-global-settings-storage.md`.

use std::sync::{Arc, Mutex};

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{BufferState, LogBuffer};

/// A sink that publishes nothing. The records still reach the buffer the test
/// hands in, which is what the assertions read.
#[derive(Clone, Default)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

/// The text of every record one buffer holds.
fn buffer_text(buffer: &LogBuffer) -> String {
    let page = buffer
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..crate::logging::LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the buffer answers");
    page.records
        .iter()
        .map(|record| format!("{record:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A probe that answers what a test hands it, and records what it was asked.
struct StubProbe {
    answer: Result<RelayHealth, String>,
    asked: Mutex<Vec<String>>,
}

impl StubProbe {
    fn new(answer: Result<RelayHealth, String>) -> Arc<StubProbe> {
        Arc::new(StubProbe {
            answer,
            asked: Mutex::new(Vec::new()),
        })
    }
}

impl RelayProbe for StubProbe {
    fn health(&self, health_url: &str) -> Result<RelayHealth, String> {
        self.asked
            .lock()
            .expect("the stub is not poisoned")
            .push(health_url.to_string());
        self.answer.clone()
    }
}

fn healthy() -> RelayHealth {
    RelayHealth {
        version: "v1.2.3".to_string(),
        capabilities: vec!["remote_session".to_string(), "websocket".to_string()],
    }
}

fn store() -> GlobalSettingsStore {
    GlobalSettingsStore::in_memory()
}

// GSS-FR-HPWE: the URL the author typed becomes the health route of
// `BMS-backend-microservice.md` BMS-FR-11, and a URL that cannot be one is
// refused with its own reason.
#[test]
fn the_health_route_is_derived_from_the_url_or_refused_with_its_own_reason() {
    assert_eq!(
        health_url("https://relay.example.com").expect("a route"),
        "https://relay.example.com/v1/health"
    );
    assert_eq!(
        health_url("  http://127.0.0.1:8080/  ").expect("a route"),
        "http://127.0.0.1:8080/v1/health"
    );
    // A base path is kept, so a relay behind a prefix is verified where it
    // answers. A query and a fragment name no part of a route.
    assert_eq!(
        health_url("https://relay.example.com/base?x=1#y").expect("a route"),
        "https://relay.example.com/base/v1/health"
    );
    assert_eq!(
        health_url("https://relay.example.com/base/").expect("a route"),
        "https://relay.example.com/base/v1/health"
    );

    assert_eq!(health_url("").expect_err("empty"), ERR_ENDPOINT_EMPTY);
    assert_eq!(health_url("   ").expect_err("empty"), ERR_ENDPOINT_EMPTY);
    assert_eq!(
        health_url("relay.example.com").expect_err("no scheme"),
        ERR_ENDPOINT_INVALID
    );
    assert_eq!(
        health_url("https:// relay.example.com").expect_err("white space"),
        ERR_ENDPOINT_INVALID
    );
    assert_eq!(
        health_url("https:///v1").expect_err("no authority"),
        ERR_ENDPOINT_INVALID
    );
    // GSS-FR-VMRB: a URL that carries credentials is refused rather than sent.
    assert_eq!(
        health_url("https://user:secret@relay.example.com").expect_err("credentials"),
        ERR_ENDPOINT_INVALID
    );
    for scheme in ["ws://relay.example.com", "ftp://relay", "file:///tmp"] {
        assert_eq!(
            health_url(scheme).expect_err("the scheme is refused"),
            ERR_SCHEME_UNSUPPORTED,
            "url {scheme}"
        );
    }
}

// GSS-FR-HPWE: an answer that is not the health object is not a relay's.
#[test]
fn an_answer_that_is_not_the_health_object_is_not_a_relay() {
    let parsed = parse_health(r#"{"version":"v1","capabilities":["websocket"]}"#)
        .expect("the answer is a health object");
    assert_eq!(parsed.version, "v1");
    assert_eq!(parsed.capabilities, vec!["websocket".to_string()]);

    for body in [
        "",
        "not json",
        "{}",
        r#"{"version":"v1"}"#,
        r#"{"capabilities":[]}"#,
        r#"{"version":7,"capabilities":[]}"#,
        r#"[{"version":"v1","capabilities":[]}]"#,
    ] {
        assert_eq!(
            parse_health(body).expect_err("the answer is refused"),
            ERR_NOT_A_RELAY,
            "body {body}"
        );
    }
}

// GSS-FR-HPWE, RSN-FR-VZKP: verification succeeds only for a relay that
// advertises both capabilities this application needs, and an identifier the
// application does not know is not treated as one of them.
#[test]
fn a_relay_that_lacks_a_capability_is_refused() {
    assert_eq!(missing_capability(&[]), Some("remote_session"));
    assert_eq!(
        missing_capability(&["remote_session".to_string()]),
        Some("websocket")
    );
    assert_eq!(
        missing_capability(&["remote_session".to_string(), "websocket".to_string()]),
        None
    );
    // An identifier the application does not know changes nothing.
    assert_eq!(
        missing_capability(&[
            "remote_session".to_string(),
            "websocket".to_string(),
            "something_else".to_string(),
        ]),
        None
    );
}

// GSS-FR-ZKQT, GSS-FR-NLDC: a fresh store holds no URL, a saved URL reads
// unverified, and a success binds to the URL that earned it.
#[test]
fn the_validation_state_follows_the_url_it_was_earned_against() {
    let store = store();
    let sink = Silent;
    static BUFFER: LogBuffer = LogBuffer::new();

    let initial = store.load_relay_endpoint().expect("a record").outbound();
    assert_eq!(initial.state, RelayEndpointState::Unset);
    assert_eq!(initial.url, None);

    let saved = save_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        "https://relay.example.com".to_string(),
    )
    .expect("the URL is saved");
    assert_eq!(saved.state, RelayEndpointState::Unverified);
    assert_eq!(saved.url.as_deref(), Some("https://relay.example.com"));

    let probe = StubProbe::new(Ok(healthy()));
    let verified = verify_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        probe.as_ref(),
        "https://relay.example.com".to_string(),
    )
    .expect("the relay answered");
    assert_eq!(verified.state, RelayEndpointState::Verified);
    assert_eq!(verified.version.as_deref(), Some("v1.2.3"));
    assert_eq!(
        verified.capabilities,
        vec!["remote_session".to_string(), "websocket".to_string()]
    );
    assert_eq!(
        probe.asked.lock().expect("the stub").as_slice(),
        ["https://relay.example.com/v1/health"]
    );

    // Re-reading returns the same success without probing again.
    assert_eq!(
        store
            .load_relay_endpoint()
            .expect("a record")
            .outbound()
            .state,
        RelayEndpointState::Verified
    );

    // GSS-FR-NLDC: a save that changes the URL returns the record to unverified.
    let moved = save_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        "https://other.example.com".to_string(),
    )
    .expect("the URL is saved");
    assert_eq!(moved.state, RelayEndpointState::Unverified);
    assert_eq!(moved.version, None);
    assert!(moved.capabilities.is_empty());

    // A save that changes nothing keeps the success it had.
    save_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        "https://relay.example.com".to_string(),
    )
    .expect("the URL is saved");
    verify_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        StubProbe::new(Ok(healthy())).as_ref(),
        "https://relay.example.com".to_string(),
    )
    .expect("the relay answered");
    let unchanged = save_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        "  https://relay.example.com  ".to_string(),
    )
    .expect("the URL is saved");
    assert_eq!(unchanged.state, RelayEndpointState::Verified);
}

// GSS-FR-HPWE: every refusal keeps its own reason, and a refused verification
// persists nothing.
#[test]
fn a_refused_verification_persists_nothing_and_keeps_its_reason() {
    let sink = Silent;
    static BUFFER: LogBuffer = LogBuffer::new();

    let cases: Vec<(Result<RelayHealth, String>, &str, &str)> = vec![
        (Ok(healthy()), "", ERR_ENDPOINT_EMPTY),
        (Ok(healthy()), "relay.example.com", ERR_ENDPOINT_INVALID),
        (
            Ok(healthy()),
            "ws://relay.example.com",
            ERR_SCHEME_UNSUPPORTED,
        ),
        (
            Err(ERR_UNREACHABLE.to_string()),
            "https://relay.example.com",
            ERR_UNREACHABLE,
        ),
        (
            Err(ERR_TIMED_OUT.to_string()),
            "https://relay.example.com",
            ERR_TIMED_OUT,
        ),
        (
            Err(ERR_NOT_A_RELAY.to_string()),
            "https://relay.example.com",
            ERR_NOT_A_RELAY,
        ),
        (
            Ok(RelayHealth {
                version: "v1".to_string(),
                capabilities: vec!["websocket".to_string()],
            }),
            "https://relay.example.com",
            ERR_CAPABILITY_MISSING,
        ),
    ];

    for (answer, url, expected) in cases {
        let store = store();
        let refusal = verify_relay_endpoint_with(
            &sink,
            &BUFFER,
            &store,
            StubProbe::new(answer).as_ref(),
            url.to_string(),
        )
        .expect_err("the verification is refused");
        assert_eq!(refusal, expected, "url {url}");
        assert_eq!(
            store
                .load_relay_endpoint()
                .expect("a record")
                .outbound()
                .state,
            RelayEndpointState::Unset,
            "url {url} wrote a record"
        );
    }
}

// GSS-FR-VMRB: no record and no log holds a token, a credential, or the URL
// itself.
#[test]
fn no_record_and_no_log_holds_a_credential_or_the_url() {
    let store = store();
    let sink = Silent;
    static BUFFER: LogBuffer = LogBuffer::new();

    save_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        "https://relay.example.com".to_string(),
    )
    .expect("the URL is saved");
    verify_relay_endpoint_with(
        &sink,
        &BUFFER,
        &store,
        StubProbe::new(Ok(healthy())).as_ref(),
        "https://relay.example.com".to_string(),
    )
    .expect("the relay answered");

    let text = buffer_text(&BUFFER);
    assert!(
        !text.contains("relay.example.com"),
        "a record named the endpoint: {text}"
    );
    assert!(!text.contains("Bearer"), "a record named a credential");

    let record = store.load_relay_endpoint().expect("a record");
    let serialised = toml::to_string(&record).expect("the record serialises");
    assert!(!serialised.to_lowercase().contains("token"), "{serialised}");
    assert!(
        !serialised.to_lowercase().contains("bearer"),
        "{serialised}"
    );
}

// GSS-FR-TXAO: this module reaches the relay in `verify_relay_endpoint` alone.
// It opens no WebSocket and holds no session — the session and frame contracts
// belong to the server. Asserted against the source, because the absence of a
// capability is not something a call can demonstrate.
#[test]
fn the_module_holds_no_transport_of_its_own() {
    // The comments are stripped first: this module *documents* the token it
    // must never hold, and a guard that read the prose would fail on the
    // sentence that states the rule.
    let source: String = include_str!("../relay_endpoint.rs")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "tungstenite",
        "WebSocket",
        "ws://",
        "wss://",
        "spawn",
        "Authorization",
        "Bearer",
        "SYNTHESIS_SERVER_TOKEN",
    ] {
        assert!(
            !source.contains(forbidden),
            "relay_endpoint.rs holds `{forbidden}`, which GSS-FR-TXAO and GSS-FR-VMRB forbid"
        );
    }
    // One outbound call, in the production probe alone.
    assert_eq!(source.matches(".get(health_url)").count(), 1);
}

// GSS-FR-ZKQT, GSS-FR-NLDC: the record survives a write and a read of
// `synthesis.toml`, and clearing the URL returns the store to `unset`.
#[test]
fn the_record_round_trips_through_the_file_and_clears_to_unset() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("synthesis.toml");
    let sink = Silent;
    static BUFFER: LogBuffer = LogBuffer::new();

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        save_relay_endpoint_with(
            &sink,
            &BUFFER,
            &store,
            "https://relay.example.com".to_string(),
        )
        .expect("the URL is saved");
        verify_relay_endpoint_with(
            &sink,
            &BUFFER,
            &store,
            StubProbe::new(Ok(healthy())).as_ref(),
            "https://relay.example.com".to_string(),
        )
        .expect("the relay answered");
    }

    // GSS-FR-VMRB: the file can be read, copied, or attached to a bug report.
    let written = std::fs::read_to_string(&path).expect("the file is readable");
    let relay_section = written
        .split("[relayEndpoint]")
        .nth(1)
        .unwrap_or_else(|| panic!("the file holds no relay endpoint: {written}"));
    for forbidden in ["token", "secret", "bearer", "key ="] {
        assert!(
            !relay_section.to_lowercase().contains(forbidden),
            "the stored relay endpoint holds `{forbidden}`: {relay_section}"
        );
    }

    let reopened = GlobalSettingsStore::with_path(path.clone());
    let record = reopened.load_relay_endpoint().expect("a record").outbound();
    assert_eq!(record.state, RelayEndpointState::Verified);
    assert_eq!(record.url.as_deref(), Some("https://relay.example.com"));
    assert_eq!(record.version.as_deref(), Some("v1.2.3"));
    assert_eq!(
        record.capabilities,
        vec!["remote_session".to_string(), "websocket".to_string()]
    );

    // Clearing the URL returns the record to `unset` and keeps no success.
    let cleared = save_relay_endpoint_with(&sink, &BUFFER, &reopened, "   ".to_string())
        .expect("the URL is cleared");
    assert_eq!(cleared.state, RelayEndpointState::Unset);
    assert_eq!(cleared.url, None);
    assert_eq!(cleared.version, None);
    assert!(cleared.capabilities.is_empty());

    let reopened = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reopened
            .load_relay_endpoint()
            .expect("a record")
            .outbound()
            .state,
        RelayEndpointState::Unset
    );
}
