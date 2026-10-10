//! Claude Code's Custom Gateway, which serves the Amazon Bedrock runtime API and
//! is reached from the container alone (AIC-FR-QHLN, AIC-FR-UFNB, AIC-FR-CVPW,
//! AIC-FR-XTEZ, AIC-FR-XZCS, AIC-FR-ISOC, AIC-FR-15, AIC-FR-30).

use super::*;

/// A fake gateway token. It is not a credential.
const GATEWAY_TOKEN: &str = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL: &str = "https://llm-gateway.example.com/bedrock";

/// A machine with no Claude Code binary at all, and a runner and a prober that
/// record any call made to them.
fn no_binary_harness() -> (Harness, Arc<FakeRunner>, Arc<FakeProber>) {
    let runner = FakeRunner::saying("/usr/bin/claude", "claude 2.1.4");
    let prober = FakeProber::failing(ProbeError::Unreachable("must not be asked".into()));
    let h = harness(FakeFs::with_executable(&[]), runner.clone(), prober.clone(), FakeKeychain::new());
    (h, runner, prober)
}

fn gateway_config(token: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        auth_mode: Some("custom_gateway".into()),
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

// AIC-FR-QHLN, AIC-FR-UFNB, AIC-FR-YXAB: a gateway verifies on a machine with no
// binary. It runs nothing, asks nothing, and commits the fields and the token
// with the bundled catalog and no version.
#[test]
fn a_gateway_verifies_with_no_binary_and_runs_nothing() {
    let (h, runner, prober) = no_binary_harness();

    let record = verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).expect("verifies");

    assert!(runner.calls.lock().unwrap().is_empty(), "no binary was run");
    assert!(prober.calls.lock().unwrap().is_empty(), "the gateway was not asked");
    assert_eq!(record.state, IntegrationState::Verified);
    assert_eq!(record.auth_mode, Some(AuthMode::CustomGateway));
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(record.gateway_key_state, KeyState::Set);
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    assert!(!record.models.is_empty());
    assert_eq!(record.version, None);
    assert_eq!(record.binary_path, None);
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
}

// AIC-FR-UFNB, AIC-FR-QHLN: a path in a gateway payload is ignored, and a
// stored path is kept as it is.
#[test]
fn a_gateway_verification_ignores_the_path_and_keeps_the_stored_one() {
    let (h, runner, _) = no_binary_harness();
    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records.push(AgenticRecord {
        binary_path: Some("/opt/old/claude".into()),
        path_origin: PathOrigin::UserSupplied,
        ..AgenticRecord::empty("claude_code")
    });
    h.store.save_agentic_registry(records, active).unwrap();

    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.path = Some("/nowhere/claude".into());
    let record = verify(&h, &config).expect("verifies");

    assert!(runner.calls.lock().unwrap().is_empty());
    assert_eq!(record.binary_path.as_deref(), Some("/opt/old/claude"));
    assert_eq!(stored_record(&h).path_origin, PathOrigin::UserSupplied);
    assert_eq!(record.state, IntegrationState::Verified, "the stored path is not checked");
}

// AIC-FR-QHLN, AIC-FR-IOWS, AIC-FR-UFNB, AIC-FR-XTEZ: the field checks still run,
// and a refused payload persists nothing.
#[test]
fn a_gateway_verification_keeps_every_field_check() {
    let (h, _, _) = no_binary_harness();
    assert_eq!(verify(&h, &gateway_config(None)).unwrap_err(), ERR_TOKEN_MISSING);
    assert_eq!(verify(&h, &gateway_config(Some("two words"))).unwrap_err(), ERR_TOKEN_MALFORMED);
    let mut no_url = gateway_config(Some(GATEWAY_TOKEN));
    no_url.gateway_base_url = Some(" ".into());
    assert_eq!(verify(&h, &no_url).unwrap_err(), ERR_BASE_URL_EMPTY);
    let mut bad_var = gateway_config(Some(GATEWAY_TOKEN));
    bad_var.gateway_token_var = Some("1BAD".into());
    assert_eq!(verify(&h, &bad_var).unwrap_err(), ERR_TOKEN_VAR_INVALID);
    let mut bad_entry = gateway_config(Some(GATEWAY_TOKEN));
    bad_entry.env_vars = Some(vec!["no equals sign".into()]);
    assert_eq!(verify(&h, &bad_entry).unwrap_err(), "env_var_invalid:1");

    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "no refusal persisted anything");
    assert_eq!(h.keys.get_raw("claude_code_gateway"), None);
}

