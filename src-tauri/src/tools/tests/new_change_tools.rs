//! Tests for `PNC-propose-new-change-tool.md` and the split entries of
//! `DCE-decide-change-edits-tool.md`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rig::tool::{PortableTool, ToolErrorKind};

use super::change_edit_tools::{
    agent, decisions, human, Fixture, CHANGE, QUIET, RATIONALE, STREAM,
};
use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, DiscussionTarget};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::new_change_proposals as ncp;
use crate::tools::decide_changes::{Outcome, RefusalCode};
use crate::tools::propose_new_change::{
    self, FileArg, ProposeNewChangeArgs, ProposeNewChangeTool, UpdateArg,
};
use crate::tools::tests::block_on;
use crate::tools::ToolRefusal;

fn tool(f: &Fixture, buffer: &'static LogBuffer) -> ProposeNewChangeTool<tauri::test::MockRuntime> {
    ProposeNewChangeTool::new(
        f.open.handle(),
        f.roots(),
        f.origin(),
        agent(),
        Arc::clone(&f.proposed),
    )
    .for_test(f.sites(), buffer)
}

fn args(name: &str) -> ProposeNewChangeArgs {
    ProposeNewChangeArgs {
        rationale: RATIONALE.into(),
        name: Some(name.into()),
        files: Some(vec![FileArg { path: "proposal.md".into(), text: "## Why\n\nPart.\n".into() }]),
        position: Some(1),
        total: Some(2),
        update: None,
    }
}

fn propose(
    f: &Fixture,
    args: ProposeNewChangeArgs,
) -> Result<propose_new_change::ProposeNewChangeOutput, ToolRefusal> {
    block_on(tool(f, &QUIET).call(args))
}

// PNC-FR-RTSB / PNC-FR-SODJ: a portable tool with the fixed text and a schema
// of name, files, position, total, update, and rationale.
#[test]
fn pnc_is_a_portable_tool_with_its_fixed_text_and_schema() {
    let f = Fixture::new();
    let t = tool(&f, &QUIET);
    assert_eq!(
        <ProposeNewChangeTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_new_change"
    );
    assert_eq!(t.description(), propose_new_change::DESCRIPTION);
    let schema = t.parameters();
    for key in ["rationale", "name", "files", "position", "total", "update"] {
        assert!(schema["properties"][key].is_object(), "{key}");
    }
    assert_eq!(schema["required"], serde_json::json!(["rationale"]));
}

// PNC-FR-IADA / PNC-FR-BPCH / PNC-FR-JPTJ / PNC-FR-SUUN / PNC-FR-VVSC: a call
// records one proposal into the bound change and conversation, posts the
// rationale with the reference, ends the turn, and creates no change.
#[test]
fn pnc_records_one_proposal_and_ends_the_turn() {
    let f = Fixture::new();
    assert_eq!(
        propose(&f, args("part-one")),
        Ok(propose_new_change::ProposeNewChangeOutput { proposed: true })
    );
    assert!(f.proposed.load(Ordering::SeqCst));
    let all = ncp::list_proposals_impl(&f.store(), STREAM, CHANGE);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].thread_id, f.thread_id);
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).expect("thread");
    let last = thread.comments.last().expect("comment");
    assert_eq!(last.body, RATIONALE);
    assert!(matches!(last.attachments[0], comments::Attachment::NewChangeProposal { .. }));
    assert!(!f.wc.path().join("openspec/changes/part-one").exists());
}

// PNC-FR-RLNW / PNC-FR-BVYL / PNC-FR-LQHP: the field rules refuse with
// retryable `InvalidArgs`, and a second proposal while one waits is the
// retryable pending refusal.
#[test]
fn pnc_refuses_bad_fields_and_a_second_pending_proposal() {
    let f = Fixture::new();
    let mut no_proposal_md = args("part-one");
    no_proposal_md.files = Some(vec![FileArg { path: "tasks.md".into(), text: "x".into() }]);
    let mut outside = args("part-one");
    outside.files = Some(vec![
        FileArg { path: "proposal.md".into(), text: "x".into() },
        FileArg { path: "../x.md".into(), text: "x".into() },
    ]);
    let mut position = args("part-one");
    position.position = Some(3);
    for (bad, code) in [
        (no_proposal_md, "missing_proposal_file"),
        (outside, "path_outside_change"),
        (position, "bad_position"),
        (args("Bad Name"), "invalid_change_name"),
        (args(CHANGE), "openspec_change_exists"),
    ] {
        let refused = propose(&f, bad).unwrap_err();
        assert_eq!(refused.reason(), code);
        assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
        assert!(refused.retryable());
    }
    propose(&f, args("part-one")).expect("recorded");
    let pending = propose(&f, args("part-two")).unwrap_err();
    assert_eq!(pending.reason(), "new_change_proposal_pending");
    assert_eq!(pending.kind(), ToolErrorKind::PermissionDenied);
    assert!(pending.retryable());
}

