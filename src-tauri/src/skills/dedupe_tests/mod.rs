//! Tests for the `.agents/skills` folder and for the deduplication of project
//! skills by name (`DSL-dynamic-skills-loading.md` DSL-FR-02, DSL-FR-17,
//! DSL-FR-QMRX, DSL-FR-TBWE, DSL-FR-KWQE).
//!
//! Every modification time is set explicitly. No test depends on the order in
//! which the filesystem stamped two writes.

use super::*;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

/// Write `<folder>/<dir>/SKILL.md` with the declared `name`, and set its
/// modification time to `secs` seconds after the epoch.
fn write_at(root: &Path, folder: &str, dir: &str, name: &str, secs: u64) -> PathBuf {
    let skill_dir = root.join(folder).join(dir);
    std::fs::create_dir_all(&skill_dir).unwrap();
    let file = skill_dir.join("SKILL.md");
    std::fs::write(
        &file,
        format!("---\nname: {name}\ndescription: the {folder} copy\n---\n\n# Body\n"),
    )
    .unwrap();
    set_time(&file, secs);
    file
}

fn set_time(file: &Path, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap();
}

fn descriptor(path: &str, ecosystem: Ecosystem, name: &str) -> SkillDescriptor {
    SkillDescriptor {
        path: path.into(),
        ecosystem,
        folder_name: name.into(),
        name: name.into(),
        description: "does it".into(),
        shadowed_paths: Vec::new(),
    }
}

fn kept<'a>(skills: &'a [SkillDescriptor], name: &str) -> Vec<&'a str> {
    skills.iter().filter(|s| s.name == name).map(|s| s.path.as_str()).collect()
}

// DSL-FR-02, DSL-FR-NWKE: a skill under `.agents/skills` is a project skill
// with the ecosystem `agents`.
#[test]
fn a_skill_in_the_agents_folder_is_a_project_skill_of_the_agents_ecosystem() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".agents/skills", "planner", "planner", 100);

    let (skills, excluded) = enumerate(root);
    assert!(excluded.is_empty(), "{excluded:?}");
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].path, ".agents/skills/planner/SKILL.md");
    assert_eq!(skills[0].ecosystem, Ecosystem::Agents);
    assert_eq!(skills[0].ecosystem.as_str(), "agents");
}

// DSL-FR-17, DSL-FR-KWQE: of two project copies of one name, the one with the
// newer `SKILL.md` is kept, whatever folder it is in, and it records the other.
#[test]
fn the_project_copy_with_the_newest_modification_time_is_kept() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".agents/skills", "review", "review", 100);
    write_at(root, ".opencode/skills", "review", "review", 300);
    write_at(root, ".claude/skills", "review", "review", 200);

    let (skills, _) = enumerate(root);
    assert_eq!(kept(&skills, "review"), vec![".opencode/skills/review/SKILL.md"]);
    let winner = skills.iter().find(|s| s.name == "review").unwrap();
    assert_eq!(
        winner.shadowed_paths,
        vec![".agents/skills/review/SKILL.md", ".claude/skills/review/SKILL.md"],
        "the dropped copies, in path order"
    );
}

// DSL-FR-17: on an equal modification time, the first folder in the order
// `.agents`, `.claude`, `.codex`, `.github`, `.opencode` wins.
#[test]
fn an_equal_modification_time_goes_to_the_folder_order() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".opencode/skills", "review", "review", 500);
    write_at(root, ".github/skills", "review", "review", 500);
    write_at(root, ".codex/skills", "review", "review", 500);
    write_at(root, ".claude/skills", "review", "review", 500);
    let (skills, _) = enumerate(root);
    assert_eq!(kept(&skills, "review"), vec![".claude/skills/review/SKILL.md"]);

    write_at(root, ".agents/skills", "review", "review", 500);
    let (skills, _) = enumerate(root);
    assert_eq!(kept(&skills, "review"), vec![".agents/skills/review/SKILL.md"]);
}

// DSL-FR-17: within one folder, two copies of one name with an equal time go to
// the lower path.
#[test]
fn an_equal_time_in_one_folder_goes_to_the_lower_path() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".claude/skills", "zulu", "review", 700);
    write_at(root, ".claude/skills", "alpha", "review", 700);

    let (skills, _) = enumerate(root);
    assert_eq!(kept(&skills, "review"), vec![".claude/skills/alpha/SKILL.md"]);
}

