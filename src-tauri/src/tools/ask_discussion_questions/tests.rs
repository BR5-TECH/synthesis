//! The tool's validation and its contract surface (ADQ-FR-PVXK … ADQ-FR-ZBQH).
//!
//! The reservation itself is covered where it lives, in
//! `crate::comments::tests::question_sets`, which reaches a store without a
//! Tauri runtime.

use super::*;

/// A question whose options each carry the given label and a description
/// derived from it.
fn asked(question: &str, labels: &[&str]) -> AskedQuestion {
    AskedQuestion {
        question: question.into(),
        options: labels.iter().map(|label| option(label, &format!("Why {label}."))).collect(),
    }
}

fn option(label: &str, description: &str) -> AskedOption {
    AskedOption { label: label.into(), description: description.into() }
}

fn asked_with(options: Vec<AskedOption>) -> AskedQuestion {
    AskedQuestion { question: "q?".into(), options }
}

/// ADQ-FR-PVXK, ADQ-FR-TWNS: a valid set normalizes with positions assigned in
/// the order the model wrote them, ascending from one.
#[test]
fn a_valid_set_takes_positions_in_the_order_it_was_written() {
    let questions =
        normalize(&[asked("first?", &["a", "b"]), asked("second?", &["c", "d", "e"])]).unwrap();

    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0].position, 1);
    assert_eq!(questions[0].text, "first?");
    assert_eq!(questions[1].position, 2);
    assert_eq!(
        questions[1].options.iter().map(|o| (o.position, o.value.as_str())).collect::<Vec<_>>(),
        vec![(1, "c"), (2, "d"), (3, "e")],
    );
}

/// ADQ-FR-PVXK: an empty list is refused, retryably.
#[test]
fn an_empty_set_is_refused() {
    let refusal = normalize(&[]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidArguments(_)));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable());
}

/// ADQ-FR-PVXK: more than ten questions is refused, and ten is accepted.
#[test]
fn more_than_ten_questions_is_refused() {
    let many: Vec<AskedQuestion> = (0..11).map(|_| asked("q?", &["a", "b"])).collect();
    assert!(matches!(normalize(&many).unwrap_err(), ToolRefusal::InvalidArguments(_),));
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
    assert!(refusal.to_string().contains("question 2 of the set"), "{refusal}",);
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
    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_BLANK, 1),));
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

/// ADQ-FR-GMDK, ADQ-FR-PZWD: each recorded option carries its trimmed label as
/// its value and its trimmed description.
#[test]
fn each_option_records_its_trimmed_label_and_description() {
    let questions = normalize(&[asked_with(vec![
        option("  One spec ", "  Easy to find; it grows large.  "),
        option("Paired specs", "Each stays small."),
    ])])
    .unwrap();

    let first = &questions[0].options[0];
    assert_eq!(first.value, "One spec");
    assert_eq!(first.description.as_deref(), Some("Easy to find; it grows large."));
    assert_eq!(questions[0].options[1].description.as_deref(), Some("Each stays small."));
}

/// ADQ-FR-PVXK, ADQ-FR-CJRM: a blank or missing description refuses the whole
/// call, retryably, and names the question at fault.
#[test]
fn a_blank_or_missing_description_refuses_the_whole_call() {
    for description in ["", "   \n\t"] {
        let refusal = normalize(&[
            asked("first?", &["a", "b"]),
            asked_with(vec![option("a", "Fine."), option("b", description)]),
        ])
        .unwrap_err();
        assert!(
            matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_DESCRIPTION_BLANK, 2)),
            "{description:?}: {refusal:?}",
        );
        assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
    }

    // A description the model left out entirely decodes as blank rather than
    // failing the decode, so it reaches the same refusal.
    let args: AskDiscussionQuestionsArgs = serde_json::from_value(serde_json::json!({
        "questions": [{ "question": "q?", "options": [
            { "label": "a", "description": "Fine." },
            { "label": "b" },
        ] }],
    }))
    .unwrap();
    assert!(matches!(
        normalize(&args.questions).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_OPTION_DESCRIPTION_BLANK, 1),
    ));
}

