//! The type sources: files and folders of the active worktree that carry the
//! `doc` type (DCL-FR-DKXQ, DCL-FR-TQZB, DCL-FR-NLCE, DCL-FR-HYAF,
//! DCL-FR-ATNL, DCL-FR-OWQA).
//!
//! The temporary tree is the worktree: its `.synthesis/library.toml` holds the
//! assignments, written through the attribution write of ASC-FR-24.

use super::Docs;
use crate::documents::model::{SourceKind, SourceOrigin};
use crate::documents::typed::{clear_type_assignment, worktree_type_sources};
use crate::documents::DocumentsSnapshot;
use crate::fs::RootFs;
use crate::scanning::{self, ArtifactType, Scope};

impl Docs {
    fn worktree(&self) -> RootFs {
        RootFs::for_root(&self.root)
    }

    fn assign(&self, rel: &str, artifact_type: ArtifactType, scope: Scope) {
        scanning::assign(&self.worktree(), rel, artifact_type, scope).unwrap();
    }

    /// One refresh with the type sources of the worktree, as the application
    /// runs it.
    fn refresh_typed(&self) -> Vec<DocumentsSnapshot> {
        let worktree = self.worktree();
        let announced = std::sync::Mutex::new(Vec::new());
        let read = || worktree_type_sources(&worktree);
        self.collection.refresh_with(&self.fs, &self.log, &read, &|snapshot| {
            announced.lock().unwrap().push(snapshot.clone())
        });
        announced.into_inner().unwrap()
    }
}

// DCL-FR-DKXQ, DCL-FR-TQZB, DCL-FR-FGGU: a folder of the Document type includes
// every supported file below it that resolves to `doc`, and nothing is written
// to the manual store.
#[test]
fn a_doc_folder_is_a_source_of_its_supported_doc_files() {
    let docs = Docs::new();
    docs.write("guides/intro.md", "# Intro\n");
    docs.write("guides/deep/notes.txt", "notes\n");
    docs.write("guides/manual.pdf", "pdf text");
    docs.write("guides/image.png", "png");
    docs.write("other/outside.md", "# no\n");
    docs.assign("guides", ArtifactType::Doc, Scope::Folder);

    docs.refresh_typed();

    let snapshot = docs.snapshot();
    assert_eq!(docs.names(), vec!["notes.txt", "intro.md", "manual.pdf"]);
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].kind, SourceKind::Folder);
    assert_eq!(snapshot.sources[0].origin, SourceOrigin::Type);
    assert!(docs.collection.stored_sources().is_empty(), "no manual source is stored");
}

// DCL-FR-TQZB, ASC-FR-06: a file below a Document folder that carries another
// type, by assignment or by inference, is not a document of that folder.
#[test]
fn a_file_of_another_type_below_a_doc_folder_is_not_included() {
    let docs = Docs::new();
    docs.write("guides/intro.md", "# Intro\n");
    docs.write("guides/rules.md", "# Rules\n");
    docs.write("guides/CLAUDE.md", "# Instructions\n");
    docs.write("guides/plan.md", "---\ntype: spec\n---\n# Plan\n");
    docs.assign("guides", ArtifactType::Doc, Scope::Folder);
    docs.assign("guides/rules.md", ArtifactType::Spec, Scope::File);

    docs.refresh_typed();

    assert_eq!(docs.names(), vec!["intro.md"]);
}

// DCL-FR-TQZB, DCL-FR-DKXQ: a Document file of an unsupported type gives no
// document and no unavailable entry, but it is still a source.
#[test]
fn a_doc_file_of_an_unsupported_type_is_no_document() {
    let docs = Docs::new();
    docs.write("design.docx", "binary");
    docs.assign("design.docx", ArtifactType::Doc, Scope::File);

    docs.refresh_typed();

    let snapshot = docs.snapshot();
    assert!(snapshot.documents.is_empty());
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].kind, SourceKind::File);
}

// DCL-FR-DKXQ / ASC-FR-24: an assignment whose path does not exist gives no
// source.
#[test]
fn an_assignment_for_a_missing_path_gives_no_source() {
    let docs = Docs::new();
    let path = docs.write("gone.md", "x");
    docs.assign("gone.md", ArtifactType::Doc, Scope::File);
    std::fs::remove_file(path).unwrap();

    docs.refresh_typed();

    assert!(docs.snapshot().sources.is_empty());
}

