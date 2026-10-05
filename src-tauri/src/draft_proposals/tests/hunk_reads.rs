//! Reading a proposal as hunks (DCP-FR-10, DCP-FR-QLMH, DCP-FR-BMLX).

use super::*;
use crate::draft_proposals::anchors::HunkKind;
use crate::draft_proposals::hunks::{HunkDocument, ProposalHunk};
use crate::draft_proposals::{load_hunks_impl, read_proposal_changes, HunkPlacement};

/// A hunk document written beside a proposal, standing in for what hunk-native
/// recording will write.
fn write_hunks(fx: &Fixture, proposal_id: &str, doc: &HunkDocument) {
    let dir = fx.proposals_dir();
    fx.root
        .write_text_atomic(dir.join(format!("{proposal_id}.hunks")), &doc.to_toml())
        .expect("hunk document written");
}

/// A proposal as an older build left it: a whole-document candidate in
/// `<id>.content`, with no hunk document beside it. Such a proposal arrives
/// through Git long after this build stopped writing them (DCP-FR-QLMH).
fn make_legacy(fx: &Fixture, proposal_id: &str, candidate: &str) {
    let dir = fx.proposals_dir();
    fx.root
        .write_text_atomic(dir.join(format!("{proposal_id}.content")), candidate)
        .expect("candidate written");
    fx.root
        .delete_under(&dir, format!("{proposal_id}.hunks"), false)
        .expect("hunk document removed");
}

fn replace(before: &str, after: &str) -> crate::draft_proposals::hunks::ProposedHunk {
    crate::draft_proposals::hunks::ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        note: None,
    }
}

/// A record as a build **before** proposals held changes wrote it: no ledger, no
/// hunk count, no counts, and no base checksum.
///
/// This is what actually arrives on a pull (DCP-FR-02), and the shape every
/// legacy claim has to hold against — a fixture that kept the ledger would be
/// testing this build's own writer.
fn strip_to_old_record(fx: &Fixture, proposal_id: &str) {
    let path = fx.proposals_dir().join(format!("{proposal_id}.toml"));
    let text = fx.root.read_text(&path).expect("record");
    let mut kept: Vec<&str> = Vec::new();
    let mut dropping = false;
    for line in text.lines() {
        if line.starts_with('[') {
            dropping = line.starts_with("[[ledger]]") || line.starts_with("[counts]");
        }
        if dropping {
            continue;
        }
        if line.starts_with("hunkCount") || line.starts_with("baseSha256") {
            continue;
        }
        kept.push(line);
    }
    fx.root
        .write_text_atomic(&path, &format!("{}\n", kept.join("\n")))
        .expect("old record written");
}

fn replace_hunk(id: &str, before: &str, after: &str, lead: &str, trail: &str, at: usize) -> ProposalHunk {
    ProposalHunk {
        id: id.to_string(),
        kind: HunkKind::Replace,
        revision: 0,
        note: None,
        before: Some(before.to_string()),
        after: Some(after.to_string()),
        anchor: crate::draft_proposals::anchors::HunkAnchor {
            lead: lead.to_string(),
            trail: trail.to_string(),
            hint_start: at,
            hint_end: at + before.len(),
        },
    }
}

// DCP-FR-QLMH: a proposal held as a whole document reads as one change covering
// the prompt, and says which kind it is so the surface can state that its text
// is not editable.
#[test]
fn a_whole_document_proposal_reads_as_one_change_over_the_prompt() {
    let fx = Fixture::new();
    let record = fx.pending();
    make_legacy(&fx, &record.id, PROPOSED);

    let read = load_hunks_impl(&fx.root, &record.id).expect("hunks");

    assert!(read.legacy, "it is served as legacy rather than migrated");
    assert_eq!(read.hunks.len(), 1);
    assert_eq!(read.hunks[0].kind, HunkKind::Replace);
    assert_eq!(read.hunks[0].before_text(), ORIGINAL);
    assert_eq!(read.hunks[0].after_text(), PROPOSED);
    assert_eq!(
        read.resolutions[0],
        HunkPlacement::Resolved { start: 0, end: ORIGINAL.len() },
        "it covers the whole prompt, which is what a whole-document candidate meant"
    );
}

