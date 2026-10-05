//! Paste, delete, rename and copy: the file operations of PST-FR-18/19/20,
//! and the command surface they are reached through.
//!
//! One part of `../tests/mod.rs`.

use super::*;

#[test]
fn library_tree_command_functions_are_in_scope() {
    // Compile-time references: renaming/removing any Library tree command
    // without updating `generate_handler!` / `COMMAND_NAMES` fails to
    // compile here (the silent-failure class CLAUDE.md warns about).
    let _ = load_project_tree;
    let _ = rescan_project_tree;
    let _ = assign_artifact_type;
    let _ = clear_artifact_type;
}

#[test]
fn every_command_that_writes_an_assignment_publishes_the_classification() {
    // The wiring, not the pieces. `republish_classification` is what makes a
    // reclassification take effect synchronously: it invalidates the
    // candidate list, requests a BM25 pass (BMI-FR-17), and records the
    // attribution baseline so the watcher recognises this application's own
    // write rather than reporting it as an external one (ASC-FR-20).
    //
    // Asserted over the source because these are `#[tauri::command]` fns
    // taking a concrete `AppHandle<Wry>`, which a mock-runtime app cannot
    // construct — so every other test here exercises the pure `*_impl`
    // halves, and none of those makes this call. Dropping the line is
    // therefore a one-line edit that compiles clean and passes everything,
    // while costing the whole of what this call buys: a classification the
    // UI renders before the indexes have moved to match it, and an
    // application write the watcher then mistakes for a teammate's.
    //
    // The same idiom as `bm25_index`'s `graduation_announces_before_it_
    // releases_the_hold`, and for the same reason: the invariant is a call
    // site's existence, which has no runtime observable of its own here.
    const SOURCE: &str = include_str!("../../library.rs");
    let body_of = |marker: &str| -> String {
        let after = SOURCE
            .split_once(marker)
            .unwrap_or_else(|| panic!("the {marker} command"))
            .1;
        // The command's closing brace is at column 0, so the first "\n}\n"
        // ends the body.
        after
            .split_once("\n}\n")
            .expect("the end of the command body")
            .0
            .to_string()
    };

    for marker in [
        "pub fn assign_artifact_type(",
        "pub fn clear_artifact_type(",
        "pub fn create_folder(",
    ] {
        assert!(
            body_of(marker).contains("republish_classification("),
            "{marker} writes an assignment, so it must publish the \
                 classification that assignment changed"
        );
    }
}

#[test]
fn paste_destination_joins_basename_into_folder() {
    // PST-FR-20: paste computes `dest_folder/<basename(source)>`.
    assert_eq!(paste_destination("dst", "a/b/c.md"), "dst/c.md");
    // A trailing slash on the folder is normalized away.
    assert_eq!(paste_destination("dst/", "c.md"), "dst/c.md");
    // The project root (empty folder path) pastes at the top level.
    assert_eq!(paste_destination("", "a/b/c.md"), "c.md");
}

#[test]
fn delete_path_impl_removes_node_and_missing_is_error() {
    // PST-FR-19: delete removes the file; a missing path is a typed error.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes").join("x.md"), b"x").unwrap();
    delete_path_impl(root, "notes/x.md", false).unwrap();
    assert!(!root.join("notes").join("x.md").exists());
    assert!(delete_path_impl(root, "notes/gone.md", false).is_err());
}

// ASC-FR-09, PST-FR-18 / LCM-FR-11: the non-recursive delete is also the emptiness
// probe. A folder holding anything comes back as the typed `ERR_NOT_EMPTY`
// with nothing removed, which is the signal the Library turns into its
// recursive-delete confirmation.
#[test]
fn delete_path_impl_reports_not_empty_and_honours_consent() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs").join("a.md"), b"A").unwrap();

    let err = delete_path_impl(root, "docs", false).unwrap_err();
    assert_eq!(err, ERR_NOT_EMPTY, "the UI matches on this exact string");
    assert!(root.join("docs").join("a.md").exists(), "nothing removed");

    delete_path_impl(root, "docs", true).unwrap();
    assert!(!root.join("docs").exists());
}

