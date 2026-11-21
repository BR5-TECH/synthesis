use super::*;

fn project() -> (tempfile::TempDir, crate::fs::RootFs) {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn write(dir: &tempfile::TempDir, relative: &str, text: &str) {
    let path = dir.path().join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

// DVC-FR-BGVU: the configs are listed in the stated order, the folders sorted
// by name, and nothing deeper than one level is read.
#[test]
fn configs_are_listed_in_the_stated_order() {
    let (dir, root) = project();
    write(&dir, ".devcontainer/zeta/devcontainer.json", "{}");
    write(&dir, ".devcontainer/alpha/devcontainer.json", "{}");
    write(&dir, ".devcontainer/alpha/deeper/devcontainer.json", "{}");
    write(&dir, ".devcontainer/empty/other.json", "{}");
    write(&dir, ".devcontainer.json", "{}");
    write(&dir, ".devcontainer/devcontainer.json", "{}");
    assert_eq!(
        list_configs(&root),
        vec![
            ".devcontainer/devcontainer.json",
            ".devcontainer.json",
            ".devcontainer/alpha/devcontainer.json",
            ".devcontainer/zeta/devcontainer.json",
        ]
    );
}

// DVC-FR-BGVU: a project with no config lists none.
#[test]
fn a_project_with_no_config_lists_none() {
    let (_dir, root) = project();
    assert!(list_configs(&root).is_empty());
}

// DVC-FR-BGVU: a folder that is a link out of the project is not followed.
#[cfg(unix)]
#[test]
fn a_linked_folder_out_of_the_project_is_not_followed() {
    let (dir, root) = project();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("devcontainer.json"), "{}").unwrap();
    std::fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".devcontainer/linked")).unwrap();
    assert!(list_configs(&root).is_empty());
}

// DVC-FR-WXYK: an absolute path, a path that leaves the project, and a path
// that names no file are each refused with their own reason.
#[test]
fn a_config_path_is_validated() {
    let (dir, root) = project();
    write(&dir, ".devcontainer/devcontainer.json", "{}");
    std::fs::create_dir_all(dir.path().join("folder")).unwrap();
    assert!(validate_config_path(&root, ".devcontainer/devcontainer.json").is_ok());
    assert_eq!(validate_config_path(&root, "/etc/passwd"), Err(ConfigPathProblem::Absolute));
    assert_eq!(
        validate_config_path(&root, "../other/devcontainer.json"),
        Err(ConfigPathProblem::LeavesProject)
    );
    assert_eq!(validate_config_path(&root, "missing.json"), Err(ConfigPathProblem::NotAFile));
    assert_eq!(validate_config_path(&root, "folder"), Err(ConfigPathProblem::NotAFile));
}

// DVC-FR-WXYK: a link that takes the path out of the project leaves it.
#[cfg(unix)]
#[test]
fn a_linked_config_out_of_the_project_leaves_it() {
    let (dir, root) = project();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("devcontainer.json"), "{}").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    assert_eq!(
        validate_config_path(&root, "linked/devcontainer.json"),
        Err(ConfigPathProblem::LeavesProject)
    );
}

// DVC-FR-JVKA: JSON with comments and trailing commas parses; a broken file
// and a top level that is not an object do not.
#[test]
fn a_config_is_json_with_comments() {
    let parsed = parse_config("// a comment\n{ \"name\": \"x\", /* more */ }\n").unwrap();
    assert_eq!(parsed.get("name").and_then(Value::as_str), Some("x"));
    let broken = parse_config("{\n  \"name\": \n").unwrap_err();
    assert!(broken.line.is_some());
    let array = parse_config("[1, 2]").unwrap_err();
    assert_eq!(array.line, None);
}

// DVC-FR-SEXK: a key matches the known Feature with any version tag or none,
// and a longer name does not.
#[test]
fn a_feature_key_matches_with_any_version() {
    assert!(feature_matches(CLAUDE_CODE_FEATURE, CLAUDE_CODE_FEATURE));
    assert!(feature_matches(&format!("{CLAUDE_CODE_FEATURE}:1"), CLAUDE_CODE_FEATURE));
    assert!(feature_matches(&format!("{CLAUDE_CODE_FEATURE}:1.0.5"), CLAUDE_CODE_FEATURE));
    assert!(feature_matches(&format!("{CLAUDE_CODE_FEATURE}@sha256:ab"), CLAUDE_CODE_FEATURE));
    assert!(!feature_matches(&format!("{CLAUDE_CODE_FEATURE}-extra:1"), CLAUDE_CODE_FEATURE));
    assert!(!feature_matches("ghcr.io/other/claude-code:1", CLAUDE_CODE_FEATURE));
}

