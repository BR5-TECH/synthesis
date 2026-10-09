//! Claude Code's Custom Gateway in front of the Amazon Bedrock runtime API
//! (AIC-FR-QHLN, AIC-FR-UFNB, AIC-FR-PADP, AIC-FR-KWMV, AIC-FR-YXAB,
//! AIC-FR-XZCS, AIC-FR-ISOC, AIC-FR-25).

use super::*;

/// A fake gateway token. It is not a credential.
const GATEWAY_TOKEN: &str = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL: &str = "https://llm-gateway.example.com/bedrock";

fn claude_harness(prober: std::sync::Arc<FakeProber>) -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        prober,
        FakeKeychain::new(),
    )
}

fn bedrock_config(token: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        auth_mode: Some("custom_gateway".into()),
        gateway_api: Some("bedrock".into()),
        gateway_base_url: Some(GATEWAY_URL.into()),
        gateway_token: token.map(str::to_string),
        ..Default::default()
    }
}

fn verify(h: &Harness, config: &VerifyConfig) -> Result<AgenticIntegration, String> {
    verify_integration_impl(&h.store, &h.ai, "claude_code", config)
}

fn stored_record(h: &Harness) -> AgenticRecord {
    let (records, _) = h.store.load_agentic_registry().unwrap();
    records.into_iter().find(|r| r.vendor == "claude_code").unwrap()
}

fn environment_of(h: &Harness) -> Vec<LaunchVariable> {
    match resolve_agent_launch_credential(&h.store, &h.ai, "claude_code").expect("handoff") {
        AgentLaunchCredential::ClaudeEnvironment(variables) => variables,
        other => panic!("expected an environment, got {other:?}"),
    }
}

/// The name, value, and masked flag of each variable, in order.
fn described(variables: &[LaunchVariable]) -> Vec<(String, String, bool)> {
    variables
        .iter()
        .map(|v| (v.name.clone(), v.value.expose().to_string(), v.masked))
        .collect()
}

// AIC-FR-QHLN, AIC-FR-YXAB: a Bedrock verification asks no gateway, keeps the
// catalog, and commits the API with the other gateway fields and the token.
#[test]
fn a_bedrock_verification_asks_no_gateway_and_commits_the_api() {
    let prober = FakeProber::failing(ProbeError::Status(400));
    let h = claude_harness(prober.clone());

    let record = verify(&h, &bedrock_config(Some(GATEWAY_TOKEN))).expect("verifies");

    assert!(prober.calls.lock().unwrap().is_empty(), "the gateway was not asked");
    assert_eq!(record.state, IntegrationState::Verified);
    assert_eq!(record.gateway_api, Some(GatewayApi::Bedrock));
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    assert!(!record.models.is_empty());
    assert!(!record.gateway_check_skipped);
    assert_eq!(stored_record(&h).gateway_api, GatewayApi::Bedrock);
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
}

// AIC-FR-QHLN, AIC-FR-KWMV: the skip flag changes nothing in a Bedrock payload.
#[test]
fn the_skip_flag_does_not_mark_a_bedrock_gateway() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = claude_harness(prober.clone());
    let mut config = bedrock_config(Some(GATEWAY_TOKEN));
    config.skip_gateway_check = Some(true);
    let record = verify(&h, &config).expect("verifies");
    assert!(prober.calls.lock().unwrap().is_empty(), "the gateway was not asked");
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    assert!(!record.gateway_check_skipped);
    assert!(!stored_record(&h).gateway_check_skipped);
}

