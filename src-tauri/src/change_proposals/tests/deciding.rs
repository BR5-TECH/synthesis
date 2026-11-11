//! Deciding hunks of a change edit proposal (CEP-FR-OPGW, CEP-FR-ESQR,
//! CEP-FR-LYSS, CEP-FR-UCVO, CEP-FR-RWTQ).

use super::*;

// CEP-FR-OPGW: an accepted hunk writes its whole file in the stream's working
// copy at once, resolved against the file as it stands.
#[test]
fn accepting_a_hunk_writes_its_file_in_the_working_copy() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let outcome = f.accept(&record.id, &tasks).expect("accepted");
    assert!(!outcome.resolved);
    assert!(!outcome.committed);
    assert_eq!(
        f.file("tasks.md").as_deref(),
        Some("## 1. Login\n\n- [ ] 1.1 Add the form\n- [ ] 1.2 Add logout\n")
    );
    let now = f.reread(&record.id);
    assert_eq!(state_of(&now, &tasks), HunkState::Accepted);
    assert_eq!(now.written_paths, vec!["tasks.md".to_string()]);
    // CEP-FR-RWTQ: a decision that leaves a hunk undecided appends no comment.
    assert_eq!(f.thread().comments.len(), 2);
}

// CEP-FR-HRCX: accepting a new-file hunk creates the file; a file that appears
// at that path before the acceptance refuses it with `file_exists`.
#[test]
fn a_new_file_hunk_creates_its_file_and_refuses_an_existing_one() {
    let f = Fixture::new();
    let record = f.pending();
    let spec = row_id(&record, "specs/auth/spec.md");
    f.accept(&record.id, &spec).expect("accepted");
    assert_eq!(f.file("specs/auth/spec.md").as_deref(), Some("## ADDED Requirements\n"));

    let g = Fixture::new();
    let record = g.pending();
    let spec = row_id(&record, "specs/auth/spec.md");
    g.wc.write_text_atomic(
        g.wc.path().join(format!("openspec/changes/{CHANGE}/specs/auth/spec.md")),
        "mine\n",
    )
    .expect("write");
    assert_eq!(g.accept(&record.id, &spec).map(|_| ()), Err(ERR_FILE_EXISTS.to_string()));
    assert_eq!(state_of(&g.reread(&record.id), &spec), HunkState::Pending);
    assert_eq!(g.file("specs/auth/spec.md").as_deref(), Some("mine\n"));
}

// CEP-FR-ESQR: while a run of the stream is working or reviewing, an
// acceptance is refused, writes nothing, and the hunk stays undecided.
#[test]
fn an_acceptance_on_a_busy_stream_is_refused_and_writes_nothing() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    *f.sites.busy.lock().unwrap() = Some("run-1".into());
    assert_eq!(f.accept(&record.id, &tasks).map(|_| ()), Err(ERR_STREAM_BUSY.to_string()));
    assert_eq!(f.file("tasks.md").as_deref(), Some(TASKS_MD));
    assert_eq!(state_of(&f.reread(&record.id), &tasks), HunkState::Pending);
    // A rejection writes nothing into the working copy, so a busy stream does
    // not stop it.
    assert!(f.reject(&record.id, &tasks).is_ok());
}

// CEP-FR-LYSS: an acceptance is refused where the stream is gone or where the
// change folder is not in its working copy.
#[test]
fn an_acceptance_needs_a_live_stream_and_the_change_folder() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let gone = FixedSites { sites: Vec::new(), busy: Mutex::new(None) };
    let refused = decide(
        &f.handle(),
        &f.store,
        &gone,
        DecisionRequest {
            proposal_id: &record.id,
            decision: Decision::Accept,
            hunk: Some(&tasks),
            feedback: None,
            by: &human(),
        },
    );
    assert_eq!(refused.map(|_| ()), Err(ERR_UNKNOWN_STREAM.to_string()));

    f.wc.delete_under(f.wc.path(), format!("openspec/changes/{CHANGE}"), true).expect("delete");
    assert_eq!(f.accept(&record.id, &tasks).map(|_| ()), Err(ERR_CHANGE_NOT_FOUND.to_string()));
}

