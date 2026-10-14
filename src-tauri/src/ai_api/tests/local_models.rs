//! The local-model values of the Custom provider: its provider-level values,
//! its fixed reasoning set, Ollama detection, and the effective values the read
//! paths return (`AAP-ai-api-integrations.md` AAP-FR-AYKU, AAP-FR-ITBL,
//! AAP-FR-QRAX, AAP-FR-JQIJ, AAP-FR-KJSL, AAP-FR-RANS, AAP-FR-WSIC,
//! AAP-FR-HFQZ, AAP-FR-OVDB).

use super::*;
use crate::ai_api::detection::{
    detect_all, parse_show_payload, Detected, DetectionFailure, ModelDetector,
    MAX_CONCURRENT_DETECTIONS,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Condvar;
use std::time::Duration;

const SECRET: &str = "ollama-anything";
const BASE: &str = "http://localhost:11434";

/// A detector that answers from a table, counts the requests it is sent, and
/// records the highest number in flight at one time. A model with no entry in
/// the table fails with the status 404, as a host that is not Ollama does.
///
/// With `hold` set to the number of models, each request waits until
/// `MAX_CONCURRENT_DETECTIONS` requests are in flight, or until every model
/// has a request, or until a bound passes. So a detection that keeps four in
/// flight shows four in `peak`, and one that keeps more shows more.
#[derive(Default)]
struct FakeDetector {
    answers: StdMutex<HashMap<String, Result<Detected, DetectionFailure>>>,
    requests: StdMutex<Vec<(String, Option<String>, String)>>,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    hold: Option<usize>,
    /// The requests in flight and the requests that arrived, for `hold`.
    gate: (StdMutex<(usize, usize)>, Condvar),
}

impl FakeDetector {
    fn answering(answers: &[(&str, Detected)]) -> Arc<Self> {
        let d = Arc::new(Self::default());
        *d.answers.lock().unwrap() =
            answers.iter().map(|(id, v)| (id.to_string(), Ok(v.clone()))).collect();
        d
    }
}

impl ModelDetector for Arc<FakeDetector> {
    fn show(
        &self,
        base_url: &str,
        api_key: Option<&str>,
        model_id: &str,
    ) -> Result<Detected, DetectionFailure> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        self.requests.lock().unwrap().push((
            base_url.to_string(),
            api_key.map(str::to_string),
            model_id.to_string(),
        ));
        if let Some(total) = self.hold {
            let (lock, ready) = &self.gate;
            let mut state = lock.lock().unwrap();
            state.0 += 1;
            state.1 += 1;
            ready.notify_all();
            // The bound only stops a broken detection from hanging the test.
            let (mut state, _) = ready
                .wait_timeout_while(state, Duration::from_secs(30), |(in_flight, arrived)| {
                    *in_flight < MAX_CONCURRENT_DETECTIONS && *arrived < total
                })
                .unwrap();
            state.0 -= 1;
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.answers
            .lock()
            .unwrap()
            .get(model_id)
            .cloned()
            .unwrap_or(Err(DetectionFailure::Status(404)))
    }
}

fn detected(context: Option<u64>, caps: Option<&[&str]>) -> Detected {
    Detected {
        context_size: context,
        capabilities: caps.map(|c| c.iter().map(|s| s.to_string()).collect()),
    }
}

fn local_harness(detector: Arc<FakeDetector>) -> Harness {
    let prober = FakeProber::returning(&[("qwen3:32b", "qwen3:32b"), ("plain", "plain")]);
    let keys = FakeKeychain::new();
    Harness {
        store: GlobalSettingsStore::in_memory(),
        ai: AiApiIntegrations::new(Box::new(prober.clone()), Box::new(keys.clone()))
            .with_detector(Box::new(detector)),
        keys,
        prober,
    }
}

fn verify_local(h: &Harness) -> AiApiIntegration {
    verify_integration_impl(&h.store, &h.ai, "custom", BASE, Some(SECRET)).unwrap()
}

fn model<'a>(record: &'a AiApiIntegration, id: &str) -> &'a ModelOption {
    record.models.iter().find(|m| m.id == id).unwrap()
}

