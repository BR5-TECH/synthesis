//! The notes index.

use super::*;

// ---------------------------------------------------------------------------
// BMI-FR-02, BMI-FR-05 / BMI-FR-25 / BMI-FR-09, BMI-FR-23 — the `notes` index (BMI-FR-28, BMI-FR-29)
// ---------------------------------------------------------------------------

/// A note source the pass will index: one whole document, never chunked.
fn note(note_id: &str, body: &str) -> SourceFile {
    SourceFile {
        index: IndexId::Notes,
        file: FileRef::note(note_id),
        text: Ok(body.to_string()),
        plain_text: false,
    }
}

/// Write a note into `root` through the storage module's own create, which is
/// what decides what the index may hold (NTC-FR-24).
fn seed_note(root: &crate::fs::RootFs, scope: crate::notes::NoteScope, body: &str) -> String {
    crate::notes::create_note_in(
        root,
        scope,
        body.to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes")
    .id
}

fn entity_scope(entity_id: &str) -> crate::notes::NoteScope {
    crate::notes::NoteScope::Entity {
        entity_id: entity_id.to_string(),
        entity_path: entity_id.to_string(),
    }
}

/// Re-run a full pass against the mounted root, standing in for the pass the
/// note channel would have scheduled.
fn repass(app: &tauri::App<tauri::test::MockRuntime>) {
    let handle = app.handle().clone();
    app.state::<CandidateStore>().invalidate();
    let indexer = app.state::<Bm25Indexer>();
    let root = indexer.root().expect("mounted");
    let generation = indexer.generation();
    indexer.run_pass(
        &handle,
        &crate::fs::RootFs::for_root(&root),
        PassScope::ALL,
        generation,
    );
}

fn note_ids(hits: &[ChunkHit]) -> Vec<String> {
    let mut ids: Vec<String> = hits.iter().filter_map(|h| h.note_id.clone()).collect();
    ids.sort();
    ids
}

#[test]
fn a_note_is_one_whole_document_and_every_persisted_note_is_one() {
    // BMI-FR-28 / BMI-FR-02 / BMI-FR-05: project-scoped and
    // entity-scoped notes, a note whose entity has been deleted from disk, and
    // a note carrying a `revision` are each one document on identical terms.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&root_path);
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::write(root.join("specifications/live.md"), "# Live\n\nnothing\n").unwrap();

    let project = seed_note(&root, crate::notes::NoteScope::Project, "the teardown is unhandled");
    let attached = seed_note(&root, entity_scope("specifications/live.md"), "teardown of the panel");
    let orphaned = seed_note(&root, entity_scope("specifications/gone.md"), "teardown never runs");
    let revised = crate::notes::create_note_in(
        &root,
        crate::notes::NoteScope::Project,
        "teardown against a past revision".to_string(),
        None,
        Some("abc1234".to_string()),
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes")
    .id;

    let app = indexed(&root_path);
    let indexer = app.state::<Bm25Indexer>();
    let hits = search(&indexer, &[IndexId::Notes], "teardown", 10);

    let mut expected = vec![
        project.clone(),
        attached.clone(),
        orphaned.clone(),
        revised.clone(),
    ];
    expected.sort();
    assert_eq!(note_ids(&hits), expected, "all four notes are searchable");
    for hit in &hits {
        // BMI-FR-05: never split, so the ordinal is always 0 and the text is the
        // note's whole body.
        assert_eq!(hit.chunk_ordinal, 0);
        assert_eq!(hit.index, IndexId::Notes);
        assert!(hit.note_id.is_some(), "a notes hit names its note");
        assert!(hit.node_id.is_none(), "a note has no scan node (ASC-FR-09)");
        assert!(hit.draft_id.is_none());
        let body = crate::notes::note_record(&root, hit.note_id.as_ref().unwrap())
            .expect("the note resolves")
            .body;
        assert_eq!(hit.text, body, "the document is the note's whole body");
    }

    // BMI-FR-28, BMI-FR-02, BMI-FR-05: no note appears in any other index. Asserted on the hits being
    // ABSENT rather than on their `note_id` being unset — `note_id` is set from
    // the index the hit came from, so `note_id.is_none()` is true by
    // construction for every index but `Notes` and would stay true even if a
    // note's body were fed into another index. `teardown` is seeded into notes
    // and nowhere else in this fixture, so an empty result is the real claim.
    for index in IndexId::ALL {
        if index == IndexId::Notes {
            continue;
        }
        let hits = search(&indexer, &[index], "teardown", 10);
        assert!(
            hits.is_empty(),
            "no note's body reaches the {index:?} index: {:?}",
            hits.iter().map(|h| h.text.clone()).collect::<Vec<_>>(),
        );
    }
    // And the notes index holds exactly the four notes — one document each, no
    // more — so "one whole document per note" is pinned as a count and not only
    // as an ordinal.
    let snapshot = indexer.snapshot();
    assert_eq!(snapshot.file_count(IndexId::Notes), 4);
    assert_eq!(snapshot.chunk_count(IndexId::Notes), 4);
}

#[test]
fn a_notes_scope_path_reminder_and_timestamps_are_indexed_nowhere() {
    // BMI-FR-02, BMI-FR-05 / BMI-FR-28: a query reaches what the author wrote and never
    // where the note was filed or when.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&root_path);
    crate::notes::create_note_in(
        &root,
        entity_scope("specifications/sarcophagus.md"),
        "a body naming nothing else".to_string(),
        Some("2027-03-04T05:06:07Z".to_string()),
        Some("deadbeef".to_string()),
        "2026-09-09T00:00:00Z",
    )
    .expect("the note writes");

    let app = indexed(&root_path);
    let indexer = app.state::<Bm25Indexer>();
    for query in ["sarcophagus", "2027", "deadbeef", "2026"] {
        assert!(
            search(&indexer, &[IndexId::Notes], query, 10).is_empty(),
            "{query:?} is in no index",
        );
    }
    assert_eq!(
        search(&indexer, &[IndexId::Notes], "body", 10).len(),
        1,
        "the body itself is what a query reaches",
    );
}

#[test]
fn a_note_change_is_recorded_on_the_internal_channel_and_dirties_the_notes_scope() {
    // NTC-FR-24 / BMI-FR-29. The channel is internal by construction, so what a
    // consumer received is its only observable. Deliberately unmounted, for the
    // reason the draft-channel test is: `request_pass` returns before spawning.
    let app = mounted_app();
    let handle = app.handle().clone();

    let change = crate::notes::NoteChange {
        note_id: "n1".to_string(),
    };
    note_notes_change(&handle, &change);

    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(indexer.take_note_changes(), vec![change.clone()]);
    // The scope the channel actually chooses, read from the function that
    // chooses it rather than from a restatement of the constant — a change here
    // to `PassScope::ARTIFACTS` would stop notes reaching the index in
    // production, and this is what turns red for it.
    assert_eq!(
        scope_for_note_change(&change),
        PassScope::NOTES,
        "a note change dirties the notes part and only that part",
    );
    assert!(!scope_for_note_change(&change).artifacts);
    assert!(!scope_for_note_change(&change).drafts);
    assert!(scope_for_note_change(&change).notes);
}

#[test]
fn a_note_change_reaching_a_mounted_indexer_leaves_the_pending_scope_at_notes() {
    // BMI-FR-29 end to end: the scope the channel chose is what the scheduler
    // accumulated, observed on a *mounted* indexer — `request_pass` returns
    // before touching `pending` when nothing is mounted, so the test above
    // could not have seen this.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let app = mounted_app();
    let handle = app.handle().clone();
    {
        let indexer = app.state::<Bm25Indexer>();
        indexer.mount(&crate::fs::RootFs::for_root(&root_path));
        // `mount` seeds `pending` with ALL, which would mask what the channel
        // adds. Taking it leaves the scheduler at rest.
        let mut schedule = indexer.schedule();
        schedule.pending = PassScope::default();
    }

    note_notes_change(&handle, &crate::notes::NoteChange::to("n1"));

    let indexer = app.state::<Bm25Indexer>();
    let pending = indexer.schedule().pending;
    assert_eq!(
        pending,
        PassScope::NOTES,
        "the notes part alone was dirtied: {pending:?}",
    );
    // `request_pass` spawned a thread that wakes in 250ms. Unmounting now is
    // what makes it return instead of running a pass against a `TempDir` this
    // test is about to remove.
    indexer.clear();
}

#[test]
fn a_pass_covering_one_part_leaves_the_other_two_alone() {
    // BMI-FR-29: a note pass must not evict the artifact and draft indexes, and
    // an artifact pass must not evict the notes index.
    let both = pass(
        &IndexSet::default(),
        vec![
            artifact(IndexId::Spec, "a.md", "alpha"),
            draft("d1", "prompt.md", "bravo"),
            note("n1", "charlie"),
        ],
        PassScope::ALL,
    );
    assert_eq!(both.file_count(IndexId::Notes), 1);

    let artifacts_only = pass(&both, vec![artifact(IndexId::Spec, "a.md", "alpha")], PassScope::ARTIFACTS);
    assert_eq!(
        artifacts_only.file_count(IndexId::Notes),
        1,
        "an artifact pass carries the notes index over untouched",
    );
    assert_eq!(artifacts_only.file_count(IndexId::Drafts), 1);

    let notes_only = pass(&both, vec![note("n1", "charlie")], PassScope::NOTES);
    assert_eq!(notes_only.file_count(IndexId::Spec), 1);
    assert_eq!(notes_only.file_count(IndexId::Drafts), 1);
    assert_eq!(notes_only.file_count(IndexId::Notes), 1);

    // And a note the notes pass did not see leaves the index.
    let emptied = pass(&both, vec![], PassScope::NOTES);
    assert_eq!(emptied.file_count(IndexId::Notes), 0);
    assert_eq!(emptied.file_count(IndexId::Spec), 1);
}

#[test]
fn a_note_body_rewrite_replaces_its_document_whole_and_a_deletion_removes_it() {
    // BMI-FR-25 / BMI-FR-29.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&root_path);
    let rewritten = seed_note(&root, crate::notes::NoteScope::Project, "alpha original");
    let kept = seed_note(&root, crate::notes::NoteScope::Project, "bravo standing");
    let removed = seed_note(&root, crate::notes::NoteScope::Project, "charlie doomed");

    let app = indexed(&root_path);
    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(search(&indexer, &[IndexId::Notes], "alpha", 10).len(), 1);
    drop(indexer);

    crate::notes::update_note_in(
        &root,
        &rewritten,
        crate::notes::NoteFields {
            body: Some("delta replacement".to_string()),
            ..Default::default()
        },
        "2026-02-02T00:00:00Z",
    )
    .expect("the rewrite lands");
    let created = seed_note(&root, crate::notes::NoteScope::Project, "echo arrival");
    crate::notes::delete_note_in(&root, &root, &removed).expect("the deletion lands");
    repass(&app);

    let indexer = app.state::<Bm25Indexer>();
    assert!(
        search(&indexer, &[IndexId::Notes], "original", 10).is_empty(),
        "the text the rewrite replaced stops being matchable",
    );
    assert_eq!(
        note_ids(&search(&indexer, &[IndexId::Notes], "delta", 10)),
        vec![rewritten.clone()],
        "the document was replaced whole rather than amended",
    );
    assert_eq!(
        note_ids(&search(&indexer, &[IndexId::Notes], "echo", 10)),
        vec![created],
        "a note created gains a document",
    );
    assert!(
        search(&indexer, &[IndexId::Notes], "charlie", 10).is_empty(),
        "a note deleted loses its document",
    );
    assert_eq!(
        note_ids(&search(&indexer, &[IndexId::Notes], "bravo", 10)),
        vec![kept],
        "an untouched note is found exactly as before",
    );
}

#[test]
fn a_change_that_leaves_the_body_alone_costs_the_notes_index_nothing() {
    // BMI-FR-29, BMI-FR-25: moving a note between scopes, following it through a rename,
    // and setting or clearing its reminder each touch nothing that is indexed.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&root_path);
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::write(root.join("specifications/old.md"), "# Old\n").unwrap();
    let id = seed_note(&root, entity_scope("specifications/old.md"), "foxtrot standing");

    let app = indexed(&root_path);
    let before = {
        let indexer = app.state::<Bm25Indexer>();
        search(&indexer, &[IndexId::Notes], "foxtrot", 10)
    };
    assert_eq!(note_ids(&before), vec![id.clone()]);

    crate::notes::update_note_in(
        &root,
        &id,
        crate::notes::NoteFields {
            scope: Some(crate::notes::NoteScope::Project),
            reminder: Some(Some("2027-01-01T00:00:00Z".to_string())),
            ..Default::default()
        },
        "2026-03-03T00:00:00Z",
    )
    .expect("the move lands");
    std::fs::rename(
        root.join("specifications/old.md"),
        root.join("specifications/new.md"),
    )
    .unwrap();
    crate::notes::follow_rename(&root, "specifications/old.md", "specifications/new.md");
    crate::notes::update_note_in(
        &root,
        &id,
        crate::notes::NoteFields {
            reminder: Some(None),
            ..Default::default()
        },
        "2026-03-04T00:00:00Z",
    )
    .expect("the reminder clears");
    repass(&app);

    let indexer = app.state::<Bm25Indexer>();
    let after = search(&indexer, &[IndexId::Notes], "foxtrot", 10);
    assert_eq!(note_ids(&after), vec![id], "the note is still found");
    assert_eq!(
        after[0].text, before[0].text,
        "and its document is what it was",
    );
}

#[test]
fn changing_the_active_worktree_discards_the_notes_index_with_the_others() {
    // BMI-FR-29 / BMI-FR-25: the notes of the outgoing worktree are unreachable
    // by any query the moment the new root is mounted.
    let a = tempfile::TempDir::new().unwrap();
    let a_path = crate::changes::canonicalize_lenient(a.path());
    seed_note(
        &crate::fs::RootFs::for_root(&a_path),
        crate::notes::NoteScope::Project,
        "golf belongs to A",
    );
    let b = tempfile::TempDir::new().unwrap();
    let b_path = crate::changes::canonicalize_lenient(b.path());
    seed_note(
        &crate::fs::RootFs::for_root(&b_path),
        crate::notes::NoteScope::Project,
        "hotel belongs to B",
    );

    let app = indexed(&a_path);
    {
        let indexer = app.state::<Bm25Indexer>();
        assert_eq!(search(&indexer, &[IndexId::Notes], "golf", 10).len(), 1);
    }

    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&b_path));
    assert!(
        search(&indexer, &[IndexId::Notes], "golf", 10).is_empty(),
        "the outgoing root's notes go the moment the new root is mounted",
    );
    let generation = indexer.generation();
    indexer.run_pass(
        &handle,
        &crate::fs::RootFs::for_root(&b_path),
        PassScope::ALL,
        generation,
    );
    assert!(search(&indexer, &[IndexId::Notes], "golf", 10).is_empty());
    assert_eq!(search(&indexer, &[IndexId::Notes], "hotel", 10).len(), 1);
}

