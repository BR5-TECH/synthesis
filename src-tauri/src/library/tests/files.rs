//! PST-FR-26: `create_file`.
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// PST-FR-26: create_file
// ------------------------------------------------------------------

#[test]
fn create_file_impl_creates_an_empty_untyped_file_and_returns_its_node() {
    // PST-FR-26 happy path: the file exists and is empty, nothing is recorded
    // in the attribution file, and the returned node names it with no type.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("src/hooks")).unwrap();

    let node = create_file_impl(root, Some("src/hooks"), "useProjectFolders.ts").unwrap();

    let created = root.join("src/hooks/useProjectFolders.ts");
    assert!(created.is_file());
    assert_eq!(
        std::fs::read(&created).unwrap().len(),
        0,
        "PST-FR-26: the created file is empty"
    );
    assert_eq!(node.path, "src/hooks/useProjectFolders.ts");
    assert_eq!(node.node_kind, scanning::NodeKind::File);
    assert!(
        !root.join(".synthesis/library.toml").exists(),
        "create_file records no type assignment at any scope"
    );
    assert_eq!(node.artifact_type, None);
    assert_eq!(node.type_source, None);
}

#[test]
fn create_file_impl_rejects_a_collision_without_truncating_the_existing_file() {
    // PST-FR-26: a second creation of the same name is a typed error and the
    // existing content survives — the guard has to precede the atomic write,
    // which would otherwise replace the file wholesale.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("notes.txt"), b"existing content").unwrap();

    let err = create_file_impl(root, None, "notes.txt").unwrap_err();

    assert!(err.contains("already exists"), "unexpected error: {err}");
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "existing content"
    );
}

#[test]
fn create_file_impl_rejects_a_name_an_existing_folder_occupies() {
    // PST-FR-26: a folder occupying the destination name collides just as a
    // file does, and the folder's contents are untouched.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/keep.md"), b"keep").unwrap();

    assert!(create_file_impl(root, None, "docs").is_err());

    assert!(root.join("docs").is_dir());
    assert_eq!(
        std::fs::read_to_string(root.join("docs/keep.md")).unwrap(),
        "keep"
    );
}

#[test]
fn create_file_impl_accepts_an_extensionless_name_and_a_dotfile_verbatim() {
    // PST-FR-26 / NFI-FR-05: the name is the complete basename. No extension
    // is required, appended, or privileged.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let makefile = create_file_impl(root, None, "Makefile").unwrap();
    let gitignore = create_file_impl(root, None, ".gitignore").unwrap();
    let tarball = create_file_impl(root, None, "notes.tar.gz").unwrap();

    assert!(root.join("Makefile").is_file());
    assert!(root.join(".gitignore").is_file());
    assert!(root.join("notes.tar.gz").is_file());
    assert_eq!(makefile.name, "Makefile");
    assert_eq!(gitignore.name, ".gitignore");
    assert_eq!(tarball.name, "notes.tar.gz");
    // Nothing was appended to any of them.
    assert!(!root.join("Makefile.md").exists());
    assert!(!root.join(".gitignore.md").exists());
}

#[test]
fn create_file_impl_rejects_a_name_that_is_a_path() {
    // PST-FR-26: a bare basename only, so one call never creates the folders
    // leading to the file.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    assert!(create_file_impl(root, None, "a/b.ts").is_err());
    assert!(create_file_impl(root, None, "a\\b.ts").is_err());
    assert!(create_file_impl(root, None, "").is_err());
    assert!(create_file_impl(root, None, "   ").is_err());
    assert!(create_file_impl(root, None, "..").is_err());
    assert!(!root.join("a").exists(), "no folder was created on the way");
}

#[test]
fn create_file_impl_rejects_a_location_escaping_the_root() {
    // FSA-FR-10 via `resolve_under`: nothing is written outside the project.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &{ let p = dir.path().join("project"); std::fs::create_dir_all(&p).unwrap(); crate::fs::RootFs::for_root(&p) };
    std::fs::create_dir_all(&root).unwrap();

    assert!(create_file_impl(&root, Some("../evil"), "x.ts").is_err());

    assert!(!dir.path().join("evil").exists());
}

#[test]
fn create_file_impl_with_no_location_creates_at_the_project_root() {
    // PST-FR-26: an unset location is the project root.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let node = create_file_impl(root, None, "package.json").unwrap();

    assert!(root.join("package.json").is_file());
    assert_eq!(node.path, "package.json");
}

#[test]
fn create_file_impl_returns_a_node_inheriting_the_destination_folders_type() {
    // PST-FR-26 tail / NFI-FR-11: no assignment is recorded, yet the returned
    // node resolves through the ASC-FR-06 precedence — here the folder-scope
    // assignment on the destination, reported as `inherited`.
    //
    // The destination is `drafts/`, deliberately: a folder no path convention
    // claims, so the assignment is genuinely what types the file. A folder
    // like `specifications/` would resolve by *inference* instead and prove
    // nothing about inheritance (see the test below).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("drafts")).unwrap();
    scanning::assign(
        root,
        "drafts",
        scanning::ArtifactType::Scenario,
        scanning::Scope::Folder,
    )
    .unwrap();

    let node = create_file_impl(root, Some("drafts"), "login.md").unwrap();

    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Scenario));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Inherited));
    // The inheritance came from the folder's assignment, not from a new
    // per-file one this call wrote.
    let assignments = scanning::load_assignments(root);
    assert!(
        !assignments.assignments.contains_key("drafts/login.md"),
        "create_file must record no per-file assignment"
    );
}
