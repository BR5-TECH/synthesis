//! The read-only grant and the documents profile
//! (FSA-FR-DMKC, FSA-FR-UKAE, FSA-FR-TXZU, FSA-FR-KAQV, FSA-FR-GHBN,
//! FSA-FR-WBKZ).
//!
//! Each refusal is checked two ways: the typed error came back, and the
//! filesystem is what it was, because an error that had already written
//! something would satisfy the first alone.

use super::*;
use crate::fs::{FsAccessState, GrantRefusal};

/// A selected folder with one document in it, one document beside it that is not
/// selected, and a file in a sibling folder.
struct Selection {
    _tmp: TempDir,
    root: PathBuf,
}

impl Selection {
    fn new() -> Selection {
        let tmp = TempDir::new().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        write_raw(&root.join("picked/inside.md"), "inside");
        write_raw(&root.join("picked/deep/deeper.txt"), "deeper");
        write_raw(&root.join("picked.md"), "the file named like the folder");
        write_raw(&root.join("other/outside.md"), "outside");
        write_raw(&root.join("secret.txt"), "secret");
        Selection { _tmp: tmp, root }
    }
}

// FSA-FR-DMKC, FSA-FR-GHBN: a folder grant reaches the folder and its subtree and
// nothing around it.
#[test]
fn a_folder_grant_reaches_its_subtree_and_nothing_else() {
    let s = Selection::new();
    let access = FsAccess::builder()
        .read_only_folder(s.root.join("picked"))
        .build()
        .unwrap();
    assert_eq!(access.read_text(s.root.join("picked/inside.md")).unwrap(), "inside");
    assert_eq!(access.read_text(s.root.join("picked/deep/deeper.txt")).unwrap(), "deeper");
    assert_eq!(access.list_dir(s.root.join("picked")).unwrap().len(), 2);
    for outside in ["picked.md", "other/outside.md", "secret.txt", "other"] {
        let result = access.read_bytes(s.root.join(outside));
        assert!(
            matches!(result, Err(FsError::EscapesAllowedRoots { .. })),
            "{outside} is outside the grant: {result:?}"
        );
    }
    assert!(matches!(
        access.list_dir(&s.root),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
}

// FSA-FR-DMKC, FSA-FR-GHBN: a file grant is that file and nothing beside or below it.
#[test]
fn a_file_grant_contains_its_exact_file_and_nothing_else() {
    let s = Selection::new();
    let access = FsAccess::builder()
        .read_only_file(s.root.join("picked/inside.md"))
        .build()
        .unwrap();
    assert_eq!(access.read_text(s.root.join("picked/inside.md")).unwrap(), "inside");
    for other in ["picked/deep/deeper.txt", "secret.txt", "other/outside.md"] {
        assert!(
            matches!(
                access.read_bytes(s.root.join(other)),
                Err(FsError::EscapesAllowedRoots { .. })
            ),
            "{other} is not the granted file"
        );
    }
    assert!(matches!(
        access.list_dir(s.root.join("picked")),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
}

// FSA-FR-10, FSA-FR-GHBN: containment is judged after lexical normalisation, so a
// path that climbs out of the grant is refused by where it ends.
#[test]
fn a_path_that_climbs_out_of_a_grant_is_refused() {
    let s = Selection::new();
    let access = FsAccess::builder()
        .read_only_folder(s.root.join("picked"))
        .build()
        .unwrap();
    let sneaky = s.root.join("picked/../secret.txt");
    assert!(matches!(
        access.read_bytes(sneaky),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    // A path that climbs out and back in is judged by where it ends.
    let back_in = s.root.join("picked/deep/../inside.md");
    assert_eq!(access.read_text(back_in).unwrap(), "inside");
}

// FSA-FR-UKAE: every operation that writes is refused before any filesystem
// access, and nothing changes on disk.
#[test]
fn a_read_only_instance_refuses_every_write_and_changes_nothing() {
    let s = Selection::new();
    let access = FsAccess::builder()
        .read_only_folder(s.root.join("picked"))
        .build()
        .unwrap();
    assert!(access.is_read_only());
    let file = s.root.join("picked/inside.md");
    let new_file = s.root.join("picked/new.md");
    let results: Vec<(&str, Result<(), FsError>)> = vec![
        ("create_file", access.create_file(&new_file, CreateMode::Exclusive)),
        ("write_bytes_atomic", access.write_bytes_atomic(&file, b"changed")),
        ("append_lines", access.append_lines(&file, &["line".to_string()])),
        ("create_dir", access.create_dir(s.root.join("picked/dir"))),
        ("rename_path", access.rename_path(&file, "renamed.md")),
        ("move_path", access.move_path(&file, s.root.join("picked/moved.md"))),
        ("copy_path", access.copy_path(&file, &new_file)),
        ("delete_path", access.delete_path(&file, false)),
        ("delete_owned_tree", access.delete_owned_tree(s.root.join("picked/deep"))),
        ("open Append", access.open_file(&file, OpenMode::Append).map(|_| ())),
        ("open Truncate", access.open_file(&file, OpenMode::Truncate).map(|_| ())),
    ];
    for (name, result) in results {
        assert!(
            matches!(result, Err(FsError::ReadOnly { .. })),
            "{name} must be refused as read only: {result:?}"
        );
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), "inside");
    assert!(!new_file.exists());
    assert!(!s.root.join("picked/dir").exists());
    assert!(!s.root.join("picked/moved.md").exists());
    assert!(s.root.join("picked/deep/deeper.txt").exists());
    // Reading stays open, including through a handle opened for reading.
    assert!(access.open_file(&file, OpenMode::ReadOnly).is_ok());
    assert_eq!(access.sha256_file(&file).unwrap().len(), 64);
}

// FSA-FR-UKAE: a read-only grant and a read-write root never share one instance.
#[test]
fn a_read_only_grant_and_a_read_write_root_never_share_an_instance() {
    let s = Selection::new();
    let mixed = FsAccess::builder()
        .allow_root(&s.root)
        .read_only_folder(s.root.join("picked"))
        .build();
    assert!(matches!(mixed, Err(FsAccessBuildError::MixedProfile)));
    let with_temp = FsAccess::builder()
        .session_temp(true)
        .read_only_file(s.root.join("picked.md"))
        .build();
    assert!(matches!(with_temp, Err(FsAccessBuildError::MixedProfile)));
}

// FSA-FR-KAQV: a grant whose path is missing, the wrong kind, or a link is refused
// with a typed error that names the reason.
#[test]
fn a_grant_that_cannot_be_established_is_refused_with_its_reason() {
    let s = Selection::new();
    let refusal = |result: Result<FsAccess, FsAccessBuildError>| match result {
        Err(FsAccessBuildError::GrantRefused { reason, .. }) => reason,
        other => panic!("expected a refused grant, got {other:?}"),
    };
    assert_eq!(
        refusal(FsAccess::builder().read_only_file(s.root.join("missing.md")).build()),
        GrantRefusal::Missing
    );
    assert_eq!(
        refusal(
            FsAccess::builder()
                .read_only_folder(s.root.join("nowhere/at/all"))
                .build()
        ),
        GrantRefusal::Missing
    );
    assert_eq!(
        refusal(FsAccess::builder().read_only_file(s.root.join("picked")).build()),
        GrantRefusal::Unreadable
    );
    assert_eq!(
        refusal(
            FsAccess::builder()
                .read_only_folder(s.root.join("picked.md"))
                .build()
        ),
        GrantRefusal::Unreadable
    );
}

// FSA-FR-KAQV, FSA-FR-17: a granted path that is itself a link is refused, and a
// link inside a granted folder is never followed out of it.
#[cfg(unix)]
#[test]
fn a_link_cannot_carry_a_read_out_of_a_grant() {
    let s = Selection::new();
    symlink(s.root.join("secret.txt"), s.root.join("picked/escape.txt")).unwrap();
    symlink(s.root.join("other"), s.root.join("picked/escape-dir")).unwrap();
    symlink(s.root.join("picked.md"), s.root.join("linked.md")).unwrap();

    // The granted path is itself a link.
    assert!(matches!(
        FsAccess::builder().read_only_file(s.root.join("linked.md")).build(),
        Err(FsAccessBuildError::GrantRefused {
            reason: GrantRefusal::Link,
            ..
        })
    ));

    let access = FsAccess::builder()
        .read_only_folder(s.root.join("picked"))
        .build()
        .unwrap();
    assert!(matches!(
        access.read_bytes(s.root.join("picked/escape.txt")),
        Err(FsError::SymlinkRefused { .. })
    ));
    assert!(matches!(
        access.read_bytes(s.root.join("picked/escape-dir/outside.md")),
        Err(FsError::SymlinkRefused { .. })
    ));
    // The link is listed as a link and is not followed to a target.
    let listing = access.list_dir(s.root.join("picked")).unwrap();
    let escape = listing.iter().find(|e| e.name == "escape.txt").unwrap();
    assert_eq!(escape.kind, EntryKind::Symlink);
}

// FSA-FR-18, FSA-FR-KAQV: the ancestors of a grant are canonicalised, so a grant
// reached through a link in its ancestors still contains the paths below it.
#[cfg(unix)]
#[test]
fn a_grant_reached_through_a_linked_ancestor_still_reaches_its_files() {
    let s = Selection::new();
    symlink(s.root.join("picked"), s.root.join("alias")).unwrap();
    let through_alias = s.root.join("alias/inside.md");
    let access = FsAccess::builder()
        .read_only_file(&through_alias)
        .build()
        .unwrap();
    assert_eq!(access.read_text(&through_alias).unwrap(), "inside");
    assert_eq!(access.read_text(s.root.join("picked/inside.md")).unwrap(), "inside");
}

// FSA-FR-TXZU: a selected path widens no other instance, and an agent session is
// built without it.
#[test]
fn a_selected_path_widens_no_other_instance() {
    let s = Selection::new();
    let project = TempDir::new().unwrap();
    let project_root = fs::canonicalize(project.path()).unwrap();
    write_raw(&project_root.join("readme.md"), "project");

    let state = FsAccessState::default();
    state.install_for_worktree(&project_root).unwrap();
    let refused = state.install_documents(&[], &[s.root.join("picked")]);
    assert!(refused.is_empty());
    state.open_agent_session("one").unwrap();

    let external = s.root.join("picked/inside.md");
    let documents = state.documents().expect("the documents instance");
    assert_eq!(documents.read_text(&external).unwrap(), "inside");
    // The documents instance holds no worktree root.
    assert!(matches!(
        documents.read_bytes(project_root.join("readme.md")),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    // The IDE instance and the agent instance gain no selected path.
    let ide = state.get().expect("the IDE instance");
    assert!(matches!(
        ide.read_bytes(&external),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    let agent = state.agent_session("one").expect("the agent instance");
    assert!(matches!(
        agent.read_bytes(&external),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    assert_eq!(agent.read_text(project_root.join("readme.md")).unwrap(), "project");
}

// FSA-FR-WBKZ: the instance is rebuilt from the sources as they stand, a removed
// source is refused from then on, a worktree change leaves it as it is, and a
// close discards it.
#[test]
fn the_documents_instance_follows_the_sources_and_outlives_a_worktree_change() {
    let s = Selection::new();
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let state = FsAccessState::default();
    assert!(state.documents().is_none(), "no source, no instance");

    let refused = state.install_documents(
        &[s.root.join("picked.md"), s.root.join("missing.md")],
        &[s.root.join("picked")],
    );
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].1, GrantRefusal::Missing);
    let documents = state.documents().unwrap();
    assert!(documents.read_text(s.root.join("picked.md")).is_ok());
    assert!(documents.read_text(s.root.join("picked/inside.md")).is_ok());

    // A worktree change builds a new IDE instance and leaves this one alone.
    state.install_for_worktree(first.path()).unwrap();
    state.install_for_worktree(second.path()).unwrap();
    assert!(state.documents().unwrap().read_text(s.root.join("picked.md")).is_ok());

    // The removed source is refused from the rebuild on.
    state.install_documents(&[], &[s.root.join("picked")]);
    let rebuilt = state.documents().unwrap();
    assert!(matches!(
        rebuilt.read_bytes(s.root.join("picked.md")),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    assert!(rebuilt.read_text(s.root.join("picked/inside.md")).is_ok());

    // With nothing left that can be granted, no instance exists.
    state.install_documents(&[], &[]);
    assert!(state.documents().is_none());
    state.install_documents(&[], &[s.root.join("picked")]);
    state.discard_documents();
    assert!(state.documents().is_none());
}
