//! The semantic mode (SCC-FR-MKYT, SCC-FR-UYZW, SCC-FR-VXJL, SCC-FR-NRMI,
//! SCC-FR-LXCP, SCC-FR-WEAK, SCC-FR-DBBK) against a real headless app, with
//! the embeddings index off — so the ranking is BM25 alone.

use super::*;
use tauri::{Listener, Manager};

fn semantic_app(root: Option<&Path>) -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    app.manage(CandidateStore::default());
    app.manage(SearchRegistry::default());
    app.manage(ProgressRegistry::default());
    app.manage(crate::bm25_index::Bm25Indexer::default());
    app.manage(crate::scanning::AttributionBaseline::default());
    if let Some(root) = root {
        app.state::<crate::project::ProjectState>().set_root(root.to_path_buf());
        let rooted = crate::fs::RootFs::for_root(root);
        let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.mount(&rooted);
        let generation = indexer.generation();
        indexer.run_pass(app.handle(), &rooted, crate::bm25_index::PassScope::ALL, generation);
    }
    app
}

/// Run one semantic search and return its hits and its end reason.
fn run_semantic(
    app: &tauri::App<tauri::test::MockRuntime>,
    query: &str,
    scope: SearchScope,
) -> (Vec<SearchHit>, String) {
    let (hits_tx, hits_rx) = std::sync::mpsc::channel::<Vec<SearchHit>>();
    let (end_tx, end_rx) = std::sync::mpsc::channel::<String>();
    app.listen(SEARCH_RESULTS, move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        let hits: Vec<SearchHit> = payload["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| SearchHit {
                id: h["id"].as_str().unwrap().to_string(),
                name: h["name"].as_str().unwrap().to_string(),
                path: h["path"].as_str().unwrap().to_string(),
                ordinal: h["ordinal"].as_u64().unwrap(),
                group: match h["group"].as_str().unwrap() {
                    "artifact" => SearchGroup::Artifact,
                    "document" => SearchGroup::Document,
                    "draft" => SearchGroup::Draft,
                    _ => SearchGroup::File,
                },
                match_kind: MatchKind::Content,
                subtype: h["subtype"].as_str().map(|_| ArtifactType::Spec),
                edit_context: h["editContext"].as_str().map(|_| EditContext::Standalone),
                line: h["line"].as_u64().map(|l| l as u32),
                snippet: h["snippet"].as_str().map(str::to_string),
                document_id: None,
                draft_id: None,
            })
            .collect();
        let _ = hits_tx.send(hits);
    });
    app.listen(SEARCH_ENDED, move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap();
        let _ = end_tx.send(payload["reason"].as_str().unwrap_or_default().to_string());
    });
    start_search(
        app.handle().clone(),
        query.into(),
        SearchMode::Semantic,
        scope,
        app.state::<crate::project::ProjectState>(),
        app.state::<SearchRegistry>(),
    )
    .expect("SCC-FR-DBBK: a semantic query is never invalid");
    let reason = end_rx.recv_timeout(Duration::from_secs(10)).expect("a terminal event");
    let hits = hits_rx.try_iter().flatten().collect();
    (hits, reason)
}

// SCC-FR-UYZW, SCC-FR-NRMI, SCC-FR-VXJL, SCC-FR-LXCP: specifications by
// rank, one hit per file with its best chunk as the snippet and no line;
// a source file holding the same words is not a result.
#[test]
fn semantic_mode_returns_ranked_specifications_and_no_other_file() {
    let dir = tempfile::TempDir::new().unwrap();
    write(
        dir.path(),
        "specifications/a.md",
        "# Teardown\n\nworktree teardown teardown removes the checkout.",
    );
    write(
        dir.path(),
        "specifications/b.md",
        "# Other\n\nA worktree is listed.\n\n# More\n\nteardown once.",
    );
    write(dir.path(), "src/main.rs", "// worktree teardown in code");
    let app = semantic_app(Some(dir.path()));
    let (hits, reason) = run_semantic(&app, "worktree teardown", SearchScope::Capped);
    assert_eq!(reason, "completed");
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, ["specifications/a.md", "specifications/b.md"]);
    assert!(hits
        .iter()
        .all(|h| h.group == SearchGroup::Artifact && h.subtype == Some(ArtifactType::Spec)));
    assert!(hits.iter().all(|h| h.line.is_none() && h.snippet.is_some()));
    assert_eq!(hits.iter().map(|h| h.ordinal).collect::<Vec<_>>(), [0, 1]);
}

// SCC-FR-DBBK: there is no `"invalid query"` in semantic mode.
#[test]
fn a_semantic_query_with_regex_syntax_is_not_an_invalid_query() {
    let dir = tempfile::TempDir::new().unwrap();
    write(dir.path(), "specifications/a.md", "# A\n\nalpha");
    let app = semantic_app(Some(dir.path()));
    let (_, reason) = run_semantic(&app, "alpha(", SearchScope::Full);
    assert_eq!(reason, "completed");
}

// SCC-FR-17 holds for the semantic mode too.
#[test]
fn a_semantic_search_with_no_project_open_ends_completed() {
    let app = semantic_app(None);
    let (hits, reason) = run_semantic(&app, "anything", SearchScope::Capped);
    assert!(hits.is_empty());
    assert_eq!(reason, "completed");
}

// SCC-FR-WEAK: `capped` keeps at most the capped number per group, and
// `full` a larger one.
#[test]
fn semantic_groups_are_bounded_per_group() {
    let dir = tempfile::TempDir::new().unwrap();
    for n in 0..14 {
        write(dir.path(), &format!("specifications/s{n:02}.md"), "# Topic\n\nshared needle words");
    }
    let app = semantic_app(Some(dir.path()));
    let (capped, _) = run_semantic(&app, "needle", SearchScope::Capped);
    assert_eq!(capped.len(), super::super::semantic::CAPPED_PER_GROUP);
    let (full, _) = run_semantic(&app, "needle", SearchScope::Full);
    assert_eq!(full.len(), 14);
}

