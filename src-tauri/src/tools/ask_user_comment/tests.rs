//! Tests for `AUC-ask-user-comment-tool.md`.
//!
//! The group-wide conventions this tool shares with the other five are checked
//! once in `tools/tests.rs`; what follows is what is true of this tool alone.
//!
//! The turn-level consequences of a posted question — the loop ending, which
//! belongs to `../ai/CVL-conversation-loop.md`, and the `awaiting_reply` state and
//! the registration a later comment retires, which belong to
//! `AGC-agent-conversations.md` — are tested in `agent_conversations/tests.rs`.
//! What is tested here is the call: what it appends, what it refuses, and what
//! it reports.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;

use super::*;
use crate::comments::{FragmentTarget, Participant};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted, sorted_keys};
use crate::tools::ToolRefusal;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ARTIFACT: &str = "spec.md";

/// The author of the seed comment: a person, so the thread reads as a
/// conversation an agent was addressed in.
fn human() -> Participant {
    Participant::Human {
        login: "raver119".into(),
        display_name: None,
        email: None,
    }
}

/// The agent the tool posts as (AUC-FR-04).
fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        // CMS-FR-65: the snapshot the turn stamped, which this tool must carry
        // onto the comment it appends unchanged.
        title: Some("Developer".into()),
    }
}

