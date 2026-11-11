//! Tests for `RCA-request-change-action-tool.md` and the action request
//! entries of `DCE-decide-change-edits-tool.md`.
//!
//! The store behind both tools is tested in `change_action_requests/tests/`.
//! What is tested here is the call: the binding, the arguments, the refusals,
//! the flag that ends the turn, the decision entries, and the logs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rig::tool::{PortableTool, ToolErrorKind};

use super::change_edit_tools::{
    agent, decisions, human, Fixture, CHANGE, QUIET, RATIONALE, STREAM,
};
use crate::agent_conversations::ConversationOrigin;
use crate::change_action_requests::{self as car, ChangeActionRequestState, RunStarter};
use crate::change_proposals::FixedSites;
use crate::comments::{self, DiscussionTarget, Participant};
use crate::graduation::GraduationRun;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::decide_changes::{CallRefusal, Outcome, RefusalCode};
use crate::tools::request_change_action::{
    self as rca, RequestChangeActionArgs, RequestChangeActionTool,
};
use crate::tools::tests::block_on;
use crate::tools::ToolRefusal;

const INSTRUCTION: &str = "Split task 1.1 in two.";

fn tool_with(
    f: &Fixture,
    sites: Box<FixedSites>,
    requested: &Arc<AtomicBool>,
    buffer: &'static LogBuffer,
) -> RequestChangeActionTool<tauri::test::MockRuntime> {
    RequestChangeActionTool::new(
        f.open.handle(),
        f.roots(),
        f.origin(),
        agent(),
        Arc::clone(requested),
    )
    .for_test(sites, buffer)
}

fn args(action: &str, instruction: Option<&str>) -> RequestChangeActionArgs {
    RequestChangeActionArgs {
        action: action.into(),
        instruction: instruction.map(str::to_string),
        rationale: RATIONALE.into(),
    }
}

fn request(
    f: &Fixture,
    requested: &Arc<AtomicBool>,
    args: RequestChangeActionArgs,
) -> Result<rca::RequestChangeActionOutput, ToolRefusal> {
    block_on(tool_with(f, f.sites(), requested, &QUIET).call(args))
}

fn thread_len(f: &Fixture) -> usize {
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    comments::locate_discussion_of(&f.store(), &target, &f.thread_id)
        .expect("thread")
        .comments
        .len()
}

/// The fixed fields of a refusal, for an assertion.
fn shape(refusal: &ToolRefusal) -> (&'static str, ToolErrorKind, bool) {
    (refusal.reason(), refusal.kind(), refusal.retryable())
}

// RCA-FR-UZJK / RCA-FR-PTAK: a portable tool named `request_change_action`
// with the fixed text and the schema of the contract surface.
#[test]
fn rca_is_a_portable_tool_with_its_fixed_text_and_schema() {
    let f = Fixture::new();
    let t = tool_with(&f, f.sites(), &Arc::default(), &QUIET);
    assert_eq!(
        <RequestChangeActionTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "request_change_action"
    );
    assert!(t.description().starts_with("Ask the author to start a run on the OpenSpec change"));
    assert!(t
        .description()
        .ends_with("Your turn ends when the request is posted. Make one request at a time."));
    let schema = t.parameters();
    assert_eq!(schema["required"], serde_json::json!(["action", "rationale"]));
    assert_eq!(schema["properties"]["action"]["enum"], serde_json::json!(["apply", "rework"]));
    assert_eq!(
        schema["properties"]["action"]["description"],
        "`apply` to implement the tasks of the change, or `rework` to revise the change itself."
    );
    assert!(schema["properties"]["instruction"]["description"]
        .as_str()
        .unwrap()
        .starts_with("For `rework` alone"));
    assert!(schema["properties"]["rationale"]["description"]
        .as_str()
        .unwrap()
        .contains("<spoken>"));
    // RCA-FR-QANG: no argument names a stream, a change, a conversation, or a
    // participant.
    let names: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
    assert_eq!(names.len(), 3, "{names:?}");
}

