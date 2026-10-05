//! A record carries the fields of its own kind and no others (AIC-FR-25).

use super::*;

// -- AIC-FR-25: kind-shaped fields ------------------------------------

#[test]
fn aic_ts27_a_record_carries_the_fields_of_its_own_kind_and_no_others() {
    // Every vendor really configured — the scenario's Given clause — so the
    // "carries no credential field" assertions below cannot pass merely by
    // reading a default off an unconfigured record.
    let h = all_clis_harness();
    for (vendor, path) in CLI_BINARIES {
        verify_integration_impl(&h.store, &h.ai, vendor, &cli_config_for(vendor, path))
            .unwrap_or_else(|e| panic!("{vendor} should verify, got {e}"));
    }
    for vendor in ["claude_agent_api", "custom_agent_api"] {
        verify_integration_impl(
            &h.store,
            &h.ai,
            vendor,
            &api_config("https://api.anthropic.com/v1", Some("sk-ant-a71c")),
        )
        .unwrap();
    }

    let list = list_integrations_impl(&h.store, &h.ai).unwrap();
    for i in &list {
        // The kind decides the *configuration* fields.
        match i.kind {
            VendorKind::Cli => {
                assert_eq!(i.base_url, None, "{} carries no base URL", i.vendor);
            }
            VendorKind::Api => {
                assert_eq!(i.binary_path, None, "{} carries no path", i.vendor);
                assert_eq!(i.path_origin, PathOrigin::Unset);
            }
        }
        // AIC-FR-25: the credential fields follow the credential instead.
        // Codex and OpenCode hold nothing, so they report nothing, whatever
        // their kind — which is the distinction this assertion exists for.
        if !vendor_descriptor(&i.vendor).unwrap().holds_credential() {
            assert_eq!(i.masked_hint, None, "{} carries no hint", i.vendor);
            assert_eq!(i.key_state, KeyState::Unset, "{}", i.vendor);
            assert!(!i.key_required, "{}", i.vendor);
        }
        // The common fields are present on every record of both kinds.
        assert!(!i.display_name.is_empty());
        assert!(i.models.len() + i.reasoning_efforts.len() > 0 || i.vendor == "opencode");
    }

    let api = find(&list, "claude_agent_api");
    assert_eq!(api.masked_hint.as_deref(), Some("a71c"));
    assert_eq!(api.key_state, KeyState::Set);
    assert!(api.verified_at.is_some());

    // Claude Code carries a binary path *and* a credential, which is the
    // one record in the set that does both.
    let claude = find(&list, "claude_code");
    assert_eq!(claude.binary_path.as_deref(), Some("/usr/bin/claude"));
    assert_eq!(claude.base_url, None);
    assert!(claude.key_required);
    assert_eq!(claude.key_state, KeyState::Set);
    assert_eq!(claude.masked_hint.as_deref(), Some("ygAA"));
}
