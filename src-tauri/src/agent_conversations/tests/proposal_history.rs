//! A decided proposal in the history a request carries.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// A decided proposal in the history (AGC-FR-RVQP)
// ---------------------------------------------------------------------------

/// Record one change against the draft's discussion, and decide it.
///
/// Each proposal is decided before the next is recorded, because a draft holds
/// at most one undecided proposal at a time (DCP-FR-08).
fn propose_and_decide(
    h: &Harness,
    draft_id: &str,
    thread_id: &str,
    before: &str,
    after: &str,
    accept: bool,
) {
    let proposal = crate::draft_proposals::record_proposal(
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
    .expect("recorded");
    if accept {
        crate::draft_proposals::accept_hunk_for_test(
            &h.app.handle().clone(),
            &h.root(),
            &h.root(),
            &proposal.id,
            &proposal.ledger[0].id,
            &human("ada"),
        )
        .expect("accepted");
    } else {
        crate::draft_proposals::decline_for_test(
            &h.app.handle().clone(),
            &h.root(),
            &h.root(),
            &proposal.id,
            &human("ada"),
        )
        .expect("declined");
    }
}

/// AGC-FR-RVQP: the history names each change and how the author decided it.
#[test]
fn a_decided_proposal_names_its_changes_and_their_outcomes() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up", "@arch anything else?"]);
    // The seeded prompt is `Loose notes.`, which both changes are placed against.
    propose_and_decide(&h, &draft_id, &thread.id, "Loose", "Tidy", true);
    propose_and_decide(&h, &draft_id, &thread.id, "notes", "records", false);

    let thread = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let trigger = thread.comments.last().expect("a comment").id.clone();
    let sections = build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);

    assert!(
        history.body.contains("1. replace — accepted — changed: \"Loose\""),
        "the accepted change carries its own kind, outcome and text: {}",
        history.body,
    );
    assert!(
        history.body.contains("1. replace — rejected — changed: \"notes\""),
        "the rejected change carries its own kind, outcome and text: {}",
        history.body,
    );
    // The excerpt of the text a change touched is what tells a later turn which
    // passage of the draft this conversation has already settled.
    assert!(history.body.contains("\"Loose\""), "the accepted change names its text");
    assert!(history.body.contains("\"notes\""), "the rejected change names its text");
    assert!(
        !history.body.contains("[proposed a new version of"),
        "the bare line is replaced, not kept beside the rendering",
    );
}

/// AGC-FR-RVQP: an accepted change's new text already stands in `artifact`, so
/// the history never carries it a second time; a rejected one's does, because
/// nothing else in the request says what was refused.
#[test]
fn only_a_refused_change_carries_the_text_it_offered() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up", "@arch anything else?"]);
    propose_and_decide(&h, &draft_id, &thread.id, "Loose", "ACCEPTEDTEXT", true);
    propose_and_decide(&h, &draft_id, &thread.id, "notes", "REFUSEDTEXT", false);

    let thread = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let trigger = thread.comments.last().expect("a comment").id.clone();
    let sections = build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);

    assert!(
        history.body.contains("REFUSEDTEXT"),
        "the refused text is what nothing else in the request carries: {}",
        history.body,
    );
    assert!(
        !history.body.contains("ACCEPTEDTEXT"),
        "the accepted text is in the artifact section and is not repeated here: {}",
        history.body,
    );
}

/// AGC-FR-RVQP: the proposals rendered in full are bounded, and an older one
/// contributes its counts alone — so a long conversation never carries the draft
/// many times over.
#[test]
fn an_older_proposal_contributes_its_counts_alone() {
    let h = Harness::new(vec![]);
    let (draft_id, thread) = seed_discussion(&h, &["@arch shape this up", "@arch anything else?"]);
    // Five changes decided in turn, each placed against what the last one left.
    // The seeded prompt is `Loose notes.`.
    propose_and_decide(&h, &draft_id, &thread.id, "Loose", "OLDESTMARK", true);
    propose_and_decide(&h, &draft_id, &thread.id, "OLDESTMARK", "SECONDMARK", true);
    propose_and_decide(&h, &draft_id, &thread.id, "SECONDMARK", "THIRDMARK", true);
    propose_and_decide(&h, &draft_id, &thread.id, "THIRDMARK", "FOURTHMARK", false);
    propose_and_decide(&h, &draft_id, &thread.id, "THIRDMARK", "FIFTHMARK", false);

    let thread = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let trigger = thread.comments.last().expect("a comment").id.clone();
    let sections = build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);

    // The two oldest references keep their count line and lose their changes.
    assert!(
        !history.body.contains("\"OLDESTMARK\""),
        "the oldest proposal contributes its counts alone: {}",
        history.body,
    );
    // …and the newest still carry theirs, or the bound has eaten the very thing
    // the rendering exists for.
    assert!(
        history.body.contains("\"THIRDMARK\""),
        "a recent proposal still names the text it changed: {}",
        history.body,
    );
    assert_eq!(
        history.body.matches("[proposed 1 change to").count(),
        5,
        "every reference keeps its count line: {}",
        history.body,
    );
}