// RCA-FR-QEWH / RCA-FR-IKUP / RCA-FR-QANG / RCA-FR-BYYI: a call records one
// request on the bound change, as the bound agent, in the bound thread, with
// the action in lowercase and the instruction trimmed; the comment is the
// rationale; it answers the request id and sets the flag that ends the turn.
#[test]
fn rca_records_one_request_and_ends_the_turn() {
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    let before = thread_len(&f);
    let out = request(&f, &requested, args(" ReWork ", Some(&format!("  {INSTRUCTION} "))))
        .expect("recorded");
    assert!(requested.load(Ordering::SeqCst));
    let all = car::list_requests_impl(&f.store(), STREAM, CHANGE);
    assert_eq!(all.len(), 1);
    let record = &all[0];
    assert_eq!(out.request_id, record.id);
    assert_eq!(record.action, car::ChangeAction::Rework);
    assert_eq!(record.instruction.as_deref(), Some(INSTRUCTION));
    assert_eq!(record.thread_id, f.thread_id);
    assert_eq!(record.agent, agent());
    assert_eq!(record.state, ChangeActionRequestState::Pending);
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).unwrap();
    assert_eq!(thread.comments.len(), before + 1, "exactly one comment");
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.body, RATIONALE);
    assert_eq!(comment.author, agent());
    assert!(matches!(
        comment.attachments.as_slice(),
        [comments::Attachment::ChangeActionRequest { request_id, action, .. }]
            if *request_id == record.id && action == "rework"
    ));
    // RCA-FR-IKUP: the output is the request id alone, and nothing started.
    assert_eq!(serde_json::to_value(&out).unwrap(), serde_json::json!({ "request_id": record.id }));
    assert!(record.run_id.is_none());
}

// RCA-FR-BYYI / RCA-FR-KTJP / RCA-FR-XDQZ: the argument refusals are
// `InvalidArgs`, retryable, record nothing, and leave the turn running.
#[test]
fn rca_refuses_bad_arguments() {
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    let cases = [
        (args("archive", None), "bad_action", rca::BAD_ACTION),
        (args("rework", None), "no_instruction", rca::NO_INSTRUCTION),
        (args("rework", Some("  \n")), "no_instruction", rca::NO_INSTRUCTION),
        (
            args("apply", Some("do more")),
            "apply_takes_no_instruction",
            rca::APPLY_TAKES_NO_INSTRUCTION,
        ),
        (
            RequestChangeActionArgs {
                action: "apply".into(),
                instruction: None,
                rationale: "   ".into(),
            },
            "blank_rationale",
            rca::BLANK_RATIONALE,
        ),
        (
            RequestChangeActionArgs {
                action: "apply".into(),
                instruction: None,
                rationale: "Ready to build.".into(),
            },
            "no_spoken_part",
            rca::NO_SPOKEN_PART,
        ),
    ];
    for (call, code, text) in cases {
        let refused = request(&f, &requested, call).unwrap_err();
        assert_eq!(shape(&refused), (code, ToolErrorKind::InvalidArgs, true));
        assert!(matches!(refused, ToolRefusal::Fixed { message, .. } if message == text));
    }
    assert!(!requested.load(Ordering::SeqCst));
    assert!(car::list_requests_impl(&f.store(), STREAM, CHANGE).is_empty());
    // RCA-FR-KTJP: an apply with a blank instruction is an apply.
    request(&f, &requested, args("apply", Some("  "))).expect("recorded");
}

// RCA-FR-CGCE / RCA-FR-XXIP: a conversation that is not about a change, and a
// closed project, are refused and record nothing.
#[test]
fn rca_refuses_other_conversations_and_a_closed_project() {
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    let other = RequestChangeActionTool::new(
        f.open.handle(),
        f.roots(),
        ConversationOrigin::stub_artifact("t", "a.md", false),
        agent(),
        Arc::clone(&requested),
    )
    .for_test(f.sites(), &QUIET);
    let refused = block_on(other.call(args("apply", None))).unwrap_err();
    assert_eq!(
        shape(&refused),
        ("not_a_change_conversation", ToolErrorKind::PermissionDenied, false)
    );
    assert!(
        matches!(refused, ToolRefusal::Fixed { message, .. } if message == rca::NOT_A_CHANGE_CONVERSATION)
    );
    let closed = super::closed_project();
    let t = RequestChangeActionTool::new(
        closed.handle().clone(),
        f.roots(),
        f.origin(),
        agent(),
        Arc::clone(&requested),
    );
    assert_eq!(block_on(t.call(args("apply", None))).unwrap_err(), ToolRefusal::NoProjectOpen);
    assert!(!requested.load(Ordering::SeqCst));
    assert!(car::list_requests_impl(&f.store(), STREAM, CHANGE).is_empty());
}