// AAP-FR-JQIJ: the context length is read under the architecture the body
// names, and the capabilities from the top-level list.
#[test]
fn an_api_show_body_gives_the_context_length_of_its_architecture_and_the_capabilities() {
    let body = r#"{
      "model_info": {
        "general.architecture": "qwen3",
        "qwen3.context_length": 40960,
        "llama.context_length": 8192
      },
      "capabilities": ["completion", "tools", "thinking"]
    }"#;
    assert_eq!(
        parse_show_payload(body),
        Some(detected(Some(40960), Some(&["completion", "tools", "thinking"])))
    );
}

// AAP-FR-JQIJ: a value not in the expected form is null, and the other value
// stands.
#[test]
fn a_value_not_in_the_expected_form_is_null() {
    for (body, expected) in [
        (r#"{"capabilities": ["tools"]}"#, detected(None, Some(&["tools"]))),
        (
            r#"{"model_info": {"general.architecture": "x"}, "capabilities": [1]}"#,
            detected(None, None),
        ),
        (
            r#"{"model_info": {"general.architecture": "x", "x.context_length": 0}}"#,
            detected(None, None),
        ),
        (
            r#"{"model_info": {"general.architecture": "x", "x.context_length": "4096"}}"#,
            detected(None, None),
        ),
        (r#"{"model_info": {"x.context_length": 4096}}"#, detected(None, None)),
    ] {
        assert_eq!(parse_show_payload(body), Some(expected), "{body}");
    }
    assert_eq!(parse_show_payload("not json"), None);
    assert_eq!(parse_show_payload("[1, 2]"), None);
}

// AAP-FR-QRAX, AAP-FR-JQIJ, AAP-FR-10: a Custom verification asks every model,
// with the same secret, over the normalised base, and stores what it learned.
#[test]
fn a_custom_verification_detects_every_model_and_stores_the_values() {
    let detector = FakeDetector::answering(&[(
        "qwen3:32b",
        detected(Some(40960), Some(&["completion", "tools"])),
    )]);
    let h = local_harness(detector.clone());
    let rec =
        verify_integration_impl(&h.store, &h.ai, "custom", "http://localhost:11434/", Some(SECRET))
            .unwrap();

    let mut requests = detector.requests.lock().unwrap().clone();
    requests.sort_by(|a, b| a.2.cmp(&b.2));
    assert_eq!(
        requests,
        vec![
            (BASE.to_string(), Some(SECRET.to_string()), "plain".to_string()),
            (BASE.to_string(), Some(SECRET.to_string()), "qwen3:32b".to_string()),
        ]
    );
    assert_eq!(model(&rec, "qwen3:32b").detected_context_size, Some(40960));
    assert_eq!(
        model(&rec, "qwen3:32b").detected_capabilities,
        Some(vec!["completion".to_string(), "tools".to_string()])
    );
    // The values survive the store.
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(model(find(&listed, "custom"), "qwen3:32b").detected_context_size, Some(40960));
}

// AAP-FR-KJSL: a failed detection gives no values, keeps the model, and does
// not fail the verification.
#[test]
fn a_failed_detection_gives_no_values_and_changes_nothing_else() {
    let h = local_harness(FakeDetector::answering(&[]));
    let rec = verify_local(&h);

    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(
        rec.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["qwen3:32b", "plain"]
    );
    for m in &rec.models {
        assert_eq!(m.detected_context_size, None);
        assert_eq!(m.detected_capabilities, None);
    }
}

// AAP-FR-KJSL, AAP-FR-10: each verification replaces the detected values.
#[test]
fn each_verification_replaces_the_detected_values() {
    let detector = FakeDetector::answering(&[("qwen3:32b", detected(Some(8192), None))]);
    let h = local_harness(detector.clone());
    assert_eq!(model(&verify_local(&h), "qwen3:32b").detected_context_size, Some(8192));

    detector.answers.lock().unwrap().clear();
    assert_eq!(model(&verify_local(&h), "qwen3:32b").detected_context_size, None);
}

// AAP-FR-QRAX: no more than four requests are in flight at one time, and
// four are, when there are enough models.
#[test]
fn detection_keeps_at_most_four_requests_in_flight() {
    let detector = Arc::new(FakeDetector { hold: Some(12), ..Default::default() });
    let models: Vec<ModelOption> =
        (0..12).map(|i| ModelOption::new(&format!("m{i}"), &format!("m{i}"))).collect();
    let (out, report) = detect_all(&detector, BASE, Some(SECRET), models);

    assert_eq!(out.len(), 12);
    assert_eq!(detector.requests.lock().unwrap().len(), 12);
    assert_eq!(detector.peak.load(Ordering::SeqCst), MAX_CONCURRENT_DETECTIONS);
    assert_eq!(report.sent, 12);
    // The order of the list is kept.
    assert_eq!(out[11].id, "m11");
}

// AAP-FR-OVDB, AAP-FR-KJSL: the report counts the models sent, the ones with
// values, and the failures by kind. An answer with no value is not a failure.
#[test]
fn the_detection_report_counts_the_values_and_the_failures_by_kind() {
    let detector = FakeDetector::answering(&[
        ("a", detected(Some(4096), None)),
        ("b", detected(None, Some(&["tools"]))),
        ("c", detected(None, None)),
    ]);
    detector.answers.lock().unwrap().insert("d".into(), Err(DetectionFailure::Timeout));
    let models: Vec<ModelOption> =
        ["a", "b", "c", "d", "e"].iter().map(|id| ModelOption::new(id, id)).collect();
    let (_, report) = detect_all(&detector, BASE, Some(SECRET), models);

    assert_eq!(report.sent, 5);
    assert_eq!(report.detected, 2);
    assert_eq!(report.failed, 2);
    assert_eq!(report.failure_kinds(), "status_404:1,timeout:1");
}

// AAP-FR-JQIJ, AAP-FR-KJSL: a context length the store cannot hold is not in
// the expected form, so it cannot fail the verification when it is saved.
#[test]
fn a_context_length_above_the_token_bound_is_null() {
    let body = |n: &str| {
        format!(r#"{{"model_info": {{"general.architecture": "x", "x.context_length": {n}}}}}"#)
    };
    assert_eq!(
        parse_show_payload(&body(&TOKEN_LIMIT_MAX.to_string())),
        Some(detected(Some(TOKEN_LIMIT_MAX), None))
    );
    for bad in
        [(TOKEN_LIMIT_MAX + 1).to_string(), u64::MAX.to_string(), "-1".into(), "4096.5".into()]
    {
        assert_eq!(parse_show_payload(&body(&bad)), Some(detected(None, None)), "{bad}");
    }

    let detector = FakeDetector::answering(&[]);
    detector.answers.lock().unwrap().insert(
        "qwen3:32b".into(),
        parse_show_payload(&body(&u64::MAX.to_string())).ok_or(DetectionFailure::BadBody),
    );
    let h = local_harness(detector);
    let rec = verify_local(&h);
    assert_eq!(rec.state, IntegrationState::Verified);
    assert_eq!(model(&rec, "qwen3:32b").detected_context_size, None);
}

// AAP-FR-QRAX, AAP-FR-MDLQ: a model list that is not valid fails the
// verification, and no detection request is sent.
#[test]
fn a_model_list_that_fails_sends_no_detection() {
    let detector = FakeDetector::answering(&[]);
    let mut h = local_harness(detector.clone());
    h.prober = FakeProber::failing(ProbeError::NotExpectedKind);
    h.ai = AiApiIntegrations::new(Box::new(h.prober.clone()), Box::new(h.keys.clone()))
        .with_detector(Box::new(detector.clone()));
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "custom", BASE, Some(SECRET)).unwrap_err(),
        ERR_NOT_AN_AI_ENDPOINT
    );
    assert!(detector.requests.lock().unwrap().is_empty());
}

// AAP-FR-QRAX: detection runs for Custom alone.
#[test]
fn no_other_provider_runs_detection() {
    let detector = FakeDetector::answering(&[]);
    let h = local_harness(detector.clone());
    verify_integration_impl(&h.store, &h.ai, "openai", "https://api.openai.com/v1", Some("sk-1"))
        .unwrap();
    assert!(detector.requests.lock().unwrap().is_empty());
}

// AAP-FR-WSIC, AAP-FR-24, AAP-FR-25: every Custom model offers the fixed set,
// whatever the detection reports.
#[test]
fn every_custom_model_offers_the_fixed_reasoning_set() {
    let h = local_harness(FakeDetector::answering(&[(
        "qwen3:32b",
        detected(None, Some(&["completion"])),
    )]));
    let rec = verify_local(&h);
    for m in &rec.models {
        let reasoning = m.reasoning.as_ref().expect("a Custom model offers reasoning");
        assert!(!reasoning.mandatory);
        assert_eq!(
            reasoning.supported_efforts.as_deref(),
            Some(&["none".to_string(), "low".into(), "medium".into(), "high".into()][..])
        );
        assert_eq!(reasoning.default_effort, None);
    }
    // The models of another provider still declare none.
    let openai =
        verify_integration_impl(&h.store, &h.ai, "openai", "https://api.openai.com/v1", Some("k"))
            .unwrap();
    assert!(!openai.models.is_empty());
    assert!(openai.models.iter().all(|m| m.reasoning.is_none()));
}

// AAP-FR-27, AAP-FR-AYKU, AAP-FR-WSIC: the Custom reasoning default needs no
// selected model and takes only a level of the fixed set.
#[test]
fn the_custom_reasoning_default_needs_no_model_and_takes_only_the_fixed_levels() {
    let h = local_harness(FakeDetector::answering(&[]));
    verify_local(&h);
    let effort = |e: &str| ReasoningChoice::Effort { effort: e.to_string() };

    let rec = set_reasoning_impl(&h.store, &h.ai, "custom", Some(effort("none"))).unwrap();
    assert_eq!(rec.selected_model, None);
    assert_eq!(rec.selected_reasoning, Some(effort("none")));

    for refused in [effort("xhigh"), ReasoningChoice::Off, ReasoningChoice::On] {
        assert_eq!(
            set_reasoning_impl(&h.store, &h.ai, "custom", Some(refused)).unwrap_err(),
            ERR_UNKNOWN_EFFORT
        );
    }
    // A model change or a clear of the model keeps the provider default.
    set_model_impl(&h.store, &h.ai, "custom", Some("plain")).unwrap();
    let rec = set_model_impl(&h.store, &h.ai, "custom", None).unwrap();
    assert_eq!(rec.selected_reasoning, Some(effort("none")));
    assert_eq!(verify_local(&h).selected_reasoning, Some(effort("none")));
}

// AAP-FR-ITBL, AAP-FR-AYKU: the two token values are stored and cleared at
// once, in range only, for Custom only.
#[test]
fn the_token_values_are_stored_in_range_for_custom_only() {
    let h = local_harness(FakeDetector::answering(&[]));
    let set = |limit, tokens| set_token_limit_impl(&h.store, &h.ai, "custom", limit, tokens);

    let rec = set(TokenLimit::MaxOutputTokens, Some(8192)).unwrap();
    assert_eq!(rec.max_output_tokens, Some(8192));
    let rec = set(TokenLimit::MaxContextSize, Some(TOKEN_LIMIT_MAX)).unwrap();
    assert_eq!(rec.max_context_size, Some(TOKEN_LIMIT_MAX));
    assert_eq!(rec.max_output_tokens, Some(8192));

    for bad in [0, TOKEN_LIMIT_MAX + 1] {
        assert_eq!(
            set(TokenLimit::MaxOutputTokens, Some(bad)).unwrap_err(),
            ERR_TOKEN_LIMIT_INVALID
        );
        assert_eq!(
            set(TokenLimit::MaxContextSize, Some(bad)).unwrap_err(),
            ERR_TOKEN_LIMIT_INVALID
        );
    }
    // A refused value changes nothing.
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&listed, "custom").max_output_tokens, Some(8192));
    assert_eq!(find(&listed, "custom").max_context_size, Some(TOKEN_LIMIT_MAX));
    let rec = set(TokenLimit::MaxOutputTokens, Some(TOKEN_LIMIT_MAX)).unwrap();
    assert_eq!(rec.max_output_tokens, Some(TOKEN_LIMIT_MAX));
    let rec = set(TokenLimit::MaxOutputTokens, Some(1)).unwrap();
    assert_eq!(rec.max_output_tokens, Some(1));
    set(TokenLimit::MaxOutputTokens, Some(8192)).unwrap();
    let rec = set(TokenLimit::MaxOutputTokens, None).unwrap();
    assert_eq!(rec.max_output_tokens, None);
    assert_eq!(rec.max_context_size, Some(TOKEN_LIMIT_MAX));

    assert_eq!(
        set_token_limit_impl(&h.store, &h.ai, "openai", TokenLimit::MaxContextSize, Some(10))
            .unwrap_err(),
        ERR_LIMIT_UNSUPPORTED
    );
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&listed, "openai").max_context_size, None);
}

