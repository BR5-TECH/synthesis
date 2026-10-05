//! Tests for the order and the rollback of the acceptance transaction.

use super::*;

// ---------------------------------------------------------------------------
// DHS-FR-13, DHS-FR-14, DHS-FR-15, DHS-FR-17, DHS-FR-19 — the transaction's order and its rollback
// (DHS-FR-13, FR-14, FR-15, FR-17, FR-19)
// ---------------------------------------------------------------------------

#[test]
fn the_journal_is_gone_once_the_transaction_completes() {
    let f = Fixture::new();
    let outcome = f.accept_new("rewritten\n");
    let entry = outcome.entry.expect("an entry");

    let names = f.history_files();
    assert!(!names.contains(&JOURNAL_FILE.to_string()), "{names:?}");
    assert!(!names.contains(&JOURNAL_PRIOR.to_string()), "{names:?}");
    assert_eq!(
        names,
        {
            let mut expected = vec![
                format!("{}.toml", f.list().entries[0].id),
                format!("{}.snapshot", f.list().entries[0].id),
                format!("{}.toml", entry.id),
                format!("{}.snapshot", entry.id),
            ];
            expected.sort();
            expected
        },
        "two entries and nothing else",
    );
}

#[test]
fn the_transaction_writes_in_the_one_order_that_makes_it_recoverable() {
    // DHS-FR-14, DHS-FR-15 (DHS-FR-13, FR-14, FR-15). The whole crash/recovery matrix rests
    // on this order, and every end-state assertion elsewhere stays green if two
    // of the steps are transposed — so the order is observed directly, by
    // watching the modification times of the files each step writes.
    //
    // Times rather than a recording hook, because the steps are made by four
    // different modules through the shared filesystem primitives, and a hook
    // threaded through all four would be a second implementation of the order
    // rather than an observation of it.
    //
    // The steps are inside one call, so nothing can be slept between them: what
    // makes the stamps distinguishable is the filesystem's own resolution. The
    // assertion below therefore proves it CAN distinguish them before it
    // asserts the order — on a filesystem stamping whole seconds every `<=`
    // here would hold whatever order the writes were made in, and this test
    // would pass by observing nothing.
    use std::time::SystemTime;

    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let dir = drafts::draft_dir(&f.root, &f.draft_id).expect("dir");
    let hist = f.hist();
    let proposals = drafts::draft_proposals_dir(&f.root, &f.draft_id).expect("proposals");

    // The journal is removed by the time the call returns, so the two files that
    // only exist mid-transaction are watched by *creation* rather than by
    // survival: a `prepared` journal is staged, its stamps read, and the
    // transaction then resumed. What the ordering below asserts is the part that
    // outlives the call — payload before manifest, both before the prompt, the
    // prompt before the proposal record, and the decision comment after all of
    // them.
    let outcome = f.accept(&proposal).expect("accepted");
    let entry = outcome.entry.expect("an entry");

    let at = |path: std::path::PathBuf| -> SystemTime {
        std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("{path:?}: {e}"))
            .modified()
            .expect("mtime")
    };
    let original = outcome.original.expect("the Original it settled");
    let original_payload = at(snapshot_path(&hist, &original.id));
    let original_manifest = at(manifest_path(&hist, &original.id));
    let payload = at(snapshot_path(&hist, &entry.id));
    let manifest = at(manifest_path(&hist, &entry.id));
    let prompt = at(dir.join(drafts::FILES_DIR).join(FILE));
    let record = at(proposals.join(format!("{}.toml", proposal.id)));
    // CMS-FR-37: the decision comment lands in the store, under the draft's
    // stable id, rather than inside the draft's own folder.
    let conversation = at(
        f.root
            .path()
            .join(format!("drafts/{}/comments/discussion.jsonl", f.draft_id)),
    );

    assert!(
        payload > original_payload || conversation > payload,
        "this filesystem stamps too coarsely to observe the order at all — \
         every assertion below would hold whatever order the writes were made in",
    );
    assert!(
        original_payload <= original_manifest,
        "the Original's payload is written before its manifest",
    );
    assert!(
        original_manifest <= payload,
        "the superseded version is settled before the one that supersedes it",
    );
    assert!(payload <= manifest, "the payload is written before its manifest");
    assert!(manifest <= prompt, "the entry is written before the prompt");
    assert!(prompt <= record, "the prompt is written before the proposal record");
    assert!(
        record <= conversation,
        "the decision comment is appended after the record moved",
    );

    // And the journal is gone, which is only true of an operation that reached
    // `complete` (DHS-FR-14).
    assert!(!hist.join(JOURNAL_FILE).exists());
    assert!(!hist.join(JOURNAL_PRIOR).exists());
}

