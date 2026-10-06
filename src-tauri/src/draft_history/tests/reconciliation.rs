//! Tests for the reconciliation of an interrupted acceptance and the event it announces.

use super::*;

// ---------------------------------------------------------------------------
// DHS-FR-07, DHS-FR-18, DHS-FR-19, DHS-FR-21, DHS-FR-20, CMS-FR-06, DHS-FR-12 — reconciliation
// (DHS-FR-12, FR-18, FR-19, FR-20, FR-21)
// ---------------------------------------------------------------------------

#[test]
fn an_uncommitted_acceptance_is_rolled_back_by_the_next_reconciliation() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    let entry_id = journal.entry_id.clone().expect("an entry id");

    // DHS-FR-12: the entry is on disk but not exposed, and is not loadable.
    assert!(manifest_path(&f.hist(), &entry_id).is_file());
    assert!(
        !f.list().entries.iter().any(|e| e.id == entry_id),
        "an uncommitted entry is not a version of the prompt",
    );
    assert_eq!(load_impl(&f.root, &entry_id).unwrap_err(), ERR_ENTRY_NOT_FOUND);

    let seen = f.watch();
    assert_eq!(f.reconcile().expect("reconciled"), Reconciliation::RolledBack);

    assert_eq!(f.prompt(), ORIGINAL_TEXT, "the prior bytes are back");
    assert!(!manifest_path(&f.hist(), &entry_id).exists());
    assert!(!snapshot_path(&f.hist(), &entry_id).exists());
    assert_eq!(
        f.history_files(),
        Vec::<String>::new(),
        "the Original the rolled-back acceptance would have settled went with it",
    );
    assert!(f.list().entries.is_empty());
    assert_eq!(
        dcp::find_proposal(&f.root, &proposal.id).expect("there").1.state,
        dcp::ProposalState::Pending,
    );
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "the conversation still holds the proposal's comment and nothing since",
    );
    assert!(!f.history_files().contains(&JOURNAL_FILE.to_string()));
    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(events.len(), 1);
    assert!(events[0]["entry"].is_null(), "a removal carries a null entry");

    // Reconciling again does what reconciling once did.
    f.reconcile().expect("idempotent");
    assert!(f.list().entries.is_empty());

    // And accepting again lands whole, with exactly one entry for it beside the
    // Original it settles this time.
    let outcome = f.accept(&proposal).expect("accepted");
    assert!(outcome.entry.is_some());
    let entries = f.list().entries;
    // The shape rather than the count: a duplicated `Original` or a reused seq
    // would leave a two-entry list that says something false about both.
    assert_eq!(
        entries.iter().map(|e| (e.seq, e.source.clone())).collect::<Vec<_>>(),
        vec![
            (1, DraftHistorySource::Original),
            (
                2,
                DraftHistorySource::ProposalAccepted {
                    hunk_id: None,
                    proposal_id: proposal.id.clone(),
                    agent: agent(),
                },
            ),
        ],
    );
}

#[test]
fn a_committed_acceptance_is_rolled_forward_and_its_comment_folds_once() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Committed);
    let before = f.discussion().comments.len();

    assert_eq!(
        f.reconcile().expect("rolled forward"),
        Reconciliation::RolledForward {
            proposal_id: proposal.id.clone(),
            comment_id: journal.comment_id.clone(),
        },
    );

    let after = f.discussion();
    assert_eq!(after.comments.len(), before + 1, "the comment was appended once");
    assert!(after.comments.last().expect("last").body.contains("Accepted"));
    assert_eq!(f.prompt(), PROPOSED, "the prompt holds the accepted bytes");
    assert_eq!(f.list().entries.len(), 2, "the entry is exposed");
    assert_eq!(
        dcp::find_proposal(&f.root, &proposal.id).expect("there").1.state,
        dcp::ProposalState::Accepted,
    );
    assert!(!f.history_files().contains(&JOURNAL_FILE.to_string()));

    // DHS-FR-21: an interruption *after* the append and before the journal was
    // removed re-appends under the same `event_id`, and the fold applies it once.
    f.root
        .write_toml_atomic(f.hist().join(JOURNAL_FILE), &journal)
        .expect("journal back");
    std::fs::write(f.hist().join(JOURNAL_PRIOR), ORIGINAL_TEXT).expect("prior");
    f.reconcile().expect("reconciled again");
    assert_eq!(
        f.discussion().comments.len(),
        before + 1,
        "no second line was contributed",
    );
}

