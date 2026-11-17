//! A call with `diagrams` joins the open proposal of the file, or records one
//! where none is open (PDM-FR-WLBN, PDM-FR-UITL, PDM-FR-IGRM).

use super::*;
use crate::draft_proposals::anchors::HunkKind;

// PDM-FR-WLBN: with an open proposal that holds a text hunk, a call with
// `diagrams` adds the diagram hunk to it. There is no second proposal, no
// refusal that one is pending, and one comment that carries the update counts.
#[test]
fn a_diagram_change_joins_the_open_proposal_of_text() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.offer_text("Intro line.", "Intro text.").expect("a text proposal stands");
        let standing = f.one();
        let comments = f.comment_count();
        assert!(!f.proposed.load(Ordering::SeqCst));

        let output = f.propose(vec![change(SRC_A, SRC_A2)]).expect("joined");
        assert!(output.proposed);
        assert!(f.proposed.load(Ordering::SeqCst), "a join ends the turn (CVL-FR-15)");

        let joined = f.one();
        assert_eq!(joined.id, standing.id, "no second proposal, {subject:?}");
        assert_eq!(joined.hunks.len(), 2);
        // The store places the anchors of every hunk again on an update, so the
        // text hunk is compared by what it changes.
        assert_eq!(joined.hunks[0].id, standing.hunks[0].id, "the text hunk is as it was");
        assert_eq!(joined.hunks[0].before, standing.hunks[0].before);
        assert_eq!(joined.hunks[0].after, standing.hunks[0].after);
        assert!(!joined.hunks[0].diagram);
        assert!(joined.hunks[1].diagram);
        assert_eq!(joined.hunks[1].kind, HunkKind::Replace);
        let states: Vec<&str> = joined.rows.iter().map(|row| row.1.as_str()).collect();
        assert_eq!(states, ["Pending", "Pending"], "the author decides each on its own");
        assert_eq!(f.body(), BODY, "a join writes no file");

        assert_eq!(f.comment_count(), comments + 1, "exactly one comment");
        let (kind, named, update) = f.last_attachment();
        assert_eq!(kind, f.attachment_kind());
        assert_eq!(named, standing.id);
        assert_eq!(update, Some(ProposalUpdateCounts { changed: 0, added: 1, withdrawn: 0 }));
        assert_eq!(f.discussion().comments.last().expect("comment").body, RATIONALE);
    }
}

// PDM-FR-WLBN: every diagram change of the call is added, in one call, with one
// comment. The counts of the comment say how many.
#[test]
fn several_diagram_changes_join_with_one_comment() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.offer_text("Intro line.", "Intro text.").expect("a text proposal stands");
        let comments = f.comment_count();

        f.propose(vec![change(SRC_A, SRC_A2), addition(SRC_B, "End line.")]).expect("joined");

        let joined = f.one();
        assert_eq!(joined.hunks.len(), 3);
        assert_eq!(joined.hunks.iter().filter(|h| h.diagram).count(), 2);
        assert_eq!(f.comment_count(), comments + 1);
        let (_, _, update) = f.last_attachment();
        assert_eq!(update, Some(ProposalUpdateCounts { changed: 0, added: 2, withdrawn: 0 }));
    }
}

// PDM-FR-WLBN: it joins whichever agent made the proposal. The comment of the
// join is the comment of the agent of the turn.
#[test]
fn a_proposal_of_another_agent_is_joined() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.offer_text_as(other_agent(), "Intro line.", "Intro text.").expect("recorded");
        let standing = f.one();

        f.propose(vec![change(SRC_A, SRC_A2)]).expect("joined");

        assert_eq!(f.one().id, standing.id);
        let thread = f.discussion();
        let authors: Vec<&Participant> =
            thread.comments.iter().skip(1).map(|c| &c.author).collect();
        assert_eq!(authors, [&other_agent(), &agent()]);
    }
}

