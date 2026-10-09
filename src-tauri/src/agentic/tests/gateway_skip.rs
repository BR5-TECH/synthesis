//! Claude Code's Custom Gateway accepted without the gateway check
//! (AIC-FR-KWMV, AIC-FR-UFNB, AIC-FR-DRPC, AIC-FR-25).

use super::*;

/// A fake gateway token. It is not a credential.
const GATEWAY_TOKEN: &str = "gw-FAKE-TEST-TOKEN-NOT-A-CREDENTIAL-k2Qz";
const GATEWAY_URL: &str = "https://bedrock-gateway.example.com";

fn claude_harness(prober: std::sync::Arc<FakeProber>) -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        prober,
        FakeKeychain::new(),
    )
}

fn gateway_config(token: Option<&str>, skip: Option<bool>) -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        auth_mode: Some("custom_gateway".into()),
        gateway_base_url: Some(GATEWAY_URL.into()),
        gateway_token: token.map(str::to_string),
        skip_gateway_check: skip,
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

// AIC-FR-KWMV: a gateway that refuses `/v1/models` is accepted when the author
// skips the check. The gateway is not asked, the catalog is the model list, and
// the URL, the variable name, the hint, the token, and the flag are committed.
#[test]
fn a_skipped_check_asks_no_gateway_and_commits_the_configuration() {
    let prober = FakeProber::failing(ProbeError::Status(400));
    let h = claude_harness(prober.clone());

    assert_eq!(
        verify(&h, &gateway_config(Some(GATEWAY_TOKEN), None)).unwrap_err(),
        "gateway_status:400"
    );
    let record = verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).expect("accepted");

    assert_eq!(prober.calls.lock().unwrap().len(), 1, "only the first verification asked");
    assert_eq!(record.state, IntegrationState::Verified);
    assert_eq!(record.auth_mode, Some(AuthMode::CustomGateway));
    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    assert_eq!(record.gateway_key_state, KeyState::Set);
    assert_eq!(record.gateway_masked_hint.as_deref(), Some("k2Qz"));
    assert!(record.gateway_check_skipped);
    assert_eq!(record.models_origin, ModelsOrigin::Catalog);
    assert!(!record.models.is_empty());
    assert_eq!(record.version.as_deref(), Some("2.1.4"));
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
    assert!(stored_record(&h).gateway_check_skipped, "the flag is persisted");
}

// AIC-FR-KWMV: the binary is still verified, and a binary failure persists
// nothing.
#[test]
fn a_skipped_check_still_needs_a_binary_that_verifies() {
    let prober = FakeProber::returning(&[("m", "M")]);
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "not the expected program"),
        prober.clone(),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).unwrap_err(),
        ERR_NOT_THE_EXPECTED_CLI
    );
    assert_eq!(h.keys.get_raw("claude_code_gateway"), None);
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "the registry is unchanged");
}

// AIC-FR-KWMV, AIC-FR-06: a skipped verification whose binary fails leaves a
// stored skipped configuration exactly as it was.
#[test]
fn a_failed_skip_keeps_the_stored_configuration() {
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/other"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::failing(ProbeError::Status(400)),
        FakeKeychain::new(),
    );
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).unwrap();
    let before = stored_record(&h);

    let mut other = gateway_config(Some("gw-ANOTHER-FAKE-TOKEN-zzzz"), Some(true));
    other.path = Some("/usr/bin/other".into());
    other.gateway_base_url = Some("https://other.example.com".into());
    assert!(verify(&h, &other).is_err());

    assert_eq!(stored_record(&h), before);
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some(GATEWAY_TOKEN));
}

