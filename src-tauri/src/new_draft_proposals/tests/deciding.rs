//! Accepting and rejecting a new-draft proposal (DCP-FR-KOOA, DCP-FR-AQDM,
//! DCP-FR-FZEP, DCP-FR-PDCP, DCP-FR-EEDK, DCP-FR-GJXE, DCP-FR-KUSI).

use super::*;

// DCP-FR-KOOA / DCP-FR-PDCP / DCP-FR-ZDSP / DRS-FR-EMKG: an acceptance creates
// the draft in the original's folder with the body and the sequence, records the
// decision, appends the author's comment, and announces the creation first.
#[test]
fn an_acceptance_creates_the_draft_in_the_original_folder() {
    let f = Fixture::new();
    let p = f.record("relay-auth", "The auth part.\n", 1, 3).unwrap();
    f.clear_events();
    let outcome = f.decide(&p.id, NewDraftDecision::Accept, Some("Good split.")).unwrap();
    assert_eq!(outcome.proposal.state, NewDraftState::Accepted);
    let created = outcome.created_draft.clone().unwrap();
    assert_eq!(outcome.proposal.created_draft_id.as_deref(), Some(created.id.as_str()));
    assert_eq!(created.name, "relay-auth");
    assert_eq!(drafts::draft_folder_of(&f.root, &created.id).unwrap(), "UI/Sub");
    assert_eq!(
        created.sequence,
        Some(drafts::DraftSequence {
            set_id: p.set_id.clone(),
            position: 1,
            total: 3,
            original_draft_id: f.draft_id.clone(),
            original_name: "relay".into(),
        })
    );
    let prompt = drafts::read_draft_prompt(&f.root, &created.id).unwrap().content;
    assert_eq!(prompt, "The auth part.\n");
    assert_eq!(outcome.origin_kind, "draft_discussion");
    let last = f.thread().comments.last().unwrap().clone();
    assert_eq!(Some(last.id.clone()), outcome.comment_id);
    assert!(matches!(last.author, Participant::Human { .. }));
    assert!(last.body.starts_with("Accepted the proposed draft “relay-auth”, part 1 of 3."));
    assert!(last.body.ends_with("Good split."));
    let causes = f.causes();
    let drafts_at = causes.iter().position(|c| c == "drafts").unwrap();
    let decided_at = causes.iter().position(|c| c == "decided").unwrap();
    assert!(drafts_at < decided_at, "{causes:?}");
    // The original draft does not change.
    assert_eq!(
        drafts::read_draft_record(&f.root, &f.draft_id).unwrap().status,
        drafts::DraftStatus::Active
    );
}

// DCP-FR-FZEP / DCP-FR-PDCP: a rejection creates nothing and keeps the record
// and the body.
#[test]
fn a_rejection_creates_nothing() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    let count = drafts::list_drafts_impl(&f.root).drafts.len();
    let outcome = f.decide(&p.id, NewDraftDecision::Reject, None).unwrap();
    assert_eq!(outcome.proposal.state, NewDraftState::Rejected);
    assert!(outcome.created_draft.is_none());
    assert_eq!(drafts::list_drafts_impl(&f.root).drafts.len(), count);
    assert_eq!(load_body_impl(&f.root, &p.id).unwrap(), "body");
    let last = f.thread().comments.last().unwrap().clone();
    assert!(last.body.starts_with("Rejected the proposed draft “a”, part 1 of 2."));
}

// DCP-FR-EEDK: a proposal is decided once, and an unknown id names nothing.
#[test]
fn a_decided_proposal_and_an_unknown_id_are_refused() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    f.decide(&p.id, NewDraftDecision::Reject, None).unwrap();
    let before = f.tree();
    f.clear_events();
    assert_eq!(f.decide(&p.id, NewDraftDecision::Accept, None).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(
        f.decide("nope", NewDraftDecision::Accept, None).unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND
    );
    assert_eq!(
        f.decide("../x", NewDraftDecision::Reject, None).unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND
    );
    assert_eq!(f.tree(), before);
    assert!(f.causes().is_empty());
}

// DCP-FR-LTEZ: an archived original refuses a decision and creates nothing.
#[test]
fn an_archived_original_refuses_a_decision() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Archived).unwrap();
    let count = drafts::list_drafts_impl(&f.root).drafts.len();
    assert_eq!(f.decide(&p.id, NewDraftDecision::Accept, None).unwrap_err(), ERR_DRAFT_ARCHIVED);
    assert_eq!(f.decide(&p.id, NewDraftDecision::Reject, None).unwrap_err(), ERR_DRAFT_ARCHIVED);
    assert_eq!(drafts::list_drafts_impl(&f.root).drafts.len(), count);
}

// DCP-FR-GJXE: a later total and a rejection leave a created draft's sequence
// as it is.
#[test]
fn a_created_drafts_sequence_is_never_rewritten() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 3).unwrap();
    let created = f.decide(&p.id, NewDraftDecision::Accept, None).unwrap().created_draft.unwrap();
    let q = f.record("b", "body", 2, 5).unwrap();
    f.decide(&q.id, NewDraftDecision::Reject, None).unwrap();
    let sequence = drafts::read_draft_record(&f.root, &created.id).unwrap().sequence.unwrap();
    assert_eq!((sequence.position, sequence.total), (1, 3));
}