// DCP-FR-QLMH: nothing is written back, so a repository read by two builds does
// not corrupt.
#[test]
fn reading_a_whole_document_proposal_writes_nothing() {
    let fx = Fixture::new();
    let record = fx.pending();
    make_legacy(&fx, &record.id, PROPOSED);
    let dir = fx.proposals_dir();

    load_hunks_impl(&fx.root, &record.id).expect("hunks");

    assert!(
        fx.root.read_text(dir.join(format!("{}.hunks", record.id))).is_err(),
        "no hunk document is created for a legacy proposal"
    );
    assert!(fx.root.read_text(dir.join(format!("{}.content", record.id))).is_ok());
}

// DCP-FR-10: a hunk document beside the proposal is what is served, and the
// legacy candidate is not consulted.
#[test]
fn a_hunk_document_is_preferred_over_a_whole_document_candidate() {
    let fx = Fixture::new();
    let record = fx.pending();
    make_legacy(&fx, &record.id, PROPOSED);
    let doc = HunkDocument::new(vec![replace_hunk(
        "h1",
        "original",
        "revised",
        "The ",
        " three",
        ORIGINAL.find("original").unwrap(),
    )]);
    write_hunks(&fx, &record.id, &doc);

    let read = load_hunks_impl(&fx.root, &record.id).expect("hunks");

    assert!(!read.legacy);
    assert_eq!(read.hunks.len(), 1);
    assert_eq!(read.hunks[0].id, "h1");
    assert_eq!(read.checksum, doc.checksum(), "the baseline an edit is checked against");
}

// DCP-FR-06 / DCP-FR-10: a hunk is placed against the prompt **as it stands**,
// so an edit above it moves its placement rather than losing it.
#[test]
fn a_hunk_is_placed_against_the_prompt_as_it_stands() {
    let fx = Fixture::new();
    let record = fx.pending();
    let at = ORIGINAL.find("original").unwrap();
    write_hunks(
        &fx,
        &record.id,
        &HunkDocument::new(vec![replace_hunk("h1", "original", "revised", "The ", " three", at)]),
    );

    let before_edit = load_hunks_impl(&fx.root, &record.id).expect("hunks");
    let HunkPlacement::Resolved { start, .. } = before_edit.resolutions[0] else {
        panic!("resolved before the edit");
    };

    // The author types a line above the passage the hunk is about.
    let grown = format!("A NEW OPENING LINE\n{ORIGINAL}");
    drafts::save_draft_file_impl(&fx.root, &fx.draft_id, FILE, &grown).expect("saved");

    let after_edit = load_hunks_impl(&fx.root, &record.id).expect("hunks");
    let HunkPlacement::Resolved { start: moved, end } = after_edit.resolutions[0] else {
        panic!("the hunk is still placeable after the author typed above it");
    };
    assert!(moved > start, "the placement moved down with the text");
    assert_eq!(&grown[moved..end], "original");
}

// DCP-FR-BMLX: a hunk whose text the author rewrote is lost, and lost is derived
// on every read — so undoing that edit resolves it again.
#[test]
fn a_hunk_is_lost_when_its_text_goes_and_resolves_again_when_it_returns() {
    let fx = Fixture::new();
    let record = fx.pending();
    let at = ORIGINAL.find("original").unwrap();
    write_hunks(
        &fx,
        &record.id,
        &HunkDocument::new(vec![replace_hunk("h1", "original", "revised", "The ", " three", at)]),
    );

    let rewritten = "# Spec\n\nCompletely different prose now.\n";
    drafts::save_draft_file_impl(&fx.root, &fx.draft_id, FILE, rewritten).expect("saved");
    assert_eq!(
        load_hunks_impl(&fx.root, &record.id).expect("hunks").resolutions[0],
        HunkPlacement::Lost,
        "the text it changes is no longer in the prompt"
    );

    drafts::save_draft_file_impl(&fx.root, &fx.draft_id, FILE, ORIGINAL).expect("saved");
    assert!(
        !matches!(
            load_hunks_impl(&fx.root, &record.id).expect("hunks").resolutions[0],
            HunkPlacement::Lost
        ),
        "lost is derived, never stored, so undoing the edit brings the hunk back"
    );
}