// DVC-FR-SEXK: Claude Code has a known Feature and Codex has none.
#[test]
fn only_claude_code_has_a_known_feature() {
    assert_eq!(known_feature("claude_code"), Some(CLAUDE_CODE_FEATURE));
    assert_eq!(known_feature("codex"), None);
    assert_eq!(known_feature("opencode"), None);
    let config =
        parse_config(&format!("{{\"features\": {{\"{CLAUDE_CODE_FEATURE}:1\": {{}}}}}}")).unwrap();
    assert!(holds_known_feature(&config, "claude_code"));
    assert!(!holds_known_feature(&config, "codex"));
}

// DVC-FR-ABQZ: the Feature and the Node Feature are added, and every comment
// and the rest of the file stay.
#[test]
fn adding_the_feature_keeps_comments_and_adds_node() {
    let text = "// The team's container.\n{\n  // The base image.\n  \"image\": \"debian\",\n  \"features\": {\n    \"ghcr.io/x/y:2\": {} // keep\n  }\n}\n";
    let changed = add_agent_feature(text, "claude_code").unwrap().unwrap();
    assert!(changed.contains("// The team's container."), "{changed}");
    assert!(changed.contains("// The base image."), "{changed}");
    assert!(changed.contains("// keep"), "{changed}");
    assert!(changed.contains("\"ghcr.io/x/y:2\""), "{changed}");
    let parsed = parse_config(&changed).unwrap();
    let keys = feature_keys(&parsed);
    assert!(keys.contains(&format!("{CLAUDE_CODE_FEATURE}:1")), "{keys:?}");
    assert!(keys.contains(&format!("{NODE_FEATURE}:1")), "{keys:?}");
    assert_eq!(parsed.get("image").and_then(Value::as_str), Some("debian"));
    let node = changed.find(NODE_FEATURE).unwrap();
    let claude = changed.find(CLAUDE_CODE_FEATURE).unwrap();
    assert!(node < claude, "the Node Feature comes first");
}

// DVC-FR-ABQZ: a config with no `features` gets the object, and a config that
// has a Node Feature gets no second one.
#[test]
fn adding_the_feature_creates_features_and_keeps_an_existing_node() {
    let changed = add_agent_feature("{ \"image\": \"debian\" }", "claude_code").unwrap().unwrap();
    let parsed = parse_config(&changed).unwrap();
    assert_eq!(feature_keys(&parsed).len(), 2);

    let with_node =
        format!("{{ \"features\": {{ \"{NODE_FEATURE}:1\": {{ \"version\": \"22\" }} }} }}");
    let changed = add_agent_feature(&with_node, "claude_code").unwrap().unwrap();
    let parsed = parse_config(&changed).unwrap();
    let keys = feature_keys(&parsed);
    assert_eq!(keys.iter().filter(|k| feature_matches(k, NODE_FEATURE)).count(), 1);
    assert!(changed.contains("\"version\": \"22\""));
}

// DVC-FR-JVKA: a config that holds the Feature is not changed; a vendor with
// no known Feature and an invalid config are refused.
#[test]
fn adding_the_feature_refuses_or_changes_nothing() {
    let holds = format!("{{ \"features\": {{ \"{CLAUDE_CODE_FEATURE}\": {{}} }} }}");
    assert_eq!(add_agent_feature(&holds, "claude_code"), Ok(None));
    assert_eq!(add_agent_feature("{}", "codex"), Err(AddFeatureError::NoKnownFeature));
    assert!(matches!(
        add_agent_feature("{ \"a\": ", "claude_code"),
        Err(AddFeatureError::Invalid(_))
    ));
    assert!(matches!(add_agent_feature("[]", "claude_code"), Err(AddFeatureError::Invalid(_))));
}

