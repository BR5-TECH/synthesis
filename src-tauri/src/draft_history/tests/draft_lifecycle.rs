//! Tests for a history across the life of its draft, the path refusals, the log, the commands and the statistics line.

use super::*;

// ---------------------------------------------------------------------------
// DHS-FR-06, DHS-FR-09, DHS-FR-02, DRS-FR-20, DRS-FR-21 — entries are immutable and go with the draft
// (DHS-FR-02, DHS-FR-06, DHS-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn a_rename_leaves_every_manifest_exactly_as_it_was() {
    let f = Fixture::new();
    f.accept_new("rewritten\n");
    let before = f.list().entries;

    drafts::rename_draft_impl(&f.root, &f.draft_id, "overview").expect("rename");

    let after = f.list();
    assert_eq!(after.entries, before, "no manifest was rewritten by the rename");
    assert!(after.entries.iter().all(|e| e.path == FILE));
    assert_eq!(after.live.path, "overview.md", "while the live prompt moved");
}

#[test]
fn deleting_a_draft_takes_its_history_with_it_and_writes_nothing_outside() {
    let f = Fixture::new();
    f.accept_new("rewritten\n");
    let hist = f.hist();
    std::fs::write(f.root.join("README.md"), b"untouched").expect("project file");

    drafts::delete_draft_impl(&f.root, &f.root, &f.draft_id).expect("deleted");

    assert!(!hist.exists());
    assert_eq!(
        std::fs::read_to_string(f.root.join("README.md")).expect("read"),
        "untouched",
    );
}

#[test]
/// DRS-FR-20: graduating is neither archiving nor deletion. The draft's history
/// is exactly where it was, and nothing of it reached the project tree — which
/// is what makes "the prompt that produced a specification is still readable a
/// year later" true rather than aspirational.
fn graduating_a_draft_leaves_its_history_exactly_where_it_was() {
    let f = Fixture::new();
    f.accept_new("rewritten\n");
    let hist = f.hist();
    let before = std::fs::read_dir(&hist)
        .expect("history")
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .count();

    drafts::set_draft_graduated(&f.root, &f.draft_id, "run-1").expect("graduated");

    assert!(hist.exists(), "the history folder is retained");
    let after = std::fs::read_dir(&hist)
        .expect("history")
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .count();
    assert_eq!(before, after, "and every entry in it");
    // Nothing of the draft's own history reached the project.
    assert!(!f.root.join("docs").exists());
}

// ---------------------------------------------------------------------------
// DHS-FR-24, FSA-FR-10, DHS-FR-25, DRS-FR-11 — path escapes and the typed refusals
// (DHS-FR-24, DHS-FR-25)
// ---------------------------------------------------------------------------

#[test]
fn an_entry_id_crafted_to_escape_the_history_folder_is_refused_having_written_nothing() {
    let f = Fixture::new();
    for escape in ["../../etc/passwd", "/etc/passwd", "..", "a/b", ""] {
        assert!(!is_valid_entry_id(escape), "{escape:?} must be refused");
        assert_eq!(
            load_impl(&f.root, escape).unwrap_err(),
            ERR_ENTRY_NOT_FOUND,
            "{escape:?}",
        );
    }
    assert_eq!(f.history_files(), Vec::<String>::new(), "nothing was written");
}

#[test]
fn an_inconsistent_draft_and_an_unknown_id_are_typed_refusals() {
    let f = Fixture::new();
    f.accept_new("rewritten\n");
    assert_eq!(
        list_impl(&f.root, "no-such-draft").unwrap_err(),
        ERR_DRAFT_NOT_FOUND,
    );
    assert_eq!(
        f.reconcile_of("no-such-draft").unwrap_err(),
        ERR_DRAFT_NOT_FOUND,
    );
    assert_eq!(load_impl(&f.root, "no-such-entry").unwrap_err(), ERR_ENTRY_NOT_FOUND);

    // DRS-FR-11: a second file by hand, which is the only way one gets there.
    let entry_id = f.list().entries[0].id.clone();
    std::fs::write(
        drafts::draft_dir(&f.root, &f.draft_id)
            .expect("dir")
            .join("files")
            .join("smuggled.md"),
        "by hand",
    )
    .expect("second file");

    assert_eq!(
        list_impl(&f.root, &f.draft_id).unwrap_err(),
        drafts::ERR_NOT_SINGLE_FILE,
    );
    assert_eq!(
        load_impl(&f.root, &entry_id).unwrap_err(),
        drafts::ERR_NOT_SINGLE_FILE,
    );
}

// ---------------------------------------------------------------------------
// DHS-FR-26 — what the log says, and what it must never say (DHS-FR-26)
// ---------------------------------------------------------------------------

