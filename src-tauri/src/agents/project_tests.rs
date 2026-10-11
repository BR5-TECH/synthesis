//! The tests of the Project scope
//! (`../../specifications/core/AGR-agent-registry.md` AGR-FR-12 through
//! AGR-FR-15, and `../../specifications/core/PSS-project-settings-storage.md`
//! PSS-FR-AGFL through PSS-FR-AGWT).

use super::*;
use crate::ai_api::AiApiRecord;
use crate::ai_shared::{ModelOption, ModelReasoning, ModelsOrigin};
use tempfile::TempDir;

/// A verified OpenRouter record offering `models`.
fn openrouter(models: Vec<ModelOption>) -> AiApiRecord {
    AiApiRecord {
        provider: "openrouter".into(),
        base_url: Some("https://openrouter.example/v1".into()),
        masked_hint: Some("abcd".into()),
        verified_at: Some("2026-01-01T00:00:00Z".into()),
        models_origin: ModelsOrigin::Probed,
        models,
        ..Default::default()
    }
}

fn store() -> GlobalSettingsStore {
    let store = GlobalSettingsStore::in_memory();
    let reasoning = ModelOption {
        reasoning: Some(ModelReasoning {
            supported_efforts: Some(vec!["high".into(), "low".into()]),
            ..Default::default()
        }),
        ..ModelOption::new("m", "M")
    };
    store
        .save_ai_api_registry(
            vec![openrouter(vec![reasoning, ModelOption::new("n", "N")])],
            Some("openrouter".into()),
        )
        .unwrap();
    store
}

fn worktree() -> (TempDir, RootFs) {
    let dir = TempDir::new().unwrap();
    let root = RootFs::for_root(dir.path());
    (dir, root)
}

fn draft(nickname: &str) -> AgentDraft {
    AgentDraft {
        nickname: nickname.into(),
        title: String::new(),
        model_id: "m".into(),
        instructions: String::new(),
        reasoning: None,
    }
}

fn create(
    store: &GlobalSettingsStore,
    root: Option<&RootFs>,
    scope: AgentScope,
    nickname: &str,
) -> Result<Agent, String> {
    create_agent_impl(store, "proj", root, scope, &draft(nickname))
}

/// The names in the agents folder, read through the instance (FSA-FR-19).
fn agent_files(root: &RootFs) -> Vec<String> {
    root.list_dir(".synthesis/agents")
        .map(|entries| entries.into_iter().map(|e| e.name).collect())
        .unwrap_or_default()
}

fn read(root: &RootFs, path: &std::path::Path) -> String {
    root.read_text(path).unwrap()
}

fn put(root: &RootFs, path: &std::path::Path, text: &str) {
    root.write_text_atomic(path, text).unwrap();
}

// AGR-FR-12, PSS-FR-AGFL: one file per agent, named by the agent's id, in the
// project's `.synthesis/agents/`.
#[test]
fn a_project_agent_is_one_file_named_by_its_id() {
    let store = store();
    let (_dir, root) = worktree();
    let arch = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    let sec = create(&store, Some(&root), AgentScope::Project, "sec").unwrap();

    assert_eq!(
        agent_files(&root),
        {
            let mut expected = vec![format!("{}.toml", arch.id), format!("{}.toml", sec.id)];
            expected.sort();
            expected
        },
    );
    // The agent is addressable at once, with no enrolment step, and nothing
    // was written into the user-global registry.
    assert!(store.load_agent_registry().unwrap().is_empty());
    assert_eq!(arch.created_at, "");
    assert_eq!(arch.updated_at, "");
}

