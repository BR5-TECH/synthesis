//! Tests for `DDC-decide-draft-changes-tool.md`.
//!
//! What a decision writes is `DCP-draft-change-proposals.md`'s and is tested in
//! `draft_proposals/tests`. What is tested here is the call: which entries run,
//! what each one does through the chip's own function, what is refused, and
//! what is reported and logged.

use std::sync::{Arc, Mutex};

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::comments::{Attachment, DiscussionTarget, ThreadRef};
use crate::draft_proposals::{hunks::ProposedHunk, NewProposal};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::decide_changes::{DecisionArg, Outcome, RefusalCode};
use crate::tools::tests::{block_on, closed_project, mounted};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FILE: &str = "spec.md";
const ORIGINAL: &str = "# Spec\n\nAlpha line.\n\nBeta line.\n\nGamma line.\n";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: None,
    }
}

static QUIET: LogBuffer = LogBuffer::new();

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    proposal_id: String,
    /// The hunk ids, in proposal order.
    hunks: Vec<String>,
}

impl Fixture {
    /// A draft whose prompt holds three lines and whose one open proposal
    /// changes each of them.
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("spec")).expect("draft");
        assert_eq!(created.file, FILE);
        crate::drafts::save_draft_file_impl(&root, &created.draft.id, FILE, ORIGINAL)
            .expect("seed");
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch what would you change?".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        let open = mounted(dir);
        let proposed: Vec<ProposedHunk> = ["Alpha", "Beta", "Gamma"]
            .iter()
            .map(|word| {
                ProposedHunk::from_fields(
                    Some(format!("{word} line.")),
                    Some(format!("{word} better.")),
                    None,
                    None,
                    None,
                    None,
                )
            })
            .collect();
        let recorded = crate::draft_proposals::record_proposal(
            &open.handle(),
            &root,
            &root,
            NewProposal {
                draft_id: &created.draft.id,
                path: FILE,
                hunks: &proposed,
                rationale: "Sharper. <spoken>Sharper.</spoken>",
                agent: &agent(),
                target: ThreadRef::discussion(&created.draft.id),
                thread_id: &thread.id,
            },
        )
        .expect("recorded");
        Fixture {
            open,
            path,
            draft_id: created.draft.id,
            thread_id: thread.id,
            proposal_id: recorded.id.clone(),
            hunks: recorded.ledger.iter().map(|row| row.id.clone()).collect(),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
    }

    fn tool_for(
        &self,
        origin: ConversationOrigin,
        buffer: &'static LogBuffer,
    ) -> DecideDraftChangesTool<tauri::test::MockRuntime> {
        DecideDraftChangesTool::new(
            self.open.handle(),
            OwnedRoots { worktree: self.root(), store: self.root() },
            origin,
            agent(),
        )
        .for_test(human(), buffer)
    }

    fn call(&self, decisions: &[(&str, &str)]) -> Result<DecideChangesOutput, CallRefusal> {
        self.call_into(&QUIET, decisions)
    }

    fn call_into(
        &self,
        buffer: &'static LogBuffer,
        decisions: &[(&str, &str)],
    ) -> Result<DecideChangesOutput, CallRefusal> {
        block_on(self.tool_for(self.origin(), buffer).call(args(decisions)))
    }

    fn hunk(&self, place: usize) -> &str {
        &self.hunks[place - 1]
    }

    fn prompt(&self) -> String {
        crate::drafts::load_draft_file_impl(&self.root(), &self.draft_id, FILE)
            .expect("prompt")
            .body
    }

    fn record(&self) -> crate::draft_proposals::DraftChangeProposal {
        crate::draft_proposals::proposal_as_it_stands(&self.root(), &self.proposal_id)
            .expect("record")
    }

    fn state_of(&self, place: usize) -> HunkState {
        let id = self.hunk(place).to_string();
        self.record().ledger.iter().find(|row| row.id == id).expect("row").state
    }

    fn versions(&self) -> usize {
        crate::draft_history::list_impl(&self.root(), &self.draft_id)
            .expect("history")
            .entries
            .len()
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(
            &self.root(),
            &DiscussionTarget::Draft { draft_id: self.draft_id.clone() },
        )
    }

    fn thread(&self) -> comments::Discussion {
        self.discussions().into_iter().find(|d| d.id == self.thread_id).expect("thread")
    }
}

