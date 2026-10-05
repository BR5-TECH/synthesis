//! The tool's validation and its contract surface (ADQ-FR-PVXK … ADQ-FR-ZBQH).
//!
//! The reservation itself is covered where it lives, in
//! `crate::comments::tests::question_sets`, which reaches a store without a
//! Tauri runtime.

use super::*;

fn asked(question: &str, options: &[&str]) -> AskedQuestion {
    AskedQuestion {
        question: question.into(),
        options: options.iter().map(|o| (*o).to_string()).collect(),
    }
}

/// ADQ-FR-PVXK, ADQ-FR-TWNS: a valid set normalizes with positions assigned in
/// the order the model wrote them, ascending from one.
#[test]
fn a_valid_set_takes_positions_in_the_order_it_was_written() {
    let questions = normalize(&[
        asked("first?", &["a", "b"]),
        asked("second?", &["c", "d", "e"]),
    ])
    .unwrap();

    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0].position, 1);
    assert_eq!(questions[0].text, "first?");
    assert_eq!(questions[1].position, 2);
    assert_eq!(
        questions[1]
            .options
            .iter()
            .map(|o| (o.position, o.value.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "c"), (2, "d"), (3, "e")],
    );
}

/// ADQ-FR-PVXK: an empty list is refused, retryably.
#[test]
fn an_empty_set_is_refused() {
    let refusal = normalize(&[]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidArguments(_)));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::InvalidArgs);
    assert_eq!(refusal.retryable(), true);
}

/// ADQ-FR-PVXK: more than ten questions is refused, and ten is accepted.
#[test]
fn more_than_ten_questions_is_refused() {
    let many: Vec<AskedQuestion> = (0..11).map(|_| asked("q?", &["a", "b"])).collect();
    assert!(matches!(
        normalize(&many).unwrap_err(),
        ToolRefusal::InvalidArguments(_),
    ));
    let ten: Vec<AskedQuestion> = (0..10).map(|_| asked("q?", &["a", "b"])).collect();
    assert_eq!(normalize(&ten).unwrap().len(), 10);
}

/// ADQ-FR-PVXK, ADQ-FR-CJRM: blank question text refuses the whole call and
/// names the question at fault, so the model corrects one rather than guessing.
#[test]
fn a_blank_question_refuses_the_whole_call_and_names_its_position() {
    let refusal = normalize(&[
        asked("first?", &["a", "b"]),
        asked("   ", &["a", "b"]),
        asked("third?", &["a", "b"]),
    ])
    .unwrap_err();

    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(_, 2)));
    assert!(
        refusal.to_string().contains("question 2 of the set"),
        "{refusal}",
    );
}

/// ADQ-FR-PVXK, ADQ-FR-CJRM: an option count outside two or three refuses the
/// whole call; two and three are the accepted boundaries.
#[test]
fn an_option_count_outside_two_or_three_is_refused() {
    for options in [vec!["only"], vec!["a", "b", "c", "d"], vec![]] {
        let refusal = normalize(&[asked("q?", &options)]).unwrap_err();
        assert!(
            matches!(refusal, ToolRefusal::InvalidQuestion(WRONG_OPTION_COUNT, 1)),
            "{options:?} was accepted",
        );
    }
    assert!(normalize(&[asked("q?", &["a", "b"])]).is_ok());
    assert!(normalize(&[asked("q?", &["a", "b", "c"])]).is_ok());
}

/// ADQ-FR-PVXK, ADQ-FR-CJRM: a blank option refuses the whole call rather than
/// being dropped, so the author never sees a set smaller than was meant.
#[test]
fn a_blank_option_refuses_the_whole_call() {
    let refusal = normalize(&[asked("q?", &["a", "  "])]).unwrap_err();
    assert!(matches!(
        refusal,
        ToolRefusal::InvalidQuestion(SET_OPTION_BLANK, 1),
    ));
}