// PSS-FR-AGFL: an update rewrites one file and a delete removes one file, so
// the other agents' files are untouched.
#[test]
fn an_update_and_a_delete_touch_one_file_only() {
    let store = store();
    let (dir, root) = worktree();
    let arch = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    let sec = create(&store, Some(&root), AgentScope::Project, "sec").unwrap();
    let sec_file = dir
        .path()
        .join(".synthesis/agents")
        .join(format!("{}.toml", sec.id));
    let before = read(&root, &sec_file);

    let mut changed = draft("arch");
    changed.instructions = "Argue about structure.".into();
    update_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &arch.id, &changed)
        .unwrap();
    assert_eq!(read(&root, &sec_file), before);

    let left = delete_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &arch.id)
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].agent.id, sec.id);
    assert_eq!(read(&root, &sec_file), before);
    assert_eq!(agent_files(&root), vec![format!("{}.toml", sec.id)]);
    // Idempotent.
    assert!(delete_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &arch.id).is_ok());
}

// PSS-FR-AGKY, AGR-FR-20: persona fields only; no id, no timestamp, no provider,
// no URL, no key. A key the file does not declare is dropped on the next write.
#[test]
fn a_project_agent_file_holds_persona_fields_only() {
    let store = store();
    let (dir, root) = worktree();
    let mut d = draft("arch");
    d.title = "Architect".into();
    d.instructions = "Argue about structure.".into();
    d.reasoning = Some(ReasoningChoice::Effort {
        effort: "high".into(),
    });
    let arch =
        create_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &d).unwrap();

    let path = dir
        .path()
        .join(".synthesis/agents")
        .join(format!("{}.toml", arch.id));
    let text = read(&root, &path);
    for wanted in ["nickname", "title", "modelId", "instructions", "reasoning"] {
        assert!(text.contains(wanted), "the file lacks {wanted}: {text}");
    }
    for forbidden in ["provider", "url", "key", "secret", "createdAt", "updatedAt", "openrouter"] {
        assert!(
            !text.to_lowercase().contains(&forbidden.to_lowercase()),
            "the file holds {forbidden}: {text}"
        );
    }
    assert!(!text.contains(&format!("\"{}\"", arch.id)), "the id is the file name");

    put(
        &root,
        &path,
        &format!("{text}\nprovider = \"openrouter\"\napiKey = \"sk-123\"\n"),
    );
    update_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &arch.id, &d).unwrap();
    let rewritten = read(&root, &path);
    assert!(!rewritten.contains("provider") && !rewritten.contains("sk-123"));
}

// PSS-FR-AGRD: an absent folder is an empty list; a file that cannot be parsed,
// has an invalid name, or carries an invalid nickname is skipped without
// stopping the others.
#[test]
fn an_unreadable_file_is_skipped_and_the_others_load() {
    let store = store();
    let (dir, root) = worktree();
    assert_eq!(
        list_project_agents_impl(&store, "proj", &root).unwrap(),
        vec![]
    );

    let arch = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    let folder = dir.path().join(".synthesis/agents");
    put(&root, &folder.join("broken.toml"), "nickname = [");
    put(&root, &folder.join("reserved.toml"), "nickname = \"all\"\nmodelId = \"m\"\n");
    put(&root, &folder.join("spaced.toml"), "nickname = \"two words\"\n");
    put(&root, &folder.join("bad name.toml"), "nickname = \"ok\"\n");
    put(&root, &folder.join("notes.txt"), "not an agent");

    let (roster, skipped) = roster_of(&store, "proj", &root).unwrap();
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].agent.id, arch.id);
    assert_eq!(skipped.len(), 4, "skipped: {skipped:?}");
}

// AGR-FR-13: the roster holds both scopes, ordered by nickname without regard to
// case, a Project agent before a Personal agent of the same nickname.
#[test]
fn the_roster_holds_both_scopes_in_nickname_order() {
    let store = store();
    let (_dir, root) = worktree();
    create(&store, Some(&root), AgentScope::Personal, "Zed").unwrap();
    create(&store, Some(&root), AgentScope::Personal, "bee").unwrap();
    create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    create(&store, Some(&root), AgentScope::Project, "Cy").unwrap();

    let roster = list_project_agents_impl(&store, "proj", &root).unwrap();
    let seen: Vec<(String, AgentScope)> = roster
        .into_iter()
        .map(|p| (p.agent.nickname, p.scope))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("arch".to_string(), AgentScope::Project),
            ("bee".to_string(), AgentScope::Personal),
            ("Cy".to_string(), AgentScope::Project),
            ("Zed".to_string(), AgentScope::Personal),
        ]
    );
}

