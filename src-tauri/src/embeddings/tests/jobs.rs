//! The jobs: first indexing, the project-open check, incremental passes and
//! their debounce, branch and root changes, model switches, and Reindex.

use super::*;

// EMB-FR-NNOK, EMB-FR-OVPR, EMB-FR-BLHF, EMB-FR-RFNF: turning embeddings on
// downloads the model, then the first indexing runs once the BM25 snapshot is
// published; until it ends vector search is not ready. The store is one file
// in the repository store and nothing else.
#[test]
fn enabling_downloads_the_model_then_indexes_the_project() {
    let h = Harness::new();
    h.enable();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.settle(Instant::now());
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 1);
    assert_eq!(h.service().status().model_state, ModelState::Ready);
    assert_eq!(h.phase(), Some(Phase::Preparing), "the snapshot is not published yet");
    assert!(!h.ready_for_spec());

    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.jobs(), [JobKind::FirstIndexing]);
    assert_eq!(h.phase(), Some(Phase::Ready));
    assert!(h.ready_for_spec());
    assert_eq!(h.embedded(), 3);
    let project = h.service().status().project.unwrap();
    assert_eq!(project.spec_chunks, 3);
    assert_eq!(project.git, "repository");
    assert_eq!(project.branch.as_deref(), Some("main"));
    assert_eq!(project.root_branch.as_deref(), Some("main"));
    assert!(project.root_tip.is_some());
    assert!(h.store_file().exists());
    assert!(!h.repo_dir.path().join(store::STORE_FILE).exists());
}

// EMB-FR-NNOK: a model the cache holds loads with no download.
#[test]
fn a_cached_model_loads_without_a_download() {
    let (factory, log) = FakeFactory::new();
    *factory.cached.lock().unwrap() = true;
    let h = Harness::with_factory(&[("specifications/a.md", SPEC_A)], factory, log);
    h.ready();
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 0);
    assert_eq!(h.service().status().model_state, ModelState::Ready);
}

// EMB-FR-WWMW, EMB-FR-BZZT, EMB-FR-YLBN, EMB-FR-HWRQ, EMB-FR-IRQA: edits
// collapse into one incremental pass after the 2-second window; only the
// changed chunks are embedded, and search stays ready during the window.
#[test]
fn edits_are_debounced_into_one_incremental_pass() {
    let h = Harness::new();
    h.ready();
    let before = h.embedded();
    let jobs = h.jobs().len();

    // Taken before the first touching publish, so it is earlier than any
    // deadline that publish sets, however slow the machine is.
    let earlier = Instant::now();
    h.write("specifications/b.md", "# Search\n\nThe search bar ranks files by meaning.");
    h.pass();
    h.write("specifications/c.md", "# Third\n\nAnother specification.");
    h.pass();
    h.settle(earlier);
    assert_eq!(h.embedded(), before, "nothing runs inside the window");
    assert!(h.ready_for_spec(), "search stays available");

    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::Incremental], "one pass");
    assert_eq!(h.embedded(), before + 2, "the two changed chunks");
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-HWRQ, EMB-FR-VFPV: a publish that touched no specification and no
// document starts no job; a change to a file that is not a specification is
// not embedded.
#[test]
fn a_change_outside_the_specifications_starts_no_job() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.service().index_published(false, h.app.state::<Bm25Indexer>().generation(), Instant::now());
    h.write("src/main.rs", "// worktree teardown, in code");
    h.write("notes/plan.md", "# Plan\n\nworktree teardown notes");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs().len(), jobs);
    assert!(h.texts().iter().all(|t| !t.contains("in code") && !t.contains("notes")));
}

// EMB-FR-VFPV: only specification and document chunks are embedded — skills,
// source files, and other Markdown are not.
#[test]
fn only_specifications_are_embedded() {
    let h = Harness::with_files(&[
        ("specifications/a.md", SPEC_A),
        (
            ".claude/skills/demo/SKILL.md",
            "---\nname: demo\ndescription: skill text\n---\nskill body",
        ),
        ("src/main.rs", "fn main() { /* source text */ }"),
        ("docs/guide.md", "# Guide\n\nguide text"),
    ]);
    h.ready();
    let texts = h.texts();
    assert_eq!(texts.len(), 2);
    assert!(texts.iter().all(|t| t.contains("worktree") || t.contains("branch")));
}

// EMB-FR-OAJQ, EMB-FR-HWRQ: a file that gains the `spec` type enters the
// layer with its chunks, and one that loses it leaves.
#[test]
fn a_type_change_moves_a_file_into_and_out_of_the_layer() {
    let h = Harness::with_files(&[
        ("specifications/a.md", SPEC_A),
        ("notes/assigned.md", "# Assigned\n\nassigned text"),
    ]);
    h.ready();
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 2);

    h.write(
        ".synthesis/library.toml",
        "[assignments.\"notes/assigned.md\"]\ntype = \"spec\"\nscope = \"file\"\n",
    );
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 3);

    h.write(".synthesis/library.toml", "");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 2);
}