// AIC-FR-UFNB, AIC-FR-QHLN: a payload from an older build, which still names an
// API shape and a skip flag, verifies as a Bedrock gateway and asks nothing.
#[test]
fn an_older_payload_verifies_as_a_bedrock_gateway() {
    let (h, runner, prober) = no_binary_harness();
    let config: VerifyConfig = serde_json::from_str(&format!(
        r#"{{"path":"/usr/bin/claude","authMode":"custom_gateway","gatewayBaseUrl":"{GATEWAY_URL}","gatewayToken":"{GATEWAY_TOKEN}","gatewayApi":"anthropic","skipGatewayCheck":true}}"#
    ))
    .unwrap();
    verify(&h, &config).expect("verifies");
    assert!(runner.calls.lock().unwrap().is_empty());
    assert!(prober.calls.lock().unwrap().is_empty());
    let names: Vec<String> = environment_of(&h).into_iter().map(|v| v.name.clone()).collect();
    assert_eq!(
        names,
        vec!["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_SKIP_BEDROCK_AUTH", "ANTHROPIC_BEDROCK_BASE_URL", "ANTHROPIC_AUTH_TOKEN"]
    );
}

// AIC-FR-QHLN, AIC-FR-XZCS: a settings file written by an older build, with an
// API shape and a skip flag on the record, loads and launches the Bedrock four.
#[test]
fn an_older_registry_record_launches_the_bedrock_variables() {
    let (h, _, _) = no_binary_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    let written = toml::to_string(&stored_record(&h)).unwrap();
    let older = format!("{written}\ngatewayApi = \"anthropic\"\ngatewayCheckSkipped = true\n");
    let read: AgenticRecord = toml::from_str(&older).expect("an older record loads");
    let (_, active) = h.store.load_agentic_registry().unwrap();
    h.store.save_agentic_registry(vec![read], active).unwrap();

    let names: Vec<String> = environment_of(&h).into_iter().map(|v| v.name.clone()).collect();
    assert_eq!(
        names,
        vec!["CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_SKIP_BEDROCK_AUTH", "ANTHROPIC_BEDROCK_BASE_URL", "ANTHROPIC_AUTH_TOKEN"]
    );
}

// AIC-FR-06, AIC-FR-WNQR: a gateway verification that fails leaves a working
// subscription configuration exactly as it was.
#[test]
fn a_failed_gateway_verification_leaves_a_subscription_record_as_it_was() {
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("m", "M")]),
        FakeKeychain::new(),
    );
    let mut subscription = claude_config(Some(SAMPLE_TOKEN));
    subscription.env_vars = Some(vec!["HTTPS_PROXY=http://proxy:3128".into()]);
    verify(&h, &subscription).unwrap();
    let before = stored_record(&h);

    let mut bad_var = gateway_config(Some(GATEWAY_TOKEN));
    bad_var.gateway_token_var = Some("1BAD".into());
    assert_eq!(verify(&h, &bad_var).unwrap_err(), ERR_TOKEN_VAR_INVALID);
    assert_eq!(stored_record(&h), before);

    h.keys.lock_it();
    assert_eq!(
        verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert_eq!(stored_record(&h), before);
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));
    assert_eq!(h.keys.get_raw("claude_code_gateway"), None);
}

