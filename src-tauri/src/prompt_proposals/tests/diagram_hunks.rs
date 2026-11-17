//! Diagram hunks of a prompt change proposal (PCP-FR-QRVD, PCP-FR-WSGE,
//! PCP-FR-KDTA, PCP-FR-ZHLN, PCP-FR-VNPS).

use super::*;
use crate::text_hunks::anchors::HunkKind;
use crate::text_hunks::hunks::ProposedHunk;
use crate::text_hunks::updates::UpdateEntry;

const FENCED: &str = "# Review\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd.\n";
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
    write_file(&f.root, PROMPT, FENCED);
    f
}

fn propose(f: &Fixture, hunks: &[ProposedHunk]) -> Result<PromptChangeProposal, RecordRefusal> {
    record_prompt_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        NewPromptProposal {
            artifact_id: PROMPT,
            change: ProposedChange::Hunks(hunks),
            rationale: "The picture is wrong.",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
}

fn update(f: &Fixture, updates: &[UpdateEntry]) -> Result<PromptChangeProposal, UpdateRefusal> {
    update_prompt_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        PromptProposalUpdate {
            artifact_id: PROMPT,
            updates,
            rationale: "Asked for. <spoken>Changed.</spoken>",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
}

fn hunks_bytes(f: &Fixture, p: &PromptChangeProposal) -> String {
    std::fs::read_to_string(at(&f.dir(), &p.id, HUNKS_EXT)).expect("hunks file")
}

// PCP-FR-QRVD: a diagram hunk is recorded beside a text hunk with its flag and
// fence text, and the ledger row carries the flag.
#[test]
fn a_diagram_hunk_is_recorded_beside_a_text_hunk() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    assert!(p.ledger[0].diagram);
    assert!(!p.ledger[1].diagram);
    assert_eq!(p.ledger[0].kind, HunkKind::Replace);

    let record_toml = std::fs::read_to_string(at(&f.dir(), &p.id, RECORD_EXT)).unwrap();
    assert_eq!(record_toml.matches("diagram = true").count(), 1);
    let on_disk = hunks_bytes(&f, &p);
    assert_eq!(on_disk.matches("diagram = true").count(), 1);
    let doc = crate::text_hunks::hunks::HunkDocument::from_toml(&on_disk).expect("round trip");
    assert_eq!(doc.hunks[0].before_text(), "```mermaid\nflowchart TD\n  A --> B\n```");
    assert_eq!(doc.hunks[0].after_text(), "```mermaid\nflowchart TD\n  A --> C\n```");
    assert_eq!(doc.to_toml(), on_disk);
    assert!(pending_for(&f.root, PROMPT).is_some());
}

// PCP-FR-QRVD: a ledger row written before diagram hunks existed is a text hunk.
#[test]
fn a_ledger_row_without_the_flag_is_a_text_hunk() {
    let old = "id = \"h1\"\nkind = \"replace\"\nstate = \"pending\"\n";
    let row: HunkLedgerRow = toml::from_str(old).expect("old row");
    assert!(!row.diagram);
    assert!(!toml::to_string(&row).unwrap().contains("diagram"));
}

// PCP-FR-QRVD: a source no fence holds is refused with the place of the hunk.
#[test]
fn a_diagram_hunk_naming_no_fence_is_refused() {
    let f = fenced();
    let refused = propose(&f, &[diagram_change("graph LR\n  X --> Y", "x")]);
    assert_eq!(refused.expect_err("refused"), RecordRefusal::HunkAnchorLost { at: 1, of: 1 });
}

// PCP-FR-ZHLN: accepting a diagram change writes the artifact once, with the
// fence replaced and nothing else touched.
#[test]
fn accepting_a_diagram_change_writes_the_artifact() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(f.artifact_body(), "# Review\n\n```mermaid\nflowchart TD\n  A --> C\n```\n\nEnd.\n");
    assert_eq!(read_proposal(&f.root, &p.id).unwrap().state, PromptProposalState::Accepted);
}

// PCP-FR-ZHLN: accepting a diagram addition inserts a blank line and a new fence.
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
    let p = propose(&f, &[hunk]).expect("recorded");
    assert_eq!(p.ledger[0].kind, HunkKind::Add);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(
        f.artifact_body(),
        "# Review\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd.\n\n```mermaid\ngraph LR\n  X --> Y\n```\n"
    );
}

// PCP-FR-ZHLN: accepting a diagram removal deletes the fence and the blank line
// after it.
#[test]
fn accepting_a_diagram_removal_deletes_the_fence() {
    let f = fenced();
    let hunk = ProposedHunk::diagram_fields(some(SOURCE), None, None, None, None, None);
    let p = propose(&f, &[hunk]).expect("recorded");
    assert_eq!(p.ledger[0].kind, HunkKind::Del);
    f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect("accepted");
    assert_eq!(f.artifact_body(), "# Review\n\nEnd.\n");
}

