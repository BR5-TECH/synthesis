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
    // of the steps are transposed — so the order is observed directly.
    //
    // The steps are made by four different modules, all through the shared
    // filesystem primitives, and those primitives keep a record of the paths
    // they write (`fs::write_log`, test builds only). The order of that record
    // is the order of the writes. File times are not used: a filesystem
    // stamps them in coarse steps, so two writes of one call can carry the
    // same time and prove nothing.
    use crate::fs::write_log;

    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose(PROPOSED);
    let dir = drafts::draft_dir(&f.root, &f.draft_id).expect("dir");
    let hist = f.hist();
    let proposals = drafts::draft_proposals_dir(&f.root, &f.draft_id).expect("proposals");

    write_log::start();
    let outcome = f.accept(&proposal);
    let writes = write_log::finish();
    let outcome = outcome.expect("accepted");
    let entry = outcome.entry.expect("an entry");
    let original = outcome.original.expect("the Original it settled");

    let resolved = |path: std::path::PathBuf| -> std::path::PathBuf {
        std::fs::canonicalize(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"))
    };
    let writes: Vec<std::path::PathBuf> =
        writes.into_iter().filter_map(|p| std::fs::canonicalize(p).ok()).collect();
    // The journal is rewritten along the way, so the files below are located by
    // their own paths. Each is written once; the first write is the one that
    // counts if a path ever appears twice.
    let position = |label: &str, path: std::path::PathBuf| -> usize {
        let path = resolved(path);
        writes
            .iter()
            .position(|written| *written == path)
            .unwrap_or_else(|| panic!("{label} was not written through the primitives: {writes:?}"))
    };
    let original_payload = position("the Original's payload", snapshot_path(&hist, &original.id));
    let original_manifest = position("the Original's manifest", manifest_path(&hist, &original.id));
    let payload = position("the payload", snapshot_path(&hist, &entry.id));
    let manifest = position("the manifest", manifest_path(&hist, &entry.id));
    let prompt = position("the prompt", dir.join(drafts::FILES_DIR).join(FILE));
    let record = position("the proposal record", proposals.join(format!("{}.toml", proposal.id)));
    // CMS-FR-37: the decision comment lands in the store, under the draft's
    // stable id, rather than inside the draft's own folder.
    let conversation = position(
        "the decision comment",
        f.root.path().join(format!("drafts/{}/comments/discussion.jsonl", f.draft_id)),
    );

    assert!(
        original_payload < original_manifest,
        "the Original's payload is written before its manifest",
    );
    assert!(
        original_manifest < payload,
        "the superseded version is settled before the one that supersedes it",
    );
    assert!(payload < manifest, "the payload is written before its manifest");
    assert!(manifest < prompt, "the entry is written before the prompt");
    assert!(prompt < record, "the prompt is written before the proposal record");
    assert!(record < conversation, "the decision comment is appended after the record moved");

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
    let original_id =
        journal.original_entry_id.clone().expect("a first acceptance settles the Original too");
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
    let files = drafts::draft_dir(&f.root, &f.draft_id).expect("dir").join("files");
    // The atomic write stages a temp file beside its target and renames it into
    // place, so a read-only `files/` refuses both halves — the draft still holds
    // exactly its one prompt, which is what makes this a *write* failure rather
    // than the inconsistency refusal of DRS-FR-15.
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&files).expect("metadata").permissions();
    std::fs::set_permissions(&files, std::fs::Permissions::from_mode(0o555)).expect("read-only");

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
        dcp::find_proposal(&f.root, &proposal.id).expect("still there").1.state,
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

/// Every draft event queued for one draft of a fixture.
fn draft_events(f: &Fixture) -> usize {
    crate::storage_floor::commit::pending_events(&f.root)
        .into_iter()
        .filter(|(id, _, _)| id == &f.draft_id)
        .count()
}

// PST-FR-DQZT / DHS-FR-02 / DHS-FR-14: an acceptance raises no draft event, so
// the accepted prompt stays uncommitted on disk until the next draft event.
#[test]
fn an_acceptance_raises_no_draft_event() {
    let f = Fixture::new();
    let before = draft_events(&f);
    f.accept_new("rewritten\n");
    f.accept_new("rewritten again\n");
    assert_eq!(draft_events(&f), before, "no event for an acceptance");
    assert_eq!(f.prompt(), "rewritten again\n", "the accepted prompt is on disk");
}
