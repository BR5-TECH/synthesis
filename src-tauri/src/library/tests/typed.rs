//! Typed-artifact creation
//! (PST-FR-29 / ASC-FR-24 / NTA-new-typed-artifact.md), and the type a
//! plain creation infers.
//!
//! One part of `../tests/mod.rs`.

use super::*;

// -----------------------------------------------------------------------
// Typed-artifact creation (PST-FR-29 / ASC-FR-24 / NTA-new-typed-artifact.md)
// -----------------------------------------------------------------------

#[test]
fn create_typed_file_impl_writes_an_empty_file_and_records_a_file_scope_assignment() {
    // PST-FR-29 / ASC-FR-24, ASC-FR-05, ASC-FR-06 / NTA-FR-10, NTA-FR-11 / NTA-FR-12, NTA-FR-13, LIB-FR-18: one zero-byte file, one
    // file-scope assignment, and a node resolving to exactly the type the
    // caller chose with `type_source = "assigned"`.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();

    let node = create_typed_file_impl(
        root,
        Some("specifications"),
        "overview.md",
        Some(scanning::ArtifactType::Spec),
    )
    .unwrap();

    let written = root.join("specifications/overview.md");
    assert!(written.is_file());
    assert_eq!(
        std::fs::read(&written).unwrap().len(),
        0,
        "NTA-FR-11: the window captures no content, so the file is born empty"
    );
    assert_eq!(node.path, "specifications/overview.md");
    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Spec));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));

    let assignments = scanning::load_assignments(root);
    let stored = assignments
        .assignments
        .get("specifications/overview.md")
        .expect("a file-scope assignment for the new path");
    assert_eq!(stored.artifact_type, scanning::ArtifactType::Spec);
    assert_eq!(stored.scope, scanning::Scope::File);
}

#[test]
fn create_typed_file_and_assign_artifact_type_store_the_identical_assignment() {
    // ASC-FR-24, ASC-FR-05, ASC-FR-06 tail: a file created with a type and a file typed afterwards
    // from the Project context menu end in a byte-identical stored
    // assignment, because both go through the one attribution write
    // (ASC-FR-24).
    let created = tempfile::TempDir::new().unwrap();
    let typed_after = tempfile::TempDir::new().unwrap();
    let a = &crate::fs::RootFs::for_root(created.path());
    let b = &crate::fs::RootFs::for_root(typed_after.path());
    for root in [a, b] {
        std::fs::create_dir_all(root.join("specifications")).unwrap();
    }

    create_typed_file_impl(
        a,
        Some("specifications"),
        "overview.md",
        Some(scanning::ArtifactType::Spec),
    )
    .unwrap();

    create_file_impl(b, Some("specifications"), "overview.md").unwrap();
    scanning::assign(
        b,
        "specifications/overview.md",
        scanning::ArtifactType::Spec,
        scanning::Scope::File,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(created.path().join(".synthesis/library.toml")).unwrap(),
        std::fs::read_to_string(typed_after.path().join(".synthesis/library.toml")).unwrap(),
    );
}

#[test]
fn create_typed_file_impl_outranks_an_inherited_folder_type() {
    // PST-FR-29, ASC-FR-06 / NTA-FR-11, NTA-FR-12, NTA-FR-13, FLO-FR-04: a file-scope assignment beats the ancestor
    // folder's, so the file resolves to the type the author chose rather
    // than the one the folder lends it (ASC-FR-06).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("pipelines")).unwrap();
    scanning::assign(
        root,
        "pipelines",
        scanning::ArtifactType::Spec,
        scanning::Scope::Folder,
    )
    .unwrap();

    let node = create_typed_file_impl(
        root,
        Some("pipelines"),
        "checks.md",
        Some(scanning::ArtifactType::Flow),
    )
    .unwrap();

    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Flow));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));
}

#[test]
fn create_typed_file_impl_refuses_a_collision_without_touching_the_existing_file() {
    // PST-FR-29 head / NTA-FR-16, NTA-FR-10: an entry of that name already in the
    // destination is a typed collision — the existing bytes stand and no
    // assignment is gained.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::write(root.join("specifications/overview.md"), b"mine").unwrap();

    let err = create_typed_file_impl(
        root,
        Some("specifications"),
        "overview.md",
        Some(scanning::ArtifactType::Spec),
    )
    .unwrap_err();

    assert!(err.contains("already exists"), "{err}");
    assert_eq!(
        std::fs::read_to_string(root.join("specifications/overview.md")).unwrap(),
        "mine"
    );
    assert!(scanning::load_assignments(root).assignments.is_empty());
}