/// ADQ-FR-LGKM: a label of more than five words is refused; five is accepted.
#[test]
fn a_label_of_more_than_five_words_is_refused() {
    let five = asked_with(vec![option("one two three four five", "d"), option("b", "d")]);
    assert!(normalize(&[five]).is_ok());

    let six = asked_with(vec![option("a", "d"), option("one two  three\tfour five six", "d")]);
    let refusal = normalize(&[six]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(SET_LABEL_TOO_MANY_WORDS, 1)));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable());
}

/// ADQ-FR-ZBQH: a label above 40 characters is refused and 40 is accepted. The
/// limit counts characters, so 40 multibyte characters are accepted too.
#[test]
fn a_label_above_its_character_limit_is_refused() {
    let at_limit = "x".repeat(LABEL_CHAR_LIMIT);
    assert!(normalize(&[asked("q?", &["a", &at_limit])]).is_ok());
    let multibyte = "é".repeat(LABEL_CHAR_LIMIT);
    assert!(normalize(&[asked("q?", &["a", &multibyte])]).is_ok());

    let over = "x".repeat(LABEL_CHAR_LIMIT + 1);
    let refusal = normalize(&[asked("q?", &["a", &over])]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_TOO_LONG, 1)));
    assert!(refusal.retryable());
}

/// ADQ-FR-ZBQH: a description above 300 characters is refused and 300 is
/// accepted, counted in characters rather than bytes.
#[test]
fn a_description_above_its_character_limit_is_refused() {
    let at_limit = "é".repeat(DESCRIPTION_CHAR_LIMIT);
    assert!(normalize(&[asked_with(vec![option("a", &at_limit), option("b", "d")])]).is_ok());

    let over = "x".repeat(DESCRIPTION_CHAR_LIMIT + 1);
    let refusal = normalize(&[asked_with(vec![option("a", "d"), option("b", &over)])]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_DESCRIPTION_TOO_LONG, 1)));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable());
}

/// ADQ-FR-QNJU, ADQ-FR-RWTP: the already-pending refusal is `PermissionDenied`
/// and says a further call will not succeed, no argument freeing a slot the
/// author holds.
#[test]
fn the_already_pending_refusal_is_not_retryable() {
    let refusal = refusal_for(crate::comments::ERR_QUESTION_SET_ALREADY_PENDING);
    assert!(matches!(refusal, ToolRefusal::QuestionSetAlreadyPending));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable());
    assert!(refusal.to_string().contains("will not succeed"));
}

/// ADQ-FR-VDGT: a resolved discussion is the not-retryable `PermissionDenied`.
#[test]
fn the_resolved_refusal_is_not_retryable() {
    let refusal = refusal_for(crate::comments::ERR_DISCUSSION_RESOLVED);
    assert!(matches!(refusal, ToolRefusal::QuestionSetConversationResolved));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::PermissionDenied);
    assert!(!refusal.retryable());
}

