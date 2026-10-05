//! The consolidated vault (AIC-FR-20, AIC-FR-32).

use super::*;

// -- The consolidated vault (AIC-FR-20 / AIC-FR-32) ---------------------
//
// The only tests here wired to the real vault rather than to
// `FakeKeychain`. What they assert is the *contract with the vault* — which
// credentials it is given to adopt, and which vendors are deliberately not
// named to it at all.

use crate::secret_vault::test_support::FakeKeyring;
use crate::secret_vault::{Vault, VaultSecrets};

/// This module's collaborators wired to a real vault over a fake keyring,
/// exactly as `lib.rs` wires them in production — migration candidate
/// source included.
fn vault_harness(
    keyring: &FakeKeyring,
    fs: Arc<FakeFs>,
    registry: Vec<AgenticRecord>,
) -> (GlobalSettingsStore, AgenticIntegrations) {
    vault_harness_with(
        keyring,
        fs,
        registry,
        Arc::new(FakeProber::default()),
    )
}

fn vault_harness_with(
    keyring: &FakeKeyring,
    fs: Arc<FakeFs>,
    registry: Vec<AgenticRecord>,
    prober: Arc<FakeProber>,
) -> (GlobalSettingsStore, AgenticIntegrations) {
    let store = GlobalSettingsStore::in_memory();
    store.save_agentic_registry(registry.clone(), None).unwrap();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.set_candidate_source(Box::new(move || migration_candidates(&registry)));
    let ai = AgenticIntegrations::new(
        Box::new(fs),
        Box::new(Arc::new(FakeRunner::default())),
        Box::new(prober),
        Box::new(VaultSecrets::new(vault, VAULT_NAMESPACE)),
    );
    (store, ai)
}

/// The five typed vault failures, each with the switch that provokes it.
fn vault_provocations() -> Vec<(&'static str, Box<dyn Fn(&FakeKeyring)>)> {
    vec![
        (
            "vault_unavailable",
            Box::new(|k: &FakeKeyring| k.state().refuse_read = true),
        ),
        (
            "vault_write_failed",
            Box::new(|k: &FakeKeyring| k.state().refuse_write = true),
        ),
        (
            "vault_verify_failed",
            Box::new(|k: &FakeKeyring| k.state().mangle_next_writes = 1),
        ),
        (
            "vault_malformed",
            Box::new(|k: &FakeKeyring| k.set_entry("not an AppSecrets object")),
        ),
        (
            "vault_unsupported_version",
            Box::new(|k: &FakeKeyring| k.set_entry(r#"{"version":99}"#)),
        ),
    ]
}

/// A verified API-kind record with a stored credential.
fn verified_api_record(vendor: &str, hint: &str) -> AgenticRecord {
    AgenticRecord {
        vendor: vendor.to_string(),
        base_url: Some("https://api.anthropic.com/v1".into()),
        masked_hint: Some(hint.to_string()),
        verified_at: Some("2026-01-01T00:00:00Z".into()),
        ..AgenticRecord::empty(vendor)
    }
}

/// AIC-FR-32, AIC-FR-20, AIC-FR-15: an API-kind vendor and Claude Code whose credentials are
/// still in the legacy per-vendor entries are adopted on the first listing;
/// no candidate is supplied for Codex or OpenCode; and both records read
/// `verified` rather than `key_unavailable`.
#[test]
fn aic_ts37_legacy_credentials_are_adopted_and_codex_is_left_alone() {
    use crate::secret_vault::test_support::Call;

    let keyring = FakeKeyring::default();
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "claude_agent_api", "k-api-key");
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "claude_code", "sk-ant-oat01-token");
    // A legacy entry Codex never had, planted to prove nothing goes looking
    // for one: no candidate names it, so it is neither read nor deleted.
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "codex", "not ours to touch");
    let fs = FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex"]);
    let (store, ai) = vault_harness(
        &keyring,
        fs,
        vec![
            AgenticRecord {
                vendor: "claude_agent_api".into(),
                base_url: Some("https://api.anthropic.com/v1".into()),
                masked_hint: Some("-key".into()),
                verified_at: Some("2026-01-01T00:00:00Z".into()),
                ..AgenticRecord::empty("claude_agent_api")
            },
            AgenticRecord {
                vendor: "claude_code".into(),
                binary_path: Some("/usr/bin/claude".into()),
                masked_hint: Some("oken".into()),
                verified_at: Some("2026-01-01T00:00:00Z".into()),
                ..AgenticRecord::empty("claude_code")
            },
            AgenticRecord {
                vendor: "codex".into(),
                binary_path: Some("/usr/bin/codex".into()),
                verified_at: Some("2026-01-01T00:00:00Z".into()),
                ..AgenticRecord::empty("codex")
            },
        ],
    );

    let list = list_integrations_impl(&store, &ai).unwrap();

    let api = list.iter().find(|i| i.vendor == "claude_agent_api").unwrap();
    let cli = list.iter().find(|i| i.vendor == "claude_code").unwrap();
    assert_eq!(api.state, IntegrationState::Verified);
    assert_eq!(api.key_state, KeyState::Set);
    assert_eq!(cli.state, IntegrationState::Verified);
    assert_eq!(cli.key_state, KeyState::Set);

    assert!(keyring
        .legacy(LEGACY_KEYCHAIN_SERVICE, "claude_agent_api")
        .is_none());
    assert!(keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "claude_code").is_none());
    // AIC-FR-32 / ASV-FR-32: Codex holds no credential here, so migration
    // never named it — its legacy entry was not read and not deleted.
    assert_eq!(
        keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "codex").as_deref(),
        Some("not ours to touch")
    );
    assert!(!keyring.calls().iter().any(|c| matches!(
        c,
        Call::ReadLegacy(_, account) | Call::DeleteLegacy(_, account) if account == "codex"
    )));

    let object = keyring.object();
    let vendors = object
        .get("agentic")
        .and_then(|v| v.get("vendors"))
        .and_then(|v| v.as_object())
        .expect("the agentic.vendors namespace exists");
    assert_eq!(
        vendors.get("claude_agent_api").and_then(|v| v.as_str()),
        Some("k-api-key")
    );
    assert_eq!(
        vendors.get("claude_code").and_then(|v| v.as_str()),
        Some("sk-ant-oat01-token")
    );
    assert!(!vendors.contains_key("codex"));
}

