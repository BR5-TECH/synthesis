//! Tests for the author's edits of a hunk's proposed text (PCP-FR-EDTW).

use super::per_hunk::two_hunks;
use super::*;

// PCP-FR-EDTW: an edit overwrites one undecided hunk's proposed text, returns
// the new baseline, sets the row's `edited` flag, and is not a change to the
// file: the artifact is untouched and no event is emitted. Putting the agent's
// own text back clears the flag again.
#[test]
fn an_edit_changes_one_hunk_and_nothing_else() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let events = f.watch();
    let loaded = load_hunks_impl(&f.root, &recorded.id).expect("hunks");

    let saved = edit_hunk_impl(&f.root, &recorded.id, &first, "# My review", &loaded.checksum)
        .expect("edited");
    let after = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(saved.checksum, after.checksum, "the checksum is the next baseline");
    assert_ne!(saved.checksum, loaded.checksum);
    assert_eq!(after.hunks[0].after.as_deref(), Some("# My review"));
    assert_eq!(after.hunks[1], loaded.hunks[1], "the other hunk is untouched");
    let record = read_proposal(&f.root, &recorded.id).expect("record");
    assert!(record.ledger[0].edited);
    assert!(!record.ledger[1].edited);
    assert!(record.candidate_edited);
    assert_eq!(record.state, PromptProposalState::Pending);
    assert_eq!(f.artifact_body(), ORIGINAL, "the file is not touched");
    assert!(events.lock().unwrap().is_empty(), "an edit announces nothing");

    edit_hunk_impl(&f.root, &recorded.id, &first, "# Careful review", &saved.checksum)
        .expect("restored");
    let record = read_proposal(&f.root, &recorded.id).expect("record");
    assert!(!record.ledger[0].edited, "the agent's own text clears the flag");
    assert!(!record.candidate_edited);
    let _ = second;
}

// PCP-FR-EDTW: a stale baseline is refused and writes nothing.
#[test]
fn a_stale_edit_writes_nothing() {
    let f = Fixture::new();
    let (recorded, first, second) = two_hunks(&f);
    let baseline = load_hunks_impl(&f.root, &recorded.id).expect("hunks").checksum;
    edit_hunk_impl(&f.root, &recorded.id, &first, "one", &baseline).expect("the first lands");
    assert_eq!(
        edit_hunk_impl(&f.root, &recorded.id, &second, "two", &baseline).unwrap_err(),
        ERR_CANDIDATE_STALE,
    );
    let held = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    assert_eq!(held.hunks[1].after.as_deref(), Some("file two notes"));
    edit_hunk_impl(&f.root, &recorded.id, &second, "two", &held.checksum).expect("fresh lands");
}

// PCP-FR-EDTW: a decided hunk takes no edit, and an unknown one is named so.
#[test]
fn a_decided_hunk_takes_no_edit() {
    let f = Fixture::new();
    let (recorded, first, _) = two_hunks(&f);
    f.reject_hunk(&recorded.id, &first).expect("rejected");
    let baseline = load_hunks_impl(&f.root, &recorded.id).expect("hunks").checksum;
    assert_eq!(
        edit_hunk_impl(&f.root, &recorded.id, &first, "x", &baseline).unwrap_err(),
        ERR_HUNK_ALREADY_DECIDED,
    );
    assert_eq!(
        edit_hunk_impl(&f.root, &recorded.id, "nosuchhunk", "x", &baseline).unwrap_err(),
        ERR_HUNK_NOT_FOUND,
    );
}

// PCP-FR-EDTW, PCP-FR-ZQAH, PCP-FR-RSLV: an accepted edited hunk lands the
// author's text, and the decision comment says the author rewrote it.
#[test]
fn an_edited_hunk_lands_the_authors_text() {
    let f = Fixture::new();
    let proposal = f.pending();
    let hunk = proposal.ledger[0].id.clone();
    let baseline = load_hunks_impl(&f.root, &proposal.id).expect("hunks").checksum;
    let mine = "My own version.\n";
    edit_hunk_impl(&f.root, &proposal.id, &hunk, mine, &baseline).expect("edited");
    let outcome = f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.artifact_body(), "# Review\n\n## Steps\n\nMy own version.\n");
    assert!(outcome.proposal.candidate_edited);
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert!(decision.body.starts_with("Accepted an edited version"), "{}", decision.body);
}