// AGR-FR-05, AGR-FR-04: a nickname is unique inside one scope and the reserved
// handle is refused in both; across scopes the same nickname is allowed.
#[test]
fn a_nickname_is_unique_inside_a_scope_and_free_across_scopes() {
    let store = store();
    let (_dir, root) = worktree();
    create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    assert_eq!(
        create(&store, Some(&root), AgentScope::Project, "ARCH").unwrap_err(),
        ERR_NICKNAME_TAKEN,
    );
    assert!(create(&store, Some(&root), AgentScope::Personal, "arch").is_ok());
    assert_eq!(
        create(&store, Some(&root), AgentScope::Personal, "Arch").unwrap_err(),
        ERR_NICKNAME_TAKEN,
    );
    for scope in [AgentScope::Project, AgentScope::Personal] {
        assert_eq!(
            create(&store, Some(&root), scope, "All").unwrap_err(),
            ERR_NICKNAME_RESERVED,
        );
    }
}

// AGR-FR-15, AGR-FR-14: the Project agent wins. The Personal agent of the same
// nickname is shadowed in the project, stays defined, and works again where no
// Project agent has that nickname.
#[test]
fn a_project_agent_shadows_a_personal_agent_of_the_same_nickname() {
    let store = store();
    let (_dir, root) = worktree();
    let personal = create(&store, Some(&root), AgentScope::Personal, "arch").unwrap();
    let project = create(&store, Some(&root), AgentScope::Project, "ARCH").unwrap();

    let roster = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(roster.len(), 2);
    assert_eq!(roster[0].agent.id, project.id);
    assert_eq!(roster[0].availability, AgentAvailability::Ready);
    assert_eq!(roster[1].agent.id, personal.id);
    assert_eq!(roster[1].availability, AgentAvailability::Shadowed);

    let personal_list = list_personal_agents_impl(&store, "proj", Some(&root)).unwrap();
    assert_eq!(personal_list[0].availability, AgentAvailability::Shadowed);
    // No project open: nothing shadows it.
    let without = list_personal_agents_impl(&store, "proj", None).unwrap();
    assert_eq!(without[0].availability, AgentAvailability::Ready);
    // Another worktree with no Project agent of that nickname.
    let (_other_dir, other) = worktree();
    let elsewhere = list_project_agents_impl(&store, "proj", &other).unwrap();
    assert_eq!(elsewhere[0].availability, AgentAvailability::Ready);

    // Nothing was rewritten.
    assert_eq!(store.load_agent_registry().unwrap(), vec![personal]);
}

// AGR-FR-18: a nickname resolves against the Project agents first, whatever
// their availability, and then against the Personal agents that are not shadowed.
#[test]
fn resolution_prefers_the_project_agent_and_never_reaches_a_shadowed_one() {
    let store = store();
    let (_dir, root) = worktree();
    create(&store, Some(&root), AgentScope::Personal, "arch").unwrap();
    let personal_sec = create(&store, Some(&root), AgentScope::Personal, "sec").unwrap();
    let project = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();

    assert_eq!(
        resolve_project_agent(&store, "proj", &root, "ARCH").unwrap().id,
        project.id,
    );
    assert_eq!(
        resolve_project_agent(&store, "proj", &root, "sec").unwrap().id,
        personal_sec.id,
    );
    assert_eq!(
        resolve_project_agent(&store, "proj", &root, "nobody").unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );

    // The Project agent wins even when it cannot answer: the shadowed Personal
    // agent is not a fallback.
    let mut unserved = draft("arch");
    unserved.model_id = "n".into();
    update_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &project.id, &unserved)
        .unwrap();
    store
        .save_ai_api_registry(
            vec![openrouter(vec![ModelOption::new("m", "M")])],
            Some("openrouter".into()),
        )
        .unwrap();
    assert_eq!(
        resolve_project_agent(&store, "proj", &root, "arch").unwrap_err(),
        ERR_AGENT_UNAVAILABLE,
    );
}

