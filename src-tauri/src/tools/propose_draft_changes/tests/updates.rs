//! Tests for an update of the open proposal through `updates`
//! (`PDC-propose-draft-changes-tool.md` PDC-FR-GMWR through PDC-FR-VUTJ, and
//! the store behaviour of `DCP-draft-change-proposals.md` it reaches).

use super::*;
use crate::comments::{Attachment, ProposalUpdateCounts};
use crate::draft_proposals::hunks::HunkState;
use crate::draft_proposals::ProposalState;

/// A replacement change with no context.
fn replace(before: &str, after: &str) -> HunkArg {
    HunkArg {
        kind: Some("replace".into()),
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    }
}

/// An update entry of `action`, naming `id`, with a replacement of `before`.
fn entry(action: &str, id: Option<&str>, before: Option<&str>, after: Option<&str>) -> UpdateArg {
    UpdateArg {
        action: Some(action.into()),
        id: id.map(str::to_string),
        before: before.map(str::to_string),
        after: after.map(str::to_string),
        ..UpdateArg::default()
    }
}

/// A call that carries `updates` alone.
fn updating(updates: Vec<UpdateArg>) -> ProposeDraftChangesArgs {
    ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: "The discussion asked for this. <spoken>I changed it.</spoken>".into(),
        updates: Some(updates),
        hunks: Vec::new(),
    }
}

/// A fixture with an open proposal of two changes: `original` and `The`.
fn with_two_changes() -> (Fixture, String, String) {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        updates: None,
        hunks: vec![replace("original", "revised"), replace("The", "This")],
    })
    .expect("a proposal stands");
    f.proposed.store(false, Ordering::SeqCst);
    let hunks = f.hunks_of_the_one_proposal();
    (f, hunks[0].id.clone(), hunks[1].id.clone())
}

fn set_state(f: &Fixture, hunk_id: &str, state: HunkState) {
    let proposal = f.proposals()[0].id.clone();
    crate::draft_proposals::set_hunk_state(&f.root(), &f.draft_id, &proposal, hunk_id, state)
        .expect("state set");
}

// PDC-FR-GMWR / PDC-FR-VUTJ / DCP-FR-TQBO / DCP-FR-IAFV / DCP-FR-PHUB: one call
// changes a hunk in place, withdraws another, adds a third after them, appends
// one comment carrying the counts, and ends the turn.
#[test]
fn one_update_changes_withdraws_and_adds_and_appends_one_comment() {
    let (f, first, second) = with_two_changes();
    let comments = f.discussion().comments.len();
    let out = f
        .call(updating(vec![
            entry("change", Some(&first), Some("original"), Some("first")),
            entry("withdraw", Some(&second), None, None),
            entry("add", None, Some("Spec"), Some("Specification")),
        ]))
        .expect("updated");
    assert!(out.proposed);
    assert!(f.proposed.load(Ordering::SeqCst), "an update ends the turn (CVL-FR-15)");

    let hunks = f.hunks_of_the_one_proposal();
    assert_eq!(hunks.len(), 3, "the withdrawn change is kept in the record");
    assert_eq!(hunks[0].id, first, "a changed hunk keeps its id and position");
    assert_eq!(hunks[0].after_text(), "first");
    assert_eq!(hunks[0].revision, 1);
    assert_eq!(hunks[1].id, second);
    assert_eq!(hunks[2].after_text(), "Specification", "an added hunk goes last");

    let proposal = &f.proposals()[0];
    let states: Vec<HunkState> = proposal.ledger.iter().map(|row| row.state).collect();
    assert_eq!(states, [HunkState::Pending, HunkState::Withdrawn, HunkState::Pending]);
    assert_eq!(proposal.counts.withdrawn, 1);
    assert_eq!(proposal.counts.pending, 2);
    assert_eq!(proposal.state, ProposalState::Pending);
    assert_eq!(f.proposals().len(), 1, "an update records no new proposal");

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), comments + 1, "exactly one comment");
    let posted = thread.comments.last().expect("the update comment");
    assert_eq!(posted.attachments.len(), 1);
    match &posted.attachments[0] {
        Attachment::Proposal { proposal_id, update, .. } => {
            assert_eq!(proposal_id, &proposal.id);
            assert_eq!(
                *update,
                Some(ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 1 }),
                "CMS-FR-BSQB"
            );
        }
        other => panic!("expected a proposal reference, got {other:?}"),
    }
}

// DCP-FR-TQBO: a rejected or a held change is undecided again after a change,
// and the author's own edit of it is cleared.
#[test]
fn a_rejected_change_is_pending_again_after_a_change() {
    let (f, first, second) = with_two_changes();
    set_state(&f, &first, HunkState::Rejected);
    set_state(&f, &second, HunkState::Discussing);
    f.call(updating(vec![
        entry("change", Some(&first), Some("original"), Some("first")),
        entry("change", Some(&second), Some("The"), Some("That")),
    ]))
    .expect("updated");
    let proposal = &f.proposals()[0];
    for row in &proposal.ledger {
        assert_eq!(row.state, HunkState::Pending);
        assert!(row.decided_at.is_none());
        assert!(!row.edited);
        assert_eq!(row.revision, 1);
    }
}

