//! Claude Code's Custom Gateway mode (AIC-FR-WNQR, AIC-FR-UFNB, AIC-FR-CVPW,
//! AIC-FR-IOWS, AIC-FR-QHLN, AIC-FR-YXAB, AIC-FR-XTEZ, AIC-FR-SXVA,
//! AIC-FR-XZCS, AIC-FR-ISOC).

use super::*;

/// A fake gateway token. It is not a credential.
const GATEWAY_TOKEN: &str = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL: &str = "https://gateway.example.com";

fn gateway_harness() -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("claude-a", "Claude A"), ("claude-b", "Claude B")]),
        FakeKeychain::new(),
    )
}

fn gateway_config(token: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/claude".into()),
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

fn value_of<'a>(variables: &'a [LaunchVariable], name: &str) -> Option<&'a LaunchVariable> {
    variables.iter().find(|v| v.name == name)
}

// AIC-FR-WNQR, AIC-FR-UFNB, AIC-FR-CVPW, AIC-FR-IOWS, AIC-FR-QHLN, AIC-FR-YXAB:
// a gateway verification commits the mode, the normalized URL, the default
// token variable name, and the hint, and puts the token in its own vault entry.
#[test]
fn a_gateway_verification_commits_the_mode_the_fields_and_the_token() {
    let h = gateway_harness();
    let record = verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).expect("verifies");

    assert_eq!(record.auth_mode, Some(AuthMode::CustomGateway));
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert_eq!(record.gateway_key_state, KeyState::Set);
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(record.state, IntegrationState::Verified);
    assert!(!record.models.is_empty());
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
    assert_eq!(h.keys.get_raw("claude_code"), None, "the OAuth token is not touched");
}

