//! Reading documents (DCL-FR-NCBQ, DCL-FR-TEUK, DCL-FR-GLUS, DCL-FR-VRTS,
//! DCL-FR-QVYZ).
//!
//! Every read goes through the documents instance of `FSA-filesystem-access.md`
//! (DCL-FR-BAQY), which reaches the selected paths read-only. A path never
//! leaves this module for the frontend to read; the frontend gets content.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use base64::Engine;

use crate::fs::{FsAccess, FsAccessState, FsError};
use crate::logging::LogLevel;

use super::log::DocLog;
use super::manager::{lock, DocumentsCollection, Fingerprint, PdfCacheEntry};
use super::model::{
    Availability, DocumentEntry, DocumentFormat, DocumentPdf, DocumentText, DocumentsError,
};

/// Why a read of a file's content failed.
#[derive(Debug)]
pub(super) enum ReadFailure {
    /// The file could not be read.
    Unreadable,
    /// A `markdown` or `text` file that is not valid UTF-8 (DCL-FR-QTGN).
    NotUtf8,
    /// DCL-FR-RXMB: not a regular file.
    NotRegular,
    /// DCL-FR-RXMB: a file over the size limit.
    TooLarge,
}

impl ReadFailure {
    /// The fixed words a `WARN` record gives as the reason.
    pub(super) fn reason(&self) -> &'static str {
        match self {
            ReadFailure::Unreadable => "unreadable",
            ReadFailure::NotUtf8 => "not valid utf-8",
            ReadFailure::NotRegular => "not a regular file",
            ReadFailure::TooLarge => "over the size limit",
        }
    }
}

/// DCL-FR-RXMB: read the content of one document. Only a regular file is read,
/// and at most `max_bytes` of it.
pub(super) fn read_content(
    access: &FsAccess,
    path: &std::path::Path,
    max_bytes: u64,
) -> Result<Vec<u8>, ReadFailure> {
    access.read_regular_bytes(path, max_bytes).map_err(|e| match e {
        FsError::NotRegular { .. } => ReadFailure::NotRegular,
        FsError::TooLarge { .. } => ReadFailure::TooLarge,
        _ => ReadFailure::Unreadable,
    })
}

/// The revision of a document: the lowercase hexadecimal SHA-256 of its content
/// bytes (DCL-FR-SQEP). A `markdown` or `text` file must also be valid UTF-8,
/// or it is unavailable (DCL-FR-QTGN).
pub(super) fn revision_of(
    access: &FsAccess,
    path: &str,
    format: DocumentFormat,
    max_bytes: u64,
) -> Result<String, ReadFailure> {
    let bytes = read_content(access, &PathBuf::from(path), max_bytes)?;
    if format != DocumentFormat::Pdf && std::str::from_utf8(&bytes).is_err() {
        return Err(ReadFailure::NotUtf8);
    }
    Ok(crate::fs::sha256_bytes(&bytes))
}

/// One available document with its text, for the `documents` index
/// (BMI-FR-WBKZ).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDocument {
    pub id: String,
    pub format: DocumentFormat,
    /// The full text, or the fixed reason the document contributes none.
    pub text: Result<String, &'static str>,
}

impl DocumentsCollection {
    /// DCL contract: the entry for an id in the collection, or
    /// `unknown_document`. It is the only way a tool turns an id into a
    /// document.
    pub fn resolve_document(&self, id: &str) -> Result<DocumentEntry, DocumentsError> {
        let inner = lock(&self.inner);
        if !inner.open {
            return Err(DocumentsError::NoProjectOpen);
        }
        // DCL-FR-MSHW: with the store unreadable the collection shows no
        // documents, and the operation says why.
        if !inner.store_ok {
            return Err(DocumentsError::StoreUnavailable);
        }
        inner
            .records
            .get(id)
            .map(|record| record.entry.clone())
            .ok_or(DocumentsError::UnknownDocument)
    }

    /// DCL-FR-RXMB: the bytes of one document, read within the limit. A WARN
    /// record names the document when the read is refused for its kind or size.
    fn read_document_bytes(
        &self,
        access: &FsAccess,
        entry: &DocumentEntry,
    ) -> Result<Vec<u8>, DocumentsError> {
        read_content(access, std::path::Path::new(&entry.path), self.limits.max_read_bytes)
            .map_err(|_| DocumentsError::Unavailable)
    }