// AIC-FR-QHLN: the field checks and the binary still run for Bedrock.
#[test]
fn a_bedrock_verification_keeps_every_other_check() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    assert_eq!(verify(&h, &bedrock_config(None)).unwrap_err(), ERR_TOKEN_MISSING);
    let mut no_url = bedrock_config(Some(GATEWAY_TOKEN));
    no_url.gateway_base_url = Some(" ".into());
    assert_eq!(verify(&h, &no_url).unwrap_err(), ERR_BASE_URL_EMPTY);
    let mut bad_var = bedrock_config(Some(GATEWAY_TOKEN));
    bad_var.gateway_token_var = Some("1BAD".into());
    assert_eq!(verify(&h, &bad_var).unwrap_err(), ERR_TOKEN_VAR_INVALID);
    let mut reserved = bedrock_config(Some(GATEWAY_TOKEN));
    reserved.env_vars = Some(vec!["PATH=/tmp".into()]);
    assert_eq!(verify(&h, &reserved).unwrap_err(), "env_var_reserved:PATH");

    let wrong_binary = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "not the expected program"),
        FakeProber::returning(&[("m", "M")]),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify(&wrong_binary, &bedrock_config(Some(GATEWAY_TOKEN))).unwrap_err(),
        ERR_NOT_THE_EXPECTED_CLI
    );
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "no field failure persisted anything");
    let (records, _) = wrong_binary.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "the binary failure persisted nothing");
    assert_eq!(wrong_binary.keys.get_raw("claude_code_gateway"), None);
}

// AIC-FR-QHLN, AIC-FR-UFNB: an unknown API, and the API in a subscription or
// another vendor's payload, are the wrong kind.
#[test]
fn the_api_outside_its_values_and_its_payload_is_the_wrong_kind() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    for value in ["vertex", "Bedrock", " bedrock", ""] {
        let mut unknown = bedrock_config(Some(GATEWAY_TOKEN));
        unknown.gateway_api = Some(value.into());
        assert_eq!(verify(&h, &unknown).unwrap_err(), ERR_WRONG_CONFIG_KIND, "{value:?}");
    }

    let mut subscription = claude_config(Some(SAMPLE_TOKEN));
    subscription.gateway_api = Some("anthropic".into());
    assert_eq!(verify(&h, &subscription).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    let codex = all_clis_harness();
    let mut for_codex = cli_config("/usr/bin/codex");
    for_codex.gateway_api = Some("bedrock".into());
    assert_eq!(
        verify_integration_impl(&codex.store, &codex.ai, "codex", &for_codex).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
}

// AIC-FR-QHLN, AIC-FR-PADP: an explicit or absent `anthropic` still asks the
// gateway, and a record that stores no API reads as `anthropic`.
#[test]
fn an_anthropic_gateway_is_still_asked_and_is_the_default() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = claude_harness(prober.clone());
    let mut explicit = bedrock_config(Some(GATEWAY_TOKEN));
    explicit.gateway_api = Some("anthropic".into());
    assert_eq!(verify(&h, &explicit).unwrap().gateway_api, Some(GatewayApi::Anthropic));
    let mut absent = bedrock_config(None);
    absent.gateway_api = None;
    assert_eq!(verify(&h, &absent).unwrap().gateway_api, Some(GatewayApi::Anthropic));
    assert_eq!(prober.calls.lock().unwrap().len(), 2);

    let written = toml::to_string(&stored_record(&h)).unwrap();
    assert!(!written.contains("gatewayApi"), "the default is not written");
    let read: AgenticRecord = toml::from_str("vendor = \"claude_code\"\n").unwrap();
    assert_eq!(read.gateway_api, GatewayApi::Anthropic);
}

// AIC-FR-QHLN, AIC-FR-YXAB: a Bedrock gateway is written to the settings file,
// and reads back as Bedrock, so a relaunch does not send the token to the
// other API.
#[test]
fn a_bedrock_gateway_survives_the_settings_file() {
    let record = AgenticRecord {
        auth_mode: AuthMode::CustomGateway,
        gateway_api: GatewayApi::Bedrock,
        gateway_base_url: Some(GATEWAY_URL.into()),
        ..AgenticRecord::empty("claude_code")
    };
    let written = toml::to_string(&record).unwrap();
    assert!(written.contains("gatewayApi = \"bedrock\""), "{written}");
    let read: AgenticRecord = toml::from_str(&written).unwrap();
    assert_eq!(read, record);
}