// AIC-FR-UFNB: a field of the other shape is refused, never ignored.
#[test]
fn a_payload_that_mixes_the_two_shapes_is_refused() {
    let h = gateway_harness();

    let mut subscription = claude_config(Some(SAMPLE_TOKEN));
    subscription.gateway_base_url = Some(GATEWAY_URL.into());
    assert_eq!(verify(&h, &subscription).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    let mut gateway = gateway_config(Some(GATEWAY_TOKEN));
    gateway.oauth_token = Some(SAMPLE_TOKEN.into());
    assert_eq!(verify(&h, &gateway).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    let mut unknown_mode = claude_config(Some(SAMPLE_TOKEN));
    unknown_mode.auth_mode = Some("bearer".into());
    assert_eq!(verify(&h, &unknown_mode).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    // AIC-FR-25: the gateway fields and the variables belong to Claude Code.
    let codex = all_clis_harness();
    let mut for_codex = cli_config("/usr/bin/codex");
    for_codex.env_vars = Some(vec!["A=b".into()]);
    assert_eq!(
        verify_integration_impl(&codex.store, &codex.ai, "codex", &for_codex).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
    let mut for_api = api_config("https://api.example.com/v1", Some("k"));
    for_api.auth_mode = Some("custom_gateway".into());
    assert_eq!(
        verify_integration_impl(&codex.store, &codex.ai, "claude_agent_api", &for_api).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
    assert!(h.runner.calls.lock().unwrap().is_empty(), "no binary ran");
}

// AIC-FR-UFNB, AIC-FR-CVPW, AIC-FR-IOWS: every shape error comes before the
// binary runs and before the vault is touched.
#[test]
fn a_bad_gateway_field_is_refused_before_anything_runs_or_is_stored() {
    let cases: Vec<(&str, Box<dyn Fn(&mut VerifyConfig)>, &str)> = vec![
        ("empty URL", Box::new(|c| c.gateway_base_url = Some("  ".into())), "base_url_empty"),
        ("relative URL", Box::new(|c| c.gateway_base_url = Some("gateway.example.com".into())), "base_url_invalid"),
        (
            "URL carrying a credential",
            Box::new(|c| c.gateway_base_url = Some("https://user:secret@gateway.example.com".into())),
            "base_url_invalid",
        ),
        ("variable with a space", Box::new(|c| c.gateway_token_var = Some("MY TOKEN".into())), "token_var_invalid"),
        ("variable starting with a digit", Box::new(|c| c.gateway_token_var = Some("1TOKEN".into())), "token_var_invalid"),
        ("the base URL variable", Box::new(|c| c.gateway_token_var = Some("ANTHROPIC_BASE_URL".into())), "token_var_invalid"),
        ("a reserved variable", Box::new(|c| c.gateway_token_var = Some("PATH".into())), "token_var_invalid"),
        ("token with a space", Box::new(|c| c.gateway_token = Some("two words".into())), "token_malformed"),
        ("empty token", Box::new(|c| c.gateway_token = Some("   ".into())), "token_malformed"),
        ("token with a line break", Box::new(|c| c.gateway_token = Some("abc\ndef".into())), "token_malformed"),
    ];
    for (label, change, expected) in cases {
        let h = gateway_harness();
        let mut config = gateway_config(Some(GATEWAY_TOKEN));
        change(&mut config);
        assert_eq!(verify(&h, &config).unwrap_err(), expected, "{label}");
        assert!(h.runner.calls.lock().unwrap().is_empty(), "{label}: no binary ran");
        assert!(h.keys.calls.lock().unwrap().iter().all(|(op, _)| *op != "set"), "{label}: nothing stored");
    }

    // No token supplied and none stored.
    let h = gateway_harness();
    assert_eq!(verify(&h, &gateway_config(None)).unwrap_err(), ERR_TOKEN_MISSING);
    assert!(h.runner.calls.lock().unwrap().is_empty());
}

// AIC-FR-IOWS, AIC-FR-CVPW: an absent token keeps the stored one, and a blank
// variable name means the default.
#[test]
fn a_gateway_reverification_keeps_the_stored_token() {
    let h = gateway_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    h.keys.forget_calls();

    let mut again = gateway_config(None);
    again.gateway_token_var = Some("  ".into());
    let record = verify(&h, &again).expect("re-verifies with the stored token");
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert!(h.keys.calls_for("claude_code_gateway").is_empty(), "the token was not rewritten");
}

// AIC-FR-XTEZ: entries are checked, and a bad one is named by its number or its
// name and never by its value.
#[test]
fn environment_entries_are_checked_without_quoting_their_values() {
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["A=b", "no equals sign"], "env_var_invalid:2"),
        (vec!["=value"], "env_var_invalid:1"),
        (vec!["1BAD=x"], "env_var_invalid:1"),
        (vec!["OK=fine", "SECRET=line\nbreak"], "env_var_invalid:2"),
        (vec!["PATH=/tmp"], "env_var_reserved:PATH"),
        (vec!["DOCKER_HOST=tcp://x"], "env_var_reserved:DOCKER_HOST"),
        (vec!["LD_PRELOAD=x"], "env_var_reserved:LD_PRELOAD"),
        (vec!["CLAUDE_CONFIG_DIR=/x"], "env_var_reserved:CLAUDE_CONFIG_DIR"),
    ];
    for (entries, expected) in cases {
        let h = claude_harness();
        let mut config = claude_config(Some(SAMPLE_TOKEN));
        config.env_vars = Some(entries.iter().map(|e| e.to_string()).collect());
        let error = verify(&h, &config).unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.contains("tcp://x") && !error.contains("break"));
        assert!(h.runner.calls.lock().unwrap().is_empty());
    }
}

// AIC-FR-SXVA: an absent list keeps the stored one; a supplied list replaces it,
// and an empty one clears it. Valid in both modes.
#[test]
fn environment_entries_are_kept_replaced_or_cleared() {
    let h = claude_harness();
    let mut first = claude_config(Some(SAMPLE_TOKEN));
    first.env_vars = Some(vec!["HTTPS_PROXY=http://proxy:3128".into(), "EMPTY=".into()]);
    let record = verify(&h, &first).unwrap();
    assert_eq!(record.env_vars, vec!["HTTPS_PROXY=http://proxy:3128", "EMPTY="]);

    let kept = verify(&h, &claude_config(None)).unwrap();
    assert_eq!(kept.env_vars.len(), 2, "an absent list keeps the stored one");

    let mut replace = claude_config(None);
    replace.env_vars = Some(vec!["ONE=1".into()]);
    assert_eq!(verify(&h, &replace).unwrap().env_vars, vec!["ONE=1"]);

    let mut clear = claude_config(None);
    clear.env_vars = Some(vec![]);
    assert!(verify(&h, &clear).unwrap().env_vars.is_empty());
    assert!(stored_record(&h).env_vars.is_empty());
}

// AIC-FR-YXAB: the two tokens are independent. Each mode's verification leaves
// the other mode's token where it was, and the mode follows the last one.
#[test]
fn each_mode_leaves_the_other_modes_token_alone() {
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    let record = verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    assert_eq!(record.auth_mode, Some(AuthMode::CustomGateway));
    assert_eq!(record.key_state, KeyState::Set, "the OAuth token is still held");
    assert_eq!(record.masked_hint.as_deref(), Some("ygAA"));
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));

    let back = verify(&h, &claude_config(None)).unwrap();
    assert_eq!(back.auth_mode, Some(AuthMode::Subscription));
    assert_eq!(back.gateway_key_state, KeyState::Set, "the gateway token is still held");
    assert_eq!(back.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
}

// AIC-FR-15, AIC-FR-WNQR: the state follows the credential of the stored mode.
#[test]
fn the_state_follows_the_credential_of_the_stored_mode() {
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();

    h.keys.wipe("claude_code");
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let claude = find(&listed, "claude_code");
    assert_eq!(claude.state, IntegrationState::Verified, "the OAuth token is not needed");

    h.keys.wipe("claude_code_gateway");
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let claude = find(&listed, "claude_code");
    assert_eq!(claude.state, IntegrationState::KeyUnavailable);
    assert_eq!(claude.gateway_key_state, KeyState::Unavailable);
    assert_eq!(claude.key_state, KeyState::Unset, "the subscription mode is not the stored one");
}

// AIC-FR-25: only Claude Code carries the gateway fields.
#[test]
fn only_claude_code_carries_the_gateway_fields() {
    let h = all_clis_harness();
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    for vendor in ["codex", "opencode", "claude_agent_api", "custom_agent_api"] {
        let record = find(&listed, vendor);
        assert_eq!(record.auth_mode, None, "{vendor}");
        assert_eq!(record.gateway_base_url, None);
        assert_eq!(record.gateway_key_state, KeyState::Unset);
        assert!(record.env_vars.is_empty());
    }
    let claude = find(&listed, "claude_code");
    assert_eq!(claude.auth_mode, Some(AuthMode::Subscription));
}

// AIC-FR-14: clearing removes both tokens and every gateway field.
#[test]
fn clearing_claude_code_removes_both_tokens_and_the_gateway_fields() {
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    let mut gateway = gateway_config(Some(GATEWAY_TOKEN));
    gateway.env_vars = Some(vec!["A=b".into()]);
    verify(&h, &gateway).unwrap();

    let listed = clear_integration_impl(&h.store, &h.ai, "claude_code").unwrap();
    let claude = find(&listed, "claude_code");
    assert_eq!(claude.state, IntegrationState::Unconfigured);
    assert_eq!(claude.auth_mode, Some(AuthMode::Subscription));
    assert_eq!(claude.gateway_base_url, None);
    assert_eq!(claude.gateway_masked_hint, None);
    assert!(claude.env_vars.is_empty());
    assert_eq!(h.keys.get_raw("claude_code"), None);
    assert_eq!(h.keys.get_raw("claude_code_gateway"), None);
}

// AIC-FR-XZCS, AIC-FR-ISOC: gateway mode passes the URL and the token under the
// author's name, and not the OAuth token.
#[test]
fn the_gateway_launch_environment_has_the_url_and_the_token_and_no_oauth_token() {
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.gateway_token_var = Some("ANTHROPIC_API_KEY".into());
    verify(&h, &config).unwrap();

    let variables = environment_of(&h);
    let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
            "ANTHROPIC_BEDROCK_BASE_URL",
            "ANTHROPIC_API_KEY",
        ]
    );
    let url = value_of(&variables, "ANTHROPIC_BEDROCK_BASE_URL").unwrap();
    assert_eq!(url.value.expose(), GATEWAY_URL);
    assert!(!url.masked);
    let token = value_of(&variables, "ANTHROPIC_API_KEY").unwrap();
    assert_eq!(token.value.expose(), GATEWAY_TOKEN);
    assert!(token.masked);
    assert!(value_of(&variables, "CLAUDE_CODE_OAUTH_TOKEN").is_none());
}

