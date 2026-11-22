//! Document attachments: validation at append, the limit, and the session
//! cache (CMS-FR-47, CMS-FR-EHOG through CMS-FR-RQTB).

use super::*;
use crate::doc_extraction::tests::fixtures::{self as fx, DocxItem, PdfImage};
use crate::doc_extraction::{self, cache, DocBlock, DOCX_MEDIA_TYPE};

fn inline(media_type: &str, filename: &str, bytes: &[u8]) -> AttachmentInput {
    AttachmentInput::Inline {
        media_type: media_type.into(),
        filename: filename.into(),
        data: b64(bytes),
    }
}

fn docx_input(filename: &str, bytes: &[u8]) -> AttachmentInput {
    inline(DOCX_MEDIA_TYPE, filename, bytes)
}

fn pdf_input(filename: &str, bytes: &[u8]) -> AttachmentInput {
    inline("application/pdf", filename, bytes)
}

fn stamp<'a>(by: &'a Participant, at: &'a str) -> Stamp<'a> {
    Stamp { by, at }
}

/// Every file under the comments folder, with its bytes, in name order.
fn store_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&comments_dir(root), &mut out);
    out.sort();
    out
}

fn opened(root: &crate::fs::RootFs, by: &Participant) -> Discussion {
    open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "a thread".into(),
        vec![],
        by,
        "2026-01-01T00:00:00Z",
    )
    .unwrap()
}

fn blob_digest(thread: &Discussion) -> String {
    thread
        .comments
        .iter()
        .flat_map(|c| &c.attachments)
        .find_map(|a| match a {
            Attachment::Blob { digest, .. } => Some(digest.clone()),
            _ => None,
        })
        .expect("a blob")
}

// CMS-FR-47, CMS-FR-EHOG: a DOCX is accepted. The store holds its original
// bytes under their digest, and no file holds its extracted text.
#[test]
fn a_docx_is_stored_as_its_original_bytes_and_its_text_is_stored_nowhere() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let bytes = fx::text_docx("ZQX-marker-original-bytes");
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "a document".into(),
        vec![docx_input("spec.docx", &bytes)],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let digest = blob_digest(&thread);
    assert_eq!(digest, fsa::sha256_bytes(&bytes));
    assert_eq!(stored_files(dir.path()), vec![digest.clone()]);
    assert_eq!(std::fs::read(attachments_dir(dir.path()).join(&digest)).unwrap(), bytes);
    let blob = attachments_dir(dir.path()).join(&digest);
    for (path, content) in store_snapshot(dir.path()) {
        if path == blob {
            continue;
        }
        let text = String::from_utf8_lossy(&content);
        assert!(!text.contains("ZQX-marker-original-bytes"), "{} holds the text", path.display());
    }
}

// CMS-FR-LZZV, CMS-FR-47: each refusal is `<code>: <filename>`, and it
// refuses the whole append, with a valid image beside it: the log and the
// store are byte for byte what they were.
#[test]
fn each_document_refusal_names_the_file_and_appends_nothing() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("ada");
    let thread = opened(root, &by);
    crate::comments::set_test_text_limit_kb(Some(1));
    let scanned = fx::pdf(&[fx::page(&[], &[PdfImage::Rgb(300, 300)])]);
    let large = fx::text_docx(&"x".repeat(2_000));
    let cases = [
        (pdf_input("scan.pdf", &scanned), "no_text_extracted: scan.pdf"),
        (docx_input("broken.docx", b"not a zip"), "document_unreadable: broken.docx"),
        (pdf_input("cut: part.pdf", b"%PDF-1.7 cut"), "document_unreadable: cut: part.pdf"),
        (docx_input("big.docx", &large), "extracted_text_too_large: big.docx"),
    ];
    for (input, expected) in cases {
        let before = store_snapshot(dir.path());
        let result = add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "with a document".into(),
            vec![],
            vec![inline_png("ok.png", b"\x89PNG\r\n\x1a\nfine"), input],
            stamp(&by, "2026-01-01T00:00:01Z"),
        );
        assert_eq!(result.unwrap_err(), expected);
        assert_eq!(store_snapshot(dir.path()), before, "{expected}: nothing written");
    }
    crate::comments::set_test_text_limit_kb(None);
}

/// A DOCX whose extracted text is `text` and a paragraph end.
fn docx_with_text_bytes(text: &str) -> Vec<u8> {
    fx::docx(&[DocxItem::Paragraph(text.to_string())])
}