// DCP-FR-22: a proposal id that names nothing is a typed refusal rather than a
// panic or an empty answer.
#[test]
fn an_unknown_proposal_is_refused() {
    let fx = Fixture::new();
    assert_eq!(
        load_hunks_impl(&fx.root, "no-such-proposal"),
        Err(ERR_PROPOSAL_NOT_FOUND.to_string())
    );
}

// DCP-FR-09: a record written before the ledger existed still **reads** — its
// new fields default rather than the whole record failing to parse.
//
// The record is stripped to the shape an older build actually wrote, because a
// record this build's own writer produced would be testing the writer.
#[test]
fn a_record_written_by_an_older_build_still_reads() {
    let fx = Fixture::new();
    let record = fx.pending();
    strip_to_old_record(&fx, &record.id);

    let listed = list_proposals_impl(&fx.root, &fx.draft_id).expect("listed");
    let found = listed.iter().find(|p| p.id == record.id).expect("still listed");
    assert_eq!(found.state, ProposalState::Pending, "it still says where it stands");
    assert_eq!(found.path, record.path);
    // The fields it never carried default rather than refusing the record.
    assert!(found.ledger.is_empty());
    assert_eq!(found.hunk_count, 0);
    assert!(found.base_sha256.is_none());
}

// DCP-FR-09: a record this build wrote carries its ledger, its count and the
// prompt the agent read.
#[test]
fn a_record_this_build_wrote_carries_its_ledger() {
    let fx = Fixture::new();
    let record = fx.pending();

    let listed = list_proposals_impl(&fx.root, &fx.draft_id).expect("listed");
    let found = listed.iter().find(|p| p.id == record.id).expect("still listed");
    assert_eq!(found.hunk_count, 1);
    assert_eq!(found.ledger.len(), 1);
    assert_eq!(found.counts.pending, 1);
    assert_eq!(found.counts.undecided(), 1, "an undecided change holds the draft's one slot");
    assert!(found.base_sha256.is_some(), "the prompt as the agent read it");
}

// DCP-FR-QLMH / DCR-FR-KDSV: a proposal an older build wrote — a whole-document
// candidate, no hunk file, and a record with no ledger at all — is read as one
// change and **decided on the ordinary terms**.
//
// This is not a migration path that can be skipped: `proposals/` is committed
// and travels through Git, so such a proposal arrives on a pull long after this
// build stopped writing them. Refused, it would hold the draft's one pending
// slot with no way to release it.
#[test]
fn a_proposal_an_older_build_wrote_is_accepted_on_the_ordinary_terms() {
    let f = Fixture::new();
    let p = f.propose_hunks(FILE, &[replace("original", "revised")]).expect("recorded");
    make_legacy(&f, &p.id, "# Spec\n\nThe whole thing, rewritten.\n");
    strip_to_old_record(&f, &p.id);

    // Read: one change covering the whole prompt, and the surface is told which
    // kind it is holding.
    let read = crate::draft_proposals::load_hunks_impl(&f.root, &p.id).expect("hunks");
    assert!(read.legacy, "the surface is told it is a legacy proposal");
    assert_eq!(read.hunks.len(), 1);
    let hunk_id = read.hunks[0].id.clone();

    // And listing it reports the one change rather than none, so the review bar
    // says `1 of 1` rather than `0 of 0`.
    let listed = crate::draft_proposals::list_proposals_impl(&f.root, &f.draft_id).expect("list");
    let row = listed.iter().find(|r| r.id == p.id).expect("listed");
    assert_eq!(row.state, ProposalState::Pending);

    // Accept: the prompt becomes the candidate, exactly as this build's
    // whole-file acceptance always did.
    f.accept_hunk(&p.id, &hunk_id).expect("a legacy change is accepted");
    assert_eq!(f.file_body(), "# Spec\n\nThe whole thing, rewritten.\n");
    assert_eq!(f.reread(&p.id).state, ProposalState::Accepted);
}