/// ADQ-FR-ZBQH: a question above its named limit is refused, and the limit
/// itself is accepted.
#[test]
fn a_question_above_its_limit_is_refused() {
    let at_limit = "x".repeat(QUESTION_LIMIT);
    assert!(normalize(&[asked(&at_limit, &["a", "b"])]).is_ok());

    let over = "x".repeat(QUESTION_LIMIT + 1);
    assert!(matches!(
        normalize(&[asked(&over, &["a", "b"])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_QUESTION_TOO_LONG, 1),
    ));
}

/// ADQ-FR-ZBQH: an option above its named limit is refused.
#[test]
fn an_option_above_its_limit_is_refused() {
    let at_limit = "x".repeat(OPTION_LIMIT);
    assert!(normalize(&[asked("q?", &["a", &at_limit])]).is_ok());

    let over = "x".repeat(OPTION_LIMIT + 1);
    assert!(matches!(
        normalize(&[asked("q?", &["a", &over])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_OPTION_TOO_LONG, 1),
    ));
}

/// ADQ-FR-QNJU, ADQ-FR-RWTP: the already-pending refusal is `PermissionDenied`
/// and says a further call will not succeed, no argument freeing a slot the
/// author holds.
#[test]
fn the_already_pending_refusal_is_not_retryable() {
    let refusal = refusal_for(crate::comments::ERR_QUESTION_SET_ALREADY_PENDING);
    assert!(matches!(refusal, ToolRefusal::QuestionSetAlreadyPending));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::PermissionDenied);
    assert_eq!(refusal.retryable(), false);
    assert!(refusal.to_string().contains("will not succeed"));
}

/// ADQ-FR-VDGT: a locked discussion is the not-retryable `PermissionDenied`.
#[test]
fn the_locked_refusal_is_not_retryable() {
    let refusal = refusal_for(crate::comments::ERR_DISCUSSION_LOCKED);
    assert!(matches!(refusal, ToolRefusal::QuestionSetConversationLocked));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::PermissionDenied);
    assert_eq!(refusal.retryable(), false);
}

/// ADQ-FR-HBLN: any other failure is the retryable `Other`, and nothing was
/// recorded.
#[test]
fn any_other_failure_is_the_retryable_other_refusal() {
    let refusal = refusal_for("some disk problem");
    assert!(matches!(refusal, ToolRefusal::QuestionSetNotRecorded));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(refusal.retryable(), true);
}

/// ADQ-FR-WBQL: the parameter schema declares the one documented argument with
/// the documented descriptions and the documented bounds.
///
/// ADQ-FR-HTGC: and nothing else. No argument names a conversation, a thread, a
/// target, or a participant — its absence is the containment.
#[test]
fn the_parameter_schema_declares_the_documented_argument_and_no_other() {
    let schema = parameters();
    let questions = &schema["properties"]["questions"];
    assert_eq!(questions["description"], QUESTIONS_DESCRIPTION);
    assert_eq!(questions["minItems"], 1);
    assert_eq!(questions["maxItems"], 10);

    let item = &questions["items"];
    assert_eq!(
        item["properties"]["question"]["description"],
        QUESTION_DESCRIPTION,
    );
    assert_eq!(
        item["properties"]["options"]["description"],
        OPTIONS_DESCRIPTION,
    );
    assert_eq!(item["properties"]["options"]["minItems"], 2);
    assert_eq!(item["properties"]["options"]["maxItems"], 3);

    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 1, "{properties:?}");
}

/// ADQ-FR-WBQL: the description tells this tool from `ask_user_comment` on its
/// first sentence, and names that tool as the one for an open question.
#[test]
fn the_description_separates_it_from_the_single_question_tool() {
    assert!(DESCRIPTION.starts_with("Put several questions to the author at once"));
    assert!(DESCRIPTION.contains("ask_user_comment"));
    // ADQ-FR-FQPA: it says the turn ends here, so the model does not expect an
    // answer back inside this call.
    assert!(DESCRIPTION.contains("your turn ends there"));
}

