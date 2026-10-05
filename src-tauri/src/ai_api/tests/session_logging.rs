//! Session logging (LGC-logging.md, AAP-FR-07).

use super::*;

// -- Session logging (LGC-logging.md, AAP-FR-07) ---------------------

/// A sink that publishes nowhere. The records still reach the process-wide
/// buffer, which is what these tests read; only the event is dropped, since
/// no Tauri runtime is running to deliver one.
#[derive(Clone)]
struct NullSink;

impl crate::logging::LogSink for NullSink {
    fn publish(&self, _state: &crate::logging::BufferState) {}
}

/// A buffer of these tests' own.
///
/// Not `logging::BUFFER`: that one is process-wide and is *cleared* when a
/// project closes or a worktree changes (LGC-FR-15), which
/// `crate::project`'s and `crate::menu`'s tests do while these are running.
/// A test reading the session buffer therefore reads a buffer another test
/// can empty mid-assertion — observed failing better than half the time
/// when those tests are co-scheduled. Taking the buffer as a parameter, as
/// `logging::log` itself does, is what makes these deterministic; the
/// commands pass the real one.
static TEST_BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();

/// Every record in the buffer as one string, for asserting that something is
/// absent from all of them — including the ones nobody thought to look at.
fn buffer_text() -> String {
    serde_json::to_string(&records()).expect("records serialise")
}

fn records() -> Vec<crate::logging::LogRecord> {
    TEST_BUFFER
        .query(&crate::logging::LogFilter::default(), None, crate::logging::BUFFER_CAPACITY)
        .expect("the buffer answers a default filter")
        .records
}

/// The newest record reading `message` whose `key` field is `value`.
///
/// Narrowed on a field because the buffer is process-wide and these tests
/// run alongside each other: the message alone would let one test read
/// another's record. Each test below therefore uses a provider id of its
/// own, distinct from the four real ones so that nothing outside it can
/// produce a matching record.
fn record_where(message: &str, key: &str, value: &str) -> crate::logging::LogRecord {
    records()
        .into_iter()
        .rev()
        .find(|r| r.message == message && r.fields.get(key) == Some(&serde_json::json!(value)))
        .unwrap_or_else(|| panic!("no record reads {message:?} with {key} = {value}"))
}

fn record_reading(message: &str, provider: &str) -> crate::logging::LogRecord {
    record_where(message, "provider", provider)
}

#[test]
fn no_verification_record_carries_the_key_or_even_its_hint() {
    // AAP-FR-07 / LGC-FR-16: the facility redacts nothing, so this is
    // enforceable at the emit site alone. The helpers are handed a real key
    // precisely so the test is about what they do with one rather than about
    // what a caller remembered not to pass.
    let secret = "sk-live-do-not-log-me-9f3c";
    let hint = "qzqz";
    log_verify_attempt(&NullSink, &TEST_BUFFER, "probe-openai", "https://api.openai.com/v1", Some(secret));
    log_verify_outcome(
        &NullSink,
        &TEST_BUFFER,
        "probe-openai",
        &Err(ERR_REJECTED.to_string()),
        41,
    );
    log_verify_outcome(
        &NullSink,
        &TEST_BUFFER,
        "probe-openai",
        &Ok(AiApiIntegration {
            turn_timeout_ms: None,
            provider: "probe-openai".into(),
            display_name: "OpenAI".into(),
            base_url: Some("https://api.openai.com/v1".into()),
            key_state: KeyState::Set,
            masked_hint: Some(hint.into()),
            key_required: true,
            state: IntegrationState::Verified,
            verified_at: Some("2026-08-04T00:00:00Z".into()),
            models: vec![ModelOption::new("gpt-5", "GPT-5")],
            models_origin: ModelsOrigin::Probed,
            selected_model: Some("gpt-5".into()),
            selected_reasoning: None,
            active: true,
        }),
        41,
    );

    // The records exist first, so the absence assertions below are
    // assertions about something rather than about an empty buffer.
    // What the attempt *does* say about the key is that there was one.
    let attempt = record_reading("verifying ai api integration", "probe-openai");
    let text = buffer_text();
    assert!(!text.contains(secret), "a key reached the session log");
    assert!(
        !text.contains(hint),
        "even the masked hint stays out: four characters of a credential are \
         worth nothing to a reader and travel as far as the log does",
    );
    assert_eq!(
        attempt.fields.get("keySupplied"),
        Some(&serde_json::json!(true)),
    );
    assert_eq!(
        attempt.fields.get("baseUrl"),
        Some(&serde_json::json!("https://api.openai.com/v1")),
    );
    assert_eq!(
        attempt.domains,
        vec![crate::logging::Domain::Ai, crate::logging::Domain::Remote],
        "a verification is about the AI integration and about the network",
    );

    // A failed verification is an ERROR carrying the typed refusal alone.
    let failed = record_reading("ai api verification failed", "probe-openai");
    assert_eq!(failed.level, crate::logging::LogLevel::Error);
    assert_eq!(failed.fields.get("error"), Some(&serde_json::json!(ERR_REJECTED)));
    assert_eq!(failed.fields.get("durationMs"), Some(&serde_json::json!(41)));

    // A successful one reports what changed about the record.
    let ok = record_reading("ai api integration verified", "probe-openai");
    assert_eq!(ok.level, crate::logging::LogLevel::Info);
    assert_eq!(ok.fields.get("models"), Some(&serde_json::json!(1)));
    assert_eq!(ok.fields.get("modelsOrigin"), Some(&serde_json::json!("probed")));
    assert_eq!(ok.fields.get("state"), Some(&serde_json::json!("verified")));
    assert_eq!(ok.fields.get("selectedModel"), Some(&serde_json::json!("gpt-5")));
}