// RCA-FR-FRLK / RCA-FR-YRTD / RCA-FR-KKVG: a pending request, a resolved
// conversation, a gone stream, and a gone change are refused with their kinds,
// none retryable, and record nothing.
#[test]
fn rca_refuses_what_the_store_refuses() {
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    // A gone stream: the resolver holds no stream.
    let gone = Box::new(FixedSites {
        sites: Vec::new(),
        busy: Mutex::new(None),
        rework: Mutex::new(None),
    });
    let refused =
        block_on(tool_with(&f, gone, &requested, &QUIET).call(args("apply", None))).unwrap_err();
    assert_eq!(shape(&refused), ("stream_gone", ToolErrorKind::NotFound, false));
    // A gone change: the working copy holds no folder of the change.
    std::fs::remove_dir_all(f.wc.path().join(format!("openspec/changes/{CHANGE}"))).unwrap();
    let refused = request(&f, &requested, args("apply", None)).unwrap_err();
    assert_eq!(shape(&refused), ("change_gone", ToolErrorKind::NotFound, false));
    assert!(matches!(refused, ToolRefusal::Fixed { message, .. } if message == rca::CHANGE_GONE));
    assert!(car::list_requests_impl(&f.store(), STREAM, CHANGE).is_empty());
    std::fs::create_dir_all(f.wc.path().join(format!("openspec/changes/{CHANGE}"))).unwrap();

    // A pending request.
    request(&f, &requested, args("apply", None)).expect("recorded");
    requested.store(false, Ordering::SeqCst);
    let refused = request(&f, &requested, args("rework", Some(INSTRUCTION))).unwrap_err();
    assert_eq!(shape(&refused), ("request_pending", ToolErrorKind::PermissionDenied, false));
    assert!(
        matches!(refused, ToolRefusal::Fixed { message, .. } if message == rca::REQUEST_PENDING)
    );
    let pending = car::pending_request(&f.store(), STREAM, CHANGE).expect("pending");
    car::decline_action_request(&f.open.handle(), &f.store(), &pending.id, None, &human())
        .expect("declined");

    // A resolved conversation.
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).unwrap();
    comments::set_resolution_to(
        &f.store(),
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let refused = request(&f, &requested, args("apply", None)).unwrap_err();
    assert_eq!(shape(&refused), ("conversation_resolved", ToolErrorKind::PermissionDenied, false));
    assert!(!requested.load(Ordering::SeqCst));
    assert_eq!(car::list_requests_impl(&f.store(), STREAM, CHANGE).len(), 1);
}

// RCA-FR-DJOP: a comment that does not append is the retryable `could not
// record` refusal, and no request stays.
#[test]
fn rca_reports_a_failed_recording() {
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    car::FAIL_APPEND.with(|fail| fail.set(true));
    let refused = request(&f, &requested, args("apply", None)).unwrap_err();
    assert_eq!(shape(&refused), ("could_not_record", ToolErrorKind::Other, true));
    assert!(
        matches!(refused, ToolRefusal::Fixed { message, .. } if message == rca::COULD_NOT_RECORD)
    );
    assert!(!requested.load(Ordering::SeqCst));
    assert!(car::list_requests_impl(&f.store(), STREAM, CHANGE).is_empty());
}