// DCL-FR-NLCE, DCL-FR-YWCP: manual sources come first in stored order, then the
// type sources by path. A path that is both is listed once with the origin
// `both`, and a file reached twice is one document.
#[test]
fn sources_list_manual_first_and_merge_a_shared_path() {
    let docs = Docs::new();
    docs.write("b/two.md", "two");
    docs.write("a/one.md", "one");
    docs.write("shared/three.md", "three");
    docs.write("manual.txt", "m");
    docs.add_file("manual.txt");
    docs.add_folder("shared");
    docs.assign("shared", ArtifactType::Doc, Scope::Folder);
    docs.assign("b", ArtifactType::Doc, Scope::Folder);
    docs.assign("a", ArtifactType::Doc, Scope::Folder);

    docs.refresh_typed();

    let snapshot = docs.snapshot();
    let listed: Vec<(String, SourceOrigin)> = snapshot
        .sources
        .iter()
        .map(|s| (s.path.rsplit('/').next().unwrap().to_string(), s.origin))
        .collect();
    assert_eq!(
        listed,
        vec![
            ("manual.txt".to_string(), SourceOrigin::Manual),
            ("shared".to_string(), SourceOrigin::Both),
            ("a".to_string(), SourceOrigin::Type),
            ("b".to_string(), SourceOrigin::Type),
        ]
    );
    let three = snapshot.documents.iter().filter(|d| d.name == "three.md").count();
    assert_eq!(three, 1, "a file two sources include is one document");
}

// DCL-FR-HYAF: a document below the worktree root carries its project path, and
// a document outside it carries none.
#[test]
fn a_document_in_the_worktree_carries_its_project_path() {
    let docs = Docs::new();
    docs.write("guides/intro.md", "# Intro\n");
    docs.assign("guides", ArtifactType::Doc, Scope::Folder);
    let outside = tempfile::TempDir::new().unwrap();
    let outside_root = std::fs::canonicalize(outside.path()).unwrap();
    std::fs::write(outside_root.join("ref.md"), "ref").unwrap();
    let source = crate::documents::StoredSource {
        kind: SourceKind::File,
        path: outside_root.join("ref.md").to_string_lossy().into_owned(),
    };
    docs.collection.add_sources(&[source], &|_, _| Ok(())).unwrap();

    docs.refresh_typed();

    let snapshot = docs.snapshot();
    let intro = snapshot.documents.iter().find(|d| d.name == "intro.md").unwrap();
    assert_eq!(intro.project_path.as_deref(), Some("guides/intro.md"));
    let reference = snapshot.documents.iter().find(|d| d.name == "ref.md").unwrap();
    assert_eq!(reference.project_path, None);
}

// DCL-FR-RMVT, DCL-FR-PTRP: a cleared assignment takes the documents out of the
// next refresh, and the change is announced.
#[test]
fn a_cleared_assignment_shows_in_the_next_refresh() {
    let docs = Docs::new();
    docs.write("guides/intro.md", "# Intro\n");
    docs.assign("guides", ArtifactType::Doc, Scope::Folder);
    docs.refresh_typed();
    assert_eq!(docs.names(), vec!["intro.md"]);

    scanning::clear(&docs.worktree(), "guides").unwrap();
    let announced = docs.refresh_typed();

    assert_eq!(announced.len(), 1, "the changed snapshot is announced");
    assert!(docs.snapshot().documents.is_empty());
    assert!(docs.snapshot().sources.is_empty());
}

// DCL-FR-ATNL, DCL-FR-VEVZ: removing a type source clears its `doc` assignment
// and leaves the files on disk as they were.
#[test]
fn clearing_a_type_source_keeps_the_files() {
    let docs = Docs::new();
    let file = docs.write("guides/intro.md", "# Intro\n");
    docs.assign("guides", ArtifactType::Doc, Scope::Folder);
    docs.assign("specs", ArtifactType::Spec, Scope::Folder);

    let cleared = clear_type_assignment(&docs.worktree(), &docs.path_of("guides")).unwrap();

    assert!(cleared);
    let stored = scanning::load_assignments(&docs.worktree());
    assert!(!stored.assignments.contains_key("guides"));
    assert!(stored.assignments.contains_key("specs"), "other assignments stay");
    assert_eq!(std::fs::read_to_string(file).unwrap(), "# Intro\n");
}

