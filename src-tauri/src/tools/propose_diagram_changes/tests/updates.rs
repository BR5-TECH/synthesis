//! A call with `updates` changes, adds, and withdraws the diagram hunks of the
//! open proposal, and acts on diagram hunks alone (PDM-FR-BWOH, PDM-FR-NQEK,
//! PDM-FR-SDJP, PDM-FR-HOQI, PDM-FR-CEZB, PDM-FR-UITL, PDM-FR-IGRM).

use super::*;
use crate::tools::proposal_updates::{
    UPDATE_CHANGE_IS_DIAGRAM, UPDATE_CHANGE_SETTLED, UPDATE_MALFORMED, UPDATE_NAMED_TWICE,
    UPDATE_NO_SUCH_CHANGE,
};

/// A fixture whose open proposal holds a text hunk and a diagram hunk, and the
/// id of each. The diagram hunk joined the proposal of the text hunk.
fn with_text_and_diagram(subject: Subject) -> (Fixture, String, String) {
    let f = Fixture::new(subject);
    f.offer_text("Intro line.", "Intro text.").expect("a text proposal stands");
    f.propose(vec![change(SRC_A, SRC_A2)]).expect("the diagram joins");
    f.proposed.store(false, Ordering::SeqCst);
    let standing = f.one();
    assert!(!standing.hunks[0].diagram && standing.hunks[1].diagram);
    (f, standing.hunks[0].id.clone(), standing.hunks[1].id.clone())
}

fn standing_state(f: &Fixture, id: &str) -> String {
    f.one().rows.iter().find(|row| row.0 == id).expect("a row").1.clone()
}

// PDM-FR-BWOH: a `change` of a diagram hunk keeps its id and its place, bumps
// its revision, and sets it undecided again. One comment carries the counts and
// the turn ends.
#[test]
fn a_change_keeps_the_id_and_sets_the_hunk_undecided() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let proposal = f.one().id;
        f.hold(&proposal, &diagram_id);
        assert_eq!(standing_state(&f, &diagram_id), "Discussing");
        let comments = f.comment_count();

        let output = f
            .call(f.updating(vec![entry(
                "change",
                Some(&diagram_id),
                Some(SRC_A),
                Some(SRC_A3),
                None,
            )]))
            .expect("updated");

        assert!(output.proposed);
        assert!(f.proposed.load(Ordering::SeqCst), "an update ends the turn (CVL-FR-15)");
        let standing = f.one();
        assert_eq!(standing.id, proposal, "no new proposal, {subject:?}");
        assert_eq!(standing.hunks.len(), 2);
        let hunk = &standing.hunks[1];
        assert_eq!(hunk.id, diagram_id, "a changed hunk keeps its id and position");
        assert!(hunk.diagram);
        assert_eq!(hunk.revision, 1);
        assert_eq!(hunk.readable_after(), SRC_A3);
        assert_eq!(standing_state(&f, &diagram_id), "Pending", "the author decides it again");
        assert_eq!(f.body(), BODY, "an update writes no file");

        assert_eq!(f.comment_count(), comments + 1, "exactly one comment");
        let (kind, named, update) = f.last_attachment();
        assert_eq!(kind, f.attachment_kind());
        assert_eq!(named, proposal);
        assert_eq!(update, Some(ProposalUpdateCounts { changed: 1, added: 0, withdrawn: 0 }));
        assert_eq!(f.discussion().comments.last().expect("comment").body, UPDATE_RATIONALE);
    }
}