fn args(decisions: &[(&str, &str)]) -> DecideChangesArgs {
    DecideChangesArgs {
        decisions: Some(
            decisions
                .iter()
                .map(|(id, action)| DecisionArg {
                    id: Some(id.to_string()),
                    action: Some(action.to_string()),
                })
                .collect(),
        ),
    }
}

fn code_of(output: &DecideChangesOutput, place: usize) -> Option<RefusalCode> {
    output.decisions[place - 1].refusal.as_ref().map(|r| r.code)
}

fn log_records(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the buffer answers")
        .records
}

// ---------------------------------------------------------------------------
// The definition
// ---------------------------------------------------------------------------

// DDC-FR-SQYQ, DDC-FR-VAYO, DDC-FR-ROLG, DDC-FR-MDCX
#[test]
fn it_is_a_portable_tool_with_the_fixed_description_and_schema() {
    let f = Fixture::new();
    let definition = rig::tool::tool_definition(&f.tool_for(f.origin(), &QUIET));
    assert_eq!(definition.name, "decide_draft_changes");
    assert_eq!(definition.description, DESCRIPTION);
    // DDC-FR-ROLG: an explicit instruction, and no read-back.
    assert!(DESCRIPTION.contains("Call it only when the author's latest comment"));
    assert!(DESCRIPTION.contains("Do not read the decision back and wait for a confirmation"));
    // DDC-FR-MDCX: a place maps to proposal order, withdrawn ones skipped.
    assert!(DESCRIPTION.contains("count the changes in their order and skip the withdrawn ones"));
    let schema = definition.parameters;
    let decisions = &schema["properties"]["decisions"];
    assert_eq!(decisions["minItems"], 1);
    assert_eq!(decisions["maxItems"], 50);
    assert_eq!(decisions["items"]["required"], serde_json::json!(["id", "action"]));
    assert_eq!(
        decisions["items"]["properties"]["action"]["enum"],
        serde_json::json!(["accept", "reject", "discuss"])
    );
    assert!(decisions["description"].as_str().is_some_and(|d| d.contains("latest comment")));
    assert!(decisions["items"]["properties"]["action"]["description"]
        .as_str()
        .is_some_and(|d| d.contains("into the prompt")));
    assert_eq!(schema["required"], serde_json::json!(["decisions"]));
}

// DDC-FR-QRIY, DDC-FR-VAYO
#[test]
fn no_argument_names_a_draft_a_proposal_a_conversation_or_a_participant() {
    // Two tools bound to two different drafts read one identical definition.
    let a = Fixture::new();
    let b = Fixture::new();
    let one = rig::tool::tool_definition(&a.tool_for(a.origin(), &QUIET));
    let two = rig::tool::tool_definition(&b.tool_for(b.origin(), &QUIET));
    assert_eq!(one.parameters, two.parameters);
    assert_eq!(one.description, two.description);
    let properties = one.parameters["properties"].as_object().expect("properties");
    assert_eq!(properties.keys().collect::<Vec<_>>(), vec!["decisions"]);
    let entry = one.parameters["properties"]["decisions"]["items"]["properties"]
        .as_object()
        .expect("entry");
    let mut keys: Vec<&String> = entry.keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["action", "id"]);
}

// ---------------------------------------------------------------------------
// The whole-call refusals
// ---------------------------------------------------------------------------

