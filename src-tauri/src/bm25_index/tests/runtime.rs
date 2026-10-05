//! Mounted state and end-to-end passes over a real tree.

use super::*;

// ---------------------------------------------------------------------------
// Mounted state: teardown, and the draft channel (BMI-FR-13, BMI-FR-14 / BMI-FR-25 / DRS-FR-28, DRS-FR-20)
// ---------------------------------------------------------------------------

#[test]
fn a_query_against_an_unmounted_indexer_returns_an_empty_list() {
    // BMI-FR-10: no project open is an empty list, not an error.
    let indexer = Bm25Indexer::default();
    assert!(search(&indexer, &[], "anything", 10).is_empty());
    assert_eq!(indexer.root(), None);
}

#[test]
fn mounting_a_root_discards_what_the_previous_root_left() {
    // BMI-FR-25: nothing from the outgoing root is carried over or merged.
    let indexer = Bm25Indexer::default();
    indexer.mount(&scratch_root("a"));
    indexer.publish(pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    ));
    assert!(!search(&indexer, &[], "teardown", 10).is_empty());

    let b = scratch_root("b");
    indexer.mount(&b);
    assert!(
        search(&indexer, &[], "teardown", 10).is_empty(),
        "the new root starts from nothing"
    );
    assert_eq!(indexer.root().unwrap(), b.path());
}

#[test]
fn clearing_discards_every_index_and_stops_scheduling_passes() {
    // BMI-FR-25: a query after the close returns an empty list.
    let indexer = Bm25Indexer::default();
    indexer.mount(&scratch_root("a"));
    indexer.publish(pass(
        &IndexSet::default(),
        vec![artifact(IndexId::Spec, "a.md", "teardown")],
        PassScope::ALL,
    ));

    indexer.clear();
    assert!(search(&indexer, &[], "teardown", 10).is_empty());
    assert_eq!(indexer.root(), None);
}

#[test]
fn requesting_a_pass_with_nothing_mounted_does_nothing() {
    // A pass would have no root to reconcile against, and the request must not
    // spawn a thread that spins on one.
    let app = mounted_app();
    request_pass(&app.handle().clone(), PassScope::ALL);
    assert!(!app.state::<Bm25Indexer>().schedule().scheduled);
}

#[test]
fn a_draft_change_is_recorded_on_the_internal_channel_and_dirties_the_drafts_scope() {
    // DRS-FR-28 / BMI-FR-19. The channel is internal by construction, so what
    // a consumer received is its only observable.
    // Deliberately unmounted: `request_pass` returns before spawning, so this
    // test asserts on the channel alone and leaves no pass thread to wake up
    // 250ms later against a `TempDir` that has already been removed.
    let app = mounted_app();
    let handle = app.handle().clone();

    let change = crate::drafts::DraftChange {
        draft_id: "d1".to_string(),
        paths: vec!["spec.md".to_string()],
    };
    note_draft_change(&handle, &change);

    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(indexer.take_draft_changes(), vec![change.clone()]);
    assert_eq!(
        scope_for_draft_change(&change),
        PassScope::DRAFTS,
        "a draft change dirties the drafts half and only that half"
    );
}

#[test]
fn no_draft_change_ever_dirties_the_artifact_half() {
    // BMI-FR-20: a graduation writes into the project through its own targeted
    // operation and leaves the draft where it is, so this channel carries
    // nothing about the project tree and there is no shape of draft change that
    // widens the scope. The published files arrive on the watcher's report of
    // those paths, like every other project write.
    for paths in [
        Vec::new(),
        vec!["spec.md".to_string()],
        vec!["ui/a.md".to_string(), "ui/b.md".to_string()],
    ] {
        let change = crate::drafts::DraftChange {
            draft_id: "d1".to_string(),
            paths,
        };
        assert_eq!(scope_for_draft_change(&change), PassScope::DRAFTS);
    }
}

// ---------------------------------------------------------------------------
// End to end over a real tree (BMI-FR-12, BMI-FR-13 / BMI-FR-18, BMI-FR-21, BMI-FR-22 / BMI-FR-19 / BMI-FR-26)
// ---------------------------------------------------------------------------

/// A project whose files the scan classifies, so a pass has something real to
/// reconcile against.
fn project_with_artifacts() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("specifications/wtc.md"),
        // Carries the query term the pass tests search for, so a hit here is
        // evidence the file was indexed rather than evidence about its wording.
        "# Worktree context\n\nActivating a worktree runs the teardown of the scan.\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".claude/skills/review.md"),
        "# Review\n\nReview the diff for teardown bugs.\n",
    )
    .unwrap();
    // Carries no artifact type, so it must reach no index (BMI-FR-03).
    std::fs::write(root.join("src/main.rs"), "// teardown happens here\n").unwrap();
    dir
}