// PDM-FR-BWOH: a `withdraw` takes back a diagram hunk, which the record keeps
// as withdrawn. The text hunk is not touched.
#[test]
fn a_withdraw_takes_back_a_diagram_hunk() {
    for subject in SUBJECTS {
        let (f, text_id, diagram_id) = with_text_and_diagram(subject);
        let before = f.one();

        f.call(f.updating(vec![entry("withdraw", Some(&diagram_id), None, None, None)]))
            .expect("updated");

        let standing = f.one();
        assert_eq!(standing.hunks, before.hunks, "the record keeps the hunk, {subject:?}");
        assert_eq!(standing_state(&f, &diagram_id), "Withdrawn");
        assert_eq!(standing_state(&f, &text_id), "Pending");
        let (_, _, update) = f.last_attachment();
        assert_eq!(update, Some(ProposalUpdateCounts { changed: 0, added: 0, withdrawn: 1 }));
    }
}

// PDM-FR-BWOH, PDM-FR-NQEK: an `add` records a diagram hunk that goes last, and
// one call changes, adds, and withdraws with one comment.
#[test]
fn an_add_records_a_diagram_hunk_and_one_call_may_do_all_three() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let comments = f.comment_count();

        f.call(f.updating(vec![
            entry("add", None, None, Some(SRC_B), Some("End line.")),
            entry("change", Some(&diagram_id), Some(SRC_A), Some(SRC_A3), None),
        ]))
        .expect("updated");

        let standing = f.one();
        assert_eq!(standing.hunks.len(), 3);
        assert!(standing.hunks[2].diagram, "an added hunk is a diagram hunk, {subject:?}");
        assert_eq!(standing.hunks[2].kind, crate::draft_proposals::anchors::HunkKind::Add);
        assert_eq!(f.comment_count(), comments + 1);
        let (_, _, update) = f.last_attachment();
        assert_eq!(update, Some(ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 0 }));

        // The hunk that the call added can be withdrawn in its turn.
        let withdraw_added = standing.hunks[2].id.clone();
        f.call(f.updating(vec![entry("withdraw", Some(&withdraw_added), None, None, None)]))
            .expect("updated");
        assert_eq!(standing_state(&f, &withdraw_added), "Withdrawn");
    }
}

// PDM-FR-NQEK: a `change` or a `withdraw` that names a text hunk is the refusal
// that it is a text change, with the place of the update, and nothing is
// changed.
#[test]
fn an_update_of_a_text_hunk_is_refused() {
    for subject in SUBJECTS {
        let (f, text_id, _) = with_text_and_diagram(subject);
        let before = f.world();
        for update in [
            entry("change", Some(&text_id), Some(SRC_A), Some(SRC_A3), None),
            entry("withdraw", Some(&text_id), None, None, None),
        ] {
            let refusal = f.call(f.updating(vec![update])).expect_err("refused");
            assert_eq!(
                refusal,
                ToolRefusal::InvalidUpdateAt { message: CHANGE_IS_TEXT, at: 1, of: 1 },
                "{subject:?}",
            );
            assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
            assert!(refusal.retryable());
            assert!(refusal.to_string().contains("is a text change"), "{refusal}");
            f.assert_untouched(&before);
        }
    }
}

// PDM-FR-NQEK: the update of a text tool refuses a diagram hunk in the same way,
// and names this tool.
#[test]
fn the_text_tool_refuses_to_update_a_diagram_hunk() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let before = f.world();
        for update in [
            serde_json::json!({ "action": "change", "id": diagram_id, "before": "x", "after": "y" }),
            serde_json::json!({ "action": "withdraw", "id": diagram_id }),
        ] {
            let refusal = f.text_updates(serde_json::json!([update])).expect_err("refused");
            assert_eq!(
                refusal,
                ToolRefusal::InvalidUpdateAt { message: UPDATE_CHANGE_IS_DIAGRAM, at: 1, of: 1 },
                "{subject:?}",
            );
            assert!(refusal.to_string().contains("diagram change"), "{refusal}");
            assert!(refusal.to_string().contains(NAME), "{refusal}");
            assert_eq!(f.world(), before, "the text tool wrote nothing");
        }
    }
}