#[test]
fn a_complete_journal_is_simply_removed() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    interrupt(&f, &proposal, Phase::Complete);
    let before = f.discussion().comments.len();

    assert_eq!(f.reconcile().expect("reconciled"), Reconciliation::Nothing);

    assert!(!f.history_files().contains(&JOURNAL_FILE.to_string()));
    assert!(!f.history_files().contains(&JOURNAL_PRIOR.to_string()));
    assert_eq!(f.discussion().comments.len(), before, "nothing was appended");
    assert_eq!(f.list().entries.len(), 2);
}

// ---------------------------------------------------------------------------
// DHS-FR-21, DHS-FR-23 — recovery that cannot complete (DHS-FR-21, DHS-FR-23)
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_prior_payload_is_a_typed_recovery_failure_that_leaves_the_journal() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    interrupt(&f, &proposal, Phase::Prepared);
    std::fs::remove_file(f.hist().join(JOURNAL_PRIOR)).expect("make it unreadable");

    for refusal in [f.reconcile().err(), list_impl_after_reconcile(&f).err()] {
        assert_eq!(refusal.as_deref(), Some(ERR_HISTORY_RECOVERY_FAILED));
    }
    assert!(
        f.history_files().contains(&JOURNAL_FILE.to_string()),
        "the journal stands so the next run tries again",
    );

    // With the file readable again the reconciliation completes and the command
    // answers normally.
    std::fs::write(f.hist().join(JOURNAL_PRIOR), ORIGINAL_TEXT).expect("restore");
    f.reconcile().expect("reconciled");
    assert_eq!(f.prompt(), ORIGINAL_TEXT);
    assert!(f.list().entries.is_empty());
}

