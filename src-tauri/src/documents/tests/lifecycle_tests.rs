//! The collection on the application: open, refresh, event, picker result,
//! persistence, close, and the `documents` index (DCL-FR-QGLH, DCL-FR-LRUP,
//! DCL-FR-PTRP, DCL-FR-VXXI, DCL-FR-XHSJ, BMI-FR-WBKZ, BMI-FR-FGGU,
//! BMI-FR-MWNQ, BMI-FR-NEIW, BMI-FR-25).

use std::sync::Arc;

use tauri::{Listener, Manager};

use super::*;
use crate::bm25_index::{self, Bm25Indexer, IndexId, PassScope};
use crate::documents::lifecycle::{self, DOCUMENTS_CHANGED};
use crate::documents::model::{Availability, PickMode};
use crate::documents::watcher::DocumentsWatcher;
use crate::global_settings::GlobalSettingsStore;

const KEY: &str = "/repo";

struct App {
    app: tauri::App<tauri::test::MockRuntime>,
    _store_dir: TempDir,
    /// The files the user selected from.
    _docs: TempDir,
    root: PathBuf,
}

impl App {
    fn new() -> App {
        let store_dir = TempDir::new().unwrap();
        let docs = TempDir::new().unwrap();
        let app = tauri::test::mock_app();
        app.manage(FsAccessState::default());
        app.manage(GlobalSettingsStore::with_path(store_dir.path().join("synthesis.toml")));
        app.manage(DocumentsCollection::with_extractor(extract_as_text));
        app.manage(DocumentsWatcher::default());
        app.manage(Bm25Indexer::default());
        app.manage(crate::progress::ProgressRegistry::default());
        app.manage(crate::scanning::CandidateStore::default());
        let root = std::fs::canonicalize(docs.path()).unwrap();
        App {
            app,
            _store_dir: store_dir,
            _docs: docs,
            root,
        }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    fn collection(&self) -> tauri::State<'_, DocumentsCollection> {
        self.app.state::<DocumentsCollection>()
    }

    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn folder_source(&self, relative: &str) -> StoredSource {
        StoredSource {
            kind: SourceKind::Folder,
            path: self.root.join(relative).to_string_lossy().into_owned(),
        }
    }

    /// Open the collection as a project open does, and refresh on this thread.
    fn open_with(&self, sources: Vec<StoredSource>) {
        self.app
            .state::<GlobalSettingsStore>()
            .save_document_sources(KEY, sources)
            .unwrap();
        lifecycle::on_content_root_mounted(&self.handle(), KEY);
        lifecycle::run_refresh(&self.handle());
    }

    /// Mount a project root on the indexer and run a pass over the documents.
    fn index(&self, project: &Path) {
        let indexer = self.app.state::<Bm25Indexer>();
        let root = crate::fs::RootFs::for_root(project);
        if indexer.root().as_deref() != Some(project) {
            indexer.mount(&root);
        }
        let generation = indexer.generation();
        indexer
            .run_pass(&self.handle(), &root, PassScope::DOCUMENTS, generation)
            .expect("the pass publishes");
    }

    fn hits(&self, query: &str) -> Vec<bm25_index::ChunkHit> {
        bm25_index::search(&self.app.state::<Bm25Indexer>(), &[IndexId::Documents], query, 10)
    }
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("timed out waiting for {what}");
}

// DCL-FR-QGLH: opening loads the stored sources and refreshes in the background,
// and no command blocks on it.
#[test]
fn the_collection_opens_with_the_project_and_refreshes_in_the_background() {
    let t = App::new();
    t.write("refs/a.md", "alpha");
    t.app
        .state::<GlobalSettingsStore>()
        .save_document_sources(KEY, vec![t.folder_source("refs")])
        .unwrap();
    lifecycle::on_content_root_mounted(&t.handle(), KEY);
    assert!(t.collection().is_open_for(KEY));
    // The snapshot answers at once from memory; the background refresh fills it.
    assert!(t.collection().snapshot().is_ok());
    wait_for("the background refresh", || {
        t.collection().snapshot().map(|s| s.documents.len()) == Ok(1)
    });
    assert_eq!(t.collection().snapshot().unwrap().sources.len(), 1);
}

