//! The stored references (DCL-FR-HNRM, DCL-FR-VEVZ, GSS-FR-CDYK, GSS-FR-ZWJW).

use super::*;
use crate::global_settings::GlobalSettingsStore;

fn sources() -> Vec<StoredSource> {
    vec![
        StoredSource {
            kind: SourceKind::Folder,
            path: "/refs/manuals".to_string(),
        },
        StoredSource {
            kind: SourceKind::File,
            path: "/elsewhere/paper.pdf".to_string(),
        },
        StoredSource {
            kind: SourceKind::File,
            path: "/work/project/docs/in-project.md".to_string(),
        },
    ]
}

// DCL-FR-HNRM, GSS-FR-CDYK: the references persist across launches, in the order
// they were first added, exactly as selected.
#[test]
fn the_sources_persist_across_a_relaunch_in_stored_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        assert!(store.load_document_sources("/work/project").unwrap().is_empty());
        store.save_document_sources("/work/project", sources()).unwrap();
    }
    let relaunched = GlobalSettingsStore::with_path(path);
    assert_eq!(relaunched.load_document_sources("/work/project").unwrap(), sources());
}

// DCL-FR-HNRM, GSS-FR-ZWJW: the list belongs to the repository, so one project key
// serves every worktree of it, and another project sees none of it.
#[test]
fn the_sources_are_keyed_by_project_and_shared_by_its_worktrees() {
    let store = GlobalSettingsStore::in_memory();
    store.save_document_sources("/repo", sources()).unwrap();
    // A change of active worktree keeps the project key, so it reads one list.
    assert_eq!(store.load_document_sources("/repo").unwrap(), sources());
    assert!(store.load_document_sources("/another-repo").unwrap().is_empty());
}

// GSS-FR-CDYK, DCL-FR-VEVZ: a path inside a worktree stays the exact path that was
// selected, and an external one stays absolute.
#[test]
fn a_selected_path_is_kept_exactly() {
    let store = GlobalSettingsStore::in_memory();
    store.save_document_sources("/repo", sources()).unwrap();
    let loaded = store.load_document_sources("/repo").unwrap();
    assert_eq!(loaded[2].path, "/work/project/docs/in-project.md");
    assert_eq!(loaded[1].path, "/elsewhere/paper.pdf");
}

// DCL-FR-VEVZ, GSS-FR-CDYK: the stored file holds paths and kinds and no document
// content, and it is the user-global store, never a file of the project.
#[test]
fn the_store_holds_references_only() {
    let dir = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let doc = project.path().join("secret.md");
    std::fs::write(&doc, "THE CONTENT OF THE DOCUMENT").unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    store
        .save_document_sources(
            "/repo",
            vec![StoredSource {
                kind: SourceKind::File,
                path: doc.to_string_lossy().into_owned(),
            }],
        )
        .unwrap();
    let toml = std::fs::read_to_string(&path).unwrap();
    assert!(toml.contains("documentSources"), "{toml}");
    assert!(toml.contains("secret.md"));
    assert!(!toml.contains("THE CONTENT"));
    assert!(
        std::fs::read_dir(project.path()).unwrap().count() == 1,
        "nothing was written into the project"
    );
}

// GSS-FR-CDYK: the array of tables coexists with the other fields of the slot, so
// writing the sources neither loses them nor is lost by them.
#[test]
fn the_sources_coexist_with_the_other_per_project_facts() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_project_layout("/repo", crate::layout::LayoutPreferences::default())
            .unwrap();
        store.save_document_sources("/repo", sources()).unwrap();
        store
            .save_project_agent_enrolment("/repo", vec!["agent-1".to_string()])
            .unwrap();
    }
    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(reloaded.load_document_sources("/repo").unwrap(), sources());
    assert_eq!(
        reloaded.load_project_agent_enrolment("/repo").unwrap(),
        vec!["agent-1".to_string()]
    );
    assert!(reloaded.load_project_layout("/repo").unwrap().is_some());
}
