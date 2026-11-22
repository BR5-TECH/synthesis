//! Document attachments: a PDF or DOCX that a comment carries
//! (CMS-FR-EHOG through CMS-FR-RQTB).
//!
//! The append extracts the document to validate it and stores only the
//! original bytes (CMS-FR-EHOG). The extraction result lives in the session
//! cache of `doc_extraction::cache` (CMS-FR-AAMO), never on disk and never in
//! a log line.

use std::sync::{Arc, OnceLock};

use super::*;
use crate::doc_extraction::{self, cache, DocumentKind, ExtractError, ExtractedDocument};
use crate::logging::{Domain, Fields, LogLevel};

/// The emit of one log record.
type LogHook = Box<dyn Fn(LogLevel, &str, Fields) + Send + Sync>;

/// What the comments module reads from the application: the extracted text
/// limit of the global settings (CMS-FR-YSRD) and the log.
struct Hooks {
    limit_kb: Box<dyn Fn() -> u32 + Send + Sync>,
    log: LogHook,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static TEST_LIMIT_KB: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Gives the comments module the extracted text limit and the log. Installed
/// once, at application setup.
pub fn install_document_hooks<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    let reader = app.clone();
    let _ = HOOKS.set(Hooks {
        limit_kb: Box::new(move || {
            reader
                .try_state::<GlobalSettingsStore>()
                .and_then(|store| store.load_extracted_text_limit_kb().ok())
                .unwrap_or(crate::global_settings::EXTRACTED_TEXT_LIMIT_KB_DEFAULT)
        }),
        log: Box::new(move |level, message, fields| {
            let domains = [Domain::Backend];
            let buffer = &crate::logging::BUFFER;
            match level {
                LogLevel::Debug => {
                    crate::logging::log_debug(&app, buffer, &domains, message, fields)
                }
                LogLevel::Info => crate::logging::log_info(&app, buffer, &domains, message, fields),
                LogLevel::Warn => crate::logging::log_warn(&app, buffer, &domains, message, fields),
                LogLevel::Error => {
                    crate::logging::log_error(&app, buffer, &domains, message, fields)
                }
            }
        }),
    });
}

/// Sets the limit that appends on the current test thread read.
#[cfg(test)]
pub fn set_test_text_limit_kb(kb: Option<u32>) {
    TEST_LIMIT_KB.with(|cell| cell.set(kb));
}

/// CMS-FR-YSRD: the limit in bytes, read at each append.
fn text_limit_bytes() -> usize {
    #[cfg(test)]
    if let Some(kb) = TEST_LIMIT_KB.with(|cell| cell.get()) {
        return kb as usize * 1024;
    }
    let kb = HOOKS
        .get()
        .map(|hooks| (hooks.limit_kb)())
        .unwrap_or(crate::global_settings::EXTRACTED_TEXT_LIMIT_KB_DEFAULT);
    kb as usize * 1024
}

fn log(level: LogLevel, message: &str, fields: Fields) {
    if let Some(hooks) = HOOKS.get() {
        (hooks.log)(level, message, fields);
    }
}

/// The typed code of an extraction failure (CMS-FR-LZZV).
fn error_code(error: &ExtractError) -> &'static str {
    match error {
        ExtractError::NoText => ERR_NO_TEXT_EXTRACTED,
        ExtractError::TooLarge => ERR_EXTRACTED_TEXT_TOO_LARGE,
        ExtractError::Unreadable(_) | ExtractError::Interrupted(_) => ERR_DOCUMENT_UNREADABLE,
    }
}

/// The cached extraction of `bytes`, or a new one stored in the cache
/// (CMS-FR-AAMO).
fn extract_cached(digest: &str, kind: DocumentKind, bytes: Vec<u8>) -> cache::Outcome {
    let mut extracted = false;
    let outcome = cache::get_or_extract(digest, || {
        extracted = true;
        doc_extraction::extract(kind, bytes)
    });
    if extracted {
        let fields = match &outcome {
            Ok(document) => crate::log_fields! {
                "kind" => kind.as_str(),
                "text_bytes" => document.text_bytes(),
                "images" => document.image_count(),
            },
            Err(error) => crate::log_fields! {
                "kind" => kind.as_str(),
                "reason" => error.reason(),
            },
        };
        match &outcome {
            Ok(_) => log(LogLevel::Debug, "document attachment extracted", fields),
            Err(_) => log(LogLevel::Warn, "document attachment extraction failed", fields),
        }
    }
    outcome
}