// PDM-FR-IGRM: a text hunk added to a proposal that holds a diagram hunk is
// refused where it overlaps the fence of that hunk.
#[test]
fn a_text_hunk_over_the_fence_of_a_diagram_hunk_overlaps() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let before = f.world();
        let covering = "Intro line.\n\n```mermaid\nflowchart TD\n  A --> B\n```";
        let refusal = f
            .text_updates(serde_json::json!([
                { "action": "add", "before": covering, "after": "Replaced." }
            ]))
            .expect_err("refused");
        assert_eq!(
            refusal,
            ToolRefusal::InvalidUpdateAt { message: PROPOSAL_HUNKS_OVERLAP, at: 1, of: 1 },
            "{subject:?}",
        );
        assert_eq!(f.world(), before);

        // A text hunk beside the fence is fine.
        f.text_updates(serde_json::json!([
            { "action": "add", "before": "Intro line.", "after": "Intro text." }
        ]))
        .expect("a text hunk beside the fence");
        assert_eq!(f.one().hunks.len(), 2);
    }
}

// PDM-FR-BWOH (as PDC-FR-UOBH, PDC-FR-DBFT): the entry refusals name the place
// of the update, and a refused update leaves the proposal as it was.
#[test]
fn entry_refusals_name_the_place_of_the_update() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let ok = || entry("change", Some(&diagram_id), Some(SRC_A), Some(SRC_A3), None);
        let cases = [
            (vec![ok(), entry("withdraw", None, None, None, None)], UPDATE_MALFORMED, 2, 2),
            (
                vec![ok(), entry("add", Some("x"), None, Some(SRC_B), Some("End line."))],
                UPDATE_MALFORMED,
                2,
                2,
            ),
            (
                vec![ok(), entry("withdraw", Some("nope"), None, None, None)],
                UPDATE_NO_SUCH_CHANGE,
                2,
                2,
            ),
            (
                vec![ok(), entry("withdraw", Some(&diagram_id), None, None, None)],
                UPDATE_NAMED_TWICE,
                2,
                2,
            ),
            (
                vec![ok(), entry("add", None, None, Some(SRC_B), Some("Not in the file."))],
                DIAGRAM_NOT_FOUND,
                2,
                2,
            ),
            (
                vec![entry("change", Some(&diagram_id), Some(SRC_A), Some(SRC_A), None)],
                PROPOSAL_NO_CHANGE,
                1,
                1,
            ),
            (
                vec![entry(
                    "change",
                    Some(&diagram_id),
                    Some("flowchart TD\n  Z --> Q"),
                    Some(SRC_A3),
                    None,
                )],
                DIAGRAM_NOT_FOUND,
                1,
                1,
            ),
            (
                vec![entry(
                    "change",
                    Some(&diagram_id),
                    Some(SRC_A),
                    Some("flowchart TD\n```"),
                    None,
                )],
                FENCE_IN_SOURCE,
                1,
                1,
            ),
        ];
        let before = f.world();
        for (updates, message, at, of) in cases {
            let refusal = f.call(f.updating(updates)).expect_err("refused");
            assert_eq!(refusal, ToolRefusal::InvalidUpdateAt { message, at, of }, "{subject:?}");
            assert!(refusal.to_string().starts_with(&format!("Update {at} of {of}: ")));
            assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
            assert!(refusal.retryable());
            // All or nothing.
            f.assert_untouched(&before);
        }
    }
}

// PDM-FR-HOQI, PDM-FR-BWOH: a new source of an update that does not parse is
// refused with the parse error and the place of the update, and the first
// update is not applied either.
#[test]
fn an_update_with_a_source_that_does_not_parse_is_refused() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let before = f.world();
        let refusal = f
            .call(f.updating(vec![
                entry("add", None, None, Some(SRC_B), Some("End line.")),
                entry("change", Some(&diagram_id), Some(SRC_A), Some("not a diagram"), None),
            ]))
            .expect_err("refused");
        let ToolRefusal::NotValidMermaid { error, at, of, update } = &refusal else {
            panic!("expected a parse refusal, got {refusal:?}");
        };
        assert_eq!((*at, *of, *update), (2, 2, true), "{subject:?}");
        assert!(!error.is_empty());
        assert!(refusal.to_string().starts_with("Update 2 of 2: "), "{refusal}");
        assert!(refusal.to_string().contains("not valid Mermaid"));
        assert!(refusal.retryable());
        f.assert_untouched(&before);
    }
}