/// ADQ-FR-HBLN: any other failure is the retryable `Other`, and nothing was
/// recorded.
#[test]
fn any_other_failure_is_the_retryable_other_refusal() {
    let refusal = refusal_for("some disk problem");
    assert!(matches!(refusal, ToolRefusal::QuestionSetNotRecorded));
    assert_eq!(refusal.kind(), rig::tool::ToolErrorKind::Other);
    assert!(refusal.retryable());
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
    assert_eq!(item["properties"]["question"]["description"], QUESTION_DESCRIPTION,);
    assert_eq!(item["properties"]["options"]["description"], OPTIONS_DESCRIPTION,);
    assert_eq!(item["properties"]["options"]["minItems"], 2);
    assert_eq!(item["properties"]["options"]["maxItems"], 3);

    // ADQ-FR-PVXK: each option is a label and a description, both required.
    let option = &item["properties"]["options"]["items"];
    assert_eq!(option["type"], "object");
    assert_eq!(option["properties"]["label"]["description"], LABEL_DESCRIPTION);
    assert_eq!(option["properties"]["description"]["description"], OPTION_DESCRIPTION_DESCRIPTION);
    assert_eq!(option["required"], serde_json::json!(["label", "description"]));

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

/// ADQ-FR-ZBQH, ADQ-FR-LGKM: the limits count the trimmed text, so whitespace
/// around a value at its limit costs it nothing.
#[test]
fn the_limits_count_the_trimmed_text() {
    let label = format!("  {}\t", "x".repeat(LABEL_CHAR_LIMIT));
    let description = format!("\n {} ", "d".repeat(DESCRIPTION_CHAR_LIMIT));
    let words = "  one two three four five  ";
    assert!(
        normalize(&[asked_with(vec![option(&label, &description), option(words, "d"),])]).is_ok()
    );
}

/// ADQ-FR-LGKM, ADQ-FR-ZBQH: the word limit is checked before the character
/// limit, so a long label of few words is told it is too long and a label of
/// many words is told to use fewer.
#[test]
fn the_word_limit_is_checked_before_the_character_limit() {
    let long_five = "abcdefghij abcdefghij abcdefghij abcdefghij a";
    assert_eq!(long_five.split_whitespace().count(), 5);
    assert!(long_five.chars().count() > LABEL_CHAR_LIMIT);
    assert!(matches!(
        normalize(&[asked("q?", &["a", long_five])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_OPTION_TOO_LONG, 1),
    ));

    let long_six = "abcdefghij abcdefghij abcdefghij abcdefghij a b";
    assert!(matches!(
        normalize(&[asked("q?", &["a", long_six])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_LABEL_TOO_MANY_WORDS, 1),
    ));
}

/// ADQ-FR-LGKM: any Unicode whitespace separates words, and a label of nothing
/// but whitespace is a blank label.
#[test]
fn any_unicode_whitespace_separates_words() {
    let six = "one\ntwo\u{00A0}three\u{3000}four five six";
    assert!(matches!(
        normalize(&[asked("q?", &["a", six])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_LABEL_TOO_MANY_WORDS, 1),
    ));
    assert!(matches!(
        normalize(&[asked("q?", &["a", "\u{00A0}\u{3000}\n"])]).unwrap_err(),
        ToolRefusal::InvalidQuestion(SET_OPTION_BLANK, 1),
    ));
}

/// ADQ-FR-PVXK, ADQ-FR-ZBQH, ADQ-FR-LGKM: an option refusal names the question
/// it is in, wherever that question stands in the set.
#[test]
fn an_option_refusal_names_the_question_it_is_in() {
    let over = "x".repeat(DESCRIPTION_CHAR_LIMIT + 1);
    for bad in [
        option("one two three four five six", "d"),
        option("a", ""),
        option("a", &over),
        option(&"x".repeat(LABEL_CHAR_LIMIT + 1), "d"),
    ] {
        let refusal = normalize(&[
            asked("first?", &["a", "b"]),
            asked("second?", &["a", "b"]),
            asked_with(vec![option("fine", "d"), bad.clone()]),
        ])
        .unwrap_err();
        assert!(matches!(refusal, ToolRefusal::InvalidQuestion(_, 3)), "{bad:?}");
        assert!(refusal.to_string().contains("question 3 of the set"), "{refusal}");
    }
}

/// ADQ-FR-PVXK: an option written as a bare string, or with a null
/// description, decodes and is refused as a blank description naming its
/// question, rather than as an undecodable call that names nothing.
#[test]
fn an_option_in_another_shape_is_refused_as_a_blank_description() {
    for options in [
        serde_json::json!(["a", "b"]),
        serde_json::json!([
            { "label": "a", "description": "Fine." },
            { "label": "b", "description": null },
        ]),
    ] {
        let args: AskDiscussionQuestionsArgs = serde_json::from_value(serde_json::json!({
            "questions": [
                { "question": "first?", "options": [
                    { "label": "a", "description": "d" },
                    { "label": "b", "description": "d" },
                ] },
                { "question": "second?", "options": options },
            ],
        }))
        .expect("decodes");
        let refusal = normalize(&args.questions).unwrap_err();
        assert!(
            matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_DESCRIPTION_BLANK, 2)),
            "{refusal:?}",
        );
    }
}

/// ADQ-FR-WBQL, ADQ-FR-ZBQH, ADQ-FR-LGKM: the description and the parameter
/// descriptions state the label and description limits the tool enforces.
#[test]
fn the_descriptions_state_the_option_limits() {
    for text in [DESCRIPTION, LABEL_DESCRIPTION] {
        assert!(text.contains("5 words") && text.contains("40 characters"), "{text}");
    }
    for text in [DESCRIPTION, OPTION_DESCRIPTION_DESCRIPTION] {
        assert!(text.contains("300 characters"), "{text}");
    }
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
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
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
        let draft_id =
            crate::drafts::create_draft_at_root(&root, Some("ontology")).expect("draft").draft.id;
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &crate::comments::DiscussionTarget::Draft { draft_id: draft_id.clone() },
            None,
            "@arch which way?".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
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
        crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() }
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
        set.questions.iter().map(|q| (q.position, q.text.as_str())).collect::<Vec<_>>(),
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

/// ADQ-FR-GMDK, ADQ-FR-PZWD: the recorded set carries each option's trimmed
/// label and description.
#[test]
fn a_recorded_set_carries_each_options_description() {
    let fixture = Fixture::new();
    block_on(fixture.tool().call(args(vec![asked_with(vec![
        option(" One spec ", "  Easy to find.  "),
        option("Paired specs", "Each stays small."),
    ])])))
    .expect("recorded");

    let set = fixture.set().expect("a set stands");
    let options: Vec<(&str, Option<&str>)> = set.questions[0]
        .options
        .iter()
        .map(|o| (o.value.as_str(), o.description.as_deref()))
        .collect();
    assert_eq!(
        options,
        vec![("One spec", Some("Easy to find.")), ("Paired specs", Some("Each stays small."))],
    );
}

/// ADQ-FR-CJRM: one blank description in an otherwise valid set records
/// nothing at all.
#[test]
fn a_blank_description_in_a_valid_set_records_nothing() {
    let fixture = Fixture::new();
    let refusal = block_on(fixture.tool().call(args(vec![
        asked("first?", &["a", "b"]),
        asked_with(vec![option("a", "d"), option("b", "  ")]),
    ])))
    .expect_err("refused");
    assert!(matches!(refusal, ToolRefusal::InvalidQuestion(SET_OPTION_DESCRIPTION_BLANK, 2)));
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

    block_on(fixture.reporting_into(&ADQ_LOG).call(args(vec![
        asked("Should the ontology live in one spec or two?", &["one spec", "two specs"]),
        asked("Where does the flow diagram go?", &["the ui spec", "the core spec"]),
    ])))
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
    for composed in
        ["ontology", "one spec", "two specs", "flow diagram", "the ui spec", "archive", "keep"]
    {
        assert!(!rendered.contains(composed), "ADQ-FR-LZHV: a record carried {composed:?}",);
    }
}

/// ADQ-FR-ZBQH, ADQ-FR-LGKM: each refusal names the limit it enforces, so a
/// model told it went over learns by how much it must shorten.
#[test]
fn each_option_refusal_names_its_limit() {
    assert!(SET_OPTION_TOO_LONG.contains(&format!("{LABEL_CHAR_LIMIT} characters")));
    assert!(
        SET_OPTION_DESCRIPTION_TOO_LONG.contains(&format!("{DESCRIPTION_CHAR_LIMIT} characters"))
    );
    assert!(SET_LABEL_TOO_MANY_WORDS.contains(&format!("{LABEL_WORD_LIMIT} words")));
}
