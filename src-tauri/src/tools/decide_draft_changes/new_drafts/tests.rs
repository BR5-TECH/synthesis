//! Tests for the new-draft entries of `DDC-decide-draft-changes-tool.md`.

use rig::tool::PortableTool;
use tempfile::TempDir;

use super::*;
use crate::comments::{DiscussionTarget, ThreadRef};
use crate::logging::LogBuffer;
use crate::tools::decide_changes::{DecideChangesArgs, DecisionArg};
use crate::tools::tests::{block_on, mounted};

static QUIET: LogBuffer = LogBuffer::new();

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent { agent_id: "a".into(), handle: "arch".into(), model: None, title: None }
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let draft_id = crate::drafts::create_draft_at_root(&root, Some("relay")).unwrap().draft.id;
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: draft_id.clone() },
            None,
            "@arch decompose".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .unwrap();
        Fixture { open: mounted(dir), path, draft_id, thread_id: thread.id }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn propose(&self, name: &str) -> String {
        nd::record_new_draft_proposal(
            &self.open.handle(),
            &self.root(),
            &self.root(),
            nd::NewDraftInput {
                draft_id: &self.draft_id,
                name,
                body: "body",
                position: 1,
                total: 2,
                rationale: "<spoken>x</spoken>",
                agent: &agent(),
                target: ThreadRef::discussion(&self.draft_id),
                thread_id: &self.thread_id,
            },
        )
        .unwrap()
        .id
    }

    fn call(&self, decisions: &[(&str, &str)]) -> Result<DecideChangesOutput, CallRefusal> {
        let origin =
            ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false);
        let turn =
            crate::tools::tests::authorized_turn_for(&self.open.handle(), &self.root(), &origin);
        let tool = DecideDraftChangesTool::new(
            self.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() },
            origin,
            agent(),
            &turn,
        )
        .for_test(human(), &QUIET);
        block_on(
            tool.call(DecideChangesArgs {
                instruction: Some(crate::tools::tests::DECISION_INSTRUCTION.into()),
                decisions: Some(
                    decisions
                        .iter()
                        .map(|(id, action)| DecisionArg {
                            id: Some(id.to_string()),
                            action: Some(action.to_string()),
                        })
                        .collect(),
                ),
            }),
        )
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
    }

    /// One call on `turn`, with `instruction`.
    fn call_as(
        &self,
        turn: &str,
        instruction: Option<&str>,
        decisions: &[(&str, &str)],
    ) -> Result<DecideChangesOutput, CallRefusal> {
        let tool = DecideDraftChangesTool::new(
            self.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() },
            self.origin(),
            agent(),
            turn,
        )
        .for_test(human(), &QUIET);
        block_on(
            tool.call(DecideChangesArgs {
                instruction: instruction.map(str::to_string),
                decisions: Some(
                    decisions
                        .iter()
                        .map(|(id, action)| DecisionArg {
                            id: Some(id.to_string()),
                            action: Some(action.to_string()),
                        })
                        .collect(),
                ),
            }),
        )
    }

    /// An author comment with `body`, and a turn it triggers.
    fn author_turn(&self, body: &str) -> (String, String) {
        let id = crate::notes::new_note_id();
        comments::add_comment_with_id(
            &self.root(),
            ThreadRef::discussion(&self.draft_id),
            &self.thread_id,
            id.clone(),
            body.to_string(),
            &human(),
            "2026-01-01T00:00:02Z",
        )
        .unwrap();
        let turn = crate::agent_conversations::authority_for_test::register_turn(
            &self.open.handle(),
            self.origin(),
            Some(&id),
            None,
        );
        // Noted after the turn registers, which manages the registry of the
        // test app.
        crate::agent_conversations::note_author_comment(&self.open.handle(), &id);
        (id, turn)
    }

    fn set_id(&self) -> String {
        nd::list_impl(&self.root(), &self.draft_id).unwrap()[0].set_id.clone()
    }

    fn thread(&self) -> comments::Discussion {
        self.discussions().into_iter().find(|d| d.id == self.thread_id).unwrap()
    }

    fn state(&self, id: &str) -> NewDraftState {
        nd::list_impl(&self.root(), &self.draft_id)
            .unwrap()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap()
            .state
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(
            &self.root(),
            &DiscussionTarget::Draft { draft_id: self.draft_id.clone() },
        )
    }
}