// PCP-FR-ZHLN: a fence that is not in the file is the typed `anchor_lost`, and
// rejecting the hunk clears it.
#[test]
fn a_diagram_hunk_whose_fence_is_gone_is_lost() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE)]).expect("recorded");
    let rewritten = "# Review\n\n```mermaid\nflowchart TD\n  A --> Z\n```\n\nEnd.\n";
    write_file(&f.root, PROMPT, rewritten);
    assert_eq!(
        f.accept_hunk(&p.id, &p.ledger[0].id.clone()).expect_err("refused"),
        ERR_ANCHOR_LOST
    );
    assert_eq!(f.artifact_body(), rewritten);
    f.reject_hunk(&p.id, &p.ledger[0].id.clone()).expect("rejecting clears it");
}

// PCP-FR-KDTA: the source of a diagram hunk is not the author's to edit. The
// edit writes nothing, emits no event, and leaves `edited` false.
#[test]
fn a_diagram_hunk_is_not_editable() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let loaded = load_hunks_impl(&f.root, &p.id).expect("hunks");
    let doc_before = hunks_bytes(&f, &p);
    let record_before = std::fs::read_to_string(at(&f.dir(), &p.id, RECORD_EXT)).unwrap();
    let events = f.watch();

    let refused =
        edit_hunk_impl(&f.root, &p.id, &p.ledger[0].id, "graph LR\n  X --> Y", &loaded.checksum);
    assert_eq!(refused.unwrap_err(), ERR_HUNK_NOT_EDITABLE);
    assert_eq!(hunks_bytes(&f, &p), doc_before);
    assert_eq!(std::fs::read_to_string(at(&f.dir(), &p.id, RECORD_EXT)).unwrap(), record_before);
    assert!(events.lock().unwrap().is_empty(), "no event");
    assert!(read_proposal(&f.root, &p.id).unwrap().ledger.iter().all(|row| !row.edited));

    // A text hunk of the same proposal is still editable.
    edit_hunk_impl(&f.root, &p.id, &p.ledger[1].id, "Done.", &loaded.checksum).expect("edited");
}

// PCP-FR-WSGE: a `change` or a `withdraw` across the two subjects is
// `hunk_subject_mismatch` with the place of the update, and writes nothing.
#[test]
fn an_update_across_the_two_subjects_is_refused() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let (diagram_id, text_id) = (p.ledger[0].id.clone(), p.ledger[1].id.clone());
    let doc_before = hunks_bytes(&f, &p);
    let comments_before = f.discussion().comments.len();

    let withdraw = |id: &str, diagram: bool| UpdateEntry::Withdraw { id: id.into(), diagram };
    let cases = [
        vec![UpdateEntry::Change { id: diagram_id.clone(), hunk: text_change("End.", "x") }],
        vec![withdraw(&diagram_id, false)],
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

// PCP-FR-WSGE: a diagram update changes a diagram hunk in place, adds one, and
// withdraws one.
#[test]
fn a_diagram_update_changes_adds_and_withdraws_diagram_hunks() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let diagram_id = p.ledger[0].id.clone();

    let changed = diagram_change(SOURCE, "graph LR\n  Q --> R");
    let after = update(&f, &[UpdateEntry::Change { id: diagram_id.clone(), hunk: changed }])
        .expect("updated");
    assert_eq!(after.ledger[0].revision, 1);
    assert!(after.ledger[0].diagram);
    let doc = crate::text_hunks::hunks::HunkDocument::from_toml(&hunks_bytes(&f, &p)).unwrap();
    assert_eq!(doc.hunks[0].after_text(), "```mermaid\ngraph LR\n  Q --> R\n```");

    let added = ProposedHunk::diagram_fields(
        None,
        some("graph TD\n  M --> N"),
        some("# Review"),
        None,
        None,
        None,
    );
    let after = update(&f, &[UpdateEntry::Add(added)]).expect("added");
    assert_eq!(after.ledger.len(), 3);
    assert!(after.ledger[2].diagram);

    let after =
        update(&f, &[UpdateEntry::Withdraw { id: diagram_id, diagram: true }]).expect("withdrawn");
    assert_eq!(after.ledger[0].state, HunkState::Withdrawn);
}

// PCP-FR-VNPS: what an agent reads of a diagram hunk is its source and its flag,
// and the hunks the surface loads carry `diagram`, absent for a text hunk.
#[test]
fn reads_carry_the_flag_and_the_source() {
    let f = fenced();
    let p = propose(&f, &[diagram_change(SOURCE, NEW_SOURCE), text_change("End.", "Fin.")])
        .expect("recorded");
    let read = read_prompt_proposal_changes(&f.root, &p.id).expect("read");
    assert!(read.changes[0].diagram);
    assert_eq!(read.changes[0].before, SOURCE);
    assert_eq!(read.changes[0].after, NEW_SOURCE);
    assert!(!read.changes[1].diagram);
    assert_eq!(read.changes[1].before, "End.");

    let loaded = load_hunks_impl(&f.root, &p.id).expect("loaded");
    let json = serde_json::to_value(&loaded.hunks).unwrap();
    assert_eq!(json[0]["diagram"], true);
    assert!(json[1].get("diagram").is_none());
}