// EMB-FR-SWRV, EMB-FR-BZZT: a change published while a job runs is embedded
// by an incremental pass after the job.
#[test]
fn a_change_during_a_running_job_is_embedded_after_it() {
    let h = Harness::new();
    h.enable();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    let app = h.handle().clone();
    let root = h.rooted.clone();
    let path = h.repo_dir.path().join("specifications/c.md");
    h.during_next_embed(move || {
        std::fs::write(&path, "# Third\n\nWritten during the first indexing.").unwrap();
        pass_on(&app, &root);
    });
    h.settle(Instant::now());
    assert_eq!(h.jobs(), [JobKind::FirstIndexing]);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 3, "the job read its snapshot");

    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs(), [JobKind::FirstIndexing, JobKind::Incremental]);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 4);
}

// EMB-FR-SWRV, EMB-FR-HRYV: a Reindex requested while a job runs cancels it
// and runs in its place.
#[test]
fn a_reindex_cancels_a_running_job() {
    let h = Harness::new();
    h.enable();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    let app = h.handle().clone();
    h.during_next_embed(move || {
        app.state::<EmbeddingsService>().request_reindex(ReindexScope::WholeTrace).unwrap();
    });
    h.settle(Instant::now());
    assert_eq!(h.jobs(), [JobKind::FirstIndexing, JobKind::Reindex]);
    assert_eq!(h.phase(), Some(Phase::Ready));
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 3);
    assert!(h.app.state::<crate::progress::ProgressRegistry>().in_flight().is_empty());
}

// EMB-FR-HRYV: a Reindex runs as a job of its own; vector search is not
// ready until it ends.
#[test]
fn reindex_runs_and_returns_to_ready() {
    let h = Harness::new();
    h.ready();
    h.service().request_reindex(ReindexScope::WholeTrace).unwrap();
    assert_eq!(h.phase(), Some(Phase::Preparing));
    assert!(!h.ready_for_spec());
    h.settle(Instant::now());
    assert_eq!(h.embedded(), 6);
    assert_eq!(h.jobs().last(), Some(&JobKind::Reindex));
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-TGCL, EMB-FR-UJLS: a model switch downloads the new model and builds
// the store again with it; the old vectors are not reused.
#[test]
fn a_model_switch_embeds_everything_again() {
    let h = Harness::new();
    h.ready();
    assert_eq!(h.embedded(), 3);
    h.service()
        .apply_settings(EmbeddingsRecord { enabled: true, model: Some("AllMiniLML6V2".into()) });
    assert!(!h.ready_for_spec(), "BM25 only until the switch ends");
    h.settle(Instant::now());
    assert_eq!(h.embedded(), 6);
    assert_eq!(h.jobs().last(), Some(&JobKind::ModelSwitch));
    assert_eq!(h.phase(), Some(Phase::Ready));
    assert_eq!(h.service().status().model, "AllMiniLML6V2");
}

// EMB-FR-UTCB, EMB-FR-UJLS: after a model switch, the store of a project that
// was not open is stale and is built again when it opens.
#[test]
fn a_project_closed_during_a_model_switch_rebuilds_on_open() {
    let h = Harness::new();
    h.ready();
    h.service().unmount();
    h.service()
        .apply_settings(EmbeddingsRecord { enabled: true, model: Some("AllMiniLML6V2".into()) });
    h.settle(Instant::now());
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::StaleRebuild));
    assert_eq!(h.embedded(), 6);
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-JIJH, EMB-FR-UTWM: on project open a store that is not stale is
// kept, and only chunks changed while the application was closed are
// embedded.
#[test]
fn the_project_open_check_embeds_only_what_changed() {
    let h = Harness::new();
    h.ready();
    h.service().unmount();

    h.write("specifications/b.md", "# Search\n\nChanged while closed.");
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.embedded(), 4, "one changed chunk");
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-NHQG, EMB-FR-PYCS, EMB-FR-DCXI: a branch change evaluates the layers
// again by difference only; back on the root branch nothing is embedded, and
// the documents layer is the same layer throughout.
#[test]
fn a_branch_change_embeds_only_its_difference() {
    let h = Harness::new();
    h.ready();
    let documents_layer = |h: &Harness| {
        h.service().lock().project.as_ref().unwrap().outcome.as_ref().unwrap().documents_layer
    };
    let docs = documents_layer(&h);

    let head = h.repo.head().unwrap().peel_to_commit().unwrap();
    h.repo.branch("feature", &head, false).unwrap();
    h.repo.set_head("refs/heads/feature").unwrap();
    h.write("specifications/b.md", "# Search\n\nFeature text.");
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::BranchUpdate));
    assert_eq!(h.embedded(), 4);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));

    h.repo.set_head("refs/heads/main").unwrap();
    h.write("specifications/b.md", SPEC_B);
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.embedded(), 4, "the root vectors serve main again");
    assert_eq!(documents_layer(&h), docs);
}

