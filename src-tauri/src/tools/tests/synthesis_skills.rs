//! The three skill tools against the synthesis skills of `.synthesis/skills`
//! and the opt-outs of the agent filter (`SLT-skill-list-tool.md`,
//! `SST-skill-search-tool.md`, `LSK-load-skill-tool.md`).

use super::*;
use crate::skills::{Ecosystem, SkillFilter};
use crate::tools::skill_list::{SkillListArgs, SkillListTool};
use crate::tools::skill_load::{LoadSkillArgs, SkillLoadTool};
use crate::tools::skill_search::{SkillSearchArgs, SkillSearchTool};

const GLOSSARY: &str = ".synthesis/skills/glossary/SKILL.md";

/// A project with a synthesis skill that keeps a reference file, and a
/// synthesis skill and an `.opencode` skill that share the name `review`.
fn synthesis_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".synthesis/skills",
        "glossary",
        "name: glossary\ndescription: Use the project glossary of domain terms.",
        "# Glossary\n\nRead `references/terms.md`.",
    );
    std::fs::create_dir_all(root.join(".synthesis/skills/glossary/references")).unwrap();
    std::fs::write(
        root.join(".synthesis/skills/glossary/references/terms.md"),
        "---\nkept: yes\n---\nArtifact: a file of the project.\n",
    )
    .unwrap();
    std::fs::write(root.join(".synthesis/library.toml"), "").unwrap();
    write_skill(
        root,
        ".synthesis/skills",
        "review",
        "name: review\ndescription: Review a document against the house rules.",
        "# Synthesis review",
    );
    write_skill(
        root,
        ".opencode/skills",
        "review",
        "name: review\ndescription: Review a pull request for defects.",
        "# Opencode review",
    );
    mounted(dir)
}

fn load(
    fixture: &Fixture,
    filter: SkillFilter,
    name: &str,
    ecosystem: Option<&str>,
    file: Option<&str>,
) -> Result<String, ToolRefusal> {
    block_on(SkillLoadTool::new(fixture.handle(), filter).call(LoadSkillArgs {
        name: name.to_string(),
        ecosystem: ecosystem.map(str::to_string),
        file: file.map(str::to_string),
    }))
}

fn listed(fixture: &Fixture, filter: SkillFilter) -> Vec<(String, Ecosystem, String)> {
    block_on(SkillListTool::new(fixture.handle(), filter).call(SkillListArgs::default()))
        .unwrap()
        .skills
        .into_iter()
        .filter(|s| s.ecosystem != Ecosystem::Builtin)
        .map(|s| (s.name, s.ecosystem, s.path))
        .collect()
}

fn searched(fixture: &Fixture, filter: SkillFilter, query: &str) -> Vec<(String, Ecosystem)> {
    block_on(
        SkillSearchTool::new(fixture.handle(), filter)
            .call(SkillSearchArgs { query: query.to_string(), limit: None }),
    )
    .unwrap()
    .skills
    .into_iter()
    .map(|s| (s.name, s.ecosystem))
    .collect()
}

fn opencode_review() -> SkillFilter {
    SkillFilter::with_project_paths([".opencode/skills/review/SKILL.md"])
}

// ---------------------------------------------------------------------------
// SLT-FR-06 / SLT-FR-XKDW / SLT-FR-13 — the list
// ---------------------------------------------------------------------------

// SLT-FR-06, SLT-FR-10, SLT-FR-XKDW: an agent with no opt-out lists every
// synthesis skill with the `synthesis` ecosystem and its project path.
#[test]
fn the_list_holds_every_synthesis_skill_by_default() {
    let fixture = synthesis_project();
    assert_eq!(
        listed(&fixture, SkillFilter::empty()),
        vec![
            ("glossary".into(), Ecosystem::Synthesis, GLOSSARY.into()),
            ("review".into(), Ecosystem::Synthesis, ".synthesis/skills/review/SKILL.md".into()),
        ],
    );
}

// SLT-FR-XKDW: a synthesis skill the agent turns off is never listed.
#[test]
fn a_disabled_synthesis_skill_is_not_listed() {
    let fixture = synthesis_project();
    let filter = SkillFilter::for_agent(Vec::<String>::new(), [GLOSSARY]);
    let names: Vec<String> = listed(&fixture, filter).into_iter().map(|(n, ..)| n).collect();
    assert_eq!(names, vec!["review"]);
}

// SLT-FR-13: a name held by an `.opencode` skill and a synthesis skill is
// listed once, and the `.opencode` skill sorts first.
#[test]
fn the_list_prefers_an_opencode_skill_over_a_synthesis_skill_of_the_same_name() {
    let fixture = synthesis_project();
    let reviews: Vec<Ecosystem> = listed(&fixture, opencode_review())
        .into_iter()
        .filter(|(name, ..)| name == "review")
        .map(|(_, ecosystem, _)| ecosystem)
        .collect();
    assert_eq!(reviews, vec![Ecosystem::Opencode]);
}