// ---------------------------------------------------------------------------
// The call itself (ADQ-FR-GMDK, ADQ-FR-RMBC, ADQ-FR-LZHV, ADQ-FR-KOWX)
// ---------------------------------------------------------------------------

use rig::tool::PortableTool;
use tempfile::TempDir;

use crate::comments::Participant;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted, sorted_keys};

static ADQ_LOG: LogBuffer = LogBuffer::new();

fn human() -> Participant {
    Participant::Human {
        login: "raver119".into(),
        display_name: None,
        email: None,
    }
}

/// The agent the set is recorded as (ADQ-FR-GMDK).
fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: Some("Architect".into()),
    }
}

/// A project holding one draft with one discussion on it.
struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    recorded: std::sync::Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().to_path_buf();
        let root = crate::fs::RootFs::for_root(&path);
        let draft_id = crate::drafts::create_draft_at_root(&root, Some("ontology"))
            .expect("draft")
            .draft
            .id;
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &crate::comments::DiscussionTarget::Draft {
                draft_id: draft_id.clone(),
            },
            None,
            "@arch which way?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            draft_id,
            thread_id: thread.id,
            recorded: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn roots(&self) -> crate::agent_conversations::OwnedRoots {
        crate::agent_conversations::OwnedRoots {
            worktree: self.root(),
            store: self.root(),
        }
    }

    fn tool(&self) -> AskDiscussionQuestionsTool<tauri::test::MockRuntime> {
        AskDiscussionQuestionsTool::new(
            self.open.handle(),
            self.roots(),
            ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false),
            agent(),
            std::sync::Arc::clone(&self.recorded),
        )
    }

    fn reporting_into(
        &self,
        buffer: &'static LogBuffer,
    ) -> AskDiscussionQuestionsTool<tauri::test::MockRuntime> {
        self.tool().with_buffer(buffer)
    }

    fn set(&self) -> Option<crate::comments::PendingQuestionSet> {
        crate::comments::read_question_set(&self.root(), &self.root(), &self.thread_id)
    }
}

fn args(questions: Vec<AskedQuestion>) -> AskDiscussionQuestionsArgs {
    AskDiscussionQuestionsArgs { questions }
}

fn two_questions() -> Vec<AskedQuestion> {
    vec![asked("first?", &["a", "b"]), asked("second?", &["c", "d"])]
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

/// ADQ-FR-GMDK, ADQ-FR-TWNS: a successful call records one set carrying a fresh
/// id, the turn's agent, and every question in the order given.
#[test]
fn a_successful_call_records_the_set_against_the_bound_discussion() {
    let fixture = Fixture::new();
    let output = block_on(fixture.tool().call(args(two_questions()))).expect("recorded");
    assert_eq!(output, AskDiscussionQuestionsOutput { recorded: true });

    let set = fixture.set().expect("a set stands");
    assert_eq!(set.discussion_id, fixture.thread_id, "ADQ-FR-HTGC: the bound discussion");
    assert_eq!(set.asked_by, agent(), "CMS-FR-41: the turn's own participant");
    assert!(!set.set_id.is_empty());
    assert_eq!(
        set.questions
            .iter()
            .map(|q| (q.position, q.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "first?"), (2, "second?")],
    );
    // CVL-FR-15: and the loop is told the set actually went out.
    assert!(fixture.recorded.load(Ordering::SeqCst));
}

/// ADQ-FR-DHZK: a successful call appends **no comment**.
#[test]
fn a_successful_call_appends_no_comment() {
    let fixture = Fixture::new();
    let before = crate::comments::fold_discussion(&fixture.root(), &fixture.draft_id)
        .into_iter()
        .find(|t| t.id == fixture.thread_id)
        .expect("thread");
    block_on(fixture.tool().call(args(two_questions()))).expect("recorded");
    let after = crate::comments::fold_discussion(&fixture.root(), &fixture.draft_id)
        .into_iter()
        .find(|t| t.id == fixture.thread_id)
        .expect("thread");
    assert_eq!(before, after, "the discussion's log is what it was");
}

/// ADQ-FR-QNJU: a second call is refused and the standing set is untouched.
#[test]
fn a_second_call_is_refused_and_the_standing_set_is_unchanged() {
    let fixture = Fixture::new();
    block_on(fixture.tool().call(args(two_questions()))).expect("recorded");
    let standing = fixture.set().expect("a set stands");

    let refusal = block_on(fixture.tool().call(args(vec![asked("third?", &["e", "f"])])))
        .expect_err("refused");
    assert!(matches!(refusal, ToolRefusal::QuestionSetAlreadyPending));
    assert_eq!(fixture.set().as_ref(), Some(&standing));
}

/// ADQ-FR-CJRM: a refused call records nothing at all.
#[test]
fn a_refused_call_records_nothing() {
    let fixture = Fixture::new();
    block_on(fixture.tool().call(args(vec![asked("only one option?", &["a"])])))
        .expect_err("refused");
    assert!(fixture.set().is_none());
    assert!(!fixture.recorded.load(Ordering::SeqCst));
}

/// ADQ-FR-KOWX: with no project open the tool gives the shared refusal.
#[test]
fn a_closed_project_is_the_shared_refusal() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().to_path_buf();
    let open = closed_project();
    let tool = AskDiscussionQuestionsTool::new(
        open.handle().clone(),
        crate::agent_conversations::OwnedRoots {
            worktree: crate::fs::RootFs::for_root(&path),
            store: crate::fs::RootFs::for_root(&path),
        },
        ConversationOrigin::stub_draft("t", "d", false),
        agent(),
        std::sync::Arc::new(AtomicBool::new(false)),
    );
    let refusal = block_on(tool.call(args(two_questions()))).expect_err("refused");
    assert!(matches!(refusal, ToolRefusal::NoProjectOpen));
}