#[test]
fn an_oversized_or_unreadable_note_contributes_no_document_and_is_warned_about() {
    // BMI-FR-28 / BMI-FR-09 / BMI-FR-23: neither is truncated into a
    // document, the pass completes, and the well-formed note beside them is
    // still returned.
    let dir = tempfile::TempDir::new().unwrap();
    let root_path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&root_path);
    let good = seed_note(&root, crate::notes::NoteScope::Project, "india is well formed");

    // A stored body of 2 KiB — larger than the store would ever accept
    // (NTC-FR-23), and reachable by a hand edit or a merge all the same.
    //
    // The ids carry a token nothing else in the process writes: `logging::BUFFER`
    // is process-global and `cargo test` runs in parallel, so a scan that filtered
    // on `oversized` alone would be flaky by construction.
    const OVERSIZED: &str = "zz-bmi32-oversized";
    const BROKEN: &str = "zz-bmi32-broken";
    const MISLABELLED: &str = "zz-bmi32-mislabelled";
    const UNKNOWN_SCOPE: &str = "zz-bmi32-unknown-scope";
    let oversized = format!(
        "id = \"{OVERSIZED}\"\nscope = \"project\"\nbody = \"{}\"\ncreated_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n",
        "india ".repeat(350),
    );
    std::fs::write(root.join(format!(".synthesis/notes/{OVERSIZED}.toml")), oversized).unwrap();
    std::fs::write(
        root.join(format!(".synthesis/notes/{BROKEN}.toml")),
        "this is not = = toml",
    )
    .unwrap();
    // NTC-FR-14's other two shapes, which reach this index on the same terms:
    // a file whose stem disagrees with the id inside it, and one whose scope is
    // no scope this application serves.
    std::fs::write(
        root.join(format!(".synthesis/notes/{MISLABELLED}.toml")),
        "id = \"somebody-else\"\nscope = \"project\"\nbody = \"india mislabelled\"\n\
         created_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join(format!(".synthesis/notes/{UNKNOWN_SCOPE}.toml")),
        format!(
            "id = \"{UNKNOWN_SCOPE}\"\nscope = \"elsewhere\"\nbody = \"india unknown scope\"\n\
             created_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n"
        ),
    )
    .unwrap();

    let app = indexed(&root_path);
    let indexer = app.state::<Bm25Indexer>();

    let hits = search(&indexer, &[IndexId::Notes], "india", 10);
    assert_eq!(
        note_ids(&hits),
        vec![good],
        "only the well-formed note contributes a document",
    );
    assert_eq!(
        hits[0].text, "india is well formed",
        "and it is that note's whole body — nothing was truncated into a document",
    );

    let records = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Warn,
                ..crate::logging::LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the filter compiles")
        .records;
    let skipped: Vec<&crate::logging::LogRecord> = records
        .iter()
        .filter(|r| r.message == "index skipped a file")
        .filter(|r| r.fields.get("index").and_then(|v| v.as_str()) == Some("notes"))
        .filter(|r| {
            r.fields
                .get("note")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.starts_with("zz-bmi32-"))
        })
        .collect();
    let reasons: std::collections::BTreeMap<String, String> = skipped
        .iter()
        .filter_map(|r| {
            Some((
                r.fields.get("note")?.as_str()?.to_string(),
                r.fields.get("reason")?.as_str()?.to_string(),
            ))
        })
        .collect();
    // A WARN names each note the pass could not take up, **and says which
    // failure it was** — a note refused for its size reported as unreadable
    // would send an author looking for a corrupt file they do not have.
    assert!(
        reasons
            .get(OVERSIZED)
            .is_some_and(|r| r.contains(&crate::notes::MAX_NOTE_BODY_BYTES.to_string())),
        "the oversized note is named with the bound it broke: {reasons:?}",
    );
    for id in [BROKEN, MISLABELLED, UNKNOWN_SCOPE] {
        assert!(
            reasons.contains_key(id),
            "{id} is named as a note the pass could not take up: {reasons:?}",
        );
    }
    assert_eq!(
        reasons.len(),
        4,
        "each of the four is reported exactly once: {reasons:?}",
    );
    // Nothing downstream redacts anything, so the emit site is the only place
    // this can be enforced: no word of a skipped note's text is in a record.
    for record in skipped {
        let rendered = serde_json::to_string(&record.fields).unwrap();
        assert!(
            !rendered.contains("india"),
            "no part of a note's text reaches a log record: {rendered}",
        );
    }
}
