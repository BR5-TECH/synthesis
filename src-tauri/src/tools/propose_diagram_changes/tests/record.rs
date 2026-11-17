//! Recording a proposal of diagram hunks, on a draft origin and on an artifact
//! origin (PDM-FR-UITL, PDM-FR-FZGT, PDM-FR-JLYU, PDM-FR-DWAF, PDM-FR-RNUS).

use super::*;
use crate::draft_proposals::anchors::HunkKind;

const FENCED_A: &str = "```mermaid\nflowchart TD\n  A --> B\n```";

// PDM-FR-UITL, PDM-FR-FZGT, PDM-FR-JLYU: a change of a diagram records one
// proposal and one comment. The hunk is a diagram hunk of the right kind, and
// the file is not rewritten.
#[test]
fn a_change_of_a_diagram_records_one_proposal_and_one_comment() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let comments = f.comment_count();
        let output = f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");

        assert!(output.proposed);
        assert!(f.proposed.load(Ordering::SeqCst), "the turn ends on a proposal (CVL-FR-15)");
        let standing = f.one();
        assert_eq!(standing.hunks.len(), 1);
        let hunk = &standing.hunks[0];
        assert!(hunk.diagram, "{subject:?}: a diagram hunk");
        assert_eq!(hunk.kind, HunkKind::Replace);
        assert_eq!(hunk.before_text(), FENCED_A);
        assert_eq!(hunk.after_text(), "```mermaid\nflowchart TD\n  A --> C\n```");
        assert_eq!(hunk.readable_before(), SRC_A);
        assert_eq!(hunk.readable_after(), SRC_A2);
        assert_eq!(standing.rows.len(), 1);
        assert_eq!(standing.rows[0].1, "Pending");
        assert!(standing.rows[0].2, "the ledger row carries the diagram flag");

        assert_eq!(f.comment_count(), comments + 1, "exactly one comment");
        assert_eq!(f.body(), BODY, "the file is not rewritten");
    }
}

// PDM-FR-UITL: the removal of a diagram is a diagram hunk that holds `before`
// alone.
#[test]
fn a_removal_of_a_diagram_records_a_deletion_hunk() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![removal(SRC_A)]).expect("recorded");

        let standing = f.one();
        let hunk = &standing.hunks[0];
        assert!(hunk.diagram);
        assert_eq!(hunk.kind, HunkKind::Del);
        assert!(hunk.after.is_none());
        assert!(hunk.before_text().starts_with(FENCED_A), "{subject:?}");
        assert_eq!(f.body(), BODY);
        assert_eq!(f.comment_count(), 2);
    }
}

// PDM-FR-UITL, PDM-FR-DWAF: the addition of a diagram is a diagram hunk that
// holds `after`, and the new fence goes after the line `after_text` names.
#[test]
fn an_addition_of_a_diagram_records_an_insertion_hunk() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![addition(SRC_B, "End line.")]).expect("recorded");

        let standing = f.one();
        let hunk = &standing.hunks[0];
        assert!(hunk.diagram);
        assert_eq!(hunk.kind, HunkKind::Add);
        assert!(hunk.before.is_none());
        assert_eq!(hunk.after_text(), "\n\n```mermaid\ngraph LR\n  X --> Y\n```");
        assert_eq!(f.body(), BODY);
        assert_eq!(f.comment_count(), 2);
    }
}

// PDM-FR-UITL: a call of several diagram changes records ONE proposal that
// holds them all, and posts ONE comment. The order is the order sent.
#[test]
fn several_diagram_changes_make_one_proposal_and_one_comment() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2), addition(SRC_B, "End line.")]).expect("recorded");

        let standing = f.one();
        assert_eq!(standing.hunks.len(), 2);
        assert_eq!(standing.hunks[0].kind, HunkKind::Replace);
        assert_eq!(standing.hunks[1].kind, HunkKind::Add);
        assert!(standing.hunks.iter().all(|h| h.diagram));
        assert!(standing.rows.iter().all(|row| row.2));
        assert_eq!(f.comment_count(), 2, "one comment for the call");
    }
}