// DDC-FR-ZGXF: with neither kind of proposal open the whole call is refused.
#[test]
fn a_call_with_nothing_open_is_refused() {
    let f = Fixture::new();
    assert!(matches!(f.call(&[("x", "accept")]), Err(CallRefusal::NoOpenProposal(_))));
}

// DDC-FR-GGWO / DDC-FR-ZXML / DDC-FR-NWRG: an accept entry runs the card's
// Accept with no feedback, creates the draft, and the output names no change
// proposal state.
#[test]
fn an_accept_entry_creates_the_draft_and_reports_it() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let out = f.call(&[(&id, "accept")]).unwrap();
    assert!(out.proposal_state.is_none());
    assert!(!out.closed);
    let row = &out.decisions[0];
    assert_eq!(row.outcome, Outcome::Accepted);
    assert_eq!(row.new_draft_state, Some("accepted"));
    let created = row.created_draft_id.clone().unwrap();
    assert!(crate::drafts::read_draft_record(&f.root(), &created).is_ok());
    assert_eq!(f.state(&id), NewDraftState::Accepted);
    let json = serde_json::to_value(&out).unwrap();
    assert!(json.get("proposal_state").is_none());
}

// DDC-FR-ZXML / DDC-FR-ZWKS: a reject entry creates nothing, and a second entry
// on the same proposal is refused with `no_open_proposal`.
#[test]
fn a_reject_entry_then_a_second_entry_is_refused() {
    let f = Fixture::new();
    let id = f.propose("a");
    let count = crate::drafts::list_drafts_impl(&f.root()).drafts.len();
    let out = f.call(&[(&id, "reject"), (&id, "accept")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Rejected);
    let refusal = out.decisions[1].refusal.as_ref().unwrap();
    assert_eq!(refusal.code, RefusalCode::NoOpenProposal);
    assert!(refusal.message.starts_with("Decision 2 of 2:"));
    assert_eq!(crate::drafts::list_drafts_impl(&f.root()).drafts.len(), count);
}

// DDC-FR-YDUQ / DDC-FR-ZWKS: discuss opens one discussion by the agent with the
// fixed line and one `new_draft_subject`; a second discuss is refused.
#[test]
fn a_discuss_entry_opens_one_discussion_and_decides_nothing() {
    let f = Fixture::new();
    let id = f.propose("a");
    let out = f.call(&[(&id, "discuss")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::DiscussionOpened);
    let opened = f
        .discussions()
        .into_iter()
        .find(|d| Some(&d.id) == out.decisions[0].discussion_id.as_ref())
        .unwrap();
    let first = &opened.comments[0];
    assert_eq!(first.body, NEW_DRAFT_DISCUSSION_BODY);
    assert_eq!(first.author, agent());
    assert_eq!(
        first.attachments,
        vec![comments::Attachment::NewDraftSubject {
            proposal_id: id.clone(),
            draft_id: f.draft_id.clone()
        }]
    );
    assert_eq!(f.state(&id), NewDraftState::Pending);
    let again = f.call(&[(&id, "discuss")]).unwrap();
    assert_eq!(again.decisions[0].refusal.as_ref().unwrap().code, RefusalCode::AlreadyInState);
}

// DDC-FR-GGWO: an id that names neither a hunk nor the pending proposal is
// `unknown_hunk`, a proposal decided before the call included.
#[test]
fn an_id_of_no_pending_proposal_is_unknown() {
    let f = Fixture::new();
    let old = f.propose("a");
    f.call(&[(&old, "reject")]).unwrap();
    let id = f.propose("b");
    let out = f.call(&[("nope", "accept"), (&old, "accept"), (&id, "launch")]).unwrap();
    assert_eq!(out.decisions[0].refusal.as_ref().unwrap().code, RefusalCode::UnknownHunk);
    assert_eq!(out.decisions[1].refusal.as_ref().unwrap().code, RefusalCode::UnknownHunk);
    assert_eq!(out.decisions[2].refusal.as_ref().unwrap().code, RefusalCode::InvalidAction);
}

// DDC-FR-ZXML / DDC-FR-LYTA: an archived original refuses with
// `decision_failed`, naming the backend's error, and not retryable.
#[test]
fn an_archived_original_is_decision_failed() {
    let f = Fixture::new();
    let id = f.propose("a");
    crate::drafts::set_draft_status_impl(
        &f.root(),
        &f.draft_id,
        crate::drafts::DraftStatus::Archived,
    )
    .unwrap();
    let out = f.call(&[(&id, "accept")]).unwrap();
    let refusal = out.decisions[0].refusal.as_ref().unwrap();
    assert_eq!(refusal.code, RefusalCode::DecisionFailed);
    assert_eq!(refusal.error.as_deref(), Some(nd::ERR_DRAFT_ARCHIVED));
    assert!(!refusal.retryable);
}

// DDC-FR-VEKP, DDC-FR-ROLG, DDC-FR-OKUI: the description says how to confirm
// a decomposition and how to give the instruction, and that the tool never
// proposes a draft.
#[test]
fn the_description_says_how_to_confirm_and_to_quote_the_author() {
    let text = super::super::DESCRIPTION;
    assert!(text.contains("give the id of the decomposition with `confirm`"));
    assert!(text.contains("Copy into `instruction` the words of the author's latest comment"));
    assert!(text.contains("wait until the comment of the speech arrives"));
    assert!(text.contains("propose_new_draft"));
}

// DDC-FR-GGWO: one call that names a hunk of the open change proposal and the
// pending new-draft proposal runs each on its own terms.
#[test]
fn a_call_with_a_hunk_and_a_new_draft_runs_both() {
    let f = Fixture::new();
    let root = f.root();
    let path = crate::drafts::read_draft_record(&root, &f.draft_id).unwrap().prompt_path.unwrap();
    crate::drafts::save_draft_file_impl(&root, &f.draft_id, &path, "Loose notes.\n").unwrap();
    let change = crate::draft_proposals::record_proposal(
        &f.open.handle(),
        &root,
        &root,
        crate::draft_proposals::NewProposal {
            draft_id: &f.draft_id,
            path: &path,
            hunks: &[crate::draft_proposals::hunks::ProposedHunk::from_fields(
                Some("Loose notes.".into()),
                Some("Tight notes.".into()),
                None,
                None,
                None,
                None,
            )],
            rationale: "<spoken>x</spoken>",
            agent: &agent(),
            target: ThreadRef::discussion(&f.draft_id),
            thread_id: &f.thread_id,
        },
    )
    .unwrap();
    let hunk = change.ledger[0].id.clone();
    let id = f.propose("a");
    let out = f.call(&[(&hunk, "reject"), (&id, "accept")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Rejected);
    assert!(out.decisions[0].new_draft_state.is_none());
    assert_eq!(out.decisions[1].outcome, Outcome::Accepted);
    assert!(out.closed, "the change proposal closed");
    assert!(out.proposal_state.is_some());
    // DDC-FR-ZXML / DCP-FR-EJWZ: the decision comment is the author's, with no
    // feedback. DDC-FR-JJHZ: it ends with the instruction line.
    let thread = comments::locate_discussion_of(
        &root,
        &DiscussionTarget::Draft { draft_id: f.draft_id.clone() },
        &f.thread_id,
    )
    .unwrap();
    let last = thread.comments.last().unwrap();
    assert!(matches!(last.author, Participant::Human { .. }));
    assert!(last.body.starts_with("Accepted the proposed draft “a”"));
    assert!(last.body.ends_with("\n\nDecided by arch on the author's instruction: “arch”"));
    assert_eq!(last.body.matches("\n\n").count(), 1);
}

// ---------------------------------------------------------------------------
// Confirming a decomposition (DDC-FR-VEKP, DDC-FR-PEJA, DDC-FR-WITR)
// ---------------------------------------------------------------------------

// DDC-FR-VEKP / DDC-FR-WITR / DCP-FR-IKTU / DDC-FR-JJHZ / DCP-FR-HMUG: a
// `confirm` entry with the id of the decomposition confirms it, as the row's
// button does, appends the confirmation comment with the instruction line,
// and dispatches no turn.
#[test]
fn a_confirm_entry_with_the_decomposition_id_confirms_it() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    f.call(&[(&id, "accept")]).unwrap();
    let (_, turn) = f.author_turn("@arch Confirm the decomposition, it is done.");
    let turns_before = crate::agent_conversations::authority_for_test::live_turns(&f.open.handle());
    let out = f.call_as(&turn, Some("confirm the decomposition"), &[(&f.set_id(), "confirm")]);
    let out = out.expect("the call runs on a confirmable decomposition");
    assert_eq!(out.decisions[0].outcome, Outcome::Confirmed);
    let record = crate::drafts::read_draft_record(&f.root(), &f.draft_id).unwrap();
    assert_eq!(record.status, crate::drafts::DraftStatus::Archived);
    assert_eq!(record.superseded.expect("marker").set_id, f.set_id());
    let last = f.thread().comments.last().cloned().unwrap();
    assert!(matches!(last.author, Participant::Human { .. }));
    assert!(last.body.starts_with("Decomposition confirmed. This draft is superseded by"));
    assert!(last
        .body
        .ends_with("\n\nDecided by arch on the author's instruction: “confirm the decomposition”"));
    assert_eq!(
        crate::agent_conversations::authority_for_test::live_turns(&f.open.handle()),
        turns_before,
        "no agent turn is dispatched"
    );
}

// DDC-FR-VEKP / DDC-FR-JCSV: a `confirm` with another id is `unknown_hunk`,
// and another action with the decomposition id is `invalid_action`.
#[test]
fn confirm_and_the_decomposition_id_go_together() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    f.call(&[(&id, "accept")]).unwrap();
    let out = f.call(&[("not-the-set", "confirm"), (&f.set_id(), "accept")]).unwrap();
    let first = out.decisions[0].refusal.as_ref().unwrap();
    assert_eq!(first.code, RefusalCode::UnknownHunk);
    assert!(first.message.starts_with("Decision 1 of 2: "));
    assert!(first
        .message
        .contains("is not a change of the open proposal. Use an id from your input."));
    let second = out.decisions[1].refusal.as_ref().unwrap();
    assert_eq!(second.code, RefusalCode::InvalidAction);
    assert!(second.message.contains("`confirm` for the decomposition"));
    let record = crate::drafts::read_draft_record(&f.root(), &f.draft_id).unwrap();
    assert!(record.superseded.is_none());
}

// DDC-FR-PEJA: a backend refusal is `decision_failed` with the typed error,
// not retryable.
#[test]
fn a_confirm_with_a_pending_proposal_or_nothing_accepted_is_decision_failed() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let out = f.call(&[(&f.set_id(), "confirm")]).unwrap();
    let refusal = out.decisions[0].refusal.as_ref().unwrap();
    assert_eq!(refusal.code, RefusalCode::DecisionFailed);
    assert_eq!(refusal.error.as_deref(), Some(nd::ERR_PROPOSAL_PENDING));
    assert!(!refusal.retryable);

    f.call(&[(&id, "reject")]).unwrap();
    let out = f.call(&[(&f.set_id(), "confirm")]);
    // DDC-FR-ZGXF: nothing accepted and nothing pending, so nothing is open.
    assert!(matches!(out, Err(CallRefusal::NoOpenProposal(_))));
}