// PNC-FR-SWWQ: `update` changes or withdraws the open proposal; a malformed
// update is refused.
#[test]
fn pnc_updates_the_open_proposal() {
    let f = Fixture::new();
    propose(&f, args("part-one")).expect("recorded");
    let change = ProposeNewChangeArgs {
        rationale: RATIONALE.into(),
        name: Some("part-a".into()),
        files: None,
        position: None,
        total: None,
        update: Some(UpdateArg { action: Some("change".into()) }),
    };
    propose(&f, change).expect("changed");
    assert_eq!(ncp::list_proposals_impl(&f.store(), STREAM, CHANGE)[0].name, "part-a");
    let malformed = ProposeNewChangeArgs {
        update: Some(UpdateArg { action: Some("withdraw".into()) }),
        ..args("x")
    };
    assert_eq!(propose(&f, malformed).unwrap_err().reason(), "update_malformed");
    let withdraw = ProposeNewChangeArgs {
        rationale: RATIONALE.into(),
        name: None,
        files: None,
        position: None,
        total: None,
        update: Some(UpdateArg { action: Some("withdraw".into()) }),
    };
    propose(&f, withdraw).expect("withdrawn");
    assert_eq!(
        ncp::list_proposals_impl(&f.store(), STREAM, CHANGE)[0].state,
        ncp::NewChangeProposalState::Withdrawn
    );
}

// PNC-FR-EJAH / PNC-FR-HBSW: a turn not about a change, and a closed project,
// are refused.
#[test]
fn pnc_refuses_other_conversations_and_a_closed_project() {
    let f = Fixture::new();
    let other = ProposeNewChangeTool::new(
        f.open.handle(),
        f.roots(),
        ConversationOrigin::stub_artifact("t", "a.md", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    )
    .for_test(f.sites(), &QUIET);
    let refused = block_on(other.call(args("part-one"))).unwrap_err();
    assert_eq!(refused.reason(), "not_a_change_conversation");
    assert!(!refused.retryable());
    let closed = super::closed_project();
    let t = ProposeNewChangeTool::new(
        closed.handle().clone(),
        f.roots(),
        f.origin(),
        agent(),
        Arc::default(),
    );
    assert_eq!(block_on(t.call(args("part-one"))).unwrap_err(), ToolRefusal::NoProjectOpen);
}

// PNC-FR-VHLS: each call logs the tool, the stream, the change, the name, and
// counts, and no file text or rationale.
#[test]
fn pnc_logs_ids_and_counts_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    block_on(tool(&f, &BUFFER).call(args("part-one"))).expect("recorded");
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
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["tool"], "propose_new_change");
    assert_eq!(fields["name"], "part-one");
    assert_eq!(fields["files"], 1);
    let all = serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>())
        .expect("json");
    assert!(!all.contains("Part.") && !all.contains("Add logout"));
}

// DCE-FR-UZDN / DCE-FR-RAWJ / DCE-FR-WRSI / DCE-FR-PHSU: with no hunk proposal
// open, the decide tool accepts the pending new change and then confirms the
// split by its id, in one call.
#[test]
fn dce_accepts_a_new_change_and_confirms_the_split() {
    let f = Fixture::new();
    propose(&f, args("part-one")).expect("recorded");
    let record = ncp::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let out = block_on(
        f.decider(f.sites(), &QUIET)
            .call(decisions(&[(&record.id, "accept"), (&record.set_id, "confirm")])),
    )
    .expect("decided");
    assert_eq!(out.decisions[0].outcome, Outcome::Accepted);
    assert_eq!(out.decisions[1].outcome, Outcome::Confirmed);
    assert!(f.wc.path().join("openspec/changes/part-one/proposal.md").is_file());
    assert_eq!(
        ncp::read_split(&f.store(), STREAM, CHANGE).map(|s| s.accepted),
        Some(vec!["part-one".to_string()])
    );
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).expect("thread");
    let decision = thread
        .comments
        .iter()
        .find(|c| c.body.starts_with("Accepted the proposed new"))
        .expect("decision");
    assert_eq!(decision.author, human());
    assert!(decision.body.contains("Decided by arch on the author's instruction"));
}