// PDM-FR-FZGT, PDM-FR-UITL: the comment is the rationale as the model wrote it,
// by the turn's own agent, with exactly one attachment of the kind of the
// origin that names the proposal and carries no update counts.
#[test]
fn the_comment_is_the_rationale_and_one_attachment_by_the_agent() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let standing = f.one();

        let thread = f.discussion();
        let posted = thread.comments.last().expect("the proposal's comment");
        assert_eq!(posted.body, RATIONALE, "the rationale and nothing else");
        assert_eq!(posted.author, agent(), "the participant of the turn's agent (PDC-FR-13)");
        let (kind, named, update) = f.last_attachment();
        assert_eq!(kind, f.attachment_kind());
        assert_eq!(named, standing.id);
        assert_eq!(update, None, "a recording carries no update counts");
        for marker in [&standing.id[..], f.file(), "propose_diagram_changes", "```"] {
            assert!(
                !posted.body.contains(marker),
                "the body carries no identifier or marker: {marker:?}",
            );
        }
    }
}

// PDM-FR-FZGT: the call changes nothing outside the proposal store and the
// conversation's own log. The file, the version history, and the other files
// of the project are as they were.
#[test]
fn a_successful_call_changes_only_the_proposal_store_and_the_conversation_log() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let before = tree_of(&f.path);
        f.propose(vec![change(SRC_A, SRC_A2), removal("flowchart TD\n  A --> B")])
            .expect_err("two changes name one diagram");
        assert_eq!(tree_of(&f.path), before, "a refusal adds no file");

        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let after = tree_of(&f.path);
        let removed: Vec<&String> = before.iter().filter(|p| !after.contains(*p)).collect();
        assert!(removed.is_empty(), "nothing was removed: {removed:?}");
        let added: Vec<&String> = after.iter().filter(|p| !before.contains(*p)).collect();
        assert!(!added.is_empty(), "the record was written");
        for path in &added {
            assert!(
                path.contains("/proposals/")
                    || path.ends_with(".jsonl")
                    || path.starts_with(".synthesis/"),
                "{subject:?}: only the proposal and the conversation log: {path}",
            );
        }
        assert_eq!(f.body(), BODY, "and the file is as it was");
    }
}

// PDM-FR-RNUS: two sources are equal when they hold the same text once trailing
// white space is cut from both, and line endings are normalised first.
#[test]
fn a_source_is_found_after_trailing_white_space_and_line_endings_are_normalised() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change("flowchart TD\r\n  A --> B  \r\n", SRC_A2)]).expect("recorded");
        assert_eq!(f.one().hunks[0].before_text(), FENCED_A);
    }
    // The file itself may hold line endings of the other kind.
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, &BODY.replace('\n', "\r\n"));
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        assert!(f.one().hunks[0].diagram);
    }
}

// PDM-FR-RNUS: a fence of another language is never found, and the Mermaid
// fence beside it is.
#[test]
fn a_fence_of_another_language_is_never_found() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, OTHER_FENCE);
        let before = f.world();
        let refusal = f.propose(vec![change("nodes: [a, b]", SRC_A2)]).unwrap_err();
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 1, of: 1 },
        );
        f.assert_untouched(&before);

        f.propose(vec![change(SRC_A, SRC_A2)]).expect("the Mermaid fence is found");
    }
}

// PDM-FR-FZGT, PDM-FR-JLYU: a proposal that cannot be written is the retryable
// `Other` refusal, nothing is left behind, and the turn runs on.
#[test]
fn a_proposal_that_cannot_be_written_is_the_retryable_other_refusal() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let sealed_dir = match subject {
            Subject::Draft => {
                let proposals = crate::drafts::draft_proposals_dir(&f.root(), &f.draft_id)
                    .expect("the proposals folder");
                let proposals =
                    if proposals.is_absolute() { proposals } else { f.path.join(proposals) };
                // The folder of the proposals where it stands, else the folder
                // that would hold it.
                if proposals.is_dir() {
                    proposals
                } else {
                    proposals.parent().expect("the draft folder").to_path_buf()
                }
            }
            Subject::Artifact => f.path.join(".synthesis"),
        };
        let before = f.world();
        let restore = std::fs::metadata(&sealed_dir).expect("dir").permissions();
        let mut sealed = restore.clone();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            sealed.set_mode(0o500);
        }
        #[cfg(not(unix))]
        sealed.set_readonly(true);
        std::fs::set_permissions(&sealed_dir, sealed).expect("seal");

        let refusal = f.propose(vec![change(SRC_A, SRC_A2)]).expect_err("refused");
        std::fs::set_permissions(&sealed_dir, restore).expect("unseal");

        assert_eq!(refusal, ToolRefusal::ProposalNotRecorded, "{subject:?}");
        assert_eq!(refusal.kind(), ToolErrorKind::Other);
        assert!(refusal.retryable(), "a write that failed once may succeed next time");
        f.assert_untouched(&before);
    }
}
