//! Tests for the built-in source, the per-agent filter, and the
//! `list_project_skills` command (`DSL-dynamic-skills-loading.md`).

use tauri::Manager;

use super::*;
use crate::logging::{LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{closed_project, demo_project, mounted, write_skill};

fn builtin_named<'a>(skills: &'a [SkillDescriptor], name: &str) -> Option<&'a SkillDescriptor> {
    skills.iter().find(|s| s.ecosystem == Ecosystem::Builtin && s.name == name)
}

// ---------------------------------------------------------------------------
// DSL-FR-WTPA / DSL-FR-UYMA / DSL-FR-YBDO — the embedded source
// ---------------------------------------------------------------------------

// DSL-FR-WTPA: every file under a skill folder is embedded, subfolders
// included, with a path relative to `resources/skills/`.
#[test]
fn the_build_embeds_every_file_of_a_skill_folder_with_its_subfolders() {
    let files = builtin::files();
    assert!(files.iter().any(|(path, _)| *path == "mermaid/SKILL.md"));
    assert!(
        files.iter().any(|(path, _)| *path == "mermaid/references/agentflow-beta.md"),
        "a file in a subfolder is embedded too",
    );
    let mut sorted: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
    sorted.sort();
    assert_eq!(sorted, files.iter().map(|(p, _)| *p).collect::<Vec<_>>(), "sorted by path");
    for (path, _) in files {
        assert!(!path.contains('\\') && !path.starts_with('/'), "{path} is relative, with '/'");
    }
}

// DSL-FR-UYMA: every built-in skill the application ships is eligible.
#[test]
fn every_shipped_builtin_skill_is_eligible() {
    let (skills, excluded) = enumerate_builtin();
    assert!(excluded.is_empty(), "built-in skills that are not eligible: {excluded:?}");
    let folders: std::collections::BTreeSet<&str> = builtin::files()
        .iter()
        .filter_map(|(path, _)| path.strip_suffix("/SKILL.md"))
        .filter(|folder| !folder.contains('/'))
        .collect();
    assert_eq!(skills.len(), folders.len(), "one descriptor per skill folder");
}

// DSL-FR-YBDO / DSL-FR-12: the descriptor of a built-in skill.
#[test]
fn a_builtin_descriptor_has_the_builtin_ecosystem_and_a_virtual_path() {
    let (skills, _) = enumerate_builtin();
    let mermaid = builtin_named(&skills, "mermaid").expect("the mermaid skill");
    assert_eq!(mermaid.ecosystem, Ecosystem::Builtin);
    assert_eq!(mermaid.folder_name, "mermaid");
    assert_eq!(mermaid.path, "builtin:mermaid/SKILL.md");
    assert!(!mermaid.description.trim().is_empty());
    assert_eq!(Ecosystem::Builtin.as_str(), "builtin");
    assert_eq!(serde_json::to_value(Ecosystem::Builtin).unwrap(), "builtin");
}

