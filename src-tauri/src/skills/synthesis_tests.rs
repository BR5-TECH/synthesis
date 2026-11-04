//! Tests for the synthesis skills of `.synthesis/skills` and the opt-out part
//! of the per-agent filter (`DSL-dynamic-skills-loading.md`).

use tauri::Manager;

use super::*;
use crate::tools::tests::{mounted, write_skill};

const GLOSSARY: &str = ".synthesis/skills/glossary/SKILL.md";
const STYLE: &str = ".synthesis/skills/style/SKILL.md";
const ANALYST: &str = ".claude/skills/analyst/SKILL.md";

/// A project with two synthesis skills and one project skill.
fn synthesis_project() -> crate::tools::tests::Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".synthesis/skills",
        "glossary",
        "name: glossary\ndescription: Use the project glossary of domain terms.",
        "# Glossary",
    );
    write_skill(
        root,
        ".synthesis/skills",
        "style",
        "name: style\ndescription: Follow the house writing style guide.",
        "# Style",
    );
    write_skill(
        root,
        ".claude/skills",
        "analyst",
        "name: analyst\ndescription: Capture a feature request as a specification.",
        "# Analyst",
    );
    mounted(dir)
}

fn listed_paths(indexer: &Bm25Indexer, filter: &SkillFilter) -> Vec<String> {
    list_skills(indexer, filter)
        .into_iter()
        .filter(|s| s.ecosystem != Ecosystem::Builtin)
        .map(|s| s.path)
        .collect()
}

fn descriptor(path: &str, ecosystem: Ecosystem) -> SkillDescriptor {
    SkillDescriptor {
        path: path.into(),
        ecosystem,
        folder_name: "x".into(),
        name: "x".into(),
        description: "does x".into(),
    }
}

// ---------------------------------------------------------------------------
// DSL-FR-02 / DSL-FR-NWKE — where a synthesis skill lives
// ---------------------------------------------------------------------------

// DSL-FR-02, DSL-FR-NWKE, DSL-FR-12, DSL-FR-25: `.synthesis/skills` is the
// fifth special folder. Its skills carry the `synthesis` ecosystem and sort
// after `.opencode` and before the built-in skills.
#[test]
fn the_synthesis_folder_is_enumerated_with_its_own_ecosystem() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".opencode/skills", "d", "name: D\ndescription: does d", "");
    write_skill(root, ".synthesis/skills", "e", "name: E\ndescription: does e", "");

    let (skills, excluded) = enumerate(root);
    assert!(excluded.is_empty());
    assert_eq!(
        skills.iter().map(|s| (s.path.as_str(), s.ecosystem)).collect::<Vec<_>>(),
        vec![
            (".opencode/skills/d/SKILL.md", Ecosystem::Opencode),
            (".synthesis/skills/e/SKILL.md", Ecosystem::Synthesis),
        ],
    );
    assert_eq!(skills[1].folder_name, "e");
    assert_eq!(Ecosystem::Synthesis.as_str(), "synthesis");
    assert_eq!(serde_json::to_value(Ecosystem::Synthesis).unwrap(), "synthesis");
    assert_eq!(enumerate(root).0, skills, "the same tree enumerates the same way");
}