// DCL-FR-PTRP: `documents-changed` carries the whole snapshot, once per changed
// snapshot, in the order the snapshots arose.
#[test]
fn a_changed_snapshot_is_emitted_with_the_full_snapshot() {
    let t = App::new();
    let a = t.write("refs/a.md", "alpha");
    let events: Arc<Mutex<Vec<DocumentsSnapshot>>> = Arc::default();
    let sink = Arc::clone(&events);
    t.app.listen(DOCUMENTS_CHANGED, move |event| {
        sink.lock().unwrap().push(serde_json::from_str(event.payload()).unwrap());
    });
    t.open_with(vec![t.folder_source("refs")]);
    wait_for("the first event", || !events.lock().unwrap().is_empty());
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(events.lock().unwrap().len(), 1);
    assert_eq!(events.lock().unwrap()[0].documents.len(), 1);

    lifecycle::run_refresh(&t.handle());
    assert_eq!(events.lock().unwrap().len(), 1, "an unchanged snapshot is not announced");

    std::fs::write(&a, "alpha changed").unwrap();
    lifecycle::run_refresh(&t.handle());
    let seen = events.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0].documents[0].revision, seen[1].documents[0].revision);
}

// DCL-FR-XHSJ: a change of active worktree keeps the sources and the snapshot and
// runs one refresh; it does not read the store again.
#[test]
fn a_worktree_change_keeps_the_sources() {
    let t = App::new();
    t.write("refs/a.md", "alpha");
    t.open_with(vec![t.folder_source("refs")]);
    // The store changes behind the collection's back. A worktree change must not
    // pick that up, because it is the same project.
    t.app
        .state::<GlobalSettingsStore>()
        .save_document_sources(KEY, Vec::new())
        .unwrap();
    lifecycle::on_content_root_mounted(&t.handle(), KEY);
    lifecycle::run_refresh(&t.handle());
    assert_eq!(t.collection().stored_sources().len(), 1);
    assert_eq!(t.collection().snapshot().unwrap().documents.len(), 1);

    // Another project key is another project: its own list is loaded.
    lifecycle::on_content_root_mounted(&t.handle(), "/other-repo");
    assert!(t.collection().is_open_for("/other-repo"));
    assert!(t.collection().stored_sources().is_empty());
}

// DCL-FR-VXXI: a cancelled picker adds nothing and is not an error.
#[test]
fn a_cancelled_picker_adds_nothing() {
    let t = App::new();
    t.open_with(Vec::new());
    let result = lifecycle::add_picked_sources(&t.handle(), PickMode::Files, None).unwrap();
    assert!(result.cancelled);
    assert_eq!(result.ignored_count, 0);
    assert!(result.snapshot.sources.is_empty());
    assert!(t
        .app
        .state::<GlobalSettingsStore>()
        .load_document_sources(KEY)
        .unwrap()
        .is_empty());
}

// DCL-FR-VXXI, DCL-FR-FGGU: the Files picker adds each supported file and counts
// the files it ignored for their type.
#[test]
fn picked_files_are_added_and_unsupported_ones_are_counted() {
    let t = App::new();
    t.open_with(Vec::new());
    let a = t.write("pick/a.md", "alpha");
    let b = t.write("pick/b.PDF", "bravo");
    let c = t.write("pick/c.docx", "charlie");
    let d = t.write("pick/d.png", "delta");
    let result = lifecycle::add_picked_sources(
        &t.handle(),
        PickMode::Files,
        Some(vec![a.clone(), b.clone(), c, d]),
    )
    .unwrap();
    assert!(!result.cancelled);
    assert_eq!(result.ignored_count, 2);
    assert_eq!(result.snapshot.sources.len(), 2);
    assert!(result.snapshot.sources.iter().all(|s| s.kind == SourceKind::File));
    assert_eq!(result.snapshot.documents.len(), 2);
    // The new sources are in the store, as selected.
    let stored = t.app.state::<GlobalSettingsStore>().load_document_sources(KEY).unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].path, a.to_string_lossy());
}