// DSL-FR-VTMI: the `mermaid` skill names one reference file per diagram type,
// and every file it names is embedded and holds at least one diagram.
#[test]
fn the_mermaid_skill_names_a_reference_for_each_diagram_type_and_each_exists() {
    let skill = builtin_skill_file("mermaid", "SKILL.md").expect("SKILL.md");
    let named: Vec<&str> = skill
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| token.starts_with("references/") && token.ends_with(".md"))
        .collect();
    let embedded: Vec<&str> = builtin::files()
        .iter()
        .filter_map(|(path, _)| path.strip_prefix("mermaid/"))
        .filter(|path| path.starts_with("references/"))
        .collect();
    for file in &embedded {
        assert!(named.contains(file), "SKILL.md does not name {file}");
    }
    for file in &named {
        let text = builtin_skill_file("mermaid", file).unwrap_or_else(|| panic!("{file} missing"));
        assert!(text.contains("```mermaid"), "{file} holds no complete diagram");
    }
    // The diagram types Mermaid 12.2.0 registers, by their declaration. A
    // type is covered when some reference file declares it.
    let all_text: String = embedded
        .iter()
        .map(|f| builtin_skill_file("mermaid", f).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for keyword in [
        "flowchart",
        "sequenceDiagram",
        "classDiagram",
        "stateDiagram-v2",
        "erDiagram",
        "journey",
        "gantt",
        "pie",
        "quadrantChart",
        "requirementDiagram",
        "gitGraph",
        "C4Context",
        "C4Container",
        "C4Component",
        "C4Dynamic",
        "C4Deployment",
        "mindmap",
        "timeline",
        "sankey",
        "xychart",
        "block",
        "packet",
        "kanban",
        "architecture-beta",
        "radar-beta",
        "treemap",
        "agentflow-beta",
        "swimlane-beta",
        "treeView-beta",
        "eventmodeling",
        "ishikawa",
        "venn-beta",
        "usecase-beta",
        "wardley-beta",
        "cynefin-beta",
        "railroad-beta",
        "railroad-ebnf-beta",
        "railroad-abnf-beta",
        "railroad-peg-beta",
    ] {
        assert!(
            all_text.contains(&format!("```mermaid\n{keyword}"))
                || all_text.contains(&format!("---\n{keyword}"))
                || all_text.contains(&format!("\n{keyword}")),
            "no reference shows a {keyword} diagram",
        );
    }
}

// ---------------------------------------------------------------------------
// DSL-FR-LFCA — the embedded files
// ---------------------------------------------------------------------------

// DSL-FR-LFCA: a file relative to the skill folder, and none for the cases
// the requirement lists.
#[test]
fn builtin_skill_file_reads_inside_one_skill_folder_and_nowhere_else() {
    let reference = builtin_skill_file("mermaid", "references/agentflow-beta.md");
    assert!(reference.is_some_and(|t| t.contains("agentflow-beta")));
    assert_eq!(
        builtin_skill_file("mermaid", "./references//agentflow-beta.md"),
        reference,
        "`.` segments and repeated separators name the same file",
    );
    assert!(builtin_skill_file("mermaid", "references/no-such-diagram.md").is_none());
    assert!(builtin_skill_file("no-such-skill", "SKILL.md").is_none());
    assert!(builtin_skill_file("mermaid", "../mermaid/SKILL.md").is_none(), "no `..`");
    assert!(builtin_skill_file("mermaid", "references/../SKILL.md").is_none(), "no `..`");
    assert!(builtin_skill_file("mermaid", "/mermaid/SKILL.md").is_none(), "not absolute");
    assert!(builtin_skill_file("mermaid", "\\SKILL.md").is_none(), "not absolute");
    assert!(builtin_skill_file("mermaid", "C:/SKILL.md").is_none(), "not a drive path");
    assert!(builtin_skill_file("mermaid", "references\\..\\SKILL.md").is_none());
    assert!(builtin_skill_file("mermaid", "").is_none());
    assert!(builtin_skill_file("mermaid", "references").is_none(), "a folder is no file");
}

// ---------------------------------------------------------------------------
// DSL-FR-ZUFC / DSL-FR-17 / DSL-FR-RJKY — the built-in skills join every pass
// ---------------------------------------------------------------------------

// DSL-FR-ZUFC: an open project with no skill folder holds the built-in skills,
// in the registry and in the `skills` index, and DSL-FR-15 still orders by
// path with them after the project skills.
#[test]
fn every_pass_adds_the_builtin_skills_to_the_registry_and_the_index() {
    let empty = crate::tools::tests::empty_project();
    let indexer = empty.app.state::<Bm25Indexer>();
    let listed = list_skills(&indexer, &SkillFilter::empty());
    assert!(builtin_named(&listed, "mermaid").is_some(), "present with no skill folder");
    let ranked = search_skills(&indexer, "sequence diagram flowchart", 5, &SkillFilter::empty());
    assert!(ranked.iter().any(|r| r.skill.name == "mermaid"), "and rankable");

    let demo = demo_project();
    let indexer = demo.app.state::<Bm25Indexer>();
    let every = indexer.snapshot().skills().to_vec();
    let paths: Vec<&str> = every.iter().map(|s| s.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "DSL-FR-15: path order");
    assert_eq!(every.last().unwrap().ecosystem, Ecosystem::Builtin, "`builtin:` sorts last");
}