// AAP-FR-ITBL: Clear returns both token values to null.
#[test]
fn clear_returns_the_token_values_to_null() {
    let h = local_harness(FakeDetector::answering(&[]));
    verify_local(&h);
    set_token_limit_impl(&h.store, &h.ai, "custom", TokenLimit::MaxOutputTokens, Some(4096))
        .unwrap();
    set_token_limit_impl(&h.store, &h.ai, "custom", TokenLimit::MaxContextSize, Some(4096))
        .unwrap();
    let listed = clear_integration_impl(&h.store, &h.ai, "custom").unwrap();
    let custom = find(&listed, "custom");
    assert_eq!(custom.max_output_tokens, None);
    assert_eq!(custom.max_context_size, None);
}

/// A verified, active Custom provider with one detected model and one plain.
fn active_local(h: &Harness) {
    verify_local(h);
    set_active_impl(&h.store, &h.ai, "custom").unwrap();
}

// AAP-FR-RANS, AAP-FR-HFQZ, AAP-FR-34, AGC-FR-14: the endpoint of an agent's turn carries
// the effective context size, the output limit, and the reasoning default.
#[test]
fn an_agent_endpoint_carries_the_effective_values() {
    let h = local_harness(FakeDetector::answering(&[("qwen3:32b", detected(Some(40960), None))]));
    active_local(&h);
    let effort = |e: &str| ReasoningChoice::Effort { effort: e.to_string() };

    // Nothing provider-level set: the detected size alone, no limit, no reasoning.
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "qwen3:32b", None).unwrap();
    assert_eq!(call.context_size, Some(40960));
    assert_eq!(call.max_output_tokens, None);
    assert_eq!(call.reasoning, None);
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "plain", None).unwrap();
    assert_eq!(call.context_size, None);

    set_token_limit_impl(&h.store, &h.ai, "custom", TokenLimit::MaxContextSize, Some(16384))
        .unwrap();
    set_token_limit_impl(&h.store, &h.ai, "custom", TokenLimit::MaxOutputTokens, Some(4096))
        .unwrap();
    set_reasoning_impl(&h.store, &h.ai, "custom", Some(effort("low"))).unwrap();

    // The detected size wins over the provider value; the provider value
    // stands where nothing was detected.
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "qwen3:32b", None).unwrap();
    assert_eq!(call.context_size, Some(40960));
    assert_eq!(call.max_output_tokens, Some(4096));
    assert_eq!(call.reasoning, Some(effort("low")));
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "plain", None).unwrap();
    assert_eq!(call.context_size, Some(16384));

    // The agent's own choice wins over the provider default.
    let call =
        resolve_ai_api_endpoint(&h.store, &h.ai, "custom", "plain", Some(effort("high"))).unwrap();
    assert_eq!(call.reasoning, Some(effort("high")));
}