/// AIC-FR-32: a candidate for every vendor that holds a credential, and for
/// no other — whatever the registry carries.
#[test]
fn aic_fr32_no_candidate_is_supplied_for_a_keyless_vendor() {
    let registry: Vec<AgenticRecord> = VENDORS
        .iter()
        .map(|d| AgenticRecord::empty(d.vendor))
        .collect();

    let candidates = migration_candidates(&registry);

    let accounts: Vec<&str> = candidates
        .iter()
        .map(|c| c.legacy_account.as_str())
        .collect();
    assert_eq!(
        accounts,
        vec!["claude_code", "claude_agent_api", "custom_agent_api"]
    );
    for candidate in &candidates {
        assert_eq!(
            candidate.legacy_service,
            "com.synthesis.agentic-integration"
        );
        assert_eq!(candidate.path[0], "agentic");
        assert_eq!(candidate.path[1], "vendors");
    }
}

/// AIC-FR-24 / ASV-FR-31: every typed vault failure that can refuse a read
/// leaves the record `key_unavailable` and never fails the listing.
#[test]
fn aic_ts26_every_vault_failure_downgrades_but_never_fails_the_listing() {
    let provocations: Vec<(&str, Box<dyn Fn(&FakeKeyring)>)> = vec![
        (
            "vault_unavailable",
            Box::new(|k: &FakeKeyring| k.state().refuse_read = true),
        ),
        (
            "vault_malformed",
            Box::new(|k: &FakeKeyring| k.set_entry("not an AppSecrets object")),
        ),
        (
            "vault_unsupported_version",
            Box::new(|k: &FakeKeyring| k.set_entry(r#"{"version":99}"#)),
        ),
    ];

    for (name, provoke) in provocations {
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness(
            &keyring,
            FakeFs::with_executable(&[]),
            vec![
                verified_api_record("claude_agent_api", "1234"),
                verified_api_record("custom_agent_api", "5678"),
            ],
        );

        let list = list_integrations_impl(&store, &ai).unwrap();

        assert_eq!(list.len(), 5, "provoked {name}");
        // A vault that will not answer downgrades **every** record holding
        // a credential, not the one that happened to be asked first.
        for vendor in ["claude_agent_api", "custom_agent_api"] {
            let record = list.iter().find(|i| i.vendor == vendor).unwrap();
            assert_eq!(
                record.key_state,
                KeyState::Unavailable,
                "provoked {name} for {vendor}"
            );
            assert_eq!(
                record.state,
                IntegrationState::KeyUnavailable,
                "provoked {name} for {vendor}"
            );
        }
        assert_eq!(store.load_agentic_registry().unwrap().0.len(), 2);
    }
}

/// AIC-FR-24 / AIC-FR-14 / ASV-FR-31: every typed vault failure that can
/// refuse a **write** crosses the boundary as one `keychain_unavailable`,
/// from both operations that touch the vault, and leaves the registry as it
/// was.
///
/// `vault_malformed` is excluded on the write path for the reason
/// ASV-FR-17 gives: a mutation recovers from an undecodable value rather
/// than refusing over it.
#[test]
fn aic_ts26_every_vault_write_failure_is_one_keychain_unavailable() {
    for (name, provoke) in vault_provocations() {
        if name == "vault_malformed" {
            continue;
        }

        // The verification path, which writes the credential.
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness_with(
            &keyring,
            FakeFs::with_executable(&[]),
            vec![],
            FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        );
        let err = verify_integration_impl(
            &store,
            &ai,
            "claude_agent_api",
            &api_config("https://api.anthropic.com/v1", Some("k-1234")),
        )
        .unwrap_err();
        assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE, "verify, provoked {name}");
        assert!(
            store.load_agentic_registry().unwrap().0.is_empty(),
            "verify, provoked {name}: the registry is untouched"
        );

        // The clear path, which deletes it (AIC-FR-24, AIC-FR-14).
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness(
            &keyring,
            FakeFs::with_executable(&[]),
            vec![verified_api_record("claude_agent_api", "1234")],
        );
        let err = clear_integration_impl(&store, &ai, "claude_agent_api").unwrap_err();
        assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE, "clear, provoked {name}");
        assert_eq!(
            store.load_agentic_registry().unwrap().0.len(),
            1,
            "clear, provoked {name}: the registry is untouched"
        );
    }
}