#[test]
fn the_log_names_the_draft_and_the_entry_and_carries_no_composed_text() {
    // Distinctive enough that a substring search over the whole buffer is a
    // real assertion: if any of these reached a record, they are what a user
    // exporting their logs would hand to a stranger.
    const PROMPT_TEXT: &str = "zqxPROMPTBODYzqx";
    const CANDIDATE_TEXT: &str = "zqxCANDIDATEBODYzqx";

    let f = Fixture::new();
    f.write_prompt(PROMPT_TEXT);
    let proposal = f.propose(CANDIDATE_TEXT);
    f.accept(&proposal).expect("accepted");

    let records = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter::default(),
            None,
            crate::logging::BUFFER_CAPACITY,
        )
        .expect("the buffer answers a default filter")
        .records;

    let mine: Vec<_> = records
        .iter()
        .filter(|r| {
            r.fields.get("draftId")
                == Some(&serde_json::Value::String(f.draft_id.clone()))
        })
        .collect();
    // DHS-FR-26: one record per entry the acceptance created — two for a first
    // acceptance, which settles the `Original` as well (DHS-FR-07). A record for
    // "the entry" would leave the other version unaccounted for in the one
    // channel this is debuggable from.
    let created: Vec<_> = mine
        .iter()
        .filter(|r| r.message == "draft history entry created")
        .collect();
    assert_eq!(
        created.len(),
        2,
        "one record per version created: {:?}",
        mine.iter().map(|r| &r.message).collect::<Vec<_>>(),
    );
    assert!(
        created.iter().any(|r| {
            r.fields.get("source") == Some(&serde_json::Value::String("original".into()))
        }),
        "no record names the Original the acceptance settled",
    );
    // …and the commit names both, so a reader of that one line can find them.
    let commit = mine
        .iter()
        .find(|r| r.message == "draft acceptance committed")
        .expect("a commit record");
    for field in ["entryId", "originalEntryId"] {
        assert!(
            commit
                .fields
                .get(field)
                .and_then(|v| v.as_str())
                .is_some_and(|id| !id.is_empty()),
            "the commit record does not name {field}",
        );
    }
    assert!(
        mine.iter().any(|r| r.message == "draft acceptance committed"),
        "no record for the commit: {:?}",
        mine.iter().map(|r| &r.message).collect::<Vec<_>>(),
    );
    // Under the `backend` domain, which is what the Logs panel filters on.
    for record in &mine {
        assert!(
            record.domains.contains(&crate::logging::Domain::Backend),
            "{:?} carries no backend domain",
            record.message,
        );
    }

    // DHS-FR-26: no record **anywhere in the buffer** carries a byte of the
    // snapshot, the prompt, the candidate, the rationale, or any feedback. The
    // whole buffer rather than this draft's records, because a leak that landed
    // on a record naming no draft would pass a narrower check.
    let haystack = serde_json::to_string(&records).expect("records serialise");
    for secret in [PROMPT_TEXT, CANDIDATE_TEXT, "The opening buries the point."] {
        assert!(
            !haystack.contains(secret),
            "{secret:?} reached the session log",
        );
    }
}

// ---------------------------------------------------------------------------
// The two commands themselves (DHS-FR-18, DHS-FR-25)
// ---------------------------------------------------------------------------

#[test]
fn the_commands_reconcile_before_they_answer_and_refuse_a_closed_project() {
    // DHS-FR-18: `reconcile` runs before `list_draft_history` and before
    // `load_draft_history_entry`. Asserted against the **commands** rather than
    // against a stand-in for them: the reconciliation is one line inside each,
    // and a test that calls `list_impl` directly stays green if that line is
    // deleted.
    use tauri::Manager;

    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    // A draft that already holds versions, so the reconciliation has something
    // to leave standing as well as something to withdraw.
    f.accept_new("an accepted rewrite\n");
    f.write_prompt("an accepted rewrite\nand the author typed on\n");
    let proposal = f.propose(PROPOSED);
    let journal = interrupt(&f, &proposal, Phase::Prepared);
    let entry_id = journal.entry_id.clone().expect("an entry id");

    f.app.manage(crate::project::ProjectState::default());
    let project = f.app.state::<crate::project::ProjectState>();
    project.set_root(f.root.path().to_path_buf());

    let listed = list_draft_history(
        f.handle(),
        f.draft_id.clone(),
        f.app.state::<crate::project::ProjectState>(),
    )
    .expect("listed");

    // The interrupted operation was rolled back on the way, so the answer is the
    // reconciled state rather than the one on disk when the call arrived.
    assert_eq!(
        listed.entries.len(),
        2,
        "the two the earlier acceptance settled, and nothing of the rolled-back one",
    );
    assert_eq!(listed.live.path, FILE);
    // The author had typed since the last accepted version, so the live prompt
    // has moved on from it — and the rolled-back acceptance did not change that
    // either way (NAW-FR-37).
    assert!(!listed.live.matches_latest);
    assert!(!manifest_path(&f.hist(), &entry_id).exists());
    assert_eq!(f.prompt(), "an accepted rewrite\nand the author typed on\n");

    // And the entry command answers for a version that does stand.
    let original = listed.entries[0].id.clone();
    let loaded = load_draft_history_entry(
        f.handle(),
        original.clone(),
        f.app.state::<crate::project::ProjectState>(),
    )
    .expect("loaded");
    assert_eq!(loaded.sha256, listed.entries[0].sha256);

    // DHS-FR-25: with no project open, both refuse before reading anything.
    project.clear_root();
    for refusal in [
        list_draft_history(
            f.handle(),
            f.draft_id.clone(),
            f.app.state::<crate::project::ProjectState>(),
        )
        .err(),
        load_draft_history_entry(
            f.handle(),
            original,
            f.app.state::<crate::project::ProjectState>(),
        )
        .err(),
    ] {
        assert_eq!(
            refusal.as_deref(),
            Some("no project open"),
            "a closed project is refused before anything is read",
        );
    }
}