// RCA-FR-ZWAF: a recorded call logs `INFO` under `ai` with the tool, the
// stream, the change, and the action; a refusal logs `WARN` with the tool and
// the reason. No record carries the instruction or the rationale.
#[test]
fn rca_logs_ids_and_the_action_alone() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let requested = Arc::new(AtomicBool::new(false));
    block_on(tool_with(&f, f.sites(), &requested, &BUFFER).call(args("rework", Some(INSTRUCTION))))
        .expect("recorded");
    let _ = block_on(tool_with(&f, f.sites(), &requested, &BUFFER).call(args("apply", None)));
    let records = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .expect("buffer")
        .records;
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("info");
    assert!(info.domains.contains(&Domain::Backend));
    let fields = serde_json::to_value(&info.fields).unwrap();
    assert_eq!(fields["tool"], "request_change_action");
    assert_eq!(fields["streamId"], STREAM);
    assert_eq!(fields["change"], CHANGE);
    assert_eq!(fields["action"], "rework");
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("warn");
    let fields = serde_json::to_value(&warn.fields).unwrap();
    assert_eq!(fields["reason"], "request_pending");
    let all =
        serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>()).unwrap();
    assert!(!all.contains("Split task") && !all.contains("Add logout"), "{all}");
}

// ---------------------------------------------------------------------------
// decide_change_edits on a change action request
// ---------------------------------------------------------------------------

/// The run queue as a confirmation reaches it, with the calls it took.
struct Starter {
    calls: Arc<Mutex<Vec<String>>>,
    refusal: Option<&'static str>,
}

impl Starter {
    fn new(refusal: Option<&'static str>) -> (Box<Starter>, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::default();
        (Box::new(Starter { calls: Arc::clone(&calls), refusal }), calls)
    }

    fn answer(&self, phase: &str) -> Result<GraduationRun, String> {
        if let Some(code) = self.refusal {
            return Err(code.to_string());
        }
        let mut run = crate::graduation::build_run(
            "project",
            STREAM.into(),
            "Stream".into(),
            None,
            crate::graduation::CapturedGraduationInput::default(),
            crate::graduation::StandingWork::Keep,
            None,
        );
        run.openspec = Some(crate::graduation::RunOpenSpec::of_phase(phase, CHANGE));
        Ok(run)
    }
}

impl RunStarter for Starter {
    fn start_apply(&self, _stream_id: &str, change: &str) -> Result<GraduationRun, String> {
        self.calls.lock().unwrap().push(format!("apply {change}"));
        self.answer("apply")
    }

    fn start_rework(
        &self,
        _stream_id: &str,
        change: &str,
        _instruction: &str,
        _thread_id: &str,
        _requester: &Participant,
    ) -> Result<GraduationRun, String> {
        self.calls.lock().unwrap().push(format!("rework {change}"));
        self.answer("rework")
    }
}

/// A pending request of the fixture's change.
fn pending(f: &Fixture, action: &str) -> car::ChangeActionRequest {
    let instruction = (action == "rework").then_some(INSTRUCTION);
    request(f, &Arc::default(), args(action, instruction)).expect("recorded");
    car::pending_request(&f.store(), STREAM, CHANGE).expect("pending")
}

// DCE-FR-IZTA / DCE-FR-UZDN / DCE-FR-PHSU: with no edit proposal open, a
// `confirm` entry with the request id, on the author's typed instruction,
// starts its run and answers `started` with the run id.
#[test]
fn dce_confirms_a_request_and_starts_its_run() {
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let (starter, calls) = Starter::new(None);
    let out = decide_now(
        f.decider(f.sites(), &QUIET)
            .with_starter(starter)
            .call(decisions(&[(&record.id, "confirm")])),
    )
    .expect("decided");
    assert_eq!(*calls.lock().unwrap(), vec![format!("apply {CHANGE}")]);
    let row = &out.decisions[0];
    assert_eq!(row.outcome, Outcome::Started);
    let held = car::list_requests_impl(&f.store(), STREAM, CHANGE).remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Started);
    assert_eq!(row.run_id, held.run_id);
    let wire = serde_json::to_value(row).unwrap();
    assert_eq!(wire["outcome"], "started");
    assert!(wire["run_id"].is_string() || wire["runId"].is_string(), "{wire}");
    // CAR-FR-RKRY: the decision comment carries no feedback.
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).unwrap();
    let decision = thread.comments.last().unwrap();
    assert_eq!(decision.author, human());
    assert!(decision.body.ends_with("started."), "{}", decision.body);
}