// SCC-FR-VXJL: the snippet is the chunk text bounded to a maximum length.
#[test]
fn a_long_chunk_snippet_is_bounded() {
    let long = "word ".repeat(200);
    let snippet = super::super::semantic::snippet_of(&long);
    assert_eq!(snippet.chars().count(), super::super::semantic::SNIPPET_CHARS + 1);
    assert!(snippet.ends_with('…'));
    assert_eq!(super::super::semantic::snippet_of("  short  "), "short");
}

fn chunk_hit(
    index: crate::bm25_index::IndexId,
    path: &str,
    text: &str,
) -> crate::bm25_index::ChunkHit {
    crate::bm25_index::ChunkHit {
        index,
        path: path.to_string(),
        node_id: None,
        draft_id: None,
        note_id: None,
        document_id: None,
        chunk_ordinal: 0,
        score: 1.0,
        text: text.to_string(),
    }
}

// SCC-FR-UEDP, SCC-FR-VXJL: an available document gives a `document` hit
// with its id, file name, and normalised path, from its best chunk; a
// document that left the collection gives no hit.
#[test]
fn document_hits_resolve_through_the_collection() {
    use crate::documents::tests::Docs;
    let docs = Docs::new();
    docs.write("refs/manual.md", "# Manual\n\nRate limits.");
    docs.add_file("refs/manual.md");
    docs.refresh();
    let path = crate::documents::normalise_path(&docs.path_of("refs/manual.md"));
    let id = crate::documents::document_id(&path);
    let mut best = chunk_hit(crate::bm25_index::IndexId::Documents, &id, "Rate limits.");
    best.document_id = Some(id.clone());
    let mut worse = best.clone();
    worse.text = "a worse chunk".to_string();
    let mut gone = best.clone();
    gone.document_id = Some("doc-00000000000000000000000000000000".to_string());

    let hits =
        super::super::semantic::document_hits(vec![best, worse, gone], Some(&docs.collection), 10);
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.group, SearchGroup::Document);
    assert_eq!((hit.id.as_str(), hit.document_id.as_deref()), (id.as_str(), Some(id.as_str())));
    assert_eq!(hit.name, "manual.md");
    assert_eq!(hit.path, path);
    assert_eq!(hit.snippet.as_deref(), Some("Rate limits."));
    assert_eq!(hit.ordinal, 0);

    std::fs::remove_file(docs.path_of("refs/manual.md")).unwrap();
    docs.refresh();
    let mut again = chunk_hit(crate::bm25_index::IndexId::Documents, &id, "Rate limits.");
    again.document_id = Some(id);
    assert!(
        super::super::semantic::document_hits(vec![again], Some(&docs.collection), 10).is_empty()
    );
}

// SCC-FR-UDOH: a draft hit carries the draft id, its name, and its folder in
// the drafts tree; a draft that no longer exists gives no hit.
#[test]
fn draft_hits_name_the_draft_and_its_folder() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis/drafts/Core")).unwrap();
    let created = crate::drafts::create_draft_impl(&root, Some("Search"), Some("Core")).unwrap();
    let id = created.draft.id.clone();
    let mut hit = chunk_hit(crate::bm25_index::IndexId::Drafts, &created.file, "the prompt");
    hit.draft_id = Some(id.clone());
    let mut gone = hit.clone();
    gone.draft_id = Some("no-such-draft".to_string());

    let hits = super::super::semantic::draft_hits(vec![hit, gone], &root, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].group, SearchGroup::Draft);
    assert_eq!(hits[0].draft_id.as_deref(), Some(id.as_str()));
    assert_eq!(hits[0].name, "Search");
    assert_eq!(hits[0].path, "Core");
}

// SCC-FR-NRMI: a specification chunk whose file the scan does not surface
// now gives no hit.
#[test]
fn a_spec_hit_needs_a_file_the_scan_surfaces() {
    let hit = chunk_hit(crate::bm25_index::IndexId::Spec, "specifications/gone.md", "text");
    assert!(super::super::semantic::spec_hits(vec![hit.clone()], Some(&[]), 10).is_empty());
    assert!(super::super::semantic::spec_hits(vec![hit], None, 10).is_empty());
}

// SCC-FR-DBBK, SCC-FR-LXCP: with the embeddings index managed but not ready,
// the results are the BM25 ranking, and a second run gives the same ordinals.
#[test]
fn a_semantic_search_is_bm25_while_embeddings_are_not_ready() {
    let dir = tempfile::TempDir::new().unwrap();
    write(dir.path(), "specifications/a.md", "# A\n\nneedle needle needle");
    write(dir.path(), "specifications/b.md", "# B\n\nneedle");
    let plain_app = semantic_app(Some(dir.path()));
    let (plain, _) = run_semantic(&plain_app, "needle", SearchScope::Full);

    let app = semantic_app(Some(dir.path()));
    let (factory, _) = crate::embeddings::test_factory();
    app.manage(crate::embeddings::EmbeddingsService::new(factory, false));
    let (with_service, reason) = run_semantic(&app, "needle", SearchScope::Full);
    assert_eq!(reason, "completed");
    let order =
        |hits: &[SearchHit]| hits.iter().map(|h| (h.path.clone(), h.ordinal)).collect::<Vec<_>>();
    assert_eq!(order(&with_service), order(&plain));
    let (again, _) = run_semantic(&app, "needle", SearchScope::Full);
    assert_eq!(order(&again), order(&with_service));
}
