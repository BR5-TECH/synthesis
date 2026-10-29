//! A `proposal_hunk` attachment in the input a turn carries (AGC-FR-DKHT).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

/// Record one replacement against the draft's discussion and leave it standing.
fn record_one(
    h: &Harness,
    draft_id: &str,
    thread_id: &str,
    before: &str,
    after: &str,
) -> crate::draft_proposals::DraftChangeProposal {
    crate::draft_proposals::record_proposal(
        &h.app.handle().clone(),
        &h.root(),
        &h.root(),
        crate::draft_proposals::NewProposal {
            draft_id,
            path: &crate::drafts::require_prompt(&h.root(), draft_id).expect("prompt"),
            hunks: &[crate::draft_proposals::hunks::ProposedHunk {
                kind: crate::draft_proposals::anchors::HunkKind::Replace,
                before: Some(before.into()),
                after: Some(after.into()),
                after_text: None,
                lead: None,
                trail: None,
                note: None,
            }],
            rationale: "The opening buries the point.",
            agent: &crate::comments::Participant::Agent {
                agent_id: "a1".into(),
                handle: "arch".into(),
                model: None,
                title: None,
            },
            target: crate::comments::ThreadRef::discussion(draft_id),
            thread_id,
        },
    )
    .expect("recorded")
}

/// An author's comment about one change, carrying the `proposal_hunk`
/// reference to it. Stamped after every comment the recording appended, so it
/// folds last.
fn comment_about(
    h: &Harness,
    draft_id: &str,
    thread_id: &str,
    reference: crate::comments::AttachmentInput,
    body: &str,
    at: &str,
) -> crate::comments::Discussion {
    crate::comments::add_comment_to(
        &h.root(),
        crate::comments::ThreadRef::discussion(draft_id),
        thread_id,
        body.into(),
        Vec::new(),
        vec![reference],
        crate::comments::Stamp { by: &human("ada"), at },
    )
    .expect("comment")
}

fn reference_to(
    proposal: &crate::draft_proposals::DraftChangeProposal,
    hunk_id: &str,
) -> crate::comments::AttachmentInput {
    crate::comments::AttachmentInput::ProposalHunk {
        proposal_id: proposal.id.clone(),
        hunk_id: hunk_id.into(),
        draft_id: proposal.draft_id.clone(),
        path: proposal.path.clone(),
    }
}

// AGC-FR-DKHT / AGC-FR-09: a `proposal_hunk` in `discussion_history` and one on
// the `current_comment` each name their change — its id, its kind, its state,
// and excerpts of the text it changes and the text it offers.
#[test]
fn a_proposal_hunk_names_its_change_in_the_history_and_on_the_current_comment() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up"]);
    // The seeded prompt is `Loose notes.`.
    let proposal = record_one(&h, &draft_id, &thread.id, "Loose", "Tidy");
    let hunk_id = proposal.ledger[0].id.clone();
    comment_about(
        &h,
        &draft_id,
        &thread.id,
        reference_to(&proposal, &hunk_id),
        "Not this one.",
        "2099-01-02T00:00:00Z",
    );
    let thread = comment_about(
        &h,
        &draft_id,
        &thread.id,
        reference_to(&proposal, &hunk_id),
        "@arch try again here",
        "2099-01-03T00:00:00Z",
    );
    let trigger = thread.comments.last().expect("a comment").id.clone();

    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let expected = format!(
        "[about proposed change {hunk_id} to {}: replace — undecided — changes: \"Loose\" — offers: \"Tidy\"]",
        proposal.path
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert!(history.body.contains(&expected), "{}", history.body);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(current.body.contains(&expected), "{}", current.body);
}

// AGC-FR-DKHT: the state named is the change's state as it stands, a rejected
// change reading `rejected`.
#[test]
fn a_proposal_hunk_names_the_state_its_change_holds_now() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up"]);
    let proposal = record_one(&h, &draft_id, &thread.id, "notes", "records");
    let hunk_id = proposal.ledger[0].id.clone();
    crate::draft_proposals::decline_for_test(
        &h.app.handle().clone(),
        &h.root(),
        &h.root(),
        &proposal.id,
        &human("ada"),
    )
    .expect("declined");
    let thread = comment_about(
        &h,
        &draft_id,
        &thread.id,
        reference_to(&proposal, &hunk_id),
        "@arch why this?",
        "2099-01-03T00:00:00Z",
    );
    let trigger = thread.comments.last().expect("a comment").id.clone();

    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(
        current
            .body
            .contains(&format!("change {hunk_id} to {}: replace — rejected", proposal.path)),
        "{}",
        current.body
    );
}

// AGC-FR-DKHT: a reference that cannot be read still names the change by its
// id, so the agent can still name it as `revises`.
#[test]
fn a_proposal_hunk_that_cannot_be_read_keeps_a_bare_line_with_its_id() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up"]);
    let thread = comment_about(
        &h,
        &draft_id,
        &thread.id,
        crate::comments::AttachmentInput::ProposalHunk {
            proposal_id: "p-nonesuch".into(),
            hunk_id: "h-nonesuch".into(),
            draft_id: draft_id.clone(),
            path: "prompt.md".into(),
        },
        "@arch what about this?",
        "2099-01-03T00:00:00Z",
    );
    let trigger = thread.comments.last().expect("a comment").id.clone();

    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(
        current.body.contains("[about proposed change h-nonesuch to prompt.md]"),
        "{}",
        current.body
    );
}

// AGC-FR-DKHT: the excerpts are bounded, and a change that offers or changes
// no text names only the text it has.
#[test]
fn a_proposal_hunk_line_bounds_its_excerpts() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::HunkState;
    let long = "word ".repeat(200);
    let change = crate::draft_proposals::ProposalChange {
        id: "h1".into(),
        kind: HunkKind::Del,
        state: HunkState::Accepted,
        before: long,
        after: String::new(),
    };
    let line = super::sections::proposal_hunk_line("prompt.md", &change);
    assert!(
        line.starts_with("[about proposed change h1 to prompt.md: del — accepted — changes: \"")
    );
    assert!(line.contains("…\""), "the excerpt is cut: {line}");
    assert!(!line.contains("offers:"), "a deletion offers no text: {line}");
    assert!(line.chars().count() < 250, "{line}");
}