// PDM-FR-WLBN: the open proposal may hold diagram hunks alone.
#[test]
fn a_diagram_change_joins_a_proposal_of_diagrams() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        f.proposed.store(false, Ordering::SeqCst);
        let standing = f.one();

        f.propose(vec![addition(SRC_B, "End line.")]).expect("joined");

        let joined = f.one();
        assert_eq!(joined.id, standing.id);
        assert_eq!(joined.hunks.len(), 2);
        assert!(joined.hunks.iter().all(|h| h.diagram));
    }
}

// PDM-FR-WLBN: where the file holds no proposal that is open, the call records
// one. A proposal whose every hunk is withdrawn is not open.
#[test]
fn a_settled_proposal_is_not_joined_and_a_new_one_is_recorded() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let first = f.one();
        f.proposed.store(false, Ordering::SeqCst);
        f.call(f.updating(vec![entry("withdraw", Some(&first.hunks[0].id), None, None, None)]))
            .expect("withdrawn");
        f.proposed.store(false, Ordering::SeqCst);

        f.propose(vec![change(SRC_A, SRC_A3)]).expect("a new proposal");

        let all = f.proposals();
        assert_eq!(all.len(), 2, "{subject:?}: the closed proposal and the new one");
        let new = all.iter().find(|p| p.id != first.id).expect("the new proposal");
        assert_eq!(new.hunks.len(), 1);
        assert!(new.hunks[0].diagram);
        let (_, named, update) = f.last_attachment();
        assert_eq!(named, new.id);
        assert_eq!(update, None, "a recording, not a join");
    }
}

// PDM-FR-UITL, PDM-FR-IGRM: a diagram change whose fence is covered by a text
// hunk of the open proposal is the retryable refusal that two changes cover the
// same text, with the place of the diagram change in the list the model sent.
// Nothing is changed.
#[test]
fn a_diagram_change_over_a_text_hunk_is_refused_with_its_place() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let covering = "Intro line.\n\n```mermaid\nflowchart TD\n  A --> B\n```";
        f.offer_text(covering, "Replaced.").expect("the text tool judges no diagram");
        let before = f.world();

        let refusal = f
            .propose(vec![addition(SRC_B, "End line."), change(SRC_A, SRC_A2)])
            .expect_err("refused");

        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt {
                message: crate::tools::propose_draft_changes::PROPOSAL_HUNKS_OVERLAP,
                at: 2,
                of: 2,
            },
            "{subject:?}",
        );
        assert!(refusal.to_string().starts_with("Diagram change 2 of 2: "), "{refusal}");
        assert!(refusal.retryable());
        f.assert_untouched(&before);
    }
}

// PDM-FR-UITL: a diagram that is not found, when the call joins an open
// proposal, names the place of the diagram change and not of an update.
#[test]
fn a_refusal_while_joining_names_the_diagram_change() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.offer_text("Intro line.", "Intro text.").expect("a text proposal stands");
        let before = f.world();

        let refusal = f
            .propose(vec![change(SRC_A, SRC_A2), change("flowchart TD\n  Z --> Q", SRC_A3)])
            .expect_err("refused");

        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 2, of: 2 },
            "{subject:?}",
        );
        assert!(refusal.to_string().starts_with("Diagram change 2 of 2: "), "{refusal}");
        // All or nothing: the first change was not added either.
        f.assert_untouched(&before);
    }
}

// PDM-FR-HOQI, PDM-FR-WLBN: a new source that does not parse is refused before
// anything is joined.
#[test]
fn a_source_that_does_not_parse_joins_nothing() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.offer_text("Intro line.", "Intro text.").expect("a text proposal stands");
        let before = f.world();

        let refusal = f.propose(vec![change(SRC_A, "this is not a diagram")]).unwrap_err();

        assert!(matches!(refusal, ToolRefusal::NotValidMermaid { update: false, .. }));
        f.assert_untouched(&before);
    }
}
