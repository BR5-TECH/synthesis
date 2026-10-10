//! The tests of `../../../specifications/core/AIC-agentic-integrations.md`.
//!
//! This file holds the test doubles the whole suite runs against — the fake
//! filesystem, CLI runner, endpoint prober, and keychain — with the harness
//! that binds them and the small configuration builders. Every topic file
//! beside it starts with `use super::*;` and holds one subject.
//!
//! No test below reaches a CLI on this machine, a keychain, or a network.

use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};

// -- Test doubles ----------------------------------------------------
//
// All three outside dependencies are faked so every test below runs without
// a CLI installed, a keychain, or a network. That is not only a speed
// choice: CI runs the Rust suite on a headless runner where none of them
// exists, so a test that reached for a real one would fail there while
// passing locally.

#[derive(Default)]
struct FakeFs {
    files: StdMutex<HashSet<String>>,
    executable: StdMutex<HashSet<String>>,
}

impl FakeFs {
    fn with_executable(paths: &[&str]) -> Arc<Self> {
        let fs = Arc::new(Self::default());
        for p in paths {
            fs.add_executable(p);
        }
        fs
    }
    fn add_executable(&self, path: &str) {
        self.files.lock().unwrap().insert(path.to_string());
        self.executable.lock().unwrap().insert(path.to_string());
    }
    fn add_file(&self, path: &str) {
        self.files.lock().unwrap().insert(path.to_string());
    }
    fn remove(&self, path: &str) {
        self.files.lock().unwrap().remove(path);
        self.executable.lock().unwrap().remove(path);
    }
}

impl FileProbe for Arc<FakeFs> {
    fn exists(&self, path: &Path) -> bool {
        self.files
            .lock()
            .unwrap()
            .contains(&path.to_string_lossy().to_string())
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.executable
            .lock()
            .unwrap()
            .contains(&path.to_string_lossy().to_string())
    }
}

#[derive(Default)]
struct FakeRunner {
    /// path -> (args joined) -> outcome
    responses: StdMutex<HashMap<String, CliOutput>>,
    error: StdMutex<Option<&'static str>>,
    calls: StdMutex<Vec<(String, Vec<String>)>>,
}

impl FakeRunner {
    fn saying(path: &str, banner: &str) -> Arc<Self> {
        let r = Arc::new(Self::default());
        r.responses.lock().unwrap().insert(
            path.to_string(),
            CliOutput {
                stdout: banner.to_string(),
                stderr: String::new(),
                success: true,
            },
        );
        r
    }
    /// Several binaries at once, for the tests that need every vendor
    /// actually configured rather than merely present in the list.
    fn saying_each(banners: &[(&str, &str)]) -> Arc<Self> {
        let r = Arc::new(Self::default());
        for (path, banner) in banners {
            r.responses.lock().unwrap().insert(
                path.to_string(),
                CliOutput {
                    stdout: banner.to_string(),
                    stderr: String::new(),
                    success: true,
                },
            );
        }
        r
    }
    fn failing(kind: &'static str) -> Arc<Self> {
        let r = Arc::new(Self::default());
        *r.error.lock().unwrap() = Some(kind);
        r
    }
}

impl CliRunner for Arc<FakeRunner> {
    fn run(
        &self,
        path: &Path,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<CliOutput, RunError> {
        self.calls.lock().unwrap().push((
            path.to_string_lossy().to_string(),
            args.iter().map(|s| s.to_string()).collect(),
        ));
        if let Some(kind) = *self.error.lock().unwrap() {
            return Err(match kind {
                "timeout" => RunError::TimedOut,
                "notfound" => RunError::NotFound,
                "notexec" => RunError::NotExecutable,
                _ => RunError::Failed("boom".into()),
            });
        }
        self.responses
            .lock()
            .unwrap()
            .get(&path.to_string_lossy().to_string())
            .cloned()
            .ok_or(RunError::NotFound)
    }
}

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
    /// Every mutating call, in order.
    ///
    /// Final state is not enough to judge AIC-FR-28: "the vendor holds
    /// exactly one token at every observable moment and never zero between
    /// the two" is a claim about the *sequence*, and a delete-then-set
    /// implementation reaches an identical end state through an observable
    /// moment holding neither. Only a call log can tell them apart.
    calls: StdMutex<Vec<(&'static str, String)>>,
}

impl FakeKeychain {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn lock_it(&self) {
        *self.locked.lock().unwrap() = true;
    }
    fn wipe(&self, id: &str) {
        self.entries.lock().unwrap().remove(id);
    }
    fn get_raw(&self, id: &str) -> Option<String> {
        self.entries.lock().unwrap().get(id).cloned()
    }
    /// The mutating calls made against `id`, oldest first.
    fn calls_for(&self, id: &str) -> Vec<&'static str> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, target)| target == id)
            .map(|(op, _)| *op)
            .collect()
    }
    fn forget_calls(&self) {
        self.calls.lock().unwrap().clear();
    }
}

