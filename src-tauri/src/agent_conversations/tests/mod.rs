//! The test scenarios of `AGC-agent-conversations.md` and of
//! `../ai/CVL-conversation-loop.md`, whose implementations share this module.
//!
//! Every one of them runs against [`ScriptedCompletion`] rather than a provider:
//! no request leaves the machine, no key is needed, and no token is spent
//! (CVL-FR-11). A test that reached a network would be a test that had bypassed
//! the seam, which is the property CVL-FR-11 asserts directly.

use super::*;
use std::path::{Path, PathBuf};

use std::sync::mpsc;

use crate::agents::{self, AgentDraft};
use crate::ai_api::{AiApiRecord, ReasoningChoice};
use crate::ai_shared::{ModelOption, ModelReasoning, ModelsOrigin};
use crate::comments::{FragmentTarget, Discussion, LogScope};
use crate::global_settings::GlobalSettingsStore;
use crate::logging::{self, LogFilter, LogLevel, LogRecord};
use crate::progress::ProgressRegistry;
use crate::project::ProjectState;

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{App, Listener, Manager};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// The buffer this module's harness reports into, in place of the session's.
///
/// Nothing else in the process holds it, so nothing else can clear it: the
/// records a test appends are the records it reads. It stands with the harness
/// rather than with the tests that read it, because the harness is what binds
/// it to every turn.
static TEST_BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();

const PROJECT_KEY: &str = "proj";

/// The turn deadline most tests run under: short, so a test that needs
/// CVL-FR-16's bound to bite does not have to wait two minutes for it.
const SHORT_TIMEOUT: Duration = Duration::from_millis(400);

struct Harness {
    app: App<tauri::test::MockRuntime>,
    root: tempfile::TempDir,
    seam: Arc<ScriptedCompletion>,
    /// Every terminal event this app has emitted, captured from construction.
    terminal: Arc<Mutex<Vec<AgentTurn>>>,
    /// *Every* turn event, terminal or not — which is what makes CVL-FR-20's
    /// "nothing about a retry reaches a surface" an observation rather than an
    /// inference from the terminal set.
    events: Arc<Mutex<Vec<AgentTurn>>>,
}

/// CVL-FR-19: the shipped schedule with its waits collapsed.
///
/// The attempt count, the jitter fraction, and the ceiling are the real ones —
/// only the two base delays shrink, because what nearly every test wants is that
/// a call was repeated three times and not that it waited 1.5 seconds to do it.
fn fast_retries() -> RetryPolicy {
    RetryPolicy {
        first_base: Duration::from_millis(1),
        ..RetryPolicy::default()
    }
}

/// CVL-FR-19: a jitter source that always returns the same fraction, so a
/// backoff is asserted to the millisecond rather than tolerated to a margin.
struct FixedJitter(f64);

impl JitterSource for FixedJitter {
    fn fraction(&self) -> f64 {
        self.0
    }
}

impl Harness {
    fn new(replies: Vec<Result<String, &'static str>>) -> Self {
        Self::with(replies, Duration::ZERO, DEFAULT_CONCURRENCY)
    }

    fn with(
        replies: Vec<Result<String, &'static str>>,
        delay: Duration,
        concurrency: usize,
    ) -> Self {
        let seam = Arc::new(ScriptedCompletion::new(replies).with_delay(delay));
        Self::build(Box::new(seam.clone()), seam, concurrency)
    }

    /// A harness over a seam of the test's own, for the cases a scripted reply
    /// cannot express — a call that blocks, or one that never answers.
    fn with_seam(seam: Box<dyn CompletionSeam>, concurrency: usize) -> Self {
        Self::build(seam, Arc::new(ScriptedCompletion::new(vec![])), concurrency)
    }

    /// [`Harness::with_seam`], with a deadline long enough that a test which
    /// gates a call is not also racing the turn timeout.
    fn with_patient_seam(seam: Box<dyn CompletionSeam>) -> Self {
        Self::build_with(
            seam,
            Arc::new(ScriptedCompletion::new(vec![])),
            DEFAULT_CONCURRENCY,
            Box::new(FixedSecrets),
            Duration::from_secs(30),
        )
    }