/// ASV-FR-31: **no other error value** crosses the boundary — including out
/// of `resolve_agentic_invocation`, the read path an actual agent launch
/// takes.
#[test]
fn no_vault_error_code_reaches_the_ipc_boundary() {
    for (name, provoke) in vault_provocations() {
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness_with(
            &keyring,
            FakeFs::with_executable(&["/usr/bin/claude"]),
            vec![verified_api_record("claude_agent_api", "1234")],
            FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        );

        let errors = [
            verify_integration_impl(
                &store,
                &ai,
                "custom_agent_api",
                &api_config("http://localhost:8080/v1", Some("sk-1234")),
            )
            .err(),
            clear_integration_impl(&store, &ai, "claude_agent_api").err(),
            resolve_agentic_invocation(&store, &ai, "/dev/acme", None).err(),
        ];
        for error in errors.into_iter().flatten() {
            // `contains` rather than `starts_with`: a code wrapped in prose
            // (`"keychain: vault_malformed"`) has crossed the boundary just
            // as surely as a bare one.
            for code in [
                "vault_unavailable",
                "vault_write_failed",
                "vault_verify_failed",
                "vault_malformed",
                "vault_unsupported_version",
            ] {
                assert!(
                    !error.contains(code),
                    "provoked {name}: a vault error code crossed the boundary: {error}"
                );
            }
        }
    }
}

/// AIC-FR-20 read with AIC-FR-32: the presence question this module asks the
/// vault covers exactly the vendors whose state can depend on a credential.
///
/// `vendor_presence` asks about `holds_credential()` vendors; `key_presence`
/// decides whether to consult the answer with a differently-shaped
/// `worth_probing`. They agree today, and a descriptor edit that broke the
/// agreement would produce a permanent silent `false` — a verified
/// integration reading `key_unavailable` forever — rather than an error.
#[test]
fn every_vendor_worth_probing_is_one_the_presence_query_asks_about() {
    // The two production functions, driven against each other rather than
    // re-derived: a store that says *every* id is present, and a presence
    // query that asks only about the vendors it believes hold a credential.
    // Any vendor `key_presence` consults but `vendor_presence` never asked
    // about reads `false` here — a verified integration that would report
    // `key_unavailable` forever, silently.
    struct EverythingPresent;
    impl SecretStore for EverythingPresent {
        fn set(&self, _id: &str, _secret: &str) -> Result<(), SecretUnavailable> {
            Ok(())
        }
        fn get(&self, _id: &str) -> Result<Option<String>, SecretUnavailable> {
            Ok(Some("secret".into()))
        }
        fn delete(&self, _id: &str) -> Result<(), SecretUnavailable> {
            Ok(())
        }
    }

    let present = vendor_presence(&EverythingPresent);
    for descriptor in VENDORS {
        let record = AgenticRecord {
            masked_hint: Some("1234".into()),
            ..AgenticRecord::empty(descriptor.vendor)
        };
        if !descriptor.holds_credential() {
            // AIC-FR-25: a vendor that holds no credential never reports one
            // as set or as missing, whatever the store would say.
            assert!(
                !key_presence(&present, &record, descriptor),
                "{} holds no credential but was probed for one",
                descriptor.vendor
            );
            continue;
        }
        assert!(
            key_presence(&present, &record, descriptor),
            "{} holds a credential the store has, but the presence query \
             never asked about it — it would read `key_unavailable` forever",
            descriptor.vendor
        );
    }
}

/// AIC-FR-02 read together with `ASV-application-secret-vault.md`
/// ASV-FR-30: a listing costs **one** vault access however many vendors it
/// describes.
#[test]
fn aic_fr02_a_listing_makes_one_vault_access() {
    use crate::secret_vault::test_support::Call;

    let keyring = FakeKeyring::default();
    let (store, ai) = vault_harness(
        &keyring,
        FakeFs::with_executable(&[]),
        VENDORS
            .iter()
            .map(|d| AgenticRecord::empty(d.vendor))
            .collect(),
    );
    // Let migration run before the access is counted; it is the one
    // sequence allowed to read more than once.
    let _ = list_integrations_impl(&store, &ai).unwrap();
    let reads_before = keyring.count(&Call::Read);

    let list = list_integrations_impl(&store, &ai).unwrap();

    assert_eq!(list.len(), 5);
    assert_eq!(keyring.count(&Call::Read) - reads_before, 1);
}
