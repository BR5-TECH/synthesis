//! EAC-FR-01 … EAC-FR-10, EAC-FR-38.
//!
//! Every test here runs against the `DockerRuntime` seam (EAC-FR-17), not
//! against a daemon: the double records the exact argument vector, the exact
//! environment, and the exact stdin bytes, then replays a configured stdout,
//! stderr, and exit status. That is the same substitution
//! `../infra/ACM-agentic-cli-mock.md`'s `docker` adapter makes from the other
//! side of the process boundary, and it is why the suite passes on a machine
//! with no Docker installed (AVI-FR-12).
//!
//! Nothing below changes production integration resolution. The vendor is
//! resolved through AIC exactly as it is in a shipped build; only the program
//! the seam would have launched is different.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::json;

use super::descriptor::{self, ResponseSource, SessionStateMount};
use super::protocol::*;
use super::runtime::*;
use super::*;
use crate::agentic::{
    self, verify_integration_impl, AgenticIntegrations, CliOutput, CliRunner,
    FileProbe, RunError, VerifyConfig,
};
use crate::ai_shared::{
    EndpointProber, ModelOption, ProbeError, ProbeRequest, SecretStore, SecretUnavailable,
};
use crate::fs::FsAccess;
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{BufferState, LogSink};

mod support;
use support::*;

/// The same executor driven through a real process boundary, with
/// `agentic-cli-mock` standing in for `docker`. A child module so it shares
/// this one's harness and fixtures rather than growing a parallel set.
#[path = "../mock_tests/mod.rs"]
mod mock_tests;

mod merge_turns;
mod rebase_mounts;
mod rebase_isolation;
mod surface;
mod task_document;
mod envelope;
mod capture;
mod concurrency;
mod decoding;
mod controls;
mod watching;
mod durable;
mod observers;
mod contracts;

/// The sample well-formed token AIC's own suite uses (AIC-FR-27). It is a fake value.
const SAMPLE_TOKEN: &str = "sk-ant-oat01-FAKE-TEST-TOKEN-NOT-A-REAL-CREDENTIAL-000000000000000000000000000000000000000-ygAA";

// ---------------------------------------------------------------------------
// Doubles
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &BufferState) {}
}

#[derive(Default)]
struct FakeProbe {
    executables: Vec<String>,
    directories: Vec<PathBuf>,
}

impl FileProbe for FakeProbe {
    fn exists(&self, path: &Path) -> bool {
        self.executables.iter().any(|p| Path::new(p) == path)
    }
    fn is_executable(&self, path: &Path) -> bool {
        self.exists(path)
    }
    fn dir_exists(&self, path: &Path) -> bool {
        self.directories.iter().any(|p| p == path) || path.is_dir()
    }
}

struct FakeRunner {
    banners: HashMap<String, String>,
}

impl CliRunner for FakeRunner {
    fn run(&self, path: &Path, _args: &[&str], _timeout: Duration) -> Result<CliOutput, RunError> {
        match self.banners.get(&path.to_string_lossy().to_string()) {
            Some(banner) => Ok(CliOutput {
                stdout: banner.clone(),
                stderr: String::new(),
                success: true,
            }),
            None => Err(RunError::NotFound),
        }
    }
}