// PCP-FR-ZQAH as applied by CEP-FR-OPGW: a hunk whose text has gone from its
// file is refused with `anchor_lost`, and nothing is written.
#[test]
fn a_hunk_whose_text_left_its_file_is_anchor_lost() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let path = f.wc.path().join(format!("openspec/changes/{CHANGE}/tasks.md"));
    f.wc.write_text_atomic(&path, "## 1. Other\n").expect("write");
    assert_eq!(f.accept(&record.id, &tasks).map(|_| ()), Err(ERR_ANCHOR_LOST.to_string()));
    assert_eq!(f.file("tasks.md").as_deref(), Some("## 1. Other\n"));
}

// CEP-FR-UCVO / CEP-FR-RWTQ / CEP-FR-FBVC: rejecting the last undecided hunk
// closes the proposal, writes nothing into the working copy, and appends
// exactly one decision comment, whose id and origin kind the outcome names.
#[test]
fn rejecting_every_hunk_closes_with_one_comment_and_no_write() {
    let f = Fixture::new();
    let record = f.pending();
    let first = f.reject(&record.id, &row_id(&record, "tasks.md")).expect("rejected");
    assert!(!first.resolved);
    let last = f.reject(&record.id, &row_id(&record, "specs/auth/spec.md")).expect("rejected");
    assert!(last.resolved);
    assert!(!last.committed);
    assert_eq!(last.proposal.state, ChangeProposalState::Rejected);
    assert_eq!(f.file("tasks.md").as_deref(), Some(TASKS_MD));
    let comments = f.thread().comments;
    assert_eq!(comments.len(), 3);
    assert!(comments[2]
        .body
        .starts_with("Declined the proposed edits to OpenSpec change `add-auth`."));
    assert_eq!(last.comment_id.as_deref(), Some(comments[2].id.as_str()));
    assert_eq!(last.origin_kind, ORIGIN_CHANGE_DISCUSSION);
}

// CEP-FR-UCVO: a decline rejects every undecided hunk at once.
#[test]
fn a_decline_rejects_every_undecided_hunk() {
    let f = Fixture::new();
    let record = f.pending();
    let outcome = f.decide(&record.id, Decision::Decline, None).expect("declined");
    assert!(outcome.resolved);
    assert_eq!(outcome.proposal.counts.rejected, 2);
    assert_eq!(
        f.decide(&record.id, Decision::Decline, None).map(|_| ()),
        Err(ERR_ALREADY_DECIDED.to_string())
    );
}

// CEP-FR-UCVO: a hold marks the hunk `discussing` and decides nothing; an
// author's edit changes the hunk set and never the working copy, and a stale
// baseline is refused.
#[test]
fn a_hold_and_an_edit_write_nothing_into_the_working_copy() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let held = hold_hunk_impl(&f.store, &record.id, &tasks, true).expect("held");
    assert_eq!(state_of(&held, &tasks), HunkState::Discussing);
    assert_eq!(held.state, ChangeProposalState::Pending);

    let read = load_hunks_impl(&f.store, &f.sites, &record.id).expect("hunks");
    let saved = edit_hunk_impl(
        &f.store,
        &record.id,
        &tasks,
        "- [ ] 1.1 Add the form\n- [ ] 1.2 Log out\n",
        &read.checksum,
    )
    .expect("edited");
    assert_ne!(saved.checksum, read.checksum);
    assert!(f.reread(&record.id).row(&tasks).expect("row").edited);
    assert_eq!(f.file("tasks.md").as_deref(), Some(TASKS_MD));
    assert_eq!(
        edit_hunk_impl(&f.store, &record.id, &tasks, "x", &read.checksum).map(|_| ()),
        Err(ERR_CANDIDATE_STALE.to_string())
    );
    // The edited text is what the acceptance writes.
    f.accept(&record.id, &tasks).expect("accepted");
    assert!(f.file("tasks.md").expect("tasks").contains("1.2 Log out"));
}

// PCP-FR-18 as applied: an accepted hunk is final.
#[test]
fn an_accepted_hunk_takes_no_second_decision() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    f.accept(&record.id, &tasks).expect("accepted");
    assert_eq!(f.reject(&record.id, &tasks).map(|_| ()), Err(ERR_HUNK_ALREADY_DECIDED.to_string()));
    assert_eq!(f.accept(&record.id, "nope").map(|_| ()), Err(ERR_HUNK_NOT_FOUND.to_string()));
}