// DCP-FR-QLMH: rejecting a legacy proposal clears it and writes nothing.
#[test]
fn a_proposal_an_older_build_wrote_is_rejected_on_the_ordinary_terms() {
    let f = Fixture::new();
    let p = f.propose_hunks(FILE, &[replace("original", "revised")]).expect("recorded");
    make_legacy(&f, &p.id, "# Spec\n\nThe whole thing, rewritten.\n");
    strip_to_old_record(&f, &p.id);
    let before = f.file_body();

    let read = crate::draft_proposals::load_hunks_impl(&f.root, &p.id).expect("hunks");
    f.reject_hunk(&p.id, &read.hunks[0].id.clone()).expect("a legacy change is rejected");

    assert_eq!(f.file_body(), before, "rejecting writes nothing into the draft");
    assert_eq!(f.reread(&p.id).state, ProposalState::Rejected);
}

// DCP-FR-QLMH: a legacy proposal's text is not editable, and it says so as
// itself rather than by claiming the change does not exist.
#[test]
fn a_proposal_an_older_build_wrote_takes_no_edit() {
    let f = Fixture::new();
    let p = f.propose_hunks(FILE, &[replace("original", "revised")]).expect("recorded");
    make_legacy(&f, &p.id, "# Spec\n\nThe whole thing, rewritten.\n");
    strip_to_old_record(&f, &p.id);

    let read = crate::draft_proposals::load_hunks_impl(&f.root, &p.id).expect("hunks");
    let refused = f.edit_hunk(&p.id, &read.hunks[0].id.clone(), "mine", &read.checksum);
    assert_eq!(
        refused.unwrap_err(),
        crate::draft_proposals::ERR_WRITE_FAILED,
        "refused as a text that cannot be written, not as a change that is not there",
    );
}

// ---------------------------------------------------------------------------
// The read a turn assembles its input from (DCP-FR-HNWD)
// ---------------------------------------------------------------------------

fn replacing(before: &str, after: &str) -> hunks::ProposedHunk {
    hunks::ProposedHunk {
        kind: HunkKind::Replace,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        note: None,
    }
}

// DCP-FR-HNWD — every change of every proposal, with the state the author left it in
#[test]
fn read_proposal_changes_carries_each_change_and_how_it_was_decided() {
    let f = Fixture::new();
    let proposal = f
        .propose_hunks(FILE, &[replacing("original", "revised"), replacing("paragraphs", "sections")])
        .expect("recorded");
    let first = proposal.ledger[0].id.clone();
    let second = proposal.ledger[1].id.clone();
    f.accept_hunk(&proposal.id, &first).expect("accepted");
    f.reject_hunk(&proposal.id, &second).expect("rejected");

    let read = read_proposal_changes(&f.root, &f.draft_id).expect("read");
    assert_eq!(read.len(), 1, "the draft holds one proposal");
    let changes = &read[0].changes;
    assert_eq!(read[0].path, FILE);
    assert_eq!(changes.len(), 2, "both changes are carried, decided or not");

    // The text each change names is what tells a later turn which passage of the
    // draft this conversation has already settled.
    assert_eq!(changes[0].state, hunks::HunkState::Accepted);
    assert_eq!(changes[0].before, "original");
    assert_eq!(changes[0].after, "revised");
    assert_eq!(changes[1].state, hunks::HunkState::Rejected);
    assert_eq!(changes[1].before, "paragraphs");

    assert_eq!(read[0].counts.accepted, 1);
    assert_eq!(read[0].counts.rejected, 1);
}

// DCP-FR-HNWD — an undecided change reads as undecided rather than as absent
#[test]
fn read_proposal_changes_carries_a_change_nobody_has_decided() {
    let f = Fixture::new();
    f.pending();

    let read = read_proposal_changes(&f.root, &f.draft_id).expect("read");
    let changes = &read[0].changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].state, hunks::HunkState::Pending);
}