// AIC-FR-XZCS, AIC-FR-ISOC: the author's entries follow, win over a variable of
// the same name, and the last of one name wins. Only a value of eight characters
// or more is flagged for masking.
#[test]
fn the_authors_entries_follow_and_win() {
    let h = gateway_harness();
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.env_vars = Some(vec![
        "ANTHROPIC_BEDROCK_BASE_URL=https://override.example.com".into(),
        "SHORT=abc".into(),
        "LONG=abcdefgh".into(),
        "SHORT=xyz".into(),
    ]);
    verify(&h, &config).unwrap();

    let variables = environment_of(&h);
    let url = value_of(&variables, "ANTHROPIC_BEDROCK_BASE_URL").unwrap();
    assert_eq!(url.value.expose(), "https://override.example.com");
    assert_eq!(variables[2].name, "ANTHROPIC_BEDROCK_BASE_URL", "a replaced value keeps its place");
    assert_eq!(value_of(&variables, "SHORT").unwrap().value.expose(), "xyz");
    assert!(!value_of(&variables, "SHORT").unwrap().masked);
    assert!(value_of(&variables, "LONG").unwrap().masked);
    assert_eq!(variables.iter().filter(|v| v.name == "SHORT").count(), 1);
}

// AIC-FR-XZCS: subscription mode still passes the OAuth token, with the author's
// entries after it.
#[test]
fn the_subscription_launch_environment_carries_the_oauth_token_and_the_entries() {
    let h = claude_harness();
    let mut config = claude_config(Some(SAMPLE_TOKEN));
    config.env_vars = Some(vec!["HTTPS_PROXY=http://proxy:3128".into()]);
    verify(&h, &config).unwrap();

    let variables = environment_of(&h);
    let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["CLAUDE_CODE_OAUTH_TOKEN", "HTTPS_PROXY"]);
}