#[test]
fn a_rollback_that_cannot_remove_an_entry_leaves_the_journal_and_exposes_nothing() {
    // DHS-FR-19 / DHS-FR-21: a reconciliation that cannot complete returns
    // `history_recovery_failed` and leaves the journal standing so the next run
    // tries again.
    //
    // The removal is the step that matters here rather than the restoring
    // write: a manifest that survived a rollback whose journal went on to be
    // removed is an **exposed** version (DHS-FR-12) of a change that was undone
    // — the rail would offer it, and loading it would answer.
    use std::os::unix::fs::PermissionsExt;
    // A read-only `history/` is inert under root, where the removal simply
    // succeeds and no refusal is produced to assert on.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_rollback_that_cannot_remove_an_entry_leaves_the_journal_and_exposes_nothing",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    let entry_id = journal.entry_id.clone().expect("an entry id");
    let original_id = journal.original_entry_id.clone().expect("an Original");
    let hist = f.hist();
    // The prompt is restored from `journal.prior` first, so `files/` stays
    // writable and only the removal is made to fail.
    let mode = std::fs::metadata(&hist).expect("metadata").permissions();
    std::fs::set_permissions(&hist, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");

    let refused = f.reconcile().unwrap_err();

    assert_eq!(refused, ERR_HISTORY_RECOVERY_FAILED);
    assert!(
        hist.join(JOURNAL_FILE).is_file(),
        "the journal stands so the next run tries again",
    );
    // Both entries are still on disk — and still hidden by that journal, so
    // nothing was ever offered as a version.
    assert!(manifest_path(&hist, &entry_id).is_file());
    assert!(manifest_path(&hist, &original_id).is_file());
    assert_eq!(
        list_impl_after_reconcile(&f).unwrap_err(),
        ERR_HISTORY_RECOVERY_FAILED,
    );
    assert_eq!(load_impl(&f.root, &entry_id).unwrap_err(), ERR_ENTRY_NOT_FOUND);

    // And with the folder writable again the reconciliation completes.
    std::fs::set_permissions(&hist, mode).expect("writable again");
    assert_eq!(f.reconcile().expect("reconciled"), Reconciliation::RolledBack);
    assert_eq!(f.history_files(), Vec::<String>::new());
    assert!(f.list().entries.is_empty());
    assert_eq!(f.prompt(), ORIGINAL_TEXT);
}

#[test]
fn a_rollback_caught_between_the_two_entries_removes_the_one_that_landed() {
    // DHS-FR-19: the removals tolerate a file that never reached disk, which is
    // what a crash between the `Original` and the version it supersedes leaves —
    // and the `Original` must not survive it, no version having superseded it.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    let entry_id = journal.entry_id.clone().expect("an entry id");
    let original_id = journal.original_entry_id.clone().expect("an Original");
    // Rewind to the instant after the `Original` landed and before the version's
    // own payload was written.
    for name in [
        format!("{entry_id}.{SNAPSHOT_EXT}"),
        format!("{entry_id}.{MANIFEST_EXT}"),
    ] {
        std::fs::remove_file(f.hist().join(name)).expect("rewind");
    }
    assert!(manifest_path(&f.hist(), &original_id).is_file());

    assert_eq!(f.reconcile().expect("reconciled"), Reconciliation::RolledBack);

    assert_eq!(f.history_files(), Vec::<String>::new());
    assert!(f.list().entries.is_empty());
    assert_eq!(f.prompt(), ORIGINAL_TEXT);
}

#[test]
fn a_later_acceptance_in_progress_hides_only_its_own_entry() {
    // DHS-FR-12 with DHS-FR-07: an acceptance against a draft that already holds
    // versions creates one entry and hides one — the entries already settled are
    // exposed throughout, so a review under way costs the rail nothing.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    f.accept_new("the first accepted rewrite\n");
    let standing: Vec<String> = f.list().entries.iter().map(|e| e.id.clone()).collect();
    assert_eq!(standing.len(), 2);

    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    assert_eq!(
        journal.original_entry_id, None,
        "the Original is settled once and by the first acceptance alone",
    );
    let entry_id = journal.entry_id.clone().expect("an entry id");

    let listed: Vec<String> = f.list().entries.iter().map(|e| e.id.clone()).collect();
    assert_eq!(listed, standing, "the settled versions are exposed throughout");
    assert_eq!(load_impl(&f.root, &entry_id).unwrap_err(), ERR_ENTRY_NOT_FOUND);

    // And the rollback leaves those two exactly as they were.
    f.reconcile().expect("rolled back");
    assert_eq!(
        f.list().entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
        standing,
    );
}

#[test]
fn a_rolled_back_acceptance_that_created_no_entry_announces_nothing() {
    // DHS-FR-22 with DHS-FR-16: an operation that was never going to create an
    // entry withdraws none, so a surface is not sent to re-read for a change
    // that never happened.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    // Rewind to the shape a no-change acceptance journals: no entry of either
    // kind, and nothing under `history/` but the journal and its prior payload.
    for id in [
        journal.entry_id.clone().expect("an entry id"),
        journal.original_entry_id.clone().expect("an Original"),
    ] {
        for ext in [SNAPSHOT_EXT, MANIFEST_EXT] {
            std::fs::remove_file(f.hist().join(format!("{id}.{ext}"))).expect("rewind");
        }
    }
    let bare = Journal {
        entry_id: None,
        original_entry_id: None,
        ..journal
    };
    f.root
        .write_toml_atomic(f.hist().join(JOURNAL_FILE), &bare)
        .expect("journal");
    let seen = f.watch();

    f.reconcile().expect("rolled back");

    assert!(
        seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty(),
        "nothing was withdrawn, so nothing was announced",
    );
    assert!(f.list().entries.is_empty());
}

#[test]
fn a_standing_journal_that_cannot_be_cleared_refuses_a_second_acceptance() {
    // DHS-FR-23: two acceptances never interleave over one prompt, and one
    // journal is always enough.
    //
    // The refusal is reached only where recovery itself is blocked (DHS-FR-18),
    // and the *only* phase whose reconciliation both succeeds and leaves the
    // journal standing is `complete` — whose removal is best-effort. A
    // read-only `history/` is what makes that removal fail, which is exactly
    // the state a journal nothing can clear leaves behind.
    use std::os::unix::fs::PermissionsExt;
    // The best-effort removal is made to fail by a chmod, which root ignores.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_standing_journal_that_cannot_be_cleared_refuses_a_second_acceptance",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let first = f.propose(PROPOSED);
    interrupt(&f, &first, Phase::Complete);
    let hist = f.hist();
    let before = f.history_files();
    let mode = std::fs::metadata(&hist).expect("metadata").permissions();
    std::fs::set_permissions(&hist, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");

    let refused = apply_acceptance(
        &f.handle(),
        &f.root,
        &f.root,
        Acceptance {
            hunk_id: None,
            resolves: true,
            proposal: &first,
            candidate: "something else\n",
            comment_body: "Accepted.".into(),
            comment_by: &human(),
        },
    )
    .unwrap_err();
    std::fs::set_permissions(&hist, mode).expect("writable again");

    assert_eq!(refused, ERR_ACCEPTANCE_IN_PROGRESS);
    assert_eq!(f.history_files(), before, "nothing was written anywhere");

    // With the folder writable again the journal clears and the acceptance is
    // no longer refused for that reason.
    f.reconcile().expect("reconciled");
    assert!(!f.history_files().contains(&JOURNAL_FILE.to_string()));
}

// ---------------------------------------------------------------------------
// DHS-FR-07, DHS-FR-16 — the event (DHS-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn a_rolled_back_acceptance_withdraws_its_entry_and_a_refusal_announces_nothing() {
    // DHS-FR-07, DHS-FR-16 (DHS-FR-22): a null entry says the version the operation had
    // written is gone again — and an operation that was refused, or that was
    // never going to create one, announces nothing at all.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    interrupt(&f, &proposal, Phase::Prepared);
    let seen = f.watch();

    f.reconcile().expect("rolled back");

    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["draftId"], f.draft_id);
    assert!(events[0]["entry"].is_null());

    // A refusal announces nothing.
    seen.lock().unwrap_or_else(|e| e.into_inner()).clear();
    assert!(load_impl(&f.root, "no-such-entry").is_err());
    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
}