// CMS-FR-YSRD: the limit counts UTF-8 bytes. Text of exactly 1024 bytes is
// accepted under a 1 KB limit, and 1025 bytes are refused although they are
// far fewer characters.
#[test]
fn the_limit_counts_bytes_and_accepts_text_equal_to_it() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("ada");
    let thread = opened(root, &by);
    crate::comments::set_test_text_limit_kb(Some(1));
    // 511 × 2 bytes + 1 + the paragraph end = 1024 bytes.
    let at_limit = docx_with_text_bytes(&format!("{}a", "é".repeat(511)));
    let extracted =
        doc_extraction::extract(doc_extraction::DocumentKind::Docx, at_limit.clone()).unwrap();
    assert_eq!(extracted.text_bytes(), 1024);
    add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "at the limit".into(),
        vec![],
        vec![docx_input("at.docx", &at_limit)],
        stamp(&by, "2026-01-01T00:00:01Z"),
    )
    .expect("text equal to the limit is accepted");
    // 512 × 2 bytes + the paragraph end = 1025 bytes, 513 characters.
    let over = docx_with_text_bytes(&"é".repeat(512));
    let refused = add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "over the limit".into(),
        vec![],
        vec![docx_input("over.docx", &over)],
        stamp(&by, "2026-01-01T00:00:02Z"),
    );
    assert_eq!(refused.unwrap_err(), "extracted_text_too_large: over.docx");
    crate::comments::set_test_text_limit_kb(None);
}

// CMS-FR-YSRD, CMS-FR-AAMO: the limit is read at each append, and a second
// append of the same bytes reuses the cached extraction.
#[test]
fn the_limit_is_read_at_each_append_and_the_cache_is_reused() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("ada");
    let thread = opened(root, &by);
    let bytes = docx_with_text_bytes(&"ZQX-reread ".repeat(150));
    crate::comments::set_test_text_limit_kb(Some(1));
    let append = |at: &str| {
        add_comment_in(
            root,
            "specs/a.md",
            &thread.id,
            "again".into(),
            vec![],
            vec![docx_input("r.docx", &bytes)],
            stamp(&by, at),
        )
    };
    assert!(append("2026-01-01T00:00:01Z").is_err());
    let after_first = doc_extraction::extraction_count();
    crate::comments::set_test_text_limit_kb(Some(2));
    append("2026-01-01T00:00:02Z").expect("the larger limit accepts it");
    assert_eq!(doc_extraction::extraction_count(), after_first, "the cache was reused");
    crate::comments::set_test_text_limit_kb(None);
}

// CMS-FR-YSRD, CMS-FR-RQTB: a lower limit later never refuses a document that
// is already posted.
#[test]
fn a_posted_document_is_never_refused_by_a_later_limit() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("ada");
    let bytes = docx_with_text_bytes(&"ZQX-posted ".repeat(300));
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "posted".into(),
        vec![docx_input("p.docx", &bytes)],
        &by,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    crate::comments::set_test_text_limit_kb(Some(1));
    cache::clear_for_test();
    let document =
        read_attachment_document(root, root, &thread.id, &blob_digest(&thread), None).unwrap();
    assert!(document.text_bytes() > 1024);
    crate::comments::set_test_text_limit_kb(None);
}

// CMS-FR-AAMO, CMS-FR-RQTB: a read after the append does not extract again;
// after a cache miss one read extracts, and the next read does not.
#[test]
fn a_read_reuses_the_cache_and_a_miss_extracts_once() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let bytes = fx::docx(&[fx::para("ZQX-cache-read"), DocxItem::Image(fx::png(60, 60))]);
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "posted".into(),
        vec![docx_input("c.docx", &bytes)],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let digest = blob_digest(&thread);
    let start = doc_extraction::extraction_count();
    let document = read_attachment_document(root, root, &thread.id, &digest, None).unwrap();
    assert_eq!(doc_extraction::extraction_count(), start, "the append filled the cache");
    assert!(matches!(&document.blocks[0], DocBlock::Text(t) if t.contains("ZQX-cache-read")));
    assert!(matches!(&document.blocks[1], DocBlock::Image { .. }));
    cache::clear_for_test();
    let again = read_attachment_document(root, root, &thread.id, &digest, None).unwrap();
    assert_eq!(again, document);
    assert_eq!(doc_extraction::extraction_count(), start + 1);
    read_attachment_document(root, root, &thread.id, &digest, None).unwrap();
    assert_eq!(doc_extraction::extraction_count(), start + 1);
}

// CMS-FR-RQTB: a stored document that no longer extracts gives
// `document_unreadable`, and the cache keeps that failure.
#[test]
fn a_failed_read_is_document_unreadable_and_is_cached() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let bytes = fx::text_pdf("ZQX-failed-read");
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "posted".into(),
        vec![pdf_input("f.pdf", &bytes)],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let digest = blob_digest(&thread);
    cache::clear_for_test();
    std::fs::write(attachments_dir(dir.path()).join(&digest), b"garbage").unwrap();
    let start = doc_extraction::extraction_count();
    for _ in 0..2 {
        let read = read_attachment_document(root, root, &thread.id, &digest, None);
        assert_eq!(read.unwrap_err(), ERR_DOCUMENT_UNREADABLE);
    }
    assert_eq!(doc_extraction::extraction_count(), start + 1);
}