// DCL-FR-ATNL: a path with no `doc` assignment, an assignment of another type,
// and a path outside the worktree change nothing.
#[test]
fn clearing_changes_nothing_without_a_doc_assignment() {
    let docs = Docs::new();
    docs.write("specs/a.md", "a");
    docs.assign("specs", ArtifactType::Spec, Scope::Folder);
    let root = docs.worktree();

    assert!(!clear_type_assignment(&root, &docs.path_of("specs")).unwrap());
    assert!(!clear_type_assignment(&root, &docs.path_of("nothing")).unwrap());
    assert!(!clear_type_assignment(&root, "/elsewhere/file.md").unwrap());
    assert!(scanning::load_assignments(&root).assignments.contains_key("specs"));
}

// DCL-FR-OWQA, DCL-FR-HNRM: a change of the active worktree keeps the manual
// sources and reads the type sources of the new worktree.
#[test]
fn a_worktree_change_reads_the_type_sources_of_the_new_worktree() {
    let docs = Docs::new();
    docs.write("a/guides/one.md", "one");
    docs.write("b/notes/two.md", "two");
    docs.write("manual.txt", "m");
    docs.add_file("manual.txt");
    let first = RootFs::for_root(docs.root.join("a"));
    let second = RootFs::for_root(docs.root.join("b"));
    scanning::assign(&first, "guides", ArtifactType::Doc, Scope::Folder).unwrap();
    scanning::assign(&second, "notes", ArtifactType::Doc, Scope::Folder).unwrap();

    let refresh = |root: &RootFs| {
        let read = || worktree_type_sources(root);
        docs.collection.refresh_with(&docs.fs, &docs.log, &read, &|_| {});
    };
    refresh(&first);
    assert_eq!(docs.names(), vec!["one.md", "manual.txt"]);

    refresh(&second);
    assert_eq!(docs.names(), vec!["two.md", "manual.txt"]);
    let two = docs.snapshot().documents.into_iter().find(|d| d.name == "two.md").unwrap();
    assert_eq!(two.project_path.as_deref(), Some("notes/two.md"));
    assert_eq!(docs.collection.stored_sources().len(), 1, "the manual source stays");
}

// DCL-FR-SQEP, DCL-FR-NLCE, DCL-FR-HYAF: the wire names the frontend reads.
#[test]
fn origin_and_project_path_cross_the_wire_by_their_contract_names() {
    use crate::documents::model::{Availability, DocumentEntry, DocumentFormat, DocumentSource};
    for (origin, text) in [
        (SourceOrigin::Manual, "manual"),
        (SourceOrigin::Type, "type"),
        (SourceOrigin::Both, "both"),
    ] {
        let source = DocumentSource {
            kind: SourceKind::Folder,
            path: "/w/guides".into(),
            origin,
            status: Availability::Available,
            reason: None,
        };
        assert_eq!(serde_json::to_value(&source).unwrap()["origin"], text);
    }
    let mut entry = DocumentEntry {
        id: "doc-x".into(),
        path: "/w/guides/a.md".into(),
        project_path: Some("guides/a.md".into()),
        name: "a.md".into(),
        format: DocumentFormat::Markdown,
        status: Availability::Available,
        revision: None,
    };
    assert_eq!(serde_json::to_value(&entry).unwrap()["project_path"], "guides/a.md");
    entry.project_path = None;
    assert!(serde_json::to_value(&entry).unwrap().get("project_path").is_none());
}

// DCL-FR-HYAF: a path that only shares a prefix with the root is not below it.
#[test]
fn a_sibling_folder_with_a_shared_prefix_is_not_in_the_worktree() {
    use crate::documents::typed::project_path_of;
    assert_eq!(project_path_of("/w/proj/a.md", Some("/w/proj")).as_deref(), Some("a.md"));
    assert_eq!(project_path_of("/w/project/a.md", Some("/w/proj")), None);
    assert_eq!(project_path_of("/w/proj", Some("/w/proj")), None);
    assert_eq!(project_path_of("/w/proj/a.md", None), None);
}
