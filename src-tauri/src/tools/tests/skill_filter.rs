//! The three skill tools against the built-in skills, the per-agent filter,
//! and the `file` argument of `load_skill` (`SST-skill-search-tool.md`,
//! `SLT-skill-list-tool.md`, `LSK-load-skill-tool.md`).

use super::*;
use crate::skills::{Ecosystem, SkillFilter};
use crate::tools::skill_list::{SkillListArgs, SkillListTool};
use crate::tools::skill_load::{LoadSkillArgs, SkillLoadTool};
use crate::tools::skill_search::{SkillSearchArgs, SkillSearchTool};

/// A project with two skills, one of which keeps a reference file beside its
/// `SKILL.md`, as the four ecosystems lay them out.
fn referencing_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".claude/skills",
        "release",
        "name: release\ndescription: Cut a release of the desktop application.",
        "# Release\n\nRead `references/checklist.md` before you start.",
    );
    std::fs::create_dir_all(root.join(".claude/skills/release/references")).unwrap();
    std::fs::write(
        root.join(".claude/skills/release/references/checklist.md"),
        "---\ntitle: kept\n---\n1. Tag the commit.\n",
    )
    .unwrap();
    std::fs::write(root.join("outside.md"), "Not part of any skill.\n").unwrap();
    write_skill(
        root,
        ".codex/skills",
        "triage",
        "name: triage\ndescription: Triage an incoming bug report by severity.",
        "# Triage",
    );
    mounted(dir)
}

const RELEASE: &str = ".claude/skills/release/SKILL.md";

fn only_release() -> SkillFilter {
    SkillFilter::with_project_paths([RELEASE])
}

fn load(
    fixture: &Fixture,
    filter: SkillFilter,
    name: &str,
    file: Option<&str>,
) -> Result<String, ToolRefusal> {
    block_on(SkillLoadTool::new(fixture.handle(), filter).call(LoadSkillArgs {
        name: name.to_string(),
        ecosystem: None,
        file: file.map(str::to_string),
    }))
}

fn listed(fixture: &Fixture, filter: SkillFilter) -> Vec<(String, Ecosystem, String)> {
    block_on(SkillListTool::new(fixture.handle(), filter).call(SkillListArgs::default()))
        .unwrap()
        .skills
        .into_iter()
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

// ---------------------------------------------------------------------------
// SLT-FR-XKDW / SLT-FR-GTZM / SLT-FR-12 — the list
// ---------------------------------------------------------------------------

// SLT-FR-XKDW: only the selected project skill is listed. SLT-FR-GTZM: the
// built-in skills are listed for every agent, with a virtual path.
#[test]
fn the_list_holds_the_selected_project_skills_and_every_builtin_skill() {
    let fixture = referencing_project();
    let entries = listed(&fixture, only_release());
    assert!(entries.contains(&("release".into(), Ecosystem::Claude, RELEASE.into())));
    assert!(!entries.iter().any(|(name, _, _)| name == "triage"), "not selected, not named");
    assert!(entries.contains(&(
        "mermaid".into(),
        Ecosystem::Builtin,
        "builtin:mermaid/SKILL.md".into()
    )));
}

// SLT-FR-12 / SLT-FR-GTZM: an agent that selects nothing, in a project with
// no synthesis skill, gets the built-in skills alone, as a success.
#[test]
fn an_agent_that_selects_nothing_lists_no_project_skill() {
    let fixture = referencing_project();
    let entries = listed(&fixture, SkillFilter::empty());
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|(_, ecosystem, _)| *ecosystem == Ecosystem::Builtin));
}

// SLT-FR-13: a project skill and a built-in skill of one name — the project
// copy sorts first and stands for the name.
#[test]
fn a_project_skill_named_like_a_builtin_skill_stands_for_the_name_in_the_list() {
    let dir = TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".github/skills",
        "mermaid",
        "name: mermaid\ndescription: The project's own diagram notes.",
        "Body.",
    );
    let fixture = mounted(dir);
    let entries =
        listed(&fixture, SkillFilter::with_project_paths([".github/skills/mermaid/SKILL.md"]));
    let mermaids: Vec<&Ecosystem> =
        entries.iter().filter(|(n, _, _)| n == "mermaid").map(|(_, e, _)| e).collect();
    assert_eq!(mermaids, vec![&Ecosystem::Github]);
}

