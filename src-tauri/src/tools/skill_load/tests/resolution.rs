//! LSK-FR-03 … LSK-FR-10: which file is read, and how a name resolves.

use super::*;

// ---------------------------------------------------------------------------
// LSK-FR-03 — one file, and only one the registry named (LSK-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn the_only_file_read_is_one_the_registry_named() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "analyst",
        "name: analyst\ndescription: Author requirement documents.",
        "# Analyst body",
    );
    // A Markdown file that exists in the project and is not a skill, and a
    // credential-shaped file beside it. Neither is reachable by any argument.
    std::fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
    std::fs::write(dir.path().join(".env"), "API_KEY=hunter2\n").unwrap();
    // A decoy *inside the skill's own folder*: the file an implementation that
    // walked the folder, or read a sibling alongside SKILL.md, would pick up.
    std::fs::write(
        dir.path().join(".claude/skills/analyst/NOTES.md"),
        "DECOY SIBLING CONTENT\n",
    )
    .unwrap();
    let fixture = mounted(dir);

    let loaded = call(&fixture, "analyst", None).unwrap();
    assert!(loaded.contains("Analyst body"));
    assert!(
        !loaded.contains("DECOY SIBLING CONTENT"),
        "LSK-FR-03: the one file read is the SKILL.md, not the folder around it",
    );

    // LSK-FR-03: a model supplies a *name*, which selects a descriptor or
    // selects nothing. There is no argument through which a path reaches the
    // filesystem, so each of these is simply a name nothing goes by.
    for probe in [
        "../../etc/passwd",
        "README.md",
        "README",
        ".env",
        "/etc/passwd",
        "../analyst",
        ".claude/skills/analyst/SKILL.md",
    ] {
        assert_eq!(
            call(&fixture, probe, None).unwrap_err(),
            ToolRefusal::SkillNotFound,
            "LSK-FR-03: {probe:?} names no skill and reads nothing",
        );
    }
}