// CEP-FR-SQCG: a decision and a hold each emit the event with their cause.
#[test]
fn decisions_and_holds_emit_the_event() {
    let f = Fixture::new();
    let record = f.pending();
    let seen = f.watch();
    let tasks = row_id(&record, "tasks.md");
    hold_in_process(&f.handle(), &f.store, &record.id, &tasks, true).expect("held");
    f.accept(&record.id, &tasks).expect("accepted");
    let causes: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|e| e["cause"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(causes, ["held", "decided"]);
}

// PCP-FR-14 as applied by CEP-FR-OPGW: an acceptance interrupted after its
// file write is settled forward at the next decision, and one interrupted
// before it is rolled back.
#[test]
fn an_interrupted_acceptance_is_reconciled_from_the_file() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let (_, dir) = read_proposal(&f.store, &record.id).expect("record");
    // A journal whose write never landed: the file still holds its old text.
    let journal = Journal {
        operation_id: "op".into(),
        proposal_id: record.id.clone(),
        hunk_id: tasks.clone(),
        path: "tasks.md".into(),
        prior_sha256: None,
        candidate_sha256: fs::sha256_bytes(b"something else"),
        prior_state: HunkState::Pending,
        prior_decided_at: None,
        path_was_written: false,
        resolves: false,
        comment_id: "c".into(),
        event_id: "e".into(),
        thread_id: f.thread_id.clone(),
        comment_body: String::new(),
        comment_by: human(),
    };
    let mut marked = f.reread(&record.id);
    marked.row_mut(&tasks).expect("row").state = HunkState::Accepted;
    write_record(&f.store, &dir, &marked).expect("record");
    f.store.write_toml_atomic(at(&dir, &record.id, JOURNAL_EXT), &journal).expect("journal");
    let _guard = change_lock(STREAM, CHANGE);
    reconcile_locked(&f.handle(), &f.store, &f.sites, &record.id).expect("reconciled");
    assert_eq!(state_of(&f.reread(&record.id), &tasks), HunkState::Pending);
    assert!(f.store.file_info(at(&dir, &record.id, JOURNAL_EXT)).is_err());
}

// PCP-FR-14 as applied by CEP-FR-OPGW: while an acceptance of a proposal is in
// flight, a second decision of it is refused rather than queued.
#[test]
fn a_decision_during_an_acceptance_is_refused() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let slot = try_acceptance_slot(&record.id).expect("slot");
    assert_eq!(
        f.accept(&record.id, &tasks).map(|_| ()),
        Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string())
    );
    assert_eq!(
        f.reject(&record.id, &tasks).map(|_| ()),
        Err(ERR_ACCEPTANCE_IN_PROGRESS.to_string())
    );
    drop(slot);
    assert!(f.accept(&record.id, &tasks).is_ok());
}

// CEP-FR-WJKY / CEP-FR-EODY: a symlink inside the change folder does not let
// a hunk read or write a file outside it.
#[test]
fn a_symlink_in_the_change_folder_does_not_let_a_hunk_escape() {
    let f = Fixture::new();
    let outside = f.wc.path().join("outside");
    std::fs::create_dir_all(&outside).expect("dir");
    std::fs::write(outside.join("secret.md"), "keep\n").expect("secret");
    std::os::unix::fs::symlink(
        &outside,
        f.wc.path().join(format!("openspec/changes/{CHANGE}/link")),
    )
    .expect("link");
    let read = f.record(&[replace("link/secret.md", "keep\n", "gone\n")]);
    assert!(read.is_err(), "a hunk through a link is refused: {read:?}");
    let created = f.record(&[new_file("link/new.md", "x\n")]);
    if let Ok(record) = created {
        let id = row_id(&record, "link/new.md");
        assert!(f.accept(&record.id, &id).is_err());
    }
    assert!(!outside.join("new.md").exists());
    assert_eq!(std::fs::read_to_string(outside.join("secret.md")).unwrap(), "keep\n");
}