impl SecretStore for Arc<FakeKeychain> {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        self.calls.lock().unwrap().push(("set", id.to_string()));
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
        if *self.locked.lock().unwrap() {
            return Err(SecretUnavailable("locked".into()));
        }
        Ok(self.entries.lock().unwrap().get(id).cloned())
    }
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        self.calls.lock().unwrap().push(("delete", id.to_string()));
        if *self.locked.lock().unwrap() {
            return Err(SecretUnavailable("locked".into()));
        }
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
}

use crate::ai_shared::SecretUnavailable;

/// A module wired to the given doubles, plus an in-memory store.
struct Harness {
    store: GlobalSettingsStore,
    ai: AgenticIntegrations,
    fs: Arc<FakeFs>,
    keys: Arc<FakeKeychain>,
    /// Kept so a test can assert that a rejection happened *before* any
    /// binary was spawned (AIC-FR-27).
    runner: Arc<FakeRunner>,
}

fn harness(
    fs: Arc<FakeFs>,
    runner: Arc<FakeRunner>,
    prober: Arc<FakeProber>,
    keys: Arc<FakeKeychain>,
) -> Harness {
    Harness {
        store: GlobalSettingsStore::in_memory(),
        ai: AgenticIntegrations::new(
            Box::new(fs.clone()),
            Box::new(runner.clone()),
            Box::new(prober),
            Box::new(keys.clone()),
        ),
        fs,
        keys,
        runner,
    }
}

fn cli_config(path: &str) -> VerifyConfig {
    VerifyConfig {
        path: Some(path.to_string()),
        ..Default::default()
    }
}

/// A well-formed OAuth token (AIC-FR-27). It is a fake value, not a real credential.
const SAMPLE_TOKEN: &str = "sk-ant-oat01-FAKE-TEST-TOKEN-NOT-A-REAL-CREDENTIAL-000000000000000000000000000000000000000-ygAA";

/// The configuration a CLI tab submits for `vendor`, carrying a fresh OAuth
/// token where the vendor requires one (AIC-FR-26).
///
/// Most tests below are about paths, identity, timeouts, or selections
/// rather than about credentials, and making each of them restate Claude
/// Code's token rule would bury what they are actually asserting. The tests
/// that *are* about the token build their config explicitly instead.
fn cli_config_for(vendor: &str, path: &str) -> VerifyConfig {
    VerifyConfig {
        path: Some(path.to_string()),
        oauth_token: vendor_descriptor(vendor)
            .is_some_and(AgenticVendor::requires_oauth_token)
            .then(|| SAMPLE_TOKEN.to_string()),
        ..Default::default()
    }
}

fn api_config(base_url: &str, key: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        base_url: Some(base_url.to_string()),
        api_key: key.map(str::to_string),
        ..Default::default()
    }
}

fn find<'a>(list: &'a [AgenticIntegration], vendor: &str) -> &'a AgenticIntegration {
    list.iter().find(|i| i.vendor == vendor).unwrap()
}

/// A harness whose Claude Code binary verifies, so the tests below vary only
/// the credential.
fn claude_harness() -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/claude"]),
        FakeRunner::saying("/usr/bin/claude", "claude 2.1.4"),
        Arc::new(FakeProber::default()),
        FakeKeychain::new(),
    )
}

/// Each CLI vendor's binary and the banner it identifies itself with.
const CLI_BINARIES: &[(&str, &str)] = &[
    ("claude_code", "/usr/bin/claude"),
    ("codex", "/usr/bin/codex"),
    ("opencode", "/usr/bin/opencode"),
];

/// A harness where all three CLIs are genuinely runnable, so a test can
/// configure every vendor rather than assert against unconfigured defaults.
fn all_clis_harness() -> Harness {
    harness(
        FakeFs::with_executable(&["/usr/bin/claude", "/usr/bin/codex", "/usr/bin/opencode"]),
        FakeRunner::saying_each(&[
            ("/usr/bin/claude", "claude 2.1.4"),
            ("/usr/bin/codex", "codex-cli 0.4.0"),
            ("/usr/bin/opencode", "opencode 0.9.1"),
        ]),
        FakeProber::returning(&[("claude-opus-5", "Claude Opus 5")]),
        FakeKeychain::new(),
    )
}

fn claude_config(oauth_token: Option<&str>) -> VerifyConfig {
    VerifyConfig {
        path: Some("/usr/bin/claude".into()),
        oauth_token: oauth_token.map(str::to_string),
        ..Default::default()
    }
}

mod listing_detection;
mod cli_verification;
mod models_and_efforts;
mod selections;
mod active_choice;
mod project_override;
mod real_runner;
mod invocation;
mod api_vendors;
mod vault;
mod keychain;
mod record_shape;
mod oauth_token;
mod launch_handoff;
mod gateway;
mod gateway_bedrock;