    /// DCL-FR-NCBQ: the current text of a `markdown` or `text` document.
    pub fn read_document(
        &self,
        fs: &FsAccessState,
        id: &str,
    ) -> Result<DocumentText, DocumentsError> {
        self.require_open()?;
        let entry = self.resolve_document(id)?;
        if entry.format == DocumentFormat::Pdf {
            return Err(DocumentsError::WrongFormat);
        }
        if entry.status == Availability::Unavailable {
            return Err(DocumentsError::Unavailable);
        }
        let access = fs.documents().ok_or(DocumentsError::Unavailable)?;
        let bytes = self.read_document_bytes(&access, &entry)?;
        let revision = crate::fs::sha256_bytes(&bytes);
        let text = String::from_utf8(bytes).map_err(|_| DocumentsError::Unavailable)?;
        Ok(DocumentText {
            id: entry.id,
            name: entry.name,
            format: entry.format,
            text,
            revision,
        })
    }

    /// DCL-FR-TEUK: the whole content of a PDF document, base64 encoded.
    pub fn read_document_pdf(
        &self,
        fs: &FsAccessState,
        id: &str,
    ) -> Result<DocumentPdf, DocumentsError> {
        self.require_open()?;
        let entry = self.resolve_document(id)?;
        if entry.format != DocumentFormat::Pdf {
            return Err(DocumentsError::WrongFormat);
        }
        if entry.status == Availability::Unavailable {
            return Err(DocumentsError::Unavailable);
        }
        let access = fs.documents().ok_or(DocumentsError::Unavailable)?;
        let bytes = self.read_document_bytes(&access, &entry)?;
        Ok(DocumentPdf {
            id: entry.id,
            name: entry.name,
            revision: crate::fs::sha256_bytes(&bytes),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        })
    }

    /// DCL-FR-GLUS: the full text of any available document.
    ///
    /// For `markdown` and `text` it is the file text as the file stands now. For
    /// `pdf` it is the text the extractor finds, from the session cache while the
    /// revision is unchanged (DCL-FR-QVYZ). A PDF with no text, or one that
    /// cannot be parsed, is `no_text` (DCL-FR-VRTS).
    pub fn document_text(
        &self,
        fs: &FsAccessState,
        log: &dyn DocLog,
        id: &str,
    ) -> Result<String, DocumentsError> {
        let entry = self.resolve_document(id)?;
        if entry.status == Availability::Unavailable {
            return Err(DocumentsError::Unavailable);
        }
        let access = fs.documents().ok_or(DocumentsError::Unavailable)?;
        match entry.format {
            DocumentFormat::Markdown | DocumentFormat::Text => {
                let bytes = self.read_document_bytes(&access, &entry)?;
                String::from_utf8(bytes).map_err(|_| DocumentsError::Unavailable)
            }
            DocumentFormat::Pdf => self.pdf_text(&access, &entry, log),
        }
    }

    /// DCL-FR-QVYZ / DCL-FR-VRTS: the text of one PDF, extracted at most once per
    /// revision.
    fn pdf_text(
        &self,
        access: &FsAccess,
        entry: &DocumentEntry,
        log: &dyn DocLog,
    ) -> Result<String, DocumentsError> {
        let path = PathBuf::from(&entry.path);
        // The revision the file has now. When its size and modification time
        // are what the last refresh saw, that refresh's revision stands and the
        // file is not read; otherwise it changed since, and the bytes decide.
        let known = {
            let inner = lock(&self.inner);
            inner
                .records
                .get(&entry.id)
                .and_then(|r| Some((r.fingerprint.clone()?, r.entry.revision.clone()?)))
        };
        let current = access.file_info(&path).ok().map(|info| Fingerprint {
            size: info.size,
            modified: info.modified,
        });
        let mut bytes: Option<Vec<u8>> = None;
        let revision = match (known, current) {
            (Some((fingerprint, revision)), Some(now)) if fingerprint == now => revision,
            _ => {
                let read = self.read_document_bytes(access, entry)?;
                let revision = crate::fs::sha256_bytes(&read);
                bytes = Some(read);
                revision
            }
        };
        if let Some(text) = self.cached_pdf_text(&entry.id, &revision) {
            return text.ok_or(DocumentsError::NoText);
        }
        let bytes = match bytes {
            Some(bytes) => bytes,
            None => self.read_document_bytes(access, entry)?,
        };
        // What was actually read decides the revision the text is stored under,
        // so text is never cached against bytes it did not come from.
        let revision = crate::fs::sha256_bytes(&bytes);
        let text = self.extract(&entry.id, bytes, log);
        lock(&self.pdf_cache).insert(
            entry.id.clone(),
            PdfCacheEntry {
                revision,
                text: text.clone(),
            },
        );
        text.ok_or(DocumentsError::NoText)
    }

