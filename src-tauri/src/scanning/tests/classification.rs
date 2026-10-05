//! Tree nesting, path inference and the content tiebreak
//! (ASC-FR-01, ASC-FR-03, ASC-FR-04, ASC-FR-06).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// ASC-FR-01: the tree mirrors on-disk nesting.
// ------------------------------------------------------------------
#[test]
fn ts1_build_tree_mirrors_nesting() {
    let entries = [
        entry(".claude", true),
        entry(".claude/skills", true),
        entry(".claude/skills/foo.md", false),
        entry("specifications", true),
        entry("specifications/lib.md", false),
        entry("README.md", false),
    ];
    let tree = build_tree("proj", &entries, &LibraryAssignments::default(), &no_content);
    assert_eq!(tree.node_kind, NodeKind::Folder);
    assert_eq!(tree.name, "proj");

    let skills = find(&tree, ".claude/skills").expect("skills folder present");
    assert_eq!(skills.node_kind, NodeKind::Folder);
    let foo = find(&tree, ".claude/skills/foo.md").expect("foo.md present");
    assert_eq!(foo.node_kind, NodeKind::File);
    // README at top level.
    assert!(find(&tree, "README.md").is_some());
    // No node exists without a corresponding entry path.
    assert!(find(&tree, "does/not/exist").is_none());
}

// ------------------------------------------------------------------
// ASC-FR-03: multi-ecosystem path inference.
// ------------------------------------------------------------------
#[test]
fn ts2_skill_inferred_from_claude_skills_path() {
    let entries = [
        entry(".claude", true),
        entry(".claude/skills", true),
        entry(".claude/skills/foo.md", false),
    ];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    let foo = find(&tree, ".claude/skills/foo.md").unwrap();
    assert_eq!(foo.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(foo.type_source, Some(TypeSource::Inferred));
}

#[test]
fn ts3_agent_inferred_across_ecosystems() {
    // `.claude/agents/a.md` (Claude Code) and top-level `AGENTS.md` (Codex)
    // both classify as agent.
    assert_eq!(infer_from_path(".claude/agents/a.md"), Some(ArtifactType::Agent));
    assert_eq!(infer_from_path("AGENTS.md"), Some(ArtifactType::Agent));

    let entries = [
        entry(".claude", true),
        entry(".claude/agents", true),
        entry(".claude/agents/a.md", false),
        entry("AGENTS.md", false),
    ];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    assert_eq!(
        find(&tree, ".claude/agents/a.md").unwrap().artifact_type,
        Some(ArtifactType::Agent)
    );
    assert_eq!(
        find(&tree, "AGENTS.md").unwrap().artifact_type,
        Some(ArtifactType::Agent)
    );
}

#[test]
fn path_recognizers_cover_representative_ecosystems() {
    assert_eq!(infer_from_path(".claude/commands/x.md"), Some(ArtifactType::Prompt));
    assert_eq!(infer_from_path(".github/prompts/p.md"), Some(ArtifactType::Prompt));
    assert_eq!(
        infer_from_path(".github/copilot-instructions.md"),
        Some(ArtifactType::Instructions)
    );
    assert_eq!(
        infer_from_path("docs/setup.instructions.md"),
        Some(ArtifactType::Instructions)
    );
    assert_eq!(infer_from_path("specifications/ui/LIB.md"), Some(ArtifactType::Spec));
    assert_eq!(infer_from_path("CLAUDE.md"), Some(ArtifactType::Instructions));
    assert_eq!(infer_from_path("notes/random.md"), None);
}

#[test]
fn the_flow_document_convention_classifies_at_any_depth() {
    // ASC-FR-03: `**/*.flow` -> flow. Path inference is the only route
    // that reaches a Flow without an explicit assignment, since the content
    // tiebreak reads Markdown frontmatter and a Flow document is JSON.
    assert_eq!(
        infer_from_path("onboarding-review.flow"),
        Some(ArtifactType::Flow)
    );
    assert_eq!(
        infer_from_path("workflows/nested/deep.flow"),
        Some(ArtifactType::Flow)
    );
    // Case-insensitively, like every other filename recognizer here.
    assert_eq!(
        infer_from_path("workflows/Review.FLOW"),
        Some(ArtifactType::Flow)
    );
    // A Flow's contents are JSON but its name is not: `.json` is not the
    // convention, and neither is a file merely named "flow". A file that
    // spells both out is a JSON file, and reaches `flow` only through an
    // explicit assignment (ASC-FR-06).
    assert_eq!(infer_from_path("tsconfig.json"), None);
    assert_eq!(infer_from_path("workflows/review.flow.json"), None);
    assert_eq!(infer_from_path("workflows/flow.md"), None);
    // Nor is a JS type-declaration file that shares the suffix, or a bare
    // dotfile with no stem at all.
    assert_eq!(infer_from_path("src/index.js.flow"), None);
    assert_eq!(infer_from_path(".flow"), None);

    let entries = [
        entry("workflows", true),
        entry("workflows/review.flow", false),
    ];
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &no_content);
    let node = find(&tree, "workflows/review.flow").unwrap();
    assert_eq!(node.artifact_type, Some(ArtifactType::Flow));
    assert_eq!(node.type_source, Some(TypeSource::Inferred));
}