#[test]
fn the_module_enumerates_no_folder_of_its_own() {
    // LSK-FR-03's other half. The observational test above cannot see a walk
    // that happens to find nothing, and this tool — unlike the other two —
    // legitimately calls `read_text`, so it cannot borrow their "reads no
    // file" scan. What it must not do is discover paths for itself.
    let source = include_str!("../../skill_load.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for forbidden in [
        "read_dir",
        "list_dir",
        "SKILL_FOLDERS",
        "SKILL_FILE",
        "enumerate(",
        "WalkDir",
        "glob",
    ] {
        assert!(
            !code.contains(forbidden),
            "LSK-FR-03: skill_load must not discover paths itself, found {forbidden:?}",
        );
    }
    // And exactly one read call, so a second file cannot be picked up beside
    // the one the registry named.
    assert_eq!(
        code.matches("read_text").count(),
        1,
        "LSK-FR-03: exactly one read, of the descriptor's own path",
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-04 — eligibility is the skills module's alone (LSK-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn a_skill_the_registry_excludes_cannot_be_loaded() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    // Opts out of model invocation (DSL-FR-08) — the exclusion that matters
    // most here: opting out means a model may not USE the skill, not merely
    // that it may not find it.
    write_skill(
        root,
        ".claude/skills",
        "quiet",
        "name: quiet\ndescription: A skill that opted out.\ndisable-model-invocation: true",
        "# Quiet\n\nProcedure nobody may load.",
    );
    // Declares no description (DSL-FR-09).
    write_skill(root, ".claude/skills", "nodesc", "name: nodesc", "# No description");
    // Frontmatter that never closes, so it does not parse (DSL-FR-07).
    let broken = root.join(".claude/skills/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("SKILL.md"),
        "---\nname: broken\ndescription: Never closed.\n\n# Broken\n",
    )
    .unwrap();
    // One eligible skill, so the fixture proves exclusion rather than emptiness.
    write_skill(
        root,
        ".claude/skills",
        "good",
        "name: good\ndescription: An eligible skill.",
        "# Good",
    );
    // Reached through a symlink (DSL-FR-05).
    #[cfg(unix)]
    {
        let real = root.join("outside");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(
            real.join("SKILL.md"),
            "---\nname: linked\ndescription: Reached only through a link.\n---\n# Linked\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&real, root.join(".claude/skills/linked")).unwrap();
    }
    // Over the indexing ceiling (DSL-FR-23), which the registry refuses to
    // admit — so the one file this tool would happily have returned whole
    // (LSK-FR-14) is unreachable because there is no descriptor for it.
    let huge = root.join(".claude/skills/huge");
    std::fs::create_dir_all(&huge).unwrap();
    let filler = "x".repeat(crate::bm25_index::MAX_FILE_BYTES as usize + 1);
    std::fs::write(
        huge.join("SKILL.md"),
        format!("---\nname: huge\ndescription: Over the ceiling.\n---\n{filler}"),
    )
    .unwrap();
    let fixture = mounted(dir);

    let excluded_names: Vec<&str> = if cfg!(unix) {
        vec!["quiet", "nodesc", "broken", "linked", "huge"]
    } else {
        vec!["quiet", "nodesc", "broken", "huge"]
    };
    for excluded in excluded_names {
        let refusal = call(&fixture, excluded, None).unwrap_err();
        assert_eq!(
            refusal,
            ToolRefusal::SkillNotFound,
            "LSK-FR-04: {excluded:?} is excluded by the registry and unloadable here",
        );
        // LSK-FR-09: and the refusal never reveals that the file exists but is
        // withheld — it reads exactly as a name that matches nothing.
        assert_eq!(refusal.to_string(), SKILL_NOT_FOUND);
    }
    assert!(call(&fixture, "good", None).unwrap().contains("# Good"));
}

// ---------------------------------------------------------------------------
// LSK-FR-05 — name matching is forgiving (LSK-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn a_name_is_matched_trimmed_and_without_regard_to_case() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "analyst",
        "name: Analyst\ndescription: Author requirement documents.",
        "# The analyst body",
    );
    let fixture = mounted(dir);

    for spelling in ["Analyst", "analyst", "  Analyst  ", "ANALYST", "aNaLySt"] {
        assert!(
            call(&fixture, spelling, None)
                .unwrap_or_else(|e| panic!("LSK-FR-05: {spelling:?} must resolve, got {e:?}"))
                .contains("The analyst body"),
        );
    }
    // Not so forgiving that a different name resolves.
    assert_eq!(
        call(&fixture, "analysts", None).unwrap_err(),
        ToolRefusal::SkillNotFound,
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-06 — a blank name refuses (LSK-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_blank_name_is_a_retryable_invalid_argument() {
    let fixture = demo_project();

    for blank in ["", "   ", "\t\n "] {
        let refusal = call(&fixture, blank, None).unwrap_err();
        assert_eq!(refusal, ToolRefusal::InvalidArguments(BLANK_NAME));
        let error = refusal.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs, "LSK-FR-06");
        assert_eq!(error.retryable(), Some(true), "LSK-FR-06");
        assert_eq!(error.message(), BLANK_NAME);
    }
}

// ---------------------------------------------------------------------------
// LSK-FR-07 — the ecosystem narrows (LSK-FR-07, LSK-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn the_ecosystem_argument_narrows_and_an_unknown_value_narrows_to_nothing() {
    let fixture = collision_project();

    // Each copy is reachable by naming its ecosystem, and each returns its own
    // body rather than a shared one.
    assert!(call(&fixture, "review", Some("claude"))
        .unwrap()
        .contains("The claude copy's instructions"));
    assert!(call(&fixture, "review", Some("codex"))
        .unwrap()
        .contains("The codex copy's instructions"));
    assert!(call(&fixture, "review", Some("opencode"))
        .unwrap()
        .contains("The opencode copy's instructions"));

    // `.github` holds no `review`, so narrowing to it leaves nothing.
    assert_eq!(
        call(&fixture, "review", Some("github")).unwrap_err(),
        ToolRefusal::SkillNotFound,
        "LSK-FR-09: a name held by no skill in the ecosystem asked for",
    );

    // LSK-FR-07: a value naming none of the four narrows to nothing rather
    // than being refused as malformed — the same answer, not a second failure
    // mode for a model to learn.
    for nonsense in ["nonsense", "CLAUDE-2", ""] {
        assert_eq!(
            call(&fixture, "review", Some(nonsense)).unwrap_err(),
            ToolRefusal::SkillNotFound,
            "LSK-FR-07: {nonsense:?} names none of the four",
        );
    }
    // Spelled as the model was told, whatever the casing.
    assert!(call(&fixture, "review", Some("Claude"))
        .unwrap()
        .contains("claude copy"));
}

#[test]
fn an_ecosystem_of_the_wrong_json_shape_does_not_lose_the_name_beside_it() {
    // TLC-FR-07: a decode failure here would throw away a perfectly good
    // `name` over the shape of an optional parameter.
    let args: LoadSkillArgs =
        serde_json::from_value(serde_json::json!({ "name": "review", "ecosystem": 7 }))
            .expect("the whole call must still decode");
    assert_eq!(args.name, "review");
    assert_eq!(
        args.ecosystem.as_deref(),
        Some(""),
        "a value of the wrong shape names none of the four",
    );

    let null: LoadSkillArgs =
        serde_json::from_value(serde_json::json!({ "name": "review", "ecosystem": null })).unwrap();
    assert_eq!(null.ecosystem, None, "an explicit null means any ecosystem");

    assert_eq!(parse_ecosystem("claude"), Some(Ecosystem::Claude));
    assert_eq!(parse_ecosystem(" GITHUB "), Some(Ecosystem::Github));
    assert_eq!(parse_ecosystem("elsewhere"), None);
    assert_eq!(parse_ecosystem(""), None);
}

// ---------------------------------------------------------------------------
// LSK-FR-08 — the ordinary outcome (LSK-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn a_unique_name_with_no_ecosystem_returns_that_skills_instructions() {
    let fixture = collision_project();
    let body = call(&fixture, "analyst", None).unwrap();
    // Equality, not `contains`: the whole body, and nothing from the three
    // `review` skills sitting beside it in the same project.
    assert_eq!(
        body,
        "\n# Analyst\n\nFirst line of the analyst body.\n\nSecond paragraph.\n",
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-09 — an unknown name (LSK-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_name_refuses_as_a_retryable_not_found_naming_list_skills() {
    let fixture = demo_project();
    let refusal = call(&fixture, "no such skill anywhere", None).unwrap_err();

    assert_eq!(refusal, ToolRefusal::SkillNotFound);
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound, "LSK-FR-09");
    assert_eq!(
        error.retryable(),
        Some(true),
        "LSK-FR-09: a different name reaches a skill this project does have",
    );
    assert!(
        error.message().contains("list_skills"),
        "LSK-FR-09: the refusal names the way to learn what is available: {:?}",
        error.message(),
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-10 — an ambiguous name (LSK-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn an_ambiguous_name_refuses_naming_the_ecosystems_and_never_picks_one() {
    let fixture = collision_project();
    let refusal = call(&fixture, "review", None).unwrap_err();

    assert_eq!(
        refusal,
        ToolRefusal::SkillAmbiguous(vec![
            Ecosystem::Claude,
            Ecosystem::Codex,
            Ecosystem::Opencode
        ]),
        "LSK-FR-10: the tool never picks one",
    );
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::InvalidArgs, "LSK-FR-10");
    assert_eq!(error.retryable(), Some(true), "LSK-FR-10");

    // Asserted as an exact string rather than as a set of `contains` checks.
    // The message is assembled from compiled-in words, so `!contains("body")`
    // could never fire; equality is what actually fails if an implementation
    // starts appending paths, names, or descriptions to it.
    assert_eq!(
        error.message(),
        "More than one skill goes by that name. Call again with `ecosystem` \
         set to one of: claude, codex, opencode.",
        "LSK-FR-10: the ecosystems that hold the name, and nothing else",
    );
    assert!(!error.message().contains("github"), "and only those that do");
}

#[test]
fn a_name_two_skills_in_one_ecosystem_share_is_refused_as_unresolvable() {
    // LSK-FR-10 assumes a collision spans ecosystems, and DSL does not
    // guarantee that: two skill folders in one family may each declare the
    // same name. Narrowing by `ecosystem` then re-selects the same candidates,
    // so advising it would send the model back for a call that refuses
    // identically while `retryable` promised otherwise (TLC-FR-11).
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "alpha",
        "name: review\ndescription: Review a specification.",
        "# Alpha",
    );
    write_skill(
        dir.path(),
        ".claude/skills",
        "beta",
        "name: review\ndescription: Review a diff.",
        "# Beta",
    );
    let fixture = mounted(dir);

    for narrowing in [None, Some("claude")] {
        let refusal = call(&fixture, "review", narrowing).unwrap_err();
        assert_eq!(
            refusal,
            ToolRefusal::SkillNameNotUnique,
            "LSK-FR-10: {narrowing:?} cannot separate two skills in one ecosystem",
        );
        let error = refusal.to_execution_error();
        assert_eq!(
            error.retryable(),
            Some(false),
            "TLC-FR-11: no argument the model could compose changes this answer",
        );
        assert_eq!(error.kind(), ToolErrorKind::Other);
        // And it must not send the model back round the loop it just left.
        assert!(
            !error.message().contains("ecosystem"),
            "the refusal cannot repeat advice that does not work: {:?}",
            error.message(),
        );
        // Neither body reaches the model as a silent pick.
        assert!(!error.message().contains("Alpha") && !error.message().contains("Beta"));
    }

    // The cross-ecosystem case still gets the advice that *does* work, so the
    // two refusals are told apart by whether narrowing could help.
    assert!(matches!(
        call(&collision_project(), "review", None).unwrap_err(),
        ToolRefusal::SkillAmbiguous(_),
    ));
}