// AIC-FR-30: the handoff reads the credential of the stored mode only.
#[test]
fn the_handoff_refuses_when_the_credential_of_the_stored_mode_is_gone() {
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    h.keys.wipe("claude_code_gateway");
    assert_eq!(
        resolve_agent_launch_credential(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_TOKEN_MISSING,
        "the OAuth token that is still held is not used in gateway mode"
    );
    h.keys.lock_it();
    assert_eq!(
        resolve_agent_launch_credential(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
}

// AIC-FR-20, AIC-FR-SXVA, AIC-FR-31: the token reaches no record, no persisted
// registry, no rendering, and no error.
#[test]
fn the_gateway_token_and_the_entry_values_reach_no_record_or_rendering() {
    let h = gateway_harness();
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.env_vars = Some(vec!["PRIVATE=entry-value-123456".into()]);
    let rendered_config = format!("{config:?}");
    assert!(!rendered_config.contains(GATEWAY_TOKEN));
    assert!(!rendered_config.contains("entry-value-123456"));

    let record = verify(&h, &config).unwrap();
    let wire = serde_json::to_string(&record).unwrap();
    assert!(!wire.contains(GATEWAY_TOKEN));

    let persisted = serde_json::to_string(&stored_record(&h)).unwrap();
    assert!(!persisted.contains(GATEWAY_TOKEN));

    for variable in environment_of(&h) {
        let rendered = format!("{variable:?}");
        assert!(!rendered.contains(GATEWAY_TOKEN));
        assert!(!rendered.contains("entry-value-123456"));
    }
    let credential = resolve_agent_launch_credential(&h.store, &h.ai, "claude_code").unwrap();
    assert!(!format!("{credential:?}").contains(GATEWAY_TOKEN));

    // A failed verification quotes none of it either.
    let failing = gateway_harness();
    failing.keys.lock_it();
    let error = verify(&failing, &config).unwrap_err();
    assert!(!error.contains(GATEWAY_TOKEN));
}

// AIC-FR-24: a locked vault fails a gateway verification with one typed error and
// leaves the registry as it was.
#[test]
fn a_locked_vault_fails_a_gateway_verification_cleanly() {
    let h = gateway_harness();
    h.keys.lock_it();
    assert_eq!(
        verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty());
}

// AIC-FR-02, AIC-FR-20: the presence query asks after the gateway token, and
// the answer reflects what the vault holds.
#[test]
fn the_presence_query_covers_the_gateway_token() {
    let h = gateway_harness();
    assert!(!vendor_presence(&*h.ai.secrets).has("claude_code_gateway"));
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    let present = vendor_presence(&*h.ai.secrets);
    assert!(present.has("claude_code_gateway"));
    assert!(!present.has("claude_code"), "the OAuth token was never stored");
}

// AIC-FR-YXAB, AIC-FR-XZCS: the stored and the launched URL carry no trailing
// slash, and the gateway is not asked.
#[test]
fn the_base_url_is_normalized_before_it_is_stored_and_launched() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        prober.clone(),
        FakeKeychain::new(),
    );
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.gateway_base_url = Some("  https://gateway.example.com/  ".into());
    let record = verify(&h, &config).unwrap();
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    let launch = environment_of(&h);
    assert_eq!(value_of(&launch, "ANTHROPIC_BEDROCK_BASE_URL").unwrap().value.expose(), GATEWAY_URL);
    assert!(prober.calls.lock().unwrap().is_empty(), "the gateway is not asked");
    assert_eq!(h.keys.calls_for("claude_code_gateway"), vec!["set"], "one whole-object write");
}

// AIC-FR-UFNB: a URL carrying a query or a fragment is invalid.
#[test]
fn a_base_url_with_a_query_or_a_fragment_is_refused() {
    for url in [
        "https://gateway.example.com/?api_key=secret-value",
        "https://gateway.example.com/path#fragment",
        "https://gateway.example.com?x=1",
    ] {
        let h = gateway_harness();
        let mut config = gateway_config(Some(GATEWAY_TOKEN));
        config.gateway_base_url = Some(url.into());
        assert_eq!(verify(&h, &config).unwrap_err(), ERR_BASE_URL_INVALID, "{url}");
        assert!(h.runner.calls.lock().unwrap().is_empty());
    }
}

// AIC-FR-YXAB, AIC-FR-20: a short token keeps no hint, so no more than the last
// four characters of a long token are ever kept.
#[test]
fn a_short_gateway_token_keeps_no_hint() {
    let h = gateway_harness();
    let record = verify(&h, &gateway_config(Some("dev"))).unwrap();
    assert_eq!(record.gateway_masked_hint, None);
    assert_eq!(record.gateway_key_state, KeyState::Set);
    assert!(!serde_json::to_string(&stored_record(&h)).unwrap().contains("\"dev\""));

    let record = verify(&h, &gateway_config(Some("12345678"))).unwrap();
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("5678"));
}

