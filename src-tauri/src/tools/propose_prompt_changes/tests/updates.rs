//! Tests for `updates`, and for the author deciding each change separately
//! (PPC-FR-RVKS, PPC-FR-CVHT, PPC-FR-XHHT, PPC-FR-HMEH).

use super::*;
use crate::comments::{Attachment, ProposalUpdateCounts};
use crate::prompt_proposals::{HunkState, PromptProposalState};

fn change(before: &str, after: &str) -> PromptHunkArg {
    PromptHunkArg {
        kind: None,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        lead: None,
        trail: None,
    }
}

fn entry(action: &str, id: Option<&str>, before: Option<&str>, after: Option<&str>) -> UpdateArg {
    UpdateArg {
        action: Some(action.into()),
        id: id.map(str::to_string),
        before: before.map(str::to_string),
        after: after.map(str::to_string),
        ..UpdateArg::default()
    }
}

impl Fixture {
    fn call_updating(
        &self,
        updates: Vec<UpdateArg>,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        block_on(self.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            updates: Some(updates),
            content: None,
            hunks: None,
            rationale: "The discussion asked for it. <spoken>Changed.</spoken>".into(),
        }))
    }

    /// A standing proposal of two changes, and the id of each.
    fn two_changes(&self) -> (String, String, String) {
        let output = block_on(self.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            updates: None,
            content: None,
            hunks: Some(vec![change("# Review", "# Careful review"), change("three", "two")]),
            rationale: RATIONALE.into(),
        }))
        .expect("recorded");
        let record = self.proposals().into_iter().next().expect("the proposal");
        (output.proposal_id, record.ledger[0].id.clone(), record.ledger[1].id.clone())
    }
}

// PPC-FR-RVKS: one call changes, withdraws, and adds, keeps the proposal, posts
// one comment carrying the counts, and ends the turn.
#[test]
fn an_update_changes_withdraws_and_adds_with_one_comment() {
    let f = Fixture::new();
    let (proposal_id, first, second) = f.two_changes();
    let comments = f.discussion().comments.len();
    f.proposed.store(false, Ordering::SeqCst);

    let output = f
        .call_updating(vec![
            entry("withdraw", Some(&first), None, None),
            entry("change", Some(&second), Some("three"), Some("four")),
            entry("add", None, Some("original"), Some("first")),
        ])
        .expect("updated");
    assert_eq!(output.proposal_id, proposal_id, "the standing proposal, not a new one");
    assert!(f.proposed.load(Ordering::SeqCst), "the turn ends on an update");
    assert_eq!(f.proposals().len(), 1, "no second proposal");

    let hunks = crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).expect("hunks");
    assert_eq!(hunks.hunks.len(), 3);
    assert_eq!(hunks.hunks[1].id, second);
    assert_eq!(hunks.hunks[1].after.as_deref(), Some("four"));
    assert_eq!(hunks.hunks[2].after.as_deref(), Some("first"));
    let record = f.proposals().remove(0);
    let states: Vec<HunkState> = record.ledger.iter().map(|row| row.state).collect();
    assert_eq!(states, [HunkState::Withdrawn, HunkState::Pending, HunkState::Pending]);
    assert_eq!(record.ledger[1].revision, 1);
    assert_eq!(record.counts.withdrawn, 1);
    assert_eq!(f.artifact_body(), ORIGINAL, "an update writes no file of the project");

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), comments + 1, "exactly one comment (PCP-FR-RKJI)");
    match &thread.comments.last().expect("comment").attachments[0] {
        Attachment::PromptProposal { proposal_id: named, update, .. } => {
            assert_eq!(named, &proposal_id);
            assert_eq!(*update, Some(ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 1 }));
        }
        other => panic!("expected a prompt proposal reference, got {other:?}"),
    }
}