#[test]
fn a_pass_over_a_real_tree_indexes_the_typed_files_and_nothing_else() {
    // BMI-FR-03 / BMI-FR-13 / BMI-FR-22 / BMI-FR-26.
    let dir = project_with_artifacts();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));

    // The generation is read into a binding first, deliberately: inlining
    // `indexer.generation()` into the call would keep that guard alive
    // across it, and `run_pass` locks the same non-reentrant mutex.
    let generation = indexer.generation();
    let stats = indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ALL, generation)
        .expect("the pass publishes");
    assert_eq!(stats.files_indexed, 2, "the spec and the skill, not main.rs");

    let snapshot = indexer.snapshot();
    assert_eq!(
        snapshot.paths(IndexId::Spec),
        vec!["specifications/wtc.md".to_string()]
    );
    assert_eq!(
        snapshot.paths(IndexId::Skill),
        vec![".claude/skills/review.md".to_string()]
    );

    let hits = search(&indexer, &[], "teardown", 10);
    assert_eq!(
        hit_paths(&hits),
        vec![
            (IndexId::Skill, ".claude/skills/review.md".to_string()),
            (IndexId::Spec, "specifications/wtc.md".to_string()),
        ],
        "an untyped source file is findable in neither index"
    );

}

#[test]
fn indexing_writes_nothing_anywhere_under_the_project() {
    // BMI-FR-26: no project file, no `.synthesis/` content — `cache/` included
    // — and nothing under `app_data_dir()`. Asserted over the whole tree,
    // including a pre-existing `.synthesis/cache/`, because a check that one
    // path is absent passes for a fixture that never created it.
    let dir = project_with_artifacts();
    let root = crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis/cache")).unwrap();
    std::fs::write(root.join(".synthesis/cache/existing"), b"untouched").unwrap();

    let before = tree_fingerprint(&root);
    assert!(before.len() >= 4, "precondition: the fixture has content");

    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let generation = indexer.generation();
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ALL, generation);
    // And a query, which must be a pure in-memory lookup.
    let _ = search(&indexer, &[], "teardown", 10);

    assert_eq!(
        tree_fingerprint(&root),
        before,
        "a build and a query leave every file under the project byte-identical"
    );
}

#[test]
fn a_pass_terminates_its_progress_operation_and_leaves_nothing_in_flight() {
    // BMI-FR-22 / PRG-FR-09.
    let dir = project_with_artifacts();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let generation = indexer.generation();
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ALL, generation);

    let in_flight = app.state::<crate::progress::ProgressRegistry>().in_flight();
    assert!(
        in_flight.iter().all(|op| op.kind != "index"),
        "no index operation may be left running: {in_flight:?}"
    );
}

#[test]
fn a_pass_whose_root_moved_under_it_publishes_nothing() {
    // BMI-FR-25: a result describing a checkout the application has stopped
    // reading is dropped rather than published.
    let dir = project_with_artifacts();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let stale_generation = indexer.generation();
    // The worktree changed while the pass was in flight.
    indexer.mount(&scratch_root("elsewhere"));

    assert!(
        indexer
            .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ALL, stale_generation)
            .is_none(),
        "the pass abandons its result"
    );
    assert!(search(&indexer, &[], "teardown", 10).is_empty());
}

#[test]
fn an_oversized_file_is_skipped_and_a_readable_one_beside_it_is_not() {
    // BMI-FR-09 over a real file, so the ceiling is exercised through
    // `read_bounded` rather than through a hand-made skip.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::write(
        root.join("specifications/big.md"),
        "x".repeat(MAX_FILE_BYTES as usize + 1),
    )
    .unwrap();
    std::fs::write(root.join("specifications/small.md"), "teardown\n").unwrap();

    assert!(read_bounded(&crate::fs::RootFs::for_root(&root), &root.join("specifications/big.md")).is_err());
    assert_eq!(
        read_bounded(&crate::fs::RootFs::for_root(&root), &root.join("specifications/small.md")).unwrap(),
        "teardown\n"
    );
    // The reason names the shape of what was refused, never the contents.
    let reason = read_bounded(&crate::fs::RootFs::for_root(&root), &root.join("specifications/big.md")).unwrap_err();
    assert!(reason.contains("ceiling"), "{reason}");
    assert!(
        !reason.contains("xxxx"),
        "a skip reason names the shape of what was refused, never the file's \
         own content: {reason}"
    );
}

#[test]
fn a_file_that_is_not_utf8_is_skipped_rather_than_failing_the_pass() {
    // BMI-FR-09.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("binary.md");
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let reason = read_bounded(&crate::fs::RootFs::for_root(dir.path()), &path).unwrap_err();
    assert_eq!(reason, "not valid UTF-8 text");
}

