//! Codex's two authentication modes: the payloads, the vault, the state, and
//! the launch handoff (AIC-FR-14, AIC-FR-15, AIC-FR-20, AIC-FR-25,
//! AIC-FR-28, AIC-FR-29, AIC-FR-30, AIC-FR-32, AIC-FR-UFYH, AIC-FR-ISWS,
//! AIC-FR-BXFL, AIC-FR-OFEO, AIC-FR-KAVY).

use super::*;

/// A fake access token. It is not a real credential.
const CODEX_TOKEN: &str = "codex-at-FAKE-TEST-TOKEN-0000000000-wxyz";

fn codex_harness() -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "codex-cli 0.172.0"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    )
}

fn token_config(token: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/codex".into()),
        auth_mode: Some("access_token".into()),
        access_token: token.map(str::to_string),
        ..Default::default()
    }
}

fn folder_config() -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/codex".into()),
        auth_mode: Some("login_folder".into()),
        ..Default::default()
    }
}

fn codex(h: &Harness) -> AgenticIntegration {
    find(&list_integrations_impl(&h.store, &h.ai).unwrap(), "codex").clone()
}

// AIC-FR-UFYH / AIC-FR-OFEO / AIC-FR-25 / AIC-FR-20: an `access_token`
// verification stores the token at the vendor id `codex`, commits the mode,
// and reports the token's presence and hint, never the token.
#[test]
fn an_access_token_verification_stores_the_token_and_the_mode() {
    let h = codex_harness();
    let record =
        verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN)))
            .unwrap();

    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN));
    assert_eq!(record.codex_auth_mode, Some(CodexAuthMode::AccessToken));
    assert_eq!(record.key_state, KeyState::Set);
    assert_eq!(record.masked_hint.as_deref(), Some("wxyz"));
    assert!(!record.key_required, "the access token is optional");
    assert_eq!(record.state, IntegrationState::Verified);
    assert_eq!(record.auth_mode, None, "the Claude Code mode is not Codex's");

    // AIC-FR-29: no field of the wire record carries the token.
    let wire = serde_json::to_string(&record).unwrap();
    assert!(!wire.contains(CODEX_TOKEN));
    assert!(wire.contains("\"codexAuthMode\":\"access_token\""), "{wire}");
}

// AIC-FR-UFYH / AIC-FR-25: a record with no stored mode reads as
// `login_folder`, and every other vendor carries no Codex mode.
#[test]
fn a_record_without_a_mode_reads_as_login_folder() {
    let h = codex_harness();
    let record =
        verify_integration_impl(&h.store, &h.ai, "codex", &cli_config("/usr/bin/codex")).unwrap();
    assert_eq!(record.codex_auth_mode, Some(CodexAuthMode::LoginFolder));
    assert_eq!(record.key_state, KeyState::Unset);
    assert_eq!(record.masked_hint, None);
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    for other in list.iter().filter(|i| i.vendor != "codex") {
        assert_eq!(other.codex_auth_mode, None, "{}", other.vendor);
    }
    // The default mode is not written to the file.
    let (records, _) = h.store.load_agentic_registry().unwrap();
    let stored = records.iter().find(|r| r.vendor == "codex").unwrap();
    assert_eq!(stored.codex_auth_mode, CodexAuthMode::LoginFolder);
}

// AIC-FR-OFEO: a token shorter than eight characters keeps no hint.
#[test]
fn a_short_access_token_keeps_no_hint() {
    let h = codex_harness();
    let record =
        verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some("abc1234"))).unwrap();
    assert_eq!(record.masked_hint, None);
    assert_eq!(record.key_state, KeyState::Set);
}