// DDC-FR-OEJB
#[test]
fn a_turn_that_is_not_about_a_draft_is_refused_and_decides_nothing() {
    let f = Fixture::new();
    for origin in [
        ConversationOrigin::stub_artifact("t", "a.md", false),
        ConversationOrigin::stub_note("t", "n1"),
    ] {
        let refused = block_on(f.tool_for(origin, &QUIET).call(args(&[(f.hunk(1), "accept")])))
            .expect_err("refused");
        let error = refused.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::PermissionDenied);
        assert_eq!(error.retryable(), Some(false));
        assert_eq!(refused.to_string(), decide_changes::NOT_A_DRAFT_CONVERSATION);
    }
    assert_eq!(f.state_of(1), HunkState::Pending);
}

// DDC-FR-OEJB
#[test]
fn with_no_project_open_the_call_is_the_shared_refusal() {
    let app = closed_project();
    let dir = TempDir::new().expect("tempdir");
    let root = crate::fs::RootFs::for_root(dir.path());
    let tool = DecideDraftChangesTool::new(
        app.handle().clone(),
        OwnedRoots { worktree: root.clone(), store: root },
        ConversationOrigin::stub_draft("t", "d1", false),
        agent(),
    )
    .for_test(human(), &QUIET);
    let refused = block_on(tool.call(args(&[("h", "accept")]))).expect_err("refused");
    assert_eq!(refused, CallRefusal::NoProjectOpen);
    let error = refused.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert_eq!(error.retryable(), Some(false));
}

// DDC-FR-WRFY
#[test]
fn a_malformed_list_is_refused_and_nothing_is_decided() {
    let f = Fixture::new();
    let many: Vec<(&str, &str)> = (0..51).map(|_| (f.hunk(1), "reject")).collect();
    let cases = vec![
        DecideChangesArgs { decisions: None },
        DecideChangesArgs { decisions: Some(Vec::new()) },
        args(&many),
        DecideChangesArgs {
            decisions: Some(vec![DecisionArg { id: None, action: Some("accept".into()) }]),
        },
        DecideChangesArgs {
            decisions: Some(vec![
                DecisionArg { id: Some(f.hunk(1).into()), action: Some("reject".into()) },
                DecisionArg { id: Some(f.hunk(2).into()), action: None },
            ]),
        },
    ];
    for case in cases {
        let refused = block_on(f.tool_for(f.origin(), &QUIET).call(case)).expect_err("malformed");
        assert_eq!(refused, CallRefusal::Malformed);
        let error = refused.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert_eq!(error.retryable(), Some(true));
    }
    assert_eq!(f.state_of(1), HunkState::Pending);
    assert_eq!(f.state_of(2), HunkState::Pending);
}

// DDC-FR-ZGXF
#[test]
fn a_draft_with_no_open_proposal_is_refused_for_the_whole_call() {
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "reject"), (f.hunk(3), "reject")]).expect("closes");
    let refused = f.call(&[(f.hunk(1), "accept")]).expect_err("nothing open");
    assert_eq!(refused, CallRefusal::NoOpenProposal(decide_changes::NO_OPEN_DRAFT_PROPOSAL));
    let error = refused.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
    assert_eq!(error.retryable(), Some(true));
    assert_eq!(f.prompt(), ORIGINAL);
}

// ---------------------------------------------------------------------------
// The actions
// ---------------------------------------------------------------------------