// AIC-FR-IOWS, AIC-FR-CVPW: padding is trimmed; a stored name is replaced by the
// default when a blank one is sent; a kept token is the one the gateway sees.
#[test]
fn a_blank_variable_name_means_the_default_and_a_kept_token_is_the_one_sent() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        prober.clone(),
        FakeKeychain::new(),
    );
    let mut first = gateway_config(Some(&format!("  {GATEWAY_TOKEN}\n")));
    first.gateway_token_var = Some("MY_VAR".into());
    assert_eq!(verify(&h, &first).unwrap().gateway_token_var.as_deref(), Some("MY_VAR"));
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));

    let mut again = gateway_config(None);
    again.gateway_token_var = Some("  ".into());
    assert_eq!(verify(&h, &again).unwrap().gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    let launch = environment_of(&h);
    assert_eq!(value_of(&launch, "ANTHROPIC_AUTH_TOKEN").unwrap().value.expose(), GATEWAY_TOKEN);
    assert!(prober.calls.lock().unwrap().is_empty());
}

// AIC-FR-UFNB, AIC-FR-25: every pairing of a field with the wrong shape or the
// wrong vendor is refused.
#[test]
fn every_field_in_the_wrong_place_is_refused() {
    let h = gateway_harness();
    let subscription_with = |change: &dyn Fn(&mut VerifyConfig)| {
        let mut config = claude_config(Some(SAMPLE_TOKEN));
        change(&mut config);
        config
    };
    for config in [
        subscription_with(&|c| c.gateway_token_var = Some("X".into())),
        subscription_with(&|c| c.gateway_token = Some("t".into())),
        subscription_with(&|c| {
            c.auth_mode = Some("subscription".into());
            c.gateway_base_url = Some(GATEWAY_URL.into());
        }),
    ] {
        assert_eq!(verify(&h, &config).unwrap_err(), ERR_WRONG_CONFIG_KIND);
    }
    let mut gateway = gateway_config(Some(GATEWAY_TOKEN));
    gateway.base_url = Some(GATEWAY_URL.into());
    assert_eq!(verify(&h, &gateway).unwrap_err(), ERR_WRONG_CONFIG_KIND);
    let mut gateway = gateway_config(Some(GATEWAY_TOKEN));
    gateway.api_key = Some("k".into());
    assert_eq!(verify(&h, &gateway).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    let others = all_clis_harness();
    for vendor in ["codex", "opencode"] {
        for change in [
            (|c: &mut VerifyConfig| c.auth_mode = Some("custom_gateway".into())) as fn(&mut VerifyConfig),
            |c| c.gateway_base_url = Some(GATEWAY_URL.into()),
            |c| c.gateway_token = Some("t".into()),
            |c| c.gateway_token_var = Some("X".into()),
        ] {
            let mut config = cli_config(&format!("/usr/bin/{vendor}"));
            change(&mut config);
            assert_eq!(
                verify_integration_impl(&others.store, &others.ai, vendor, &config).unwrap_err(),
                ERR_WRONG_CONFIG_KIND,
                "{vendor}"
            );
        }
    }
    for vendor in ["claude_agent_api", "custom_agent_api"] {
        for change in [
            (|c: &mut VerifyConfig| c.env_vars = Some(vec![])) as fn(&mut VerifyConfig),
            |c| c.gateway_base_url = Some(GATEWAY_URL.into()),
            |c| c.auth_mode = Some("subscription".into()),
        ] {
            let mut config = api_config("https://api.example.com/v1", Some("k"));
            change(&mut config);
            assert_eq!(
                verify_integration_impl(&others.store, &others.ai, vendor, &config).unwrap_err(),
                ERR_WRONG_CONFIG_KIND,
                "{vendor}"
            );
        }
    }
}

// AIC-FR-XTEZ, AIC-FR-CVPW: reserved names are reserved in every case, and the
// token variable name follows the same rule.
#[test]
fn reserved_names_are_reserved_in_any_case() {
    for name in [
        "path", "Path", "HOME", "User", "docker_host", "Docker_Config", "ld_preload", "DYLD_X", "SSH_AUTH_SOCK",
        "XDG_RUNTIME_DIR", "xdg_config_home", "USERPROFILE", "systemroot", "GLIBC_TUNABLES", "claude_config_dir",
    ] {
        assert!(is_reserved_variable(name), "{name}");
        let h = claude_harness();
        let mut config = claude_config(Some(SAMPLE_TOKEN));
        config.env_vars = Some(vec![format!("{name}=x")]);
        assert_eq!(verify(&h, &config).unwrap_err(), format!("env_var_reserved:{name}"));
        assert!(h.runner.calls.lock().unwrap().is_empty());

        let g = gateway_harness();
        let mut gateway = gateway_config(Some(GATEWAY_TOKEN));
        gateway.gateway_token_var = Some(name.to_string());
        assert_eq!(verify(&g, &gateway).unwrap_err(), ERR_TOKEN_VAR_INVALID, "{name}");
    }
    let mut lower_base = gateway_config(Some(GATEWAY_TOKEN));
    lower_base.gateway_token_var = Some("anthropic_base_url".into());
    assert_eq!(verify(&gateway_harness(), &lower_base).unwrap_err(), ERR_TOKEN_VAR_INVALID);
    // The proxy variables are not reserved: they are for the CLI in the container.
    for name in ["HTTPS_PROXY", "NO_PROXY", "NODE_EXTRA_CA_CERTS", "TZ"] {
        assert!(!is_reserved_variable(name), "{name}");
    }
}

// AIC-FR-XZCS, AIC-FR-30: the registry is a file the author can edit, so the
// handoff checks what it reads.
#[test]
fn the_handoff_checks_a_hand_edited_registry_again() {
    let h = gateway_harness();
    let mut config = gateway_config(Some(GATEWAY_TOKEN));
    config.env_vars = Some(vec!["FINE=value-one".into()]);
    verify(&h, &config).unwrap();

    let edit = |change: &dyn Fn(&mut AgenticRecord)| {
        let (mut records, active) = h.store.load_agentic_registry().unwrap();
        change(records.iter_mut().find(|r| r.vendor == "claude_code").unwrap());
        h.store.save_agentic_registry(records, active).unwrap();
    };

    edit(&|r| {
        r.env_vars = vec![
            "DOCKER_HOST=tcp://evil:2375".into(),
            "path=/evil".into(),
            "not an entry".into(),
            "FINE=value-one".into(),
        ];
        r.gateway_token_var = Some("PATH".into());
    });
    let variables = environment_of(&h);
    let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
            "ANTHROPIC_BEDROCK_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "FINE",
        ]
    );
    assert_eq!(value_of(&variables, "ANTHROPIC_AUTH_TOKEN").unwrap().value.expose(), GATEWAY_TOKEN);

    // A gateway record with no URL would send the token to the CLI's default host.
    edit(&|r| r.gateway_base_url = None);
    assert_eq!(
        resolve_agent_launch_credential(&h.store, &h.ai, "claude_code").unwrap_err(),
        ERR_REGISTRY_UNAVAILABLE
    );
}

