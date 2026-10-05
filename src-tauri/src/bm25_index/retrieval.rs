//! Retrieval (BMI-FR-10, BMI-FR-11).

use super::{Bm25Indexer, ChunkHit, IndexId, IndexSet};

/// BMI-FR-10 / BMI-FR-11: the `limit` highest-scoring chunks across `indexes`,
/// ordered by descending score.
///
/// Returns a list in every circumstance rather than an error — an empty query,
/// a zero `limit`, an index set that holds nothing, and no project open each
/// yield an empty list. It reads no file and takes no lock a pass holds, so it
/// answers from the snapshot current at the moment of the call: a query issued
/// while a build is still running returns what has been indexed so far.
pub fn search_snapshot(
    snapshot: &IndexSet,
    indexes: &[IndexId],
    query: &str,
    limit: usize,
) -> Vec<ChunkHit> {
    if limit == 0 || query.trim().is_empty() {
        return Vec::new();
    }
    let selected: Vec<IndexId> = if indexes.is_empty() {
        IndexId::ALL.to_vec()
    } else {
        indexes.to_vec()
    };
    let mut hits: Vec<ChunkHit> = Vec::new();
    for ((index, _), engine) in snapshot.shards.iter() {
        if !selected.contains(index) {
            continue;
        }
        for result in engine.search(query, Some(limit)) {
            let key = result.document.id;
            hits.push(ChunkHit {
                index: *index,
                // The ASC node id is the project-relative path (ASC-FR-13); a
                // draft chunk has no scan node and carries its draft id
                // instead, and a skill descriptor has none either — its file
                // may be gitignored and so absent from the scan entirely
                // (DSL-FR-06).
                node_id: index
                    .is_artifact_index()
                    .then(|| key.path.clone()),
                draft_id: key.draft_id.clone(),
                // BMI-FR-28: a note's identity within its index is its id, so
                // the `path` slot of the key carries it and this is where it
                // comes back out under the name a consumer can act on.
                note_id: (*index == IndexId::Notes).then(|| key.path.clone()),
                // BMI-FR-FGGU: likewise a document's id, which is the one thing
                // a consumer resolves it by.
                document_id: (*index == IndexId::Documents).then(|| key.path.clone()),
                path: key.path,
                chunk_ordinal: key.ordinal,
                score: result.score,
                text: result.document.contents,
            });
        }
    }
    // Descending score, then a total tie-break on identity. The engine's own
    // ordering passes through a `HashSet`, so equal scores would otherwise come
    // back in an order that varies between runs of the same query.
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.index.cmp(&b.index))
            .then_with(|| a.draft_id.cmp(&b.draft_id))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.chunk_ordinal.cmp(&b.chunk_ordinal))
    });
    hits.truncate(limit);
    hits
}

/// BMI-FR-10: the `limit` highest-scoring chunks across `indexes`, from the
/// indexes as they currently stand.
///
/// The one operation this module exposes, and deliberately not a
/// `#[tauri::command]` (BMI-FR-01): it is reached by the backend modules that
/// retrieve against the indexes, never over the wire. It has no consumer yet —
/// the AI integrations that will call it are not yet specified.
pub fn search(indexer: &Bm25Indexer, indexes: &[IndexId], query: &str, limit: usize) -> Vec<ChunkHit> {
    search_snapshot(&indexer.snapshot(), indexes, query, limit)
}
