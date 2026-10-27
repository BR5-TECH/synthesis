//! The documents layer at service level, with a real Documents collection
//! beside the BM25 indexer (EMB-FR-JIJH, EMB-FR-IBXK, EMB-FR-PYCS).

use crate::documents::{DocumentsCollection, SourceKind, StoredSource};

use super::*;

const MANUAL: &str = "# Manual\n\nRate limits of the public interface.";
const GUIDE: &str = "# Guide\n\nHow a release is cut.";

/// A Documents collection with its files outside the project.
struct Docs {
    dir: tempfile::TempDir,
}

impl Docs {
    /// Manage a collection and the filesystem access it reads through. The
    /// "PDF" extractor reads the bytes as text.
    fn manage(h: &Harness) -> Docs {
        h.app.manage(crate::fs::FsAccessState::default());
        h.app.manage(DocumentsCollection::with_extractor(|bytes| {
            String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
        }));
        Docs { dir: tempfile::tempdir().unwrap() }
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.dir.path().join(name), text).unwrap();
    }

    fn folder(&self) -> StoredSource {
        StoredSource {
            kind: SourceKind::Folder,
            path: std::fs::canonicalize(self.dir.path()).unwrap().to_string_lossy().into_owned(),
        }
    }

    /// Open the collection with `sources`, as a project open does, with no
    /// refresh: until one ends, the collection lists no documents.
    fn open(h: &Harness, sources: Vec<StoredSource>) {
        h.app.state::<DocumentsCollection>().open("project", Ok(sources));
    }

    /// One refresh on the test thread, so no background work is left.
    fn refresh(h: &Harness) {
        let fs = h.app.state::<crate::fs::FsAccessState>();
        h.app.state::<DocumentsCollection>().refresh(&fs, h.handle(), &|_| {});
    }

    /// Enable, open the collection with the documents folder, read it, mount,
    /// index, and settle.
    fn ready(&self, h: &Harness) {
        Docs::open(h, vec![self.folder()]);
        Docs::refresh(h);
        h.ready();
    }
}

fn document_chunks(h: &Harness) -> usize {
    h.service().status().project.unwrap().document_chunks
}

// EMB-FR-JIJH, EMB-FR-UTWM: on a project open, a pass that runs before the
// collection read its sources keeps the documents layer and its vectors. The
// pass after the read compares them by hash and embeds nothing.
#[test]
fn a_project_open_keeps_the_document_vectors_until_the_collection_is_read() {
    let h = Harness::new();
    let docs = Docs::manage(&h);
    docs.write("manual.md", MANUAL);
    docs.ready(&h);
    assert_eq!(document_chunks(&h), 1);
    let embedded = h.embedded();

    h.service().unmount();
    h.app.state::<DocumentsCollection>().close();
    Docs::open(&h, vec![docs.folder()]);
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(document_chunks(&h), 1, "the stored layer stays");
    assert_eq!(h.embedded(), embedded);

    Docs::refresh(&h);
    let jobs = h.jobs().len();
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::Incremental]);
    assert_eq!(h.embedded(), embedded, "the kept vectors serve the read documents");
    assert_eq!(document_chunks(&h), 1);
}

// EMB-FR-JIJH, EMB-FR-HBLQ: sources removed while the application was closed
// empty the documents layer once the collection is read.
#[test]
fn documents_removed_while_closed_leave_the_layer_after_the_read() {
    let h = Harness::new();
    let docs = Docs::manage(&h);
    docs.write("manual.md", MANUAL);
    docs.ready(&h);
    assert_eq!(document_chunks(&h), 1);

    h.service().unmount();
    h.app.state::<DocumentsCollection>().close();
    Docs::open(&h, Vec::new());
    Docs::refresh(&h);
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(document_chunks(&h), 0);
}

// EMB-FR-IBXK: an added document is embedded, a changed one is compared by
// chunk hash, and a removed one loses its entries.
#[test]
fn a_documents_change_reaches_the_documents_layer() {
    let h = Harness::new();
    let docs = Docs::manage(&h);
    docs.write("manual.md", MANUAL);
    docs.ready(&h);
    let embedded = h.embedded();

    docs.write("guide.md", GUIDE);
    Docs::refresh(&h);
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(document_chunks(&h), 2);
    assert_eq!(h.embedded() - embedded, 1, "the added document only");

    docs.write("guide.md", "# Guide\n\nHow a release is cut and signed.");
    Docs::refresh(&h);
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.embedded() - embedded, 2, "the changed chunk only");

    std::fs::remove_file(docs.dir.path().join("guide.md")).unwrap();
    Docs::refresh(&h);
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(document_chunks(&h), 1);
}

// EMB-FR-PYCS, EMB-FR-NHQG: a branch change keeps the one documents layer.
#[test]
fn a_branch_change_keeps_the_documents_layer() {
    let h = Harness::new();
    let docs = Docs::manage(&h);
    docs.write("manual.md", MANUAL);
    docs.ready(&h);
    let layer = |h: &Harness| {
        h.service().lock().project.as_ref().unwrap().outcome.as_ref().unwrap().documents_layer
    };
    let before = layer(&h);
    assert!(before.is_some());

    let head = h.repo.head().unwrap().peel_to_commit().unwrap();
    h.repo.branch("feature", &head, false).unwrap();
    h.repo.set_head("refs/heads/feature").unwrap();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::BranchUpdate));
    assert_eq!(layer(&h), before);
    assert_eq!(document_chunks(&h), 1);
}
