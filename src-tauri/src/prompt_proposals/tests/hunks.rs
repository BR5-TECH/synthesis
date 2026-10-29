//! Tests for a candidate built from hunks (PCP-FR-CVNA, PCP-FR-CUTC,
//! PCP-FR-ENRL).

use super::*;
use crate::text_hunks::hunks::ProposedHunk;

fn hunk(before: &str, after: &str, lead: Option<&str>) -> ProposedHunk {
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
    fn propose_hunks(&self, hunks: &[ProposedHunk]) -> Result<PromptChangeProposal, RecordRefusal> {
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

// PCP-FR-CVNA, PCP-FR-ENRL: hunks build the whole candidate, and it is recorded
// exactly as the same text passed whole would be.
#[test]
fn hunks_record_the_same_candidate_as_the_whole_text() {
    let by_hunks = Fixture::new();
    let recorded = by_hunks.propose_hunks(&to_proposed()).expect("recorded");
    let by_content = Fixture::new();
    let whole = by_content.propose(PROPOSED).expect("recorded");

    let built = load_content_impl(&by_hunks.root, &recorded.id).expect("candidate").content;
    let passed = load_content_impl(&by_content.root, &whole.id).expect("candidate").content;
    assert_eq!(built, PROPOSED);
    assert_eq!(built, passed);
    assert_eq!(recorded.state, PromptProposalState::Pending);
    assert_eq!(recorded.path, whole.path);
    assert_eq!(by_hunks.artifact_body(), ORIGINAL, "the artifact is not touched");

    // PCP-FR-ENRL: the record and the comment carry no trace of the hunks.
    let record_text = std::fs::read_to_string(by_hunks.dir().join(format!("{}.toml", recorded.id)))
        .expect("record");
    assert!(!record_text.contains("hunk"), "{record_text}");
    let discussion = by_hunks.discussion();
    let comment = discussion.comments.last().expect("the proposal's comment");
    assert_eq!(comment.body, "The steps run together.");
    assert!(matches!(
        comment.attachments.as_slice(),
        [Attachment::PromptProposal { proposal_id, .. }] if *proposal_id == recorded.id
    ));
}

// PCP-FR-CVNA: the hunks resolve against the file as it is on disk, with a lead
// that names one copy of a repeated text.
#[test]
fn hunks_resolve_against_the_file_on_disk_with_their_lead() {
    let f = Fixture::new();
    write_file(&f.root, PROMPT, "# A\nSame line.\n# B\nSame line.\n");
    let recorded =
        f.propose_hunks(&[hunk("Same line.", "Other line.", Some("# B\n"))]).expect("recorded");
    assert_eq!(
        load_content_impl(&f.root, &recorded.id).expect("candidate").content,
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
        (vec![hunk("two", "2", Some(&long))], RecordRefusal::HunkContextTooLong { at: 1, of: 1 }),
    ];
    for (hunks, refusal) in cases {
        assert_eq!(f.propose_hunks(&hunks).unwrap_err(), refusal);
        assert!(f.folder_files().is_empty(), "nothing written for {refusal:?}");
        assert_eq!(f.discussion().comments.len(), comments_before, "nothing appended");
    }
}

// PCP-FR-ENRL, PCP-FR-07: `no_change` applies to the built candidate.
#[test]
fn hunks_that_change_nothing_are_the_no_change_refusal() {
    let f = Fixture::new();
    let same = hunk("Read the file", "Read the file", None);
    assert_eq!(f.propose_hunks(&[same]).unwrap_err(), RecordRefusal::NoChange);
    assert!(f.folder_files().is_empty());
}

// PCP-FR-CVNA: a CRLF file resolves, and the built candidate takes the line
// endings the write path would give it, so the comparison is not every line.
#[test]
fn a_crlf_file_builds_a_candidate_in_the_projects_line_endings() {
    let f = Fixture::new();
    let crlf = ORIGINAL.replace('\n', "\r\n");
    write_file(&f.root, PROMPT, &crlf);
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    let built = load_content_impl(&f.root, &recorded.id).expect("candidate").content;
    assert_eq!(built, normalise_for_write(&f.root, PROPOSED));
}

// PCP-FR-ENRL, PCP-FR-13: a candidate built from hunks is decided as a whole
// file. Accepting it after the file changed on disk replaces the file with the
// candidate; the hunks are not applied again to the new text.
#[test]
fn an_accepted_hunk_candidate_replaces_the_file_whole() {
    let f = Fixture::new();
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    write_file(&f.root, PROMPT, &format!("{ORIGINAL}\nA line added on disk.\n"));
    f.accept(&recorded.id).expect("accepted");
    assert_eq!(f.artifact_body(), PROPOSED, "the candidate, not the hunks on the new text");
}

// PCP-FR-ENRL, PCP-FR-15: declining a candidate built from hunks writes
// nothing into the project.
#[test]
fn a_declined_hunk_candidate_leaves_the_file_as_it_is() {
    let f = Fixture::new();
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    f.decline(&recorded.id).expect("declined");
    assert_eq!(f.artifact_body(), ORIGINAL);
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

// PCP-FR-CVNA: in a CRLF project the built candidate is CRLF, whatever line
// endings the hunks were written with.
#[test]
fn a_crlf_project_builds_a_crlf_candidate() {
    let f = Fixture::new();
    set_crlf(&f.root);
    write_file(&f.root, PROMPT, &ORIGINAL.replace('\n', "\r\n"));
    let recorded = f.propose_hunks(&to_proposed()).expect("recorded");
    let built = load_content_impl(&f.root, &recorded.id).expect("candidate").content;
    assert_eq!(built, PROPOSED.replace('\n', "\r\n"));
}

// PCP-FR-ENRL, PPC-FR-08: hunks that change nothing on a CRLF file in a CRLF
// project are the no-change refusal, not a comparison with every line in it.
#[test]
fn identity_hunks_on_a_crlf_file_change_nothing() {
    let f = Fixture::new();
    set_crlf(&f.root);
    write_file(&f.root, PROMPT, &ORIGINAL.replace('\n', "\r\n"));
    let same = hunk("Read the file", "Read the file", None);
    assert_eq!(f.propose_hunks(&[same]).unwrap_err(), RecordRefusal::NoChange);
    assert!(f.folder_files().is_empty());
}