// ASC-FR-06 level 1 over level 2: a user's assignment overrides the path
// convention. This is the half of the Flow rule that lets an author name a
// Flow anything, and keep a `.flow` file that is not one from opening as one
// — `../ui/DFV-diff-viewer.md` DFV-FR-17 renders on exactly this outcome.
#[test]
fn ts6_per_file_assignment_overrides_path_inference() {
    let entries = [
        entry("workflows", true),
        entry("workflows/review.flow", false),
        entry("workflows/pipeline", false),
    ];
    let mut assignments = LibraryAssignments::default();
    assignments.assignments.insert(
        "workflows/review.flow".to_string(),
        Assignment {
            artifact_type: ArtifactType::Spec,
            scope: Scope::File,
        },
    );
    assignments.assignments.insert(
        "workflows/pipeline".to_string(),
        Assignment {
            artifact_type: ArtifactType::Flow,
            scope: Scope::File,
        },
    );

    let tree = build_tree("p", &entries, &assignments, &no_content);

    // The extension says flow; the author said spec, and the author wins.
    let overridden = find(&tree, "workflows/review.flow").unwrap();
    assert_eq!(overridden.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(overridden.type_source, Some(TypeSource::Assigned));
    // …and a Flow no convention would recognise is one because they said so.
    let assigned = find(&tree, "workflows/pipeline").unwrap();
    assert_eq!(assigned.artifact_type, Some(ArtifactType::Flow));
    assert_eq!(assigned.type_source, Some(TypeSource::Assigned));
}

/// The same through `scan`, so `load_assignments` and the real
/// `.synthesis/library.toml` round-trip are in the path too.
#[test]
fn ts6_assignment_overrides_the_extension_on_disk() {
    let tmp = project_with_synthesis();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::create_dir_all(root.join("workflows")).unwrap();
    fs::write(root.join("workflows/review.flow"), "{}").unwrap();
    fs::write(root.join("workflows/pipeline"), "{}").unwrap();

    assign(root, "workflows/review.flow", ArtifactType::Spec, Scope::File).unwrap();
    assign(root, "workflows/pipeline", ArtifactType::Flow, Scope::File).unwrap();

    let tree = scan(root);
    assert_eq!(
        find(&tree, "workflows/review.flow").unwrap().artifact_type,
        Some(ArtifactType::Spec),
    );
    assert_eq!(
        find(&tree, "workflows/pipeline").unwrap().artifact_type,
        Some(ArtifactType::Flow),
    );

    // ASC-FR-07: clearing the override hands the file back to the extension.
    clear(root, "workflows/review.flow").unwrap();
    let reverted = find(&scan(root), "workflows/review.flow").unwrap().clone();
    assert_eq!(reverted.artifact_type, Some(ArtifactType::Flow));
    assert_eq!(reverted.type_source, Some(TypeSource::Inferred));
}

// ------------------------------------------------------------------
// ASC-FR-04: content-marker tiebreak.
// ------------------------------------------------------------------
#[test]
fn ts4_content_tiebreak_classifies_when_path_ambiguous() {
    let frontmatter = "---\ntype: spec\n---\n\n# Body\n";
    assert_eq!(infer_from_content(frontmatter), Some(ArtifactType::Spec));

    let entries = [entry("notes", true), entry("notes/x.md", false)];
    let content = |rel: &str| -> ContentFacts {
        if rel == "notes/x.md" {
            read_content_facts(frontmatter)
        } else {
            ContentFacts::default()
        }
    };
    let tree = build_tree("p", &entries, &LibraryAssignments::default(), &content);
    let x = find(&tree, "notes/x.md").unwrap();
    assert_eq!(x.artifact_type, Some(ArtifactType::Spec));
    assert_eq!(x.type_source, Some(TypeSource::Inferred));
}

#[test]
fn content_tiebreak_accepts_flow_alias_and_quotes() {
    assert_eq!(
        infer_from_content("---\ntype: \"flow\"\n---\n"),
        Some(ArtifactType::Flow)
    );
    // The legacy spelling still parses, so a project carrying it keeps its
    // classification (see the `harness` serde alias on `ArtifactType`).
    assert_eq!(
        infer_from_content("---\ntype: harness\n---\n"),
        Some(ArtifactType::Flow)
    );
    assert_eq!(infer_from_content("---\nkind: agent\n---\n"), Some(ArtifactType::Agent));
    // No frontmatter -> no match.
    assert_eq!(infer_from_content("# just a heading\ntype: spec\n"), None);
    // Unknown marker -> no match.
    assert_eq!(infer_from_content("---\ntype: novel\n---\n"), None);
}