// DDC-FR-VUJQ, DDC-FR-IBYT, DCP-FR-CIPW, DCP-FR-LAWR
#[test]
fn accept_writes_the_hunk_and_records_one_version() {
    let f = Fixture::new();
    let events = Arc::new(Mutex::new(0usize));
    let seen = Arc::clone(&events);
    f.open.app.listen(crate::draft_proposals::PROPOSALS_CHANGED, move |_| {
        *seen.lock().unwrap() += 1;
    });
    let before = f.versions();
    let output = f.call(&[(f.hunk(2), "accept")]).expect("decided");
    assert_eq!(output.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(output.decisions[0].hunk_state, Some(HunkStateView::Accepted));
    assert_eq!(output.decisions[0].comment_owed, None);
    assert_eq!(f.prompt(), "# Spec\n\nAlpha line.\n\nBeta better.\n\nGamma line.\n");
    // DHS-FR-08: the first acceptance also settles the `Original` it replaced.
    assert_eq!(f.versions(), before + 2, "the original and one version");
    f.call(&[(f.hunk(3), "accept")]).expect("second");
    assert_eq!(f.versions(), before + 3, "one version for each accepted hunk");
    // DDC-FR-IBYT: the event of DCP-FR-16, as the chip's decision emits it.
    assert!(*events.lock().unwrap() >= 1);
    assert!(!output.closed);
    assert_eq!(output.proposal_state, ProposalStateView::Pending);
}

// DDC-FR-YKPP
#[test]
fn a_rejected_hunk_is_accepted_while_the_proposal_is_open() {
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "reject")]).expect("rejected");
    let output = f.call(&[(f.hunk(1), "accept")]).expect("accepted");
    assert_eq!(output.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(f.state_of(1), HunkState::Accepted);
    assert!(f.prompt().contains("Alpha better."));
}

// DDC-FR-QFNQ
#[test]
fn reject_writes_nothing_into_the_draft() {
    let f = Fixture::new();
    let before = f.versions();
    let output = f.call(&[(f.hunk(3), "reject")]).expect("rejected");
    assert_eq!(output.decisions[0].outcome, Outcome::Rejected);
    assert_eq!(output.decisions[0].hunk_state, Some(HunkStateView::Rejected));
    assert_eq!(f.prompt(), ORIGINAL);
    assert_eq!(f.versions(), before);
}

// DDC-FR-EEAZ, DDC-FR-ILCB, DDC-FR-NKIA, DDC-FR-NLDN
#[test]
fn discuss_opens_a_new_hunk_discussion_as_the_agent_and_holds_a_pending_hunk() {
    let f = Fixture::new();
    let before = f.discussions().len();
    let output = f.call(&[(f.hunk(1), "discuss")]).expect("opened");
    let row = &output.decisions[0];
    assert_eq!(row.outcome, Outcome::DiscussionOpened);
    assert_eq!(row.hunk_state, Some(HunkStateView::Discussing));
    let id = row.discussion_id.clone().expect("discussion id");
    assert_ne!(id, f.thread_id, "a new discussion, not a reply");
    assert_eq!(f.discussions().len(), before + 1);
    let opened = f.discussions().into_iter().find(|d| d.id == id).expect("opened");
    assert!(opened.fragment_target.is_none());
    // DDC-FR-ILCB: one comment, the fixed line, by the turn's agent.
    assert_eq!(opened.comments.len(), 1);
    let opening = &opened.comments[0];
    assert_eq!(opening.body, decide_changes::DISCUSSION_BODY);
    assert_eq!(opening.author, agent());
    assert_eq!(
        opening.attachments,
        vec![Attachment::ProposalHunk {
            proposal_id: f.proposal_id.clone(),
            hunk_id: f.hunk(1).to_string(),
            draft_id: Some(f.draft_id.clone()),
            artifact_id: None,
            path: FILE.into(),
        }]
    );
    // DDC-FR-NKIA: the pending hunk is held.
    assert_eq!(f.state_of(1), HunkState::Discussing);
    // DDC-FR-NLDN: the calling conversation took no comment from the tool.
    assert_eq!(f.thread().comments.len(), 2, "the opening and the proposal");
    assert_eq!(f.prompt(), ORIGINAL);
}

// DDC-FR-NKIA
#[test]
fn discuss_of_a_rejected_hunk_keeps_it_rejected() {
    let f = Fixture::new();
    f.call(&[(f.hunk(2), "reject")]).expect("rejected");
    let output = f.call(&[(f.hunk(2), "discuss")]).expect("opened");
    assert_eq!(output.decisions[0].outcome, Outcome::DiscussionOpened);
    assert_eq!(f.state_of(2), HunkState::Rejected);
}

// ---------------------------------------------------------------------------
// The entry refusals
// ---------------------------------------------------------------------------