/// A project holding one artifact with one open thread on it.
struct Fixture {
    /// An *open* project: the tool refuses without one (AUC-FR-13), so every
    /// fixture here mounts, exactly as the application would.
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    thread_id: String,
    asked: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().to_path_buf();
        std::fs::write(path.join(ARTIFACT), "one two three four five").expect("artifact");
        let root = crate::fs::RootFs::for_root(&path);
        let thread = crate::comments::open_artifact_fragment_in(
            &root,
            ARTIFACT,
            FragmentTarget::in_artifact("", 4, 7, "two"),
            "@arch which way?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("thread");
        Fixture {
            open: mounted(dir),
            path,
            thread_id: thread.id,
            asked: Arc::new(AtomicBool::new(false)),
        }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.open.handle()
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    /// The pair a conversation-bound tool holds: one temporary directory
    /// serving as both the worktree and the repository machine store.
    fn roots(&self) -> crate::agent_conversations::OwnedRoots {
        crate::agent_conversations::OwnedRoots {
            worktree: self.root(),
            store: self.root(),
        }
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_artifact(self.thread_id.clone(), ARTIFACT, true)
    }

    fn tool(&self) -> AskUserCommentTool<tauri::test::MockRuntime> {
        AskUserCommentTool::new(
            self.handle(),
            self.roots(),
            self.origin(),
            agent(),
            Arc::clone(&self.asked),
        )
    }

    fn reporting_into(&self, buffer: &'static LogBuffer) -> AskUserCommentTool<tauri::test::MockRuntime> {
        self.tool().with_buffer(buffer)
    }

    /// The thread as it now stands on disk.
    fn thread(&self) -> crate::comments::Discussion {
        crate::comments::list_fragment_discussions_in(&self.root(), ARTIFACT)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn lock(&self) {
        crate::comments::set_lock_in(
            &self.root(),
            ARTIFACT,
            &self.thread_id,
            true,
            &human(),
            "2026-01-01T00:01:00Z",
        )
        .expect("lock");
    }

    fn call(&self, args: AskUserCommentArgs) -> Result<AskUserCommentOutput, ToolRefusal> {
        block_on(self.tool().call(args))
    }
}

fn args(question: &str, options: Option<Vec<&str>>) -> AskUserCommentArgs {
    AskUserCommentArgs {
        question: question.into(),
        options: options.map(|o| o.into_iter().map(String::from).collect()),
    }
}

fn records_of(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records
}

fn rendered_records(buffer: &LogBuffer) -> String {
    serde_json::to_string(
        &buffer
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .unwrap()
            .records,
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// AUC-FR-01 — the tool is a portable tool and nothing else (AUC-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn is_a_portable_tool_named_ask_user_comment() {
    let fixture = Fixture::new();
    let definition = rig::tool::tool_definition(&fixture.tool());
    assert_eq!(definition.name, "ask_user_comment");
    assert_eq!(
        <AskUserCommentTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        definition.name.as_str(),
        "AUC-FR-01: NAME and ToolDefinition.name are one string",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-02 — the definition is fixed application data (AUC-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn definition_is_identical_across_turns_agents_and_conversations() {
    let one = Fixture::new();
    let two = Fixture::new();

    let a = rig::tool::tool_definition(&one.tool());
    // A different conversation, a different agent, a different project.
    let other = {
        AskUserCommentTool::new(
            two.handle(),
            crate::agent_conversations::OwnedRoots { worktree: two.root(), store: two.root() },
            ConversationOrigin::stub_draft("t-9", "d-9", false),
            Participant::Agent {
                agent_id: "agent-2".into(),
                handle: "sec".into(),
                model: None,
                title: None,
            },
            Arc::new(AtomicBool::new(false)),
        )
    };
    let b = rig::tool::tool_definition(&other);

    assert_eq!(a.description, b.description, "AUC-FR-02");
    assert_eq!(a.parameters, b.parameters, "AUC-FR-02");

    let schema = parameters();
    assert_eq!(schema["required"], serde_json::json!(["question"]));
    for key in ["question", "options"] {
        let description = schema["properties"][key]["description"]
            .as_str()
            .unwrap_or_default();
        assert!(
            !description.is_empty(),
            "AUC-FR-02: `{key}` carries its own description",
        );
    }
}

// ---------------------------------------------------------------------------
// AUC-FR-03 — the conversation is bound, never named (AUC-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn no_argument_names_a_conversation() {
    let schema = parameters();
    let properties = schema["properties"].as_object().expect("properties");
    let names: Vec<&String> = properties.keys().collect();
    assert_eq!(
        names.len(),
        2,
        "AUC-FR-03: exactly `question` and `options` — nothing naming a conversation",
    );
    for forbidden in ["thread_id", "threadId", "origin", "draft_id", "artifact_id", "author"] {
        assert!(
            !properties.contains_key(forbidden),
            "AUC-FR-03: `{forbidden}` is not an argument of this tool",
        );
    }
}

#[test]
fn posts_into_the_turns_own_conversation() {
    let fixture = Fixture::new();
    fixture.call(args("which way?", None)).expect("posted");
    assert_eq!(
        fixture.thread().comments.len(),
        2,
        "AUC-FR-03: the question landed in the bound thread",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-04 — the comment, and who it is from (AUC-FR-04, AUC-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn appends_one_comment_as_the_agent_participant() {
    let fixture = Fixture::new();
    let output = fixture
        .call(args(
            "Should graduation write one spec or two?",
            Some(vec![
                "one — a single spec covering both layers",
                "two — paired ui/ and core/ specs",
            ]),
        ))
        .expect("posted");
    assert_eq!(output, AskUserCommentOutput { posted: true });

    let thread = fixture.thread();
    assert_eq!(thread.comments.len(), 2);
    let question = thread.comments.last().unwrap();
    match &question.author {
        Participant::Agent {
            agent_id,
            handle,
            model,
            title,
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(handle, "arch", "AUC-FR-04: the nickname is the handle");
            assert_eq!(model.as_deref(), Some("m"));
            // AUC-FR-04 / CMS-FR-65: the turn's snapshot, carried onto the
            // question unchanged — the same participant a delivered answer
            // would have been appended with.
            assert_eq!(title.as_deref(), Some("Developer"));
        }
        other => panic!("AUC-FR-04: expected an agent participant, got {other:?}"),
    }
    assert_eq!(
        question.body,
        "Should graduation write one spec or two?\n\n\
         - one — a single spec covering both layers\n\
         - two — paired ui/ and core/ specs\n\n\
         Or say what you'd rather do.",
        "AUC-FR-05: the question, the options one to a line, and the closing line",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-05 — the comment reads as prose (AUC-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn the_comment_carries_no_marker_or_identifier() {
    let fixture = Fixture::new();
    fixture
        .call(args("one or two?", Some(vec!["one", "two"])))
        .expect("posted");
    let body = fixture.thread().comments.last().unwrap().body.clone();

    // Nothing machine-shaped, nothing to quote back, and no instruction about
    // the form a reply must take beyond the one invitation.
    for marker in [
        "ask_user_comment",
        "<!--",
        "```",
        "option_id",
        "questionId",
        &fixture.thread_id,
        "reply with",
        "Reply with",
        "[1]",
        "AUC-",
    ] {
        assert!(
            !body.contains(marker),
            "AUC-FR-05: the body carries no `{marker}` — {body:?}",
        );
    }
    // The closing line is the only sentence the tool contributed.
    assert_eq!(
        body.matches(CLOSING_LINE).count(),
        1,
        "AUC-FR-05: one closing line and no other prose of the tool's own",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-06 — a blank question (AUC-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_blank_question_is_refused_and_appends_nothing() {
    for question in ["", "   ", "\n\t "] {
        let fixture = Fixture::new();
        let refusal = fixture.call(args(question, None)).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::InvalidArguments(QUESTION_BLANK));
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable(), "AUC-FR-06: retryable");
        assert_eq!(
            fixture.thread().comments.len(),
            1,
            "AUC-FR-06: nothing was appended",
        );
        assert!(!fixture.asked.load(Ordering::SeqCst));
    }
}

// ---------------------------------------------------------------------------
// AUC-FR-07 — options are optional and forgiving (AUC-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn absent_or_blank_options_leave_the_question_alone() {
    for options in [None, Some(vec![]), Some(vec!["", "  "])] {
        let fixture = Fixture::new();
        fixture.call(args("which way?", options)).expect("posted");
        assert_eq!(
            fixture.thread().comments.last().unwrap().body,
            "which way?",
            "AUC-FR-07: no list and no closing line under an open question",
        );
    }
}

#[test]
fn blank_entries_are_dropped_and_the_rest_kept_in_order() {
    let fixture = Fixture::new();
    fixture
        .call(args("which?", Some(vec!["a", "", "b"])))
        .expect("posted");
    assert_eq!(
        fixture.thread().comments.last().unwrap().body,
        format!("which?\n\n- a\n- b\n\n{CLOSING_LINE}"),
    );
}

#[test]
fn a_long_option_list_is_neither_refused_nor_truncated() {
    let fixture = Fixture::new();
    let options: Vec<&str> = vec!["a", "b", "c", "d", "e", "f"];
    fixture
        .call(args("which?", Some(options.clone())))
        .expect("posted");
    let body = fixture.thread().comments.last().unwrap().body.clone();
    for option in options {
        assert!(
            body.contains(&format!("- {option}\n")),
            "AUC-FR-07: `{option}` survived",
        );
    }
}

#[test]
fn a_bare_string_of_options_is_one_option() {
    let decoded: AskUserCommentArgs =
        serde_json::from_value(serde_json::json!({ "question": "q", "options": "only" }))
            .expect("decoded");
    assert_eq!(decoded.options, Some(vec!["only".to_string()]));

    // TLC-FR-07: an unusable shape falls back to the default rather than losing
    // a perfectly good question.
    for shape in [
        serde_json::json!({ "question": "q", "options": 7 }),
        serde_json::json!({ "question": "q", "options": null }),
        serde_json::json!({ "question": "q" }),
    ] {
        let decoded: AskUserCommentArgs = serde_json::from_value(shape).expect("decoded");
        assert_eq!(decoded.options, None);
    }

    // An unrecognised field is ignored (TLC-FR-07).
    let decoded: AskUserCommentArgs =
        serde_json::from_value(serde_json::json!({ "question": "q", "urgency": "high" }))
            .expect("decoded");
    assert_eq!(decoded.question, "q");
}

// ---------------------------------------------------------------------------
// AUC-FR-08 — it posts and returns (AUC-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn the_call_resolves_without_being_polled() {
    let fixture = Fixture::new();
    // `std::future::ready` is already complete, so the work happened before any
    // executor saw it: the tool waits for nothing.
    use std::future::Future as _;
    let tool = fixture.tool();
    let mut future = Box::pin(tool.call(args("which way?", None)));
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(result) => {
            result.expect("posted");
        }
        std::task::Poll::Pending => panic!("AUC-FR-08: the tool waited for something"),
    }
    assert_eq!(fixture.thread().comments.len(), 2);
}

// ---------------------------------------------------------------------------
// AUC-FR-09 — the output is data (AUC-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn the_output_is_a_json_object_with_a_named_field() {
    let value = serde_json::to_value(AskUserCommentOutput { posted: true }).expect("serialized");
    assert_eq!(value, serde_json::json!({ "posted": true }));
    assert!(
        value.is_object(),
        "TLC-FR-08: a JSON object, never a bare array and never a prose sentence",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-10 — a locked conversation (AUC-FR-10, AUC-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn a_locked_conversation_refuses_unretryably_and_appends_nothing() {
    let fixture = Fixture::new();
    fixture.lock();
    let refusal = fixture.call(args("which way?", None)).expect_err("refused");

    assert_eq!(refusal, ToolRefusal::ConversationLocked);
    assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
    assert!(
        !refusal.retryable(),
        "AUC-FR-10: no argument unlocks a conversation",
    );
    let message = refusal.to_string();
    assert!(
        message.contains("will not succeed"),
        "AUC-FR-10: the message says so as well as the flag — {message:?}",
    );
    assert_eq!(fixture.thread().comments.len(), 1);
    assert!(
        !fixture.asked.load(Ordering::SeqCst),
        "AUC-FR-12: a refusal does not end the turn",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-11 — any other append failure (AUC-FR-11, AUC-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn an_append_that_fails_otherwise_is_a_retryable_other() {
    let fixture = Fixture::new();
    // A conversation that is not there: the thread cannot be located, which is
    // every append failure that is not a lock.
    let tool = AskUserCommentTool::new(
        fixture.handle(),
        crate::agent_conversations::OwnedRoots { worktree: fixture.root(), store: fixture.root() },
        ConversationOrigin::stub_artifact("thread-that-is-not-there", "a.md", true),
        agent(),
        Arc::clone(&fixture.asked),
    );
    let refusal = block_on(tool.call(args("which way?", None))).expect_err("refused");

    assert_eq!(refusal, ToolRefusal::QuestionNotPosted);
    assert_eq!(refusal.kind(), ToolErrorKind::Other);
    assert!(refusal.retryable(), "AUC-FR-11");
    assert_eq!(
        fixture.thread().comments.len(),
        1,
        "AUC-FR-11: the conversation is byte-for-byte what it was",
    );
    assert!(!fixture.asked.load(Ordering::SeqCst), "AUC-FR-12");
}

// ---------------------------------------------------------------------------
// AUC-FR-13 — no project open (AUC-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn with_no_project_open_the_shared_refusal_is_produced() {
    // The root a turn was built with outlives the project being open: a turn can
    // run for as long as its timeout allows, and the project can close under it.
    // Without this gate a question would be appended into a checkout the
    // application has stopped showing.
    let fixture = Fixture::new();
    let closed = closed_project();
    let tool = AskUserCommentTool::new(
        closed.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: fixture.root(), store: fixture.root() },
        fixture.origin(),
        agent(),
        Arc::clone(&fixture.asked),
    );

    let refusal = block_on(tool.call(args("which way?", None))).expect_err("refused");
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(
        !refusal.retryable(),
        "AUC-FR-13: no argument opens a project",
    );
    assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
    assert_eq!(
        fixture.thread().comments.len(),
        1,
        "AUC-FR-13: nothing was appended",
    );
    assert!(!fixture.asked.load(Ordering::SeqCst));

    // The gate runs before the argument check, so a blank question with no
    // project reads as the project problem rather than the argument one.
    let blank = block_on(tool.call(args("   ", None))).expect_err("refused");
    assert_eq!(blank, ToolRefusal::NoProjectOpen);
}

// ---------------------------------------------------------------------------
// AUC-FR-15 — the question carries no attachment (AUC-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn the_question_carries_no_attachment() {
    let fixture = Fixture::new();
    fixture.call(args("which way?", None)).expect("posted");
    assert!(
        fixture
            .thread()
            .comments
            .last()
            .unwrap()
            .attachments
            .is_empty(),
        "AUC-FR-14: an agent contributes prose and never a file",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-16 — two turns, two instances (AUC-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn two_turns_hold_their_own_instances_and_neither_sees_the_other() {
    let fixture = Fixture::new();
    let arch_asked = Arc::new(AtomicBool::new(false));
    let sec_asked = Arc::new(AtomicBool::new(false));

    let arch = AskUserCommentTool::new(
        fixture.handle(),
        crate::agent_conversations::OwnedRoots { worktree: fixture.root(), store: fixture.root() },
        fixture.origin(),
        agent(),
        Arc::clone(&arch_asked),
    );
    let sec = AskUserCommentTool::new(
        fixture.handle(),
        crate::agent_conversations::OwnedRoots { worktree: fixture.root(), store: fixture.root() },
        fixture.origin(),
        Participant::Agent {
            agent_id: "agent-2".into(),
            handle: "sec".into(),
            model: Some("m".into()),
            title: None,
        },
        Arc::clone(&sec_asked),
    );

    block_on(arch.call(args("arch asks", None))).expect("posted");
    assert!(arch_asked.load(Ordering::SeqCst));
    assert!(
        !sec_asked.load(Ordering::SeqCst),
        "AUC-FR-16: neither instance observes the other's call",
    );

    block_on(sec.call(args("sec asks", None))).expect("posted");

    let thread = fixture.thread();
    assert_eq!(thread.comments.len(), 3);
    let handles: Vec<&str> = thread
        .comments
        .iter()
        .map(|c| match &c.author {
            Participant::Agent { handle, .. } => handle.as_str(),
            Participant::Human { login, .. } => login.as_str(),
        })
        .collect();
    assert_eq!(
        handles,
        vec!["raver119", "arch", "sec"],
        "AUC-FR-16: each question is attributed to its own agent",
    );
}

// ---------------------------------------------------------------------------
// AUC-FR-17 — what a call reports (AUC-FR-17)
// ---------------------------------------------------------------------------

static AUC_LOG: LogBuffer = LogBuffer::new();

#[test]
fn a_call_reports_the_tool_and_the_conversation_and_nothing_it_composed() {
    AUC_LOG.clear();
    let fixture = Fixture::new();

    fixture
        .reporting_into(&AUC_LOG)
        .run(args(
            "Should graduation write one spec or two?",
            Some(vec!["one spec", "two specs"]),
        ))
        .expect("posted");
    // The refusal path where `args.question` is in hand at the emit site.
    fixture
        .reporting_into(&AUC_LOG)
        .run(args("   ", None))
        .expect_err("refused");
    fixture.lock();
    fixture
        .reporting_into(&AUC_LOG)
        .run(args("and the archive part?", None))
        .expect_err("refused");

    // The same two records under either domain (TLC-FR-14).
    for domain in [Domain::Ai, Domain::Backend] {
        let records = records_of(&AUC_LOG, domain);
        assert_eq!(records.len(), 3, "AUC-FR-17: one INFO and two WARNs");
        assert_eq!(records[0].level, LogLevel::Info);
        assert_eq!(records[1].level, LogLevel::Warn);
        assert_eq!(records[2].level, LogLevel::Warn);
        // Pinned by key SET rather than by the absence of a few strings: a
        // record that gained `questionChars`, `optionCount`, or a truncated
        // prefix of the question would slip past a substring scan, and each of
        // those is a leak of what the model composed.
        assert_eq!(
            sorted_keys(&records[0]),
            vec!["discussionId", "originKind", "tool"],
            "AUC-FR-17: the success record names the tool and the conversation",
        );
        for refusal in &records[1..] {
            assert_eq!(
                sorted_keys(refusal),
                vec!["discussionId", "originKind", "reason", "retryable", "tool"],
                "AUC-FR-17: and a refusal adds the reason and nothing else",
            );
        }
        for record in &records {
            assert_eq!(record.fields["tool"], serde_json::json!(NAME));
            assert_eq!(
                record.fields["discussionId"],
                serde_json::json!(fixture.thread_id),
                "AUC-FR-17: a record that cannot be tied to a conversation cannot be followed",
            );
            assert_eq!(
                record.fields["originKind"],
                serde_json::json!("artifact_comment"),
            );
        }
        assert_eq!(
            records[1].fields["reason"],
            serde_json::json!("invalid_arguments"),
        );
        assert_eq!(
            records[2].fields["reason"],
            serde_json::json!("conversation_locked"),
        );
    }

    // AUC-FR-17: none of it names what the model composed.
    let rendered = rendered_records(&AUC_LOG);
    for composed in [
        "Should graduation write one spec or two?",
        "one spec",
        "two specs",
        "and the archive part?",
        CLOSING_LINE,
        // The seed comment, which is the conversation itself.
        "@arch which way?",
    ] {
        assert!(
            !rendered.contains(composed),
            "AUC-FR-17: `{composed}` is material the model composed and is not loggable",
        );
    }
}

// ---------------------------------------------------------------------------
// AUC-FR-01 — the declared side effect is the whole of it (AUC-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn nothing_changes_but_the_conversations_own_log() {
    let fixture = Fixture::new();
    let before = crate::tools::tests::tree_snapshot(fixture.path.as_path());
    fixture.call(args("which way?", None)).expect("posted");
    let after = crate::tools::tests::tree_snapshot(fixture.path.as_path());

    // Both directions: an entry of `after` missing from `before` is a create or
    // a modify, and one of `before` missing from `after` is a DELETE — which a
    // one-way comparison cannot see at all, though TLC-FR-17 names it.
    let differs = |a: &[(String, Vec<u8>)], b: &[(String, Vec<u8>)]| -> Vec<String> {
        a.iter()
            .filter(|(path, bytes)| !b.iter().any(|(p, x)| p == path && x == bytes))
            .map(|(path, _)| path.clone())
            .collect()
    };
    let changed = differs(&after, &before);
    assert!(
        differs(&before, &after)
            .iter()
            .all(|path| changed.contains(path)),
        "AUC-FR-01: nothing was deleted",
    );
    assert_eq!(
        changed.len(),
        1,
        "AUC-FR-01: exactly one file changed — {changed:?}",
    );
    assert!(
        changed[0].starts_with("comments/") && changed[0].ends_with(".jsonl"),
        "AUC-FR-01: and it is the conversation's own log — {changed:?}",
    );
    // The artifact itself is untouched.
    assert_eq!(
        std::fs::read_to_string(fixture.path.as_path().join(ARTIFACT)).unwrap(),
        "one two three four five",
    );
}
