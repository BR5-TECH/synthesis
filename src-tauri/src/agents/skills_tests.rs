//! The tests of a Project agent's project skill selection
//! (`../../specifications/core/AGR-agent-registry.md` AGR-FR-IGIL through
//! AGR-FR-LAQP, and `../../specifications/core/PSS-project-settings-storage.md`
//! PSS-FR-AGKY).

use super::*;
use crate::ai_api::AiApiRecord;
use crate::ai_shared::{ModelOption, ModelsOrigin};
use tempfile::TempDir;

fn store() -> GlobalSettingsStore {
    let store = GlobalSettingsStore::in_memory();
    store
        .save_ai_api_registry(
            vec![AiApiRecord {
                provider: "openrouter".into(),
                base_url: Some("https://openrouter.example/v1".into()),
                masked_hint: Some("abcd".into()),
                verified_at: Some("2026-01-01T00:00:00Z".into()),
                models_origin: ModelsOrigin::Probed,
                models: vec![ModelOption::new("m", "M")],
                ..Default::default()
            }],
            Some("openrouter".into()),
        )
        .unwrap();
    store
}

fn draft(nickname: &str, skills: &[&str]) -> AgentDraft {
    AgentDraft {
        nickname: nickname.into(),
        model_id: "m".into(),
        skills: skills.iter().map(|s| s.to_string()).collect(),
        ..AgentDraft::default()
    }
}

fn file_of(root: &RootFs, id: &str) -> String {
    root.read_text(root.join(format!(".synthesis/agents/{id}.toml"))).unwrap()
}

// AGR-FR-IGIL / AGR-FR-GBJL / PSS-FR-AGKY: the selection is stored by path,
// trimmed, without blanks or later duplicates, in the order given, unchecked
// against any registry, under the `skills` key of the agent's file.
#[test]
fn a_project_agent_stores_its_selection_normalised_in_its_file() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    let agent = create_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &draft(
            "arch",
            &[
                " .codex/skills/review/SKILL.md ",
                "",
                ".claude/skills/analyst/SKILL.md",
                ".codex/skills/review/SKILL.md",
                "   ",
                ".opencode/skills/not-in-this-worktree/SKILL.md",
            ],
        ),
    )
    .unwrap();
    let expected = vec![
        ".codex/skills/review/SKILL.md".to_string(),
        ".claude/skills/analyst/SKILL.md".to_string(),
        ".opencode/skills/not-in-this-worktree/SKILL.md".to_string(),
    ];
    assert_eq!(agent.skills, expected);
    assert!(file_of(&root, &agent.id).contains("skills = ["), "PSS-FR-AGKY: the `skills` key");

    let listed = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(listed[0].agent.skills, expected, "read back as written");

    // An update replaces the selection, under the same normalisation.
    let updated = update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &agent.id,
        &draft("arch", &[".claude/skills/analyst/SKILL.md"]),
    )
    .unwrap();
    assert_eq!(updated.skills, vec![".claude/skills/analyst/SKILL.md".to_string()]);
}

// AGR-FR-XWVP: a file with no `skills` key reads as an empty selection, and
// an agent that selects nothing writes no key.
#[test]
fn a_file_without_the_key_is_an_empty_selection_and_none_is_written_for_it() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    root.write_text_atomic(
        root.join(".synthesis/agents/legacy.toml"),
        "nickname = \"legacy\"\nmodelId = \"m\"\n",
    )
    .unwrap();
    let roster = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(roster[0].agent.nickname, "legacy");
    assert!(roster[0].agent.skills.is_empty());

    let plain =
        create_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &draft("plain", &[]))
            .unwrap();
    assert!(!file_of(&root, &plain.id).contains("skills"));
}

// AGR-FR-QKOV: a Personal agent has no selection — a draft carrying one is
// stored without it and not refused, and the registry holds no such field.
#[test]
fn a_personal_agent_never_holds_a_selection() {
    let store = store();
    let agent = create_agent_impl(
        &store,
        "proj",
        None,
        AgentScope::Personal,
        &draft("mine", &[".claude/skills/analyst/SKILL.md"]),
    )
    .unwrap();
    assert!(agent.skills.is_empty());
    let updated = update_agent_impl(
        &store,
        "proj",
        None,
        AgentScope::Personal,
        &agent.id,
        &draft("mine", &[".claude/skills/analyst/SKILL.md"]),
    )
    .unwrap();
    assert!(updated.skills.is_empty());
    let stored = store.load_agent_registry().unwrap();
    assert!(stored.iter().all(|a| a.skills.is_empty()), "nothing written");
    let toml = toml::to_string(&stored[0]).unwrap();
    assert!(!toml.contains("skills"), "no `skills` field in the Personal registry");

    // A record that somehow carries one still reads back empty.
    let mut hand_edited = stored;
    hand_edited[0].skills = vec!["x".into()];
    store.save_agent_registry(hand_edited).unwrap();
    assert!(load_personal(&store).unwrap()[0].skills.is_empty());
    let listed = list_personal_agents_impl(&store, "proj", None).unwrap();
    assert!(listed[0].agent.skills.is_empty());
}

// AGR-FR-LAQP: the resolved agent carries its stored selection, as it stands
// when the turn is dispatched.
#[test]
fn resolving_a_project_agent_returns_its_current_selection() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    let agent = create_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &draft("arch", &[".claude/skills/a/SKILL.md"]),
    )
    .unwrap();
    let resolved = resolve_project_agent(&store, "proj", &root, "ARCH").unwrap();
    assert_eq!(resolved.skills, vec![".claude/skills/a/SKILL.md".to_string()]);

    update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &agent.id,
        &draft("arch", &[".claude/skills/b/SKILL.md"]),
    )
    .unwrap();
    let resolved = resolve_project_agent(&store, "proj", &root, "arch").unwrap();
    assert_eq!(resolved.skills, vec![".claude/skills/b/SKILL.md".to_string()]);
}

// AGR-FR-GBJL: the normalisation alone.
#[test]
fn normalise_skills_trims_drops_blanks_and_later_duplicates_in_order() {
    let given: Vec<String> =
        [" b ", "a", "", "b", "  ", "c"].into_iter().map(String::from).collect();
    assert_eq!(normalise_skills(&given), vec!["b", "a", "c"]);
    assert!(normalise_skills(&[]).is_empty());
}