// DCE-FR-IZTA: a `reject` entry with the request id declines it with no
// feedback, and starts nothing.
#[test]
fn dce_declines_a_request() {
    let f = Fixture::new();
    let record = pending(&f, "rework");
    let (starter, calls) = Starter::new(None);
    let out = decide_now(
        f.decider(f.sites(), &QUIET)
            .with_starter(starter)
            .call(decisions(&[(&record.id, "reject")])),
    )
    .expect("decided");
    assert_eq!(out.decisions[0].outcome, Outcome::Declined);
    assert!(out.decisions[0].run_id.is_none());
    assert!(calls.lock().unwrap().is_empty());
    let held = car::list_requests_impl(&f.store(), STREAM, CHANGE).remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Declined);
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).unwrap();
    assert_eq!(
        thread.comments.last().unwrap().body,
        format!("Declined the request to rework the OpenSpec change `{CHANGE}`.")
    );
}

// DCE-FR-UVUT: a refused start is a `decision_failed` row with the code,
// retryable but for `unknown_stream` and `change_not_found`; `accept` and
// `discuss` on a request are `invalid_action`.
#[test]
fn dce_reports_a_refused_start_and_wrong_actions() {
    for (code, retryable) in [
        ("openspec_run_active: run-9", true),
        ("change_not_ready: design", true),
        ("unknown_stream", false),
        ("change_not_found", false),
    ] {
        let f = Fixture::new();
        let record = pending(&f, "apply");
        let (starter, _) = Starter::new(Some(code));
        let out = decide_now(
            f.decider(f.sites(), &QUIET)
                .with_starter(starter)
                .call(decisions(&[(&record.id, "confirm")])),
        )
        .expect("decided");
        let refusal = out.decisions[0].refusal.clone().expect("refused");
        assert_eq!(refusal.code, RefusalCode::DecisionFailed);
        let bare = code.split(':').next().unwrap();
        assert_eq!(refusal.error.as_deref(), Some(bare));
        assert_eq!(refusal.retryable, retryable, "{code}");
        assert!(!refusal.message.contains("run-9"));
    }
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let (starter, calls) = Starter::new(None);
    let out = decide_now(
        f.decider(f.sites(), &QUIET)
            .with_starter(starter)
            .call(decisions(&[(&record.id, "accept"), (&record.id, "discuss")])),
    )
    .expect("decided");
    for row in &out.decisions {
        assert_eq!(row.refusal.as_ref().map(|r| r.code), Some(RefusalCode::InvalidAction));
    }
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(car::pending_request(&f.store(), STREAM, CHANGE).map(|r| r.id), Some(record.id));
}

// DCE-FR-QUKG / DCE-FR-XVLE / DDC-FR-NKAB: a call whose instruction is not in
// the authorizing author comment confirms nothing and starts no run.
#[test]
fn dce_confirms_nothing_without_the_authors_word() {
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let (starter, calls) = Starter::new(None);
    let mut call = decisions(&[(&record.id, "confirm")]);
    call.instruction = Some("words the author never wrote".into());
    let refused = decide_now(f.decider(f.sites(), &QUIET).with_starter(starter).call(call));
    assert!(matches!(refused, Err(CallRefusal::Authority(_))));
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        car::pending_request(&f.store(), STREAM, CHANGE).map(|r| r.state),
        Some(ChangeActionRequestState::Pending)
    );
}

/// A voice turn of the fixture's conversation: a turn bound to an open
/// speech, as AGC-FR-QYNZ starts it. It answers the turn id.
fn voice_turn(f: &Fixture) -> String {
    crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        None,
        Some("speech-1"),
    )
}

/// The author speaks `body` in voice mode: the comment of the speech, with
/// its spoken original, posts and ends the speech of `turn` (AGC-FR-TCDQ).
fn speak(f: &Fixture, turn: &str, body: &str) -> String {
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).unwrap();
    let (_, id) = comments::add_comment_reporting_id(
        &f.store(),
        thread.log_ref(),
        &f.thread_id,
        body.to_string(),
        Vec::new(),
        Vec::new(),
        Some(format!("{body} (spoken)")),
        comments::Stamp { by: &human(), at: "2026-01-01T00:00:05Z" },
    )
    .expect("spoken comment");
    let app = f.open.handle();
    crate::agent_conversations::note_author_comment(&app, &id);
    crate::agent_conversations::authority_for_test::end_speech(&app, turn, &id);
    id
}