// AAP-FR-34, AAP-FR-33: an agent on Custom takes only a level of the fixed set.
#[test]
fn an_agent_on_custom_takes_only_a_level_of_the_fixed_set() {
    let h = local_harness(FakeDetector::answering(&[]));
    active_local(&h);
    let effort = |e: &str| ReasoningChoice::Effort { effort: e.to_string() };
    assert!(validate_ai_api_selection(&h.store, "custom", "plain", Some(&effort("medium"))).is_ok());
    assert_eq!(
        validate_ai_api_selection(&h.store, "custom", "plain", Some(&ReasoningChoice::Off))
            .unwrap_err(),
        ERR_UNKNOWN_EFFORT
    );
}

// AAP-FR-19, AAP-FR-34, AAP-FR-RANS: another provider's stored reasoning never
// reaches an agent, and it has no effective context size or output limit.
#[test]
fn another_providers_endpoint_carries_no_custom_values() {
    let h = ok_harness();
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openrouter",
        "https://openrouter.ai/api/v1",
        Some("k"),
    )
    .unwrap();
    set_active_impl(&h.store, &h.ai, "openrouter").unwrap();
    // The provider stores a reasoning choice of its own, which is the
    // graduation loop's and never an agent's.
    let mut records = h.store.load_ai_api_registry().unwrap().0;
    let record = records.iter_mut().find(|r| r.provider == "openrouter").unwrap();
    record.selected_model = Some("gpt-5".into());
    record.selected_reasoning = Some(ReasoningChoice::On);
    for m in &mut record.models {
        m.reasoning = Some(ModelReasoning {
            mandatory: false,
            default_enabled: Some(false),
            supported_efforts: None,
            default_effort: None,
        });
    }
    h.store.save_ai_api_registry(records, Some("openrouter".into())).unwrap();
    assert_eq!(
        resolve_ai_api_call(&h.store, &h.ai, "project").unwrap().reasoning,
        Some(ReasoningChoice::On)
    );
    let call = resolve_ai_api_endpoint(&h.store, &h.ai, "openrouter", "gpt-5", None).unwrap();
    assert_eq!(call.context_size, None);
    assert_eq!(call.max_output_tokens, None);
    assert_eq!(call.reasoning, None);
}

