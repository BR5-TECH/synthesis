//! The GitHub publication settings (PSS-FR-HWBG, PSS-FR-RDXE, PSS-FR-VCZM).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

fn fresh() -> (TempDir, crate::fs::RootFs) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn project_toml(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap()
}

/// PSS-FR-HWBG: an unset store reads as the defaults: `Feature`, `Task`, and
/// the inheriting policy.
#[test]
fn an_unset_store_reads_as_the_defaults() {
    let (_dir, root) = fresh();
    let settings = load_github_publication_settings_from(&root).unwrap();
    assert_eq!(settings.parent_issue_types, vec!["Feature".to_string()]);
    assert_eq!(settings.sub_issue_type, "Task");
    assert_eq!(settings.sub_issue_milestone_policy, MilestonePolicy::InheritParent);
    assert_eq!(settings, GithubPublicationSettings::default());
}

/// PSS-FR-HWBG / PSS-FR-VCZM: the settings live in a table of their own in the
/// project-public store, never in the project-local one, and a write carries
/// every other section through — the polling settings included.
#[test]
fn the_settings_are_project_public_and_leave_polling_alone() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"crlf\"\n\n[plugins]\nenabled = [\"git-pr\"]\n",
    )
    .unwrap();
    let polling = GithubPollingSettings { project_node_id: Some("PVT_1".into()), interval_minutes: Some(15) };
    save_github_polling_settings_to(&root, &polling).unwrap();

    let saved = GithubPublicationSettings {
        parent_issue_types: vec!["Feature".into(), "Epic".into()],
        sub_issue_type: "Bug".into(),
        sub_issue_milestone_policy: MilestonePolicy::AuthorSelected,
    };
    save_github_publication_settings_to(&root, &saved).unwrap();

    assert_eq!(load_github_publication_settings_from(&root).unwrap(), saved);
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), polling);
    let text = project_toml(&dir);
    assert!(text.contains("[githubPublication]"));
    assert!(text.contains("[githubPolling]"));
    assert!(text.contains("git-pr") && text.contains("crlf"));
    assert!(!dir.path().join(".synthesis/local.toml").exists());

    // The other direction: a polling write carries the publication settings.
    save_github_polling_settings_to(&root, &GithubPollingSettings::default()).unwrap();
    assert_eq!(load_github_publication_settings_from(&root).unwrap(), saved);
    assert!(!project_toml(&dir).contains("githubPolling"));
}

/// PSS-FR-RDXE: a value that is not valid reads as its default, one field at a
/// time; a Type GitHub does not list is valid and is returned unchanged.
#[test]
fn an_invalid_stored_value_reads_as_its_default() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "[githubPublication]\nparentIssueTypes = [\"  \", \"\"]\nsubIssueType = \"  \"\nsubIssueMilestonePolicy = \"sometimes\"\n",
    )
    .unwrap();
    assert_eq!(load_github_publication_settings_from(&root).unwrap(), GithubPublicationSettings::default());

    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "[githubPublication]\nparentIssueTypes = [\"Retired\", \"retired\", \" Epic \"]\nsubIssueType = \"Gone\"\nsubIssueMilestonePolicy = \"no_milestone\"\n",
    )
    .unwrap();
    let settings = load_github_publication_settings_from(&root).unwrap();
    assert_eq!(settings.parent_issue_types, vec!["Retired".to_string(), "Epic".to_string()]);
    assert_eq!(settings.sub_issue_type, "Gone");
    assert_eq!(settings.sub_issue_milestone_policy, MilestonePolicy::NoMilestone);
}

/// PSS-FR-RDXE: a malformed project-public file stays the typed error of
/// PSS-FR-10 and is not overwritten.
#[test]
fn a_malformed_store_stays_the_typed_error() {
    let (dir, root) = fresh();
    std::fs::write(dir.path().join(".synthesis/project.toml"), "not = [valid").unwrap();
    assert_eq!(
        load_github_publication_settings_from(&root),
        Err(ERR_MALFORMED_PROJECT_CONFIG.to_string())
    );
    assert!(save_github_publication_settings_to(&root, &GithubPublicationSettings::default()).is_err());
    assert_eq!(project_toml(&dir), "not = [valid");
}

/// PSS-FR-VCZM: saving the project configuration through the partial view
/// keeps the publication settings.
#[test]
fn the_project_config_save_carries_the_settings_through() {
    let (_dir, root) = fresh();
    let saved = GithubPublicationSettings {
        parent_issue_types: vec!["Epic".into()],
        ..Default::default()
    };
    save_github_publication_settings_to(&root, &saved).unwrap();
    let mut config = load_project_config_from(&root).unwrap();
    config.line_endings = LineEndings::Crlf;
    save_project_config_to(&root, config).unwrap();
    assert_eq!(load_github_publication_settings_from(&root).unwrap(), saved);
}