// AGR-FR-WQBN: a Project agent whose model this machine's provider lacks stays
// defined as unavailable, and its committed file is not rewritten.
#[test]
fn a_project_agent_with_an_unoffered_model_stays_defined_and_unchanged() {
    let store = store();
    let (dir, root) = worktree();
    let mut d = draft("arch");
    d.model_id = "n".into();
    let arch =
        create_agent_impl(&store, "proj", Some(&root), AgentScope::Project, &d).unwrap();
    let path = dir
        .path()
        .join(".synthesis/agents")
        .join(format!("{}.toml", arch.id));
    let before = read(&root, &path);

    store
        .save_ai_api_registry(
            vec![openrouter(vec![ModelOption::new("m", "M")])],
            Some("openrouter".into()),
        )
        .unwrap();
    let roster = list_project_agents_impl(&store, "proj", &root).unwrap();
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].availability, AgentAvailability::ModelUnavailable);
    assert_eq!(roster[0].agent.model_id, "n");
    assert_eq!(read(&root, &path), before);
}

// AGR-FR-22: the Project scope needs an open project; the Personal scope does not.
#[test]
fn the_project_scope_refuses_without_a_project() {
    let store = store();
    assert_eq!(
        create(&store, None, AgentScope::Project, "arch").unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        update_agent_impl(&store, "", None, AgentScope::Project, "x", &draft("arch"))
            .unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        delete_agent_impl(&store, "", None, AgentScope::Project, "x").unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );
    assert!(create(&store, None, AgentScope::Personal, "arch").is_ok());
}

// AGR-FR-10: an id naming no agent of the scope is refused, and an id that is
// not a bare token never reaches a path.
#[test]
fn update_refuses_an_unknown_id_and_an_id_that_names_a_path() {
    let store = store();
    let (_dir, root) = worktree();
    let arch = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    for id in ["missing", "../escape", "a/b", ""] {
        assert_eq!(
            update_agent_impl(&store, "proj", Some(&root), AgentScope::Project, id, &draft("arch"))
                .unwrap_err(),
            ERR_AGENT_NOT_FOUND,
            "id {id:?}",
        );
    }
    // An id of one scope is not found in the other.
    assert_eq!(
        update_agent_impl(&store, "proj", Some(&root), AgentScope::Personal, &arch.id, &draft("x"))
            .unwrap_err(),
        ERR_AGENT_NOT_FOUND,
    );
    assert_eq!(agent_files(&root).len(), 1);
}

// AGR-FR-05: two Project agent files that hold the same nickname after a merge
// both stay listed, and an edit that keeps the nickname is not refused for it.
#[test]
fn two_merged_files_with_one_nickname_stay_editable() {
    let store = store();
    let (_dir, root) = worktree();
    let arch = create(&store, Some(&root), AgentScope::Project, "arch").unwrap();
    // The second file arrives by a merge, not through the command.
    put(
        &root,
        &std::path::Path::new(".synthesis/agents").join("zzz-merged.toml"),
        "nickname = \"arch\"\nmodelId = \"m\"\n",
    );
    assert_eq!(
        list_project_agents_impl(&store, "proj", &root).unwrap().len(),
        2
    );

    let mut changed = draft("arch");
    changed.instructions = "Argue about structure.".into();
    assert!(update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        &arch.id,
        &changed
    )
    .is_ok());
    // The merged file keeps its own nickname without a refusal.
    assert!(update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        "zzz-merged",
        &draft("ARCH")
    )
    .is_ok());
    // Renaming it away works, and renaming it back onto the other is refused.
    assert!(update_agent_impl(
        &store,
        "proj",
        Some(&root),
        AgentScope::Project,
        "zzz-merged",
        &draft("other")
    )
    .is_ok());
    assert_eq!(
        update_agent_impl(
            &store,
            "proj",
            Some(&root),
            AgentScope::Project,
            "zzz-merged",
            &draft("arch")
        )
        .unwrap_err(),
        ERR_NICKNAME_TAKEN,
    );
}
