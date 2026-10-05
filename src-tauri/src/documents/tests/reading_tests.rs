//! Reading documents, PDF text extraction, and its session cache (DCL-FR-NCBQ,
//! DCL-FR-TEUK, DCL-FR-GLUS, DCL-FR-VRTS, DCL-FR-QVYZ, DCL-FR-BAQY,
//! DCL-FR-TAGV).

use super::*;
use base64::Engine;

fn mtime_gap() {
    // The refresh reads a file again only when its size or modification time
    // moved, so a rewrite of the same size needs the clock to tick.
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

// DCL-FR-NCBQ: `read_document` returns the current text, name, format, and
// revision of a Markdown or text document, by id alone.
#[test]
fn a_text_document_is_read_by_id() {
    let docs = Docs::new();
    docs.write("refs/a.md", "# Title\n\nbody");
    docs.add_folder("refs");
    docs.refresh();
    let id = docs.id_of("refs/a.md");
    let read = docs.collection.read_document(&docs.fs, &id).unwrap();
    assert_eq!(read.id, id);
    assert_eq!(read.name, "a.md");
    assert_eq!(read.text, "# Title\n\nbody");
    assert_eq!(read.revision, crate::fs::sha256_bytes(b"# Title\n\nbody"));
}

// DCL-FR-NCBQ: the read is of the file as it stands now, with the revision of
// those bytes, before any refresh has noticed the change.
#[test]
fn a_read_sees_the_current_file_content() {
    let docs = Docs::new();
    let file = docs.write("a.txt", "before");
    docs.add_file("a.txt");
    docs.refresh();
    std::fs::write(&file, "after the change").unwrap();
    let read = docs.collection.read_document(&docs.fs, &docs.id_of("a.txt")).unwrap();
    assert_eq!(read.text, "after the change");
    assert_eq!(read.revision, crate::fs::sha256_bytes(b"after the change"));
}

// DCL-FR-NCBQ, DCL-FR-TEUK, DCL-FR-BAQY: the typed refusals. An id outside the
// collection is unknown however it is spelled, and a path is never an id.
#[test]
fn reads_refuse_with_typed_errors() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.write("refs/b.pdf", "b");
    let secret = docs.write("secret.md", "not selected");
    docs.add_folder("refs");
    docs.refresh();
    let md = docs.id_of("refs/a.md");
    let pdf = docs.id_of("refs/b.pdf");

    fn code<T>(r: Result<T, crate::documents::DocumentsError>) -> String {
        match r {
            Ok(_) => "ok".to_string(),
            Err(e) => e.code().to_string(),
        }
    }
    assert_eq!(code(docs.collection.read_document(&docs.fs, &pdf)), "wrong_format");
    assert_eq!(code(docs.collection.read_document_pdf(&docs.fs, &md)), "wrong_format");
    assert_eq!(code(docs.collection.read_document(&docs.fs, "doc-unknown")), "unknown_document");
    assert_eq!(
        code(docs.collection.read_document(&docs.fs, &secret.to_string_lossy())),
        "unknown_document",
        "a path is not an id"
    );
    assert_eq!(
        code(docs.collection.read_document(&docs.fs, &docs.id_of("secret.md"))),
        "unknown_document",
        "a file outside the sources has no id in the collection"
    );

    std::fs::remove_file(docs.root.join("refs/a.md")).unwrap();
    assert_eq!(
        code(docs.collection.read_document(&docs.fs, &md)),
        "unavailable",
        "a read of a file that vanished fails as unavailable"
    );
    docs.refresh();
    assert_eq!(code(docs.collection.read_document(&docs.fs, &md)), "unknown_document");
}

// DCL-FR-TEUK: `read_document_pdf` returns the whole PDF content, base64 encoded,
// with the revision of those bytes.
#[test]
fn a_pdf_is_read_as_base64_bytes() {
    let docs = Docs::new();
    let bytes: Vec<u8> = (0..=255u8).collect();
    let path = docs.root.join("paper.pdf");
    std::fs::write(&path, &bytes).unwrap();
    docs.add_file("paper.pdf");
    docs.refresh();
    let read = docs
        .collection
        .read_document_pdf(&docs.fs, &docs.id_of("paper.pdf"))
        .unwrap();
    assert_eq!(read.name, "paper.pdf");
    assert_eq!(read.revision, crate::fs::sha256_bytes(&bytes));
    assert_eq!(
        base64::engine::general_purpose::STANDARD.decode(read.bytes_base64).unwrap(),
        bytes
    );
}