    /// DCL-FR-QVYZ: the cached outcome for this id and revision. A cached entry
    /// of another revision is discarded.
    fn cached_pdf_text(&self, id: &str, revision: &str) -> Option<Option<String>> {
        let mut cache = lock(&self.pdf_cache);
        match cache.get(id) {
            Some(entry) if entry.revision == revision => Some(entry.text.clone()),
            Some(_) => {
                cache.remove(id);
                None
            }
            None => None,
        }
    }

    /// DCL-FR-VRTS, DCL-FR-DLPX: run the extractor on bytes already read, on a
    /// worker thread, under a deadline and a maximum input size. A failure, a
    /// panic, a PDF over the size limit, and an extraction that has not ended at
    /// the deadline are each recorded as a `WARN` that names the document id, and
    /// reported as no text. None of them fails the pass or the command, and none
    /// holds the caller longer than the deadline. A worker that passes the
    /// deadline is abandoned: it cannot be stopped, and its result is dropped.
    fn extract(&self, id: &str, bytes: Vec<u8>, log: &dyn DocLog) -> Option<String> {
        let warn = |reason: &str| {
            log.log(
                LogLevel::Warn,
                "pdf has no extractable text",
                crate::log_fields! { "document" => id, "format" => "pdf", "reason" => reason },
            );
        };
        if bytes.len() > self.limits.max_extract_bytes {
            warn("larger than the extraction limit");
            return None;
        }
        self.extractions.fetch_add(1, Ordering::Relaxed);
        let extractor = self.extractor;
        let (send, receive) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("documents-pdf-extract".to_string())
            .spawn(move || {
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extractor(&bytes)));
                // The receiver is gone when the deadline has passed.
                let _ = send.send(outcome);
            });
        if spawned.is_err() {
            warn("extractor could not start");
            return None;
        }
        match receive.recv_timeout(self.limits.extract_deadline) {
            Ok(Ok(Ok(text))) if !text.trim().is_empty() => Some(text),
            Ok(Ok(Ok(_))) => {
                warn("no embedded text");
                None
            }
            Ok(Ok(Err(_))) => {
                warn("could not be parsed");
                None
            }
            Ok(Err(_)) => {
                warn("extractor panicked");
                None
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                warn("extraction ran past its deadline");
                None
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                warn("extractor stopped");
                None
            }
        }
    }

    /// BMI-FR-WBKZ: every available document with its text.
    ///
    /// DCL-FR-MSHW: with no project open, or with the store unreadable, it
    /// answers with the typed refusal rather than with an empty list.
    pub fn documents_for_index(
        &self,
        fs: &FsAccessState,
        log: &dyn DocLog,
    ) -> Result<Vec<IndexDocument>, DocumentsError> {
        let entries: Vec<DocumentEntry> = {
            let inner = lock(&self.inner);
            if !inner.open {
                return Err(DocumentsError::NoProjectOpen);
            }
            if !inner.store_ok {
                return Err(DocumentsError::StoreUnavailable);
            }
            inner
                .snapshot
                .documents
                .iter()
                .filter(|d| d.status == Availability::Available)
                .cloned()
                .collect()
        };
        Ok(entries
            .into_iter()
            .map(|entry| IndexDocument {
                text: match self.document_text(fs, log, &entry.id) {
                    Ok(text) => Ok(text),
                    Err(DocumentsError::NoText) => Err("has no extractable text"),
                    Err(_) => Err("is unavailable"),
                },
                id: entry.id,
                format: entry.format,
            })
            .collect())
    }

    /// How many times the PDF extractor has run. Test-facing: it is what shows
    /// that a cached PDF is not extracted again.
    #[cfg(test)]
    pub fn extraction_count(&self) -> usize {
        self.extractions.load(Ordering::Relaxed)
    }
}