/// A `decide_change_edits` tool bound to `turn`.
fn decider_on(
    f: &Fixture,
    turn: &str,
    starter: Box<Starter>,
) -> crate::tools::decide_change_edits::DecideChangeEditsTool<tauri::test::MockRuntime> {
    crate::tools::decide_change_edits::DecideChangeEditsTool::new(
        f.open.handle(),
        f.roots(),
        f.origin(),
        agent(),
        turn,
    )
    .for_test(human(), f.sites(), &QUIET)
    .with_starter(starter)
}

fn confirm_call(id: &str, instruction: &str) -> crate::tools::decide_changes::DecideChangesArgs {
    let mut call = decisions(&[(id, "confirm")]);
    call.instruction = Some(instruction.into());
    call
}

// DCE-FR-QUKG / DDC-FR-ELAE: an author comment spoken in voice mode, with its
// spoken original, authorizes a `confirm`, and the run starts.
#[test]
fn dce_a_spoken_comment_confirms_a_request() {
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let turn = voice_turn(&f);
    let spoken = speak(&f, &turn, "Yes, build it now.");
    let stored = comments::locate_discussion_of(
        &f.store(),
        &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
        &f.thread_id,
    )
    .unwrap();
    let comment = stored.comments.iter().find(|c| c.id == spoken).expect("spoken comment");
    assert_eq!(comment.spoken_original.as_deref(), Some("Yes, build it now. (spoken)"));
    let (starter, calls) = Starter::new(None);
    let out = decide_now(decider_on(&f, &turn, starter).call(confirm_call(&record.id, "build it")))
        .expect("decided");
    assert_eq!(out.decisions[0].outcome, Outcome::Started);
    assert_eq!(*calls.lock().unwrap(), vec![format!("apply {CHANGE}")]);
    assert_eq!(
        car::list_requests_impl(&f.store(), STREAM, CHANGE)[0].state,
        ChangeActionRequestState::Started
    );
}

// DCE-FR-QUKG / DDC-FR-ZYOL: a `confirm` while the turn is bound to an open
// speech is the speech not posted refusal, and starts nothing.
#[test]
fn dce_confirms_nothing_while_the_speech_is_open() {
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let turn = voice_turn(&f);
    let (starter, calls) = Starter::new(None);
    let refused =
        decide_now(decider_on(&f, &turn, starter).call(confirm_call(&record.id, "build it")));
    assert!(
        matches!(
            refused,
            Err(CallRefusal::Authority(
                crate::agent_conversations::AuthorityRefusal::SpeechNotPosted
            ))
        ),
        "{:?}",
        refused.as_ref().err().map(CallRefusal::reason)
    );
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        car::pending_request(&f.store(), STREAM, CHANGE).map(|r| r.id),
        Some(record.id),
        "still pending"
    );
}

// DCE-FR-QUKG / DDC-FR-VONX: a confirmation spends its spoken comment; a
// second `confirm` under the same comment, of a new pending request, is the
// instruction used refusal and starts nothing.
#[test]
fn dce_a_spent_spoken_comment_confirms_nothing_more() {
    let f = Fixture::new();
    let first = pending(&f, "apply");
    let turn = voice_turn(&f);
    speak(&f, &turn, "Yes, build it now.");
    let (starter, calls) = Starter::new(None);
    decide_now(decider_on(&f, &turn, starter).call(confirm_call(&first.id, "build it")))
        .expect("decided");
    assert_eq!(calls.lock().unwrap().len(), 1);

    let second = pending(&f, "apply");
    assert_ne!(second.id, first.id);
    let (starter, calls) = Starter::new(None);
    let refused =
        decide_now(decider_on(&f, &turn, starter).call(confirm_call(&second.id, "build it")));
    assert!(
        matches!(
            refused,
            Err(CallRefusal::Authority(
                crate::agent_conversations::AuthorityRefusal::InstructionUsed
            ))
        ),
        "{:?}",
        refused.as_ref().err().map(CallRefusal::reason)
    );
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        car::pending_request(&f.store(), STREAM, CHANGE).map(|r| r.id),
        Some(second.id),
        "still pending"
    );
}