// DDC-FR-PEJA / DDC-FR-DSTU: an accept and then a confirm in one call confirm
// the decomposition with the accepted draft in it.
#[test]
fn an_accept_then_a_confirm_in_one_call_confirms_with_that_draft() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let out = f.call(&[(&id, "accept"), (&f.set_id(), "confirm")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(out.decisions[1].outcome, Outcome::Confirmed);
    let created = out.decisions[0].created_draft_id.clone().unwrap();
    let marker = crate::drafts::read_draft_record(&f.root(), &f.draft_id)
        .unwrap()
        .superseded
        .expect("marker");
    assert_eq!(marker.drafts.iter().map(|d| d.id.clone()).collect::<Vec<_>>(), vec![created]);
}

// DDC-FR-PEJA: with an open change proposal the call runs, and a confirm of
// a decomposition with nothing accepted is `decision_failed` with
// `nothing_accepted`, not retryable.
#[test]
fn a_confirm_with_nothing_accepted_is_decision_failed() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    f.call(&[(&id, "reject")]).unwrap();
    open_change_proposal(&f);
    let out = f.call(&[(&f.set_id(), "confirm")]).unwrap();
    let refusal = out.decisions[0].refusal.as_ref().unwrap();
    assert_eq!(refusal.code, RefusalCode::DecisionFailed);
    assert_eq!(refusal.error.as_deref(), Some(nd::ERR_NOTHING_ACCEPTED));
    assert!(!refusal.retryable);
}