// AIC-FR-BXFL: a supplied token is trimmed, and one that is then empty, too
// long, or holds whitespace or a control character is `token_malformed`,
// before any binary runs and before any vault write.
#[test]
fn a_malformed_access_token_is_refused_before_anything_runs() {
    let too_long = "a".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES + 1);
    let too_long_multibyte = "é".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES / 2 + 1);
    for token in [
        "",
        "   ",
        "two words",
        "tab\there",
        "ctrl\u{7}char",
        too_long.as_str(),
        // Non-ASCII whitespace and the C1 control characters, as the UI judges them.
        "no\u{a0}break",
        "line\u{2028}separator",
        "next\u{85}line",
        "\u{85}leading-next-line",
        "byte\u{feff}order",
        too_long_multibyte.as_str(),
    ] {
        let h = codex_harness();
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(token)))
                .unwrap_err(),
            ERR_TOKEN_MALFORMED,
            "{token:?}"
        );
        assert!(h.runner.calls.lock().unwrap().is_empty(), "no binary ran for {token:?}");
        assert!(h.keys.calls_for("codex").is_empty(), "no vault write for {token:?}");
    }

    let h = codex_harness();
    let longest = "b".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES);
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(&longest))).unwrap();
    // The limit counts bytes after the trim, so surrounding spaces do not count.
    let padded = format!("  {longest}");
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(&padded))).unwrap();
    let multibyte = "é".repeat(CODEX_ACCESS_TOKEN_MAX_BYTES / 2);
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(&multibyte))).unwrap();
    // U+FEFF and non-ASCII spaces around a token are trimmed, as the UI trims them.
    let wrapped = format!("\u{feff}\u{a0}{CODEX_TOKEN}\u{2028}");
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(&wrapped))).unwrap();
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN));
    verify_integration_impl(
        &h.store,
        &h.ai,
        "codex",
        &token_config(Some(&format!("  {CODEX_TOKEN}\n"))),
    )
    .unwrap();
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN), "stored trimmed");
}

// AIC-FR-ISWS / AIC-FR-UFYH: a field of another shape, an access token in a
// `login_folder` payload, or an unknown mode is `wrong_config_kind`.
#[test]
fn a_codex_payload_of_another_shape_is_refused() {
    let base = || VerifyConfig { path: Some("/usr/bin/codex".into()), ..Default::default() };
    let cases = [
        VerifyConfig { oauth_token: Some(SAMPLE_TOKEN.into()), ..base() },
        VerifyConfig { gateway_base_url: Some("https://gw.example".into()), ..base() },
        VerifyConfig { gateway_token: Some("t".into()), ..base() },
        VerifyConfig { env_vars: Some(vec!["A=b".into()]), ..base() },
        VerifyConfig { access_token: Some(CODEX_TOKEN.into()), ..base() },
        VerifyConfig { access_token: Some(CODEX_TOKEN.into()), ..folder_config() },
        VerifyConfig { auth_mode: Some("subscription".into()), ..base() },
        VerifyConfig { auth_mode: Some("api_key".into()), ..base() },
        VerifyConfig { base_url: Some("https://x.example".into()), ..base() },
    ];
    for config in cases {
        let h = codex_harness();
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, "codex", &config).unwrap_err(),
            ERR_WRONG_CONFIG_KIND,
            "{config:?}"
        );
        assert!(h.runner.calls.lock().unwrap().is_empty());
    }

    // The Codex fields belong to Codex alone.
    let h = all_clis_harness();
    let codex_fields = |path: &str| VerifyConfig {
        path: Some(path.into()),
        access_token: Some(CODEX_TOKEN.into()),
        ..Default::default()
    };
    for (vendor, path) in [("claude_code", "/usr/bin/claude"), ("opencode", "/usr/bin/opencode")] {
        assert_eq!(
            verify_integration_impl(&h.store, &h.ai, vendor, &codex_fields(path)).unwrap_err(),
            ERR_WRONG_CONFIG_KIND,
            "{vendor}"
        );
    }
    let opencode_mode = VerifyConfig {
        path: Some("/usr/bin/opencode".into()),
        auth_mode: Some("login_folder".into()),
        ..Default::default()
    };
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "opencode", &opencode_mode).unwrap_err(),
        ERR_WRONG_CONFIG_KIND
    );
}