// EMB-FR-VQSK, EMB-FR-DQVX: a commit on the root branch moves the tip; the
// root check updates the root layer. A check with no move starts no job.
#[test]
fn a_root_tip_check_updates_the_root_layer() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs().len(), jobs, "the tip did not move");

    let old_tip = h.service().status().project.unwrap().root_tip;
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nA committed specification.")]);
    h.pass();
    h.service().request_root_check();
    h.settle(Instant::now());
    let project = h.service().status().project.unwrap();
    assert_ne!(project.root_tip, old_tip);
    assert_eq!(project.spec_chunks, 4);
    assert_eq!(project.state, Phase::Ready);
    assert!(h.jobs()[jobs..].contains(&JobKind::RootUpdate));
}

// EMB-FR-HDFV: a fetch that moves only a remote-tracking ref does not move a
// tip that a local branch gives.
#[test]
fn a_remote_only_move_does_not_move_the_root_tip() {
    let h = Harness::new();
    h.repo.remote("origin", "https://example.invalid/r.git").unwrap();
    h.ready();
    let jobs = h.jobs().len();
    let tip = h.service().status().project.unwrap().root_tip;
    // A commit object the remote-tracking ref points at, reached by no local
    // branch.
    let tree = h.repo.head().unwrap().peel_to_tree().unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let parent = h.repo.head().unwrap().peel_to_commit().unwrap();
    let remote = h.repo.commit(None, &sig, &sig, "remote", &tree, &[&parent]).unwrap();
    h.repo.reference("refs/remotes/origin/main", remote, true, "fetch").unwrap();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs().len(), jobs);
    assert_eq!(h.service().status().project.unwrap().root_tip, tip);
}

// EMB-FR-VVBY, EMB-FR-RFNF: indexing, a Reindex, and a root update change no
// file of the worktree, no ref, and no Git index, and create no `.synthesis/`.
#[test]
fn embedding_jobs_change_nothing_in_the_repository() {
    let h = Harness::new();
    let index_bytes = || std::fs::read(h.repo_dir.path().join(".git/index")).unwrap();
    let before_index = index_bytes();
    let before_head = h.repo.head().unwrap().target();
    h.ready();
    h.service().request_reindex(ReindexScope::WholeTrace).unwrap();
    h.settle(Instant::now());
    let statuses = h.repo.statuses(None).unwrap();
    assert!(statuses.is_empty(), "git status is clean");
    assert_eq!(index_bytes(), before_index);
    assert_eq!(h.repo.head().unwrap().target(), before_head);
    assert!(!h.repo_dir.path().join(".synthesis").exists());
}

// EMB-FR-RMZH: closing the project during a job cancels it and keeps the
// store on disk.
#[test]
fn closing_the_project_cancels_the_job_and_keeps_the_store() {
    let h = Harness::new();
    h.ready();
    h.service().request_reindex(ReindexScope::WholeTrace).unwrap();
    let app = h.handle().clone();
    h.during_next_embed(move || app.state::<EmbeddingsService>().unmount());
    h.settle(Instant::now());
    assert!(h.service().status().project.is_none());
    assert!(h.store_file().exists());
    assert!(h.app.state::<crate::progress::ProgressRegistry>().in_flight().is_empty());
}

// EMB-FR-HDPR, EMB-FR-WJCW: each job registers one `embed` operation with
// its label and terminates it; nothing is left in flight.
#[test]
fn every_job_registers_and_terminates_one_embed_operation() {
    use tauri::Listener;
    let h = Harness::new();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let sink = seen.clone();
    h.app.listen(crate::progress::OPERATION_PROGRESS, move |event| {
        sink.lock().unwrap().push(serde_json::from_str(event.payload()).unwrap());
    });
    h.ready();
    let seen = seen.lock().unwrap();
    let embed: Vec<&serde_json::Value> = seen.iter().filter(|op| op["kind"] == "embed").collect();
    let labels: std::collections::BTreeSet<&str> =
        embed.iter().filter_map(|op| op["label"].as_str()).collect();
    assert!(labels.contains("Downloading embedding model…"), "{labels:?}");
    assert!(labels.contains("Embedding specifications…"), "{labels:?}");
    assert!(embed.iter().any(|op| op["state"] == "finished"));
    assert!(h.app.state::<crate::progress::ProgressRegistry>().in_flight().is_empty());
}

// EMB-FR-ZFGT: every status change is published, and the last event equals
// the status.
#[test]
fn the_status_event_follows_the_status() {
    use tauri::Listener;
    let h = Harness::new();
    let last = Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
    let sink = last.clone();
    h.app.listen(EMBEDDINGS_STATUS_CHANGED, move |event| {
        *sink.lock().unwrap() = Some(serde_json::from_str(event.payload()).unwrap());
    });
    h.ready();
    let status = serde_json::to_value(h.service().status()).unwrap();
    assert_eq!(last.lock().unwrap().clone(), Some(status));
}