// AIC-FR-XZCS: after a return to subscription mode the gateway fields stay stored
// and the launch environment holds the OAuth token alone.
#[test]
fn a_return_to_subscription_launches_without_the_gateway_variables() {
    let h = gateway_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    assert_eq!(stored_record(&h).gateway_base_url.as_deref(), Some(GATEWAY_URL));

    let variables = environment_of(&h);
    let names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["CLAUDE_CODE_OAUTH_TOKEN"]);
}

// AIC-FR-ISOC: the flag follows the source of the variable. The gateway's own
// base URL is not masked; an author entry of that name is masked by its length.
#[test]
fn the_masked_flag_follows_where_the_value_came_from() {
    let h = gateway_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    let variables = environment_of(&h);
    assert!(!value_of(&variables, "ANTHROPIC_BEDROCK_BASE_URL").unwrap().masked);

    let mut config = gateway_config(None);
    config.env_vars = Some(vec![
        "ANTHROPIC_BEDROCK_BASE_URL=https://override.example.com".into(),
        "SEVEN77=1234567".into(),
        "EIGHT88=12345678".into(),
    ]);
    verify(&h, &config).unwrap();
    let variables = environment_of(&h);
    assert!(value_of(&variables, "ANTHROPIC_BEDROCK_BASE_URL").unwrap().masked);
    assert!(!value_of(&variables, "SEVEN77").unwrap().masked);
    assert!(value_of(&variables, "EIGHT88").unwrap().masked);
}

