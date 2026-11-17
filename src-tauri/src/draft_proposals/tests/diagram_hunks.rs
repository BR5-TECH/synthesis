//! Diagram hunks of a draft's proposal (DCP-FR-QRVD, DCP-FR-WSLE, DCP-FR-HXNP,
//! DCP-FR-GMYT, DCP-FR-RKBE, DCP-FR-ZUDT, DCP-FR-NDAQ).

use super::*;
use crate::draft_proposals::anchors::HunkKind;
use crate::draft_proposals::hunks::{HunkState, ProposedHunk};
use crate::text_hunks::updates::UpdateEntry;

const FENCED: &str = "# Spec\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd.\n";
const SOURCE: &str = "flowchart TD\n  A --> B";
const NEW_SOURCE: &str = "flowchart TD\n  A --> C";

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

fn diagram_change(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk::diagram_fields(some(before), some(after), None, None, None, None)
}

fn text_change(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk::from_fields(some(before), some(after), None, None, None, None)
}

/// A fixture whose prompt holds one Mermaid fence.
fn fenced() -> Fixture {
    let f = Fixture::new();
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, FENCED).expect("seed");
    f
}

fn update(f: &Fixture, updates: &[UpdateEntry]) -> Result<DraftChangeProposal, UpdateRefusal> {
    update_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        ProposalUpdate {
            draft_id: &f.draft_id,
            updates,
            rationale: "Asked for. <spoken>Changed.</spoken>",
            agent: &agent(),
            target: f.target(),
            thread_id: &f.thread_id,
        },
    )
}

fn hunks_bytes(f: &Fixture, p: &DraftChangeProposal) -> String {
    std::fs::read_to_string(hunks_path(&f.proposals_dir(), &p.id)).expect("hunks file")
}

fn version_count(f: &Fixture) -> usize {
    crate::draft_history::list_impl(&f.root, &f.draft_id).expect("history").entries.len()
}

// DCP-FR-QRVD, DCP-FR-WSLE, DCP-FR-HXNP: a diagram hunk is recorded in the
// proposal with its flag and fence text, beside a text hunk, and the ledger row
// carries the flag.
#[test]
fn a_diagram_hunk_is_recorded_beside_a_text_hunk() {
    let f = fenced();
    let p = f
        .propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    assert_eq!(p.ledger.len(), 2);
    assert!(p.ledger[0].diagram);
    assert!(!p.ledger[1].diagram);
    assert_eq!(p.ledger[0].kind, HunkKind::Replace);

    let record_toml = std::fs::read_to_string(record_path(&f.proposals_dir(), &p.id)).unwrap();
    assert_eq!(record_toml.matches("diagram = true").count(), 1);
    assert!(!record_toml.contains("diagram = false"));

    let on_disk = hunks_bytes(&f, &p);
    assert_eq!(on_disk.matches("diagram = true").count(), 1);
    let doc = hunks::HunkDocument::from_toml(&on_disk).expect("round trip");
    assert!(doc.hunks[0].diagram && !doc.hunks[1].diagram);
    assert_eq!(doc.hunks[0].before_text(), "```mermaid\nflowchart TD\n  A --> B\n```");
    assert_eq!(doc.hunks[0].after_text(), "```mermaid\nflowchart TD\n  A --> C\n```");
    assert_eq!(doc.to_toml(), on_disk);
}

// DCP-FR-QRVD: a ledger row written before diagram hunks existed reads as a
// text hunk.
#[test]
fn a_ledger_row_without_the_flag_is_a_text_hunk() {
    let old = "id = \"h1\"\nkind = \"replace\"\nstate = \"pending\"\n";
    let row: hunks::HunkLedgerRow = toml::from_str(old).expect("old row");
    assert!(!row.diagram);
    assert!(!toml::to_string(&row).unwrap().contains("diagram"));
}

// DCP-FR-HXNP: a source that no fence holds is refused with the place of the hunk.
#[test]
fn a_diagram_hunk_naming_no_fence_is_refused() {
    let f = fenced();
    let refused = f.propose_hunks(FILE, &[diagram_change("graph LR\n  X --> Y", "x")]);
    assert_eq!(refused.expect_err("refused"), RecordRefusal::HunkAnchorLost { at: 1, of: 1 });
    assert!(pending_for(&f.root, &f.draft_id).is_none());
}

