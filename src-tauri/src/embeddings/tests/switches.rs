//! The two switches, the commands that set them, and what they stop.

use super::*;

// EMB-FR-VXIS: with the Global switch off nothing is downloaded, loaded, or
// embedded, and search is BM25-only.
#[test]
fn a_fresh_install_downloads_and_embeds_nothing() {
    let h = Harness::new();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 0);
    assert_eq!(h.embedded(), 0);
    assert_eq!(h.phase(), Some(Phase::Off));
    assert!(!h.ready_for_spec());
    assert!(!h.store_file().exists(), "no store is opened");
    let status = h.service().status();
    assert!(!status.enabled);
    assert_eq!(status.model_state, ModelState::NotDownloaded);
}

// EMB-FR-WQGF, EMB-FR-CTXC: On does nothing under a Global off; turning the
// Global switch on starts the project-open check; Off stops embeddings and
// On again queues the check.
#[test]
fn the_project_switch_follows_the_global_master_switch() {
    let h = Harness::new();
    h.mount(ProjectEmbeddingsMode::On);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.log.downloads.load(Ordering::SeqCst), 0, "On does nothing while Global is off");
    assert_eq!(h.phase(), Some(Phase::Off));

    h.enable();
    h.settle(Instant::now());
    assert_eq!(h.phase(), Some(Phase::Ready));

    h.service().apply_mode(ProjectEmbeddingsMode::Off);
    assert_eq!(h.phase(), Some(Phase::Off));
    assert!(!h.ready_for_spec());
    assert_eq!(
        h.service().request_reindex(ReindexScope::CurrentBranch).unwrap_err(),
        ERR_EMBEDDINGS_NOT_WORKING
    );

    h.service().apply_mode(ProjectEmbeddingsMode::On);
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::OpenCheck));
    assert_eq!(h.phase(), Some(Phase::Ready));
}

// EMB-FR-UUML: while embeddings do not work for the project, no job runs, the
// store stays on disk unchanged, and search is exactly the BM25 search.
#[test]
fn a_project_set_off_keeps_its_store_and_searches_bm25_only() {
    let h = Harness::new();
    h.ready();
    h.service().apply_mode(ProjectEmbeddingsMode::Off);
    let before = std::fs::read(h.store_file()).unwrap();
    let jobs = h.jobs().len();

    h.write("specifications/c.md", "# Third\n\nWritten while off.");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs().len(), jobs, "no embedding job runs");
    assert_eq!(std::fs::read(h.store_file()).unwrap(), before, "the store is unchanged");

    let indexer = h.app.state::<Bm25Indexer>();
    let index = crate::bm25_index::IndexId::Spec;
    assert_eq!(
        search_open_project(h.handle(), &indexer, index, "worktree", 5),
        crate::bm25_index::search(&indexer, &[index], "worktree", 5)
    );
}

// EMB-FR-JFOV: turning the Global switch off stops embeddings at once.
#[test]
fn turning_the_global_switch_off_makes_search_bm25_only() {
    let h = Harness::new();
    h.ready();
    assert!(h.ready_for_spec());
    h.service().apply_settings(EmbeddingsRecord { enabled: false, model: None });
    assert!(!h.ready_for_spec());
    assert_eq!(h.phase(), Some(Phase::Off));
    assert_eq!(h.service().status().model_state, ModelState::NotDownloaded);
}

// EMB-FR-JFOV, EMB-FR-DWJB: the save command persists the settings in the
// user-global store; an unknown model is refused and nothing is stored.
#[test]
fn the_save_command_persists_settings_and_refuses_an_unknown_model() {
    let h = Harness::new();
    let saved = commands::save_settings(
        h.handle(),
        EmbeddingsSettings { enabled: true, model: "BGEM3".to_string() },
    )
    .unwrap();
    assert_eq!(saved, EmbeddingsSettings { enabled: true, model: "BGEM3".to_string() });
    let store = h.app.state::<crate::global_settings::GlobalSettingsStore>();
    assert_eq!(store.load_embeddings_config().unwrap().model.as_deref(), Some("BGEM3"));
    assert!(h.service().settings().enabled);

    let refused = commands::save_settings(
        h.handle(),
        EmbeddingsSettings { enabled: false, model: "no-such-model".to_string() },
    );
    assert_eq!(refused.unwrap_err(), ERR_UNKNOWN_MODEL);
    assert_eq!(store.load_embeddings_config().unwrap().model.as_deref(), Some("BGEM3"));
    assert!(store.load_embeddings_config().unwrap().enabled);
}

// EMB-FR-CTXC, EMB-FR-QNWF: with no project open the mode commands and a
// Reindex are refused with `no_project_open`.
#[test]
fn the_project_commands_need_an_open_project() {
    let h = Harness::new();
    assert_eq!(
        commands::load_project_embeddings_mode(
            h.app.state::<crate::project::ProjectState>(),
            h.app.state::<crate::global_settings::GlobalSettingsStore>(),
        )
        .unwrap_err(),
        ERR_NO_PROJECT_OPEN
    );
    assert_eq!(
        commands::save_project_embeddings_mode(h.handle().clone(), ProjectEmbeddingsMode::Off)
            .unwrap_err(),
        ERR_NO_PROJECT_OPEN
    );
    assert_eq!(
        h.service().request_reindex(ReindexScope::WholeTrace).unwrap_err(),
        ERR_NO_PROJECT_OPEN
    );
}