// AIC-FR-ISWS: an `access_token` payload with no token keeps the stored one,
// and returns `token_missing` where none is stored.
#[test]
fn an_access_token_payload_without_a_token_keeps_the_stored_one() {
    let h = codex_harness();
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "codex", &token_config(None)).unwrap_err(),
        ERR_TOKEN_MISSING
    );
    assert!(h.store.load_agentic_registry().unwrap().0.is_empty(), "nothing persisted");

    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    h.keys.forget_calls();
    let again = verify_integration_impl(&h.store, &h.ai, "codex", &token_config(None)).unwrap();
    assert!(h.keys.calls_for("codex").is_empty(), "the stored token is not rewritten");
    assert_eq!(again.masked_hint.as_deref(), Some("wxyz"));
    assert_eq!(again.key_state, KeyState::Set);

    // A vault that will not answer is not a missing token.
    let locked = codex_harness();
    locked.keys.lock_it();
    assert_eq!(
        verify_integration_impl(&locked.store, &locked.ai, "codex", &token_config(None))
            .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
}

// AIC-FR-OFEO / AIC-FR-UFYH: a `login_folder` verification never touches the
// stored token, so a return to `access_token` mode verifies with it.
#[test]
fn a_login_folder_verification_keeps_the_access_token() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    h.keys.forget_calls();

    let folder = verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    assert!(h.keys.calls_for("codex").is_empty(), "the vault is not touched");
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN));
    assert_eq!(folder.codex_auth_mode, Some(CodexAuthMode::LoginFolder));
    assert_eq!(folder.key_state, KeyState::Unset, "login_folder mode reads no token");
    assert_eq!(folder.masked_hint, None);

    // A locked vault does not fail a verification that needs no token.
    h.keys.lock_it();
    verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    *h.keys.locked.lock().unwrap() = false;

    let back = verify_integration_impl(&h.store, &h.ai, "codex", &token_config(None)).unwrap();
    assert_eq!(back.codex_auth_mode, Some(CodexAuthMode::AccessToken));
    assert_eq!(back.masked_hint.as_deref(), Some("wxyz"));
}

// AIC-FR-28 / AIC-FR-06: a verification that fails after its token was
// accepted leaves the vault and the registry exactly as they were.
#[test]
fn a_failed_verification_leaves_the_token_and_the_record() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    let before = h.store.load_agentic_registry().unwrap();
    h.keys.forget_calls();

    let wrong_path = VerifyConfig {
        path: Some("/nowhere/codex".into()),
        ..token_config(Some("codex-at-ANOTHER-TOKEN-1111"))
    };
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "codex", &wrong_path).unwrap_err(),
        ERR_NOT_FOUND
    );
    assert!(h.keys.calls_for("codex").is_empty());
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN));
    assert_eq!(h.store.load_agentic_registry().unwrap(), before);
}

// AIC-FR-15 / AIC-FR-25: in `access_token` mode, a token the vault no longer
// holds makes the record `key_unavailable`; a missing path wins over it; in
// `login_folder` mode the token does not matter.
#[test]
fn the_codex_state_follows_the_credential_of_its_mode() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    h.keys.wipe("codex");
    let degraded = codex(&h);
    assert_eq!(degraded.state, IntegrationState::KeyUnavailable);
    assert_eq!(degraded.key_state, KeyState::Unavailable);
    assert_eq!(
        set_active_impl(&h.store, &h.ai, "codex").unwrap_err(),
        ERR_NOT_VERIFIED,
        "a degraded integration cannot be activated"
    );

    h.fs.remove("/usr/bin/codex");
    assert_eq!(codex(&h).state, IntegrationState::Missing, "missing wins");

    let folder = codex_harness();
    verify_integration_impl(&folder.store, &folder.ai, "codex", &folder_config()).unwrap();
    assert_eq!(codex(&folder).state, IntegrationState::Verified);
}

// AIC-FR-14: clearing Codex deletes its access token and returns it to
// `login_folder`.
#[test]
fn clearing_codex_deletes_the_token_and_the_mode() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    let list = clear_integration_impl(&h.store, &h.ai, "codex").unwrap();
    assert_eq!(h.keys.get_raw("codex"), None);
    let cleared = find(&list, "codex");
    assert_eq!(cleared.state, IntegrationState::Unconfigured);
    assert_eq!(cleared.codex_auth_mode, Some(CodexAuthMode::LoginFolder));
    assert_eq!(cleared.masked_hint, None);
    clear_integration_impl(&h.store, &h.ai, "codex").unwrap();
}