// DDC-FR-PEJA: a refusal of DCP-FR-LTEZ, here an archived original, is
// `decision_failed`, not retryable, and writes no marker.
#[test]
fn a_confirm_on_an_archived_original_is_decision_failed() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    f.call(&[(&id, "accept")]).unwrap();
    crate::drafts::set_draft_status_impl(
        &f.root(),
        &f.draft_id,
        crate::drafts::DraftStatus::Archived,
    )
    .unwrap();
    let out = f.call(&[(&f.set_id(), "confirm")]).unwrap();
    let refusal = out.decisions[0].refusal.as_ref().unwrap();
    assert_eq!(refusal.code, RefusalCode::DecisionFailed);
    assert_eq!(refusal.error.as_deref(), Some(nd::ERR_DRAFT_ARCHIVED));
    assert!(!refusal.retryable);
    assert!(crate::drafts::read_draft_record(&f.root(), &f.draft_id).unwrap().superseded.is_none());
}

/// An open change proposal on the draft, so a call runs without a
/// confirmable decomposition (DDC-FR-ZGXF).
fn open_change_proposal(f: &Fixture) {
    let root = f.root();
    let path = crate::drafts::read_draft_record(&root, &f.draft_id).unwrap().prompt_path.unwrap();
    crate::drafts::save_draft_file_impl(&root, &f.draft_id, &path, "Alpha line.\n").unwrap();
    crate::draft_proposals::record_proposal(
        &f.open.handle(),
        &root,
        &root,
        crate::draft_proposals::NewProposal {
            draft_id: &f.draft_id,
            path: &path,
            hunks: &[crate::draft_proposals::hunks::ProposedHunk::from_fields(
                Some("Alpha line.".into()),
                Some("Alpha better.".into()),
                None,
                None,
                None,
                None,
            )],
            rationale: "<spoken>x</spoken>",
            agent: &agent(),
            target: ThreadRef::discussion(&f.draft_id),
            thread_id: &f.thread_id,
        },
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Authority (DDC-FR-PZBH, DDC-FR-ELAE, DDC-FR-ZYOL, DDC-FR-NKAB, DDC-FR-VONX,
// DDC-FR-VYLJ)
// ---------------------------------------------------------------------------

// DDC-FR-PZBH: an absent, blank, or too long `instruction` is the malformed
// arguments refusal, and nothing is decided.
#[test]
fn a_missing_or_malformed_instruction_is_refused_as_malformed() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let (_, turn) = f.author_turn("accept it");
    let long = "a".repeat(501);
    for instruction in [None, Some(""), Some("  "), Some("?!"), Some(long.as_str())] {
        let refused = f.call_as(&turn, instruction, &[(&id, "accept")]).expect_err("malformed");
        assert_eq!(refused, CallRefusal::Malformed);
        let error = refused.to_execution_error();
        assert_eq!(error.kind(), rig::tool::ToolErrorKind::InvalidArgs);
        assert_eq!(error.retryable(), Some(true));
    }
    assert_eq!(f.state(&id), NewDraftState::Pending);
}

// DDC-FR-ELAE / DDC-FR-IHAY: a turn whose newest comment is not an author
// comment decides nothing, for every action.
#[test]
fn a_turn_without_an_author_comment_decides_nothing() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        Some(&f.thread().comments[0].id),
        None,
    );
    for action in ["accept", "reject", "discuss"] {
        let refused = f.call_as(&turn, Some("decompose"), &[(&id, action)]).expect_err("refused");
        assert_eq!(
            refused,
            CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::NotAuthorized)
        );
        let error = refused.to_execution_error();
        assert_eq!(error.kind(), rig::tool::ToolErrorKind::PermissionDenied);
        assert_eq!(error.retryable(), Some(false));
        assert_eq!(refused.to_string(), decide_changes::NOT_AUTHORIZED);
    }
    assert_eq!(f.state(&id), NewDraftState::Pending);
    assert_eq!(f.discussions().len(), 1, "no discussion opened");
}

