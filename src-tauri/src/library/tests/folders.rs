//! PST-FR-25: `create_folder`, the type its contents inherit, and the
//! rollback a failed assignment runs.
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// PST-FR-25: create_folder
// ------------------------------------------------------------------

#[test]
fn create_folder_impl_creates_an_empty_untyped_folder_and_returns_its_node() {
    // PST-FR-25 happy path: the folder exists and is empty, nothing is
    // recorded in the attribution file, and the returned node names it.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();

    let node = create_folder_impl(root, Some("specifications"), "scenarios", None).unwrap();

    let created = root.join("specifications/scenarios");
    assert!(created.is_dir());
    assert_eq!(std::fs::read_dir(&created).unwrap().count(), 0);
    assert_eq!(node.path, "specifications/scenarios");
    assert_eq!(node.node_kind, scanning::NodeKind::Folder);
    // No type chosen -> no assignment, and the node carries no type of its
    // own (ASC-FR-18).
    assert!(
        !root.join(".synthesis/library.toml").exists(),
        "an untyped creation must not write an attribution file"
    );
    assert_eq!(node.artifact_type, None);
    assert_eq!(node.type_source, None);
}

#[test]
fn create_folder_impl_rejects_a_collision_without_touching_the_existing_folder() {
    // PST-FR-25: repeating the call collides, and the existing folder's
    // contents survive — the primitive never adopts or clears one.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_folder_impl(root, None, "drafts", None).unwrap();
    std::fs::write(root.join("drafts/keep.md"), b"mine").unwrap();

    let err = create_folder_impl(root, None, "drafts", None).unwrap_err();
    assert!(err.contains("already exists"), "got: {err}");
    assert_eq!(
        std::fs::read_to_string(root.join("drafts/keep.md")).unwrap(),
        "mine"
    );
}

#[test]
fn create_folder_impl_rejects_a_name_that_is_a_path() {
    // PST-FR-25 / NFW-FR-08: one invocation creates one folder, never a chain
    // of nested ones — so a name carrying a separator is rejected outright and
    // no part of it is created.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert!(create_folder_impl(root, None, "a/b", None).is_err());
    assert!(!root.join("a").exists(), "no partial chain was created");
    assert!(create_folder_impl(root, None, "", None).is_err());
    assert!(create_folder_impl(root, None, "..", None).is_err());
}

#[test]
fn create_folder_impl_records_a_folder_scope_assignment_its_contents_inherit() {
    // PST-FR-25: the chosen type becomes a FOLDER-scope assignment, so a file
    // added later resolves to it as `inherited` without being tagged itself.
    // A file-scope assignment here would type the directory entry and leave
    // its contents unclassified — the bug this pins.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let node = create_folder_impl(root, None, "drafts", Some(scanning::ArtifactType::Scenario))
        .unwrap();
    assert!(root.join("drafts").is_dir());
    // ASC-FR-18: the folder reports its own assignment on the returned node.
    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Scenario));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));

    let stored = scanning::load_assignments(root);
    let a = stored.assignments.get("drafts").expect("assignment recorded");
    assert_eq!(a.artifact_type, scanning::ArtifactType::Scenario);
    assert_eq!(a.scope, scanning::Scope::Folder);

    std::fs::write(root.join("drafts/x.md"), b"body").unwrap();
    let tree = scanning::scan(root);
    let child = find_node(&tree, "drafts/x.md").expect("the new file is in the tree");
    assert_eq!(child.artifact_type, Some(scanning::ArtifactType::Scenario));
    assert_eq!(child.type_source, Some(scanning::TypeSource::Inherited));
}

#[test]
fn create_folder_impl_rejects_a_name_an_existing_file_occupies() {
    // PST-FR-25: a *file* of that name in the destination is a collision too,
    // and is left unchanged.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("README.md"), b"readme").unwrap();
    assert!(create_folder_impl(root, None, "README.md", None).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "readme"
    );
}

#[test]
fn create_folder_impl_rejects_a_location_escaping_the_root() {
    // PST-FR-25 / FSA-FR-10: a location that climbs out of the project root is
    // rejected by the escape gate and nothing is created.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &{ let p = dir.path().join("root"); std::fs::create_dir_all(&p).unwrap(); crate::fs::RootFs::for_root(&p) };
    std::fs::create_dir_all(&root).unwrap();
    assert!(create_folder_impl(&root, Some("../evil"), "x", None).is_err());
    assert!(!dir.path().join("evil").exists());
}