// PST-FR-18: emptiness is read from the filesystem, not from the tree the
// Library renders — so a folder whose only contents the scan excludes still
// reports non-empty and still prompts. This is the whole reason the probe
// lives in the backend rather than in the panel.
#[test]
fn delete_path_impl_counts_entries_the_scan_would_hide() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join(".gitignore"), b"vendor/\n").unwrap();
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(root.join("vendor").join("lib.js"), b"x").unwrap();

    // The scan surfaces no node under `vendor/`, so the panel renders it as
    // holding nothing...
    let tree = scanning::scan(root);
    let vendor = find_node(&tree, "vendor");
    assert!(
        vendor.is_none_or(|n| n.children.as_ref().is_none_or(|c| c.is_empty())),
        "the scan must hide the gitignored contents for this test to mean anything"
    );
    // ...and the delete still refuses, because it asked the filesystem.
    assert_eq!(
        delete_path_impl(root, "vendor", false).unwrap_err(),
        ERR_NOT_EMPTY
    );
    assert!(root.join("vendor").join("lib.js").exists());
}

// PST-FR-19: renaming a node to what it is already called is its own typed
// error, not the sibling-collision the primitive would otherwise report, and
// it never reaches the filesystem.
#[test]
fn rename_path_impl_rejects_unchanged_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("a.md"), b"A").unwrap();
    std::fs::create_dir_all(root.join("notes")).unwrap();
    std::fs::write(root.join("notes").join("n.md"), b"N").unwrap();

    assert_eq!(rename_path_impl(root, root, "a.md", "a.md").unwrap_err(), ERR_UNCHANGED_NAME);
    assert_eq!(
        rename_path_impl(root, root, "notes/n.md", "n.md").unwrap_err(),
        ERR_UNCHANGED_NAME,
        "a nested path is compared on its basename, not its whole path"
    );
    // Untouched: the error was decided before any filesystem call.
    assert_eq!(std::fs::read(root.join("a.md")).unwrap(), b"A");
    // And it is distinguishable from the collision case.
    std::fs::write(root.join("b.md"), b"B").unwrap();
    assert_ne!(rename_path_impl(root, root, "a.md", "b.md").unwrap_err(), ERR_UNCHANGED_NAME);
}

#[test]
fn rename_path_impl_renames_and_rejects_collision_and_separator() {
    // PST-FR-20: rename within the parent; collision and path-separator are
    // typed errors that leave the source unchanged.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("a.md"), b"A").unwrap();
    std::fs::write(root.join("b.md"), b"B").unwrap();
    // Collision with an existing sibling.
    assert!(rename_path_impl(root, root, "a.md", "b.md").is_err());
    assert!(root.join("a.md").exists(), "source kept on collision");
    // Path separator in the new name.
    assert!(rename_path_impl(root, root, "a.md", "c/d.md").is_err());
    assert!(root.join("a.md").exists(), "source kept on invalid name");
    // Happy path.
    rename_path_impl(root, root, "a.md", "c.md").unwrap();
    assert!(!root.join("a.md").exists());
    assert_eq!(std::fs::read_to_string(root.join("c.md")).unwrap(), "A");
}

#[test]
fn renamed_path_keeps_the_entry_in_its_parent_folder() {
    assert_eq!(renamed_path("a.md", "b.md"), "b.md");
    assert_eq!(renamed_path("specs/a.md", "b.md"), "specs/b.md");
    assert_eq!(renamed_path("a/b/c.md", "d.md"), "a/b/d.md");
    assert_eq!(renamed_path("old/", "new"), "new");
    assert_eq!(renamed_path("a/old/", "new"), "a/new");
}