/// CMS-FR-EHOG, CMS-FR-LZZV, CMS-FR-YSRD: validate a document attachment at
/// append time. `Ok` for any media type that is not a document. A refusal is
/// `<code>: <filename>`.
pub(super) fn validate_document(
    media_type: &str,
    filename: &str,
    digest: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let Some(kind) = DocumentKind::of_media_type(media_type) else {
        return Ok(());
    };
    let refuse = |code: &str, reason: &str| {
        log(
            LogLevel::Warn,
            "document attachment refused",
            crate::log_fields! { "kind" => kind.as_str(), "code" => code, "reason" => reason },
        );
        Err(format!("{code}: {filename}"))
    };
    let document = match extract_cached(digest, kind, bytes.to_vec()) {
        Ok(document) => document,
        Err(error) => return refuse(error_code(&error), error.reason()),
    };
    let limit = text_limit_bytes();
    if document.text_bytes() > limit {
        return refuse(ERR_EXTRACTED_TEXT_TOO_LARGE, "text above the extracted text limit");
    }
    Ok(())
}

/// CMS-FR-RQTB: the blocks of one document `blob` of a discussion, from the
/// cache or extracted with no text limit. A failure is `no_text_extracted` or
/// `document_unreadable`, and the cache keeps it.
pub fn read_attachment_document(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
    digest: &str,
    draft_id: Option<&str>,
) -> Result<Arc<ExtractedDocument>, String> {
    let thread = locate_discussion_for_attachment(root, worktree, thread_id, draft_id)
        .ok_or(ERR_DISCUSSION_NOT_FOUND)?;
    // The same bytes can be on the thread as another media type too, so the
    // digest and a document media type are matched together.
    let kind = thread
        .comments
        .iter()
        .flat_map(|c| &c.attachments)
        .find_map(|a| match a {
            Attachment::Blob { digest: d, media_type, .. } if d == digest => {
                DocumentKind::of_media_type(media_type)
            }
            _ => None,
        })
        .ok_or(ERR_ATTACHMENT_NOT_FOUND)?;
    let failed = |code: &str, reason: &str| {
        log(
            LogLevel::Warn,
            "document attachment not read",
            crate::log_fields! { "kind" => kind.as_str(), "code" => code, "reason" => reason },
        );
        code.to_string()
    };
    if let Some(outcome) = cache::lookup(digest) {
        return outcome.map_err(|e| failed(error_code(&e), e.reason()));
    }
    let bytes = read_blob_bytes_in(root, attachment_scope(&thread), digest)
        .map_err(|code| failed(&code, "the stored file could not be read"))?;
    extract_cached(digest, kind, bytes).map_err(|e| failed(error_code(&e), e.reason()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // CMS-FR-LZZV, CMS-FR-RQTB: each extraction failure has its typed code. A
    // parser failure, a panic, a deadline, and a worker that did not start are
    // all `document_unreadable`.
    #[test]
    fn each_extraction_failure_has_its_code() {
        assert_eq!(error_code(&ExtractError::NoText), ERR_NO_TEXT_EXTRACTED);
        assert_eq!(error_code(&ExtractError::TooLarge), ERR_EXTRACTED_TEXT_TOO_LARGE);
        for failure in [
            ExtractError::Unreadable("parser panicked".to_string()),
            ExtractError::Interrupted("extraction ran past its deadline".to_string()),
            ExtractError::Interrupted("extractor could not start".to_string()),
        ] {
            assert_eq!(error_code(&failure), ERR_DOCUMENT_UNREADABLE, "{failure:?}");
        }
    }
}
