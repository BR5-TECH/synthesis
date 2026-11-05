//! The provider that a capture and a speech use: the active one, or the one
//! that the test of a settings tab names (VOP-FR-TPXA).

use super::support::*;
use crate::ai_shared::SecretStore;
use crate::voice_ai::*;

fn verified(provider: &str, base_url: &str) -> VoiceAiRecord {
    let mut record = VoiceAiRecord::seeded(descriptor(provider).unwrap());
    record.base_url = base_url.into();
    record.verified_at = Some("2026-11-05T00:00:00Z".into());
    record
}

// VOP-FR-TPXA: with no provider, the call uses the active provider. A named
// provider takes its place, also when it is not the active one.
#[test]
fn a_named_provider_takes_the_place_of_the_active_one() {
    let bench = Bench::new();
    bench
        .store
        .save_voice_ai_registry(
            vec![
                verified("openai", "https://api.example/v1"),
                verified("custom", "http://localhost:1"),
            ],
            Some("openai".into()),
        )
        .unwrap();
    let active = resolve_voice(&bench.store, &bench.voice, None).unwrap();
    assert_eq!(active.record.provider, "openai");
    let named = resolve_voice(&bench.store, &bench.voice, Some("custom")).unwrap();
    assert_eq!(named.record.provider, "custom");
    assert_eq!(named.record.base_url, "http://localhost:1");
}

// VOP-FR-TPXA: a named provider needs a verified record, and works with no
// active provider at all.
#[test]
fn a_named_provider_must_be_verified() {
    let bench = Bench::new();
    let unverified = VoiceAiRecord::seeded(descriptor("openrouter").unwrap());
    bench
        .store
        .save_voice_ai_registry(vec![unverified, verified("custom", "http://localhost:1")], None)
        .unwrap();
    assert_eq!(
        resolve_voice(&bench.store, &bench.voice, Some("openrouter")).err(),
        Some(ERR_NOT_VERIFIED.to_string())
    );
    assert_eq!(
        resolve_voice(&bench.store, &bench.voice, Some("openai")).err(),
        Some(ERR_NOT_VERIFIED.to_string()),
        "a provider with no record"
    );
    assert_eq!(
        resolve_voice(&bench.store, &bench.voice, Some("acme")).err(),
        Some(ERR_UNKNOWN_PROVIDER.to_string())
    );
    assert_eq!(
        resolve_voice(&bench.store, &bench.voice, None).err(),
        Some(ERR_NO_ACTIVE_PROVIDER.to_string())
    );
    assert!(resolve_voice(&bench.store, &bench.voice, Some("custom")).is_ok());
}

// VOP-FR-TPXA, VOP-FR-EBQI: a named provider takes its own key, and not the
// key of the active provider.
#[test]
fn a_named_provider_takes_its_own_key() {
    let bench = Bench::new();
    bench
        .store
        .save_voice_ai_registry(
            vec![
                verified("openai", "https://api.example/v1"),
                verified("openrouter", "https://or.example/v1"),
            ],
            Some("openai".into()),
        )
        .unwrap();
    bench.secrets.set("openai", "key-of-openai").unwrap();
    bench.secrets.set("openrouter", "key-of-openrouter").unwrap();
    let named = resolve_voice(&bench.store, &bench.voice, Some("openrouter")).unwrap();
    assert_eq!(named.key.as_deref(), Some("key-of-openrouter"));
    let active = resolve_voice(&bench.store, &bench.voice, None).unwrap();
    assert_eq!(active.key.as_deref(), Some("key-of-openai"));
}