// ---------------------------------------------------------------------------
// SST-FR-BDHX / SST-FR-TKLW — the search
// ---------------------------------------------------------------------------

// SST-FR-BDHX: a project skill the agent does not select is never ranked,
// even for a query only it matches.
#[test]
fn the_search_never_ranks_a_project_skill_the_agent_does_not_select() {
    let fixture = referencing_project();
    assert!(
        searched(&fixture, SkillFilter::all_project(), "triage bug report severity")
            .iter()
            .any(|(n, _)| n == "triage"),
        "precondition: the query matches triage"
    );
    let ranked = searched(&fixture, only_release(), "triage bug report severity");
    assert!(!ranked.iter().any(|(n, _)| n == "triage"));
}

// SST-FR-TKLW: a built-in skill is a match like any other, for every agent.
#[test]
fn the_search_ranks_a_builtin_skill_for_every_agent() {
    let fixture = referencing_project();
    for filter in [SkillFilter::empty(), only_release()] {
        let ranked = searched(&fixture, filter, "draw a sequence diagram of the login flow");
        assert_eq!(ranked.first(), Some(&("mermaid".to_string(), Ecosystem::Builtin)));
    }
}

// ---------------------------------------------------------------------------
// LSK-FR-BSJS / LSK-FR-SGVX / LSK-FR-TLXK — loading by agent
// ---------------------------------------------------------------------------

// LSK-FR-BSJS: an unselected project skill is unknown, with or without
// `file`, and the refusal does not say that it exists.
#[test]
fn an_unselected_project_skill_is_unknown_with_or_without_file() {
    let fixture = referencing_project();
    assert!(load(&fixture, SkillFilter::all_project(), "triage", None).is_ok(), "precondition");
    let filter = SkillFilter::empty();
    assert_eq!(load(&fixture, filter.clone(), "release", None), Err(ToolRefusal::SkillNotFound));
    assert_eq!(
        load(&fixture, filter, "release", Some("references/checklist.md")),
        Err(ToolRefusal::SkillNotFound),
    );
    assert_eq!(load(&fixture, only_release(), "triage", None), Err(ToolRefusal::SkillNotFound));
}

// LSK-FR-SGVX: a built-in skill loads like a project skill — its instructions
// without the frontmatter. LSK-FR-TLXK: its reference file comes from the
// embedded folder, whole.
#[test]
fn a_builtin_skill_and_its_reference_file_load_from_the_binary() {
    let fixture = referencing_project();
    let instructions = load(&fixture, SkillFilter::empty(), "mermaid", None).unwrap();
    assert!(
        instructions.starts_with("\n# Mermaid diagrams") || instructions.starts_with("# Mermaid")
    );
    assert!(!instructions.contains("description:"), "the frontmatter is removed");

    let reference =
        load(&fixture, SkillFilter::empty(), "mermaid", Some("references/agentflow-beta.md"))
            .unwrap();
    assert_eq!(
        reference,
        crate::skills::builtin_skill_file("mermaid", "references/agentflow-beta.md").unwrap(),
    );
    let whole = load(&fixture, SkillFilter::empty(), "mermaid", Some("SKILL.md")).unwrap();
    assert!(
        whole.starts_with("---\nname: mermaid"),
        "LSK-FR-UBQE: `SKILL.md` keeps its frontmatter"
    );

    // `ecosystem: builtin` narrows to the built-in copy (LSK-FR-07).
    let narrowed =
        block_on(SkillLoadTool::new(fixture.handle(), SkillFilter::empty()).call(LoadSkillArgs {
            name: "MERMAID ".to_string(),
            ecosystem: Some("builtin".to_string()),
            file: None,
        }));
    assert_eq!(narrowed.unwrap(), instructions);
}

