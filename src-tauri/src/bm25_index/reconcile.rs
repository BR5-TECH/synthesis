//! Reconciliation of the index set against its sources (BMI-FR-13 to BMI-FR-21, BMI-FR-24, BMI-FR-27).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bm25::{Document, Language, SearchEngineBuilder};

use super::chunking::{chunk_text, detect_file_language, language_key};
use super::{documents, ChunkKey, FileEntry, FileRef, IndexId, IndexSet, PassScope, PassStats, SourceFile};

/// Reconcile `previous` against the files that ought to be indexed, returning
/// the next snapshot.
///
/// Pure over its inputs — no filesystem, no Tauri runtime — so every claim
/// about membership, replacement, removal, and determinism is testable without
/// either. `sources` carries only the indexes `scope` covers; anything outside
/// it is carried over from `previous` untouched.
pub fn reconcile(
    previous: &IndexSet,
    sources: Vec<SourceFile>,
    scope: PassScope,
    mut on_skip: impl FnMut(&IndexId, &FileRef, &str),
    mut on_progress: impl FnMut(u64),
) -> (IndexSet, PassStats) {
    let mut next = previous.clone();
    let mut stats = PassStats::default();
    // The document sets of the shards this pass touches, seeded from the
    // shards already mounted. A touched shard is rebuilt from its whole set
    // rather than amended in place, which is what keeps `avgdl` fitted to what
    // the shard actually holds — and that in turn is what makes an
    // incrementally-maintained index score identically to a freshly-built one
    // (BMI-FR-27). An untouched shard is carried over by its `Arc` and costs
    // this pass nothing.
    let mut working: BTreeMap<(IndexId, &'static str), WorkingShard> = BTreeMap::new();
    // Everything the pass saw, so what it did not see can be removed.
    let mut seen: BTreeSet<(IndexId, FileRef)> = BTreeSet::new();

    for (processed, source) in sources.into_iter().enumerate() {
        on_progress(processed as u64);
        stats.files_seen += 1;
        let key = (source.index, source.file.clone());
        seen.insert(key.clone());

        let text = match source.text {
            Ok(text) => text,
            Err(reason) => {
                // BMI-FR-09: skipped rather than fatal. A file that was indexed
                // before and has now become unreadable is dropped, so the index
                // never keeps chunks it can no longer justify.
                on_skip(&source.index, &source.file, &reason);
                stats.files_skipped += 1;
                if drop_file(&mut next, &mut working, &key) {
                    stats.files_removed += 1;
                }
                continue;
            }
        };

        let checksum = crate::fs::sha256_bytes(text.as_bytes());
        if let Some(existing) = next.files.get(&key) {
            if existing.checksum == checksum {
                stats.files_unchanged += 1;
                continue;
            }
            // BMI-FR-16: a content change replaces the file's chunks whole, so
            // a chunk the edit removed leaves the index with it.
            drop_file(&mut next, &mut working, &key);
        }

        let language = detect_file_language(&text);
        // BMI-FR-05: the nine file-backed indexes split a file into chunks; the
        // `skills` index takes its document whole, since a descriptor has no
        // sections to cut at and its ordinal is always 0.
        let chunks = if source.plain_text {
            documents::chunk_plain_text(&text)
        } else if source.index.splits_into_chunks() {
            chunk_text(&text)
        } else if text.trim().is_empty() {
            Vec::new()
        } else {
            vec![text.clone()]
        };
        if !chunks.is_empty() {
            let shard = working_shard(&next, &mut working, source.index, &language);
            for (ordinal, chunk) in chunks.iter().enumerate() {
                shard
                    .documents
                    .insert(source.file.chunk(ordinal as u32), chunk.clone());
            }
            stats.files_indexed += 1;
        }
        // Recorded even when it yielded no chunk — an empty file, or one of
        // nothing but whitespace. Without the record its checksum could never
        // match, so it would be re-read, re-hashed and re-chunked on every pass
        // for the life of the session.
        next.files.insert(
            key,
            FileEntry {
                checksum,
                language,
                chunk_count: chunks.len() as u32,
            },
        );
    }

    // BMI-FR-15 / BMI-FR-17: anything the pass did not see in an index it
    // covered is gone from disk or has lost the type that put it there. Either
    // way it leaves the index.
    let stale: Vec<(IndexId, FileRef)> = next
        .files
        .keys()
        .filter(|(index, _)| scope.covers(*index))
        .filter(|key| !seen.contains(*key))
        .cloned()
        .collect();
    for key in stale {
        if drop_file(&mut next, &mut working, &key) {
            stats.files_removed += 1;
        }
    }

    rebuild_shards(&mut next, working);
    stats.chunks = next.total_chunks();
    (next, stats)
}

/// A shard's document set while a pass is rearranging it.
struct WorkingShard {
    language: Language,
    documents: BTreeMap<ChunkKey, String>,
}

/// The working set for a shard, seeded on first touch from the engine already
/// mounted for it (or empty when this pass is creating the shard).
fn working_shard<'a>(
    next: &IndexSet,
    working: &'a mut BTreeMap<(IndexId, &'static str), WorkingShard>,
    index: IndexId,
    language: &Language,
) -> &'a mut WorkingShard {
    let shard_key = (index, language_key(language));
    working.entry(shard_key).or_insert_with(|| WorkingShard {
        language: language.clone(),
        documents: next
            .shards
            .get(&shard_key)
            .map(|engine| engine.iter().map(|doc| (doc.id, doc.contents)).collect())
            .unwrap_or_default(),
    })
}

/// Remove a file's record and every chunk it contributed. Returns whether it
/// was indexed at all.
fn drop_file(
    next: &mut IndexSet,
    working: &mut BTreeMap<(IndexId, &'static str), WorkingShard>,
    key: &(IndexId, FileRef),
) -> bool {
    let Some(entry) = next.files.remove(key) else {
        return false;
    };
    let shard = working_shard(next, working, key.0, &entry.language);
    for ordinal in 0..entry.chunk_count {
        shard.documents.remove(&key.1.chunk(ordinal));
    }
    true
}

/// Build an engine for every shard this pass touched, fitting each to the
/// documents it now holds. A shard left holding nothing is dropped rather than
/// kept empty — an empty corpus has no meaningful average document length, and
/// a query against one would divide by zero.
fn rebuild_shards(
    set: &mut IndexSet,
    working: BTreeMap<(IndexId, &'static str), WorkingShard>,
) {
    for (shard_key, shard) in working {
        if shard.documents.is_empty() {
            set.shards.remove(&shard_key);
            continue;
        }
        // Ordered by `ChunkKey`, so the corpus a rebuild is fitted to is the
        // same sequence however the pass arrived at it (BMI-FR-27).
        let docs: Vec<Document<ChunkKey>> = shard
            .documents
            .into_iter()
            .map(|(id, contents)| Document::new(id, contents))
            .collect();
        let engine = SearchEngineBuilder::<ChunkKey>::with_documents(shard.language, docs).build();
        set.shards.insert(shard_key, Arc::new(engine));
    }
}