// DDC-FR-ZYOL: a call while the turn's speech is open waits for it.
#[test]
fn a_call_during_an_open_speech_is_refused_and_retryable() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        None,
        Some("speech-1"),
    );
    let refused = f.call_as(&turn, Some("accept it"), &[(&id, "accept")]).expect_err("refused");
    let error = refused.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(error.retryable(), Some(true));
    assert_eq!(refused.to_string(), decide_changes::SPEECH_NOT_POSTED);
    assert_eq!(f.state(&id), NewDraftState::Pending);
}

// DDC-FR-NKAB: words that are not in the comment are refused, retryable.
#[test]
fn an_instruction_that_is_not_in_the_comment_is_refused() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let (_, turn) = f.author_turn("@arch do not accept it yet");
    let refused = f.call_as(&turn, Some("accept it now"), &[(&id, "accept")]).expect_err("x");
    let error = refused.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::InvalidArgs);
    assert_eq!(error.retryable(), Some(true));
    assert_eq!(refused.to_string(), decide_changes::INSTRUCTION_NOT_FOUND);
    assert_eq!(f.state(&id), NewDraftState::Pending);
}

// DDC-FR-VONX: a call that decided an entry spends the comment, and a second
// call on it is refused, not retryable.
#[test]
fn a_comment_authorizes_one_call_that_decides() {
    let f = Fixture::new();
    let first = f.propose("relay-auth");
    let (comment, turn) = f.author_turn("@arch reject it");
    f.call_as(&turn, Some("reject it"), &[(&first, "reject")]).unwrap();
    assert!(crate::agent_conversations::authority_for_test::is_spent(&f.open.handle(), &comment));
    let second = f.propose("relay-auth-2");
    let refused = f.call_as(&turn, Some("reject it"), &[(&second, "reject")]).expect_err("used");
    let error = refused.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::PermissionDenied);
    assert_eq!(error.retryable(), Some(false));
    assert_eq!(refused.to_string(), decide_changes::INSTRUCTION_USED);
    assert_eq!(f.state(&second), NewDraftState::Pending);
}