#[test]
fn a_pass_picks_up_a_file_created_after_the_previous_one() {
    // BMI-FR-15 / BMI-FR-18 end to end: a pull, a checkout, and an ordinary
    // creation all reach the index through this same reconciliation.
    let dir = project_with_artifacts();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let generation = indexer.generation();
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert!(search(&indexer, &[], "reconciliation", 10).is_empty());

    std::fs::write(
        root.join("specifications/new.md"),
        "# New\n\nA fresh reconciliation of the tree.\n",
    )
    .unwrap();
    // The scan's candidate list is what a pass reads, and the watcher is what
    // keeps it current; here the invalidation is explicit because no watcher is
    // mounted in this test.
    app.state::<CandidateStore>().invalidate();
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);

    assert_eq!(
        hit_paths(&search(&indexer, &[], "reconciliation", 10)),
        vec![(IndexId::Spec, "specifications/new.md".to_string())]
    );

    // And a deletion takes it back out.
    std::fs::remove_file(root.join("specifications/new.md")).unwrap();
    app.state::<CandidateStore>().invalidate();
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert!(search(&indexer, &[], "reconciliation", 10).is_empty());
}

#[test]
fn a_type_assignment_invalidates_the_candidate_list_so_the_next_pass_moves_the_file() {
    // BMI-FR-17, over the synchronous route: the one `assign_artifact_type`
    // takes, which publishes the reclassification itself rather than waiting for
    // the watcher to carry `.synthesis/library.toml` a debounce window later.
    // Driven with no watcher at all, so what it pins is that the invalidation is
    // load-bearing — without it the pass goes on reading a candidate list that
    // still reports the old type and the file never moves, which is what the
    // middle third asserts.
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/x.md"), "# X\n\nteardown material\n").unwrap();

    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    let candidates = app.state::<CandidateStore>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let generation = indexer.generation();

    // Untyped: the file belongs to no index (BMI-FR-03).
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert!(candidates.is_mounted(), "the pass mounted the candidate list");
    assert!(search(&indexer, &[], "teardown", 10).is_empty());

    crate::scanning::assign(
        &crate::fs::RootFs::for_root(&root),
        "notes/x.md",
        ArtifactType::Scenario,
        crate::scanning::Scope::File,
    )
    .unwrap();

    // The assignment alone changes nothing a pass can see: the mounted list
    // still carries the old classification.
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert!(
        search(&indexer, &[], "teardown", 10).is_empty(),
        "a stale candidate list must not be what makes this pass"
    );

    crate::library::republish_classification(
        &handle,
        &crate::fs::RootFs::for_root(&root),
        &candidates,
    );
    assert!(
        !candidates.is_mounted(),
        "the stale classification is dropped so the next enumeration rebuilds"
    );

    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert_eq!(
        hit_paths(&search(&indexer, &[], "teardown", 10)),
        vec![(IndexId::Scenario, "notes/x.md".to_string())],
        "the file moves to the index its new type names"
    );
}

#[test]
fn an_inconsistent_draft_contributes_nothing_to_the_drafts_index() {
    // BMI-FR-04 / DRS-FR-11: the drafts index holds one file per draft — that
    // draft's prompt. A draft whose `files/` is not the single prompt DRS-FR-11
    // requires has no prompt to index, and choosing one of its files would be
    // choosing one on the author's behalf (DRS-FR-15), so it contributes none.
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let rooted = crate::fs::RootFs::for_root(&root);
    std::fs::create_dir_all(root.join(".synthesis/drafts")).unwrap();

    let good = crate::drafts::create_draft_at_root(&rooted, Some("Good")).unwrap();
    let broken = crate::drafts::create_draft_at_root(&rooted, Some("Broken")).unwrap();

    // Both prompts carry the term, so only the invariant can separate them.
    for created in [&good, &broken] {
        let abs = crate::drafts::draft_file_abs_path(&rooted, &created.draft.id, &created.file)
            .unwrap();
        std::fs::write(&abs, "# P\n\nteardown material\n").unwrap();
    }
    // A second file under `files/` is exactly what DRS-FR-13 forbids anyone to
    // create, so finding one means something outside this application made it.
    let broken_files = crate::drafts::draft_file_abs_path(&rooted, &broken.draft.id, &broken.file)
        .unwrap()
        .parent()
        .unwrap()
        .join("intruder.md");
    std::fs::write(&broken_files, "teardown material\n").unwrap();

    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&rooted);
    let generation = indexer.generation();
    indexer.run_pass(&handle, &rooted, PassScope::DRAFTS, generation);

    let hits = search(&indexer, &[IndexId::Drafts], "teardown", 10);
    assert_eq!(
        hits.iter().map(|h| h.draft_id.clone()).collect::<Vec<_>>(),
        vec![Some(good.draft.id.clone())],
        "only the consistent draft's prompt is indexed"
    );
}