// DSL-FR-NWKE, DSL-FR-02, DSL-FR-03, DSL-FR-07, DSL-FR-08, DSL-FR-09,
// DSL-FR-10: the eligibility rules of the four folders apply to the fifth the
// same way, and the folder is matched at the project root alone.
#[test]
fn the_synthesis_folder_follows_the_same_eligibility_rules() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".synthesis/skills", "unnamed", "description: no name declared", "");
    write_skill(root, ".synthesis/skills", "silent", "name: silent", "");
    write_skill(
        root,
        ".synthesis/skills",
        "hidden",
        "name: hidden\ndescription: opts out\ndisable-model-invocation: true",
        "",
    );
    root.write_text_atomic(
        root.join(".synthesis/skills/broken/SKILL.md"),
        "---\nname: X\ndescription: never closed\n\n# Body\n",
    )
    .unwrap();
    root.write_text_atomic(root.join(".synthesis/skills/bare/SKILL.md"), "# No frontmatter\n")
        .unwrap();
    write_skill(root, "pkg/.synthesis/skills", "nested", "name: n\ndescription: not root", "");
    write_skill(root, ".synthesis/skills/deep", "nested", "name: n\ndescription: too deep", "");
    root.write_text_atomic(root.join(".synthesis/skills/loose.md"), "---\nname: l\n---\n").unwrap();
    // Other `.synthesis` folders hold no skills.
    write_skill(root, ".synthesis/drafts", "draft", "name: d\ndescription: a draft", "");

    let (skills, excluded) = enumerate(root);
    assert_eq!(
        skills.iter().map(|s| (s.path.as_str(), s.name.as_str())).collect::<Vec<_>>(),
        vec![(".synthesis/skills/unnamed/SKILL.md", "unnamed")],
        "the name falls back to the folder, and nothing else is eligible",
    );
    let mut excluded: Vec<&str> = excluded.iter().map(|e| e.path.as_str()).collect();
    excluded.sort_unstable();
    assert_eq!(
        excluded,
        vec![
            ".synthesis/skills/bare/SKILL.md",
            ".synthesis/skills/broken/SKILL.md",
            ".synthesis/skills/hidden/SKILL.md",
            ".synthesis/skills/silent/SKILL.md",
        ],
        "no frontmatter, an unclosed block, an opt-out of model invocation, and no description",
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-VJZD — the opt-out part of the filter
// ---------------------------------------------------------------------------

// DSL-FR-VJZD: built-in skills always pass, synthesis skills pass unless
// disabled, and project skills pass only when selected. A path in the wrong
// list has no effect.
#[test]
fn the_filter_admits_synthesis_skills_unless_disabled() {
    let synthesis = descriptor(GLOSSARY, Ecosystem::Synthesis);
    let project = descriptor(ANALYST, Ecosystem::Claude);
    let builtin = descriptor("builtin:mermaid/SKILL.md", Ecosystem::Builtin);

    let empty = SkillFilter::empty();
    assert!(empty.admits(&synthesis) && empty.admits(&builtin) && !empty.admits(&project));

    let disabled = SkillFilter::for_agent(Vec::<String>::new(), [GLOSSARY]);
    assert!(!disabled.admits(&synthesis));
    assert!(disabled.admits(&builtin));
    assert!(disabled.admits(&descriptor(STYLE, Ecosystem::Synthesis)), "only that path");
    assert_eq!(disabled.disabled_path_count(), 1);

    // A project path in the opt-outs does not refuse the project skill it
    // names, and a synthesis path in the selection does not admit anything
    // that was not admitted already.
    let crossed =
        SkillFilter::for_agent([GLOSSARY, ANALYST], [ANALYST, "builtin:mermaid/SKILL.md"]);
    assert!(crossed.admits(&project), "the selection admits it; the opt-out has no effect");
    assert!(crossed.admits(&builtin), "a built-in skill cannot be turned off");
    let crossed = SkillFilter::for_agent([GLOSSARY], [GLOSSARY]);
    assert!(!crossed.admits(&synthesis), "a synthesis skill obeys the opt-out alone");
}

// DSL-FR-VJZD, DSL-FR-15, DSL-FR-16: against a mounted registry, an empty
// filter lists and ranks every synthesis skill, and an opt-out removes that
// one skill from both.
#[test]
fn list_and_search_hold_synthesis_skills_unless_disabled() {
    let fixture = synthesis_project();
    let indexer = fixture.app.state::<Bm25Indexer>();

    assert_eq!(listed_paths(&indexer, &SkillFilter::empty()), vec![GLOSSARY, STYLE]);
    // DSL-FR-15: path order puts every synthesis skill before the built-in
    // skills.
    let every = list_skills(&indexer, &SkillFilter::all_project());
    let order: Vec<Ecosystem> =
        list_skills(&indexer, &SkillFilter::empty()).into_iter().map(|s| s.ecosystem).collect();
    assert_eq!(order.first(), Some(&Ecosystem::Synthesis));
    let first_builtin = order.iter().position(|e| *e == Ecosystem::Builtin).unwrap();
    assert!(order[first_builtin..].iter().all(|e| *e == Ecosystem::Builtin));
    assert_eq!(every.last().map(|s| s.path.as_str()), Some(STYLE));
    let ranked = search_skills(&indexer, "glossary of domain terms", 5, &SkillFilter::empty());
    assert_eq!(ranked.first().map(|r| r.skill.path.as_str()), Some(GLOSSARY));

    let off = SkillFilter::for_agent([ANALYST], [GLOSSARY]);
    assert_eq!(listed_paths(&indexer, &off), vec![ANALYST, STYLE]);
    let ranked = search_skills(&indexer, "glossary of domain terms", 5, &off);
    assert!(ranked.iter().all(|r| r.skill.path != GLOSSARY), "never ranked when off");
}

// ---------------------------------------------------------------------------
// DSL-FR-PEHR — `list_project_skills`
// ---------------------------------------------------------------------------

// DSL-FR-PEHR: the command returns the synthesis skills beside the project
// skills, in path order, and never a built-in skill.
#[test]
fn list_project_skills_returns_the_synthesis_skills_too() {
    let fixture = synthesis_project();
    let indexer = fixture.app.state::<Bm25Indexer>();
    let skills = list_project_skills_impl(&indexer).unwrap();
    assert_eq!(
        skills.iter().map(|s| (s.path.as_str(), s.ecosystem)).collect::<Vec<_>>(),
        vec![
            (ANALYST, Ecosystem::Claude),
            (GLOSSARY, Ecosystem::Synthesis),
            (STYLE, Ecosystem::Synthesis),
        ],
    );
    let json = serde_json::to_value(&skills[1]).unwrap();
    assert_eq!(json["ecosystem"], "synthesis");
}