// DCP-FR-KUSI: two acceptances of one proposal create one draft.
#[test]
fn two_concurrent_acceptances_create_one_draft() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 1).unwrap();
    let count = drafts::list_drafts_impl(&f.root).drafts.len();
    let barrier = std::sync::Barrier::new(2);
    let root = &f.root;
    let id = p.id.as_str();
    let results: Vec<Result<NewDraftDecisionOutcome, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let app = f.handle();
                let barrier = &barrier;
                s.spawn(move || {
                    barrier.wait();
                    decide_new_draft_in_process(
                        &app,
                        root,
                        root,
                        id,
                        NewDraftDecision::Accept,
                        None,
                        &human(),
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(drafts::list_drafts_impl(&f.root).drafts.len(), count + 1);
}

// DCP-FR-EJWZ / DCP-FR-PDCP: a decision whose thread is gone still lands, with
// no decision comment.
#[test]
fn a_decision_whose_thread_is_gone_lands_without_a_comment() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 1).unwrap();
    let dir = f.new_drafts_dir();
    let mut record: NewDraftProposal =
        f.root.read_toml(dir.join(format!("{}.toml", p.id))).unwrap();
    record.thread_id = "gone".into();
    f.root.write_toml_atomic(dir.join(format!("{}.toml", p.id)), &record).unwrap();
    let outcome = f.decide(&p.id, NewDraftDecision::Reject, None).unwrap();
    assert_eq!(outcome.proposal.state, NewDraftState::Rejected);
    assert!(outcome.comment_id.is_none());
}

fn set_readonly(path: &std::path::Path, readonly: bool) {
    let mut perms = std::fs::metadata(path).expect("meta").permissions();
    // Reason: the test restores write permission on its own temp directory
    // so that the directory can be deleted; no production code does this.
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(readonly);
    std::fs::set_permissions(path, perms).expect("chmod");
}

// DCP-FR-KOOA: the draft is created in the folder that holds the original when
// it is accepted, not when it was proposed.
#[test]
fn an_acceptance_files_the_draft_where_the_original_is_now() {
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    drafts::move_draft_to_folder_impl(&f.root, &f.draft_id, "UI").unwrap();
    let created = f.decide(&p.id, NewDraftDecision::Accept, None).unwrap().created_draft.unwrap();
    assert_eq!(drafts::draft_folder_of(&f.root, &created.id).unwrap(), "UI");
}

// DCP-FR-AQDM: a draft that can not be created changes nothing, and a record
// that can not be written takes the created draft back.
#[test]
fn a_failed_acceptance_leaves_nothing_behind() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_acceptance_leaves_nothing_behind",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    let p = f.record("a", "body", 1, 2).unwrap();
    let count = drafts::list_drafts_impl(&f.root).drafts.len();
    let folder = f.root.path().join(".synthesis/drafts/UI/Sub");
    set_readonly(&folder, true);
    let refused = f.decide(&p.id, NewDraftDecision::Accept, None);
    set_readonly(&folder, false);
    assert_eq!(refused.unwrap_err(), ERR_WRITE_FAILED);
    assert_eq!(
        f.decide(&p.id, NewDraftDecision::Reject, None).map(|o| o.proposal.state),
        Ok(NewDraftState::Rejected)
    );
    assert_eq!(drafts::list_drafts_impl(&f.root).drafts.len(), count);

    let g = Fixture::new();
    let q = g.record("b", "body", 1, 2).unwrap();
    let count = drafts::list_drafts_impl(&g.root).drafts.len();
    let dir = g.new_drafts_dir();
    set_readonly(&dir, true);
    let refused = g.decide(&q.id, NewDraftDecision::Accept, None);
    set_readonly(&dir, false);
    assert_eq!(refused.unwrap_err(), ERR_WRITE_FAILED);
    assert_eq!(drafts::list_drafts_impl(&g.root).drafts.len(), count, "the created draft is gone");
    assert_eq!(list_impl(&g.root, &g.draft_id).unwrap()[0].state, NewDraftState::Pending);
}

// DCP-FR-GDTC / DCP-FR-ZJRW: an update whose comment can not be appended
// restores both files to the bytes they held.
#[test]
fn an_update_whose_comment_fails_restores_both_files() {
    let f = Fixture::new();
    f.record("a", "body", 1, 2).unwrap();
    comments::set_resolution_to(
        &f.root,
        ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let before = f.tree();
    let change = NewDraftUpdate::Change {
        name: Some("b".into()),
        body: Some("other body".into()),
        position: None,
        total: None,
    };
    assert_eq!(f.update(change), Err(ERR_DISCUSSION_RESOLVED.into()));
    let after: Vec<_> =
        f.tree().into_iter().filter(|(p, _)| p.starts_with(f.new_drafts_dir())).collect();
    let was: Vec<_> =
        before.into_iter().filter(|(p, _)| p.starts_with(f.new_drafts_dir())).collect();
    assert_eq!(after, was);
}