#[test]
fn the_event_carries_an_entry_when_one_becomes_visible_and_null_when_one_is_withdrawn() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let seen = f.watch();

    let outcome = f.accept_new(PROPOSED);
    let entry = outcome.entry.expect("an entry");
    let original = outcome.original.expect("an Original");
    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // DHS-FR-07 / DHS-FR-22: a first acceptance exposes two versions, oldest
    // first — the `Original` it settled and the version it produced.
    assert_eq!(events.len(), 2, "one event per version that became visible");
    assert_eq!(events[0]["draftId"], f.draft_id);
    assert_eq!(events[0]["entry"]["id"], original.id);
    assert_eq!(events[0]["entry"]["seq"], 1);
    assert_eq!(events[0]["entry"]["source"]["kind"], "original");
    assert_eq!(events[1]["entry"]["id"], entry.id);
    assert_eq!(events[1]["entry"]["seq"], 2);

    // And every acceptance after it exposes exactly one.
    seen.lock().unwrap_or_else(|e| e.into_inner()).clear();
    f.accept_new("a third version\n");
    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["entry"]["seq"], 3);
}

/// Every draft event queued for the fixture's draft.
fn draft_events(f: &Fixture) -> usize {
    crate::storage_floor::commit::pending_events(&f.root)
        .into_iter()
        .filter(|(id, _, _)| id == &f.draft_id)
        .count()
}

// PST-FR-DQZT / DHS-FR-02 / DHS-FR-20 / DHS-FR-19: neither a rolled-forward nor a
// rolled-back acceptance raises a draft event.
#[test]
fn a_reconciled_acceptance_raises_no_draft_event() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    interrupt(&f, &proposal, Phase::Committed);
    let before = draft_events(&f);
    assert!(matches!(
        f.reconcile().expect("rolled forward"),
        Reconciliation::RolledForward { .. }
    ));
    assert_eq!(draft_events(&f), before);

    let g = Fixture::new();
    g.write_prompt(ORIGINAL_TEXT);
    let proposal = g.propose(PROPOSED);
    interrupt(&g, &proposal, Phase::Prepared);
    let before = draft_events(&g);
    assert_eq!(g.reconcile().expect("rolled back"), Reconciliation::RolledBack);
    assert_eq!(draft_events(&g), before);
}