// AIC-FR-XTEZ, AIC-FR-XZCS: `ANTHROPIC_BASE_URL` is reserved in subscription
// mode too, and a hand-edited subscription entry of that name is skipped.
#[test]
fn subscription_mode_never_passes_anthropic_base_url_either() {
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("m", "M")]),
        FakeKeychain::new(),
    );
    let mut config = claude_config(Some(SAMPLE_TOKEN));
    config.env_vars = Some(vec!["ANTHROPIC_BASE_URL=https://other.example.com".into()]);
    assert_eq!(verify(&h, &config).unwrap_err(), "env_var_reserved:ANTHROPIC_BASE_URL");

    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records[0].env_vars = vec!["ANTHROPIC_BASE_URL=https://other.example.com".into()];
    h.store.save_agentic_registry(records, active).unwrap();
    let names: Vec<String> = environment_of(&h).into_iter().map(|v| v.name.clone()).collect();
    assert_eq!(names, vec!["CLAUDE_CODE_OAUTH_TOKEN"]);
}

// AIC-FR-XZCS, AIC-FR-ISOC, CCP-FR-20: the gateway launch passes the three
// Bedrock variables and the token, in that order, with only the token masked,
// and no `ANTHROPIC_BASE_URL`.
#[test]
fn the_gateway_launch_environment_is_the_bedrock_four() {
    let (h, _, _) = no_binary_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();

    assert_eq!(
        described(&environment_of(&h)),
        vec![
            ("CLAUDE_CODE_USE_BEDROCK".into(), "1".into(), false),
            ("CLAUDE_CODE_SKIP_BEDROCK_AUTH".into(), "1".into(), false),
            ("ANTHROPIC_BEDROCK_BASE_URL".into(), GATEWAY_URL.into(), false),
            ("ANTHROPIC_AUTH_TOKEN".into(), GATEWAY_TOKEN.into(), true),
        ]
    );
}

// AIC-FR-XZCS: an author entry follows and replaces a Bedrock variable of the
// same name in its place, and a renamed token travels under its name.
#[test]
fn an_author_entry_follows_and_replaces_a_bedrock_variable() {
    let (h, _, _) = no_binary_harness();
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.gateway_token_var = Some("GATEWAY_KEY".into());
    config.env_vars = Some(vec!["CLAUDE_CODE_SKIP_BEDROCK_AUTH=0".into(), "AWS_REGION=eu-west-1".into()]);
    verify(&h, &config).unwrap();

    assert_eq!(
        described(&environment_of(&h)),
        vec![
            ("CLAUDE_CODE_USE_BEDROCK".into(), "1".into(), false),
            ("CLAUDE_CODE_SKIP_BEDROCK_AUTH".into(), "0".into(), false),
            ("ANTHROPIC_BEDROCK_BASE_URL".into(), GATEWAY_URL.into(), false),
            ("GATEWAY_KEY".into(), GATEWAY_TOKEN.into(), true),
            ("AWS_REGION".into(), "eu-west-1".into(), true),
        ]
    );
}

// AIC-FR-XTEZ, AIC-FR-XZCS: `ANTHROPIC_BASE_URL` is reserved in any case, so no
// author entry can pass it, and a hand-edited entry of that name is skipped.
#[test]
fn no_launch_passes_anthropic_base_url() {
    let (h, _, _) = no_binary_harness();
    for name in ["ANTHROPIC_BASE_URL", "anthropic_base_url"] {
        let mut config = gateway_config(Some(GATEWAY_TOKEN));
        config.env_vars = Some(vec![format!("{name}=https://other.example.com")]);
        assert_eq!(verify(&h, &config).unwrap_err(), format!("env_var_reserved:{name}"));
    }
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records[0].env_vars = vec!["ANTHROPIC_BASE_URL=https://other.example.com".into()];
    h.store.save_agentic_registry(records, active).unwrap();

    let names: Vec<String> = environment_of(&h).into_iter().map(|v| v.name.clone()).collect();
    assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("ANTHROPIC_BASE_URL")), "{names:?}");
}