// AIC-FR-KWMV, AIC-FR-IOWS, AIC-FR-UFNB: the field checks still run when the
// gateway check is skipped.
#[test]
fn a_skipped_check_keeps_every_field_check() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));

    assert_eq!(
        verify(&h, &gateway_config(None, Some(true))).unwrap_err(),
        ERR_TOKEN_MISSING
    );
    assert_eq!(
        verify(&h, &gateway_config(Some("has space"), Some(true))).unwrap_err(),
        ERR_TOKEN_MALFORMED
    );
    let mut no_url = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    no_url.gateway_base_url = Some("  ".into());
    assert_eq!(verify(&h, &no_url).unwrap_err(), ERR_BASE_URL_EMPTY);
    let mut bad_entry = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    bad_entry.env_vars = Some(vec!["no equals sign".into()]);
    assert_eq!(verify(&h, &bad_entry).unwrap_err(), "env_var_invalid:1");
    let mut reserved = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    reserved.env_vars = Some(vec!["PATH=/tmp".into()]);
    assert_eq!(verify(&h, &reserved).unwrap_err(), "env_var_reserved:PATH");
    let mut bad_var = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    bad_var.gateway_token_var = Some("1BAD".into());
    assert_eq!(verify(&h, &bad_var).unwrap_err(), ERR_TOKEN_VAR_INVALID);
    let mut with_credential = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    with_credential.gateway_base_url = Some("https://user:secret@gateway.example.com".into());
    assert_eq!(verify(&h, &with_credential).unwrap_err(), ERR_BASE_URL_INVALID);
    let mut with_oauth = gateway_config(Some(GATEWAY_TOKEN), Some(true));
    with_oauth.oauth_token = Some(SAMPLE_TOKEN.into());
    assert_eq!(verify(&h, &with_oauth).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    assert!(h.keys.calls_for("claude_code_gateway").iter().all(|c| *c != "set"), "no token stored");
    let (records, _) = h.store.load_agentic_registry().unwrap();
    assert!(records.is_empty(), "the registry is unchanged");
}

// AIC-FR-KWMV: a verification that runs the check clears the flag, and the
// listed models replace the catalog.
#[test]
fn a_checked_verification_clears_the_flag() {
    let h = claude_harness(FakeProber::returning(&[("claude-a", "Claude A")]));
    assert!(verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).unwrap().gateway_check_skipped);

    let record = verify(&h, &gateway_config(None, Some(false))).expect("checked");
    assert!(!record.gateway_check_skipped);
    assert_eq!(record.models_origin, ModelsOrigin::Probed);
    assert!(!stored_record(&h).gateway_check_skipped);

    verify(&h, &gateway_config(None, Some(true))).unwrap();
    let record = verify(&h, &gateway_config(None, None)).expect("checked");
    assert!(!record.gateway_check_skipped, "an absent flag runs the check");
}

// AIC-FR-KWMV: a skipped check returns a gateway-listed model list to the
// catalog, so no model the gateway was not asked about stays selectable.
#[test]
fn a_skipped_check_replaces_a_listed_model_list_with_the_catalog() {
    let h = claude_harness(FakeProber::returning(&[("gw-only", "Gateway only")]));
    let probed = verify(&h, &gateway_config(Some(GATEWAY_TOKEN), None)).unwrap();
    assert_eq!(probed.models_origin, ModelsOrigin::Probed);

    let skipped = verify(&h, &gateway_config(None, Some(true))).unwrap();
    assert_eq!(skipped.models_origin, ModelsOrigin::Catalog);
    assert!(skipped.models.iter().all(|m| m.id != "gw-only"));
}

// AIC-FR-KWMV: a subscription verification clears the flag.
#[test]
fn a_subscription_verification_clears_the_flag() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).unwrap();

    let record = verify(&h, &claude_config(Some(SAMPLE_TOKEN))).expect("subscription");
    assert_eq!(record.auth_mode, Some(AuthMode::Subscription));
    assert!(!record.gateway_check_skipped);
    assert!(!stored_record(&h).gateway_check_skipped);
}

// AIC-FR-UFNB, AIC-FR-25: the flag belongs to the gateway payload of Claude Code
// alone.
#[test]
fn the_flag_outside_a_gateway_payload_is_the_wrong_kind() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    let mut subscription = claude_config(Some(SAMPLE_TOKEN));
    subscription.skip_gateway_check = Some(true);
    assert_eq!(verify(&h, &subscription).unwrap_err(), ERR_WRONG_CONFIG_KIND);
    subscription.skip_gateway_check = Some(false);
    assert_eq!(verify(&h, &subscription).unwrap_err(), ERR_WRONG_CONFIG_KIND);

    let codex = all_clis_harness();
    let mut for_codex = cli_config("/usr/bin/codex");
    for_codex.skip_gateway_check = Some(true);
    assert_eq!(
        verify_integration_impl(&codex.store, &codex.ai, "codex", &for_codex).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
}

