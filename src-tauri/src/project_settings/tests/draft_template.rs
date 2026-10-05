//! The draft template (PSS-FR-21).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// The draft template (PSS-FR-21)
// -----------------------------------------------------------------------

/// PSS-FR-21: unset is not empty text, the template is project-**public**,
/// and it survives a re-read.
#[test]
fn an_unconfigured_draft_template_reads_as_unset_and_a_configured_one_round_trips() {
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());

    assert_eq!(
        load_project_config_from(&root).unwrap().draft_template,
        None,
        "a project.toml recording no template reports it unset, not empty"
    );

    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: Some("# Context\n\n## Decision\n".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();

    // In `project.toml` (committed), never in `local.toml` (per-machine).
    let public = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(public.contains("draftTemplate"), "{public}");
    assert!(!dir.path().join(".synthesis/local.toml").exists());

    // Byte-for-byte on the way back, which is what a relaunch into the same
    // worktree reads.
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(dir.path()))
            .unwrap()
            .draft_template
            .as_deref(),
        Some("# Context\n\n## Decision\n")
    );
}

/// PSS-FR-21, PSS-FR-17: an empty template clears the key rather than storing `""`,
/// and the whole-store write carries every other section through (PSS-FR-17).
#[test]
fn an_empty_draft_template_clears_the_key_and_leaves_every_other_section_standing() {
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    // An unrelated project-public section, standing in for the MCP
    // connections of PSS-FR-09.
    let mut seed = toml::Table::new();
    seed.insert("mcp".to_string(), toml::Value::String("one-connection".into()));
    save_public(&root, &seed).unwrap();

    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: Some("# Context".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: Some(String::new()),
                        ..Default::default()
        },
    )
    .unwrap();

    let table = load_public(&root).unwrap();
    assert!(
        !table.contains_key(DRAFT_TEMPLATE_KEY),
        "an empty template removes the key rather than recording an empty value"
    );
    assert_eq!(
        load_project_config_from(&root).unwrap().draft_template,
        None
    );
    assert_eq!(table.get("mcp").and_then(|v| v.as_str()), Some("one-connection"));
    assert_eq!(
        load_project_config_from(&root).unwrap().line_endings,
        LineEndings::Crlf
    );

    // And changing the convention afterwards rewrites nothing about the
    // template: it is still unset.
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();
    let after = load_project_config_from(&root).unwrap();
    assert_eq!(after.line_endings, LineEndings::Lf);
    assert_eq!(after.draft_template, None);
}

/// PSS-FR-17: a write that says nothing about the template leaves a
/// configured one exactly as it stands — which is what the status bar's
/// line-ending control does on every selection.
#[test]
fn a_write_carrying_no_template_leaves_a_configured_one_untouched() {
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: Some("# Context".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();
    save_project_config_to(
        &root,
        ProjectConfig {
            line_endings: LineEndings::Crlf,
            draft_template: None,
                        ..Default::default()
        },
    )
    .unwrap();
    let after = load_project_config_from(&root).unwrap();
    assert_eq!(after.draft_template.as_deref(), Some("# Context"));
    assert_eq!(after.line_endings, LineEndings::Crlf);
}

/// PSS-FR-21, PSS-FR-16, PSS-FR-10: the store resolves against the active worktree's own copy,
/// and a malformed one is the typed error of PSS-FR-10 rather than an
/// absent template.
#[test]
fn the_template_resolves_per_worktree_and_a_malformed_store_is_a_typed_error() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    save_project_config_to(
        &crate::fs::RootFs::for_root(a.path()),
        ProjectConfig {
            line_endings: LineEndings::Lf,
            draft_template: Some("# A".to_string()),
                        ..Default::default()
        },
    )
    .unwrap();

    // Worktree B configures none, and nothing was copied from A.
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(b.path()))
            .unwrap()
            .draft_template,
        None
    );
    assert!(!b.path().join(".synthesis/project.toml").exists());

    std::fs::write(a.path().join(".synthesis/project.toml"), "not = = toml").unwrap();
    assert_eq!(
        load_project_config_from(&crate::fs::RootFs::for_root(a.path())).unwrap_err(),
        ERR_MALFORMED_PROJECT_CONFIG,
        "a damaged store is never read as a cleared template"
    );
}

#[test]
fn line_endings_terminators_are_the_expected_bytes() {
    assert_eq!(LineEndings::Lf.terminator(), "\n");
    assert_eq!(LineEndings::Crlf.terminator(), "\r\n");
}