/// Every file under the draft's own directory, by relative path, with its bytes.
///
/// A snapshot of the whole tree rather than of the two files a read is expected
/// to touch: a write to `history/`, to the journal, or to the comment log would
/// otherwise go unseen by the assertion that is supposed to catch exactly that.
fn draft_tree(fx: &Fixture) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(
        fx: &Fixture,
        dir: &std::path::Path,
        prefix: &str,
        out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        let Ok(entries) = fx.root.list_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = dir.join(&entry.name);
            let rel = format!("{prefix}{}", entry.name);
            if entry.kind == crate::fs::EntryKind::Dir {
                walk(fx, &path, &format!("{rel}/"), out);
            } else if let Ok(bytes) = fx.root.read_bytes(&path) {
                out.insert(rel, bytes);
            }
        }
    }
    let dir = crate::drafts::draft_dir(&fx.root, &fx.draft_id).expect("draft dir");
    let mut out = std::collections::BTreeMap::new();
    walk(fx, &dir, "", &mut out);
    out
}

// DCP-FR-HNWD — the read writes nothing and reconciles nothing, so a turn never
// changes the draft it reads (CVL-FR-09)
#[test]
fn read_proposal_changes_writes_nothing_and_reconciles_nothing() {
    let f = Fixture::new();
    f.pending();
    // A journal standing over the draft is what reconciliation consumes
    // (DHS-FR-18). A read that ran one would clear or act on this file; a read
    // that runs none leaves it exactly where it is.
    let history = crate::drafts::draft_dir(&f.root, &f.draft_id)
        .expect("draft dir")
        .join(crate::drafts::HISTORY_DIR);
    f.root
        .write_text_atomic(history.join("journal.toml"), "operationId = \"stand-here\"\n")
        .expect("journal planted");

    let before = draft_tree(&f);
    assert!(
        before.keys().any(|k| k.contains("journal.toml")),
        "the journal is part of what the snapshot covers",
    );

    read_proposal_changes(&f.root, &f.draft_id).expect("read");
    read_proposal_changes(&f.root, &f.draft_id).expect("read again");

    assert_eq!(
        draft_tree(&f),
        before,
        "no file of the draft is created, changed, or removed by reading it",
    );
}

// DCP-FR-HNWD — a draft holding no proposal reads as an empty list, not an error
#[test]
fn read_proposal_changes_of_a_draft_with_no_proposals_is_empty() {
    let f = Fixture::new();
    assert!(read_proposal_changes(&f.root, &f.draft_id).expect("read").is_empty());
}

// DCP-FR-HNWD / DCP-FR-QLMH — a decided legacy proposal reads as decided
#[test]
fn read_proposal_changes_settles_a_legacy_proposal_from_its_record() {
    let f = Fixture::new();
    let proposal = f.pending();
    f.accept(&proposal.id).expect("accepted");
    // A proposal as an older build left it: a whole-document candidate, and a
    // record carrying neither a ledger nor counts — only the state the decision
    // put on the proposal as a whole.
    make_legacy(&f, &proposal.id, "Anything at all.");
    strip_to_old_record(&f, &proposal.id);

    let read = read_proposal_changes(&f.root, &f.draft_id).expect("read");
    assert_eq!(read.len(), 1);
    assert_eq!(
        read[0].changes.len(),
        1,
        "a legacy proposal is one whole-document change",
    );
    // Without settling the synthesised row from the record, an already-decided
    // proposal reads as undecided — and an agent told a settled passage is still
    // open proposes into it again, which is what AGC-FR-RVQP exists to stop.
    assert_eq!(
        read[0].changes[0].state,
        hunks::HunkState::Accepted,
        "the change carries the state the record holds",
    );
    assert_eq!(read[0].counts.accepted, 1, "and the counts agree with it");
    assert_eq!(read[0].counts.rejected, 0);
}