#[test]
fn an_empty_key_field_is_reported_as_no_key_supplied() {
    // AAP-FR-32: an empty field means "re-present what is stored", and the
    // difference between that and a fresh credential is the first thing to
    // check when a verification that used to work stops.
    log_verify_attempt(&NullSink, &TEST_BUFFER, "probe-blank", "http://localhost:11434/v1", None);
    let none = record_reading("verifying ai api integration", "probe-blank");
    assert_eq!(none.fields.get("keySupplied"), Some(&serde_json::json!(false)));

    log_verify_attempt(&NullSink, &TEST_BUFFER, "probe-blank", "http://localhost:11434/v1", Some("   "));
    let blank = record_reading("verifying ai api integration", "probe-blank");
    assert_eq!(
        blank.fields.get("keySupplied"),
        Some(&serde_json::json!(false)),
        "whitespace is not a key",
    );
}

#[test]
fn a_refused_configuration_change_is_a_warning_carrying_its_typed_error() {
    // Nothing broke: the module declined to store a selection it could not
    // serve. The record exists so an author whose choice did not stick can
    // see that it was refused rather than lost.
    let refused: Result<(), String> = Err(ERR_UNKNOWN_MODEL.to_string());
    log_config_outcome(
        &NullSink,
        &TEST_BUFFER,
        "ai api model selected",
        &refused,
        log_fields! { "provider" => "probe-config", "model" => "no-such-model" },
    );
    let record = record_reading("ai api model selected", "probe-config");
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(record.fields.get("error"), Some(&serde_json::json!(ERR_UNKNOWN_MODEL)));
    assert_eq!(record.fields.get("model"), Some(&serde_json::json!("no-such-model")));
    assert_eq!(record.domains, vec![crate::logging::Domain::Ai]);

    let taken: Result<(), String> = Ok(());
    log_config_outcome(
        &NullSink,
        &TEST_BUFFER,
        "ai api model selected",
        &taken,
        log_fields! { "provider" => "probe-config", "model" => "m" },
    );
    let record = record_reading("ai api model selected", "probe-config");
    assert_eq!(record.level, crate::logging::LogLevel::Info);
    assert!(!record.fields.contains_key("error"));
}

#[test]
fn a_reasoning_choice_reads_as_one_field() {
    // The depth an author asked for, in the spelling both this module and
    // `crate::agent_conversations` log it under.
    assert_eq!(reasoning_label(None), "default");
    assert_eq!(reasoning_label(Some(&ReasoningChoice::Off)), "off");
    assert_eq!(reasoning_label(Some(&ReasoningChoice::On)), "on");
    assert_eq!(
        reasoning_label(Some(&ReasoningChoice::Effort {
            effort: "high".into()
        })),
        "effort:high",
    );
}

