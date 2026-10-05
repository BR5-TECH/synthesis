//! Recursive discovery, overlapping sources, availability, and the snapshot
//! (DCL-FR-TYOU, DCL-FR-YWCP, DCL-FR-SQEP, DCL-FR-EWPO, DCL-FR-QTGN,
//! DCL-FR-KMHY).

use super::*;
use crate::documents::model::{Availability, DocumentFormat, SourceReason};

// DCL-FR-TYOU, DCL-FR-KMHY, DCL-FR-FGGU: a folder source includes every
// supported file below it at any depth, and ignores the rest.
#[test]
fn a_folder_source_is_searched_recursively_for_supported_files() {
    let docs = Docs::new();
    docs.write("refs/top.md", "# top");
    docs.write("refs/sub/notes.TXT", "notes");
    docs.write("refs/sub/deep/er/paper.PDF", "paper");
    docs.write("refs/sub/guide.markdown", "guide");
    docs.write("refs/sub/image.png", "png");
    docs.write("refs/sub/deep/archive.zip", "zip");
    docs.write("outside.md", "not selected");
    docs.add_folder("refs");
    docs.refresh();

    assert_eq!(
        docs.names(),
        ["paper.PDF", "guide.markdown", "notes.TXT", "top.md"],
        "sorted by normalised path, supported types only"
    );
    let snapshot = docs.snapshot();
    assert!(snapshot.documents.iter().all(|d| d.status == Availability::Available));
    let pdf = snapshot.documents.iter().find(|d| d.name == "paper.PDF").unwrap();
    assert_eq!(pdf.format, DocumentFormat::Pdf);
}

// DCL-FR-FGGU: a selected file of an unsupported type is ignored too.
#[test]
fn a_file_source_of_an_unsupported_type_adds_no_document() {
    let docs = Docs::new();
    docs.write("a.docx", "doc");
    docs.write("b.md", "b");
    docs.add_file("a.docx");
    docs.add_file("b.md");
    docs.refresh();
    assert_eq!(docs.names(), ["b.md"]);
}

// DCL-FR-SQEP: the snapshot gives each document its id, path, name, format,
// status, and the SHA-256 of its content as its revision.
#[test]
fn a_document_entry_carries_its_id_name_format_and_revision() {
    let docs = Docs::new();
    docs.write("refs/a.md", "alpha");
    docs.add_folder("refs");
    docs.refresh();
    let entry = &docs.snapshot().documents[0];
    assert_eq!(entry.id, docs.id_of("refs/a.md"));
    assert_eq!(entry.path, normalise(&docs.path_of("refs/a.md")));
    assert_eq!(entry.name, "a.md");
    assert_eq!(entry.format, DocumentFormat::Markdown);
    assert_eq!(entry.status, Availability::Available);
    assert_eq!(entry.revision.as_deref(), Some(crate::fs::sha256_bytes(b"alpha").as_str()));
}

fn normalise(path: &str) -> String {
    crate::documents::model::normalise_path(path)
}

// DCL-FR-YWCP: a file reached by several sources is listed once, and stays while
// any one source includes it.
#[test]
fn overlapping_sources_list_a_file_once_and_keep_it_while_any_source_includes_it() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.write("refs/sub/b.md", "b");
    docs.add_folder("refs");
    docs.add_folder("refs/sub");
    docs.add_file("refs/sub/b.md");
    docs.add_file("refs/a.md");
    docs.refresh();
    assert_eq!(docs.names(), ["a.md", "b.md"], "one entry per file");

    docs.remove("refs");
    docs.refresh();
    assert_eq!(docs.names(), ["a.md", "b.md"], "b stays through its other sources, and a through its file source");

    docs.remove("refs/a.md");
    docs.refresh();
    assert_eq!(docs.names(), ["b.md"], "a left when no source included it");

    docs.remove("refs/sub");
    docs.refresh();
    assert_eq!(docs.names(), ["b.md"], "the file source still includes b");

    docs.remove("refs/sub/b.md");
    docs.refresh();
    assert!(docs.names().is_empty());
    assert!(docs.snapshot().sources.is_empty());
}

// DCL-FR-VEVZ, DCL-FR-ATNL: removing a source changes no file of the user.
#[test]
fn removing_a_source_leaves_the_files_alone() {
    let docs = Docs::new();
    let file = docs.write("refs/a.md", "a");
    docs.add_folder("refs");
    docs.refresh();
    docs.remove("refs");
    docs.refresh();
    assert!(docs.names().is_empty());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "a");
    assert!(docs.root.join("refs").is_dir());
}