// DCP-FR-PWSF / DCP-FR-15 / DCP-FR-WTKA: an update that leaves no undecided hunk
// closes the proposal, by today's rule with withdrawn hunks counted for neither
// state, and appends no decision comment.
#[test]
fn an_update_that_leaves_nothing_undecided_closes_the_proposal() {
    let (f, first, second) = with_two_changes();
    set_state(&f, &first, HunkState::Accepted);
    let comments = f.discussion().comments.len();
    f.call(updating(vec![entry("withdraw", Some(&second), None, None)])).expect("updated");
    let proposal = &f.proposals()[0];
    assert_eq!(proposal.state, ProposalState::Accepted);
    assert!(proposal.decided_at.is_some());
    assert_eq!(f.discussion().comments.len(), comments + 1, "the update's own comment only");

    let (g, first, second) = with_two_changes();
    g.call(updating(vec![
        entry("withdraw", Some(&first), None, None),
        entry("withdraw", Some(&second), None, None),
    ]))
    .expect("updated");
    assert_eq!(g.proposals()[0].state, ProposalState::Rejected, "no hunk is accepted");
}

// PDC-FR-GPLQ: `hunks` and `updates` together, and an empty `updates`, are each
// refused, and nothing is written.
#[test]
fn one_form_per_call_and_no_empty_list() {
    let (f, first, _) = with_two_changes();
    let mut both = updating(vec![entry("withdraw", Some(&first), None, None)]);
    both.hunks = vec![replace("Spec", "Specification")];
    assert_eq!(f.call(both), Err(ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM)));
    assert_eq!(f.call(updating(Vec::new())), Err(ToolRefusal::InvalidArguments(UPDATE_NONE)));
    assert_eq!(f.proposals()[0].counts.withdrawn, 0, "nothing was written");
}

// PDC-FR-LGWJ: `updates` where no proposal is open is refused, and the message
// sends the model to a new proposal.
#[test]
fn an_update_with_no_open_proposal_is_refused() {
    let f = Fixture::new();
    let refusal =
        f.call(updating(vec![entry("withdraw", Some("h"), None, None)])).expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_NO_OPEN));
    assert!(refusal.retryable());
    assert!(PROPOSAL_NO_OPEN.contains("`hunks`"));
    assert!(f.proposals().is_empty());
}

// PDC-FR-UOBH / PDC-FR-DBFT / PDC-FR-SNUK: the entry refusals, each naming the
// place of the update it is about, and each writing nothing.
#[test]
fn entry_refusals_name_the_place_of_the_update() {
    let (f, first, second) = with_two_changes();
    set_state(&f, &second, HunkState::Accepted);
    let ok = || entry("change", Some(&first), Some("original"), Some("first"));
    let cases = [
        (vec![ok(), entry("withdraw", None, None, None)], UPDATE_MALFORMED, 2, 2),
        (vec![ok(), entry("add", Some("x"), Some("Spec"), Some("S"))], UPDATE_MALFORMED, 2, 2),
        (vec![ok(), entry("withdraw", Some("nope"), None, None)], UPDATE_NO_SUCH_CHANGE, 2, 2),
        (vec![entry("withdraw", Some(&second), None, None)], UPDATE_CHANGE_SETTLED, 1, 1),
        (vec![ok(), entry("withdraw", Some(&first), None, None)], UPDATE_NAMED_TWICE, 2, 2),
    ];
    let before = f.hunks_of_the_one_proposal();
    let comments = f.discussion().comments.len();
    for (updates, message, at, of) in cases {
        let refusal = f.call(updating(updates)).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::InvalidUpdateAt { message, at, of });
        assert!(refusal.to_string().starts_with(&format!("Update {at} of {of}: ")));
        assert!(refusal.retryable());
    }
    // PDC-FR-VUTJ: all or nothing.
    assert_eq!(f.hunks_of_the_one_proposal(), before);
    assert_eq!(f.discussion().comments.len(), comments);
}

// PDC-FR-DBFT: a withdrawn change is settled for every later update.
#[test]
fn a_withdrawn_change_can_not_be_updated_again() {
    let (f, first, second) = with_two_changes();
    f.call(updating(vec![entry("withdraw", Some(&second), None, None)])).expect("updated");
    let refusal = f
        .call(updating(vec![
            entry("change", Some(&first), Some("original"), Some("first")),
            entry("change", Some(&second), Some("The"), Some("That")),
        ]))
        .expect_err("refused");
    assert_eq!(
        refusal,
        ToolRefusal::InvalidUpdateAt { message: UPDATE_CHANGE_SETTLED, at: 2, of: 2 }
    );
}