#[test]
fn a_rename_carries_the_renamed_entitys_notes_across() {
    // NTC-FR-11: an entity id is a project-relative path, so
    // without this the rename below would orphan the note.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join("specs/a.md"), b"A").unwrap();
    let note = crate::notes::create_note_in(
        root,
        crate::notes::NoteScope::Entity {
            entity_id: "specs/a.md".into(),
            entity_path: "specs/a.md".into(),
        },
        "still mine".into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    rename_path_impl(root, root, "specs/a.md", "b.md").unwrap();

    let moved = crate::notes::list_notes_for_entity_in(root, root, "specs/b.md");
    assert_eq!(moved.len(), 1, "the note followed the rename");
    assert_eq!(moved[0].note.id, note.id);
    assert!(!moved[0].unresolved);
    assert_eq!(
        moved[0].note.updated_at, "2026-01-01T00:00:00Z",
        "and the note's own edit time did not move"
    );
}

#[test]
fn a_rename_carries_the_renamed_artifacts_comment_threads_across() {
    // CMS-FR-24. A comment log is named from the artifact's path,
    // so without this seam a rename would strand every thread on the file —
    // the rail would come back empty with the conversation still on disk.
    // Driven through `rename_path_impl` rather than `follow_rename` directly,
    // because "the correlation is wired up at all" is the thing that breaks.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join("specs/a.md"), b"A").unwrap();
    let thread = crate::comments::open_artifact_fragment_in(
        root,
        "specs/a.md",
        crate::comments::FragmentTarget::in_artifact("", 0, 1, "A"),
        "still mine".into(),
        Vec::new(),
        &crate::comments::Participant::Human {
            login: "raver119".into(),
            display_name: None,
            email: None,
        },
        "2026-01-01T00:00:00Z")
    .unwrap();

    rename_path_impl(root, root, "specs/a.md", "b.md").unwrap();

    let moved = crate::comments::list_fragment_discussions_in(root, "specs/b.md");
    assert_eq!(moved.len(), 1, "the thread followed the rename");
    assert_eq!(moved[0].id, thread.id);
    assert_eq!(moved[0].comments[0].body, "still mine");
    assert_eq!(
        moved[0].updated_at, "2026-01-01T00:00:00Z",
        "the conversation did not change, so its time did not move"
    );
    assert!(crate::comments::list_fragment_discussions_in(root, "specs/a.md").is_empty());
}

#[test]
fn a_failed_rename_leaves_the_notes_alone() {
    // The follow runs only after the filesystem rename succeeded; a
    // collision must not silently rebind notes to a path nothing moved to.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("a.md"), b"A").unwrap();
    std::fs::write(root.join("b.md"), b"B").unwrap();
    crate::notes::create_note_in(
        root,
        crate::notes::NoteScope::Entity {
            entity_id: "a.md".into(),
            entity_path: "a.md".into(),
        },
        "note".into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    assert!(rename_path_impl(root, root, "a.md", "b.md").is_err());
    assert_eq!(crate::notes::list_notes_for_entity_in(root, root, "a.md").len(), 1);
    assert!(crate::notes::list_notes_for_entity_in(root, root, "b.md").is_empty());
}

#[test]
fn deleting_an_entity_leaves_its_notes_in_place_to_be_reattached() {
    // NTC-FR-10 / NTC-FR-12: a delete is not a rename this module can
    // correlate, so the notes stay — unresolved, editable, and reattachable
    // through Move… (NTS-FR-23). A cascade delete here would silently
    // destroy the user's writing along with the file.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("a.md"), b"A").unwrap();
    crate::notes::create_note_in(
        root,
        crate::notes::NoteScope::Entity {
            entity_id: "a.md".into(),
            entity_path: "a.md".into(),
        },
        "survives the file".into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .unwrap();

    delete_path_impl(root, "a.md", false).unwrap();

    let items = crate::notes::list_all_notes_in(root, root);
    assert_eq!(items.len(), 1, "the note outlives the file it was on");
    assert!(items[0].unresolved);
    assert_eq!(items[0].note.body, "survives the file");
}

#[test]
fn copy_path_into_folder_impl_copies_keeps_source_and_rejects_collision() {
    // PST-FR-21: copy duplicates the source into the folder, leaves the
    // source in place, and a destination collision is a typed error.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("src.md"), b"content").unwrap();
    std::fs::create_dir_all(root.join("dst")).unwrap();
    copy_path_into_folder_impl(root, "src.md", "dst").unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("dst").join("src.md")).unwrap(),
        "content"
    );
    assert!(root.join("src.md").exists(), "copy leaves the source in place");
    // A second copy collides with the now-existing destination.
    assert!(copy_path_into_folder_impl(root, "src.md", "dst").is_err());
}

#[test]
fn library_file_operation_commands_are_in_scope() {
    // Compile-time guard: renaming/removing any file-operation command
    // without updating `generate_handler!` / `COMMAND_NAMES` fails here.
    let _ = delete_path;
    let _ = rename_path;
    let _ = copy_path_into_folder;
    let _ = create_file;
    let _ = create_folder;
}
