//! The invocation read path, and that no key reaches the store
//! (AIC-FR-16, AIC-FR-19, AIC-FR-20, AIC-FR-25).

use super::*;

// -- AIC-FR-16, AIC-FR-19: the invocation read path ------------------------------

#[test]
fn aic_ts21_an_invocation_is_tagged_by_kind_and_refuses_when_nothing_resolves() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        FakeKeychain::new(),
    );

    // Nothing configured at all.
    assert_eq!(
        resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap_err(),
        ERR_NONE_CONFIGURED
    );

    verify_integration_impl(&h.store, &h.ai, "claude_code", &cli_config_for("claude_code", "/usr/bin/claude"))
        .unwrap();
    set_model_impl(&h.store, &h.ai, "claude_code", None, Some("opus")).unwrap();
    set_effort_impl(&h.store, &h.ai, "claude_code", None, Some("high")).unwrap();

    match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap() {
        AgenticInvocation::Cli {
            vendor,
            binary_path,
            model_id,
            effort_id,
        } => {
            assert_eq!(vendor, "claude_code");
            assert_eq!(binary_path, "/usr/bin/claude");
            assert_eq!(model_id.as_deref(), Some("opus"));
            assert_eq!(effort_id.as_deref(), Some("high"));
        }
        other => panic!("expected a CLI invocation, got {other:?}"),
    }

    // Now an API agent, overridden for this project.
    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c")),
    )
    .unwrap();
    set_project_impl(&h.store, &h.ai, "/dev/acme", Some("claude_agent_api")).unwrap();
    match resolve_agentic_invocation(&h.store, &h.ai, "/dev/acme", None).unwrap() {
        AgenticInvocation::Api {
            vendor,
            base_url,
            api_key,
            ..
        } => {
            assert_eq!(vendor, "claude_agent_api");
            assert_eq!(base_url, "https://api.anthropic.com/v1");
            assert_eq!(
                api_key.as_deref(),
                Some("sk-ant-a71c"),
                "the one place a key is read back, for the caller about to present it"
            );
        }
        other => panic!("expected an API invocation, got {other:?}"),
    }
}

// -- AIC-FR-20, AIC-FR-25: no key in the store -----------------------------------

#[test]
fn aic_ts22_no_key_reaches_the_registry_and_a_cli_record_has_no_key_field() {
    // Every CLI is really executable here, so the credential assertions
    // below run against *configured* codex and opencode records. Against
    // unconfigured ones they would read the struct defaults and hold even
    // if a configured Codex wrongly carried a hint.
    let h = all_clis_harness();
    for (vendor, path) in CLI_BINARIES {
        verify_integration_impl(&h.store, &h.ai, vendor, &cli_config_for(vendor, path))
            .unwrap_or_else(|e| panic!("{vendor} should verify, got {e}"));
    }
    verify_integration_impl(
        &h.store,
        &h.ai,
        "claude_agent_api",
        &api_config("https://api.anthropic.com/v1", Some("sk-ant-secret-a71c")),
    )
    .unwrap();

    let (records, _) = h.store.load_agentic_registry().unwrap();
    let as_text = serde_json::to_string(&records).unwrap();
    assert!(
        !as_text.contains("sk-ant-secret"),
        "no key material may reach the registry"
    );
    assert!(
        !as_text.contains(SAMPLE_TOKEN) && !as_text.contains("sk-ant-oat01-"),
        "no OAuth token material may reach the registry"
    );
    assert!(as_text.contains("a71c"), "the masked hint is the exception");
    assert!(as_text.contains("ygAA"), "and so is the token's");

    // AIC-FR-25: Claude Code carries the credential fields because it holds
    // a credential; Codex and OpenCode carry none because they hold nothing.
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    let claude = find(&list, "claude_code");
    assert!(claude.key_required);
    assert_eq!(claude.key_state, KeyState::Set);
    assert_eq!(claude.masked_hint.as_deref(), Some("ygAA"));
    for vendor in ["codex", "opencode"] {
        let rec = find(&list, vendor);
        assert!(!rec.key_required, "{vendor} holds no credential");
        assert_eq!(rec.key_state, KeyState::Unset, "{vendor}");
        assert_eq!(rec.masked_hint, None, "{vendor}");
    }
}
