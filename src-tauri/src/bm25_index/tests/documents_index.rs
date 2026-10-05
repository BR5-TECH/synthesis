//! The `documents` index (BMI-FR-02, BMI-FR-05, BMI-FR-WBKZ, BMI-FR-FGGU,
//! BMI-FR-MWNQ, BMI-FR-NEIW).

use super::*;

fn document(id: &str, text: &str, plain_text: bool) -> SourceFile {
    SourceFile {
        index: IndexId::Documents,
        file: FileRef::document(id),
        text: Ok(text.to_string()),
        plain_text,
    }
}

// BMI-FR-02: there are exactly twelve indexes, and the documents index is one of
// them.
#[test]
fn there_are_twelve_indexes_and_documents_is_one() {
    assert_eq!(IndexId::ALL.len(), 12);
    assert!(IndexId::ALL.contains(&IndexId::Documents));
    assert_eq!(IndexId::Documents.as_str(), "documents");
    assert_eq!(
        serde_json::to_value(IndexId::Documents).unwrap(),
        serde_json::json!("documents")
    );
    assert!(!IndexId::Documents.is_artifact_index(), "no scan node id");
}

// BMI-FR-05, BMI-FR-FGGU: a Markdown document is cut at its headings, and a text
// or PDF document is cut at blank lines only, so a `#` line is not a section.
#[test]
fn markdown_is_cut_at_headings_and_plain_text_is_not() {
    let text = "# One\n\nalpha words\n\n# Two\n\nbeta words\n";
    let index = pass(
        &IndexSet::default(),
        vec![document("doc-md", text, false), document("doc-txt", text, true)],
        PassScope::DOCUMENTS,
    );
    let chunks = |id: &str| {
        index
            .files
            .iter()
            .find(|((i, f), _)| *i == IndexId::Documents && f.path == id)
            .map(|(_, entry)| entry.chunk_count)
            .unwrap()
    };
    assert_eq!(chunks("doc-md"), 2);
    assert_eq!(chunks("doc-txt"), 1);
}

// BMI-FR-FGGU: a hit carries the document id in `document_id` and in `path`, and
// no other field names a document or a filesystem path.
#[test]
fn a_hit_carries_the_document_id() {
    let index = pass(
        &IndexSet::default(),
        vec![document("doc-0123", "the quokka is a small marsupial", true)],
        PassScope::DOCUMENTS,
    );
    let hits = search_snapshot(&index, &[IndexId::Documents], "quokka", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].index, IndexId::Documents);
    assert_eq!(hits[0].document_id.as_deref(), Some("doc-0123"));
    assert_eq!(hits[0].path, "doc-0123");
    assert_eq!(hits[0].node_id, None);
    assert_eq!(hits[0].draft_id, None);
    assert_eq!(hits[0].note_id, None);
    let json = serde_json::to_value(&hits[0]).unwrap();
    assert_eq!(json["documentId"], "doc-0123");
}

// BMI-FR-02: another index's text does not change the documents index's
// results, and a query for other indexes returns no document.
#[test]
fn the_documents_index_is_separate_from_the_others() {
    let index = pass(
        &IndexSet::default(),
        vec![
            document("doc-1", "gecko lizards climb walls", true),
            artifact(IndexId::Spec, "specs/a.md", "# Gecko\n\ngecko lizards climb walls"),
        ],
        PassScope::ALL,
    );
    let only_documents = search_snapshot(&index, &[IndexId::Documents], "gecko", 10);
    assert_eq!(only_documents.len(), 1);
    assert_eq!(only_documents[0].index, IndexId::Documents);
    let only_specs = search_snapshot(&index, &[IndexId::Spec], "gecko", 10);
    assert_eq!(only_specs.len(), 1);
    assert_eq!(only_specs[0].index, IndexId::Spec);
}

// BMI-FR-MWNQ: a changed text replaces a document's chunks whole, an unchanged one
// is left, and a document no longer supplied loses its chunks. A pass of another
// scope leaves the documents index as it was.
#[test]
fn a_documents_pass_replaces_adds_and_removes() {
    let first = pass(
        &IndexSet::default(),
        vec![
            document("doc-a", "wombat burrow", true),
            document("doc-b", "echidna spines", true),
        ],
        PassScope::DOCUMENTS,
    );
    assert_eq!(first.file_count(IndexId::Documents), 2);

    let (second, stats) = pass_with_stats(
        &first,
        vec![
            document("doc-a", "numbat termites", true),
            document("doc-b", "echidna spines", true),
        ],
        PassScope::DOCUMENTS,
    );
    assert_eq!(stats.files_unchanged, 1);
    assert!(search_snapshot(&second, &[IndexId::Documents], "wombat", 5).is_empty());
    assert_eq!(search_snapshot(&second, &[IndexId::Documents], "numbat", 5).len(), 1);

    let third = pass(&second, vec![document("doc-b", "echidna spines", true)], PassScope::DOCUMENTS);
    assert_eq!(third.file_count(IndexId::Documents), 1);
    assert!(search_snapshot(&third, &[IndexId::Documents], "numbat", 5).is_empty());

    // A drafts-only pass sees no document source and removes none.
    let fourth = pass(&third, Vec::new(), PassScope::DRAFTS);
    assert_eq!(fourth.file_count(IndexId::Documents), 1);
    assert!(PassScope::DOCUMENTS.covers(IndexId::Documents));
    assert!(!PassScope::DRAFTS.covers(IndexId::Documents));
    assert!(PassScope::ALL.covers(IndexId::Documents));
}

// BMI-FR-WBKZ: a document with no text, or an unreadable one, is skipped rather
// than fatal, and drops what the index held for it.
#[test]
fn a_skipped_document_loses_its_chunks() {
    let first = pass(&IndexSet::default(), vec![document("doc-a", "ibex horns", true)], PassScope::DOCUMENTS);
    let second = pass(
        &first,
        vec![SourceFile {
            index: IndexId::Documents,
            file: FileRef::document("doc-a"),
            text: Err("has no extractable text".to_string()),
            plain_text: true,
        }],
        PassScope::DOCUMENTS,
    );
    assert_eq!(second.file_count(IndexId::Documents), 0);
}

// BMI-FR-NEIW: the part of a snapshot that survives a new content root holds the
// documents index and nothing else.
#[test]
fn only_the_documents_part_survives_a_new_root() {
    let full = pass(
        &IndexSet::default(),
        vec![
            document("doc-a", "ibex horns", true),
            artifact(IndexId::Spec, "specs/a.md", "# Ibex\n\nibex horns"),
        ],
        PassScope::ALL,
    );
    let kept = full.only_documents();
    assert_eq!(kept.file_count(IndexId::Documents), 1);
    assert_eq!(kept.file_count(IndexId::Spec), 0);
    assert_eq!(search_snapshot(&kept, &[IndexId::Documents], "ibex", 5).len(), 1);
    assert!(search_snapshot(&kept, &[IndexId::Spec], "ibex", 5).is_empty());
}