#[test]
fn create_typed_file_impl_rolls_the_file_back_when_the_assignment_cannot_be_written() {
    // PST-FR-29 tail / ASC-FR-24 / NTA-FR-16: the two steps are one
    // transaction. The assignment write is made to fail by putting a
    // *directory* where `.synthesis/library.toml` has to go, so neither a
    // partially-created file nor an orphaned assignment survives the call.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::create_dir_all(root.join(".synthesis/library.toml")).unwrap();

    let err = create_typed_file_impl(
        root,
        Some("specifications"),
        "overview.md",
        Some(scanning::ArtifactType::Spec),
    )
    .unwrap_err();
    assert!(!err.is_empty());
    assert!(
        !root.join("specifications/overview.md").exists(),
        "the file just written is taken back off disk"
    );

    // And with the obstruction gone, the two things ASC-FR-24 asks about:
    // `.synthesis/library.toml` holds no assignment for that path, and the
    // rescanned tree holds no node for it.
    std::fs::remove_dir_all(root.join(".synthesis/library.toml")).unwrap();
    assert!(
        !scanning::load_assignments(root)
            .assignments
            .contains_key("specifications/overview.md"),
        "no assignment survives for a path the project does not have"
    );
    assert!(
        find_node(&scanning::scan(root), "specifications/overview.md").is_none(),
        "and the rescanned tree holds no node for it"
    );

    // The very same call then succeeds and leaves exactly one file and one
    // assignment.
    let node = create_typed_file_impl(
        root,
        Some("specifications"),
        "overview.md",
        Some(scanning::ArtifactType::Spec),
    )
    .unwrap();
    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Spec));
    assert_eq!(scanning::load_assignments(root).assignments.len(), 1);
}

#[test]
fn create_typed_file_impl_requires_a_type_and_a_bare_basename() {
    // PST-FR-29 tail / PST-FR-29 / NTA-FR-09: a call with no type is a typed
    // validation error and creates nothing — the one thing that separates
    // this operation from `create_file`. A name that is a path, or empty, is
    // refused on `create_file`'s own terms.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    assert_eq!(
        create_typed_file_impl(root, None, "overview.md", None).unwrap_err(),
        ERR_TYPED_FILE_NEEDS_A_TYPE
    );
    assert!(!root.join("overview.md").exists());
    assert!(scanning::load_assignments(root).assignments.is_empty());

    for bad in ["a/b.md", "a\\b.md", "", "   ", ".."] {
        assert!(
            create_typed_file_impl(root, None, bad, Some(scanning::ArtifactType::Spec))
                .is_err(),
            "{bad:?} must be refused"
        );
    }
    assert!(scanning::load_assignments(root).assignments.is_empty());
    assert!(create_typed_file_impl(
        root,
        Some("../evil"),
        "x.md",
        Some(scanning::ArtifactType::Spec)
    )
    .is_err());
}

#[test]
fn create_typed_file_impl_takes_the_name_verbatim_for_every_built_in_type() {
    // NTA-FR-05, NTA-FR-09 / NTA-FR-06, NTA-FR-11, NTA-FR-12, NTA-FR-13: the name is the complete basename, nothing is
    // appended and no extension is derived from the type; and each of the
    // eight built-in types creates an empty file carrying exactly that type.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let cases = [
        (scanning::ArtifactType::Skill, "SKILL.md"),
        (scanning::ArtifactType::Agent, "reviewer.agent"),
        (scanning::ArtifactType::Prompt, "prompt"),
        (scanning::ArtifactType::Spec, "overview.md"),
        (scanning::ArtifactType::Flow, "login.flow"),
        (scanning::ArtifactType::Instructions, "Makefile"),
        (scanning::ArtifactType::Scenario, "login.scenario.md"),
        (scanning::ArtifactType::Scratchpad, "notes.tar.gz"),
    ];
    for (artifact_type, name) in cases {
        let node = create_typed_file_impl(root, None, name, Some(artifact_type)).unwrap();
        assert_eq!(node.name, name, "the name is taken exactly as typed");
        assert_eq!(node.artifact_type, Some(artifact_type));
        assert_eq!(node.type_source, Some(scanning::TypeSource::Assigned));
        assert_eq!(std::fs::read(root.join(name)).unwrap().len(), 0);
    }
    assert_eq!(scanning::load_assignments(root).assignments.len(), 8);
}

