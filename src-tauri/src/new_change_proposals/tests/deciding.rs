//! Deciding a new-change proposal and confirming the split.

use super::*;

// NCP-FR-UWOR / NCP-FR-KRSA / NCP-FR-EBSA: an acceptance writes the new
// change folder in the working copy, commits exactly its files, records the
// created change, and appends one decision comment.
#[test]
fn an_acceptance_creates_and_commits_the_new_change() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let outcome = f.decide(&record.id, true).expect("accepted");
    assert!(outcome.committed);
    assert_eq!(outcome.proposal.state, NewChangeProposalState::Accepted);
    assert_eq!(outcome.proposal.created_change.as_deref(), Some("part-one"));
    assert_eq!(
        f.wc_file("openspec/changes/part-one/proposal.md").as_deref(),
        Some("## Why\n\npart-one\n")
    );
    assert_eq!(
        f.head(),
        (
            "Add OpenSpec change part-one".to_string(),
            vec![
                "openspec/changes/part-one/proposal.md".to_string(),
                "openspec/changes/part-one/specs/auth/spec.md".to_string(),
            ]
        )
    );
    let last = f.thread().comments.last().cloned().expect("decision");
    assert_eq!(last.body, "Accepted the proposed new OpenSpec change `part-one` (1 of 2).");
    assert_eq!(outcome.comment_id.as_deref(), Some(last.id.as_str()));
    assert_eq!(outcome.origin_kind, "change_discussion");
    // NCP-FR-MHBS: the original change is not touched.
    assert_eq!(
        f.wc_file(&format!("openspec/changes/{CHANGE}/proposal.md")).as_deref(),
        Some("## Why\n\nToo big.\n")
    );
}

// NCP-FR-TWDN / NCP-FR-WYZI: a rejection writes nothing into the working
// copy; a second decision is already decided; an unknown id is not found.
#[test]
fn a_rejection_writes_nothing() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let before = crate::tools::tests::tree_snapshot(f.wc.path());
    let outcome = f.decide(&record.id, false).expect("rejected");
    assert!(!outcome.committed);
    assert_eq!(crate::tools::tests::tree_snapshot(f.wc.path()), before);
    assert_eq!(f.decide(&record.id, true).map(|_| ()), Err(ERR_ALREADY_DECIDED.to_string()));
    assert_eq!(f.decide("nope", true).map(|_| ()), Err(ERR_PROPOSAL_NOT_FOUND.to_string()));
}

// NCP-FR-BOVN: an acceptance on a busy stream, and one whose folder exists
// now, are refused and write nothing.
#[test]
fn an_acceptance_on_a_busy_stream_or_an_existing_folder_is_refused() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    *f.sites.busy.lock().unwrap() = Some("run-1".into());
    assert_eq!(f.decide(&record.id, true).map(|_| ()), Err(ERR_STREAM_BUSY.to_string()));
    *f.sites.busy.lock().unwrap() = None;
    std::fs::create_dir_all(f.wc.path().join("openspec/changes/part-one")).expect("dir");
    assert_eq!(f.decide(&record.id, true).map(|_| ()), Err(ERR_CHANGE_EXISTS.to_string()));
    assert!(f.wc_file("openspec/changes/part-one/proposal.md").is_none());
}

// NCP-FR-KRSA: a commit that fails leaves the files written and the proposal
// owing the commit; completion makes it and clears the mark.
#[test]
fn a_failed_commit_is_owed_and_completed() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let lock = f.wc.path().join(".git/index.lock");
    std::fs::write(&lock, "").expect("lock");
    let outcome = f.decide(&record.id, true).expect("accepted");
    assert!(!outcome.committed);
    assert!(outcome.proposal.commit_owed);
    assert!(f.wc_file("openspec/changes/part-one/proposal.md").is_some());
    std::fs::remove_file(&lock).expect("unlock");
    let seen = f.watch();
    let done = complete_new_change(&f.handle(), &f.store, &f.sites, &record.id).expect("completed");
    assert!(done.committed);
    assert!(!done.proposal.commit_owed);
    assert_eq!(f.head().0, "Add OpenSpec change part-one");
    assert_eq!(seen.lock().unwrap()[0]["cause"], "committed");
}