    /// A harness whose model can ask for tools (CVL-FR-12).
    ///
    /// Given a generous deadline rather than [`SHORT_TIMEOUT`]: a turn that
    /// loops spends real time dispatching tools, and CVL-FR-16's bound now spans
    /// the whole turn, so the short deadline that keeps the *timeout* tests
    /// quick would make these ones a race against the machine's load. The tests
    /// that want the deadline to bite ask for it explicitly.
    fn scripted(replies: Vec<Result<ScriptedReply, &'static str>>) -> Self {
        let seam = Arc::new(ScriptedCompletion::scripted(replies));
        Self::build_with(
            Box::new(seam.clone()),
            seam,
            DEFAULT_CONCURRENCY,
            Box::new(FixedSecrets),
            Duration::from_secs(30),
        )
    }

    /// A looping model under the *short* deadline, so CVL-FR-16's whole-turn
    /// bound can be observed biting.
    fn scripted_impatient(
        replies: Vec<Result<ScriptedReply, &'static str>>,
        delay: Duration,
    ) -> Self {
        let seam = Arc::new(ScriptedCompletion::scripted(replies).with_delay(delay));
        Self::build_with(
            Box::new(seam.clone()),
            seam,
            DEFAULT_CONCURRENCY,
            Box::new(FixedSecrets),
            SHORT_TIMEOUT,
        )
    }

    /// Mount the content root into the indexes and run one pass, so the tools
    /// answer from a real project rather than refusing for want of one.
    fn mount(&self) -> &Self {
        let indexer = self.app.state::<crate::bm25_index::Bm25Indexer>();
        let root = self.root();
        indexer.mount(&root);
        let generation = indexer.generation();
        indexer
            .run_pass(
                &self.app.handle().clone(),
                &root,
                crate::bm25_index::PassScope::ALL,
                generation,
            )
            .expect("the pass publishes");
        self.app
            .state::<crate::fs::FsAccessState>()
            .install_for_worktree(self.root().path())
            .expect("a test root is a real directory");
        self
    }

    /// How many agent sessions currently hold a filesystem instance
    /// (CVL-FR-24).
    fn agent_sessions(&self) -> usize {
        self.app
            .state::<crate::fs::FsAccessState>()
            .agent_session_count()
    }

    /// A harness whose keychain refuses to answer, so a turn fails where a real
    /// one does — inside `resolve_ai_api_endpoint` — rather than at a scripted
    /// seam. The agent is still *enrollable*: AAP-FR-33 validates a selection
    /// without touching the credential store, so the dispatch succeeds and the
    /// registered turn is what fails.
    fn with_locked_keychain(replies: Vec<Result<String, &'static str>>) -> Self {
        let seam = Arc::new(ScriptedCompletion::new(replies));
        Self::build_with(
            Box::new(seam.clone()),
            seam,
            DEFAULT_CONCURRENCY,
            Box::new(LockedSecrets),
            SHORT_TIMEOUT,
        )
    }

    fn build(
        seam: Box<dyn CompletionSeam>,
        recorder: Arc<ScriptedCompletion>,
        concurrency: usize,
    ) -> Self {
        Self::build_with(
            seam,
            recorder,
            concurrency,
            Box::new(FixedSecrets),
            SHORT_TIMEOUT,
        )
    }

    /// A harness running under a retry schedule of the test's own, for the
    /// cases that assert the waiting rather than the repeating.
    fn with_seam_and_retries(
        seam: Box<dyn CompletionSeam>,
        retry: RetryPolicy,
        timeout: Duration,
    ) -> Self {
        Self::build_full(
            seam,
            Arc::new(ScriptedCompletion::new(vec![])),
            DEFAULT_CONCURRENCY,
            Box::new(FixedSecrets),
            timeout,
            retry,
            0.5,
        )
    }

    fn with_retries(
        replies: Vec<Result<String, &'static str>>,
        retry: RetryPolicy,
        jitter: f64,
        timeout: Duration,
    ) -> Self {
        let seam = Arc::new(ScriptedCompletion::new(replies));
        Self::build_full(
            Box::new(seam.clone()),
            seam,
            DEFAULT_CONCURRENCY,
            Box::new(FixedSecrets),
            timeout,
            retry,
            jitter,
        )
    }