// DDC-FR-JCSV, DDC-FR-SLBB, DDC-FR-WSIA
#[test]
fn an_entry_is_refused_with_the_code_that_applies_first() {
    let f = Fixture::new();
    // A withdrawn hunk, written as an update leaves it.
    crate::draft_proposals::set_hunk_state(
        &f.root(),
        &f.draft_id,
        &f.proposal_id,
        f.hunk(3),
        HunkState::Withdrawn,
    )
    .expect("withdrawn");
    f.call(&[(f.hunk(1), "accept")]).expect("accepted");
    let output = f
        .call(&[
            (f.hunk(2), "approve"),
            ("no-such-hunk", "accept"),
            (f.hunk(3), "accept"),
            (f.hunk(1), "reject"),
            ("no-such-hunk", "maybe"),
        ])
        .expect("every entry refused, and the call still answers");
    assert_eq!(output.decisions.len(), 5, "one row per entry, in call order");
    assert_eq!(code_of(&output, 1), Some(RefusalCode::InvalidAction));
    assert_eq!(code_of(&output, 2), Some(RefusalCode::UnknownHunk));
    assert_eq!(output.decisions[1].hunk_state, None, "no state for an unknown hunk");
    assert_eq!(code_of(&output, 3), Some(RefusalCode::HunkWithdrawn));
    assert_eq!(code_of(&output, 4), Some(RefusalCode::HunkAccepted));
    // invalid_action applies before unknown_hunk.
    assert_eq!(code_of(&output, 5), Some(RefusalCode::InvalidAction));
    for (index, row) in output.decisions.iter().enumerate() {
        assert_eq!(row.place, index + 1);
        assert_eq!(row.outcome, Outcome::Refused);
        let refusal = row.refusal.as_ref().expect("refusal");
        assert!(refusal.retryable, "DDC-FR-SLBB: every one of these is retryable");
        assert!(
            refusal.message.starts_with(&format!("Decision {} of 5: ", index + 1)),
            "{}",
            refusal.message
        );
    }
    assert_eq!(f.state_of(2), HunkState::Pending);
}

// DDC-FR-JCSV — the confirmed rule: a lost hunk is refused for every action.
#[test]
fn a_lost_hunk_is_refused_for_every_action() {
    let f = Fixture::new();
    // The author rewrites the line the second hunk changes.
    crate::drafts::save_draft_file_impl(
        &f.root(),
        &f.draft_id,
        FILE,
        "# Spec\n\nAlpha line.\n\nSomething else.\n\nGamma line.\n",
    )
    .expect("edit");
    let before = f.discussions().len();
    let output = f
        .call(&[(f.hunk(2), "accept"), (f.hunk(2), "reject"), (f.hunk(2), "discuss")])
        .expect("answers");
    for place in 1..=3 {
        assert_eq!(code_of(&output, place), Some(RefusalCode::HunkLost));
        let message = &output.decisions[place - 1].refusal.as_ref().expect("refusal").message;
        assert!(message.contains("no longer in the prompt"), "{message}");
    }
    assert_eq!(f.state_of(2), HunkState::Pending);
    assert_eq!(f.discussions().len(), before);
}

// DDC-FR-EDMT
#[test]
fn a_reject_of_a_rejected_hunk_and_a_second_discussion_are_already_in_state() {
    let f = Fixture::new();
    f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "discuss")]).expect("first");
    // A resolved hunk discussion still counts (DCR-FR-VBQN).
    let opened = f
        .discussions()
        .into_iter()
        .find(|d| decide_changes::is_hunk_discussion(d, &f.proposal_id, f.hunk(2)))
        .expect("hunk discussion");
    comments::set_resolution_to(
        &f.root(),
        ThreadRef::discussion(&f.draft_id),
        &opened.id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let before = f.discussions().len();
    let output = f.call(&[(f.hunk(1), "reject"), (f.hunk(2), "discuss")]).expect("answers");
    assert_eq!(code_of(&output, 1), Some(RefusalCode::AlreadyInState));
    assert_eq!(code_of(&output, 2), Some(RefusalCode::AlreadyInState));
    let first = &output.decisions[0].refusal.as_ref().expect("refusal").message;
    assert!(first.contains("already rejected"), "{first}");
    let second = &output.decisions[1].refusal.as_ref().expect("refusal").message;
    assert!(second.contains("already has a discussion"), "{second}");
    assert_eq!(f.discussions().len(), before);
}

