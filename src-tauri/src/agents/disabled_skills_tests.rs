//! The tests of an agent's synthesis skill opt-outs
//! (`../../specifications/core/AGR-agent-registry.md` AGR-FR-HMVQ,
//! AGR-FR-WDZT, AGR-FR-CJPB, AGR-FR-LAQP,
//! `../../specifications/core/PSS-project-settings-storage.md` PSS-FR-AGKY,
//! and `../../specifications/core/GSS-global-settings-storage.md` GSS-FR-30).

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

fn draft(nickname: &str, disabled: &[&str]) -> AgentDraft {
    AgentDraft {
        nickname: nickname.into(),
        model_id: "m".into(),
        disabled_skills: disabled.iter().map(|s| s.to_string()).collect(),
        ..AgentDraft::default()
    }
}

fn file_of(root: &RootFs, id: &str) -> String {
    root.read_text(root.join(format!(".synthesis/agents/{id}.toml"))).unwrap()
}

const GLOSSARY: &str = ".synthesis/skills/glossary/SKILL.md";
const STYLE: &str = ".synthesis/skills/style/SKILL.md";

// AGR-FR-HMVQ / AGR-FR-CJPB / PSS-FR-AGKY: a Project agent stores its
// opt-outs normalised, under the `disabledSkills` key of its file, and reads
// them back as written. An update replaces them.
#[test]
fn a_project_agent_stores_its_opt_outs_normalised_in_its_file() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    let agent = create_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &draft("arch", &[" .synthesis/skills/glossary/SKILL.md ", "", GLOSSARY, STYLE]),
    )
    .unwrap();
    let expected = vec![GLOSSARY.to_string(), STYLE.to_string()];
    assert_eq!(agent.disabled_skills, expected);
    assert!(file_of(&root, &agent.id).contains("disabledSkills = ["), "the file key");
    assert!(agent.skills.is_empty(), "the opt-outs are not a project skill selection");

    let listed = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(listed[0].agent.disabled_skills, expected, "read back as written");

    let updated = update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &agent.id,
        &draft("arch", &[]),
    )
    .unwrap();
    assert!(updated.disabled_skills.is_empty());
    assert!(!file_of(&root, &agent.id).contains("disabledSkills"), "an empty list writes no key");
}

// AGR-FR-WDZT: a Project agent file and a Personal agent record without the
// key read as an empty list, so every synthesis skill is on.
#[test]
fn a_record_without_the_key_turns_nothing_off() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    root.write_text_atomic(
        root.join(".synthesis/agents/legacy.toml"),
        "nickname = \"legacy\"\nmodelId = \"m\"\nskills = [\".claude/skills/a/SKILL.md\"]\n",
    )
    .unwrap();
    let roster = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(roster[0].agent.nickname, "legacy");
    assert!(roster[0].agent.disabled_skills.is_empty());

    let personal =
        create_agent_impl(&store, "proj", None, AgentScope::Personal, &draft("mine", &[])).unwrap();
    assert!(personal.disabled_skills.is_empty());
    let stored = store.load_agent_registry().unwrap();
    let toml = toml::to_string(&stored[0]).unwrap();
    assert!(!toml.contains("disabled"), "an empty list writes no field");
    assert!(load_personal(&store).unwrap()[0].disabled_skills.is_empty());
}

// AGR-FR-HMVQ / AGR-FR-CJPB / GSS-FR-30: a Personal agent keeps its opt-outs
// through create, update, save, and load, while it still holds no project
// skill selection (AGR-FR-QKOV). A path from another project stays.
#[test]
fn a_personal_agent_keeps_its_opt_outs() {
    let store = store();
    let other_project = ".synthesis/skills/from-another-project/SKILL.md";
    let mut with_selection = draft("mine", &[GLOSSARY, other_project, GLOSSARY]);
    with_selection.skills = vec![".claude/skills/analyst/SKILL.md".into()];
    let agent =
        create_agent_impl(&store, "proj", None, AgentScope::Personal, &with_selection).unwrap();
    assert_eq!(agent.disabled_skills, vec![GLOSSARY.to_string(), other_project.to_string()]);
    assert!(agent.skills.is_empty());

    let loaded = load_personal(&store).unwrap();
    assert_eq!(loaded[0].disabled_skills, agent.disabled_skills, "saved and loaded");
    let listed = list_personal_agents_impl(&store, "proj", None).unwrap();
    assert_eq!(listed[0].agent.disabled_skills, agent.disabled_skills);

    let updated = update_agent_impl(
        &store,
        "proj",
        None,
        AgentScope::Personal,
        &agent.id,
        &draft("mine", &[STYLE, " "]),
    )
    .unwrap();
    assert_eq!(updated.disabled_skills, vec![STYLE.to_string()]);
    assert_eq!(load_personal(&store).unwrap()[0].disabled_skills, vec![STYLE.to_string()]);
}

// AGR-FR-HMVQ / AGT-FR-19: the frontend sends the opt-outs as
// `disabledSkills`, and the backend returns them under the same key. A wrong
// key would be dropped by `serde(default)` without an error.
#[test]
fn the_opt_outs_cross_the_wire_as_disabled_skills() {
    let draft: AgentDraft = serde_json::from_value(serde_json::json!({
        "nickname": "arch",
        "modelId": "m",
        "disabledSkills": [GLOSSARY],
    }))
    .unwrap();
    assert_eq!(draft.disabled_skills, vec![GLOSSARY.to_string()]);

    let agent = Agent { disabled_skills: vec![STYLE.into()], ..Agent::default() };
    let json = serde_json::to_value(&agent).unwrap();
    assert_eq!(json["disabledSkills"], serde_json::json!([STYLE]));
}

// AGR-FR-LAQP / AGR-FR-HMVQ: the resolved agent carries its opt-outs as they
// stand at dispatch, for a Project agent and for a Personal agent.
#[test]
fn resolving_an_agent_returns_its_current_opt_outs() {
    let store = store();
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    let project = create_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &draft("arch", &[STYLE]),
    )
    .unwrap();
    create_agent_impl(&store, "proj", None, AgentScope::Personal, &draft("mine", &[GLOSSARY]))
        .unwrap();

    let resolved = resolve_project_agent(&store, "proj", &root, "arch").unwrap();
    assert_eq!(resolved.disabled_skills, vec![STYLE.to_string()]);
    let resolved = resolve_project_agent(&store, "proj", &root, "mine").unwrap();
    assert_eq!(resolved.disabled_skills, vec![GLOSSARY.to_string()]);

    update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &project.id,
        &draft("arch", &[GLOSSARY]),
    )
    .unwrap();
    let resolved = resolve_project_agent(&store, "proj", &root, "arch").unwrap();
    assert_eq!(resolved.disabled_skills, vec![GLOSSARY.to_string()]);
}