// LSK-FR-SGVX: a built-in skill reads no project file, so a project holding a
// file at the same relative path changes nothing.
#[test]
fn a_builtin_skill_never_reads_the_project() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("mermaid/references")).unwrap();
    std::fs::write(dir.path().join("mermaid/references/pie.md"), "PROJECT COPY").unwrap();
    let fixture = mounted(dir);
    let text = load(&fixture, SkillFilter::empty(), "mermaid", Some("references/pie.md")).unwrap();
    assert!(!text.contains("PROJECT COPY"));
}

// LSK-FR-16: with no project open, a built-in skill is refused like any other.
#[test]
fn a_builtin_skill_is_refused_with_no_project_open() {
    let app = closed_project();
    let result = block_on(
        SkillLoadTool::new(app.handle().clone(), SkillFilter::empty()).call(LoadSkillArgs {
            name: "mermaid".to_string(),
            ecosystem: None,
            file: None,
        }),
    );
    assert_eq!(result, Err(ToolRefusal::NoProjectOpen));
}

// ---------------------------------------------------------------------------
// LSK-FR-UBQE / LSK-FR-PZZE / LSK-FR-KDVE / LSK-FR-ULLF — `file`
// ---------------------------------------------------------------------------

// LSK-FR-UBQE: a project skill's file is returned whole, frontmatter and all,
// and a blank `file` is no `file`.
#[test]
fn a_project_skill_file_is_returned_whole() {
    let fixture = referencing_project();
    let text = load(&fixture, only_release(), "release", Some("references/checklist.md")).unwrap();
    assert_eq!(text, "---\ntitle: kept\n---\n1. Tag the commit.\n");
    let instructions = load(&fixture, only_release(), "release", None).unwrap();
    assert_eq!(load(&fixture, only_release(), "release", Some("   ")).unwrap(), instructions);
    let whole = load(&fixture, only_release(), "release", Some("SKILL.md")).unwrap();
    assert!(whole.starts_with("---\nname: release"));
}

// LSK-FR-PZZE: a path that leaves the skill's folder is the retryable
// `InvalidArgs` refusal, for a project skill and a built-in skill alike.
#[test]
fn a_file_outside_the_skill_folder_is_refused_before_any_read() {
    let fixture = referencing_project();
    for (filter, name) in [(only_release(), "release"), (SkillFilter::empty(), "mermaid")] {
        for path in
            ["../../../outside.md", "/etc/hosts", "references/../../triage/SKILL.md", "..\\x"]
        {
            let refusal = load(&fixture, filter.clone(), name, Some(path)).unwrap_err();
            assert_eq!(
                refusal,
                ToolRefusal::InvalidArguments(crate::tools::refusals::SKILL_FILE_OUTSIDE),
                "{name} {path}",
            );
            assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
            assert!(refusal.retryable());
        }
    }
}

// LSK-FR-ULLF: a missing file, a folder, and a non-text file are the
// retryable `NotFound` refusal, which points at the skill's instructions.
#[test]
fn a_missing_file_a_folder_and_a_binary_file_are_not_found() {
    let fixture = referencing_project();
    let root = fixture.app.state::<Bm25Indexer>().root().unwrap();
    std::fs::write(root.join(".claude/skills/release/logo.bin"), [0xff, 0xfe, 0x00, 0x9f]).unwrap();
    for path in ["references/missing.md", "references", "logo.bin", "."] {
        let refusal = load(&fixture, only_release(), "release", Some(path)).unwrap_err();
        assert_eq!(refusal, ToolRefusal::SkillFileNotFound, "{path}");
        assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
        assert!(refusal.retryable());
        assert!(refusal.to_string().contains("without `file`"));
    }
    assert_eq!(
        load(&fixture, SkillFilter::empty(), "mermaid", Some("references/nothing.md")),
        Err(ToolRefusal::SkillFileNotFound),
    );
}