// DSL-FR-17 / DSL-FR-RJKY: a project skill named like a built-in skill is a
// second skill, not a replacement.
#[test]
fn a_project_skill_named_like_a_builtin_skill_does_not_replace_it() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "mermaid",
        "name: mermaid\ndescription: The project's own diagram notes.",
        "Project body.",
    );
    let fixture = mounted(dir);
    let indexer = fixture.app.state::<Bm25Indexer>();
    let filter = SkillFilter::with_project_paths([".claude/skills/mermaid/SKILL.md"]);
    let listed = list_skills(&indexer, &filter);
    let mermaids: Vec<(&str, Ecosystem)> = listed
        .iter()
        .filter(|s| s.name == "mermaid")
        .map(|s| (s.path.as_str(), s.ecosystem))
        .collect();
    assert_eq!(
        mermaids,
        vec![
            (".claude/skills/mermaid/SKILL.md", Ecosystem::Claude),
            ("builtin:mermaid/SKILL.md", Ecosystem::Builtin),
        ],
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-VJZD / DSL-FR-15 / DSL-FR-16 — the filter
// ---------------------------------------------------------------------------

// DSL-FR-VJZD: the filter admits every built-in skill, and a project skill
// only by its path. A path naming nothing admits nothing. The fixture holds no
// synthesis skill, which `synthesis_tests.rs` covers.
#[test]
fn the_filter_admits_builtin_skills_and_selected_project_skills() {
    let fixture = demo_project();
    let indexer = fixture.app.state::<Bm25Indexer>();

    let none = list_skills(&indexer, &SkillFilter::empty());
    assert!(none.iter().all(|s| s.ecosystem == Ecosystem::Builtin));
    assert!(builtin_named(&none, "mermaid").is_some());

    let one = list_skills(
        &indexer,
        &SkillFilter::with_project_paths([
            ".claude/skills/analyst/SKILL.md",
            ".claude/skills/no-such-skill/SKILL.md",
        ]),
    );
    let project: Vec<&str> =
        one.iter().filter(|s| s.ecosystem != Ecosystem::Builtin).map(|s| s.path.as_str()).collect();
    assert_eq!(project, vec![".claude/skills/analyst/SKILL.md"]);
    assert!(builtin_named(&one, "mermaid").is_some(), "built-in skills are not selected");

    // A name is not an identity: selecting by name admits nothing.
    let by_name = list_skills(&indexer, &SkillFilter::with_project_paths(["analyst"]));
    assert!(by_name.iter().all(|s| s.ecosystem == Ecosystem::Builtin));
}

// DSL-FR-16: the filter applies before the limit, so a refused skill takes no
// place in the result.
#[test]
fn the_search_filter_applies_before_the_limit() {
    let fixture = demo_project();
    let indexer = fixture.app.state::<Bm25Indexer>();
    let query = "review a specification for internal contradictions";
    let open = search_skills(&indexer, query, 1, &SkillFilter::all_project());
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].skill.name, "analyst", "precondition: analyst ranks first");

    let deployer_only = SkillFilter::with_project_paths([".codex/skills/deployer/SKILL.md"]);
    let ranked = search_skills(&indexer, "deploy container image production", 1, &deployer_only);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].skill.name, "deployer");

    // A query that ranks the refused skill first and the admitted one below
    // it: a filter applied after the limit would answer an empty list here.
    let mixed = "review a specification for internal contradictions and deploy a container image";
    let open = search_skills(&indexer, mixed, 10, &SkillFilter::all_project());
    assert_eq!(open[0].skill.name, "analyst", "precondition: analyst ranks first");
    assert!(open.iter().any(|r| r.skill.name == "deployer"), "precondition: deployer matches");
    let refused = search_skills(&indexer, mixed, 1, &deployer_only);
    let names: Vec<&str> = refused.iter().map(|r| r.skill.name.as_str()).collect();
    assert_eq!(names, vec!["deployer"], "the refused skill takes no place, even ranked first");
    assert!(search_skills(&indexer, query, 0, &deployer_only).is_empty(), "a limit of zero");
}