// PPC-FR-RVKS (as DCP-FR-PWSF): an update that withdraws every undecided hunk
// closes the proposal as `rejected` where nothing was accepted.
#[test]
fn an_update_that_leaves_nothing_undecided_closes_the_proposal() {
    let f = Fixture::new();
    let (_, first, second) = f.two_changes();
    f.call_updating(vec![
        entry("withdraw", Some(&first), None, None),
        entry("withdraw", Some(&second), None, None),
    ])
    .expect("updated");
    assert_eq!(f.proposals()[0].state, PromptProposalState::Rejected);
}

// PPC-FR-RVKS: an update with no open proposal is refused, and the message sends
// the model to a new proposal.
#[test]
fn an_update_with_nothing_open_is_refused() {
    let f = Fixture::new();
    let refused =
        f.call_updating(vec![entry("withdraw", Some("h"), None, None)]).expect_err("refused");
    assert_eq!(refused, ToolRefusal::InvalidArguments(PROPOSAL_NO_OPEN));
    assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
    assert!(f.proposals().is_empty());
}

// PPC-FR-XHHT: `updates` beside `content` or `hunks` is not one form.
#[test]
fn updates_beside_another_form_is_refused() {
    let f = Fixture::new();
    let (_, first, _) = f.two_changes();
    for (content, hunks) in [(Some(PROPOSED), None), (None, Some(vec![change("three", "x")]))] {
        let refused = block_on(f.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            updates: Some(vec![entry("withdraw", Some(&first), None, None)]),
            content: content.map(Into::into),
            hunks,
            rationale: RATIONALE.into(),
        }))
        .expect_err("refused");
        assert_eq!(refused, ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM));
    }
}

// PPC-FR-RVKS: an entry refusal names the update's place, and nothing changes.
#[test]
fn an_entry_refusal_names_its_place_and_writes_nothing() {
    let f = Fixture::new();
    let (proposal_id, first, _) = f.two_changes();
    let before = crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).unwrap();
    let refused = f
        .call_updating(vec![
            entry("change", Some(&first), Some("# Review"), Some("# A")),
            entry("change", Some("nosuchhunk"), Some("three"), Some("x")),
        ])
        .expect_err("refused");
    assert_eq!(
        refused,
        ToolRefusal::InvalidUpdateAt { message: UPDATE_NO_SUCH_CHANGE, at: 2, of: 2 }
    );
    let refused = f
        .call_updating(vec![entry("change", Some(&first), Some("Not there"), Some("x"))])
        .expect_err("refused");
    assert_eq!(
        refused,
        ToolRefusal::InvalidUpdateAt { message: PROPOSAL_TEXT_NOT_FOUND, at: 1, of: 1 }
    );
    assert_eq!(crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).unwrap(), before);
}

// PPC-FR-CVHT: the description and the parameter texts say the author decides
// each change separately and that full text is converted into changes; a
// `content` call records changes the author decides one by one.
#[test]
fn the_author_decides_each_change_and_full_text_is_converted() {
    assert!(DESCRIPTION.contains("The author decides each change separately"));
    assert!(DESCRIPTION.contains("it is converted into changes for you"));
    assert!(CONTENT_DESCRIPTION.contains("converted into separate changes"));
    assert!(CONTENT_DESCRIPTION.contains("the author decides each of them"));
    assert!(DESCRIPTION.contains("`updates`"));

    let f = Fixture::new();
    let output = f.call().expect("recorded");
    let record = f.proposals().into_iter().next().expect("proposal");
    assert_eq!(record.id, output.proposal_id);
    assert!(!record.legacy);
    assert!(record.hunk_count >= 1);
    assert!(record.ledger.iter().all(|row| row.state == HunkState::Pending));
}

// PPC-FR-HMEH: an update appends a comment, so it needs the tag.
#[test]
fn an_update_needs_a_spoken_part() {
    let f = Fixture::new();
    let (_, first, _) = f.two_changes();
    let refusal = block_on(f.tool().call(ProposePromptChangesArgs {
        path: PROMPT.into(),
        updates: Some(vec![entry("withdraw", Some(&first), None, None)]),
        content: None,
        hunks: None,
        rationale: "No tag here.".into(),
    }))
    .expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidArguments(crate::tools::RATIONALE_NO_SPOKEN_PART));
}