// AIC-FR-25: no vendor but a skipped Claude Code gateway reports the flag, and a
// record with the flag off does not write it to the settings file.
#[test]
fn only_a_skipped_claude_gateway_reports_the_flag() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN), Some(true))).unwrap();
    // A hand-edited file that puts the flag on another vendor's record.
    let (mut records, active) = h.store.load_agentic_registry().unwrap();
    records.push(AgenticRecord {
        gateway_check_skipped: true,
        ..AgenticRecord::empty("codex")
    });
    h.store.save_agentic_registry(records, active).unwrap();

    for integration in list_integrations_impl(&h.store, &h.ai).unwrap() {
        assert_eq!(
            integration.gateway_check_skipped,
            integration.vendor == "claude_code",
            "{}",
            integration.vendor
        );
    }

    let off = toml::to_string(&AgenticRecord::empty("claude_code")).unwrap();
    assert!(!off.contains("gatewayCheckSkipped"));
    let on = toml::to_string(&AgenticRecord {
        gateway_check_skipped: true,
        ..AgenticRecord::empty("claude_code")
    })
    .unwrap();
    assert!(on.contains("gatewayCheckSkipped = true"));
}

// AIC-FR-KWMV, AIC-FR-YXAB: a skipped check commits what a checked one commits:
// the normalized URL, a named variable, the entries, no hint for a short token,
// and no change to the OAuth token.
#[test]
fn a_skipped_check_commits_what_a_checked_one_commits() {
    let h = claude_harness(FakeProber::failing(ProbeError::Status(400)));
    verify(&h, &claude_config(Some(SAMPLE_TOKEN))).unwrap();

    let mut config = gateway_config(Some("short"), Some(true));
    config.gateway_base_url = Some(format!("  {GATEWAY_URL}/  "));
    config.gateway_token_var = Some("ANTHROPIC_API_KEY".into());
    config.env_vars = Some(vec!["HTTPS_PROXY=http://proxy:3128".into()]);
    let record = verify(&h, &config).unwrap();

    assert_eq!(record.gateway_base_url.as_deref(), Some(GATEWAY_URL));
    assert_eq!(record.gateway_token_var.as_deref(), Some("ANTHROPIC_API_KEY"));
    assert_eq!(record.env_vars, vec!["HTTPS_PROXY=http://proxy:3128".to_string()]);
    assert_eq!(record.gateway_masked_hint, None);
    assert_eq!(h.keys.get_raw("claude_code_gateway").as_deref(), Some("short"));
    assert_eq!(h.keys.get_raw("claude_code").as_deref(), Some(SAMPLE_TOKEN));
}

// AIC-FR-KWMV, AIC-FR-11: a model the gateway listed is no longer selected once
// a skipped check returns the list to the catalog.
#[test]
fn a_skipped_check_drops_selections_the_catalog_does_not_offer() {
    let h = claude_harness(FakeProber::returning(&[("gw-only", "Gateway only")]));
    verify(&h, &gateway_config(Some(GATEWAY_TOKEN), None)).unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("gw-only")).unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", Some("review"), Some("gw-only")).unwrap();

    verify(&h, &gateway_config(None, Some(true))).unwrap();

    let record = stored_record(&h);
    assert_eq!(record.selected_model, None);
    assert!(record.model_overrides.is_empty());
}

// AIC-FR-UFNB, AIC-FR-25: an API-kind vendor refuses the flag too.
#[test]
fn an_api_vendor_refuses_the_flag() {
    let h = claude_harness(FakeProber::returning(&[("m", "M")]));
    let mut config = api_config("https://agents.corp/v1", Some("k"));
    config.skip_gateway_check = Some(true);
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "claude_agent_api", &config).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
}

// AIC-FR-UFNB: the payload names the flag `skipGatewayCheck` on the wire.
#[test]
fn the_flag_has_its_wire_name() {
    let config: VerifyConfig = serde_json::from_str(
        r#"{"path":"/usr/bin/claude","authMode":"custom_gateway","gatewayBaseUrl":"https://g","skipGatewayCheck":true}"#,
    )
    .unwrap();
    assert_eq!(config.skip_gateway_check, Some(true));
}