// PDC-FR-DGUC / PDC-FR-SNUK: an added change must not overlap a live change
// the call keeps, and a placement refusal names the update's place.
#[test]
fn a_placement_refusal_names_the_update() {
    let (f, first, _) = with_two_changes();
    let refusal = f
        .call(updating(vec![
            entry("withdraw", Some(&first), None, None),
            entry("add", None, Some("The original"), Some("A new")),
        ]))
        .expect_err("refused");
    assert_eq!(
        refusal,
        ToolRefusal::InvalidUpdateAt { message: PROPOSAL_HUNKS_OVERLAP, at: 2, of: 2 }
    );
    let refusal = f
        .call(updating(vec![entry("add", None, Some("not in the prompt"), Some("x"))]))
        .expect_err("refused");
    assert_eq!(
        refusal,
        ToolRefusal::InvalidUpdateAt { message: PROPOSAL_TEXT_NOT_FOUND, at: 1, of: 1 }
    );
    // A change of the hunk itself may cover its own text.
    f.call(updating(vec![entry("change", Some(&first), Some("original."), Some("first."))]))
        .expect("a change may widen its own text");
}

// PDC-FR-VSMM: a change that leaves its text as it is is refused.
#[test]
fn a_change_to_the_same_text_is_refused() {
    let (f, first, _) = with_two_changes();
    let refusal = f
        .call(updating(vec![entry("change", Some(&first), Some("original"), Some("original"))]))
        .expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidUpdateAt { message: PROPOSAL_NO_CHANGE, at: 1, of: 1 });
}

// PDC-FR-ZQOH: an update appends a comment, so its rationale needs a spoken part.
#[test]
fn an_update_needs_a_spoken_part() {
    let (f, first, _) = with_two_changes();
    let mut call = updating(vec![entry("withdraw", Some(&first), None, None)]);
    call.rationale = "No tag here.".into();
    assert_eq!(f.call(call), Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART)));
}

// PDC-FR-09: a resolved conversation takes no update, and nothing is written.
#[test]
fn an_update_into_a_resolved_conversation_writes_nothing() {
    let (f, first, _) = with_two_changes();
    f.resolve_conversation();
    let refusal =
        f.call(updating(vec![entry("withdraw", Some(&first), None, None)])).expect_err("refused");
    assert_eq!(refusal, ToolRefusal::ProposalConversationResolved);
    assert_eq!(f.proposals()[0].counts.withdrawn, 0, "DCP-FR-VTYX: the record is as it was");
}

// PDC-FR-18: an update logs the count of each action and no text.
#[test]
fn an_update_logs_its_action_counts_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let (f, first, second) = with_two_changes();
    BUFFER.clear();
    block_on(f.tool().with_buffer(&BUFFER).call(updating(vec![
        entry("change", Some(&first), Some("original"), Some("A SECRET REPLACEMENT")),
        entry("withdraw", Some(&second), None, None),
    ])))
    .expect("updated");
    let records = records(&BUFFER, Domain::Ai);
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("the success");
    let fields = serde_json::to_value(&info.fields).expect("fields");
    assert_eq!(fields["updates"], 2);
    assert_eq!(fields["updateChanges"], 1);
    assert_eq!(fields["updateWithdraws"], 1);
    assert_eq!(fields["updateAdds"], 0);
    let rendered = serde_json::to_string(&fields).expect("render");
    assert!(!rendered.contains("A SECRET REPLACEMENT"));
}

// PDC-FR-02 / TLC-FR-06: the description and the schema name `updates`.
#[test]
fn the_description_and_schema_name_updates() {
    assert!(DESCRIPTION.contains("`updates`"));
    assert!(!DESCRIPTION.contains("`revises`"));
    let schema = parameters();
    let item = &schema["properties"]["updates"]["items"];
    assert_eq!(
        item["properties"]["action"]["enum"],
        serde_json::json!(["change", "add", "withdraw"])
    );
    for key in ["id", "before", "after", "after_text", "lead", "trail"] {
        assert!(item["properties"][key].is_object(), "{key}");
    }
}

// PDC-FR-05: an update names the draft's prompt as its path, as a recording
// does, and any other path is refused with nothing written.
#[test]
fn an_update_with_another_path_is_refused() {
    let (f, first, _) = with_two_changes();
    let mut call = updating(vec![entry("withdraw", Some(&first), None, None)]);
    call.path = "other.md".into();
    assert_eq!(f.call(call), Err(ToolRefusal::ProposalPathMissing));
    assert_eq!(f.proposals()[0].counts.withdrawn, 0);
}

// PDC-FR-VSMM: an added replacement that leaves its text as it is is refused
// as a change is.
#[test]
fn an_add_to_the_same_text_is_refused() {
    let (f, _, _) = with_two_changes();
    let refusal = f
        .call(updating(vec![entry("add", None, Some("Spec"), Some("Spec"))]))
        .expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidUpdateAt { message: PROPOSAL_NO_CHANGE, at: 1, of: 1 });
}

// PDC-FR-15: a refused update ends nothing — the turn runs on, so the model
// can correct the update.
#[test]
fn a_refused_update_leaves_the_turn_running() {
    let (f, first, _) = with_two_changes();
    f.call(updating(vec![entry("change", Some(&first), Some("not there"), Some("x"))]))
        .expect_err("refused");
    assert!(!f.proposed.load(Ordering::SeqCst));
}