#[test]
fn create_folder_impl_with_no_location_creates_at_the_project_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let node = create_folder_impl(root, None, "top", None).unwrap();
    assert!(root.join("top").is_dir());
    assert_eq!(node.path, "top");
}

#[test]
fn a_folder_created_inside_a_typed_folder_inherits_that_type() {
    // A subfolder left untyped by the user takes the type of the nearest
    // ancestor folder that carries one, recorded as its own folder-scope
    // assignment. Without this the new folder carries no type, so it is
    // invisible under the **All Artifacts** lens (LIB-FR-09) and untagged
    // (LIB-FR-08) even though every file put inside it resolves to the
    // parent's type — the folder would look unclassified while behaving
    // classified.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_folder_impl(root, None, "prompts", Some(scanning::ArtifactType::Prompt)).unwrap();

    let node = create_folder_impl(root, Some("prompts"), "drafts", None).unwrap();

    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Prompt));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));
    let stored = scanning::load_assignments(root);
    let a = stored
        .assignments
        .get("prompts/drafts")
        .expect("the inherited type is recorded on the new folder");
    assert_eq!(a.artifact_type, scanning::ArtifactType::Prompt);
    assert_eq!(a.scope, scanning::Scope::Folder);
}

#[test]
fn inheritance_reaches_through_an_untyped_ancestor_and_the_user_can_override_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_folder_impl(root, None, "prompts", Some(scanning::ArtifactType::Prompt)).unwrap();
    // An untyped middle folder created outside Synthesis: the walk is
    // nearest-ancestor, so it does not stop the inheritance.
    std::fs::create_dir_all(root.join("prompts/mid")).unwrap();

    let deep = create_folder_impl(root, Some("prompts/mid"), "leaf", None).unwrap();
    assert_eq!(deep.artifact_type, Some(scanning::ArtifactType::Prompt));

    // An explicit choice always wins over what would have been inherited.
    let override_node =
        create_folder_impl(root, Some("prompts"), "specs", Some(scanning::ArtifactType::Spec))
            .unwrap();
    assert_eq!(override_node.artifact_type, Some(scanning::ArtifactType::Spec));
    assert_eq!(
        scanning::load_assignments(root)
            .assignments
            .get("prompts/specs")
            .unwrap()
            .artifact_type,
        scanning::ArtifactType::Spec
    );
}

#[test]
fn a_folder_with_no_typed_ancestor_stays_untyped() {
    // The other half: inheritance only happens where there is something to
    // inherit. A folder at the root of an untyped project records nothing.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("plain")).unwrap();

    let node = create_folder_impl(root, Some("plain"), "child", None).unwrap();

    assert_eq!(node.artifact_type, None);
    assert_eq!(node.type_source, None);
    assert!(
        !root.join(".synthesis/library.toml").exists(),
        "nothing to inherit means nothing written"
    );
}

#[test]
fn a_file_scope_assignment_on_an_ancestor_is_not_inherited() {
    // Only a FOLDER-scope assignment types a subtree (ASC-FR-05); a file-scope
    // entry that happens to name a directory must not leak into it.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("odd")).unwrap();
    scanning::assign(
        root,
        "odd",
        scanning::ArtifactType::Prompt,
        scanning::Scope::File,
    )
    .unwrap();

    let node = create_folder_impl(root, Some("odd"), "child", None).unwrap();
    assert_eq!(node.artifact_type, None);
}

#[test]
fn a_folder_whose_name_the_scan_excludes_is_still_reported_as_created() {
    // ASC-FR-09: the scan honours `.gitignore`, so a folder named by an ignore
    // rule is legitimately absent from the tree. The creation succeeded, so
    // PST-FR-25's "return the new folder node" must be answered rather than
    // turned into an error about a folder that is sitting on disk — which
    // would also strand the user, since the retry then reports a collision.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join(".gitignore"), b"dist/\n").unwrap();

    let node = create_folder_impl(root, None, "dist", Some(scanning::ArtifactType::Spec))
        .unwrap();

    assert!(root.join("dist").is_dir());
    assert_eq!(node.path, "dist");
    assert_eq!(node.name, "dist");
    assert_eq!(node.node_kind, scanning::NodeKind::Folder);
    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Spec));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));
    assert_eq!(node.has_artifacts, Some(true));
    assert_eq!(node.children.as_deref(), Some(&[][..]));
    // And it really was excluded from the scan, which is what makes the
    // fallback the only way this node could be returned.
    assert!(find_node(&scanning::scan(root), "dist").is_none());
}