// AIC-FR-CVPW: a token variable name that the launch sets itself, or the one no
// launch passes, is refused in any case.
#[test]
fn a_token_variable_name_the_launch_owns_is_refused() {
    let (h, _, _) = no_binary_harness();
    for name in [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_BEDROCK_BASE_URL",
        "CLAUDE_CODE_USE_BEDROCK",
        "claude_code_skip_bedrock_auth",
    ] {
        let mut config = gateway_config(Some(GATEWAY_TOKEN));
        config.gateway_token_var = Some(name.into());
        assert_eq!(verify(&h, &config).unwrap_err(), ERR_TOKEN_VAR_INVALID, "{name}");
    }
}

// AIC-FR-15, AIC-FR-QHLN: a gateway record's state follows its URL and its
// token, never a binary path.
#[test]
fn a_gateway_state_follows_the_url_and_the_token() {
    let (h, _, _) = no_binary_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    let state = || {
        list_integrations_impl(&h.store, &h.ai)
            .unwrap()
            .into_iter()
            .find(|i| i.vendor == "claude_code")
            .unwrap()
    };
    assert_eq!(state().state, IntegrationState::Verified);

    h.keys.delete("claude_code_gateway").unwrap();
    assert_eq!(state().state, IntegrationState::KeyUnavailable);
    assert_eq!(state().gateway_key_state, KeyState::Unavailable);

    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records[0].gateway_base_url = None;
    h.store.save_agentic_registry(records, active).unwrap();
    assert_eq!(state().state, IntegrationState::Unconfigured);
}

// AIC-FR-30, AIC-FR-QHLN: a gateway configuration resolves an invocation with
// no binary path.
#[test]
fn a_gateway_invocation_needs_no_binary_path() {
    let (h, _, _) = no_binary_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();

    match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).expect("resolves") {
        AgenticInvocation::Cli { vendor, binary_path, .. } => {
            assert_eq!(vendor, "claude_code");
            assert_eq!(binary_path, "");
        }
        other => panic!("expected a CLI invocation, got {other:?}"),
    }
}

// AIC-FR-QHLN, AIC-FR-WNQR: a return to subscription verifies the binary again,
// and a later gateway verification keeps the path it stored.
#[test]
fn subscription_still_verifies_the_binary_and_a_gateway_keeps_its_path() {
    let runner = FakeRunner::saying("/usr/bin/claude", "claude 2.1.4");
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        runner.clone(),
        FakeProber::returning(&[("m", "M")]),
        FakeKeychain::new(),
    );
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).expect("subscription verifies");
    let runs = runner.calls.lock().unwrap().len();
    assert!(runs > 0, "the subscription verification ran the binary");

    // AIC-FR-30: a subscription invocation carries its path.
    set_active_impl(&h.store, &h.ai, "claude_code").unwrap();
    let path_of = || match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap() {
        AgenticInvocation::Cli { binary_path, .. } => binary_path,
        other => panic!("expected a CLI invocation, got {other:?}"),
    };
    assert_eq!(path_of(), "/usr/bin/claude");
    let origin = stored_record(&h).path_origin;

    let record = verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).expect("gateway verifies");
    assert_eq!(runner.calls.lock().unwrap().len(), runs, "the gateway ran nothing");
    assert_eq!(record.binary_path.as_deref(), Some("/usr/bin/claude"));
    assert_eq!(stored_record(&h).path_origin, origin, "the path origin is kept");
    // AIC-FR-QHLN: the gateway record carries no version and the catalog.
    assert_eq!(record.version, None);
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    // AIC-FR-30: a gateway invocation carries no path, even with one stored.
    assert_eq!(path_of(), "");
}
