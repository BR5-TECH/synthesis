//! Extension matching, normalisation, and ids (DCL-FR-FGGU, DCL-FR-QKJO).

use crate::documents::model::{
    document_id, file_name_of, format_of, is_absolute_path, normalise_path, DocumentFormat,
};

// DCL-FR-FGGU: the four supported extensions, ignoring case, give their formats.
#[test]
fn supported_extensions_match_in_any_case() {
    for (name, format) in [
        ("a.pdf", DocumentFormat::Pdf),
        ("A.PDF", DocumentFormat::Pdf),
        ("Report.PdF", DocumentFormat::Pdf),
        ("notes.md", DocumentFormat::Markdown),
        ("NOTES.MD", DocumentFormat::Markdown),
        ("guide.markdown", DocumentFormat::Markdown),
        ("Guide.MarkDown", DocumentFormat::Markdown),
        ("plain.txt", DocumentFormat::Text),
        ("PLAIN.TXT", DocumentFormat::Text),
        ("many.dots.in.name.md", DocumentFormat::Markdown),
    ] {
        assert_eq!(format_of(name), Some(format), "{name}");
    }
}

// DCL-FR-FGGU: every other file is ignored.
#[test]
fn other_files_are_ignored() {
    for name in [
        "a.docx", "a.html", "a.rtf", "a.md.bak", "a.pdf.zip", "a", "a.", "mdfile", ".md", "pdf",
        "a.mdx", "a.text", "a.markdn",
    ] {
        assert_eq!(format_of(name), None, "{name}");
    }
}

// DCL-FR-QKJO: the id is `doc-` and 32 lowercase hexadecimal characters of the
// SHA-256 of the normalised path, so one path has one id.
#[test]
fn an_id_is_derived_from_the_normalised_path() {
    let id = document_id("/refs/a.md");
    assert!(id.starts_with("doc-"));
    let hex = &id["doc-".len()..];
    assert_eq!(hex.len(), 32);
    assert!(hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    let digest = crate::fs::sha256_bytes(b"/refs/a.md");
    assert_eq!(hex, &digest[..32]);
    assert_eq!(id, document_id("/refs/a.md"), "deterministic");
    assert_ne!(id, document_id("/refs/b.md"));
}

// DCL-FR-QKJO: normalisation is lexical, collapses dot segments, turns `\` into
// `/`, and drops a trailing separator.
#[test]
fn normalisation_does_not_consult_the_disk() {
    assert_eq!(normalise_path("/a/b/../c/./d"), "/a/c/d");
    assert_eq!(normalise_path("/a//b///c/"), "/a/b/c");
    assert_eq!(normalise_path("/a/b/"), "/a/b");
    assert_eq!(normalise_path("C:\\Users\\me\\ref.md"), "C:/Users/me/ref.md");
    assert_eq!(normalise_path("/../../x"), "/x", "never climbs above the root");
    assert_eq!(
        document_id(&normalise_path("/a/b/../c.md")),
        document_id(&normalise_path("/a/c.md")),
        "two spellings of one path give one id"
    );
    assert_eq!(file_name_of("/a/b/c.md"), "c.md");
}

// DCL-FR-PHYP: a path is absolute in either syntax, judged on its text alone.
#[test]
fn absolute_paths_are_judged_on_their_text() {
    assert!(is_absolute_path("/a/b"));
    assert!(is_absolute_path("C:\\a"));
    assert!(is_absolute_path("c:/a"));
    assert!(!is_absolute_path("a/b"));
    assert!(!is_absolute_path("./a"));
    assert!(!is_absolute_path("C:a"));
    assert!(!is_absolute_path(""));
}