/// One change, for the rendering tests that do not need a draft on disk.
fn change(
    kind: crate::draft_proposals::anchors::HunkKind,
    state: crate::draft_proposals::hunks::HunkState,
    before: &str,
    after: &str,
) -> crate::draft_proposals::ProposalChange {
    crate::draft_proposals::ProposalChange {
        kind,
        state,
        before: before.into(),
        after: after.into(),
    }
}

fn digest_of(changes: Vec<crate::draft_proposals::ProposalChange>) -> super::sections::ProposalDigest {
    let counts = crate::draft_proposals::hunks::HunkCounts::of(changes.iter().map(|c| c.state));
    super::sections::ProposalDigest {
        counts,
        changes: Some(changes),
    }
}

/// AGC-FR-RVQP: every change names its kind, whichever kind it is.
#[test]
fn each_kind_of_change_is_named_by_its_own_word() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::HunkState;
    let line = super::sections::proposal_digest_line(
        "spec.md",
        &digest_of(vec![
            change(HunkKind::Replace, HunkState::Rejected, "old", "new"),
            change(HunkKind::Add, HunkState::Rejected, "", "inserted"),
            change(HunkKind::Del, HunkState::Rejected, "gone", ""),
        ]),
    );
    assert!(line.contains("1. replace — rejected"), "{line}");
    assert!(line.contains("2. add — rejected"), "{line}");
    assert!(line.contains("3. del — rejected"), "{line}");
    // A deletion offered no text, so it names only the text it removed.
    assert!(line.contains("3. del — rejected — changed: \"gone\""), "{line}");
}

/// AGC-FR-RVQP: an accepted insertion names no existing text, so it carries what
/// it inserted — a change with no text at all would name no passage, which is the
/// one thing this rendering exists to do.
#[test]
fn an_accepted_insertion_still_names_a_passage() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::HunkState;
    let line = super::sections::proposal_digest_line(
        "spec.md",
        &digest_of(vec![
            change(HunkKind::Add, HunkState::Accepted, "", "INSERTED"),
            change(HunkKind::Replace, HunkState::Accepted, "old", "REPLACEMENT"),
        ]),
    );
    assert!(line.contains("INSERTED"), "an accepted insertion names its text: {line}");
    // …while an accepted replacement still withholds it, that text being in the
    // artifact section already and its `before` naming the passage well enough.
    assert!(
        !line.contains("REPLACEMENT"),
        "an accepted replacement does not repeat the artifact: {line}",
    );
}

/// AGC-FR-RVQP / AGC-FR-11: the changes of one proposal are bounded, so a large
/// proposal cannot fill the section and truncate the comments after it.
#[test]
fn a_proposal_of_many_changes_names_only_the_bounded_number_of_them() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::HunkState;
    let many: Vec<_> = (0..40)
        .map(|i| change(HunkKind::Replace, HunkState::Rejected, &format!("text{i}"), "x"))
        .collect();
    let line = super::sections::proposal_digest_line("spec.md", &digest_of(many));

    let listed = line.matches(" — rejected").count();
    assert_eq!(listed, super::sections::CHANGES_RENDERED_PER_PROPOSAL, "{line}");
    assert!(line.contains("…and 28 more, not listed here"), "{line}");
    // The header still counts every change, decided or not.
    assert!(line.contains("[proposed 40 changes to spec.md"), "{line}");
}

/// AGC-FR-RVQP: an excerpt is bounded and is always one line, so the change after
/// it cannot read as part of it.
#[test]
fn an_excerpt_is_bounded_and_folded_to_one_line() {
    let long = "word ".repeat(200);
    let rendered = super::sections::excerpt(&long);
    assert!(!rendered.contains('\n'), "an excerpt never spans lines: {rendered}");
    assert!(rendered.ends_with("…\""), "a cut excerpt says it was cut: {rendered}");
    // The quotes and the marker are the only characters beyond the bound.
    assert_eq!(
        rendered.chars().count(),
        super::sections::CHANGE_EXCERPT_MAX_CHARS + 3,
        "{rendered}",
    );

    // Multi-byte text is cut on a character boundary rather than panicking.
    let wide = "日本語のテキスト ".repeat(40);
    assert!(super::sections::excerpt(&wide).ends_with("…\""));

    // A multi-line change folds into one line rather than forging a new row.
    let folded = super::sections::excerpt("first line\n  2. replace — accepted");
    assert_eq!(folded, "\"first line 2. replace — accepted\"");
}
