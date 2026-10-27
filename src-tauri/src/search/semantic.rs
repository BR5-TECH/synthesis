//! The semantic mode of the search bar (SCC-FR-UYZW through SCC-FR-DBBK).
//!
//! It reads no file and walks no candidate list. Specifications and documents
//! are ranked by `hybrid_search` of `EMB-embeddings-index.md`, drafts by the
//! `drafts` BM25 index, and each group keeps the best chunk of each file,
//! document, or draft.

use std::collections::HashSet;

use tauri::Manager;

use super::{
    EditContext, EndReason, MatchKind, SearchEndedPayload, SearchGroup, SearchHit,
    SearchResultsPayload, SearchScope, SearchSink, Stop,
};
use crate::bm25_index::{self, Bm25Indexer, ChunkHit, IndexId};
use crate::scanning::{ArtifactType, CandidateStore};

/// SCC-FR-WEAK: hits per group of an overlay search.
pub const CAPPED_PER_GROUP: usize = 10;
/// SCC-FR-WEAK: hits per group of a Search results tab.
pub const FULL_PER_GROUP: usize = 50;
/// How many chunks are asked for per hit wanted, so that a few long files do
/// not fill a group with one file's chunks.
const OVERFETCH_FACTOR: usize = 5;
/// SCC-FR-VXJL: the longest snippet, in characters.
pub const SNIPPET_CHARS: usize = 280;

/// SCC-FR-UYZW: run one semantic search and emit its hits and its end.
pub(super) fn run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    search_id: &str,
    root: Option<crate::fs::RootFs>,
    query: &str,
    scope: SearchScope,
    stop: &Stop,
) -> EndReason {
    let end = |reason: EndReason| {
        app.ended(&SearchEndedPayload { search_id: search_id.to_string(), reason });
        reason
    };
    // SCC-FR-17: no project open — an id, no hits, and an immediate end.
    let (Some(root), Some(indexer)) = (root, app.try_state::<Bm25Indexer>()) else {
        return end(EndReason::Completed);
    };
    let per_group = match scope {
        SearchScope::Capped => CAPPED_PER_GROUP,
        SearchScope::Full => FULL_PER_GROUP,
    };
    let wanted = per_group.saturating_mul(OVERFETCH_FACTOR);

    let mut hits = Vec::new();
    let specs = crate::embeddings::search_open_project(app, &indexer, IndexId::Spec, query, wanted);
    let candidates = app.try_state::<CandidateStore>().map(|store| store.candidates(&root));
    hits.extend(spec_hits(specs, candidates.as_deref().map(Vec::as_slice), per_group));
    if let Some(reason) = stop.reason() {
        return end(reason);
    }

    let documents =
        crate::embeddings::search_open_project(app, &indexer, IndexId::Documents, query, wanted);
    let collection = app.try_state::<crate::documents::DocumentsCollection>();
    hits.extend(document_hits(documents, collection.as_deref(), per_group));

    let drafts = bm25_index::search(&indexer, &[IndexId::Drafts], query, wanted);
    hits.extend(draft_hits(drafts, &root, per_group));
    if let Some(reason) = stop.reason() {
        return end(reason);
    }
    if !hits.is_empty() {
        app.results(&SearchResultsPayload { search_id: search_id.to_string(), hits });
    }
    end(EndReason::Completed)
}

/// SCC-FR-VXJL: the chunk text, cut at a character boundary.
pub fn snippet_of(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= SNIPPET_CHARS {
        return trimmed.to_string();
    }
    let mut cut: String = trimmed.chars().take(SNIPPET_CHARS).collect();
    cut.push('…');
    cut
}

/// The best chunk of each file, document, or draft, in rank order.
fn best_per_key(
    hits: Vec<ChunkHit>,
    key: impl Fn(&ChunkHit) -> Option<String>,
) -> Vec<(String, ChunkHit)> {
    let mut seen = HashSet::new();
    hits.into_iter()
        .filter_map(|hit| key(&hit).map(|k| (k, hit)))
        .filter(|(k, _)| seen.insert(k.clone()))
        .collect()
}

/// SCC-FR-NRMI: specification hits. A chunk whose file the scan does not
/// surface now gives no hit.
pub fn spec_hits(
    hits: Vec<ChunkHit>,
    candidates: Option<&[crate::scanning::Candidate]>,
    limit: usize,
) -> Vec<SearchHit> {
    best_per_key(hits, |hit| Some(hit.path.clone()))
        .into_iter()
        .filter_map(|(path, hit)| {
            let candidate = candidates?.iter().find(|c| c.path == path)?;
            Some(SearchHit {
                id: candidate.id.clone(),
                name: candidate.name.clone(),
                path: candidate.path.clone(),
                ordinal: 0,
                group: SearchGroup::Artifact,
                match_kind: MatchKind::Content,
                subtype: Some(ArtifactType::Spec),
                edit_context: Some(EditContext::Standalone),
                line: None,
                snippet: Some(snippet_of(&hit.text)),
                document_id: None,
                draft_id: None,
            })
        })
        .take(limit)
        .enumerate()
        .map(|(rank, hit)| SearchHit { ordinal: rank as u64, ..hit })
        .collect()
}

/// SCC-FR-UEDP: document hits, resolved through the collection at the moment
/// of the search. A document that left it or is not available gives no hit.
pub fn document_hits(
    hits: Vec<ChunkHit>,
    collection: Option<&crate::documents::DocumentsCollection>,
    limit: usize,
) -> Vec<SearchHit> {
    let Some(collection) = collection else {
        return Vec::new();
    };
    best_per_key(hits, |hit| hit.document_id.clone())
        .into_iter()
        .filter_map(|(id, hit)| {
            let entry = collection.resolve_document(&id).ok()?;
            if entry.status != crate::documents::Availability::Available {
                return None;
            }
            Some(SearchHit {
                id: entry.id.clone(),
                name: entry.name.clone(),
                path: entry.path.clone(),
                ordinal: 0,
                group: SearchGroup::Document,
                match_kind: MatchKind::Content,
                subtype: None,
                edit_context: None,
                line: None,
                snippet: Some(snippet_of(&hit.text)),
                document_id: Some(entry.id),
                draft_id: None,
            })
        })
        .take(limit)
        .enumerate()
        .map(|(rank, hit)| SearchHit { ordinal: rank as u64, ..hit })
        .collect()
}

/// SCC-FR-UDOH: draft hits. A draft that no longer exists gives no hit.
pub fn draft_hits(hits: Vec<ChunkHit>, root: &crate::fs::RootFs, limit: usize) -> Vec<SearchHit> {
    let drafts_root = root.path().join(crate::storage_floor::DRAFTS_REL);
    best_per_key(hits, |hit| hit.draft_id.clone())
        .into_iter()
        .filter_map(|(id, hit)| {
            let record = crate::drafts::draft_record(root, &id).ok()?;
            let dir = crate::drafts::draft_dir(root, &id).ok()?;
            let folder = dir
                .strip_prefix(&drafts_root)
                .ok()
                .and_then(|rel| rel.parent())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            Some(SearchHit {
                id: record.id.clone(),
                name: record.name,
                path: folder,
                ordinal: 0,
                group: SearchGroup::Draft,
                match_kind: MatchKind::Content,
                subtype: None,
                edit_context: None,
                line: None,
                snippet: Some(snippet_of(&hit.text)),
                document_id: None,
                draft_id: Some(record.id),
            })
        })
        .take(limit)
        .enumerate()
        .map(|(rank, hit)| SearchHit { ordinal: rank as u64, ..hit })
        .collect()
}