/// A prober that answers, so an API-kind vendor can be verified.
#[derive(Default)]
struct OkProber;
impl EndpointProber for OkProber {
    fn probe(&self, _request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError> {
        Ok(vec![ModelOption::new("agent-1", "Agent One")])
    }
}

#[derive(Default)]
struct NoProber;
impl EndpointProber for NoProber {
    fn probe(&self, _request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct FakeKeys(StdMutex<HashMap<String, String>>);

impl SecretStore for Arc<FakeKeys> {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        self.0
            .lock()
            .unwrap()
            .insert(id.to_string(), secret.to_string());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
        Ok(self.0.lock().unwrap().get(id).cloned())
    }
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}

/// One recorded launch, in the form a test asserts against.
#[derive(Clone, Debug)]
struct Recorded {
    argv: Vec<String>,
    /// EAC-FR-39: the structured launch the seam was handed, which the vector
    /// beside it is the Docker CLI backend's rendering of.
    container: Option<descriptor::ContainerSpec>,
    env: BTreeMap<String, String>,
    stdin: Vec<u8>,
    timeout: Duration,
    /// The caller's own token, not a fresh one — identity is the claim.
    cancel: CancellationToken,
    stdout_limit: usize,
    stderr_limit: usize,
}

/// What the seam should do when asked.
#[derive(Clone)]
enum Script {
    /// Replay a response and exit status.
    Reply {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    /// Replay bytes that are not valid UTF-8.
    ReplyRaw { stdout: Vec<u8>, exit_code: i32 },
    /// Replay more bytes on a stream than its bound allows.
    Flood { stream: &'static str },
    /// End as if the deadline elapsed.
    TimeOut,
    /// End as if the caller cancelled.
    Cancel,
    /// End with no exit code, as a signal from outside would.
    Terminate,
    /// Answer each vendor in its own output shape, decided from the argv.
    PerVendor,
}

/// The recording seam (EAC-FR-17).
struct RecordingRuntime {
    script: Script,
    available: bool,
    image_available: bool,
    launch_error: Option<RuntimeError>,
    remove_fails: bool,
    runs: StdMutex<Vec<Recorded>>,
    removed: StdMutex<Vec<String>>,
    images_asked: StdMutex<Vec<String>>,
}

impl RecordingRuntime {
    fn replying(stdout: &str) -> Arc<Self> {
        Arc::new(RecordingRuntime {
            script: Script::Reply {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code: 0,
            },
            available: true,
            image_available: true,
            launch_error: None,
            remove_fails: false,
            runs: StdMutex::new(Vec::new()),
            removed: StdMutex::new(Vec::new()),
            images_asked: StdMutex::new(Vec::new()),
        })
    }

    /// A run that writes to both streams, so the success path can be asserted
    /// with diagnostics present rather than only with an empty stderr.
    fn replying_with_stderr(stdout: &str, stderr: &str) -> Arc<Self> {
        Arc::new(RecordingRuntime {
            script: Script::Reply {
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                exit_code: 0,
            },
            available: true,
            image_available: true,
            launch_error: None,
            remove_fails: false,
            runs: StdMutex::new(Vec::new()),
            removed: StdMutex::new(Vec::new()),
            images_asked: StdMutex::new(Vec::new()),
        })
    }

    /// A run that exits non-zero, having said why on stderr.
    fn refused(stdout: &str, stderr: &str, exit_code: i32) -> Arc<Self> {
        Arc::new(RecordingRuntime {
            script: Script::Reply {
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                exit_code,
            },
            available: true,
            image_available: true,
            launch_error: None,
            remove_fails: false,
            runs: StdMutex::new(Vec::new()),
            removed: StdMutex::new(Vec::new()),
            images_asked: StdMutex::new(Vec::new()),
        })
    }

    fn broken(available: bool, image_available: bool, launch: Option<RuntimeError>) -> Arc<Self> {
        Arc::new(RecordingRuntime {
            script: Script::Reply {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            },
            available,
            image_available,
            launch_error: launch,
            remove_fails: false,
            runs: StdMutex::new(Vec::new()),
            removed: StdMutex::new(Vec::new()),
            images_asked: StdMutex::new(Vec::new()),
        })
    }

    fn only_run(&self) -> Recorded {
        let runs = self.runs.lock().unwrap();
        assert_eq!(runs.len(), 1, "expected exactly one launch");
        runs[0].clone()
    }

    fn launched(&self) -> usize {
        self.runs.lock().unwrap().len()
    }
}

impl DockerRuntime for RecordingRuntime {
    fn ensure_available(&self) -> BoxFuture<'_, Result<(), RuntimeError>> {
        Box::pin(async move {
            if self.available {
                Ok(())
            } else {
                Err(RuntimeError::Unavailable)
            }
        })
    }

    fn ensure_image<'a>(&'a self, image: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            self.images_asked.lock().unwrap().push(image.to_string());
            if self.image_available {
                Ok(())
            } else {
                Err(RuntimeError::ImageUnavailable)
            }
        })
    }

