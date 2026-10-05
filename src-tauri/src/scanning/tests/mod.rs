//! The test scenarios of `specifications/core/ASC-artifact-scanning.md`.
//!
//! Each test builds its own project root in a temporary directory, so no test
//! reads or writes the developer's own tree. This module holds the fixtures and
//! the lookup helpers; the topic modules hold the tests.

use super::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// A content lookup that never matches — used when exercising path-only
/// classification.
fn no_content(_rel: &str) -> ContentFacts {
    ContentFacts::default()
}

fn entry(rel: &str, is_dir: bool) -> Entry {
    Entry {
        rel: rel.to_string(),
        is_dir,
    }
}

/// The eight built-in types, so a variant added later fails to compile here
/// rather than quietly escaping the checks below.
const EVERY_TYPE: [ArtifactType; 8] = [
    ArtifactType::Skill,
    ArtifactType::Agent,
    ArtifactType::Prompt,
    ArtifactType::Spec,
    ArtifactType::Flow,
    ArtifactType::Instructions,
    ArtifactType::Scenario,
    ArtifactType::Scratchpad,
];

/// The three entries a skill produces on disk: the folder, its `SKILL.md`,
/// and a supporting file beside it that is not a skill of its own.
fn skill_entries() -> Vec<Entry> {
    vec![
        entry(".claude", true),
        entry(".claude/skills", true),
        entry(".claude/skills/code-review", true),
        entry(".claude/skills/code-review/SKILL.md", false),
        entry(".claude/skills/code-review/references/rules.md", false),
    ]
}

fn facts_for(rel: &str, text: &'static str) -> impl Fn(&str) -> ContentFacts {
    let owned = rel.to_string();
    move |r: &str| {
        if r == owned {
            read_content_facts(text)
        } else {
            ContentFacts::default()
        }
    }
}

/// Find a node by its project-relative path anywhere in the tree.
fn find<'a>(node: &'a TreeNode, path: &str) -> Option<&'a TreeNode> {
    if node.path == path {
        return Some(node);
    }
    node.children
        .as_ref()
        .and_then(|kids| kids.iter().find_map(|c| find(c, path)))
}

fn project_with_synthesis() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join(".synthesis")).unwrap();
    tmp
}

mod vocabulary;
mod skill_names;
mod classification;
mod assignments;
mod tree_shape;
mod scope_rules;
mod scan_skills;
mod change_paths;
mod wire_shapes;
