//! Tests for an agent's revision of one hunk (PCP-FR-RVSN).

use super::hunks::hunk;
use super::per_hunk::two_hunks;
use super::*;

// PCP-FR-RVSN: a revision replaces one undecided hunk in place — same id, same
// position — bumps its revision, and leaves every other hunk as it was. It
// writes nothing into the file and appends no comment.
#[test]
fn a_revision_replaces_one_hunk_in_place() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let before = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    let comments_before = f.discussion().comments.len();

    let revised =
        revise_prompt_change_hunk(&f.root, PROMPT, &second, &hunk("a note", "three notes", None))
            .expect("revised");
    let after = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(after.hunks.len(), 2);
    assert_eq!(after.hunks[0], before.hunks[0], "the other hunk is untouched");
    assert_eq!(after.hunks[1].id, second, "the id a discussion names is kept");
    assert_eq!(after.hunks[1].after.as_deref(), Some("three notes"));
    assert_eq!(after.hunks[1].revision, 1);
    assert_eq!(revised.ledger[1].revision, 1);
    assert_eq!(revised.ledger[1].state, HunkState::Pending);
    assert!(!revised.ledger[1].edited);
    assert_eq!(revised.ledger[0].id, first);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert_eq!(f.discussion().comments.len(), comments_before);
    assert_eq!(read_proposal(&f.root, &recorded.id).unwrap(), revised);
}

// PCP-FR-RVSN: a rejected hunk can be revised, and is undecided again; an
// accepted one cannot.
#[test]
fn a_rejected_hunk_is_revisable_and_an_accepted_one_is_not() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    f.reject_hunk(&recorded.id, &first).expect("rejected");
    f.accept_hunk(&recorded.id, &second).expect("accepted");
    assert_eq!(revision_target(&f.root, PROMPT, &second), RevisionTarget::NothingStanding);

    let g = Fixture::new();
    let (other, first, second) = two_hunks(&g);
    g.reject_hunk(&other.id, &first).expect("rejected");
    hold_hunk_impl(&g.root, &other.id, &second, true).expect("held");
    assert_eq!(revision_target(&g.root, PROMPT, &first), RevisionTarget::Revisable);
    let revised =
        revise_prompt_change_hunk(&g.root, PROMPT, &first, &hunk("# Review", "# Review!", None))
            .expect("revised");
    assert_eq!(revised.ledger[0].state, HunkState::Pending, "undecided again");
    assert!(revised.ledger[0].decided_at.is_none());

    g.accept_hunk(&other.id, &first).expect("accepted");
    assert_eq!(revision_target(&g.root, PROMPT, &first), RevisionTarget::NoSuchChange);
    assert_eq!(
        revise_prompt_change_hunk(&g.root, PROMPT, &first, &hunk("x", "y", None)).unwrap_err(),
        ERR_HUNK_ALREADY_DECIDED,
    );
    assert_eq!(revision_target(&g.root, PROMPT, "nosuchhunk"), RevisionTarget::NoSuchChange);
    assert_eq!(
        revise_prompt_change_hunk(&g.root, PROMPT, "nosuchhunk", &hunk("x", "y", None))
            .unwrap_err(),
        ERR_HUNK_NOT_FOUND,
    );
}

// PCP-FR-RVSN: the replacement is placed against the file as a recording is,
// and refuses with the same typed errors.
#[test]
fn a_revision_refuses_text_the_file_does_not_hold_once() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    let before = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(
        revise_prompt_change_hunk(&f.root, PROMPT, &first, &hunk("Not there", "x", None))
            .unwrap_err(),
        ERR_ANCHOR_LOST,
    );
    assert_eq!(
        revise_prompt_change_hunk(&f.root, PROMPT, &first, &hunk("file", "x", None)).unwrap_err(),
        ERR_HUNK_AMBIGUOUS,
    );
    let long = "x".repeat(201);
    assert_eq!(
        revise_prompt_change_hunk(&f.root, PROMPT, &first, &hunk("# Review", "x", Some(&long)))
            .unwrap_err(),
        format!("{ERR_HUNK_CONTEXT_TOO_LONG}:lead"),
    );
    assert_eq!(load_hunks_impl(&f.root, &recorded.id).unwrap(), before, "nothing was written");
}