// CMS-FR-RQTB: an unknown discussion and a digest that is not a document of
// the thread read nothing. The same bytes posted as plain text and as a DOCX
// are read as the DOCX.
#[test]
fn a_document_read_resolves_the_digest_with_a_document_media_type() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let by = human("ada");
    let bytes = fx::text_docx("ZQX-two-types");
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "first as text".into(),
        vec![inline("text/plain", "d.txt", &bytes), inline_png("p.png", b"\x89PNG\r\n\x1a\nx")],
        &by,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let png_digest = fsa::sha256_bytes(b"\x89PNG\r\n\x1a\nx");
    let digest = fsa::sha256_bytes(&bytes);
    assert_eq!(
        read_attachment_document(root, root, &thread.id, &digest, None).unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
        "only a text attachment carries the digest yet",
    );
    let thread = add_comment_in(
        root,
        "specs/a.md",
        &thread.id,
        "then as a document".into(),
        vec![],
        vec![docx_input("d.docx", &bytes)],
        stamp(&by, "2026-01-01T00:00:01Z"),
    )
    .unwrap();
    assert!(read_attachment_document(root, root, &thread.id, &digest, None).is_ok());
    assert_eq!(
        read_attachment_document(root, root, &thread.id, &png_digest, None).unwrap_err(),
        ERR_ATTACHMENT_NOT_FOUND,
    );
    assert_eq!(
        read_attachment_document(root, root, "no-such-thread", &digest, None).unwrap_err(),
        ERR_DISCUSSION_NOT_FOUND,
    );
}

// CMS-FR-47: the size bound and the base64 check come before extraction, so
// a PDF above the bound or with bad data is never extracted.
#[test]
fn the_size_bound_and_the_encoding_are_checked_before_extraction() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let start = doc_extraction::extraction_count();
    let open_with = |input: AttachmentInput| {
        open_artifact_fragment_in(
            root,
            "specs/a.md",
            anchor(0, 5, "hello"),
            "big".into(),
            vec![input],
            &human("ada"),
            "2026-01-01T00:00:00Z",
        )
    };
    let big = vec![b'%'; MAX_ATTACHMENT_BYTES + 1];
    assert_eq!(open_with(pdf_input("big.pdf", &big)).unwrap_err(), ERR_ATTACHMENT_TOO_LARGE);
    let bad = AttachmentInput::Inline {
        media_type: "application/pdf".into(),
        filename: "bad.pdf".into(),
        data: "!!not base64!!".into(),
    };
    assert_eq!(open_with(bad).unwrap_err(), ERR_MALFORMED_ATTACHMENT);
    assert_eq!(doc_extraction::extraction_count(), start);
}

// CMS-FR-YSRD, CMS-FR-EHOG: in the application the append reads the limit of
// the global settings through the installed hooks, and its log records name
// no filename and hold no document text.
#[test]
fn the_installed_hooks_read_the_stored_limit_and_log_no_content() {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    let store = GlobalSettingsStore::in_memory();
    store.save_extracted_text_limit_kb(1).unwrap();
    app.manage(store);
    crate::comments::install_document_hooks(app.handle().clone());
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let bytes = fx::text_docx(&"ZQX-secret-text ".repeat(150));
    let refused = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "large".into(),
        vec![docx_input("private-name.docx", &bytes)],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    );
    assert_eq!(refused.unwrap_err(), "extracted_text_too_large: private-name.docx");
    let (text, count) =
        crate::logging::BUFFER.export_text(&crate::logging::LogFilter::default()).unwrap();
    assert!(count >= 2, "the extraction and the refusal are logged: {text}");
    assert!(text.contains("document attachment refused"), "{text}");
    assert!(!text.contains("ZQX-secret-text"), "no document text in the log");
    assert!(!text.contains("private-name"), "no filename in the log");
}

// CMS-FR-YSRD: a posted document is read whole after the limit is lowered.
#[test]
fn a_posted_document_is_read_whole_after_the_limit_is_lowered() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let text = "ZQX-whole ".repeat(330);
    let bytes = docx_with_text_bytes(&text);
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "posted".into(),
        vec![docx_input("w.docx", &bytes)],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    crate::comments::set_test_text_limit_kb(Some(1));
    cache::clear_for_test();
    let document =
        read_attachment_document(root, root, &thread.id, &blob_digest(&thread), None).unwrap();
    assert_eq!(document.text_bytes(), text.len() + 1, "the whole text and its paragraph end");
    crate::comments::set_test_text_limit_kb(None);
}

// CMS-FR-47: a Markdown file is accepted under either Markdown media type.
// It is not a document attachment, so it is stored and not extracted.
#[test]
fn a_markdown_file_is_accepted_and_not_extracted() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let start = doc_extraction::extraction_count();
    let thread = open_artifact_fragment_in(
        root,
        "specs/a.md",
        anchor(0, 5, "hello"),
        "notes".into(),
        vec![
            inline("text/markdown", "notes.md", b"# Notes\n"),
            inline("text/x-markdown", "more.markdown", b"# More\n"),
        ],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("markdown is accepted");
    assert_eq!(only_attachments(&thread).len(), 2);
    assert_eq!(stored_files(dir.path()).len(), 2);
    assert_eq!(doc_extraction::extraction_count(), start);
}