// DCL-FR-EWPO: a source whose path is missing stays in the snapshot as
// unavailable with its reason and contributes no document.
#[test]
fn a_missing_source_is_kept_and_reported_unavailable() {
    let docs = Docs::new();
    docs.write("refs/a.md", "a");
    docs.add_folder("refs");
    docs.add_folder("gone");
    docs.add_file("gone.md");
    docs.refresh();

    let snapshot = docs.snapshot();
    assert_eq!(snapshot.sources.len(), 3);
    let gone = &snapshot.sources[1];
    assert_eq!(gone.status, Availability::Unavailable);
    assert_eq!(gone.reason, Some(SourceReason::Missing));
    assert_eq!(snapshot.sources[0].status, Availability::Available);
    // The unavailable file source keeps its own entry as an unavailable document.
    let entry = snapshot.documents.iter().find(|d| d.name == "gone.md").unwrap();
    assert_eq!(entry.status, Availability::Unavailable);
    assert_eq!(entry.revision, None);
    assert!(docs.log.count(LogLevel::Warn) >= 2, "each unavailable source is a WARN");
    assert!(!docs.log.everything().contains("gone"), "no path or name in a record");
}

// DCL-FR-EWPO: a source that comes back is available again, with no re-adding.
#[test]
fn a_source_that_reappears_becomes_available_again() {
    let docs = Docs::new();
    docs.add_file("later.md");
    docs.refresh();
    assert_eq!(docs.snapshot().documents[0].status, Availability::Unavailable);
    docs.write("later.md", "now here");
    docs.refresh();
    let entry = &docs.snapshot().documents[0];
    assert_eq!(entry.status, Availability::Available);
    assert!(entry.revision.is_some());
}

// DCL-FR-QTGN: a Markdown or text file that is not UTF-8 is an unavailable
// document that stays in the collection and returns no text.
#[test]
fn a_file_that_is_not_utf8_is_unavailable() {
    let docs = Docs::new();
    let path = docs.root.join("refs/bad.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).unwrap();
    docs.write("refs/good.txt", "good");
    docs.add_folder("refs");
    docs.refresh();
    let snapshot = docs.snapshot();
    let bad = snapshot.documents.iter().find(|d| d.name == "bad.txt").unwrap();
    assert_eq!(bad.status, Availability::Unavailable);
    assert_eq!(bad.revision, None);
    assert_eq!(
        docs.collection.read_document(&docs.fs, &bad.id).unwrap_err().code(),
        "unavailable"
    );
}

// DCL-FR-EWPO, DCL-FR-KMHY: a link is not followed and is not listed, and a source
// that is itself a link is unavailable.
#[cfg(unix)]
#[test]
fn links_are_neither_followed_nor_listed() {
    use std::os::unix::fs::symlink;
    let docs = Docs::new();
    docs.write("refs/real.md", "real");
    docs.write("elsewhere/secret.md", "secret");
    symlink(docs.root.join("elsewhere/secret.md"), docs.root.join("refs/link.md")).unwrap();
    symlink(docs.root.join("elsewhere"), docs.root.join("refs/linked-dir")).unwrap();
    symlink(docs.root.join("refs/real.md"), docs.root.join("alias.md")).unwrap();
    symlink(docs.root.join("refs"), docs.root.join("refs-alias")).unwrap();
    docs.add_folder("refs");
    docs.add_file("alias.md");
    docs.add_folder("refs-alias");
    docs.refresh();

    let snapshot = docs.snapshot();
    // The linked file source keeps its own entry, as an unavailable document.
    assert_eq!(docs.names(), ["alias.md", "real.md"]);
    let alias = snapshot.documents.iter().find(|d| d.name == "alias.md").unwrap();
    assert_eq!(alias.status, Availability::Unavailable);
    assert!(!snapshot.documents.iter().any(|d| d.name == "secret.md" || d.name == "link.md"));
    for relative in ["alias.md", "refs-alias"] {
        let source = snapshot
            .sources
            .iter()
            .find(|s| s.path.ends_with(relative))
            .unwrap();
        assert_eq!(source.status, Availability::Unavailable, "{relative}");
        assert_eq!(source.reason, Some(SourceReason::Link), "{relative}");
    }
}

// DCL-FR-KMHY, DCL-FR-UPFP: a subfolder that cannot be read costs that subfolder
// alone, with a WARN, and the walk continues.
#[cfg(unix)]
#[test]
fn an_unreadable_subfolder_is_skipped_with_a_warning() {
    use std::os::unix::fs::PermissionsExt;
    let docs = Docs::new();
    docs.write("refs/ok.md", "ok");
    docs.write("refs/locked/hidden.md", "hidden");
    let locked = docs.root.join("refs/locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = can_read_despite_mode_zero(&locked);
    docs.add_folder("refs");
    docs.refresh();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    if readable_anyway {
        return;
    }
    assert_eq!(docs.names(), ["ok.md"]);
    assert_eq!(docs.snapshot().sources[0].status, Availability::Available);
    assert_eq!(docs.log.count(LogLevel::Warn), 1);
    assert!(!docs.log.everything().contains("locked"));
}

// DCL-FR-KMHY: the order of discovery does not change the snapshot.
#[test]
fn the_snapshot_does_not_depend_on_the_order_of_sources() {
    let first = Docs::new();
    let second = Docs::new();
    for docs in [&first, &second] {
        docs.write("a/x.md", "x");
        docs.write("b/y.md", "y");
    }
    first.add_folder("a");
    first.add_folder("b");
    second.add_folder("b");
    second.add_folder("a");
    first.refresh();
    second.refresh();
    assert_eq!(first.names(), second.names());
    assert_eq!(first.names(), ["x.md", "y.md"]);
}
