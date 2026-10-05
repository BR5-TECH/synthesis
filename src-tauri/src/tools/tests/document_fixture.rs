//! The fixture the two document tools' tests share: a mounted, indexed project
//! with a Documents collection beside it, whose files live outside the project.

use std::path::PathBuf;

use tauri::Manager;
use tempfile::TempDir;

use super::{empty_project, Fixture};
use crate::documents::{DocumentsCollection, SourceKind, StoredSource};

/// The extractor the tests give the collection: the bytes of a "PDF" are its
/// extracted text.
pub(crate) fn extract_as_text(bytes: &[u8]) -> Result<String, String> {
    String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
}

pub(crate) struct DocFixture {
    pub(crate) fixture: Fixture,
    /// Where the selected documents live, outside the project.
    pub(crate) _docs: TempDir,
    pub(crate) root: PathBuf,
}

impl DocFixture {
    /// A project that is open with an empty Documents collection.
    pub(crate) fn new() -> DocFixture {
        DocFixture::with_extractor(extract_as_text)
    }

    pub(crate) fn with_extractor(extractor: crate::documents::manager::Extractor) -> DocFixture {
        let fixture = empty_project();
        fixture
            .app
            .manage(DocumentsCollection::with_extractor(extractor));
        fixture
            .app
            .state::<DocumentsCollection>()
            .open("project-key", Ok(Vec::new()));
        let docs = TempDir::new().unwrap();
        let root = std::fs::canonicalize(docs.path()).unwrap();
        DocFixture {
            fixture,
            _docs: docs,
            root,
        }
    }

    pub(crate) fn collection(&self) -> tauri::State<'_, DocumentsCollection> {
        self.fixture.app.state::<DocumentsCollection>()
    }

    pub(crate) fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    pub(crate) fn select(&self, kind: SourceKind, relative: &str) {
        let source = StoredSource {
            kind,
            path: self.root.join(relative).to_string_lossy().into_owned(),
        };
        self.collection()
            .add_sources(&[source], &|_, _| Ok(()))
            .expect("the source is added");
        self.sync();
    }

    /// Refresh the collection and run an index pass, as the application does
    /// after a change.
    pub(crate) fn sync(&self) {
        let fs = self.fixture.app.state::<crate::fs::FsAccessState>();
        self.collection()
            .refresh(&fs, &self.fixture.handle(), &|_| {});
        self.fixture.reindex();
    }

    /// The id the collection gave a file.
    pub(crate) fn id_of(&self, relative: &str) -> String {
        crate::documents::document_id(&crate::documents::normalise_path(
            &self.root.join(relative).to_string_lossy(),
        ))
    }
}