// AAP-FR-31, AAP-FR-HFQZ, AAP-FR-RANS: a graduation run's endpoint carries the
// stored reasoning default and the effective values of the selected model.
#[test]
fn a_graduation_endpoint_carries_the_provider_default_and_the_effective_values() {
    let h = local_harness(FakeDetector::answering(&[("qwen3:32b", detected(Some(40960), None))]));
    active_local(&h);
    set_model_impl(&h.store, &h.ai, "custom", Some("qwen3:32b")).unwrap();
    let effort = ReasoningChoice::Effort { effort: "medium".into() };
    set_reasoning_impl(&h.store, &h.ai, "custom", Some(effort.clone())).unwrap();
    set_token_limit_impl(&h.store, &h.ai, "custom", TokenLimit::MaxOutputTokens, Some(2048))
        .unwrap();

    let call = resolve_ai_api_call(&h.store, &h.ai, "project").unwrap();
    assert_eq!(call.reasoning, Some(effort));
    assert_eq!(call.max_output_tokens, Some(2048));
    assert_eq!(call.context_size, Some(40960));
}

// GSS-FR-27, AAP-FR-AYKU: the new values round-trip through the TOML store.
#[test]
fn the_new_values_round_trip_through_the_store_file() {
    let record = AiApiRecord {
        provider: "custom".into(),
        base_url: Some(BASE.into()),
        max_output_tokens: Some(4096),
        max_context_size: Some(32768),
        selected_reasoning: Some(ReasoningChoice::Effort { effort: "low".into() }),
        models: vec![ModelOption {
            detected_context_size: Some(40960),
            detected_capabilities: Some(vec!["tools".into()]),
            ..ModelOption::new("qwen3:32b", "qwen3:32b")
                .with_reasoning(Some(crate::ai_api::custom_limits::custom_reasoning()))
        }],
        ..Default::default()
    };
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        records: Vec<AiApiRecord>,
    }
    let text = toml::to_string(&Wrapper { records: vec![record.clone()] }).unwrap();
    let back: Wrapper = toml::from_str(&text).unwrap();
    assert_eq!(back.records, vec![record]);
}