// DVC-FR-BCDZ: the starter config names the base image and both Features.
#[test]
fn the_starter_config_holds_the_image_and_both_features() {
    let parsed = parse_config(&starter_config("acme")).unwrap();
    assert_eq!(parsed.get("name").and_then(Value::as_str), Some("acme"));
    assert_eq!(parsed.get("image").and_then(Value::as_str), Some(STARTER_IMAGE));
    assert!(holds_known_feature(&parsed, "claude_code"));
    assert!(feature_keys(&parsed).iter().any(|k| feature_matches(k, NODE_FEATURE)));
}

// DVC-FR-DELL / DVC-FR-PFKJ: the hash changes with the config and with the
// Dockerfile it names, and not with an unrelated file.
#[test]
fn the_hash_follows_the_config_and_its_dockerfile() {
    let (dir, root) = project();
    let config = ".devcontainer/devcontainer.json";
    write(&dir, config, "{ \"build\": { \"dockerfile\": \"Dockerfile\" } }");
    write(&dir, ".devcontainer/Dockerfile", "FROM debian\n");
    let path = dir.path().join(config);
    let first = config_hash(&root, &path).unwrap();
    write(&dir, "README.md", "unrelated");
    assert_eq!(config_hash(&root, &path).unwrap(), first);
    write(&dir, ".devcontainer/Dockerfile", "FROM ubuntu\n");
    let second = config_hash(&root, &path).unwrap();
    assert_ne!(second, first);
    write(&dir, config, "{ \"build\": { \"dockerfile\": \"Dockerfile\" }, \"name\": \"x\" }");
    assert_ne!(config_hash(&root, &path).unwrap(), second);
}

// DVC-FR-DELL: the compose files a config names count too.
#[test]
fn the_hash_follows_the_compose_files() {
    let (dir, root) = project();
    let config = ".devcontainer/devcontainer.json";
    write(&dir, config, "{ \"dockerComposeFile\": [\"a.yml\", \"b.yml\"], \"service\": \"app\" }");
    write(&dir, ".devcontainer/a.yml", "a");
    write(&dir, ".devcontainer/b.yml", "b");
    let path = dir.path().join(config);
    let first = config_hash(&root, &path).unwrap();
    write(&dir, ".devcontainer/b.yml", "changed");
    assert_ne!(config_hash(&root, &path).unwrap(), first);
}

// DVC-FR-QGUT / DVC-FR-IFBK: the generated config keeps the user's keys, puts
// the worktree at its host path, and adds the launch mounts, read-only where
// asked.
#[test]
fn the_generated_config_sets_the_workspace_and_adds_mounts() {
    let config = parse_config(
        "{ \"image\": \"debian\", \"mounts\": [\"type=volume,source=cache,target=/cache\"] }",
    )
    .unwrap();
    let mounts = vec![
        LaunchMount {
            source: "/state".into(),
            target: "/home/agent/.claude".into(),
            read_only: false,
        },
        LaunchMount { source: "/repo/.git".into(), target: "/repo/.git".into(), read_only: true },
    ];
    let env = vec![("CLAUDE_CONFIG_DIR".to_string(), "/home/agent/.claude".to_string())];
    let generated = generated_config(&config, "/work/tree", &mounts, &env);
    assert_eq!(generated["image"], "debian");
    assert_eq!(generated["workspaceFolder"], "/work/tree");
    assert_eq!(generated["workspaceMount"], "type=bind,source=/work/tree,target=/work/tree");
    let all: Vec<&str> =
        generated["mounts"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(
        all,
        vec![
            "type=volume,source=cache,target=/cache",
            "type=bind,source=/state,target=/home/agent/.claude",
            "type=bind,source=/repo/.git,target=/repo/.git,readonly",
        ]
    );
    assert_eq!(generated["remoteEnv"]["CLAUDE_CONFIG_DIR"], "/home/agent/.claude");
}

// DVC-FR-QGUT: a compose config gets the worktree as one more mount, and no
// `workspaceMount`.
#[test]
fn a_compose_config_mounts_the_worktree_as_a_mount() {
    let config =
        parse_config("{ \"dockerComposeFile\": \"c.yml\", \"service\": \"app\" }").unwrap();
    let generated = generated_config(&config, "/work/tree", &[], &[]);
    assert!(generated.get("workspaceMount").is_none());
    assert_eq!(generated["workspaceFolder"], "/work/tree");
    assert_eq!(generated["mounts"][0], "type=bind,source=/work/tree,target=/work/tree");
    assert!(is_compose(&config));
}