#[test]
fn a_credential_pasted_into_the_base_url_field_never_reaches_a_record() {
    // The order that makes this necessary: the attempt is logged first and
    // `normalize_base_url` refuses userinfo second, inside the verification
    // — too late for a record already written. So an author who pastes a key
    // into the URL field gets `base_url_invalid` back, and, without
    // `loggable_base_url`, their key written verbatim into a buffer they can
    // export into a bug report (AAP-FR-07, LGC-FR-16).
    log_verify_attempt(
        &NullSink,
        &TEST_BUFFER,
        "probe-url",
        "https://sk-pasted-into-the-url@api.openai.com/v1",
        None,
    );
    // And the case normalisation does not catch at all: a gateway that takes
    // its key in the query string.
    log_verify_attempt(
        &NullSink,
        &TEST_BUFFER,
        "probe-url-query",
        "https://gateway.example/v1?api-key=sk-in-the-query",
        None,
    );

    // Both records first — an emit that never happened would otherwise
    // satisfy every absence assertion below. The endpoint is still
    // recognisable in each, which is the point of logging it at all.
    assert_eq!(
        record_reading("verifying ai api integration", "probe-url")
            .fields
            .get("baseUrl"),
        Some(&serde_json::json!("https://api.openai.com/v1")),
    );
    assert_eq!(
        record_reading("verifying ai api integration", "probe-url-query")
            .fields
            .get("baseUrl"),
        Some(&serde_json::json!("https://gateway.example/v1")),
    );
    let text = buffer_text();
    assert!(!text.contains("sk-pasted"), "a key reached the log through the URL");
    assert!(!text.contains("sk-in-the-query"), "a key reached the log through the query");
}

#[test]
fn a_listing_reports_a_provider_whose_key_has_gone_missing() {
    // AAP-FR-09 is the whole justification for this record: a provider that
    // was working reads `key_unavailable` because the keychain no longer
    // answers for it, and nothing else in the application says so until an
    // agent fails to answer and nobody can explain why.
    let integration = |provider: &str, state: IntegrationState, active: bool| AiApiIntegration {
        turn_timeout_ms: None,
        provider: provider.into(),
        display_name: provider.into(),
        base_url: Some("https://api.example/v1".into()),
        key_state: KeyState::Set,
        masked_hint: None,
        key_required: true,
        state,
        verified_at: Some("2026-08-04T00:00:00Z".into()),
        models: Vec::new(),
        models_origin: ModelsOrigin::Catalog,
        selected_model: None,
        selected_reasoning: None,
        active,
    };
    log_listing(
        &NullSink,
        &TEST_BUFFER,
        &Ok(vec![
            integration("probe-listing-active", IntegrationState::Verified, true),
            integration("probe-listing-lost", IntegrationState::KeyUnavailable, false),
            integration("probe-listing-unset", IntegrationState::Unconfigured, false),
        ]),
    );

    let record = record_where(
        "ai api integrations listed",
        "active",
        "probe-listing-active",
    );
    assert_eq!(record.level, crate::logging::LogLevel::Debug);
    assert_eq!(record.fields.get("total"), Some(&serde_json::json!(3)));
    assert_eq!(record.fields.get("verified"), Some(&serde_json::json!(1)));
    assert_eq!(record.fields.get("keyUnavailable"), Some(&serde_json::json!(1)));
}

#[test]
fn a_project_resolution_reports_an_override_that_is_being_ignored() {
    // AAP-FR-16's `override_unavailable`: the author believes they are
    // calling one provider and the application is calling another. Reported
    // nowhere else but in a settings panel nobody is looking at.
    log_project_resolution(
        &NullSink,
        &TEST_BUFFER,
        &ProjectAiApiIntegration {
            provider: Some("probe-resolution-effective".into()),
            resolution: ProjectResolution::OverrideUnavailable,
            override_provider: Some("probe-resolution-named".into()),
        },
    );
    let record = record_reading(
        "project ai api integration resolved",
        "probe-resolution-effective",
    );
    assert_eq!(record.level, crate::logging::LogLevel::Debug);
    assert_eq!(
        record.fields.get("resolution"),
        Some(&serde_json::json!("override_unavailable")),
    );
    assert_eq!(
        record.fields.get("override"),
        Some(&serde_json::json!("probe-resolution-named")),
    );

    // Nothing resolving reads as such rather than as an absent field.
    log_project_resolution(
        &NullSink,
        &TEST_BUFFER,
        &ProjectAiApiIntegration {
            provider: None,
            resolution: ProjectResolution::NoneConfigured,
            override_provider: None,
        },
    );
    let nothing = record_where(
        "project ai api integration resolved",
        "resolution",
        "none_configured",
    );
    assert_eq!(nothing.message, "project ai api integration resolved");
    assert_eq!(nothing.fields.get("provider"), Some(&serde_json::json!("none")));
    assert_eq!(nothing.fields.get("override"), Some(&serde_json::json!("none")));
}