// AIC-FR-32 / `ASV-application-secret-vault.md` ASV-FR-20: no migration
// candidate is supplied for Codex, whose token no earlier layout stored.
#[test]
fn codex_supplies_no_migration_candidate() {
    let records: Vec<AgenticRecord> =
        VENDORS.iter().map(|d| AgenticRecord::empty(d.vendor)).collect();
    let accounts: Vec<String> =
        migration_candidates(&records).into_iter().map(|c| c.legacy_account).collect();
    assert!(!accounts.iter().any(|a| a == "codex"), "{accounts:?}");
    assert!(!accounts.iter().any(|a| a == "opencode"), "{accounts:?}");
    assert!(accounts.iter().any(|a| a == "claude_code"), "{accounts:?}");
}

// AIC-FR-KAVY / AIC-FR-30: in `access_token` mode the handoff is one masked
// `CODEX_ACCESS_TOKEN` and no mount; in `login_folder` mode it is the mount.
#[test]
fn the_codex_handoff_follows_its_mode() {
    let home = tempfile::tempdir().expect("temp home");
    std::fs::create_dir_all(home.path().join(".codex")).expect("login dir");
    let h = codex_harness();
    let h = Harness { ai: h.ai.with_home(home.path()), ..h };

    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    let credential = resolve_agent_launch_credential(&h.store, &h.ai, "codex").unwrap();
    assert_eq!(credential.codex_auth_mode(), Some(CodexAuthMode::AccessToken));
    let rendered = format!("{credential:?}");
    assert!(!rendered.contains(CODEX_TOKEN), "{rendered}");
    match credential {
        AgentLaunchCredential::CodexEnvironment(variables) => {
            assert_eq!(variables.len(), 1);
            assert_eq!(variables[0].name, "CODEX_ACCESS_TOKEN");
            assert_eq!(variables[0].value.expose(), CODEX_TOKEN);
            assert!(variables[0].masked);
        }
        other => panic!("expected an environment, got {other:?}"),
    }

    verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    let credential = resolve_agent_launch_credential(&h.store, &h.ai, "codex").unwrap();
    assert_eq!(credential.codex_auth_mode(), Some(CodexAuthMode::LoginFolder));
    assert!(matches!(credential, AgentLaunchCredential::CodexConfigMount { .. }));
}

// AIC-FR-30: in `access_token` mode, a token the vault does not hold is
// `token_missing`, a vault that will not answer is `keychain_unavailable`,
// and no login directory is needed.
#[test]
fn the_access_token_handoff_refuses_without_a_token() {
    let empty_home = tempfile::tempdir().expect("temp home");
    let h = codex_harness();
    let h = Harness { ai: h.ai.with_home(empty_home.path()), ..h };
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    assert!(
        resolve_agent_launch_credential(&h.store, &h.ai, "codex").is_ok(),
        "no login directory is needed in access_token mode"
    );

    h.keys.lock_it();
    assert_eq!(
        resolve_agent_launch_credential(&h.store, &h.ai, "codex").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    *h.keys.locked.lock().unwrap() = false;
    h.keys.wipe("codex");
    assert_eq!(
        resolve_agent_launch_credential(&h.store, &h.ai, "codex").unwrap_err(),
        ERR_TOKEN_MISSING
    );

    // The Claude Code handoff names no Codex mode.
    let claude = AgentLaunchCredential::ClaudeEnvironment(Vec::new());
    assert_eq!(claude.codex_auth_mode(), None);
}

// AIC-FR-20 / AIC-FR-29: the payload's `Debug` never renders the token.
#[test]
fn the_payload_never_renders_the_access_token() {
    let rendered = format!("{:?}", token_config(Some(CODEX_TOKEN)));
    assert!(!rendered.contains(CODEX_TOKEN), "{rendered}");
    assert!(rendered.contains("access_token"), "{rendered}");
}

// AIC-FR-28 / AIC-FR-OFEO: a second token replaces the first in one vault
// write, and the hint follows the new token.
#[test]
fn a_new_access_token_replaces_the_stored_one_in_one_step() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    h.keys.forget_calls();
    let second = "codex-at-SECOND-FAKE-TOKEN-0000-abcd";
    let record =
        verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(second))).unwrap();
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(second));
    assert_eq!(h.keys.calls_for("codex"), ["set"], "one write, no delete in between");
    assert_eq!(record.masked_hint.as_deref(), Some("abcd"));
}