// DDC-FR-XBQV, DDC-FR-IBYT, DDC-FR-KQZN
#[test]
fn an_entry_after_the_close_is_refused_and_the_close_is_reported() {
    let f = Fixture::new();
    let output = f
        .call(&[
            (f.hunk(1), "reject"),
            (f.hunk(2), "reject"),
            (f.hunk(3), "reject"),
            (f.hunk(1), "accept"),
        ])
        .expect("answers");
    assert!(output.closed);
    assert_eq!(output.proposal_state, ProposalStateView::Rejected);
    assert_eq!(code_of(&output, 4), Some(RefusalCode::NoOpenProposal));
    assert!(output.decisions[3].refusal.as_ref().expect("refusal").retryable);
    // DDC-FR-IBYT: the close appends the decision comment as the author.
    let thread = f.thread();
    let last = thread.comments.last().expect("decision comment");
    assert_eq!(last.author, human());
    assert_eq!(thread.comments.len(), 3);
    assert_eq!(f.prompt(), ORIGINAL);
}

// DDC-FR-LYTA
#[test]
fn a_refusal_of_the_backend_is_decision_failed_with_its_typed_error() {
    let f = Fixture::new();
    // The proposal's conversation is resolved, so no decision comment can go in.
    comments::set_resolution_to(
        &f.root(),
        ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let output = f.call(&[(f.hunk(1), "accept"), (f.hunk(2), "reject")]).expect("answers");
    for place in 1..=2 {
        let refusal = output.decisions[place - 1].refusal.as_ref().expect("refusal");
        assert_eq!(refusal.code, RefusalCode::DecisionFailed);
        assert_eq!(refusal.error.as_deref(), Some(comments::ERR_DISCUSSION_RESOLVED));
        assert!(!refusal.retryable);
        assert!(refusal.message.starts_with(&format!("Decision {place} of 2: ")));
    }
    assert_eq!(f.prompt(), ORIGINAL);
    // The retryable set is the three of DDC-FR-LYTA.
    assert_eq!(
        RETRYABLE_ERRORS,
        ["write_failed", "acceptance_in_progress", "history_recovery_failed"]
    );
}

// DDC-FR-DSTU
#[test]
fn entries_run_in_order_and_a_refused_entry_stops_no_other() {
    let f = Fixture::new();
    let output = f
        .call(&[(f.hunk(1), "accept"), (f.hunk(1), "accept"), (f.hunk(3), "accept")])
        .expect("answers");
    assert_eq!(output.decisions[0].outcome, Outcome::Accepted);
    // The second entry reads the proposal after the first.
    assert_eq!(code_of(&output, 2), Some(RefusalCode::HunkAccepted));
    assert_eq!(output.decisions[2].outcome, Outcome::Accepted);
    assert_eq!(f.prompt(), "# Spec\n\nAlpha better.\n\nBeta line.\n\nGamma better.\n");
}

// DDC-FR-HUJM
#[test]
fn reject_all_is_one_reject_per_hunk_and_no_hunk_is_rewritten() {
    let f = Fixture::new();
    let hunks_before =
        crate::draft_proposals::load_hunks_impl(&f.root(), &f.proposal_id).expect("hunks");
    let output = f
        .call(&[(f.hunk(1), "reject"), (f.hunk(2), "reject"), (f.hunk(3), "reject")])
        .expect("answers");
    assert!(output.decisions.iter().all(|row| row.outcome == Outcome::Rejected));
    let hunks_after =
        crate::draft_proposals::load_hunks_impl(&f.root(), &f.proposal_id).expect("hunks");
    assert_eq!(hunks_before.checksum, hunks_after.checksum, "no hunk text changed");
    assert_eq!(f.record().ledger.len(), 3, "no hunk added or withdrawn");
    // The tool reaches none of the operations that make, change, or decline.
    let source = include_str!("../decide_draft_changes.rs");
    for forbidden in
        ["record_proposal", "update_proposal", "edit_hunk_impl", "decline_draft_change_proposal"]
    {
        assert!(!source.contains(forbidden), "the tool reaches {forbidden}");
    }
}

// DDC-FR-KQZN, DDC-FR-PWTE, DDC-FR-NLDN
#[test]
fn the_tool_dispatches_no_turn_and_does_not_end_the_turn() {
    let tool = include_str!("../decide_draft_changes.rs");
    let shared = include_str!("../decide_changes.rs");
    for source in [tool, shared] {
        assert!(!source.contains("dispatch_impl"));
        assert!(!source.contains("dispatch_agent_turn("));
        assert!(!source.contains("AtomicBool"), "no flag reaches the loop to end the turn");
    }
    // CVL-FR-15: the loop's turn-ending set does not name it.
    let loop_body = include_str!("../../agent_conversations/turn_body.rs");
    assert!(!loop_body.contains("decide_draft_changes"));
}

// DDC-FR-TBVA
#[test]
fn each_call_logs_one_info_record_and_one_warn_record_per_refused_entry() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    f.call_into(&BUFFER, &[(f.hunk(1), "accept"), ("nope", "reject"), (f.hunk(2), "discuss")])
        .expect("answers");
    let records = log_records(&BUFFER);
    let info: Vec<_> = records.iter().filter(|r| r.level == LogLevel::Info).collect();
    assert_eq!(info.len(), 1);
    assert!(info[0].domains.contains(&Domain::Backend));
    let fields = serde_json::to_value(&info[0].fields).expect("fields");
    assert_eq!(fields["tool"], NAME);
    assert_eq!(fields["draftId"], f.draft_id);
    assert_eq!(fields["accepted"], 1);
    assert_eq!(fields["refused"], 1);
    assert_eq!(fields["discussion_opened"], 1);
    let warns: Vec<_> = records.iter().filter(|r| r.level == LogLevel::Warn).collect();
    assert_eq!(warns.len(), 1);
    let warn = serde_json::to_value(&warns[0].fields).expect("fields");
    assert_eq!(warn["code"], "unknown_hunk");
    assert_eq!(warn["place"], 2);
    // No hunk text, no comment text, no input.
    let all = serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>())
        .expect("json");
    for secret in ["Alpha", "Beta", "better", decide_changes::DISCUSSION_BODY, "nope"] {
        assert!(!all.contains(secret), "a record carries {secret}");
    }

    // A whole-call refusal is one WARN.
    BUFFER.clear();
    block_on(f.tool_for(f.origin(), &BUFFER).call(DecideChangesArgs { decisions: None }))
        .expect_err("malformed");
    let records = log_records(&BUFFER);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].level, LogLevel::Warn);
}

// DDC-FR-GXEW
#[test]
fn the_tool_holds_no_state_between_calls_and_resolves_at_once() {
    let f = Fixture::new();
    let tool = f.tool_for(f.origin(), &QUIET);
    // `block_on` fails a future that is not ready on its first poll, so no
    // network wait and no timer stands in a call.
    let first = block_on(tool.call(args(&[(f.hunk(1), "reject")]))).expect("first");
    let second = block_on(tool.call(args(&[(f.hunk(1), "reject")]))).expect("second");
    assert_eq!(first.decisions[0].outcome, Outcome::Rejected);
    // The second call reads storage, not anything the first call kept.
    assert_eq!(code_of(&second, 1), Some(RefusalCode::AlreadyInState));
    let source = include_str!("../decide_draft_changes.rs");
    assert!(!source.contains("reqwest") && !source.contains("Mutex<"));
}
