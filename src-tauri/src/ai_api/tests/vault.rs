//! AAP-FR-07, AAP-FR-36 / ASV-FR-17, ASV-FR-30, ASV-FR-31: the contract with the consolidated vault.

use super::*;

// -- The consolidated vault (AAP-FR-07 / AAP-FR-36) ---------------------
//
// The only tests here wired to the real vault rather than to
// `FakeKeychain`. What they assert is the *contract with the vault*, which
// a double of this module's own would assert against itself.

use crate::secret_vault::test_support::FakeKeyring;
use crate::secret_vault::{Vault, VaultSecrets};

/// This module's collaborators wired to a real vault over a fake keyring,
/// exactly as `lib.rs` wires them in production — migration candidate
/// source included.
fn vault_harness(
    keyring: &FakeKeyring,
    registry: Vec<AiApiRecord>,
    active: Option<&str>,
) -> (GlobalSettingsStore, AiApiIntegrations) {
    vault_harness_with(keyring, registry, active, Arc::new(FakeProber::default()))
}

fn vault_harness_with(
    keyring: &FakeKeyring,
    registry: Vec<AiApiRecord>,
    active: Option<&str>,
    prober: Arc<FakeProber>,
) -> (GlobalSettingsStore, AiApiIntegrations) {
    let store = GlobalSettingsStore::in_memory();
    store
        .save_ai_api_registry(registry.clone(), active.map(str::to_string))
        .unwrap();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.set_candidate_source(Box::new(move || migration_candidates(&registry)));
    let ai = AiApiIntegrations::new(
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

/// A verified record with a stored key, as the registry holds one.
fn verified_record(provider: &str, hint: &str) -> AiApiRecord {
    AiApiRecord {
        provider: provider.to_string(),
        base_url: Some("https://example.test/v1".into()),
        masked_hint: Some(hint.to_string()),
        verified_at: Some("2026-01-01T00:00:00Z".into()),
        ..AiApiRecord::empty(provider)
    }
}

/// AAP-FR-36, AAP-FR-07, AAP-FR-09: two configured providers whose keys are still in the legacy
/// per-provider entries are adopted on the first listing, and each record
/// reads `verified` rather than `key_unavailable`.
#[test]
fn aap_ts37_legacy_keys_are_adopted_on_the_first_listing() {
    let keyring = FakeKeyring::default();
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "openai", "sk-openai-key");
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "anthropic", "sk-ant-key");
    let (store, ai) = vault_harness(
        &keyring,
        vec![
            verified_record("openai", "-key"),
            verified_record("anthropic", "-key"),
        ],
        None,
    );

    let list = list_integrations_impl(&store, &ai).unwrap();

    assert_eq!(find(&list, "openai").state, IntegrationState::Verified);
    assert_eq!(find(&list, "openai").key_state, KeyState::Set);
    assert_eq!(find(&list, "anthropic").state, IntegrationState::Verified);
    assert!(keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "openai").is_none());
    assert!(keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "anthropic").is_none());

    let providers = keyring.object();
    let providers = providers
        .get("ai_api")
        .and_then(|v| v.get("providers"))
        .and_then(|v| v.as_object())
        .expect("the ai_api.providers namespace exists");
    assert_eq!(
        providers.get("openai").and_then(|v| v.as_str()),
        Some("sk-openai-key")
    );
    assert_eq!(
        providers.get("anthropic").and_then(|v| v.as_str()),
        Some("sk-ant-key")
    );
}

/// AAP-FR-36: a candidate is supplied for every configured provider,
/// whether or not the vault already holds its key.
#[test]
fn aap_fr36_a_candidate_is_supplied_for_every_configured_provider() {
    let candidates = migration_candidates(&[
        verified_record("openai", "-key"),
        verified_record("anthropic", "-key"),
    ]);
    assert_eq!(candidates.len(), 2);
    for (candidate, provider) in candidates.iter().zip(["openai", "anthropic"]) {
        assert_eq!(candidate.legacy_service, "com.synthesis.ai-api-provider");
        assert_eq!(candidate.legacy_account, provider);
        assert_eq!(
            candidate.path,
            vec![
                "ai_api".to_string(),
                "providers".to_string(),
                provider.to_string()
            ]
        );
    }
}

/// AAP-FR-20 / ASV-FR-31: every typed vault failure that can refuse a read
/// leaves the record `key_unavailable` and never fails the listing.
#[test]
fn aap_ts19_every_vault_failure_downgrades_but_never_fails_the_listing() {
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
            vec![
                verified_record("openai", "1234"),
                verified_record("anthropic", "5678"),
            ],
            None,
        );

        let list = list_integrations_impl(&store, &ai).unwrap();

        assert_eq!(list.len(), 4, "provoked {name}");
        // A vault that will not answer downgrades **every** configured
        // record, not the one that happened to be asked first.
        for provider in ["openai", "anthropic"] {
            assert_eq!(
                find(&list, provider).key_state,
                KeyState::Unavailable,
                "provoked {name} for {provider}"
            );
            assert_eq!(
                find(&list, provider).state,
                IntegrationState::KeyUnavailable,
                "provoked {name} for {provider}"
            );
        }
        // The registry is untouched by any of them.
        assert_eq!(store.load_ai_api_registry().unwrap().0.len(), 2);
    }
}