// DCP-FR-ZUDT, DCP-FR-WSLE: accepting a diagram change replaces its fence in one
// transaction that gives one version.
#[test]
fn accepting_a_diagram_change_gives_one_version() {
    let f = fenced();
    let p = f.propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    let before = version_count(&f);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(f.file_body(), "# Spec\n\n```mermaid\nflowchart TD\n  A --> C\n```\n\nEnd.\n");
    assert_eq!(version_count(&f), before + 2, "the original and the accepted change");
    let after = f.reread(&p.id);
    assert_eq!(after.ledger[0].state, HunkState::Accepted);
    assert_eq!(after.state, ProposalState::Accepted);
}

// DCP-FR-ZUDT, DCP-FR-WSLE: accepting a diagram addition inserts a blank line and
// a new fence after the line it follows.
#[test]
fn accepting_a_diagram_addition_inserts_a_fence() {
    let f = fenced();
    let hunk = ProposedHunk::diagram_fields(
        None,
        some("graph LR\n  X --> Y"),
        some("End."),
        None,
        None,
        None,
    );
    let p = f.propose_hunks(FILE, &[hunk]).expect("recorded");
    assert_eq!(p.ledger[0].kind, HunkKind::Add);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(
        f.file_body(),
        "# Spec\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd.\n\n```mermaid\ngraph LR\n  X --> Y\n```\n"
    );
}

// DCP-FR-ZUDT, DCP-FR-WSLE: accepting a diagram removal deletes the fence with
// the line break and the blank line after it.
#[test]
fn accepting_a_diagram_removal_deletes_the_fence() {
    let f = fenced();
    let hunk = ProposedHunk::diagram_fields(some(SOURCE), None, None, None, None, None);
    let p = f.propose_hunks(FILE, &[hunk]).expect("recorded");
    assert_eq!(p.ledger[0].kind, HunkKind::Del);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(f.file_body(), "# Spec\n\nEnd.\n");
}

// DCP-FR-ZUDT: a diagram hunk is rejected on the terms of every hunk, and writes
// nothing.
#[test]
fn rejecting_a_diagram_hunk_writes_nothing() {
    let f = fenced();
    let p = f.propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    f.reject_hunk(&p.id, &p.ledger[0].id.clone()).expect("rejected");
    assert_eq!(f.file_body(), FENCED);
    assert_eq!(f.reread(&p.id).ledger[0].state, HunkState::Rejected);
}

// DCP-FR-ZUDT, DCP-FR-BMLX: a diagram hunk whose fence the author has rewritten
// is lost, and accepting it is the typed `anchor_lost`.
#[test]
fn a_diagram_hunk_whose_fence_is_gone_is_lost() {
    let f = fenced();
    let p = f.propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    let rewritten = "# Spec\n\n```mermaid\nflowchart TD\n  A --> Z\n```\n\nEnd.\n";
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, rewritten).expect("edit");
    assert_eq!(
        f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect_err("refused"),
        ERR_ANCHOR_LOST
    );
    assert_eq!(f.file_body(), rewritten);
    f.reject_hunk(&p.id, &p.ledger[0].id.clone()).expect("rejecting clears it");
}

// DCP-FR-RKBE: the source of a diagram hunk is not the author's to edit. The
// edit writes nothing, emits no event, and leaves `edited` false.
#[test]
fn a_diagram_hunk_is_not_editable() {
    let f = fenced();
    let p = f
        .propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let doc_before = hunks_bytes(&f, &p);
    let record_before = std::fs::read_to_string(record_path(&f.proposals_dir(), &p.id)).unwrap();
    let checksum = hunks::HunkDocument::from_toml(&doc_before).unwrap().checksum();
    let seen = f.watch_proposals();

    let refused = f.edit_hunk(&p.id, &p.ledger[0].id.clone(), "graph LR\n  X --> Y", &checksum);
    assert_eq!(refused.expect_err("refused"), ERR_HUNK_NOT_EDITABLE);
    assert_eq!(hunks_bytes(&f, &p), doc_before);
    assert_eq!(
        std::fs::read_to_string(record_path(&f.proposals_dir(), &p.id)).unwrap(),
        record_before
    );
    assert!(seen.lock().unwrap().is_empty(), "no event");
    assert!(f.reread(&p.id).ledger.iter().all(|row| !row.edited));

    // A text hunk of the same proposal is still editable.
    f.edit_hunk(&p.id, &p.ledger[1].id.clone(), "Done.", &checksum).expect("edited");
}