// ---------------------------------------------------------------------------
// The statistics line an exposed entry contributes (DHS-FR-ZQNM)
// ---------------------------------------------------------------------------

/// The draft's statistics log, as the fold reads it.
///
/// The recording path is asynchronous (per `DSS-draft-statistics-storage.md`
/// DSS-FR-TUMX), so the writer is drained first — production never waits for it,
/// which is the whole of that requirement, and a test that wants to read what
/// was written has to.
fn history_statistics(f: &Fixture) -> crate::statistics::DraftStatistics {
    crate::statistics::wait_for_writer();
    crate::statistics::read_statistics_impl(&f.root, &f.draft_id).expect("statistics")
}

/// Give the fixture's app a project state pointing at its root, which is what
/// the statistics recorder resolves its log against.
fn with_project(f: &Fixture) {
    use tauri::Manager;
    f.app.manage(crate::project::ProjectState::default());
    f.app
        .state::<crate::project::ProjectState>()
        .set_root(f.root.path().to_path_buf());
}

#[test]
fn dhs_ts_ukpw_every_exposed_entry_contributes_one_statistics_line() {
    // DHS-FR-07, DHS-FR-16 / DHS-FR-ZQNM.
    let f = Fixture::new();
    with_project(&f);

    // A first byte-changing acceptance settles the `Original` and the version it
    // produced, so it contributes **two** lines.
    let first = f.propose(PROPOSED);
    f.accept(&first).expect("accepted");
    assert_eq!(history_statistics(&f).totals.draft_edits.value, Some(2));

    // Every acceptance after it contributes one.
    let second = f.propose("# Spec\n\nA third opening.\n");
    f.accept(&second).expect("accepted");
    assert_eq!(history_statistics(&f).totals.draft_edits.value, Some(3));

    // DHS-FR-16: an acceptance that changes no prompt byte settles no entry, so
    // it contributes none — and the accepted proposal still counts. Reached by
    // the author rewriting the change into what the prompt already says, which
    // is the one way a change comes to propose nothing: a proposal recorded
    // against an unchanged prompt is refused outright.
    let third = f.propose("# Spec\n\nA fourth opening.\n");
    let held = dcp::load_content_impl(&f.root, &third.id).expect("candidate");
    let standing = f.prompt_body();
    dcp::save_candidate_impl(&f.root, &third.id, &standing, &held.checksum).expect("edited");
    f.accept(&third).expect("accepted");
    assert_eq!(history_statistics(&f).totals.draft_edits.value, Some(3));
}

#[test]
fn dhs_ts_ukpw_a_statistics_line_carries_no_byte_of_a_snapshot() {
    // DHS-FR-07, DHS-FR-16 (second half) / DHS-FR-ZQNM: the line carries the entry's id,
    // its seq, and its source kind, and nothing else.
    let f = Fixture::new();
    with_project(&f);
    let proposal = f.propose(PROPOSED);
    f.accept(&proposal).expect("accepted");
    crate::statistics::wait_for_writer();

    let path = crate::statistics::log_path(&f.root, &f.draft_id).expect("path");
    let text = f.root.read_text(&path).expect("log");
    for forbidden in ["Spec", "opening", "paragraph", "raver119", "arch"] {
        assert!(
            !text.contains(forbidden),
            "a history statistics line must not carry {forbidden:?}",
        );
    }
}

#[test]
fn dhs_ts_bozm_a_rolled_back_acceptance_contributes_no_line() {
    // DSS-FR-PNUE, DSS-FR-TUMX / DHS-FR-ZQNM: entries never exposed contribute nothing.
    let f = Fixture::new();
    with_project(&f);
    let proposal = f.propose(PROPOSED);
    let journal = stage_prepared_for_test(&f.root, &f.draft_id, &proposal, ORIGINAL_TEXT, &human())
        .expect("staged");
    let _ = journal;
    f.reconcile().expect("reconciled");
    crate::statistics::wait_for_writer();

    let stats = crate::statistics::read_statistics_impl(&f.root, &f.draft_id).expect("statistics");
    assert_eq!(
        stats.totals.draft_edits,
        crate::statistics::Measure::unavailable(),
        "a withdrawn entry was never exposed, so nothing was ever appended",
    );
}