/// AAP-FR-20 / ASV-FR-31: every typed vault failure that can refuse a
/// **write** crosses the boundary as one `keychain_unavailable`, from both
/// operations that touch the vault, and leaves the registry as it was.
///
/// `vault_malformed` is excluded on the write path for the reason
/// ASV-FR-17 gives: a mutation recovers from an undecodable value rather
/// than refusing over it. That case is asserted separately below.
#[test]
fn aap_ts19_every_vault_write_failure_is_one_keychain_unavailable() {
    for (name, provoke) in vault_provocations() {
        if name == "vault_malformed" {
            continue;
        }

        // The verification path, which writes the key.
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness_with(
            &keyring,
            vec![],
            None,
            FakeProber::returning(&[("gpt-5", "GPT-5")]),
        );
        let err = verify_integration_impl(
            &store,
            &ai,
            "openai",
            "https://api.openai.com/v1",
            Some("sk-1234"),
        )
        .unwrap_err();
        assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE, "verify, provoked {name}");
        assert!(
            store.load_ai_api_registry().unwrap().0.is_empty(),
            "verify, provoked {name}: the registry is untouched"
        );

        // The clear path, which deletes it.
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) =
            vault_harness(&keyring, vec![verified_record("openai", "1234")], None);
        let err = clear_integration_impl(&store, &ai, "openai").unwrap_err();
        assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE, "clear, provoked {name}");
        assert_eq!(
            store.load_ai_api_registry().unwrap().0.len(),
            1,
            "clear, provoked {name}: the registry is untouched"
        );
    }
}

/// ASV-FR-17 read with AAP-FR-06: an undecodable vault value does not cost
/// the author the verification they just paid a round trip for.
#[test]
fn a_malformed_vault_value_does_not_refuse_a_verification() {
    let keyring = FakeKeyring::default();
    keyring.set_entry("not an AppSecrets object");
    let (store, ai) = vault_harness_with(
        &keyring,
        vec![],
        None,
        FakeProber::returning(&[("gpt-5", "GPT-5")]),
    );

    let record = verify_integration_impl(
        &store,
        &ai,
        "openai",
        "https://api.openai.com/v1",
        Some("sk-supersecret1234"),
    )
    .unwrap();

    assert_eq!(record.masked_hint.as_deref(), Some("1234"));
    assert_eq!(
        keyring.object().get("quarantine").and_then(|v| v.as_str()),
        Some("not an AppSecrets object")
    );
}

/// ASV-FR-31: **no other error value** crosses the boundary. The vault's own
/// codes travel inside `SecretUnavailable` so a developer can tell the five
/// apart in a debugger, and every call site is responsible for collapsing
/// them — a `map_err` that passed one through would be invisible otherwise.
#[test]
fn no_vault_error_code_reaches_the_ipc_boundary() {
    for (name, provoke) in vault_provocations() {
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, ai) = vault_harness_with(
            &keyring,
            vec![verified_record("openai", "1234")],
            None,
            FakeProber::returning(&[("gpt-5", "GPT-5")]),
        );

        let errors = [
            verify_integration_impl(
                &store,
                &ai,
                "anthropic",
                "https://api.anthropic.com/v1",
                Some("sk-1234"),
            )
            .err(),
            clear_integration_impl(&store, &ai, "openai").err(),
            resolve_ai_api_call(&store, &ai, "/dev/acme").err(),
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

/// AAP-FR-02 read together with `ASV-application-secret-vault.md` ASV-FR-30:
/// a listing costs **one** vault access however many providers it
/// describes.
#[test]
fn aap_fr02_a_listing_makes_one_vault_access() {
    use crate::secret_vault::test_support::Call;

    let keyring = FakeKeyring::default();
    let (store, ai) = vault_harness(
        &keyring,
        vec![
            verified_record("openai", "1234"),
            verified_record("anthropic", "5678"),
            verified_record("openrouter", "9abc"),
        ],
        None,
    );
    // Let migration run before the access is counted; it is the one
    // sequence allowed to read more than once.
    let _ = list_integrations_impl(&store, &ai).unwrap();
    let reads_before = keyring.count(&Call::Read);

    let list = list_integrations_impl(&store, &ai).unwrap();

    assert_eq!(list.len(), 4);
    assert_eq!(keyring.count(&Call::Read) - reads_before, 1);
}

