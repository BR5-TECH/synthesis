//! Tests for BM25 indexing (`BMI-bm25-indexing.md`).
//!
//! The bulk run against [`reconcile`] and [`search_snapshot`], which are pure
//! over their inputs, so membership, replacement, removal, ranking, and
//! determinism are all assertable without a Tauri runtime or a filesystem. The
//! handful that need managed state — mounting, teardown, progress, and the
//! draft channel — use `tauri::test::mock_app`.

use super::*;
use tauri::Manager;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// An artifact source the pass will index.

/// A real directory to mount as a distinct content root.
///
/// `mount` needs only the root's identity for the tests below, but a `RootFs`
/// is a *governed* root and there is no such thing as a governed directory that
/// does not exist — so the fabricated `/tmp/a` markers these tests used become
/// real, process-unique directories.
fn scratch_root(name: &str) -> crate::fs::RootFs {
    let p = std::env::temp_dir().join(format!(
        "synthesis-bm25-{}-{}",
        std::process::id(),
        name
    ));
    std::fs::create_dir_all(&p).unwrap();
    crate::fs::RootFs::for_root(&p)
}

fn artifact(index: IndexId, path: &str, text: &str) -> SourceFile {
    SourceFile {
        index,
        file: FileRef::artifact(path),
        text: Ok(text.to_string()),
        plain_text: false,
    }
}

/// A draft source the pass will index.
fn draft(draft_id: &str, path: &str, text: &str) -> SourceFile {
    SourceFile {
        index: IndexId::Drafts,
        file: FileRef::draft(draft_id, path),
        text: Ok(text.to_string()),
        plain_text: false,
    }
}

/// A source the pass must skip (BMI-FR-09).
fn unreadable(index: IndexId, path: &str, reason: &str) -> SourceFile {
    SourceFile {
        index,
        file: FileRef::artifact(path),
        text: Err(reason.to_string()),
        plain_text: false,
    }
}

/// Run a pass, discarding the skip and progress reporting.
fn pass(previous: &IndexSet, sources: Vec<SourceFile>, scope: PassScope) -> IndexSet {
    reconcile(previous, sources, scope, |_, _, _| {}, |_| {}).0
}

/// Run a pass, keeping the stats.
fn pass_with_stats(
    previous: &IndexSet,
    sources: Vec<SourceFile>,
    scope: PassScope,
) -> (IndexSet, PassStats) {
    reconcile(previous, sources, scope, |_, _, _| {}, |_| {})
}

/// The paths a query returns, index-qualified, so a scenario can assert on
/// membership without depending on scores.
fn hit_paths(hits: &[ChunkHit]) -> Vec<(IndexId, String)> {
    let mut out: Vec<(IndexId, String)> = hits
        .iter()
        .map(|h| (h.index, h.path.clone()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    out.sort();
    out
}

const GERMAN: &str = "\
# Verzeichnisse

Dieses Dokument beschreibt, wie das Programm die Verzeichnisse des Projektes \
durchsucht und die gefundenen Dateien nach ihrer Art einordnet. Die Sprache \
wird einmal für die ganze Datei erkannt, und alle Abschnitte werden danach \
behandelt. Das ist wichtig, weil eine einzelne Überschrift viel zu wenig Text \
enthält, um zuverlässig erkannt zu werden.

## Überschrift

Ein kurzer Abschnitt.
";

fn mounted_app() -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(Bm25Indexer::default());
    app.manage(crate::progress::ProgressRegistry::default());
    app.manage(CandidateStore::default());
    app.manage(crate::scanning::AttributionBaseline::default());
    app
}

/// Every file under `root`, with its bytes hashed — so "the pass wrote nothing"
/// can be asserted against the whole tree rather than against one path the
/// fixture never created anyway.
fn tree_fingerprint(root: &std::path::Path) -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                out.push((
                    rel.to_string_lossy().into_owned(),
                    crate::fs::sha256_bytes(&bytes),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// Mount `root`, run one full pass, and hand back the app so the caller can
/// keep querying it. The generation is read into a binding first for the reason
/// `Bm25Indexer::schedule` documents.
fn indexed(root: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    let app = mounted_app();
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(root));
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(root), PassScope::ALL, generation)
        .expect("the pass publishes");
    drop(indexer);
    app
}

mod documents_index;
mod indexing;
mod notes_index;
mod querying;
mod regressions;
mod runtime;
mod skills_index;