// DSL-FR-15 / DSL-FR-16: with no project open, both answer an empty list,
// built-in skills included.
#[test]
fn a_closed_project_lists_and_ranks_no_skill_at_all() {
    let app = closed_project();
    let indexer = app.state::<Bm25Indexer>();
    assert!(list_skills(&indexer, &SkillFilter::empty()).is_empty());
    assert!(search_skills(&indexer, "diagram", 10, &SkillFilter::empty()).is_empty());
}

// ---------------------------------------------------------------------------
// DSL-FR-PEHR / DSL-FR-WJPI — `list_project_skills`
// ---------------------------------------------------------------------------

static DSL_BUFFER: LogBuffer = LogBuffer::new();

// DSL-FR-PEHR: every project skill in path order, never a built-in skill, and
// the typed refusal with no project open. DSL-FR-WJPI: one record each, naming
// the count or the reason and no path, name, or description.
#[test]
fn list_project_skills_returns_the_project_skills_and_logs_their_count() {
    DSL_BUFFER.clear();
    let fixture = demo_project();
    let indexer = fixture.app.state::<Bm25Indexer>();
    let skills = list_project_skills_logged(&fixture.handle(), &DSL_BUFFER, &indexer).unwrap();
    let paths: Vec<&str> = skills.iter().map(|s| s.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert!(!skills.is_empty());
    assert!(skills.iter().all(|s| s.ecosystem != Ecosystem::Builtin));
    assert!(skills
        .iter()
        .any(|s| s.path == ".claude/skills/analyst/SKILL.md" && s.name == "analyst"));
    let json = serde_json::to_value(&skills[0]).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["description", "ecosystem", "name", "path"]);

    let closed = closed_project();
    let refused =
        list_project_skills_logged(closed.handle(), &DSL_BUFFER, &closed.state::<Bm25Indexer>());
    assert_eq!(refused, Err("no_project_open".to_string()));

    let page = DSL_BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 100)
        .unwrap();
    assert_eq!(page.records.len(), 2);
    assert_eq!(page.records[0].level, LogLevel::Debug);
    assert_eq!(page.records[0].fields.get("skills").unwrap(), &serde_json::json!(skills.len()));
    assert_eq!(page.records[1].level, LogLevel::Warn);
    assert_eq!(page.records[1].fields.get("reason").unwrap(), "no_project_open");
    let text = serde_json::to_string(&page.records).unwrap();
    for leak in ["analyst", ".claude", "Review a specification"] {
        assert!(!text.contains(leak), "a record names {leak:?}");
    }
}

// DSL-FR-ZUFC: the built-in skills are present from the moment a content root
// is mounted, before its first pass, and again after a switch to another root
// — in the registry and in the `skills` index together (DSL-FR-18).
#[test]
fn the_builtin_skills_are_present_at_mount_before_any_pass() {
    let app = closed_project();
    let indexer = app.state::<Bm25Indexer>();
    let first = tempfile::TempDir::new().unwrap();
    let second = tempfile::TempDir::new().unwrap();
    for root in [first.path(), second.path()] {
        indexer.mount(&crate::fs::RootFs::for_root(root));
        let listed = list_skills(&indexer, &SkillFilter::empty());
        assert!(builtin_named(&listed, "mermaid").is_some(), "listed before any pass");
        let ranked = search_skills(&indexer, "draw a sequence diagram", 5, &SkillFilter::empty());
        assert!(ranked.iter().any(|r| r.skill.name == "mermaid"), "and rankable");
    }
}