// DCE-FR-WRSI / DCE-FR-UZDN / DCE-FR-PHSU: a `confirm` with an id that names
// neither the split nor the pending request is `unknown_hunk`; a decided
// request is not pending, so a call with nothing else open is the no open
// proposal refusal.
#[test]
fn dce_refuses_an_id_that_names_no_pending_request() {
    let f = Fixture::new();
    let record = pending(&f, "apply");
    let (starter, calls) = Starter::new(None);
    let out = decide_now(
        f.decider(f.sites(), &QUIET)
            .with_starter(starter)
            .call(decisions(&[("not-an-id", "confirm")])),
    )
    .expect("decided");
    assert_eq!(out.decisions[0].refusal.as_ref().map(|r| r.code), Some(RefusalCode::UnknownHunk));
    assert!(calls.lock().unwrap().is_empty());
    car::decline_action_request(&f.open.handle(), &f.store(), &record.id, None, &human())
        .expect("declined");
    let (starter, _) = Starter::new(None);
    let none = decide_now(
        f.decider(f.sites(), &QUIET)
            .with_starter(starter)
            .call(decisions(&[(&record.id, "confirm")])),
    );
    assert!(matches!(none, Err(CallRefusal::NoOpenProposal(_))));
}

// DCE-FR-ILCP / DCE-FR-RNHP: the description is the fixed text, which names
// the request and says a confirmation starts its run, and the `id` and
// `action` descriptions name the request.
#[test]
fn dce_describes_the_request_entries() {
    let f = Fixture::new();
    let t = f.decider(f.sites(), &QUIET);
    assert_eq!(t.description(), crate::tools::decide_change_edits::DESCRIPTION);
    assert!(t.description().contains("the pending request to apply or rework the change"));
    assert!(t.description().contains("Confirming a request starts its run."));
    assert!(t
        .description()
        .ends_with("This never creates or changes a hunk, a proposed change, or a request."));
    let schema = t.parameters().to_string();
    assert!(schema.contains("of the request to apply or rework the change"), "{schema}");
    assert!(schema.contains("with the id of the request to start its run"), "{schema}");
}

/// Every file below `root`, with its bytes, in path order.
fn tree(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.push((rel, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

// RCA-FR-DGRT: a call writes no file but the request record and the comment:
// the working copy and its branch are as they were, and the store gains the
// record alone beside the conversation's log.
#[test]
fn rca_writes_the_record_and_the_comment_alone() {
    let f = Fixture::new();
    let head = || git2::Repository::open(f.wc.path()).unwrap().head().unwrap().target();
    let (wc_before, head_before, store_before) =
        (tree(f.wc.path()), head(), tree(f.store().path()));
    let out = request(&f, &Arc::default(), args("apply", None)).expect("recorded");
    assert_eq!(tree(f.wc.path()), wc_before, "the working copy is as it was");
    assert_eq!(head(), head_before, "nothing is committed");
    let after = tree(f.store().path());
    let added: Vec<&String> = after
        .iter()
        .filter(|(path, _)| !store_before.iter().any(|(p, _)| p == path))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(added.len(), 1, "{added:?}");
    assert!(added[0].ends_with(&format!("action-requests/{}.toml", out.request_id)), "{added:?}");
    let changed: Vec<&String> = after
        .iter()
        .filter(|(path, bytes)| store_before.iter().any(|(p, b)| p == path && b != bytes))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(changed.len(), 1, "the conversation's log alone: {changed:?}");
}

/// A call of `decide_change_edits`, which runs its decisions on a blocking
/// thread (CAR-FR-AQHX), so it needs an executor and not one poll.
fn decide_now<F: std::future::Future>(future: F) -> F::Output {
    tauri::async_runtime::block_on(future)
}
