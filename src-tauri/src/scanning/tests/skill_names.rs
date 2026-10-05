//! A skill's declared name rides on its node (ASC-FR-19).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ------------------------------------------------------------------
// ASC-FR-19: a skill's declared name rides on its node.
// ------------------------------------------------------------------

#[test]
fn ts_declared_name_rides_on_a_skill_node() {
    let content = facts_for(
        ".claude/skills/code-review/SKILL.md",
        "---\nname: Code review\ndescription: d\n---\n\n# Body\n",
    );
    let tree = build_tree(
        "p",
        &skill_entries(),
        &LibraryAssignments::default(),
        &content,
    );

    let skill = find(&tree, ".claude/skills/code-review/SKILL.md").unwrap();
    assert_eq!(skill.artifact_type, Some(ArtifactType::Skill));
    assert_eq!(skill.display_name.as_deref(), Some("Code review"));
    // The supporting file beside it is not something that names itself, so
    // it carries no declared name however its own frontmatter reads.
    let beside = find(&tree, ".claude/skills/code-review/references/rules.md").unwrap();
    assert_eq!(beside.display_name, None);
}

#[test]
fn ts_a_skill_declaring_no_usable_name_carries_none() {
    for body in [
        "---\ndescription: no name here\n---\n",
        "---\nname:   \n---\n",
        "# no frontmatter at all\n",
        "",
    ] {
        let content = facts_for(".claude/skills/code-review/SKILL.md", body);
        let tree = build_tree(
            "p",
            &skill_entries(),
            &LibraryAssignments::default(),
            &content,
        );
        let skill = find(&tree, ".claude/skills/code-review/SKILL.md").unwrap();
        assert_eq!(skill.display_name, None, "for body {body:?}");
    }
}

#[test]
fn ts_declared_name_reads_only_the_files_that_name_themselves() {
    // The read is per-file and the tree is built over a project of ordinary
    // artifacts: nothing but the `SKILL.md` may be opened for a name.
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&asked);
    let content = move |rel: &str| {
        seen.lock().unwrap().push(rel.to_string());
        ContentFacts::default()
    };
    let mut entries = skill_entries();
    entries.push(entry("prompts", true));
    entries.push(entry("prompts/gather.md", false));

    build_tree("p", &entries, &LibraryAssignments::default(), &content);

    let reads = asked.lock().unwrap().clone();
    // The skill's own file is read once, for its name.
    assert_eq!(
        reads
            .iter()
            .filter(|r| *r == ".claude/skills/code-review/SKILL.md")
            .count(),
        1,
    );
    // The file beside it is path-classified, so neither the tiebreak nor the
    // name lookup opens it. (`prompts/gather.md` is read by the ASC-FR-04
    // tiebreak, which is what that closure has always been for.)
    assert!(!reads
        .iter()
        .any(|r| r == ".claude/skills/code-review/references/rules.md"));
}

#[test]
fn ts_content_facts_reads_top_level_keys_only() {
    let facts = read_content_facts(
        "---\ntype: spec\nname: \"Quoted name\"\nmeta:\n  name: nested\n---\nbody\n",
    );
    assert_eq!(facts.artifact_type, Some(ArtifactType::Spec));
    // The nested `name:` belongs to `meta`, not to the file.
    assert_eq!(facts.declared_name.as_deref(), Some("Quoted name"));
    // The first top-level occurrence wins, and a body key is not read.
    let later = read_content_facts("---\nname: first\n---\nname: body\n");
    assert_eq!(later.declared_name.as_deref(), Some("first"));
}