// LSK-FR-KDVE: a file reached through a symbolic link — at the file or at a
// folder on the way — is refused and nothing is read through it.
#[cfg(unix)]
#[test]
fn a_file_reached_through_a_symbolic_link_is_refused() {
    let fixture = referencing_project();
    let root = fixture.app.state::<Bm25Indexer>().root().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.md"), "SECRET").unwrap();
    let skill = root.join(".claude/skills/release");
    std::os::unix::fs::symlink(outside.path().join("secret.md"), skill.join("linked.md")).unwrap();
    std::os::unix::fs::symlink(outside.path(), skill.join("linked-dir")).unwrap();
    for path in ["linked.md", "linked-dir/secret.md"] {
        assert_eq!(
            load(&fixture, only_release(), "release", Some(path)),
            Err(ToolRefusal::SkillFileNotFound),
            "{path}",
        );
    }
}

// LSK-FR-02: the schema declares `file` as an optional string, and
// `ecosystem` accepts `builtin`.
#[test]
fn the_schema_declares_file_and_the_builtin_ecosystem() {
    let schema = crate::tools::skill_load::parameters();
    assert_eq!(schema["properties"]["file"]["type"], "string");
    assert_eq!(schema["required"], serde_json::json!(["name"]));
    let ecosystems = schema["properties"]["ecosystem"]["enum"].as_array().unwrap();
    assert!(ecosystems.contains(&serde_json::json!("builtin")));
    let args: LoadSkillArgs =
        serde_json::from_value(serde_json::json!({ "name": "x", "file": null })).unwrap();
    assert_eq!(args.file, None);
    let args: LoadSkillArgs =
        serde_json::from_value(serde_json::json!({ "name": "x", "file": 7 })).unwrap();
    assert_eq!(args.file.as_deref(), Some("7"), "another JSON shape names no file");
}

static FILE_BUFFER: LogBuffer = LogBuffer::new();

// LSK-FR-19: the record says whether `file` was given and never names it.
#[test]
fn a_file_load_is_logged_by_size_and_flag_alone() {
    let fixture = referencing_project();
    FILE_BUFFER.clear();
    block_on(
        SkillLoadTool::with_buffer(fixture.handle(), &FILE_BUFFER)
            .with_filter(only_release())
            .call(LoadSkillArgs {
                name: "release".to_string(),
                ecosystem: None,
                file: Some("references/checklist.md".to_string()),
            }),
    )
    .unwrap();
    let records = FILE_BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 100)
        .unwrap()
        .records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].fields.get("file").unwrap(), &serde_json::json!(true));
    let text = serde_json::to_string(&records).unwrap();
    assert!(!text.contains("checklist") && !text.contains("release") && !text.contains("Tag the"));
}

// LSK-FR-07 / LSK-FR-10: a selected project `mermaid` and the built-in
// `mermaid` — the bare name is ambiguous and names both ecosystems, and
// `ecosystem` picks either copy.
#[test]
fn a_project_and_a_builtin_skill_of_one_name_are_told_apart_by_ecosystem() {
    let dir = TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "mermaid",
        "name: mermaid\ndescription: The project's own diagram notes.",
        "PROJECT MERMAID BODY",
    );
    let fixture = mounted(dir);
    let filter = SkillFilter::with_project_paths([".claude/skills/mermaid/SKILL.md"]);
    let call = |ecosystem: Option<&str>| {
        block_on(SkillLoadTool::new(fixture.handle(), filter.clone()).call(LoadSkillArgs {
            name: "mermaid".to_string(),
            ecosystem: ecosystem.map(str::to_string),
            file: None,
        }))
    };
    assert_eq!(
        call(None),
        Err(ToolRefusal::SkillAmbiguous(vec![Ecosystem::Claude, Ecosystem::Builtin])),
    );
    assert!(call(Some("claude")).unwrap().contains("PROJECT MERMAID BODY"));
    let builtin = call(Some("builtin")).unwrap();
    assert!(builtin.contains("# Mermaid diagrams") && !builtin.contains("PROJECT MERMAID BODY"));
}