// DCL-FR-GLUS: the text of a Markdown or text document is the file text, and the
// text of a PDF is what the extractor finds in its bytes.
#[test]
fn document_text_serves_files_and_extracted_pdf_text() {
    let docs = Docs::new();
    docs.write("a.md", "markdown text");
    docs.write("b.pdf", "extracted pdf text");
    docs.add_folder(".");
    docs.refresh();
    let text = |name: &str| {
        docs.collection
            .document_text(&docs.fs, &docs.log, &docs.id_of(name))
            .unwrap()
    };
    assert_eq!(text("a.md"), "markdown text");
    assert_eq!(text("b.pdf"), "extracted pdf text");
}

// DCL-FR-GLUS: a PDF with no embedded text, or one that cannot be parsed, has no
// text and answers `no_text`.
#[test]
fn a_pdf_without_text_answers_no_text() {
    let empty = Docs::new();
    empty.write("scan.pdf", "   \n  ");
    empty.add_file("scan.pdf");
    empty.refresh();
    let err = empty
        .collection
        .document_text(&empty.fs, &empty.log, &empty.id_of("scan.pdf"))
        .unwrap_err();
    assert_eq!(err.code(), "no_text");

    let broken = Docs::with_extractor(extract_with_failure);
    broken.write("broken.pdf", "%PDF-garbage");
    broken.add_file("broken.pdf");
    broken.refresh();
    let err = broken
        .collection
        .document_text(&broken.fs, &broken.log, &broken.id_of("broken.pdf"))
        .unwrap_err();
    assert_eq!(err.code(), "no_text");
}

// DCL-FR-VRTS, DCL-FR-TAGV: a panic inside the extractor is caught and reported as
// `no_text`, with a WARN that names the document id and carries no path, name, or
// content. It fails neither the pass nor the command.
#[test]
fn an_extractor_that_panics_is_contained() {
    let docs = Docs::with_extractor(extract_with_panic);
    docs.write("secret-name.pdf", "secret content");
    docs.add_file("secret-name.pdf");
    docs.refresh();
    let id = docs.id_of("secret-name.pdf");
    let err = docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap_err();
    assert_eq!(err.code(), "no_text");

    // The index feed survives it too, and reports the document as having no text.
    let indexed = docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap();
    assert_eq!(indexed.len(), 1);
    assert!(indexed[0].text.is_err());

    let records = docs.log.records();
    let warn = records
        .iter()
        .find(|r| r.0 == LogLevel::Warn && r.1.contains("pdf"))
        .expect("a WARN record for the pdf");
    assert_eq!(warn.2.get("document").and_then(|v| v.as_str()), Some(id.as_str()));
    let everything = docs.log.everything();
    assert!(!everything.contains("secret-name"));
    assert!(!everything.contains("secret content"));
    assert!(!everything.contains(&docs.root.to_string_lossy().to_string()));
}

// DCL-FR-QVYZ: the extracted text is cached for the session by id and revision,
// so a later read reuses it, and a changed revision extracts again.
#[test]
fn pdf_text_is_cached_per_revision() {
    let docs = Docs::new();
    let file = docs.write("paper.pdf", "first text");
    docs.add_file("paper.pdf");
    docs.refresh();
    let id = docs.id_of("paper.pdf");
    let read = || docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap();

    assert_eq!(read(), "first text");
    assert_eq!(read(), "first text");
    let indexed = docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap();
    assert_eq!(indexed[0].text, Ok("first text".to_string()));
    assert_eq!(docs.collection.extraction_count(), 1, "one extraction for one revision");

    mtime_gap();
    std::fs::write(&file, "second text!").unwrap();
    // Without any refresh in between, the next read sees the new content.
    assert_eq!(read(), "second text!");
    assert_eq!(docs.collection.extraction_count(), 2);
    docs.refresh();
    assert_eq!(read(), "second text!");
    assert_eq!(docs.collection.extraction_count(), 2, "the refresh reused the cache");
}

