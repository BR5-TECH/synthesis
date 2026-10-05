//! Adding and removing sources, and the refusals of a closed collection
//! (DCL-FR-PHYP, DCL-FR-ATNL, DCL-FR-MSHW, DCL-FR-PTRP, DCL-FR-VEVZ).

use super::*;
use crate::documents::model::DocumentsError;

fn source(kind: SourceKind, path: &str) -> StoredSource {
    StoredSource {
        kind,
        path: path.to_string(),
    }
}

// DCL-FR-PHYP: a source whose normalised path equals a stored one adds nothing,
// and the stored order is the order of first addition.
#[test]
fn adding_a_known_path_adds_nothing_and_order_is_first_addition() {
    let docs = Docs::new();
    let persisted = Mutex::new(Vec::<usize>::new());
    let persist = |_key: &str, list: &[StoredSource]| {
        persisted.lock().unwrap().push(list.len());
        Ok(())
    };
    let a = source(SourceKind::Folder, "/refs/a");
    let b = source(SourceKind::File, "/refs/b.md");
    let added = docs.collection.add_sources(&[a.clone(), b.clone()], &persist).unwrap();
    assert_eq!(added, [a.clone(), b.clone()]);
    let again = source(SourceKind::Folder, "/refs/x/../a/");
    let added = docs.collection.add_sources(&[again, b.clone()], &persist).unwrap();
    assert!(added.is_empty(), "the same normalised path adds nothing");
    assert_eq!(*persisted.lock().unwrap(), [2], "nothing is stored for a repeat");
    let c = source(SourceKind::Folder, "/refs/c");
    docs.collection.add_sources(&[c.clone()], &persist).unwrap();
    assert_eq!(docs.collection.stored_sources(), [a, b, c]);
}

// DCL-FR-PHYP: a path that is not absolute is refused as `invalid_path`, and
// nothing is stored, not even the valid sources of the same call.
#[test]
fn a_relative_path_is_refused_and_stores_nothing() {
    let docs = Docs::new();
    let stored = Mutex::new(false);
    let result = docs.collection.add_sources(
        &[
            source(SourceKind::File, "/refs/ok.md"),
            source(SourceKind::File, "relative/path.md"),
        ],
        &|_, _| {
            *stored.lock().unwrap() = true;
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err(), DocumentsError::InvalidPath);
    assert_eq!(DocumentsError::InvalidPath.code(), "invalid_path");
    assert!(!*stored.lock().unwrap());
    assert!(docs.collection.stored_sources().is_empty());
}

// DCL-FR-ATNL: a path that matches no source removes nothing and is not an error,
// and a match is made on the normalised path.
#[test]
fn removing_matches_on_the_normalised_path() {
    let docs = Docs::new();
    docs.collection
        .add_sources(
            &[source(SourceKind::Folder, "/refs/a"), source(SourceKind::File, "/refs/b.md")],
            &|_, _| Ok(()),
        )
        .unwrap();
    let none = docs.collection.remove_source("/refs/other", &|_, _| Ok(())).unwrap();
    assert!(none.is_none());
    assert_eq!(docs.collection.stored_sources().len(), 2);
    let removed = docs.collection.remove_source("/refs/x/../a/", &|_, _| Ok(())).unwrap();
    assert_eq!(removed, Some(source(SourceKind::Folder, "/refs/a")));
    assert_eq!(docs.collection.stored_sources(), [source(SourceKind::File, "/refs/b.md")]);
}

// DCL-FR-MSHW: a store that cannot be written changes nothing in memory.
#[test]
fn a_failed_store_write_changes_nothing() {
    let docs = Docs::new();
    let result = docs.collection.add_sources(
        &[source(SourceKind::File, "/refs/a.md")],
        &|_, _| Err("disk full".to_string()),
    );
    assert_eq!(result.unwrap_err(), DocumentsError::StoreUnavailable);
    assert!(docs.collection.stored_sources().is_empty());
}

// DCL-FR-MSHW: with no project open every operation refuses with
// `no_project_open`.
#[test]
fn a_closed_collection_refuses_every_operation() {
    let docs = Docs::new();
    docs.collection.close();
    assert_eq!(docs.collection.snapshot().unwrap_err().code(), "no_project_open");
    assert_eq!(
        docs.collection
            .add_sources(&[source(SourceKind::File, "/a.md")], &|_, _| Ok(()))
            .unwrap_err()
            .code(),
        "no_project_open"
    );
    assert_eq!(
        docs.collection.remove_source("/a.md", &|_, _| Ok(())).unwrap_err().code(),
        "no_project_open"
    );
    assert_eq!(
        docs.collection.read_document(&docs.fs, "doc-0").unwrap_err().code(),
        "no_project_open"
    );
    assert_eq!(
        docs.collection.read_document_pdf(&docs.fs, "doc-0").unwrap_err().code(),
        "no_project_open"
    );
    assert_eq!(
        docs.collection.resolve_document("doc-0").unwrap_err().code(),
        "no_project_open"
    );
}

// DCL-FR-MSHW: a store that could not be read shows no documents rather than an
// empty store, and the operations say so.
#[test]
fn an_unreadable_store_shows_no_documents() {
    let docs = Docs::new();
    docs.collection.open("project-key", Err(()));
    assert_eq!(docs.collection.snapshot().unwrap_err().code(), "store_unavailable");
    assert_eq!(
        docs.collection
            .add_sources(&[source(SourceKind::File, "/a.md")], &|_, _| Ok(()))
            .unwrap_err()
            .code(),
        "store_unavailable"
    );
    let announced = docs.refresh();
    assert!(announced.is_empty(), "a refresh over an unreadable store commits nothing");
}

// DCL-FR-PTRP: a refresh announces a snapshot only when it differs from the
// previous one, and a change of revision, status, document set, or source status
// counts as a difference.
#[test]
fn a_changed_snapshot_is_announced_and_an_unchanged_one_is_not() {
    let docs = Docs::new();
    let file = docs.write("refs/a.md", "one");
    docs.add_folder("refs");
    assert_eq!(docs.refresh().len(), 1, "the first refresh adds a document");
    assert!(docs.refresh().is_empty(), "nothing changed");

    std::fs::write(&file, "two two").unwrap();
    let announced = docs.refresh();
    assert_eq!(announced.len(), 1, "a new revision is a difference");
    assert_eq!(
        announced[0].documents[0].revision.as_deref(),
        Some(crate::fs::sha256_bytes(b"two two").as_str())
    );

    docs.write("refs/b.md", "b");
    assert_eq!(docs.refresh().len(), 1, "a new document is a difference");

    std::fs::remove_file(&file).unwrap();
    let announced = docs.refresh();
    assert_eq!(announced.len(), 1, "a vanished document is a difference");
    assert_eq!(announced[0].documents.len(), 1);

    docs.add_folder("missing");
    assert_eq!(docs.refresh().len(), 1, "a new unavailable source is a difference");
}

// DCL-FR-ZYQC: a document whose content bytes changed gets a new revision, even
// when its size is unchanged.
#[test]
fn a_content_change_of_the_same_size_gives_a_new_revision() {
    let docs = Docs::new();
    let file = docs.write("a.md", "aaaa");
    docs.add_file("a.md");
    docs.refresh();
    let before = docs.snapshot().documents[0].revision.clone();
    // The modification time must move for the cheap check to notice.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&file, "bbbb").unwrap();
    docs.refresh();
    let after = docs.snapshot().documents[0].revision.clone();
    assert_ne!(before, after);
}