// NCP-FR-MQCS / NCP-FR-MHBS / NCP-FR-AFZP / NCP-FR-VLOO: a confirmation
// records the accepted changes, appends one comment, and closes the split.
#[test]
fn a_confirmation_closes_the_split() {
    let f = Fixture::new();
    assert_eq!(
        confirm_split(&f.handle(), &f.store, STREAM, CHANGE, &human()).map(|_| ()),
        Err(ERR_NOTHING_ACCEPTED.to_string())
    );
    let first = f.record_named("part-one", 1, 2).expect("recorded");
    assert_eq!(
        confirm_split(&f.handle(), &f.store, STREAM, CHANGE, &human()).map(|_| ()),
        Err(ERR_PROPOSAL_PENDING.to_string())
    );
    f.decide(&first.id, true).expect("accepted");
    let seen = f.watch();
    let split = confirm_split(&f.handle(), &f.store, STREAM, CHANGE, &human()).expect("confirmed");
    assert_eq!(split.accepted, vec!["part-one".to_string()]);
    assert_eq!(read_split(&f.store, STREAM, CHANGE), Some(split));
    assert!(f.thread().comments.last().expect("comment").body.contains("Confirmed the split"));
    assert_eq!(seen.lock().unwrap()[0]["cause"], "confirmed");
    assert_eq!(f.record_named("part-two", 2, 2), Err(ERR_SPLIT_CONFIRMED.to_string()));
    assert_eq!(
        confirm_split(&f.handle(), &f.store, STREAM, CHANGE, &human()).map(|_| ()),
        Err(ERR_SPLIT_CONFIRMED.to_string())
    );
    assert_eq!(
        f.wc_file(&format!("openspec/changes/{CHANGE}/proposal.md")).as_deref(),
        Some("## Why\n\nToo big.\n")
    );
}

// NCP-FR-CEIA / NCP-FR-BFCK / NCP-FR-OLPB: the module decides nothing on a
// timer, reaches no network, and logs ids and counts with no file text.
#[test]
fn the_module_holds_no_timer_and_no_network() {
    for source in [
        include_str!("../../new_change_proposals.rs"),
        include_str!("../deciding.rs"),
        include_str!("../recording.rs"),
        include_str!("../model.rs"),
        include_str!("../commands.rs"),
    ] {
        for forbidden in [
            "std::thread",
            "sleep(",
            "reqwest",
            "dispatch_agent_turn",
            "std::fs::",
            "draft_proposals::",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    let commands = include_str!("../commands.rs");
    assert!(!commands.contains("\"rationale\" =>") && !commands.contains("\"text\" =>"));
}

fn record_with(f: &Fixture, name: &str, files: &[ProposedFile]) -> NewChangeProposal {
    record_new_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        NewChange {
            stream_id: STREAM,
            change: CHANGE,
            name,
            files,
            position: 1,
            total: 2,
            rationale: "r",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
    .expect("recorded")
}

fn stored(f: &Fixture, id: &str) -> NewChangeProposal {
    let path = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/new-changes/{id}.toml"));
    toml::from_str(&std::fs::read_to_string(path).expect("record")).expect("toml")
}

// NCP-FR-KRSA: after a commit that lands, the stored record no longer owes
// the commit.
#[test]
fn a_landed_commit_clears_the_stored_mark() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    f.decide(&record.id, true).expect("accepted");
    let on_disk = stored(&f, &record.id);
    assert_eq!(on_disk.state, NewChangeProposalState::Accepted);
    assert!(!on_disk.commit_owed);
}

// NCP-FR-KRSA: a file that will not write removes the files written before
// it and the new folder; the proposal stays pending and no commit is made.
#[test]
fn a_failed_file_write_removes_the_written_files() {
    let f = Fixture::new();
    let files = vec![
        ProposedFile { path: "proposal.md".into(), text: "## Why\n\nx\n".into() },
        ProposedFile { path: "proposal.md/inner.md".into(), text: "x\n".into() },
    ];
    let record = record_with(&f, "part-one", &files);
    let head_before = f.head();
    assert_eq!(f.decide(&record.id, true).map(|_| ()), Err(ERR_WRITE_FAILED.to_string()));
    assert!(!f.wc.path().join("openspec/changes/part-one").exists());
    assert_eq!(f.head(), head_before);
    assert_eq!(stored(&f, &record.id).state, NewChangeProposalState::Pending);
}

// NCP-FR-KRSA: a record that will not write removes the new folder, so no
// change stands beside a pending proposal, and no commit is made.
#[cfg(unix)]
#[test]
fn a_failed_record_write_removes_the_folder() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let dir = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/new-changes"));
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let head_before = f.head();
    let outcome = f.decide(&record.id, true).map(|_| ());
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert_eq!(outcome, Err(ERR_WRITE_FAILED.to_string()));
    assert!(!f.wc.path().join("openspec/changes/part-one").exists());
    assert_eq!(f.head(), head_before);
    assert_eq!(stored(&f, &record.id).state, NewChangeProposalState::Pending);
}