// DCL-FR-VXXI: the Folder picker adds one folder, searched recursively.
#[test]
fn a_picked_folder_is_one_source_searched_recursively() {
    let t = App::new();
    t.open_with(Vec::new());
    t.write("folder/top.md", "top");
    t.write("folder/sub/deep.txt", "deep");
    let result = lifecycle::add_picked_sources(
        &t.handle(),
        PickMode::Folder,
        Some(vec![t.root.join("folder")]),
    )
    .unwrap();
    assert_eq!(result.snapshot.sources.len(), 1);
    assert_eq!(result.snapshot.sources[0].kind, SourceKind::Folder);
    assert_eq!(result.snapshot.documents.len(), 2);
}

// DCL-FR-LRUP: a removed source stops being readable at that moment, and the
// snapshot the command returns already reflects it. The file is left alone.
#[test]
fn removing_a_source_closes_its_reach_before_the_command_returns() {
    let t = App::new();
    let a = t.write("refs/a.md", "alpha");
    t.open_with(vec![t.folder_source("refs")]);
    let id = t.collection().snapshot().unwrap().documents[0].id.clone();
    let fs = t.app.state::<FsAccessState>();
    assert!(t.collection().read_document(&fs, &id).is_ok());

    let snapshot =
        lifecycle::remove_source(&t.handle(), &t.folder_source("refs").path).unwrap();
    assert!(snapshot.sources.is_empty());
    assert!(snapshot.documents.is_empty());
    assert_eq!(
        t.collection().read_document(&fs, &id).unwrap_err().code(),
        "unknown_document"
    );
    assert!(fs.documents().is_none());
    assert_eq!(std::fs::read_to_string(a).unwrap(), "alpha");
    assert!(t
        .app
        .state::<GlobalSettingsStore>()
        .load_document_sources(KEY)
        .unwrap()
        .is_empty());
}

// DCL-FR-XHSJ: closing the project stops the watch, discards the snapshot and the
// documents instance, and every command then refuses with `no_project_open`.
#[test]
fn closing_the_project_discards_the_collection() {
    let t = App::new();
    t.write("refs/a.md", "alpha");
    t.open_with(vec![t.folder_source("refs")]);
    assert!(t.app.state::<DocumentsWatcher>().is_armed());
    assert!(t.app.state::<FsAccessState>().documents().is_some());

    lifecycle::on_project_closed(&t.handle());
    assert!(!t.collection().is_open());
    assert_eq!(t.collection().snapshot().unwrap_err().code(), "no_project_open");
    assert!(!t.app.state::<DocumentsWatcher>().is_armed());
    assert!(t.app.state::<FsAccessState>().documents().is_none());
    assert_eq!(
        lifecycle::add_sources(&t.handle(), &[t.folder_source("refs")])
            .unwrap_err()
            .code(),
        "no_project_open"
    );
    // The references are still stored: a close discards memory, not the list.
    assert_eq!(
        t.app.state::<GlobalSettingsStore>().load_document_sources(KEY).unwrap().len(),
        1
    );
}