// DSL-FR-17: a copy whose time is unknown counts as older than a copy with one.
#[test]
fn a_copy_with_no_modification_time_loses_to_one_with_a_time() {
    let older = descriptor(".agents/skills/review/SKILL.md", Ecosystem::Agents, "review");
    let dated = descriptor(".opencode/skills/review/SKILL.md", Ecosystem::Opencode, "review");
    let skills = dedupe_project_skills(vec![
        (older, None),
        (dated, Some(UNIX_EPOCH + Duration::from_secs(1))),
    ]);
    assert_eq!(kept(&skills, "review"), vec![".opencode/skills/review/SKILL.md"]);
}

// DSL-FR-17: names are compared exactly, so `Review` and `review` are two
// skills.
#[test]
fn names_that_differ_only_in_case_are_two_skills() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".claude/skills", "review", "review", 100);
    write_at(root, ".codex/skills", "review", "Review", 200);

    let (skills, _) = enumerate(root);
    assert_eq!(skills.len(), 2);
    assert!(skills.iter().all(|s| s.shadowed_paths.is_empty()));
}

// DSL-FR-QMRX: synthesis skills are never merged — not with a project skill of
// the same name, and not with another synthesis skill of the same name.
#[test]
fn synthesis_skills_are_never_merged() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_at(root, ".synthesis/skills", "one", "review", 100);
    write_at(root, ".synthesis/skills", "two", "review", 200);
    write_at(root, ".claude/skills", "review", "review", 300);

    let (skills, _) = enumerate(root);
    assert_eq!(
        kept(&skills, "review"),
        vec![
            ".claude/skills/review/SKILL.md",
            ".synthesis/skills/one/SKILL.md",
            ".synthesis/skills/two/SKILL.md",
        ]
    );
    assert!(skills.iter().all(|s| s.shadowed_paths.is_empty()));
}

// DSL-FR-TBWE, DSL-FR-25: the time is read on every pass, so editing the older
// copy makes it the kept one, and the same tree enumerates the same way twice.
#[test]
fn editing_the_older_copy_makes_it_the_kept_one() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let claude = write_at(root, ".claude/skills", "review", "review", 100);
    write_at(root, ".codex/skills", "review", "review", 200);
    assert_eq!(kept(&enumerate(root).0, "review"), vec![".codex/skills/review/SKILL.md"]);
    assert_eq!(enumerate(root).0, enumerate(root).0, "deterministic for one tree");

    set_time(&claude, 900);
    let (skills, _) = enumerate(root);
    assert_eq!(kept(&skills, "review"), vec![".claude/skills/review/SKILL.md"]);
    assert_eq!(skills[0].shadowed_paths, vec![".codex/skills/review/SKILL.md"]);
}

// DSL-FR-TBWE, DSL-FR-VJZD: a stored path of a dropped copy admits the kept
// copy; a path of no copy of that name admits nothing.
#[test]
fn a_stored_path_of_a_dropped_copy_admits_the_kept_copy() {
    let mut winner = descriptor(".codex/skills/review/SKILL.md", Ecosystem::Codex, "review");
    winner.shadowed_paths = vec![".claude/skills/review/SKILL.md".into()];

    assert!(SkillFilter::with_project_paths([".claude/skills/review/SKILL.md"]).admits(&winner));
    assert!(SkillFilter::with_project_paths([".codex/skills/review/SKILL.md"]).admits(&winner));
    assert!(!SkillFilter::with_project_paths([".github/skills/review/SKILL.md"]).admits(&winner));
    assert!(!SkillFilter::empty().admits(&winner));
}

// DSL-FR-KWQE, DSL-FR-PEHR: `list_project_skills` carries `shadowedPaths` on
// the wire, empty for a synthesis skill, and the `agents` ecosystem value.
#[test]
fn a_project_skill_carries_its_shadowed_paths_on_the_wire() {
    let mut winner = descriptor(".agents/skills/review/SKILL.md", Ecosystem::Agents, "review");
    winner.shadowed_paths = vec![".claude/skills/review/SKILL.md".into()];
    let wire = ProjectSkill {
        path: winner.path.clone(),
        ecosystem: winner.ecosystem,
        name: winner.name.clone(),
        description: winner.description.clone(),
        shadowed_paths: winner.shadowed_paths.clone(),
    };
    let json = serde_json::to_value(&wire).unwrap();
    assert_eq!(json["ecosystem"], "agents");
    assert_eq!(json["shadowedPaths"], serde_json::json!([".claude/skills/review/SKILL.md"]));

    let descriptor_json = serde_json::to_value(&winner).unwrap();
    assert!(
        descriptor_json.get("shadowedPaths").is_none(),
        "a descriptor a tool returns carries no shadowed paths: {descriptor_json}"
    );
}
