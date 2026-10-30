//! Tests for a change recorded as hunks (PCP-FR-CNVT, PCP-FR-CUTC).

use super::*;
use crate::text_hunks::hunks::ProposedHunk;

pub(super) fn hunk(before: &str, after: &str, lead: Option<&str>) -> ProposedHunk {
    ProposedHunk::from_fields(
        Some(before.into()),
        Some(after.into()),
        None,
        lead.map(Into::into),
        None,
        None,
    )
}

impl Fixture {
    pub(super) fn propose_hunks(
        &self,
        hunks: &[ProposedHunk],
    ) -> Result<PromptChangeProposal, RecordRefusal> {
        record_prompt_proposal(
            &self.handle(),
            &self.root,
            &self.root,
            NewPromptProposal {
                artifact_id: PROMPT,
                change: ProposedChange::Hunks(hunks),
                rationale: "The steps run together.",
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }
}

/// The hunks that turn `ORIGINAL` into `PROPOSED`.
fn to_proposed() -> Vec<ProposedHunk> {
    vec![hunk(
        "Read the file, summarise it and file a note.",
        "Read the file.\n\nSummarise it, then file a note.",
        None,
    )]
}

// PCP-FR-CNVT, PCP-FR-HKLD: a `hunks` change is recorded as those hunks, each
// with its own ledger row, and the comment carries the one prompt proposal
// reference.
#[test]
fn hunks_are_recorded_as_hunks_with_one_ledger_row_each() {
    let f = Fixture::new();
    let two =
        [hunk("# Review", "# Careful review", None), hunk("file a note", "file two notes", None)];
    let recorded = f.propose_hunks(&two).expect("recorded");
    assert_eq!(recorded.hunk_count, 2);
    assert_eq!(recorded.ledger.len(), 2);
    assert!(recorded.ledger.iter().all(|row| row.state == HunkState::Pending));
    assert_eq!(recorded.counts, HunkCounts { pending: 2, ..HunkCounts::default() });
    assert_eq!(
        recorded.base_sha256.as_deref(),
        Some(fs::sha256_bytes(ORIGINAL.as_bytes()).as_str())
    );
    assert!(!recorded.legacy);
    assert_eq!(f.artifact_body(), ORIGINAL, "the artifact is not touched");

    let loaded = load_hunks_impl(&f.root, &recorded.id).expect("hunks");
    let ids: Vec<&str> = loaded.hunks.iter().map(|h| h.id.as_str()).collect();
    let rows: Vec<&str> = recorded.ledger.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, rows, "the ledger names the hunks in proposal order");
    assert_eq!(loaded.hunks[0].before.as_deref(), Some("# Review"));
    assert_eq!(loaded.hunks[1].after.as_deref(), Some("file two notes"));

    let discussion = f.discussion();
    let comment = discussion.comments.last().expect("the proposal's comment");
    assert_eq!(comment.body, "The steps run together.");
    assert!(matches!(
        comment.attachments.as_slice(),
        [Attachment::PromptProposal { proposal_id, .. }] if *proposal_id == recorded.id
    ));
}

// PCP-FR-CNVT: a `content` change is converted into hunks at record time, and
// gives the same text a `hunks` change does.
#[test]
fn a_content_change_is_converted_into_hunks_that_give_the_same_text() {
    let by_hunks = Fixture::new();
    let recorded = by_hunks.propose_hunks(&to_proposed()).expect("recorded");
    let by_content = Fixture::new();
    let whole = by_content.propose(PROPOSED).expect("recorded");

    assert!(whole.hunk_count >= 1, "the full text became changes");
    assert!(!whole.legacy);
    assert!(!by_content.dir().join(format!("{}.content", whole.id)).exists(), "no candidate file");
    assert!(by_content.dir().join(format!("{}.hunks", whole.id)).is_file());
    let built = candidate_for_test(&by_hunks.root, &recorded.id).expect("candidate");
    let converted = candidate_for_test(&by_content.root, &whole.id).expect("candidate");
    assert_eq!(built, PROPOSED);
    assert_eq!(converted, PROPOSED);
}

// PCP-FR-CNVT: the hunks resolve against the file as it is on disk, with a lead
// that names one copy of a repeated text.
#[test]
fn hunks_resolve_against_the_file_on_disk_with_their_lead() {
    let f = Fixture::new();
    write_file(&f.root, PROMPT, "# A\nSame line.\n# B\nSame line.\n");
    let recorded =
        f.propose_hunks(&[hunk("Same line.", "Other line.", Some("# B\n"))]).expect("recorded");
    assert_eq!(
        candidate_for_test(&f.root, &recorded.id).expect("candidate"),
        "# A\nSame line.\n# B\nOther line.\n",
    );
}

// PCP-FR-CUTC: every hunk refusal names the place, and writes and appends
// nothing.
#[test]
fn a_hunk_that_cannot_be_built_records_nothing_and_names_its_place() {
    let f = Fixture::new();
    write_file(&f.root, PROMPT, "one two one\n");
    let comments_before = f.discussion().comments.len();
    let long = "x".repeat(201);
    let cases: Vec<(Vec<ProposedHunk>, RecordRefusal)> = vec![
        (Vec::new(), RecordRefusal::NoHunks),
        (
            vec![hunk("two", "2", None), hunk("three", "3", None)],
            RecordRefusal::HunkAnchorLost { at: 2, of: 2 },
        ),
        (vec![hunk("one", "1", None)], RecordRefusal::HunkAmbiguous { at: 1, of: 1 }),
        (
            vec![hunk("two", "2", None), hunk("o one", "x", None)],
            RecordRefusal::HunkOverlap { at: 2, of: 2 },
        ),
        (
            vec![hunk("two", "2", Some(&long))],
            RecordRefusal::HunkContextTooLong {
                at: 1,
                of: 1,
                side: crate::text_hunks::hunks::ContextSide::Lead,
            },
        ),
    ];
    // PPC-FR-KWQB: a long `trail` alone and both contexts carry their side.
    let side = |lead: Option<&str>, trail: Option<&str>| {
        vec![ProposedHunk::from_fields(
            Some("two".into()),
            Some("2".into()),
            None,
            lead.map(Into::into),
            trail.map(Into::into),
            None,
        )]
    };
    let too_long = |side| RecordRefusal::HunkContextTooLong { at: 1, of: 1, side };
    use crate::text_hunks::hunks::ContextSide;
    let cases = cases.into_iter().chain([
        (side(None, Some(&long)), too_long(ContextSide::Trail)),
        (side(Some(&long), Some(&long)), too_long(ContextSide::Both)),
    ]);
    for (hunks, refusal) in cases {
        assert_eq!(f.propose_hunks(&hunks).unwrap_err(), refusal);
        assert!(f.folder_files().is_empty(), "nothing written for {refusal:?}");
        assert_eq!(f.discussion().comments.len(), comments_before, "nothing appended");
    }
}

// PCP-FR-07: `no_change` applies to the text the hunks give.
#[test]
fn hunks_that_change_nothing_are_the_no_change_refusal() {
    let f = Fixture::new();
    let same = hunk("Read the file", "Read the file", None);
    assert_eq!(f.propose_hunks(&[same]).unwrap_err(), RecordRefusal::NoChange);
    assert!(f.folder_files().is_empty());
}

// PCP-FR-CNVT: a CRLF file resolves, and the text the hunks give takes the line
// endings the write path would give it.
#[test]
fn a_crlf_file_resolves_its_hunks() {
    let f = Fixture::new();
    let crlf = ORIGINAL.replace('\n', "\r\n");
    write_file(&f.root, PROMPT, &crlf);
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    let built = candidate_for_test(&f.root, &recorded.id).expect("candidate");
    assert_eq!(built, normalise_for_write(&f.root, PROPOSED));
}

fn set_crlf(root: &crate::fs::RootFs) {
    crate::project_settings::save_project_config_to(
        root,
        crate::project_settings::ProjectConfig {
            line_endings: crate::project_settings::LineEndings::Crlf,
            draft_template: None,
            ..Default::default()
        },
    )
    .expect("config");
}

// PCP-FR-ZQAH: in a CRLF project an accepted hunk lands in CRLF, whatever line
// endings the hunk was written with.
#[test]
fn a_crlf_project_lands_an_accepted_hunk_in_crlf() {
    let f = Fixture::new();
    set_crlf(&f.root);
    write_file(&f.root, PROMPT, &ORIGINAL.replace('\n', "\r\n"));
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    f.accept(&recorded.id).expect("accepted");
    assert_eq!(f.artifact_body(), PROPOSED.replace('\n', "\r\n"));
}

// PCP-FR-07, PPC-FR-08: hunks that change nothing on a CRLF file in a CRLF
// project are the no-change refusal.
#[test]
fn identity_hunks_on_a_crlf_file_change_nothing() {
    let f = Fixture::new();
    set_crlf(&f.root);
    write_file(&f.root, PROMPT, &ORIGINAL.replace('\n', "\r\n"));
    let same = hunk("Read the file", "Read the file", None);
    assert_eq!(f.propose_hunks(&[same]).unwrap_err(), RecordRefusal::NoChange);
    assert!(f.folder_files().is_empty());
}