// AIC-FR-QHLN, AIC-FR-IOWS: a Bedrock re-verification with no token keeps the
// stored token, its hint, and the launch environment.
#[test]
fn a_bedrock_reverification_keeps_the_stored_token() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = claude_harness(prober.clone());
    verify(&h, &bedrock_config(Some(GATEWAY_TOKEN))).unwrap();
    let record = verify(&h, &bedrock_config(None)).expect("re-verifies");

    assert!(prober.calls.lock().unwrap().is_empty());
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(h.keys.calls_for("claude_code_gateway"), vec!["set"], "the token was not rewritten");
    let token = environment_of(&h)
        .into_iter()
        .find(|v| v.name == "ANTHROPIC_AUTH_TOKEN")
        .expect("the token variable");
    assert_eq!(token.value.expose(), GATEWAY_TOKEN);
}

// AIC-FR-QHLN and the logging rule: the API field of a verification log is a
// fixed word, and never the text a payload carried.
#[test]
fn the_api_log_field_never_carries_payload_text() {
    use crate::logging::{LogBuffer, LogFilter, LogSink};

    #[derive(Clone, Default)]
    struct Silent;
    impl LogSink for Silent {
        fn publish(&self, _state: &crate::logging::BufferState) {}
    }

    static TEST_BUFFER: LogBuffer = LogBuffer::new();
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    let smuggled = "gw-SMUGGLED-FAKE-VALUE-NOT-A-CREDENTIAL";
    let mut config = bedrock_config(Some(GATEWAY_TOKEN));
    config.gateway_api = Some(smuggled.into());
    log_verify_attempt(&Silent, &TEST_BUFFER, "claude_code", &config);
    let refused = verify(&h, &config);
    log_verify_outcome(&Silent, &TEST_BUFFER, "claude_code", &refused, 1);
    let accepted = verify(&h, &bedrock_config(Some(GATEWAY_TOKEN)));
    log_verify_outcome(&Silent, &TEST_BUFFER, "claude_code", &accepted, 1);

    let _ = TEST_BUFFER.take_pending_flush(Instant::now() + Duration::from_secs(1));
    let (text, _) = TEST_BUFFER.export_text(&LogFilter::default()).unwrap();
    assert!(text.contains("unrecognised"), "{text}");
    assert!(text.contains("Bedrock"), "the outcome names the API");
    for forbidden in [smuggled, GATEWAY_TOKEN, "k2Qz"] {
        assert!(!text.contains(forbidden), "a log record carried {forbidden:?}");
    }
}

// AIC-FR-QHLN, AIC-FR-25: only Claude Code in gateway mode reports an API.
#[test]
fn only_a_claude_gateway_record_reports_an_api() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    verify(&h, &bedrock_config(Some(GATEWAY_TOKEN))).unwrap();
    for integration in list_integrations_impl(&h.store, &h.ai).unwrap() {
        let expected = (integration.vendor == "claude_code").then_some(GatewayApi::Bedrock);
        assert_eq!(integration.gateway_api, expected, "{}", integration.vendor);
    }

    // A hand-edited file that puts a Bedrock gateway on another vendor's record.
    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records.push(AgenticRecord {
        auth_mode: AuthMode::CustomGateway,
        gateway_api: GatewayApi::Bedrock,
        ..AgenticRecord::empty("codex")
    });
    h.store.save_agentic_registry(records, active).unwrap();
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let codex = listed.iter().find(|i| i.vendor == "codex").unwrap();
    assert_eq!(codex.gateway_api, None, "another vendor reports none");

    let record = verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    assert_eq!(record.gateway_api, None, "a subscription record reports none");
}

// AIC-FR-XZCS, AIC-FR-ISOC: the Bedrock launch environment, in order, with only
// the token masked, and no `ANTHROPIC_BASE_URL`.
#[test]
fn the_bedrock_launch_environment_has_the_bedrock_variables() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    let mut config = bedrock_config(Some(GATEWAY_TOKEN));
    config.env_vars = Some(vec!["AWS_REGION=eu-west-1".into()]);
    verify(&h, &config).unwrap();

    assert_eq!(
        described(&environment_of(&h)),
        vec![
            ("CLAUDE_CODE_USE_BEDROCK".into(), "1".into(), false),
            ("CLAUDE_CODE_SKIP_BEDROCK_AUTH".into(), "1".into(), false),
            ("ANTHROPIC_BEDROCK_BASE_URL".into(), GATEWAY_URL.into(), false),
            ("ANTHROPIC_AUTH_TOKEN".into(), GATEWAY_TOKEN.into(), true),
            ("AWS_REGION".into(), "eu-west-1".into(), true),
        ]
    );
}