// DCE-FR-WMOG: `discuss` opens a discussion with a `new_change_subject`, and a
// second `discuss` is already in state.
#[test]
fn dce_discusses_a_new_change_once() {
    let f = Fixture::new();
    propose(&f, args("part-one")).expect("recorded");
    let record = ncp::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let out = block_on(f.decider(f.sites(), &QUIET).call(decisions(&[(&record.id, "discuss")])))
        .expect("decided");
    let opened = out.decisions[0].discussion_id.clone().expect("discussion");
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &opened).expect("thread");
    assert_eq!(
        thread.comments[0].attachments,
        vec![comments::Attachment::NewChangeSubject {
            proposal_id: record.id.clone(),
            stream_id: STREAM.into(),
            change: CHANGE.into(),
        }]
    );
    let again = block_on(f.decider(f.sites(), &QUIET).call(decisions(&[(&record.id, "discuss")])))
        .expect("answered");
    assert_eq!(
        again.decisions[0].refusal.as_ref().map(|r| r.code),
        Some(RefusalCode::AlreadyInState)
    );
}

// DCE-FR-RAWJ / DCE-FR-WRSI: a busy stream is a retryable row; a `confirm`
// with another id is unknown; a refused confirmation is `decision_failed`.
#[test]
fn dce_split_refusals() {
    let f = Fixture::new();
    propose(&f, args("part-one")).expect("recorded");
    let record = ncp::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    let busy = Box::new(crate::change_proposals::FixedSites {
        sites: vec![(STREAM.into(), f.wc.clone())],
        busy: std::sync::Mutex::new(Some("run-1".into())),
    });
    let out = block_on(f.decider(busy, &QUIET).call(decisions(&[
        (&record.id, "accept"),
        ("not-the-split", "confirm"),
        (&record.set_id, "confirm"),
    ])))
    .expect("answered");
    let busy_row = out.decisions[0].refusal.as_ref().expect("refused");
    assert_eq!(busy_row.error.as_deref(), Some(ncp::ERR_STREAM_BUSY));
    assert!(busy_row.retryable);
    assert_eq!(out.decisions[1].refusal.as_ref().map(|r| r.code), Some(RefusalCode::UnknownHunk));
    let confirm = out.decisions[2].refusal.as_ref().expect("refused");
    assert_eq!(confirm.code, RefusalCode::DecisionFailed);
    assert_eq!(confirm.error.as_deref(), Some(ncp::ERR_PROPOSAL_PENDING));
}

// PNC-FR-BVYL / NCP-FR-SNMA / NCP-FR-MQCS: after the split is confirmed, a new
// proposal and an update are both refused as `split_confirmed`, with no
// retry.
#[test]
fn pnc_refuses_a_proposal_and_an_update_after_the_split_is_confirmed() {
    let f = Fixture::new();
    propose(&f, args("part-one")).expect("recorded");
    let record = ncp::list_proposals_impl(&f.store(), STREAM, CHANGE).remove(0);
    block_on(
        f.decider(f.sites(), &QUIET)
            .call(decisions(&[(&record.id, "accept"), (&record.set_id, "confirm")])),
    )
    .expect("decided");
    let fresh = propose(&f, args("part-two")).unwrap_err();
    assert_eq!(fresh.reason(), "split_confirmed");
    assert!(!fresh.retryable());
    let withdraw = ProposeNewChangeArgs {
        rationale: RATIONALE.into(),
        name: None,
        files: None,
        position: None,
        total: None,
        update: Some(UpdateArg { action: Some("withdraw".into()) }),
    };
    assert_eq!(propose(&f, withdraw).unwrap_err().reason(), "split_confirmed");
}

// PNC-FR-BVYL / NCP-FR-RPGN: a resolved discussion refuses a proposal as a
// resolved conversation.
#[test]
fn pnc_refuses_a_proposal_in_a_resolved_discussion() {
    let f = Fixture::new();
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread = comments::locate_discussion_of(&f.store(), &target, &f.thread_id).expect("thread");
    comments::set_resolution_to(
        &f.store(),
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let refused = propose(&f, args("part-one")).unwrap_err();
    assert!(matches!(refused, ToolRefusal::ProposalConversationResolved), "{refused:?}");
    assert!(ncp::list_proposals_impl(&f.store(), STREAM, CHANGE).is_empty());
}