#[test]
fn nothing_of_a_draft_but_its_live_prompt_is_indexed_anywhere() {
    // BMI-FR-04 / DRS-FR-11: the drafts index holds each draft's **current live
    // prompt** and nothing else of what its folder carries. An accepted history
    // entry, a proposal candidate, a comment log with an attachment, and the
    // conversation log are none of them indexed — which is what makes
    // `../tools/DST-draft-search-tool.md`'s promise (DST-FR-04) a property of
    // the index's membership rather than of that tool's restraint.
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let rooted = crate::fs::RootFs::for_root(&root);
    std::fs::create_dir_all(root.join(".synthesis/drafts")).unwrap();

    let created = crate::drafts::create_draft_at_root(&rooted, Some("Live")).unwrap();
    let id = created.draft.id.clone();
    let prompt = crate::drafts::draft_file_abs_path(&rooted, &id, &created.file).unwrap();
    std::fs::write(&prompt, "# P\n\nthe live prompt mentions teardown\n").unwrap();

    // Everything beside the prompt, each carrying a term the prompt does not.
    let draft_dir = crate::drafts::draft_dir(&rooted, &id).unwrap();
    std::fs::write(draft_dir.join("history/e1.snapshot"), "sarcophagus in a snapshot\n").unwrap();
    std::fs::write(draft_dir.join("history/e1.toml"), "id = \"e1\"\n").unwrap();
    std::fs::write(draft_dir.join("proposals/p1.md"), "sarcophagus in a candidate\n").unwrap();
    std::fs::create_dir_all(draft_dir.join("proposals/blobs")).unwrap();
    std::fs::write(
        draft_dir.join("proposals/blobs/abc123.md"),
        "sarcophagus in an attachment\n",
    )
    .unwrap();
    std::fs::write(
        draft_dir.join("conversation.jsonl"),
        "sarcophagus in a conversation\n",
    )
    .unwrap();

    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&rooted);
    let generation = indexer.generation();
    indexer.run_pass(&handle, &rooted, PassScope::ALL, generation);

    // Across **every** index, not the drafts index alone: the claim is that this
    // material is indexed nowhere at all.
    assert!(
        search(&indexer, &[], "sarcophagus", 10).is_empty(),
        "no snapshot, candidate, comment, attachment, or conversation is indexed",
    );
    assert_eq!(
        search(&indexer, &[], "teardown", 10)
            .iter()
            .map(|h| h.draft_id.clone())
            .collect::<Vec<_>>(),
        vec![Some(id)],
        "the live prompt is indexed, once, in the drafts index",
    );
}

#[test]
fn a_folder_scope_assignment_moves_the_files_that_inherit_it() {
    // BMI-FR-17's other clause — "or through a folder-scope assignment that
    // changes what it inherits". The test above covers the `Scope::File` half;
    // this covers the inherited half, which nothing else exercises.
    //
    // Deliberately NOT named for an external arrival: no watcher runs here and
    // the `invalidate()` below stands in for one, exactly as this file's other
    // structural tests do. What this proves is the second half of the chain —
    // GIVEN the invalidation, the files that merely *inherit* the new type move
    // to its index. The first half, that an external write to the attribution
    // file produces that invalidation at all, is pinned in `watcher.rs` by
    // `an_external_attribution_write_invalidates_the_candidate_list`. Neither
    // half means much without the other, so they name each other.
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes/a.md"), "# A\n\nteardown material\n").unwrap();
    std::fs::write(root.join("notes/b.md"), "# B\n\nteardown material\n").unwrap();

    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    let candidates = app.state::<CandidateStore>();
    indexer.mount(&crate::fs::RootFs::for_root(&root));
    let generation = indexer.generation();

    // Neither file carries or infers a type, so neither belongs to any index.
    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert!(
        search(&indexer, &[], "teardown", 10).is_empty(),
        "precondition: unclassified files are indexed nowhere (BMI-FR-03)"
    );

    // The folder gains a type. No file's own path or content changed — only
    // what they inherit (ASC-FR-06).
    crate::scanning::assign(
        &crate::fs::RootFs::for_root(&root),
        "notes",
        ArtifactType::Scenario,
        crate::scanning::Scope::Folder,
    )
    .unwrap();
    candidates.invalidate();

    indexer.run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation);
    assert_eq!(
        hit_paths(&search(&indexer, &[], "teardown", 10)),
        vec![
            (IndexId::Scenario, "notes/a.md".to_string()),
            (IndexId::Scenario, "notes/b.md".to_string()),
        ],
        "every file inheriting the folder's new type enters that type's index"
    );
}