// BMI-FR-WBKZ, BMI-FR-FGGU: the `documents` index holds the text of each
// available document, a hit carries the document id and no filesystem path, and a
// PDF is indexed by its extracted text.
#[test]
fn the_documents_index_holds_document_text_under_document_ids() {
    let t = App::new();
    t.write("refs/guide.md", "# Alpha section\n\nzebrafish habitat\n\n# Beta section\n\nquokka diet");
    t.write("refs/notes.txt", "# not a heading\n\nplatypus venom");
    t.write("refs/paper.pdf", "narwhal tusk anatomy");
    t.write("refs/scan.pdf", "   ");
    t.open_with(vec![t.folder_source("refs")]);
    let project = TempDir::new().unwrap();
    t.index(&std::fs::canonicalize(project.path()).unwrap());

    let snapshot = t.app.state::<Bm25Indexer>().snapshot();
    assert_eq!(snapshot.file_count(IndexId::Documents), 3, "the scan with no text adds none");
    let docs = t.collection().snapshot().unwrap().documents;
    let id_of = |name: &str| docs.iter().find(|d| d.name == name).unwrap().id.clone();
    for (query, name) in [
        ("zebrafish", "guide.md"),
        ("platypus", "notes.txt"),
        ("narwhal", "paper.pdf"),
    ] {
        let hits = t.hits(query);
        assert_eq!(hits.len(), 1, "{query}");
        assert_eq!(hits[0].document_id.as_deref(), Some(id_of(name).as_str()));
        assert_eq!(hits[0].path, id_of(name), "no filesystem path in the index");
        assert!(!hits[0].text.contains(&*t.root.to_string_lossy()));
    }
    // A Markdown document is cut at its headings; a text document is not.
    assert_eq!(snapshot.chunk_count(IndexId::Documents), 2 + 1 + 1);
}

// BMI-FR-MWNQ: a changed revision replaces a document's chunks whole, and a
// document that left the collection loses its chunks.
#[test]
fn the_index_follows_content_changes_and_removals() {
    let t = App::new();
    let a = t.write("refs/a.txt", "wombat burrow");
    t.write("refs/b.pdf", "echidna spines");
    t.open_with(vec![t.folder_source("refs")]);
    let project = TempDir::new().unwrap();
    let project_root = std::fs::canonicalize(project.path()).unwrap();
    t.index(&project_root);
    assert_eq!(t.hits("wombat").len(), 1);
    assert_eq!(t.hits("echidna").len(), 1);

    std::fs::write(&a, "numbat termites and some extra words").unwrap();
    lifecycle::run_refresh(&t.handle());
    t.index(&project_root);
    assert!(t.hits("wombat").is_empty(), "the old text is gone");
    assert_eq!(t.hits("numbat").len(), 1);

    lifecycle::remove_source(&t.handle(), &t.folder_source("refs").path).unwrap();
    t.index(&project_root);
    assert!(t.hits("numbat").is_empty());
    assert!(t.hits("echidna").is_empty());
    assert_eq!(t.app.state::<Bm25Indexer>().snapshot().file_count(IndexId::Documents), 0);
}

// BMI-FR-NEIW, BMI-FR-25: a new content root keeps the documents index, and a
// close discards it with the others.
#[test]
fn the_documents_index_outlives_a_worktree_change_and_goes_with_a_close() {
    let t = App::new();
    t.write("refs/a.md", "capybara swimming");
    t.open_with(vec![t.folder_source("refs")]);
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    t.index(&std::fs::canonicalize(first.path()).unwrap());
    assert_eq!(t.hits("capybara").len(), 1);

    let indexer = t.app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&std::fs::canonicalize(second.path()).unwrap()));
    assert_eq!(indexer.snapshot().file_count(IndexId::Documents), 1);
    assert_eq!(t.hits("capybara").len(), 1, "kept across the worktree change");

    // The pass that follows rebuilds the index from the same sources.
    t.index(&std::fs::canonicalize(second.path()).unwrap());
    assert_eq!(t.hits("capybara").len(), 1);

    indexer.clear();
    assert!(t.hits("capybara").is_empty());
}

// DCL-FR-QTGN, BMI-FR-WBKZ: an unavailable document contributes no text to the
// index.
#[test]
fn an_unavailable_document_is_not_indexed() {
    let t = App::new();
    t.write("refs/ok.md", "ocelot");
    let bad = t.root.join("refs/bad.md");
    std::fs::write(&bad, [0xff, 0xfe, 0xfd]).unwrap();
    t.open_with(vec![t.folder_source("refs")]);
    let docs = t.collection().snapshot().unwrap().documents;
    assert!(docs.iter().any(|d| d.status == Availability::Unavailable));
    let project = TempDir::new().unwrap();
    t.index(&std::fs::canonicalize(project.path()).unwrap());
    assert_eq!(t.app.state::<Bm25Indexer>().snapshot().file_count(IndexId::Documents), 1);
}