#[test]
fn a_failed_type_assignment_rolls_the_creation_back() {
    // PST-FR-25: "in every error case nothing is created". Occupying
    // `.synthesis` with a file makes the attribution write fail, which must not
    // leave the folder behind — least of all untyped, which is silently not
    // what the user asked for.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join(".synthesis"), b"not a directory").unwrap();

    let err = create_folder_impl(root, None, "drafts", Some(scanning::ArtifactType::Spec))
        .unwrap_err();

    assert!(!err.is_empty());
    assert!(
        !root.join("drafts").exists(),
        "the folder must not survive a failed assignment: got {err}"
    );
}

#[test]
fn no_structural_mutation_in_this_module_bypasses_the_fsa_primitives() {
    // PST-FR-10: "this module performs no raw `fs::remove`, `fs::rename`,
    // `fs::copy`, or `fs::create_dir` calls" — every structural mutation goes
    // through `crate::fs`. A raw `std::fs::create_dir_all` here would compile,
    // pass every behavioural test, and silently skip the FSA-FR-10 escape gate
    // plus FSA-FR-14's refusal to adopt an existing directory. Pinned at the
    // source level, the same trick `worktree.rs` uses.
    //
    // Only the non-test half is scanned: the tests below legitimately build
    // and inspect fixtures with `std::fs`.
    let source = include_str!("../../library.rs");
    let impl_half = source
        .split_once("\n#[cfg(test)]")
        .expect("the test module")
        .0;
    for forbidden in [
        "std::fs::create_dir",
        "std::fs::remove_",
        "std::fs::rename",
        "std::fs::copy",
        "std::fs::write",
    ] {
        assert!(
            !impl_half.contains(forbidden),
            "PST-FR-10: {forbidden} must go through a `crate::fs` primitive"
        );
    }
}

#[test]
fn a_rollback_never_destroys_content_that_appeared_under_the_new_folder() {
    // The rollback is deliberately empty-only. If some other writer dropped a
    // file in between the creation and the failure, losing the race must cost
    // a stray folder — not the user's file.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("keep")).unwrap();
    std::fs::write(root.join("keep/precious.md"), b"do not delete").unwrap();

    rollback_created_folder(root, "keep");

    assert_eq!(
        std::fs::read_to_string(root.join("keep/precious.md")).unwrap(),
        "do not delete"
    );
    // While an empty one is removed, which is the case it exists for.
    std::fs::create_dir_all(root.join("hollow")).unwrap();
    rollback_created_folder(root, "hollow");
    assert!(!root.join("hollow").exists());
}

#[test]
fn is_valid_artifact_name_rejects_empty_dot_and_separators() {
    // NAW-FR-08 / PST-FR-21: a creatable name is a non-empty bare basename.
    assert!(is_valid_artifact_name("spec.md"));
    assert!(is_valid_artifact_name("  spec.md  "));
    assert!(!is_valid_artifact_name(""));
    assert!(!is_valid_artifact_name("   "));
    assert!(!is_valid_artifact_name("."));
    assert!(!is_valid_artifact_name(".."));
    assert!(!is_valid_artifact_name("a/b.md"));
    assert!(!is_valid_artifact_name("a\\b.md"));
}

#[test]
fn artifact_rel_path_joins_location_or_falls_back_to_root() {
    // PST-FR-21: an absent/empty/whitespace location creates at the root.
    assert_eq!(artifact_rel_path(Some("specs/ui"), "a.md"), "specs/ui/a.md");
    assert_eq!(artifact_rel_path(Some("specs/ui/"), "a.md"), "specs/ui/a.md");
    assert_eq!(artifact_rel_path(None, "a.md"), "a.md");
    assert_eq!(artifact_rel_path(Some(""), "a.md"), "a.md");
    assert_eq!(artifact_rel_path(Some("   "), "a.md"), "a.md");
    // The name is trimmed in the joined path.
    assert_eq!(artifact_rel_path(Some("d"), "  a.md "), "d/a.md");
}
