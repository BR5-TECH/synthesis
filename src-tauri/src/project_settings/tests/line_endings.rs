//! The project-public line-ending convention (PSS-FR-17).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// PSS-FR-17 — the project-public line-ending convention
// -----------------------------------------------------------------------

#[test]
fn an_unconfigured_project_reports_lf_and_a_change_round_trips_in_project_toml() {
    // PSS-FR-17: no key -> `lf`; set `crlf` and re-read from disk (the
    // "relaunch" — nothing is held in memory between) -> `crlf`, and the
    // value sits in the committed `project.toml`, not in `local.toml`.
    let dir = TempDir::new().unwrap();
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap().line_endings,
        LineEndings::Lf
    );

    save_project_config_to(
        &crate::fs::RootFs::for_root(dir.path()),
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap().line_endings,
        LineEndings::Crlf
    );

    let public = dir.path().join(".synthesis/project.toml");
    assert!(public.is_file(), "the convention lands in project.toml");
    assert!(std::fs::read_to_string(&public).unwrap().contains("crlf"));
    assert!(
        !dir.path().join(".synthesis/local.toml").exists(),
        "and not in the gitignored per-machine store"
    );
}

#[test]
fn project_toml_is_not_gitignored_the_way_local_toml_is() {
    // PSS-FR-02 vs PSS-FR-03: `project.toml` is committed. Writing it must
    // not drag in the ignore file that keeps `local.toml` out of the repo —
    // that ignore rule is `local.toml`-specific and lives in `.gitignore`
    // only because the local store put it there.
    let dir = TempDir::new().unwrap();
    save_project_config_to(&crate::fs::RootFs::for_root(dir.path()), ProjectConfig::default()).unwrap();
    assert!(
        !dir.path().join(".synthesis/.gitignore").exists(),
        "a public write must not create the local store's ignore file"
    );
}

#[test]
fn saving_the_convention_preserves_every_other_project_public_section() {
    // PSS-FR-17: a project configured with `crlf` and an MCP connection —
    // changing the convention must carry the connection through, and a write
    // by another section must carry the convention through.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"crlf\"\n\n[[mcpServers]]\nname = \"one\"\n\n[[mcpServers]]\nname = \"two\"\n\n[plugins]\nenabled = [\"git-pr\"]\n",
    )
    .unwrap();

    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap().line_endings,
        LineEndings::Crlf,
        "an unrelated section's presence does not disturb the convention"
    );

    save_project_config_to(
        &crate::fs::RootFs::for_root(dir.path()),
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();

    let table = load_public(&crate::fs::RootFs::for_root(dir.path())).unwrap();
    assert_eq!(
        table
            .get("mcpServers")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2),
        "both MCP connections survive the write"
    );
    assert!(
        table.get("plugins").is_some(),
        "and so does every other section"
    );
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap().line_endings,
        LineEndings::Lf,
        "while this module's own key did change"
    );
}

#[test]
fn a_malformed_project_config_is_a_typed_error_unlike_the_local_store() {
    // PSS-FR-10: project-public damage is surfaced, where project-local
    // damage is repaired silently.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = [valid").unwrap();
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap_err(),
        ERR_MALFORMED_PROJECT_CONFIG
    );
    // PST-FR-23: a save must not fail over it, though — the write path falls
    // back to the default rather than costing the user their edit.
    assert_eq!(line_endings_for(&crate::fs::RootFs::for_root(dir.path())), LineEndings::Lf);
}

#[test]
fn an_unknown_line_ending_value_falls_back_to_the_default() {
    // A `project.toml` written by a newer build must not break the status
    // bar: the file parses, so it is not damaged — only this key is unknown.
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"cr\"\n",
    )
    .unwrap();
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path())).unwrap().line_endings,
        LineEndings::Lf
    );
}

#[test]
fn project_config_serialises_camel_case_with_lowercase_conventions() {
    let json = serde_json::to_value(ProjectConfig {
        line_endings: LineEndings::Crlf,
        draft_template: None,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "lineEndings": "crlf",
            "draftTemplate": null,
            // PSS-FR-ZVSD: the graduation limit rides in the same payload, and
            // a payload that names none says nothing about it.
            "graduationConcurrencyLimit": null,
            // PSS-FR-ZLCF: the three shared bounds ride there too, each
            // reporting the unset state the contract spells `null`.
            "executionTimeoutMs": null,
            "providerCallDeadlineMs": null,
            "retryBudget": null,
        })
    );
    // And it deserialises from the frontend shape, and from an empty object.
    let parsed: ProjectConfig = serde_json::from_str(r#"{"lineEndings":"lf"}"#).unwrap();
    assert_eq!(parsed.line_endings, LineEndings::Lf);
    // PSS-FR-21: a payload that says nothing about the template reads as
    // `None`, which `save_project_config_to` treats as "leave it alone".
    assert_eq!(parsed.draft_template, None);
    // PSS-FR-ZLCF: an **absent** shared bound says nothing about the stored
    // one, and a `null` clears it. The two must not collapse into one value.
    assert_eq!(parsed.execution_timeout_ms, None, "absent says nothing");
    let cleared: ProjectConfig =
        serde_json::from_str(r#"{"executionTimeoutMs":null}"#).unwrap();
    assert_eq!(cleared.execution_timeout_ms, Some(None), "null clears it");
    let empty: ProjectConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, ProjectConfig::default());
    let with_template: ProjectConfig =
        serde_json::from_str(r##"{"draftTemplate":"# Context\n"}"##).unwrap();
    assert_eq!(with_template.draft_template.as_deref(), Some("# Context\n"));
}