#[test]
fn create_typed_file_impl_creates_no_draft_and_reads_no_draft_template() {
    // PST-FR-29, PSS-FR-21 / NTA-FR-14, DRS-FR-39 / DRS-FR-07, DRS-FR-03 tail: one file and one assignment,
    // and nothing under `.synthesis/drafts/` — this operation reaches no
    // command of `DRS-draft-storage.md` and never applies the project's
    // draft template.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    crate::project_settings::save_project_config_to(
        root,
        crate::project_settings::ProjectConfig {
            line_endings: crate::project_settings::LineEndings::Lf,
            draft_template: Some("# Context\n".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();

    for (i, artifact_type) in [
        scanning::ArtifactType::Skill,
        scanning::ArtifactType::Agent,
        scanning::ArtifactType::Prompt,
        scanning::ArtifactType::Spec,
        scanning::ArtifactType::Flow,
        scanning::ArtifactType::Instructions,
        scanning::ArtifactType::Scenario,
        scanning::ArtifactType::Scratchpad,
    ]
    .into_iter()
    .enumerate()
    {
        let node =
            create_typed_file_impl(root, None, &format!("notes-{i}.md"), Some(artifact_type))
                .unwrap();
        assert_eq!(std::fs::read(root.join(&node.path)).unwrap().len(), 0);
        assert!(
            !root.join(".synthesis/drafts").exists(),
            "nothing is written under the drafts root, for any type"
        );
    }
    assert_eq!(scanning::load_assignments(root).assignments.len(), 8);
    // The template is still exactly what it was; nothing read or rewrote it.
    assert_eq!(
        crate::project_settings::load_project_config_from(root)
            .unwrap()
            .draft_template
            .as_deref(),
        Some("# Context\n")
    );
}

#[test]
fn create_file_impl_returns_a_node_typed_by_path_inference_alone() {
    // NFI-FR-11's other branch: with nothing assigned anywhere, the created
    // file still resolves to a type when its path convention names one
    // (ASC-FR-03), reported as `inferred`. Nothing was written to the
    // attribution file to make that happen.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();

    let node = create_file_impl(root, Some("specifications"), "NFI-new-file.md").unwrap();

    assert_eq!(node.artifact_type, Some(scanning::ArtifactType::Spec));
    assert_eq!(node.type_source, Some(scanning::TypeSource::Inferred));
    assert!(
        !root.join(".synthesis/library.toml").exists(),
        "create_file records nothing, so inference is doing all the work"
    );
}

#[test]
fn create_file_impl_returns_an_untyped_node_where_nothing_classifies_it() {
    // NFI-FR-11's third branch / NFI-FR-12, ESH-FR-ATDS, LIB-FR-18: a file no convention infers and no
    // ancestor assignment covers stays unclassified, and its node carries no
    // type at all — which is what makes the Library relax to the All files
    // lens to reveal it (LIB-FR-18).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    let node = create_file_impl(root, None, "package.json").unwrap();

    assert_eq!(node.artifact_type, None);
    assert_eq!(node.type_source, None);
}

#[test]
fn created_file_node_stands_in_for_a_file_the_scan_ignores() {
    // ASC-FR-09: the scan skips ignored paths, so a created `.env` or a file
    // under `dist/` is absent from it. The creation succeeded, so PST-FR-26's
    // "return the new file node" is answered with the node it would have had.
    let node = created_file_node("dist/bundle.js");

    assert_eq!(node.id, "dist/bundle.js");
    assert_eq!(node.name, "bundle.js");
    assert_eq!(node.path, "dist/bundle.js");
    assert_eq!(node.node_kind, scanning::NodeKind::File);
    assert_eq!(node.artifact_type, None);
    assert_eq!(node.children, None);
}

#[test]
fn create_file_impl_succeeds_for_a_gitignored_destination() {
    // The end-to-end of the case above: the file lands on disk and a node
    // naming it comes back even though the scan never surfaces it.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join(".gitignore"), b"dist/\n").unwrap();
    std::fs::create_dir_all(root.join("dist")).unwrap();

    let node = create_file_impl(root, Some("dist"), "bundle.js").unwrap();

    assert!(root.join("dist/bundle.js").is_file());
    assert_eq!(node.path, "dist/bundle.js");
    assert_eq!(node.node_kind, scanning::NodeKind::File);
}