/// ADQ-FR-RMBC, ADQ-FR-LZHV: one INFO when a set is recorded and one WARN when
/// it refuses, naming the tool, the discussion, and the question **count** — and
/// nothing the model composed.
#[test]
fn a_call_reports_the_tool_the_discussion_and_the_count_and_nothing_it_composed() {
    ADQ_LOG.clear();
    let fixture = Fixture::new();

    block_on(
        fixture
            .reporting_into(&ADQ_LOG)
            .call(args(vec![
                asked("Should the ontology live in one spec or two?", &["one spec", "two specs"]),
                asked("Where does the flow diagram go?", &["the ui spec", "the core spec"]),
            ])),
    )
    .expect("recorded");
    // The refusal path where the arguments are in hand at the emit site.
    block_on(
        fixture
            .reporting_into(&ADQ_LOG)
            .call(args(vec![asked("and the archive part?", &["keep", "drop"])])),
    )
    .expect_err("refused");

    for domain in [Domain::Ai, Domain::Backend] {
        let records = records_of(&ADQ_LOG, domain);
        assert_eq!(records.len(), 2, "ADQ-FR-RMBC: one INFO and one WARN");
        assert_eq!(records[0].level, LogLevel::Info);
        assert_eq!(records[1].level, LogLevel::Warn);
        // Pinned by key SET rather than by the absence of a few strings: a
        // record that gained `questionChars` or a truncated prefix would slip
        // past a substring scan, and each of those is a leak of what the model
        // composed (ADQ-FR-LZHV).
        assert_eq!(
            sorted_keys(&records[0]),
            vec!["discussionId", "originKind", "questions", "tool"],
        );
        assert_eq!(
            sorted_keys(&records[1]),
            vec!["discussionId", "originKind", "questions", "reason", "retryable", "tool"],
        );
    }

    // And no record anywhere carries a question, an option, or a body.
    let rendered = format!("{:?}", records_of(&ADQ_LOG, Domain::Ai));
    for composed in [
        "ontology",
        "one spec",
        "two specs",
        "flow diagram",
        "the ui spec",
        "archive",
        "keep",
    ] {
        assert!(
            !rendered.contains(composed),
            "ADQ-FR-LZHV: a record carried {composed:?}",
        );
    }
}