// AIC-FR-28 / AIC-FR-06: a vault that refuses the write fails the
// verification and leaves the registry as it was.
#[test]
fn a_vault_that_refuses_the_token_fails_the_verification() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    let before = h.store.load_agentic_registry().unwrap();
    h.keys.lock_it();
    assert_eq!(
        verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN)))
            .unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert_eq!(h.store.load_agentic_registry().unwrap(), before);
    assert_eq!(h.keys.get_raw("codex"), None);
}

// AIC-FR-06 / AIC-FR-28: a binary that is not Codex, or one that does not
// answer in time, fails an access-token verification with no vault write.
#[test]
fn a_wrong_or_silent_binary_writes_no_token() {
    let wrong = harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "claude 2.1.232"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify_integration_impl(&wrong.store, &wrong.ai, "codex", &token_config(Some(CODEX_TOKEN)))
            .unwrap_err(),
        ERR_NOT_THE_EXPECTED_CLI
    );
    assert!(wrong.keys.calls_for("codex").is_empty());

    let silent = harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::failing("timeout"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    assert_eq!(
        verify_integration_impl(
            &silent.store,
            &silent.ai,
            "codex",
            &token_config(Some(CODEX_TOKEN))
        )
        .unwrap_err(),
        ERR_TIMED_OUT
    );
    assert!(silent.keys.calls_for("codex").is_empty());
}

// AIC-FR-14 / AIC-FR-OFEO: a switch to `login_folder` keeps the stored token,
// and a clear after that switch still deletes it.
#[test]
fn clearing_after_a_switch_to_the_login_folder_deletes_the_kept_token() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    assert_eq!(h.keys.get_raw("codex").as_deref(), Some(CODEX_TOKEN), "the switch keeps it");
    clear_integration_impl(&h.store, &h.ai, "codex").unwrap();
    assert_eq!(h.keys.get_raw("codex"), None);
}

// AIC-FR-UFYH: the default mode is not written to the stored record, and a
// stored record with no mode reads as `login_folder`.
#[test]
fn the_default_codex_mode_is_absent_from_the_stored_record() {
    let mut record = AgenticRecord::empty("codex");
    record.codex_auth_mode = CodexAuthMode::LoginFolder;
    let text = toml::to_string(&record).expect("the record serializes");
    assert!(!text.contains("codexAuthMode"), "{text}");

    record.codex_auth_mode = CodexAuthMode::AccessToken;
    let text = toml::to_string(&record).expect("the record serializes");
    assert!(text.contains("codexAuthMode = \"access_token\""), "{text}");

    let read: AgenticRecord = toml::from_str("vendor = \"codex\"\n").expect("a bare record reads");
    assert_eq!(read.codex_auth_mode, CodexAuthMode::LoginFolder);
}

// AIC-FR-15: an active Codex whose token the vault no longer holds does not
// resolve for a turn.
#[test]
fn a_degraded_active_codex_does_not_resolve() {
    let h = codex_harness();
    verify_integration_impl(&h.store, &h.ai, "codex", &token_config(Some(CODEX_TOKEN))).unwrap();
    set_active_impl(&h.store, &h.ai, "codex").unwrap();
    assert!(resolve_agentic_invocation(&h.store, &h.ai, "", None).is_ok());
    h.keys.wipe("codex");
    assert!(resolve_agentic_invocation(&h.store, &h.ai, "", None).is_err());
}

// CDX-FR-01 / AIC-FR-05: 0.172.0 names where the grammar was observed and is
// not a pin, so a later Codex banner still identifies the vendor and verifies.
#[test]
fn a_later_codex_version_still_verifies() {
    let h = harness(
        FakeFs::with_executable(&["/usr/bin/codex"]),
        FakeRunner::saying("/usr/bin/codex", "codex-cli 0.180.0"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let record = verify_integration_impl(&h.store, &h.ai, "codex", &folder_config()).unwrap();
    assert_eq!(record.state, IntegrationState::Verified);
    assert_eq!(record.version.as_deref(), Some("0.180.0"));
}
