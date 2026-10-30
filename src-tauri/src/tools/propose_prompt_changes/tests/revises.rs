//! Tests for `revises`, and for the author deciding each change separately
//! (PPC-FR-RVKS, PPC-FR-CVHT).

use super::*;

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

impl Fixture {
    fn call_revising(
        &self,
        revises: Option<&str>,
        content: Option<&str>,
        hunks: Option<Vec<PromptHunkArg>>,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        block_on(self.tool().call(ProposePromptChangesArgs {
            path: PROMPT.into(),
            revises: revises.map(Into::into),
            content: content.map(Into::into),
            hunks,
            rationale: RATIONALE.into(),
        }))
    }

    /// A standing proposal of two changes, and the id of each.
    fn two_changes(&self) -> (String, String, String) {
        let output = self
            .call_revising(
                None,
                None,
                Some(vec![change("# Review", "# Careful review"), change("three", "two")]),
            )
            .expect("recorded");
        let record = self.proposals().into_iter().next().expect("the proposal");
        (output.proposal_id, record.ledger[0].id.clone(), record.ledger[1].id.clone())
    }
}

// PPC-FR-RVKS: a call naming an undecided change of the standing proposal
// replaces that change in place and posts no comment; the turn ends on it.
#[test]
fn a_revision_replaces_the_named_change_in_place() {
    let f = Fixture::new();
    let (proposal_id, first, second) = f.two_changes();
    let comments = f.discussion().comments.len();
    f.proposed.store(false, Ordering::SeqCst);

    let output =
        f.call_revising(Some(&second), None, Some(vec![change("three", "four")])).expect("revised");
    assert_eq!(output.proposal_id, proposal_id, "the standing proposal, not a new one");
    assert!(f.proposed.load(Ordering::SeqCst), "the turn ends on a revision");
    assert_eq!(f.proposals().len(), 1, "no second proposal");
    assert_eq!(f.discussion().comments.len(), comments, "no new comment");
    let hunks = crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).expect("hunks");
    assert_eq!(hunks.hunks[1].id, second);
    assert_eq!(hunks.hunks[1].after.as_deref(), Some("four"));
    assert_eq!(hunks.hunks[0].id, first);
    assert_eq!(f.artifact_body(), ORIGINAL);
}

// PPC-FR-RVKS: a name that matches no revisable change is the no-such-change
// refusal; with no standing proposal the field is ignored.
#[test]
fn a_name_that_matches_nothing_is_refused_and_ignored_with_nothing_standing() {
    let f = Fixture::new();
    let output = f
        .call_revising(Some("nosuchhunk"), None, Some(vec![change("three", "two")]))
        .expect("with nothing standing the field is ignored");
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.proposals()[0].id, output.proposal_id);

    let refused = f
        .call_revising(Some("nosuchhunk"), None, Some(vec![change("three", "four")]))
        .expect_err("refused");
    assert_eq!(refused, ToolRefusal::InvalidArguments(PROPOSAL_NO_SUCH_CHANGE));
    assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
    assert!(refused.retryable());
    assert_eq!(
        PROPOSAL_NO_SUCH_CHANGE,
        "The change you named is not one of the changes you can still revise. Propose a new change instead."
    );
}

// PPC-FR-RVKS: a revision carries exactly one change in `hunks`; a `content`
// beside a revision is refused the same way, and nothing changes.
#[test]
fn a_revision_carries_exactly_one_change_in_hunks() {
    let f = Fixture::new();
    let (proposal_id, first, _) = f.two_changes();
    let before = crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).unwrap();
    let two = vec![change("# Review", "# A"), change("three", "four")];
    for (content, hunks) in [(None, Some(two)), (Some(PROPOSED), None)] {
        let refused = f.call_revising(Some(&first), content, hunks).expect_err("refused");
        assert_eq!(refused, ToolRefusal::InvalidArguments(PROPOSAL_REVISE_ONE));
    }
    assert_eq!(crate::prompt_proposals::load_hunks_impl(&f.root(), &proposal_id).unwrap(), before);
    assert_eq!(
        PROPOSAL_REVISE_ONE,
        "You named a change to replace, so send only the one change that replaces it in `hunks`. Wait for the author to decide the changes already standing before you propose the rest."
    );
}

// PPC-FR-RVKS: a replacement whose text the file does not hold is the text-not-
// found refusal, not a name that resolved to nothing.
#[test]
fn a_revision_naming_text_the_file_lacks_is_text_not_found() {
    let f = Fixture::new();
    let (_, first, _) = f.two_changes();
    let refused = f
        .call_revising(Some(&first), None, Some(vec![change("Not there", "x")]))
        .expect_err("refused");
    assert_eq!(refused, ToolRefusal::InvalidArguments(PROPOSAL_TEXT_NOT_FOUND));
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
    assert!(DESCRIPTION.contains("`revises`"));

    let f = Fixture::new();
    let output = f.call().expect("recorded");
    let record = f.proposals().into_iter().next().expect("proposal");
    assert_eq!(record.id, output.proposal_id);
    assert!(!record.legacy);
    assert!(record.hunk_count >= 1);
    assert!(record
        .ledger
        .iter()
        .all(|row| row.state == crate::prompt_proposals::HunkState::Pending));
}