// DDC-FR-VONX: a call whose every entry is refused leaves the comment
// unspent, so the agent corrects the id and calls again.
#[test]
fn a_call_that_decides_nothing_leaves_the_comment_unspent() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let (comment, turn) = f.author_turn("@arch accept it");
    let out = f.call_as(&turn, Some("accept it"), &[("wrong-id", "accept")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Refused);
    assert!(!crate::agent_conversations::authority_for_test::is_spent(&f.open.handle(), &comment));
    let out = f.call_as(&turn, Some("accept it"), &[(&id, "accept")]).unwrap();
    assert_eq!(out.decisions[0].outcome, Outcome::Accepted);
}

// DDC-FR-VYLJ: malformed arguments come before the authority, and the
// authority comes before the no-open-proposal refusal.
#[test]
fn the_whole_call_refusals_apply_in_order() {
    let f = Fixture::new();
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        None,
        None,
    );
    // Nothing open, no authority, and no instruction: malformed first.
    assert_eq!(f.call_as(&turn, None, &[("x", "accept")]), Err(CallRefusal::Malformed));
    // Nothing open and no authority: the authority refusal.
    assert_eq!(
        f.call_as(&turn, Some("accept"), &[("x", "accept")]),
        Err(CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::NotAuthorized))
    );
    // Authorized, and nothing open: the no-open-proposal refusal.
    let (comment, authorized) = f.author_turn("accept");
    assert!(matches!(
        f.call_as(&authorized, Some("accept"), &[("x", "accept")]),
        Err(CallRefusal::NoOpenProposal(_))
    ));
    assert!(!crate::agent_conversations::authority_for_test::is_spent(&f.open.handle(), &comment));
}

// DDC-FR-VYLJ: the wrong conversation comes before malformed arguments.
#[test]
fn a_wrong_conversation_is_refused_before_malformed_arguments() {
    let f = Fixture::new();
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        None,
        None,
    );
    let tool = DecideDraftChangesTool::new(
        f.open.handle(),
        crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
        ConversationOrigin::stub_artifact(f.thread_id.clone(), "spec.md", false),
        agent(),
        &turn,
    )
    .for_test(human(), &QUIET);
    let refused = block_on(tool.call(DecideChangesArgs { instruction: None, decisions: None }));
    assert_eq!(
        refused,
        Err(CallRefusal::WrongConversation(decide_changes::NOT_A_DRAFT_CONVERSATION))
    );
}