// DCP-FR-GMYT: a `change` or a `withdraw` across the two subjects is
// `hunk_subject_mismatch` with the place of the update, and writes nothing.
#[test]
fn an_update_across_the_two_subjects_is_refused() {
    let f = fenced();
    let p = f
        .propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let (diagram_id, text_id) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());
    let doc_before = hunks_bytes(&f, &p);
    let comments_before = f.discussion().comments.len();

    let withdraw = |id: &str, diagram: bool| UpdateEntry::Withdraw { id: id.into(), diagram };
    let cases = [
        // A text update of a diagram hunk.
        vec![UpdateEntry::Change { id: diagram_id.clone(), hunk: text_change("End.", "x") }],
        vec![withdraw(&diagram_id, false)],
        // A diagram update of a text hunk, after one entry that is fine.
        vec![
            withdraw(&diagram_id, true),
            UpdateEntry::Change { id: text_id.clone(), hunk: diagram_change(SOURCE, "x") },
        ],
        vec![withdraw(&diagram_id, true), withdraw(&text_id, true)],
    ];
    let expected = [1, 1, 2, 2];
    for (entries, at) in cases.iter().zip(expected) {
        assert_eq!(
            update(&f, entries).expect_err("refused"),
            UpdateRefusal::HunkSubjectMismatch { at, of: entries.len() }
        );
    }
    assert_eq!(hunks_bytes(&f, &p), doc_before, "nothing was written");
    assert_eq!(f.discussion().comments.len(), comments_before, "no comment");
    assert_eq!(
        UpdateRefusal::HunkSubjectMismatch { at: 1, of: 1 }.code(),
        ERR_HUNK_SUBJECT_MISMATCH
    );
}

// DCP-FR-GMYT, DCP-FR-IAFV: a diagram update changes a diagram hunk in place,
// withdraws it, and adds a diagram hunk to the open proposal.
#[test]
fn a_diagram_update_changes_adds_and_withdraws_diagram_hunks() {
    let f = fenced();
    let p = f
        .propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let diagram_id = p.ledger[0].id.clone();

    let after = update(
        &f,
        &[UpdateEntry::Change {
            id: diagram_id.clone(),
            hunk: diagram_change(SOURCE, "graph LR\n  Q --> R"),
        }],
    )
    .expect("updated");
    assert_eq!(after.ledger[0].revision, 1);
    assert!(after.ledger[0].diagram);
    let doc = hunks::HunkDocument::from_toml(&hunks_bytes(&f, &p)).unwrap();
    assert!(doc.hunks[0].diagram);
    assert_eq!(doc.hunks[0].after_text(), "```mermaid\ngraph LR\n  Q --> R\n```");

    let added = ProposedHunk::diagram_fields(
        None,
        some("graph TD\n  M --> N"),
        some("# Spec"),
        None,
        None,
        None,
    );
    let after = update(&f, &[UpdateEntry::Add(added)]).expect("added");
    assert_eq!(after.ledger.len(), 3);
    assert!(after.ledger[2].diagram, "an added diagram hunk is a diagram hunk");

    let after =
        update(&f, &[UpdateEntry::Withdraw { id: diagram_id, diagram: true }]).expect("withdrawn");
    assert_eq!(after.ledger[0].state, HunkState::Withdrawn);
    assert!(after.ledger[0].diagram);
}

// DCP-FR-HXNP: an update that leaves the source as it is changes nothing.
#[test]
fn a_diagram_update_without_a_change_is_refused() {
    let f = fenced();
    let p = f.propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    let same = diagram_change(SOURCE, &format!("{SOURCE}\n"));
    let entries = [UpdateEntry::Change { id: p.ledger[0].id.clone(), hunk: same }];
    assert_eq!(update(&f, &entries), Err(UpdateRefusal::NoChange { at: 1, of: 1 }));
}

// DCP-FR-NDAQ: what an agent reads of a diagram hunk is its source and its flag.
#[test]
fn an_agent_reads_the_source_of_a_diagram_hunk() {
    let f = fenced();
    f.propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let read = read_proposal_changes(&f.root, &f.draft_id).expect("read");
    let changes = &read[0].changes;
    assert!(changes[0].diagram);
    assert_eq!(changes[0].before, SOURCE);
    assert_eq!(changes[0].after, NEW_SOURCE);
    assert!(!changes[1].diagram);
    assert_eq!(changes[1].before, "End.");
}

// DCP-FR-NDAQ: the hunks the surface loads carry `diagram`, absent for a text hunk.
#[test]
fn the_loaded_hunks_carry_the_flag() {
    let f = fenced();
    let p = f
        .propose_hunks(FILE, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let read = load_hunks_impl(&f.root, &p.id).expect("loaded");
    let json = serde_json::to_value(&read.hunks).unwrap();
    assert_eq!(json[0]["diagram"], true);
    assert!(json[1].get("diagram").is_none());
}