// PDM-FR-BWOH (as PDC-FR-DBFT): a change that is withdrawn can not be updated
// again.
#[test]
fn a_withdrawn_change_can_not_be_updated_again() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        f.call(f.updating(vec![entry("withdraw", Some(&diagram_id), None, None, None)]))
            .expect("withdrawn");
        f.proposed.store(false, Ordering::SeqCst);
        let before = f.world();

        let refusal = f
            .call(f.updating(vec![entry(
                "change",
                Some(&diagram_id),
                Some(SRC_A),
                Some(SRC_A3),
                None,
            )]))
            .expect_err("refused");

        assert_eq!(
            refusal,
            ToolRefusal::InvalidUpdateAt { message: UPDATE_CHANGE_SETTLED, at: 1, of: 1 },
            "{subject:?}",
        );
        f.assert_untouched(&before);
    }
}

// PDM-FR-XRVZ: an update appends a comment, so its rationale needs a spoken
// part and is not blank.
#[test]
fn an_update_needs_a_rationale_with_a_spoken_part() {
    for subject in SUBJECTS {
        let (f, _, diagram_id) = with_text_and_diagram(subject);
        let before = f.world();
        for (rationale, message) in [
            ("No tag here.", RATIONALE_NO_SPOKEN_PART),
            (
                " ",
                match subject {
                    Subject::Draft => PROPOSAL_RATIONALE_BLANK,
                    Subject::Artifact => PROMPT_PROPOSAL_RATIONALE_BLANK,
                },
            ),
        ] {
            let mut call = f.updating(vec![entry("withdraw", Some(&diagram_id), None, None, None)]);
            call.rationale = rationale.into();
            let refusal = f.call(call).expect_err("refused");
            assert_eq!(refusal, ToolRefusal::InvalidArguments(message), "{subject:?}");
            f.assert_untouched(&before);
        }
    }
}

// PDM-FR-LJGN, PDM-FR-BWOH: an update names the path of the open proposal, and
// any other path is refused with nothing written.
#[test]
fn an_update_with_another_path_is_refused() {
    let (f, _, diagram_id) = with_text_and_diagram(Subject::Draft);
    let before = f.world();
    let mut call = f.updating(vec![entry("withdraw", Some(&diagram_id), None, None, None)]);
    call.path = "other.md".into();
    assert_eq!(f.call(call).expect_err("refused"), ToolRefusal::ProposalPathMissing);
    f.assert_untouched(&before);

    // An update of an artifact names the file it updates. A file that holds no
    // open proposal has nothing to update, as for the text tool.
    let (f, _, diagram_id) = with_text_and_diagram(Subject::Artifact);
    let before = f.world();
    let mut call = f.updating(vec![entry("withdraw", Some(&diagram_id), None, None, None)]);
    call.path = "prompts/missing.md".into();
    assert_eq!(f.call(call).expect_err("refused"), ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL));
    f.assert_untouched(&before);
}

// PDM-FR-BWOH, PDM-FR-FZGT: an update that withdraws every undecided hunk closes
// the proposal, and the file holds no open proposal after it.
#[test]
fn an_update_that_withdraws_everything_closes_the_proposal() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        let id = f.one().hunks[0].id.clone();
        f.call(f.updating(vec![entry("withdraw", Some(&id), None, None, None)]))
            .expect("withdrawn");
        // The file holds no open proposal, so `updates` says so.
        let refusal =
            f.call(f.updating(vec![entry("withdraw", Some(&id), None, None, None)])).unwrap_err();
        assert_eq!(refusal, ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL), "{subject:?}");
    }
}