    fn run<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move {
            self.runs.lock().unwrap().push(Recorded {
                argv: request.argv.to_vec(),
                container: request.container.cloned(),
                env: request
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.expose().to_string()))
                    .collect(),
                // Docker's own rule, modelled rather than assumed: a container
                // launched without `--interactive` is given `/dev/null` for
                // stdin, and whatever the client wrote to the pipe reaches
                // nobody. The double behaves like the runtime rather than like
                // a recording of one — the same reason `substitute_session`
                // exists — which is what makes every assertion about the task
                // arriving on stdin a statement about it being *reachable*
                // rather than about a vector element being present.
                stdin: if request.argv.iter().any(|a| a == "--interactive" || a == "-i") {
                    request.stdin.to_vec()
                } else {
                    Vec::new()
                },
                timeout: request.timeout,
                cancel: request.cancel.clone(),
                stdout_limit: request.stdout_limit,
                stderr_limit: request.stderr_limit,
            });

            if let Some(error) = &self.launch_error {
                return Err(error.clone());
            }

            let outcome = match &self.script {
                Script::Reply {
                    stdout,
                    stderr,
                    exit_code,
                } => RuntimeOutcome {
                    end: RunEnd::Exited,
                    exit_code: Some(*exit_code),
                    stdout: CapturedStream {
                        bytes: substitute_session(stdout, request.argv).into_bytes(),
                        truncated: false,
                    },
                    stderr: CapturedStream {
                        bytes: stderr.as_bytes().to_vec(),
                        truncated: false,
                    },
                },
                Script::ReplyRaw { stdout, exit_code } => RuntimeOutcome {
                    end: RunEnd::Exited,
                    exit_code: Some(*exit_code),
                    stdout: CapturedStream {
                        bytes: stdout.clone(),
                        truncated: false,
                    },
                    stderr: CapturedStream::default(),
                },
                Script::Flood { stream } => {
                    let full = CapturedStream {
                        bytes: vec![b'x'; 64],
                        truncated: true,
                    };
                    RuntimeOutcome {
                        end: RunEnd::Exited,
                        exit_code: Some(0),
                        stdout: if *stream == "stdout" {
                            full.clone()
                        } else {
                            CapturedStream::default()
                        },
                        stderr: if *stream == "stderr" {
                            full
                        } else {
                            CapturedStream::default()
                        },
                    }
                }
                Script::TimeOut => RuntimeOutcome {
                    end: RunEnd::TimedOut,
                    exit_code: None,
                    // A turn that wrote a whole envelope and then hung still
                    // times out: a partial run is never promoted.
                    stdout: CapturedStream {
                        bytes: valid_claude_stdout().into_bytes(),
                        truncated: false,
                    },
                    stderr: CapturedStream::default(),
                },
                Script::Cancel => RuntimeOutcome {
                    end: RunEnd::Cancelled,
                    exit_code: None,
                    stdout: CapturedStream::default(),
                    stderr: CapturedStream::default(),
                },
                Script::PerVendor => {
                    let is_codex = request.argv.iter().any(|a| a == "exec");
                    let stdout = if is_codex {
                        codex_stdout(&envelope_json("success"))
                    } else {
                        valid_claude_stdout()
                    };
                    RuntimeOutcome {
                        end: RunEnd::Exited,
                        exit_code: Some(0),
                        stdout: CapturedStream {
                            bytes: substitute_session(&stdout, request.argv).into_bytes(),
                            truncated: false,
                        },
                        stderr: CapturedStream::default(),
                    }
                }
                Script::Terminate => RuntimeOutcome {
                    end: RunEnd::Terminated,
                    exit_code: None,
                    stdout: CapturedStream::default(),
                    stderr: CapturedStream::default(),
                },
            };

            // EAC-FR-32: the runtime hands each complete line over as it
            // arrives. Modelled rather than skipped, for the same reason the
            // stdin gating above is: an observer that only ever saw what a real
            // pipe delivered would be untested by every launch in this file, and
            // the whole point of the seam is that a caller watches a run through
            // it. Delivered before the outcome is returned, because that is what
            // "while it happens" means.
            if let Some(observer) = request.observer {
                for stream in [
                    (StreamChannel::Stdout, &outcome.stdout),
                    (StreamChannel::Stderr, &outcome.stderr),
                ] {
                    let (channel, captured) = stream;
                    for line in captured.bytes.split(|b| *b == b'\n') {
                        if !line.is_empty() {
                            observer.line(channel, line, true);
                        }
                    }
                }
            }
            Ok(outcome)
        })
    }

    fn remove_container<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), RuntimeError>> {
        Box::pin(async move {
            self.removed.lock().unwrap().push(name.to_string());
            if self.remove_fails {
                return Err(RuntimeError::Unavailable);
            }
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    store: GlobalSettingsStore,
    ai: AgenticIntegrations,
    fs: FsAccess,
    workspace: tempfile::TempDir,
    /// The fake `$HOME` this project's Codex login sits under, kept so a test
    /// can name the credential directory the launch will mount.
    home: tempfile::TempDir,
    /// EAC-FR-31's host root, kept inside the harness so a test run never writes
    /// into the author's real `app_data_dir()`.
    sessions: tempfile::TempDir,
}

impl Harness {
    fn context(&self) -> LaunchContext<'_> {
        LaunchContext {
            store: &self.store,
            integrations: &self.ai,
            project_key: PROJECT_KEY,
            fs: &self.fs,
            images: &TEST_IMAGES,
        }
    }

    fn workspace(&self) -> PathBuf {
        self.workspace.path().to_path_buf()
    }

    fn sessions_root(&self) -> PathBuf {
        self.sessions.path().to_path_buf()
    }

    /// EAC-FR-ZKMR: where a turn against this harness stands. A temporary
    /// directory has a path a container can be given, so the ordinary case is
    /// the aligned one and the target is the host path itself — in the
    /// **canonical** spelling EAC-FR-02 gives the execution directory, which on
    /// this platform is not the one a temporary directory reports.
    fn workspace_target(&self) -> String {
        crate::changes::canonicalize_lenient(&self.workspace())
            .to_string_lossy()
            .into_owned()
    }


    /// Where this harness's session state for `vendor` lands, as the string the
    /// mount argument carries.
    fn session_state_dir(&self, vendor: &str) -> String {
        descriptor::session_state_dir(&self.sessions_root(), vendor, PROJECT_KEY)
            .to_string_lossy()
            .into_owned()
    }
}

/// The project every harness resolves for. Named so that the session-state path
/// a test expects is derived from the same value the executor hashes.
const PROJECT_KEY: &str = "/dev/acme";

/// A harness whose project resolves to `vendor`, configured through the
/// production AIC path rather than by writing the registry directly — so what
/// the executor resolves is what a real project would resolve.
fn harness_for(vendor: &str) -> Harness {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    std::fs::create_dir_all(home.path().join(".codex")).expect("codex login dir");

    let binary = match vendor {
        "claude_code" => "/usr/bin/claude",
        "codex" => "/usr/bin/codex",
        "opencode" => "/usr/bin/opencode",
        other => panic!("no binary for {other}"),
    };
    let banner = match vendor {
        "claude_code" => "claude 2.1.232",
        "codex" => "codex 0.147.0",
        _ => "opencode 1.0.0",
    };

    let probe = FakeProbe {
        executables: vec![binary.to_string()],
        directories: vec![home.path().join(".codex")],
    };
    let runner = FakeRunner {
        banners: HashMap::from([(binary.to_string(), banner.to_string())]),
    };

    let ai = AgenticIntegrations::new(
        Box::new(probe),
        Box::new(runner),
        Box::new(NoProber),
        Box::new(Arc::new(FakeKeys::default())),
    )
    .with_home(home.path());

    let store = GlobalSettingsStore::in_memory();
    let config = VerifyConfig {
        path: Some(binary.to_string()),
        oauth_token: (vendor == "claude_code").then(|| SAMPLE_TOKEN.to_string()),
        ..Default::default()
    };
    verify_integration_impl(&store, &ai, vendor, &config).expect("vendor verifies");
    agentic::set_active_impl(&store, &ai, vendor).expect("vendor activates");

    // Both roots, mirroring production: the shared `FsAccess` there allowlists
    // the worktree and `app_data_dir()`, and the session-state directory lives
    // under the latter (EAC-FR-31).
    let sessions = tempfile::tempdir().expect("sessions root");
    let fs = FsAccess::builder()
        .allow_root(workspace.path())
        .allow_root(sessions.path())
        .build()
        .expect("fs");

    Harness {
        store,
        ai,
        fs,
        workspace,
        home,
        sessions,
    }
}

/// A harness with nothing configured at all.
fn empty_harness() -> Harness {
    let home = tempfile::tempdir().expect("home");
    let workspace = tempfile::tempdir().expect("workspace");
    let ai = AgenticIntegrations::new(
        Box::new(FakeProbe::default()),
        Box::new(FakeRunner {
            banners: HashMap::new(),
        }),
        Box::new(NoProber),
        Box::new(Arc::new(FakeKeys::default())),
    )
    .with_home(home.path());
    let sessions = tempfile::tempdir().expect("sessions root");
    let fs = FsAccess::builder()
        .allow_root(workspace.path())
        .allow_root(sessions.path())
        .build()
        .expect("fs");
    Harness {
        store: GlobalSettingsStore::in_memory(),
        ai,
        fs,
        workspace,
        home,
        sessions,
    }
}

/// EAC-FR-38: the project's committed image, as this suite's fixture project
/// configures it.
///
/// A stand-in for `PSS-project-settings-storage.md`'s `resolve_project_vendor_image`
/// (PSS-FR-30), so the executor's own behaviour is exercised without a project
/// store behind it. It answers for the two executable vendors and refuses every
/// other, exactly as a project that configured those two would.
struct TestImages;

/// The reference this suite's fixture project configures for `vendor`.
fn test_image_reference(vendor: &str) -> String {
    format!("registry.example/synthesis-agent-{vendor}:test")
}

impl VendorImageSource for TestImages {
    fn image_for(&self, vendor: &str) -> Result<String, String> {
        if ["claude_code", "codex"].contains(&vendor) {
            Ok(test_image_reference(vendor))
        } else {
            Err(crate::project_settings::images::ERR_VENDOR_IMAGE_UNCONFIGURED.to_string())
        }
    }
}

static TEST_IMAGES: TestImages = TestImages;

/// A project that configures no image at all (PSS-FR-30's refusal).
struct NoImages;

impl VendorImageSource for NoImages {
    fn image_for(&self, _vendor: &str) -> Result<String, String> {
        Err(crate::project_settings::images::ERR_VENDOR_IMAGE_UNCONFIGURED.to_string())
    }
}

/// The shipped manifest correctly carries the unpublished sentinel (AVI-FR-08),
/// which the verify command must refuse. These entries stand in for what the
/// same manifest holds once the images are pushed, so the checks that read the
/// manifest exercise the published path without pretending the real ones exist.
#[allow(dead_code)]
fn published_image(vendor: &str) -> Option<&'static descriptor::ImageManifestEntry> {
    static PUBLISHED: std::sync::OnceLock<HashMap<String, descriptor::ImageManifestEntry>> =
        std::sync::OnceLock::new();
    PUBLISHED
        .get_or_init(|| {
            ["claude_code", "codex"]
                .into_iter()
                .map(|vendor| {
                    let real = descriptor::manifest_entry(vendor).expect("a manifest entry");
                    (
                        vendor.to_string(),
                        descriptor::ImageManifestEntry {
                            image_digest: format!("sha256:{}", "ab".repeat(32)),
                            ..real.clone()
                        },
                    )
                })
                .collect()
        })
        .get(vendor)
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}