/// DHS-FR-15's first two steps, which no completed transaction leaves behind:
/// the prior payload and the `prepared` journal are written **before** any entry
/// file, so a crash in that window has something to roll back to.
#[test]
fn the_journal_and_its_prior_payload_are_written_before_any_entry_file() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let hist = f.hist();

    // The state the transaction is in the instant before it writes the entry —
    // staged through the module's own replay of DHS-FR-15, which is what keeps
    // this from drifting from the production path.
    let before = f.history_files();
    assert!(!before.contains(&JOURNAL_FILE.to_string()));
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    let entry_id = journal.entry_id.clone().expect("an entry id");

    // Both journal files stand, and the entry they name is on disk but
    // **unexposed** — so nothing could have listed it before the commit point.
    assert!(hist.join(JOURNAL_FILE).is_file());
    assert!(hist.join(JOURNAL_PRIOR).is_file());
    assert_eq!(
        std::fs::read_to_string(hist.join(JOURNAL_PRIOR)).expect("prior"),
        ORIGINAL_TEXT,
        "the prior payload is the bytes a rollback restores",
    );
    assert!(manifest_path(&hist, &entry_id).is_file());
    assert!(!f.list().entries.iter().any(|e| e.id == entry_id));
    // DHS-FR-12: both entries the operation staged, the `Original` included —
    // exposing one without the other would put a version in the rail for an
    // acceptance that may yet be rolled back.
    let original_id = journal
        .original_entry_id
        .clone()
        .expect("a first acceptance settles the Original too");
    assert!(manifest_path(&hist, &original_id).is_file());
    assert!(f.list().entries.is_empty());
    assert_eq!(load_impl(&f.root, &original_id).unwrap_err(), ERR_ENTRY_NOT_FOUND);
}

#[test]
fn a_failed_prompt_write_rolls_the_whole_acceptance_back() {
    // DHS-FR-17 (DHS-FR-17, DHS-FR-19). The read-only `files/` below is what
    // makes the prompt write fail — both halves of the atomic write, the staged
    // temp file and the rename onto the target, are refused — and nothing else
    // about the draft changes. It makes nothing fail for root.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_prompt_write_rolls_the_whole_acceptance_back",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let files = drafts::draft_dir(&f.root, &f.draft_id)
        .expect("dir")
        .join("files");
    // The atomic write stages a temp file beside its target and renames it into
    // place, so a read-only `files/` refuses both halves — the draft still holds
    // exactly its one prompt, which is what makes this a *write* failure rather
    // than the inconsistency refusal of DRS-FR-15.
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&files).expect("metadata").permissions();
    std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");

    let refused = f.accept(&proposal).unwrap_err();
    std::fs::set_permissions(&files, mode).expect("writable again");

    assert_eq!(refused, ERR_WRITE_FAILED);
    assert_eq!(f.prompt(), ORIGINAL_TEXT, "the prompt is byte-for-byte what it was");
    assert_eq!(
        f.history_files(),
        Vec::<String>::new(),
        "no Original was left behind for a version that was never superseded",
    );
    assert_eq!(
        dcp::find_proposal(&f.root, &proposal.id)
            .expect("still there")
            .1
            .state,
        dcp::ProposalState::Pending,
    );
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "the proposal's own comment and nothing else — no decision comment was \
         appended, and none can have been",
    );

    // And with the folder writable again, the same acceptance lands whole.
    let outcome = f.accept(&proposal).expect("accepted on the retry");
    assert!(outcome.entry.is_some());
    assert_eq!(f.prompt(), PROPOSED);
}

/// The accepted-change events queued for one draft of a fixture.
fn accepted_events(f: &Fixture) -> usize {
    crate::storage_floor::commit::pending_events(&f.root)
        .into_iter()
        .filter(|(id, event, _)| {
            id == &f.draft_id && *event == crate::storage_floor::commit::DraftEvent::Accepted
        })
        .count()
}

// PST-FR-DQZT / DHS-FR-02 / DHS-FR-14: an acceptance raises one accepted-change
// draft event at its commit point; a rolled-back one raises none.
#[test]
fn an_acceptance_raises_one_accepted_change_event_at_its_commit_point() {
    let f = Fixture::new();
    let before = accepted_events(&f);
    f.accept_new("rewritten\n");
    assert_eq!(accepted_events(&f), before + 1, "one event per acceptance");
    let message = crate::storage_floor::commit::pending_events(&f.root)
        .into_iter()
        .rev()
        .find(|(id, event, _)| {
            id == &f.draft_id && *event == crate::storage_floor::commit::DraftEvent::Accepted
        })
        .map(|(_, _, message)| message)
        .expect("the event");
    assert!(message.starts_with("draft: accept change to \""), "{message}");
}

// PST-FR-DQZT / DHS-FR-19: an acceptance rolled back before its commit point
// raises no draft event, so nothing is committed for a change that did not land.
#[test]
fn a_rolled_back_acceptance_raises_no_event() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_rolled_back_acceptance_raises_no_event",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let files = drafts::draft_dir(&f.root, &f.draft_id)
        .expect("dir")
        .join("files");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&files).expect("metadata").permissions();
    std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");
    let before = accepted_events(&f);

    let refused = f.accept(&proposal).unwrap_err();
    std::fs::set_permissions(&files, mode).expect("writable again");

    assert_eq!(refused, ERR_WRITE_FAILED);
    assert_eq!(accepted_events(&f), before, "no event for a rolled-back acceptance");
}
