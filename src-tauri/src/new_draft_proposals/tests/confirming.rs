//! Confirming a decomposition (DCP-FR-VLLP, DCP-FR-FAQQ, DCP-FR-ORTA,
//! DCP-FR-QINH, DCP-FR-HMUG, DRS-FR-NAZG).

use super::*;

// DCP-FR-VLLP / DCP-FR-FAQQ / DCP-FR-QINH: the confirmation archives the
// original, names the accepted drafts oldest first with their current names,
// appends one comment, and announces the marker before its own event.
#[test]
fn a_confirmation_archives_the_original_and_names_the_accepted_drafts() {
    let f = Fixture::new();
    let one = f.record("relay-auth", "a", 1, 3).unwrap();
    let first = f.decide(&one.id, NewDraftDecision::Accept, None).unwrap().created_draft.unwrap();
    let two = f.record("relay-mid", "b", 2, 3).unwrap();
    f.decide(&two.id, NewDraftDecision::Reject, None).unwrap();
    let three = f.record("relay-sync", "c", 3, 3).unwrap();
    let third = f.decide(&three.id, NewDraftDecision::Accept, None).unwrap().created_draft.unwrap();
    drafts::rename_draft_impl(&f.root, &first.id, "relay-auth-v2").unwrap();
    drafts::delete_draft_impl(&f.root, &f.root, &third.id).unwrap();
    let prompt_before = drafts::read_draft_prompt(&f.root, &f.draft_id).unwrap().content;
    f.clear_events();

    let record = f.confirm().unwrap();
    assert_eq!(record.status, drafts::DraftStatus::Archived);
    let marker = record.superseded.unwrap();
    assert_eq!(marker.set_id, one.set_id);
    assert_eq!(
        marker.drafts,
        vec![
            drafts::SupersedingDraft { id: first.id, name: "relay-auth-v2".into() },
            drafts::SupersedingDraft { id: third.id, name: "relay-sync".into() },
        ]
    );
    // DCP-FR-FAQQ: the prompt is untouched and the draft is kept.
    assert_eq!(drafts::read_draft_prompt(&f.root, &f.draft_id).unwrap().content, prompt_before);
    let last = f.thread().comments.last().unwrap().clone();
    assert!(last.body.starts_with("Decomposition confirmed."));
    // DCP-FR-HMUG: a confirmation from the command appends no instruction line.
    assert!(!last.body.contains("Decided by"));
    assert!(!last.body.contains("\n\n"));
    assert!(matches!(last.author, Participant::Human { .. }));
    assert_eq!(f.causes(), vec!["drafts", "confirmed"]);
}

// DCP-FR-VLLP: a pending proposal, and no accepted one, each refuse.
#[test]
fn a_confirmation_needs_no_pending_and_one_accepted_proposal() {
    let f = Fixture::new();
    assert_eq!(f.confirm().unwrap_err(), ERR_NOTHING_ACCEPTED);
    let p = f.record("a", "body", 1, 2).unwrap();
    assert_eq!(f.confirm().unwrap_err(), ERR_PROPOSAL_PENDING);
    f.decide(&p.id, NewDraftDecision::Reject, None).unwrap();
    f.record("b", "body", 2, 2).unwrap();
    f.update(NewDraftUpdate::Withdraw).unwrap();
    assert_eq!(f.confirm().unwrap_err(), ERR_NOTHING_ACCEPTED);
    let status = drafts::read_draft_record(&f.root, &f.draft_id).unwrap().status;
    assert_eq!(status, drafts::DraftStatus::Active);
}

// DCP-FR-ORTA / DRS-FR-NAZG: after a confirmation the decomposition is closed,
// also when the original is restored.
#[test]
fn a_confirmed_decomposition_stays_closed_after_a_restore() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 1).unwrap();
    f.decide(&p.id, NewDraftDecision::Accept, None).unwrap();
    f.confirm().unwrap();
    assert_eq!(f.confirm().unwrap_err(), ERR_DECOMPOSITION_CONFIRMED);
    let restored =
        drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Active).unwrap();
    assert!(restored.superseded.is_some());
    assert_eq!(f.record("b", "body", 1, 1).unwrap_err(), ERR_DECOMPOSITION_CONFIRMED);
    assert_eq!(f.confirm().unwrap_err(), ERR_DECOMPOSITION_CONFIRMED);
}

// DCP-FR-ORTA: a pending proposal of a superseded draft can not be accepted.
#[test]
fn a_pending_proposal_of_a_superseded_draft_is_not_decided() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 1).unwrap();
    drafts::mark_draft_superseded(
        &f.root,
        &f.draft_id,
        drafts::SupersededMarker {
            set_id: p.set_id.clone(),
            confirmed_at: "t".into(),
            drafts: vec![],
        },
    )
    .unwrap();
    drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Active).unwrap();
    let count = drafts::list_drafts_impl(&f.root).drafts.len();
    assert_eq!(
        f.decide(&p.id, NewDraftDecision::Accept, None).unwrap_err(),
        ERR_DECOMPOSITION_CONFIRMED
    );
    assert_eq!(drafts::list_drafts_impl(&f.root).drafts.len(), count);
}

// DCP-FR-LMJW: no new-draft operation contributes a statistics line, and no
// timer decides a proposal. Checked over the module's own sources.
#[test]
fn no_new_draft_operation_writes_a_statistics_line_or_holds_a_timer() {
    let sources = [
        include_str!("../../new_draft_proposals.rs"),
        include_str!("../recording.rs"),
        include_str!("../deciding.rs"),
        include_str!("../confirming.rs"),
        include_str!("../commands.rs"),
    ];
    for source in sources {
        assert!(!source.contains("statistics::"), "a statistics call");
        assert!(!source.contains("tokio::time") && !source.contains("thread::sleep"), "a timer");
    }
}