// DDC-FR-VYLJ: not authorized comes before instruction not found, and
// instruction used comes before no open proposal.
#[test]
fn the_authority_refusals_apply_in_order() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        Some(&f.thread().comments[0].id),
        None,
    );
    assert_eq!(
        f.call_as(&turn, Some("words that are nowhere"), &[(&id, "accept")]),
        Err(CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::NotAuthorized))
    );
    // Spend a comment on a decision, which leaves nothing open.
    let (_, spent) = f.author_turn("@arch reject it");
    f.call_as(&spent, Some("reject it"), &[(&id, "reject")]).unwrap();
    assert_eq!(
        f.call_as(&spent, Some("reject it"), &[("x", "reject")]),
        Err(CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::InstructionUsed))
    );
}

// DDC-FR-IHAY: the authority rules hold for `confirm` too.
#[test]
fn a_confirm_without_the_author_s_authority_confirms_nothing() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    f.call(&[(&id, "accept")]).unwrap();
    let turn = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        None,
        None,
    );
    let refused = f.call_as(&turn, Some("confirm"), &[(&f.set_id(), "confirm")]);
    assert_eq!(
        refused,
        Err(CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::NotAuthorized))
    );
    assert!(crate::drafts::read_draft_record(&f.root(), &f.draft_id).unwrap().superseded.is_none());
}

// DDC-FR-CMRE: two tool calls on two turns that one comment triggered, at the
// same time: exactly one decides, and the other changes nothing.
#[test]
fn two_concurrent_tool_calls_on_one_comment_decide_once() {
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let (comment, first) = f.author_turn("@arch accept it");
    let second = crate::agent_conversations::authority_for_test::register_turn(
        &f.open.handle(),
        f.origin(),
        Some(&comment),
        None,
    );
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = [first, second]
        .into_iter()
        .map(|turn| {
            let tool = DecideDraftChangesTool::new(
                f.open.handle(),
                crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
                f.origin(),
                agent(),
                &turn,
            )
            .for_test(human(), &QUIET);
            let barrier = std::sync::Arc::clone(&barrier);
            let id = id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                block_on(tool.call(DecideChangesArgs {
                    instruction: Some("accept it".into()),
                    decisions: Some(vec![DecisionArg {
                        id: Some(id),
                        action: Some("accept".into()),
                    }]),
                }))
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let used =
        CallRefusal::Authority(crate::agent_conversations::AuthorityRefusal::InstructionUsed);
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().any(|r| r.as_ref().err() == Some(&used)));
    let accepted = results.iter().flatten().next().unwrap();
    assert_eq!(accepted.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(nd::list_impl(&f.root(), &f.draft_id).unwrap().len(), 1);
}

// DDC-FR-TBVA: an authority refusal is one WARN record that names its reason,
// and no record of a refused or a confirming call carries the instruction.
#[test]
fn an_authority_refusal_logs_its_reason_and_never_the_instruction() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let id = f.propose("relay-auth");
    let (_, turn) = f.author_turn("@arch accept it, zebra");
    let tool = |turn: &str| {
        DecideDraftChangesTool::new(
            f.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
            f.origin(),
            agent(),
            turn,
        )
        .for_test(human(), &BUFFER)
    };
    let call = |instruction: &str, decisions: Vec<(String, &str)>| DecideChangesArgs {
        instruction: Some(instruction.into()),
        decisions: Some(
            decisions
                .into_iter()
                .map(|(id, action)| DecisionArg { id: Some(id), action: Some(action.into()) })
                .collect(),
        ),
    };
    block_on(tool(&turn).call(call("zebra crossing", vec![(id.clone(), "accept")])))
        .expect_err("not found");
    block_on(
        tool(&turn)
            .call(call("accept it, zebra", vec![(id.clone(), "accept"), (f.set_id(), "confirm")])),
    )
    .expect("decided");
    let records = BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                domains: vec![crate::logging::Domain::Ai],
                ..crate::logging::LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records;
    let fields: Vec<_> = records.iter().map(|r| serde_json::to_value(&r.fields).unwrap()).collect();
    assert!(fields.iter().any(|f| f["reason"] == "instruction_not_found"));
    assert!(fields.iter().any(|f| f["confirmed"] == 1 && f["accepted"] == 1));
    let all = serde_json::to_string(&fields).unwrap();
    assert!(!all.contains("zebra"), "a record carries the instruction");
}
