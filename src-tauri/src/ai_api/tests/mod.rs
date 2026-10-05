//! The test scenarios of `specifications/core/AAP-ai-api-integrations.md`.
//!
//! The shared doubles, the harness, and the small helpers live here; each topic
//! file holds the scenarios of one area.

use super::*;
use crate::ai_shared::{ModelReasoning, SecretUnavailable};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

// -- Test doubles ----------------------------------------------------
//
// Both outside dependencies are faked so every test runs without a keychain
// or a network. CI has neither.

#[derive(Default)]
struct FakeProber {
    models: StdMutex<Vec<ModelOption>>,
    error: StdMutex<Option<ProbeError>>,
    calls: StdMutex<Vec<(String, Option<String>, AuthStyle)>>,
}

impl FakeProber {
    fn returning(models: &[(&str, &str)]) -> Arc<Self> {
        let p = Arc::new(Self::default());
        *p.models.lock().unwrap() = models
            .iter()
            .map(|(id, label)| ModelOption::new(id, label))
            .collect();
        p
    }
    fn failing(e: ProbeError) -> Arc<Self> {
        let p = Arc::new(Self::default());
        *p.error.lock().unwrap() = Some(e);
        p
    }
    fn set_models(&self, models: &[(&str, &str)]) {
        *self.models.lock().unwrap() = models
            .iter()
            .map(|(id, label)| ModelOption::new(id, label))
            .collect();
    }
    /// Models carrying reasoning descriptors, which is what an OpenRouter
    /// probe returns (AAP-FR-24).
    fn set_rich_models(&self, models: Vec<ModelOption>) {
        *self.models.lock().unwrap() = models;
    }
}

/// A model declaring its own ladder.
fn with_ladder(id: &str, efforts: &[&str], default: Option<&str>, mandatory: bool) -> ModelOption {
    ModelOption::new(id, id).with_reasoning(Some(ModelReasoning {
        mandatory,
        default_enabled: Some(true),
        supported_efforts: Some(efforts.iter().map(|e| e.to_string()).collect()),
        default_effort: default.map(str::to_string),
    }))
}

/// A model that reasons but declares no levels — the off/on shape.
fn ladderless(id: &str, mandatory: bool) -> ModelOption {
    ModelOption::new(id, id).with_reasoning(Some(ModelReasoning {
        mandatory,
        default_enabled: Some(true),
        supported_efforts: None,
        default_effort: None,
    }))
}

/// Verify OpenRouter through the fake prober, which stands in for the SDK.
fn verify_openrouter(h: &Harness) -> AiApiIntegration {
    verify_integration_impl(
        &h.store,
        &h.ai,
        "openrouter",
        "https://openrouter.ai/api/v1",
        Some("sk-or-1234"),
    )
    .unwrap()
}

impl EndpointProber for Arc<FakeProber> {
    fn probe(&self, request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError> {
        self.calls.lock().unwrap().push((
            format!("{}{}", request.base_url, request.models_path),
            request.api_key.map(str::to_string),
            request.auth,
        ));
        if let Some(e) = self.error.lock().unwrap().clone() {
            return Err(e);
        }
        Ok(self.models.lock().unwrap().clone())
    }
}

#[derive(Default)]
struct FakeKeychain {
    entries: StdMutex<HashMap<String, String>>,
    locked: StdMutex<bool>,
    /// Every read, whether it found anything or not. On macOS a read is what
    /// raises the system prompt, so "did not touch the keychain" is a claim
    /// about accesses rather than about the answer — a probe that failed and
    /// was treated as absent still interrupted the author.
    reads: StdMutex<usize>,
}

impl FakeKeychain {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn lock_it(&self) {
        *self.locked.lock().unwrap() = true;
    }
    fn reads(&self) -> usize {
        *self.reads.lock().unwrap()
    }
    fn wipe(&self, id: &str) {
        self.entries.lock().unwrap().remove(id);
    }
    fn get_raw(&self, id: &str) -> Option<String> {
        self.entries.lock().unwrap().get(id).cloned()
    }
}

impl SecretStore for Arc<FakeKeychain> {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        if *self.locked.lock().unwrap() {
            return Err(SecretUnavailable("locked".into()));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(id.to_string(), secret.to_string());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
        *self.reads.lock().unwrap() += 1;
        if *self.locked.lock().unwrap() {
            return Err(SecretUnavailable("locked".into()));
        }
        Ok(self.entries.lock().unwrap().get(id).cloned())
    }
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        if *self.locked.lock().unwrap() {
            return Err(SecretUnavailable("locked".into()));
        }
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
}

struct Harness {
    store: GlobalSettingsStore,
    ai: AiApiIntegrations,
    keys: Arc<FakeKeychain>,
    prober: Arc<FakeProber>,
}

fn harness(prober: Arc<FakeProber>, keys: Arc<FakeKeychain>) -> Harness {
    Harness {
        store: GlobalSettingsStore::in_memory(),
        ai: AiApiIntegrations::new(Box::new(prober.clone()), Box::new(keys.clone())),
        keys,
        prober,
    }
}

fn ok_harness() -> Harness {
    harness(
        FakeProber::returning(&[("gpt-5", "GPT-5"), ("gpt-4o", "GPT-4o")]),
        FakeKeychain::new(),
    )
}

fn find<'a>(list: &'a [AiApiIntegration], provider: &str) -> &'a AiApiIntegration {
    list.iter().find(|i| i.provider == provider).unwrap()
}


mod capabilities;
mod catalogue;
mod configuration;
mod gateway_contract;
mod read_paths;
mod reasoning;
mod session_logging;
mod stored_keys;
mod turn_timeout;
mod vault;