// SLT-FR-12: a project whose only synthesis skills are all turned off is a
// success carrying the built-in skills alone.
#[test]
fn every_synthesis_skill_turned_off_leaves_the_builtin_skills() {
    let fixture = synthesis_project();
    let filter = SkillFilter::for_agent(
        Vec::<String>::new(),
        [GLOSSARY, ".synthesis/skills/review/SKILL.md"],
    );
    let output =
        block_on(SkillListTool::new(fixture.handle(), filter).call(SkillListArgs::default()))
            .unwrap();
    assert!(!output.skills.is_empty());
    assert!(output.skills.iter().all(|s| s.ecosystem == Ecosystem::Builtin));
}

// ---------------------------------------------------------------------------
// SST-FR-BDHX — the search
// ---------------------------------------------------------------------------

// SST-FR-BDHX, SST-FR-10: a synthesis skill is ranked by default and never
// when the agent turns it off.
#[test]
fn the_search_ranks_a_synthesis_skill_unless_it_is_off() {
    let fixture = synthesis_project();
    let query = "glossary of domain terms";
    assert_eq!(
        searched(&fixture, SkillFilter::empty(), query).first(),
        Some(&("glossary".to_string(), Ecosystem::Synthesis)),
    );
    let off = SkillFilter::for_agent(Vec::<String>::new(), [GLOSSARY]);
    assert!(searched(&fixture, off, query).iter().all(|(name, _)| name != "glossary"));
}

// ---------------------------------------------------------------------------
// LSK-FR-SGVX / LSK-FR-BSJS / LSK-FR-07 — loading
// ---------------------------------------------------------------------------

// LSK-FR-SGVX, LSK-FR-13, LSK-FR-UBQE: a synthesis skill loads like a project
// skill: its instructions without frontmatter, and a reference file whole.
#[test]
fn a_synthesis_skill_and_its_reference_file_load() {
    let fixture = synthesis_project();
    let instructions = load(&fixture, SkillFilter::empty(), "glossary", None, None).unwrap();
    assert_eq!(instructions, "\n# Glossary\n\nRead `references/terms.md`.\n");
    let terms = load(&fixture, SkillFilter::empty(), "glossary", None, Some("references/terms.md"))
        .unwrap();
    assert_eq!(terms, "---\nkept: yes\n---\nArtifact: a file of the project.\n");
}

// LSK-FR-PZZE, LSK-FR-KDVE: a file outside the synthesis skill's folder is
// refused, also a file elsewhere in `.synthesis/`.
#[test]
fn a_file_outside_the_synthesis_skill_folder_is_refused() {
    let fixture = synthesis_project();
    for path in ["../../library.toml", "../review/SKILL.md", "/etc/hosts"] {
        assert_eq!(
            load(&fixture, SkillFilter::empty(), "glossary", None, Some(path)),
            Err(ToolRefusal::InvalidArguments(crate::tools::refusals::SKILL_FILE_OUTSIDE)),
            "{path}",
        );
    }
}

// LSK-FR-BSJS: a synthesis skill the agent turns off is unknown, with or
// without `file`.
#[test]
fn a_disabled_synthesis_skill_is_unknown_with_or_without_file() {
    let fixture = synthesis_project();
    let off = SkillFilter::for_agent(Vec::<String>::new(), [GLOSSARY]);
    assert_eq!(
        load(&fixture, off.clone(), "glossary", None, None),
        Err(ToolRefusal::SkillNotFound)
    );
    assert_eq!(
        load(&fixture, off, "glossary", None, Some("references/terms.md")),
        Err(ToolRefusal::SkillNotFound),
    );
}

// LSK-FR-BSJS: a turned-off synthesis skill does not show through the
// ambiguity refusal. Its name resolves to the selected `.opencode` twin, and
// `ecosystem: "synthesis"` reports it unknown.
#[test]
fn a_disabled_twin_is_not_named_by_the_ambiguity_refusal() {
    let fixture = synthesis_project();
    let filter = SkillFilter::for_agent(
        [".opencode/skills/review/SKILL.md"],
        [".synthesis/skills/review/SKILL.md"],
    );
    let opencode = load(&fixture, filter.clone(), "review", None, None).unwrap();
    assert!(opencode.contains("# Opencode review"));
    assert_eq!(
        load(&fixture, filter, "review", Some("synthesis"), None),
        Err(ToolRefusal::SkillNotFound),
    );
}

// LSK-FR-07, LSK-FR-10: `synthesis` narrows a shared name to the synthesis
// skill, and without it the name is ambiguous between the two ecosystems.
#[test]
fn the_synthesis_ecosystem_narrows_a_shared_name() {
    let fixture = synthesis_project();
    assert_eq!(
        load(&fixture, opencode_review(), "review", None, None),
        Err(ToolRefusal::SkillAmbiguous(vec![Ecosystem::Opencode, Ecosystem::Synthesis])),
    );
    let synthesis = load(&fixture, opencode_review(), "review", Some("synthesis"), None).unwrap();
    assert!(synthesis.contains("# Synthesis review"));
    let opencode = load(&fixture, opencode_review(), "review", Some("opencode"), None).unwrap();
    assert!(opencode.contains("# Opencode review"));
}
