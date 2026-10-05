//! The `documents` index (`BMI-bm25-indexing.md` BMI-FR-WBKZ, BMI-FR-FGGU,
//! BMI-FR-MWNQ).
//!
//! The documents arrive already read, on the internal channel of
//! `DCL-documents-collection.md`: file text for a `markdown` or `text` document,
//! and the text `pdf_extract` found for a `pdf` one. This module performs no
//! read of its own (BMI-FR-26), and it holds no filesystem path: a document is
//! named by its id.

use tauri::Manager;

use crate::documents::{DocumentFormat, DocumentsCollection};
use crate::fs::FsAccessState;

use super::{FileRef, IndexId, SourceFile, MAX_FILE_BYTES};

/// BMI-FR-WBKZ: one source per available document of the collection.
///
/// A document with no text, and one over the ceiling of BMI-FR-09, comes back as
/// a skip with a fixed reason, so the pass drops what the index held for it and
/// carries on. An unavailable document, and one that left the collection, is not
/// listed at all, so a pass removes its chunks (BMI-FR-MWNQ).
pub(super) fn collect_documents<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<SourceFile> {
    let (Some(collection), Some(fs)) = (
        app.try_state::<DocumentsCollection>(),
        app.try_state::<FsAccessState>(),
    ) else {
        return Vec::new();
    };
    // DCL-FR-MSHW: with no project open or an unreadable store the collection
    // refuses, and the index is left with no documents.
    let documents = match collection.documents_for_index(&fs, app) {
        Ok(documents) => documents,
        Err(error) => {
            crate::documents::log::DocLog::log(
                app,
                crate::logging::LogLevel::Warn,
                "documents index pass has no documents",
                crate::log_fields! { "error" => error.code() },
            );
            return Vec::new();
        }
    };
    documents
        .into_iter()
        .map(|document| SourceFile {
            index: IndexId::Documents,
            file: FileRef::document(document.id),
            text: match document.text {
                Ok(text) if text.len() as u64 > MAX_FILE_BYTES => {
                    Err(format!("exceeds the {MAX_FILE_BYTES}-byte ceiling"))
                }
                Ok(text) => Ok(text),
                Err(reason) => Err(reason.to_string()),
            },
            // BMI-FR-FGGU: only a `markdown` document has headings to cut at.
            plain_text: document.format != DocumentFormat::Markdown,
        })
        .collect()
}

/// BMI-FR-FGGU: split a `text` or `pdf` document into blocks no longer than
/// `MAX_CHUNK_CHARS`, cut at blank lines. A line that starts with `#` is not a
/// heading here, because a plain text file has none and extracted PDF text
/// carries no Markdown.
pub(super) fn chunk_plain_text(text: &str) -> Vec<String> {
    let normalised;
    let text = if text.contains('\r') {
        normalised = text.replace("\r\n", "\n").replace('\r', "\n");
        normalised.as_str()
    } else {
        text
    };
    super::bound_to_blocks(text)
        .into_iter()
        .filter(|block| !block.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod chunk_ceiling {
    use super::super::MAX_CHUNK_CHARS;
    use super::*;

    // BMI-FR-FGGU: no block exceeds the chunk ceiling, and a `#` line stays in
    // the block it belongs to.
    #[test]
    fn plain_text_blocks_respect_the_ceiling_and_ignore_headings() {
        let paragraph = "word ".repeat(300);
        let text = format!("# not a heading\n\n{paragraph}\n\n{paragraph}\n\n{paragraph}");
        let blocks = chunk_plain_text(&text);
        assert!(blocks.len() > 1);
        assert!(blocks.iter().all(|b| b.chars().count() <= MAX_CHUNK_CHARS));
        assert!(blocks[0].starts_with("# not a heading"));
    }
}