// AIC-FR-15, AIC-FR-24, AIC-FR-QHLN: a locked vault makes a gateway record
// unavailable, and a missing binary does not matter to a gateway record.
#[test]
fn a_gateway_record_degrades_by_the_same_rules_as_a_subscription_record() {
    let h = gateway_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();

    h.keys.lock_it();
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&listed, "claude_code").state, IntegrationState::KeyUnavailable);

    // A path stored by an earlier subscription verification stays, and its
    // binary going away does not matter to the gateway record.
    let h = gateway_harness();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    assert_eq!(stored_record(&h).binary_path.as_deref(), Some("/usr/bin/claude"));
    h.fs.remove("/usr/bin/claude");
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&listed, "claude_code").state, IntegrationState::Verified);
    h.keys.wipe("claude_code_gateway");
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(find(&listed, "claude_code").state, IntegrationState::KeyUnavailable);

    // Subscription mode with its token gone, while a gateway token is held.
    let h = gateway_harness();
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN))).unwrap();
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();
    h.keys.wipe("claude_code");
    let listed = list_integrations_impl(&h.store, &h.ai).unwrap();
    let claude = find(&listed, "claude_code");
    assert_eq!(claude.state, IntegrationState::KeyUnavailable);
    assert_eq!(claude.key_state, KeyState::Unavailable);
    assert_eq!(claude.gateway_key_state, KeyState::Set);
}