    fn build_with(
        seam: Box<dyn CompletionSeam>,
        recorder: Arc<ScriptedCompletion>,
        concurrency: usize,
        secrets: Box<dyn crate::ai_shared::SecretStore>,
        timeout: Duration,
    ) -> Self {
        Self::build_full(
            seam,
            recorder,
            concurrency,
            secrets,
            timeout,
            fast_retries(),
            0.5,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_full(
        seam: Box<dyn CompletionSeam>,
        recorder: Arc<ScriptedCompletion>,
        concurrency: usize,
        secrets: Box<dyn crate::ai_shared::SecretStore>,
        timeout: Duration,
        retry: RetryPolicy,
        jitter: f64,
    ) -> Self {
        let seam_for_registry = seam;
        let seam = recorder;
        let root = tempfile::tempdir().expect("temp root");
        let app = mock_builder()
            .manage(GlobalSettingsStore::in_memory())
            .manage(crate::ai_api::AiApiIntegrations::new(
                Box::new(NoProbe),
                secrets,
            ))
            .manage(
                TurnRegistry::new(seam_for_registry, concurrency, timeout)
                // Not the session buffer: `close_project` and a worktree change
                // clear that one (LGC-FR-15), and the tests of those paths run
                // alongside these — reading it means reading a buffer another
                // test can empty mid-assertion.
                .reporting_into(&TEST_BUFFER)
                // CVL-FR-19: the retry *behaviour* without paying for the
                // waits. Every path a retry can take is exercised at this
                // schedule; that the shipped schedule is 500 ms and 1,000 ms
                // within ±20% and capped at 2 s is
                // `the_default_retry_schedule_is_the_one_the_spec_states`'s
                // assertion, made against `RetryPolicy::default()` itself.
                .with_retry_policy(retry)
                // A fixed fraction, so a delay is decided rather than drawn and
                // a test that asserts timing has nothing random in it.
                .with_jitter(Box::new(FixedJitter(jitter))),
            )
            .manage(ProgressRegistry::default())
            .manage(ProjectState::default())
            // The state the five tools of CVL-FR-08 answer from. Managed for
            // every harness rather than only the tool tests, because a turn that
            // silently lost its tools would otherwise still look green: the
            // tools would refuse "no project open" and the loop would carry on.
            .manage(crate::bm25_index::Bm25Indexer::default())
            .manage(crate::fs::FsAccessState::default())
            .manage(crate::scanning::CandidateStore::default())
            .build(mock_context(noop_assets()))
            .expect("mock app");
        let terminal: Arc<Mutex<Vec<AgentTurn>>> = Arc::new(Mutex::new(Vec::new()));
        let events: Arc<Mutex<Vec<AgentTurn>>> = Arc::new(Mutex::new(Vec::new()));
        let all = events.clone();
        app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
            if let Ok(turn) = serde_json::from_str::<AgentTurn>(event.payload()) {
                all.lock().unwrap_or_else(|e| e.into_inner()).push(turn);
            }
        });
        // Subscribed here rather than in each test: `listen` does not replay, so
        // a listener registered *after* a dispatch can miss a turn that finished
        // first — the one way these tests could go red without anything being
        // wrong.
        let sink = terminal.clone();
        app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
            if let Ok(turn) = serde_json::from_str::<AgentTurn>(event.payload()) {
                if turn.ended_at.is_some() {
                    sink.lock().unwrap_or_else(|e| e.into_inner()).push(turn);
                }
            }
        });
        // Tauri hands an emit that finds the listener table busy to the thread
        // holding it, and that thread delivers the queued emit only when its own
        // event had a listener. Two turn threads emitting at once — one a
        // progress event nobody here listens to, the other the terminal turn
        // event — could therefore queue the terminal event and never deliver it.
        // A listener for every event a turn thread emits makes every emit
        // flush the queue.
        for event in [
            crate::progress::OPERATION_PROGRESS,
            crate::logging::LOG_RECORDS_APPENDED,
            crate::agent_activity::AGENT_ACTIVITY_APPENDED,
            crate::comments::DISCUSSION_CHANGED,
            crate::comments::DISCUSSION_QUESTION_SET_CHANGED,
        ] {
            app.listen(event, |_| {});
        }
        let harness = Self {
            app,
            root,
            seam,
            terminal,
            events,
        };
        harness.seed_verified_provider();
        harness
    }

    fn store(&self) -> tauri::State<'_, GlobalSettingsStore> {
        self.app.state::<GlobalSettingsStore>()
    }

    fn turns(&self) -> tauri::State<'_, TurnRegistry> {
        self.app.state::<TurnRegistry>()
    }

    fn progress(&self) -> tauri::State<'_, ProgressRegistry> {
        self.app.state::<ProgressRegistry>()
    }

    /// The harness's content root, governed — the shape every module under
    /// test now takes (FSA-FR-19).
    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.root.path())
    }

    /// A verified OpenRouter offering one reasoning-capable model, so an agent
    /// created against it reads `ready`.
    fn seed_verified_provider(&self) {
        self.store()
            .save_ai_api_registry(
                vec![AiApiRecord {
                    turn_timeout_ms: None,
                    provider: "openrouter".into(),
                    base_url: Some("https://openrouter.example/api/v1".into()),
                    masked_hint: Some("cdef".into()),
                    verified_at: Some("2026-01-01T00:00:00Z".into()),
                    selected_model: Some("provider-default-model".into()),
                    models_origin: ModelsOrigin::Probed,
                    selected_reasoning: Some(ReasoningChoice::Effort {
                        effort: "low".into(),
                    }),
                    models: vec![
                        ModelOption::new("m", "M").with_reasoning(Some(ModelReasoning {
                            mandatory: false,
                            supported_efforts: Some(vec!["high".into(), "low".into()]),
                            default_effort: None,
                            ..Default::default()
                        })),
                        ModelOption::new("provider-default-model", "Provider default"),
                    ],
                }],
                None,
            )
            .expect("registry");
    }

    /// AAP-FR-35: the same verified provider, whose model `m` **declares that
    /// it takes image content**.
    ///
    /// Every other harness model declares nothing about its input modalities,
    /// which AAP-FR-35 reads as absent — so without this no turn in this crate
    /// could ever carry a picture, and every assertion about a multimodal
    /// request would be an assertion about a text-only one.
    fn seed_image_capable_provider(&self) {
        self.store()
            .save_ai_api_registry(
                vec![AiApiRecord {
                    turn_timeout_ms: None,
                    provider: "openrouter".into(),
                    base_url: Some("https://openrouter.example/api/v1".into()),
                    masked_hint: Some("cdef".into()),
                    verified_at: Some("2026-01-01T00:00:00Z".into()),
                    selected_model: Some("m".into()),
                    models_origin: ModelsOrigin::Probed,
                    selected_reasoning: None,
                    models: vec![
                        ModelOption::new("m", "M")
                            .with_reasoning(Some(ModelReasoning {
                                mandatory: false,
                                supported_efforts: Some(vec!["high".into(), "low".into()]),
                                default_effort: None,
                                ..Default::default()
                            }))
                            .with_declared_image_input(Some(true)),
                    ],
                }],
                None,
            )
            .expect("registry");
    }

    /// CVL-FR-30: a second verified provider, for the claims that turn on which
    /// provider carries a call. Saved beside the OpenRouter record rather than
    /// over it, and made the active provider, because an agent is served by the
    /// provider the project resolves to (AGC-FR-14).
    fn seed_second_provider(&self, provider: &str) {
        self.store()
            .save_ai_api_registry(
                vec![
                    AiApiRecord {
                        turn_timeout_ms: None,
                        provider: "openrouter".into(),
                        base_url: Some("https://openrouter.example/api/v1".into()),
                        masked_hint: Some("cdef".into()),
                        verified_at: Some("2026-01-01T00:00:00Z".into()),
                        selected_model: Some("provider-default-model".into()),
                        models_origin: ModelsOrigin::Probed,
                        selected_reasoning: Some(ReasoningChoice::Effort {
                            effort: "low".into(),
                        }),
                        models: vec![
                            ModelOption::new("m", "M").with_reasoning(Some(ModelReasoning {
                                mandatory: false,
                                supported_efforts: Some(vec!["high".into(), "low".into()]),
                                default_effort: None,
                                ..Default::default()
                            })),
                            ModelOption::new("provider-default-model", "Provider default"),
                        ],
                    },
                    AiApiRecord {
                        turn_timeout_ms: None,
                        provider: provider.into(),
                        base_url: Some("https://provider.example/v1".into()),
                        masked_hint: Some("9876".into()),
                        verified_at: Some("2026-01-01T00:00:00Z".into()),
                        selected_model: Some("m".into()),
                        models_origin: ModelsOrigin::Probed,
                        selected_reasoning: None,
                        models: vec![ModelOption::new("m", "M")],
                    },
                ],
                Some(provider.to_string()),
            )
            .expect("registry");
    }

    /// An agent with no reasoning choice, for the harness whose active provider
    /// declares none (AGC-FR-14).
    fn create_plain_agent(&self, nickname: &str) -> agents::Agent {
        let agent = agents::create_agent_impl(
            &self.store(),
            "",
            &AgentDraft {
                nickname: nickname.into(),
                title: String::new(),
                model_id: "m".into(),
                instructions: "Argue about structure.".into(),
                reasoning: None,
            },
        )
        .expect("agent");
        agents::enrol_project_agent_impl(&self.store(), PROJECT_KEY, &agent.id).expect("enrol");
        agent
    }

    fn create_agent(&self, nickname: &str, instructions: &str) -> agents::Agent {
        self.create_titled_agent(nickname, instructions, "")
    }

    /// AGR-FR-23: an agent carrying a title, for the paths that snapshot one.
    fn create_titled_agent(
        &self,
        nickname: &str,
        instructions: &str,
        title: &str,
    ) -> agents::Agent {
        let agent = agents::create_agent_impl(
            &self.store(),
            "",
            &AgentDraft {
                nickname: nickname.into(),
                title: title.into(),
                model_id: "m".into(),
                instructions: instructions.into(),
                reasoning: Some(ReasoningChoice::Effort {
                    effort: "high".into(),
                }),
            },
        )
        .expect("agent");
        agents::enrol_project_agent_impl(&self.store(), PROJECT_KEY, &agent.id).expect("enrol");
        agent
    }

    /// Write an artifact and open a thread on it.
    fn seed_artifact_thread(&self, artifact_id: &str, source: &str) -> Discussion {
        std::fs::write(self.root().path().join(artifact_id), source).expect("artifact");
        crate::comments::open_artifact_fragment_in(
            &self.root(),
            artifact_id,
            FragmentTarget::in_artifact("", 4, 12, &source.chars().skip(4).take(8).collect::<String>()),
            "@arch what do you think?".into(),
            Vec::new(),
            &human("ada"),
            "2026-01-01T00:00:00Z")
        .expect("thread")
    }

    fn dispatch(
        &self,
        nickname: &str,
        origin: ConversationOrigin,
        comment_id: &str,
    ) -> Result<AgentTurn, String> {
        self.dispatch_in(PROJECT_KEY, nickname, origin, comment_id)
    }

    /// The same dispatch, attributed to a named project. A withdrawal is from
    /// one project rather than from the machine (AGR-FR-15), so telling the two
    /// apart needs turns belonging to each.
    fn dispatch_in(
        &self,
        project_key: &str,
        nickname: &str,
        origin: ConversationOrigin,
        comment_id: &str,
    ) -> Result<AgentTurn, String> {
        dispatch_impl(
            &self.app.handle().clone(),
            Roots::same(&self.root()),
            project_key,
            nickname,
            origin,
            comment_id,
        )
    }

    /// Wait until every turn has left the in-flight set. Turns run on their own
    /// threads, so the alternative is a fixed sleep — flaky in both directions.
    fn settle(&self) {
        for _ in 0..600 {
            if self.turns().in_flight(None).is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("turns did not settle");
    }

    fn threads(&self, artifact_id: &str) -> Vec<Discussion> {
        crate::comments::list_fragment_discussions_in(&self.root(), artifact_id)
    }
}

mod support;
// The shared helpers stand beside the harness for every scenario that uses
// them, so a scenario names one module and not two. They are `pub(super)` to
// `support`, which is this module, so the re-export carries no wider visibility
// than that.
use support::*;

mod anthropic_carrier;
mod asking_author;
mod question_set_turns;
mod builders;
mod cancellation;
mod citation_markers;
mod custom_adapter;
mod deadlines;
mod prose_replies;
mod disclosure;
mod dispatch;
mod durable_record;
mod images;
mod note_discussions;
mod openrouter;
mod origin_contract;
mod project_bounds;
mod provider_failures;
mod provider_tools;
mod proposal_history;
mod request_history;
mod request_shape;
mod request_failures;
mod responses_repair;
mod retries;
mod session_log;
mod tool_loop_bounds;
mod tool_loop_dispatch;
mod tls_carriers;
mod tool_loop_results;
mod tool_result_call_ids;
mod unified_association;