// AIC-FR-XZCS: an author entry replaces a Bedrock variable of the same name in
// its place, and a renamed token travels under the new name.
#[test]
fn an_author_entry_replaces_a_bedrock_variable() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    let mut config = bedrock_config(Some(GATEWAY_TOKEN));
    config.gateway_token_var = Some("GATEWAY_KEY".into());
    config.env_vars = Some(vec!["CLAUDE_CODE_SKIP_BEDROCK_AUTH=0".into()]);
    verify(&h, &config).unwrap();

    // AIC-FR-ISOC: a replacing entry takes the masking rule of an entry.
    config.env_vars = Some(vec![
        "CLAUDE_CODE_SKIP_BEDROCK_AUTH=0".into(),
        "ANTHROPIC_BEDROCK_BASE_URL=https://other.example.com".into(),
    ]);
    config.gateway_token = None;
    verify(&h, &config).unwrap();
    assert_eq!(
        described(&environment_of(&h)),
        vec![
            ("CLAUDE_CODE_USE_BEDROCK".into(), "1".into(), false),
            ("CLAUDE_CODE_SKIP_BEDROCK_AUTH".into(), "0".into(), false),
            ("ANTHROPIC_BEDROCK_BASE_URL".into(), "https://other.example.com".into(), true),
            ("GATEWAY_KEY".into(), GATEWAY_TOKEN.into(), true),
        ]
    );
}

// AIC-FR-XZCS, AIC-FR-QHLN: changing the API back to Anthropic returns the
// launch to `ANTHROPIC_BASE_URL`, and keeps the URL, the name, and the token.
#[test]
fn switching_back_to_anthropic_restores_the_anthropic_launch() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    verify(&h, &bedrock_config(Some(GATEWAY_TOKEN))).unwrap();
    let mut back = bedrock_config(None);
    back.gateway_api = Some("anthropic".into());
    verify(&h, &back).unwrap();

    let names: Vec<String> = environment_of(&h).iter().map(|v| v.name.clone()).collect();
    assert_eq!(names, vec!["ANTHROPIC_BASE_URL".to_string(), "ANTHROPIC_AUTH_TOKEN".to_string()]);
    let record = stored_record(&h);
    assert_eq!(record.gateway_api, GatewayApi::Anthropic);
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
}

// AIC-FR-UFNB: the payload names the API `gatewayApi` on the wire, and the view
// spells it in snake case.
#[test]
fn the_api_has_its_wire_names() {
    let config: VerifyConfig =
        serde_json::from_str(r#"{"authMode":"custom_gateway","gatewayApi":"bedrock"}"#).unwrap();
    assert_eq!(config.gateway_api.as_deref(), Some("bedrock"));
    let view = AgenticIntegration {
        gateway_api: Some(GatewayApi::Bedrock),
        ..Default::default()
    };
    assert!(serde_json::to_string(&view).unwrap().contains(r#""gatewayApi":"bedrock""#));
}

// AIC-FR-CVPW: a token variable name that the launch sets itself is refused in
// either API and in any case, so the token can never replace a base URL or a
// Bedrock flag.
#[test]
fn a_token_variable_name_the_launch_sets_is_refused() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    for api in ["anthropic", "bedrock"] {
        for name in [
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_BEDROCK_BASE_URL",
            "CLAUDE_CODE_USE_BEDROCK",
            "claude_code_skip_bedrock_auth",
        ] {
            let mut config = bedrock_config(Some(GATEWAY_TOKEN));
            config.gateway_api = Some(api.into());
            config.gateway_token_var = Some(name.into());
            assert_eq!(verify(&h, &config).unwrap_err(), ERR_TOKEN_VAR_INVALID, "{api}: {name}");
        }
    }
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "no refusal persisted anything");
}