// PCP-FR-14 as applied by CEP-FR-OPGW / CEP-FR-RWTQ / CEP-FR-FOJW: an
// acceptance interrupted after its file write is settled forward: the row is
// accepted, the owed decision comment is appended once, and the close commits.
#[test]
fn an_acceptance_interrupted_after_its_write_is_settled_forward() {
    let f = Fixture::new();
    let record = f
        .record(&[replace("tasks.md", "- [ ] 1.1 Add the form\n", "- [ ] 1.1 Add it\n")])
        .expect("recorded");
    let tasks = row_id(&record, "tasks.md");
    let (_, dir) = read_proposal(&f.store, &record.id).expect("record");
    let candidate = "## 1. Login\n\n- [ ] 1.1 Add it\n";
    let path = f.wc.path().join(format!("openspec/changes/{CHANGE}/tasks.md"));
    let prior = f.wc.sha256_file(&path).expect("sha");
    // The write landed, then the process stopped before the comment.
    f.wc.write_text_atomic(&path, candidate).expect("write");
    let journal = Journal {
        operation_id: "op".into(),
        proposal_id: record.id.clone(),
        hunk_id: tasks.clone(),
        path: "tasks.md".into(),
        prior_sha256: Some(prior),
        candidate_sha256: fs::sha256_bytes(candidate.as_bytes()),
        prior_state: HunkState::Pending,
        prior_decided_at: None,
        path_was_written: false,
        resolves: true,
        comment_id: "decision-1".into(),
        event_id: "event-1".into(),
        thread_id: f.thread_id.clone(),
        comment_body: "Accepted the proposed edits to OpenSpec change `add-auth`.".into(),
        comment_by: human(),
    };
    f.store.write_toml_atomic(at(&dir, &record.id, JOURNAL_EXT), &journal).expect("journal");
    let before = f.commits();
    {
        let _guard = change_lock(STREAM, CHANGE);
        reconcile_locked(&f.handle(), &f.store, &f.sites, &record.id).expect("reconciled");
        // A second run does nothing a second time.
        reconcile_locked(&f.handle(), &f.store, &f.sites, &record.id).expect("again");
    }
    let now = f.reread(&record.id);
    assert_eq!(state_of(&now, &tasks), HunkState::Accepted);
    assert!(!now.comment_owed);
    assert_eq!(now.written_paths, vec!["tasks.md".to_string()]);
    let comments = f.thread().comments;
    assert_eq!(comments.iter().filter(|c| c.id == "decision-1").count(), 1);
    assert_eq!(f.commits(), before + 1);
    assert!(f.store.file_info(at(&dir, &record.id, JOURNAL_EXT)).is_err());
}

// CEP-FR-UCVO / CEP-FR-RWTQ: a decline writes nothing into the working copy,
// and a hold or an edit appends no comment.
#[test]
fn a_decline_writes_nothing_and_a_hold_or_edit_appends_no_comment() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    hold_hunk_impl(&f.store, &record.id, &tasks, true).expect("held");
    let read = load_hunks_impl(&f.store, &f.sites, &record.id).expect("hunks");
    edit_hunk_impl(&f.store, &record.id, &tasks, "- [ ] x\n", &read.checksum).expect("edited");
    assert_eq!(f.thread().comments.len(), 2);
    let before = crate::tools::tests::tree_snapshot(f.wc.path());
    f.decide(&record.id, Decision::Decline, None).expect("declined");
    assert_eq!(crate::tools::tests::tree_snapshot(f.wc.path()), before);
}

// CEP-FR-MZFF: a recording, a decision, and a refusal each log under
// `backend` with the ids they know, and no record holds hunk or file text.
#[test]
fn every_operation_logs_ids_and_counts_and_no_text() {
    crate::logging::BUFFER.clear();
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    *f.sites.busy.lock().unwrap() = Some("run-1".into());
    let refused = decide_command(
        &f.handle(),
        "accept_hunk",
        &f.store,
        &f.sites,
        DecisionRequest {
            proposal_id: &record.id,
            decision: Decision::Accept,
            hunk: Some(&tasks),
            feedback: None,
            by: &human(),
        },
    );
    assert!(refused.is_err());
    let records = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                domains: vec![crate::logging::Domain::Backend],
                ..Default::default()
            },
            None,
            1000,
        )
        .expect("buffer")
        .records;
    let mine: Vec<serde_json::Value> = records
        .iter()
        .map(|r| serde_json::json!({ "message": r.message, "level": format!("{:?}", r.level), "fields": r.fields }))
        .filter(|r| r.to_string().contains(&record.id) || r.to_string().contains(CHANGE))
        .collect();
    let recorded =
        mine.iter().find(|r| r["message"] == "change edit proposal recorded").expect("recorded");
    assert_eq!(recorded["fields"]["streamId"], STREAM);
    assert_eq!(recorded["fields"]["hunks"], 2);
    let warn =
        mine.iter().find(|r| r["message"] == "change edit proposal refused").expect("refused");
    assert_eq!(warn["fields"]["proposalId"], record.id.as_str());
    assert_eq!(warn["fields"]["reason"], ERR_STREAM_BUSY);
    let all = serde_json::to_string(&mine).expect("json");
    assert!(!all.contains("Add logout") && !all.contains("Add the form") && !all.contains("ADDED"));
}
