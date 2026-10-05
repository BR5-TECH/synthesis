//! The fresh-machine listing and detection (AIC-FR-01, AIC-FR-02, AIC-FR-03).

use super::*;

// -- AIC-FR-01, AIC-FR-02 ---------------------------------------------------------

#[test]
fn aic_ts01_a_fresh_machine_lists_five_records_of_both_kinds() {
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(list.len(), 5);
    assert_eq!(
        list.iter().map(|i| i.vendor.as_str()).collect::<Vec<_>>(),
        vec![
            "claude_code",
            "codex",
            "opencode",
            "claude_agent_api",
            "custom_agent_api"
        ],
        "a stable order, CLIs first"
    );
    for i in &list {
        assert_eq!(i.state, IntegrationState::Unconfigured);
        assert!(!i.active);
    }
    assert_eq!(find(&list, "claude_code").kind, VendorKind::Cli);
    assert_eq!(find(&list, "claude_agent_api").kind, VendorKind::Api);

    // AIC-FR-25: an unconfigured Claude Code declares that it *needs* a
    // credential without claiming anything is wrong with the one it does
    // not have. `unavailable` here would render the empty tab as a
    // degradation (AII-FR-29), which is the branch this pins.
    let claude = find(&list, "claude_code");
    assert!(claude.key_required);
    assert_eq!(claude.key_state, KeyState::Unset);
    assert_eq!(claude.masked_hint, None);

    // AIC-FR-02: listing runs no binary. Now load-bearing, because listing
    // Claude Code probes the keychain where it previously did not.
    assert!(h.runner.calls.lock().unwrap().is_empty());
}

// -- AIC-FR-03 / TS-03: detection -------------------------------------

#[test]
fn aic_ts02_detection_finds_a_binary_and_persists_nothing() {
    let fs = FakeFs::with_executable(&["/usr/bin/claude"]);
    let h = harness(
        fs,
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    let found = detect_binary_impl(&h.ai, "claude_code").unwrap();
    assert_eq!(found.path.as_deref(), Some("/usr/bin/claude"));
    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    assert_eq!(
        find(&list, "claude_code").state,
        IntegrationState::Unconfigured,
        "detection persists nothing"
    );
}

#[test]
fn aic_ts03_detection_is_empty_for_a_bare_machine_and_refused_for_an_api_vendor() {
    let h = harness(
        FakeFs::with_executable(&[]),
        Arc::new(FakeRunner::default()),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    );
    for vendor in ["claude_code", "codex", "opencode"] {
        assert_eq!(detect_binary_impl(&h.ai, vendor).unwrap().path, None);
    }
    // An API agent has no binary to find; asking is a caller error rather
    // than an empty answer.
    assert_eq!(
        detect_binary_impl(&h.ai, "claude_agent_api").unwrap_err(),
        ERR_NOT_A_CLI_INTEGRATION
    );
}