// DCL-FR-QVYZ, DCL-FR-XHSJ: closing the project keeps the PDF cache for the
// application session, so reopening reuses it for an unchanged PDF.
#[test]
fn the_pdf_cache_survives_a_close() {
    let docs = Docs::new();
    docs.write("paper.pdf", "cached text");
    docs.add_file("paper.pdf");
    docs.refresh();
    let id = docs.id_of("paper.pdf");
    docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap();
    assert_eq!(docs.collection.extraction_count(), 1);

    let stored = docs.collection.stored_sources();
    docs.collection.close();
    docs.collection.open("project-key", Ok(stored));
    docs.refresh();
    let text = docs.collection.document_text(&docs.fs, &docs.log, &id).unwrap();
    assert_eq!(text, "cached text");
    assert_eq!(docs.collection.extraction_count(), 1);
}

// DCL-FR-GLUS: an unavailable document returns no content from any read.
#[test]
fn an_unavailable_document_returns_no_content() {
    let docs = Docs::new();
    docs.add_file("gone.md");
    docs.refresh();
    let id = docs.id_of("gone.md");
    for result in [
        docs.collection.read_document(&docs.fs, &id).map(|_| ()),
        docs.collection.document_text(&docs.fs, &docs.log, &id).map(|_| ()),
    ] {
        assert_eq!(result.unwrap_err().code(), "unavailable");
    }
    assert!(docs.collection.documents_for_index(&docs.fs, &docs.log).unwrap().is_empty());
}

// DCL-FR-BAQY: a read reaches only what a source selected. A document the walk
// never listed, a sibling of a selected file, and a removed source are all out of
// reach, because the instance is rebuilt from the sources.
#[test]
fn reads_stay_inside_the_selected_paths() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.write("refs/b.md", "b");
    docs.add_file("refs/a.md");
    docs.refresh();
    assert_eq!(docs.names(), ["a.md"], "the sibling is not in the collection");
    let access = docs.fs.documents().unwrap();
    assert!(access.read_bytes(docs.root.join("refs/a.md")).is_ok());
    assert!(access.read_bytes(docs.root.join("refs/b.md")).is_err());

    docs.remove("refs/a.md");
    docs.refresh();
    assert!(docs.fs.documents().is_none(), "no source, no reach");
    assert_eq!(
        docs.collection.read_document(&docs.fs, &docs.id_of("refs/a.md")).unwrap_err().code(),
        "unknown_document"
    );
}

/// A small, valid, single-page PDF whose page shows `text` in Helvetica.
fn minimal_pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 18 Tf 72 720 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref_at = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

// DCL-FR-GLUS, DCL-FR-VRTS: the real extractor, `pdf_extract`, finds the embedded
// text of a PDF read through the documents instance, and a file that is not a PDF
// answers `no_text` without failing the call.
#[test]
fn the_real_extractor_reads_embedded_pdf_text() {
    let docs = Docs::new();
    let collection = crate::documents::DocumentsCollection::default();
    collection.open("project-key", Ok(Vec::new()));
    std::fs::write(docs.root.join("real.pdf"), minimal_pdf("Hello Documents")).unwrap();
    std::fs::write(docs.root.join("fake.pdf"), b"this is not a pdf at all").unwrap();
    for name in ["real.pdf", "fake.pdf"] {
        collection
            .add_sources(
                &[StoredSource {
                    kind: SourceKind::File,
                    path: docs.path_of(name),
                }],
                &|_, _| Ok(()),
            )
            .unwrap();
    }
    collection.refresh(&docs.fs, &docs.log, &|_| {});
    let text = collection
        .document_text(&docs.fs, &docs.log, &docs.id_of("real.pdf"))
        .unwrap();
    assert!(text.contains("Hello Documents"), "{text:?}");
    let err = collection
        .document_text(&docs.fs, &docs.log, &docs.id_of("fake.pdf"))
        .unwrap_err();
    assert_eq!(err.code(), "no_text");
    assert_eq!(docs.log.count(LogLevel::Warn), 1, "one WARN, for the file that is not a PDF");
    assert!(!docs.log.everything().contains("fake.pdf"));
}
