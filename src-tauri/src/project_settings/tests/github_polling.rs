//! The GitHub polling settings and the GitHub pending claims (PSS-FR-OPQD,
//! PSS-FR-CNZO, PSS-FR-TXBB, PSS-FR-TXJV).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

fn fresh() -> (TempDir, crate::fs::RootFs) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

fn claim(number: u64, draft: Option<&str>) -> GithubPendingClaim {
    GithubPendingClaim {
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        project_node_id: "PVT_1".into(),
        draft_id: draft.map(str::to_string),
        claimed_at: "2026-10-02T09:00:00Z".into(),
    }
}

/// PSS-FR-OPQD: the settings are project-public, each unset or stored, an
/// unset value has no key, and a write carries every other section through.
#[test]
fn the_polling_settings_are_project_public_and_merge_into_the_store() {
    let (dir, root) = fresh();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"crlf\"\n\n[plugins]\nenabled = [\"git-pr\"]\n",
    )
    .unwrap();
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), GithubPollingSettings::default());

    let saved = GithubPollingSettings {
        project_node_id: Some("PVT_1".into()),
        interval_minutes: Some(15),
    };
    save_github_polling_settings_to(&root, &saved).unwrap();
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), saved);
    let text = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(text.contains("git-pr"), "another section was carried through");
    assert!(text.contains("crlf"));
    assert!(text.contains("[githubPolling]"));
    assert!(!dir.path().join(".synthesis/local.toml").exists());

    // An unset value has no key.
    let interval_only = GithubPollingSettings { project_node_id: None, interval_minutes: Some(5) };
    save_github_polling_settings_to(&root, &interval_only).unwrap();
    let text = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(!text.contains("projectNodeId"));
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), interval_only);
    save_github_polling_settings_to(&root, &GithubPollingSettings::default()).unwrap();
    let text = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(!text.contains("githubPolling"));

    // And a write of another section carries the settings through.
    save_github_polling_settings_to(&root, &saved).unwrap();
    save_project_config_to(&root, load_project_config_from(&root).unwrap()).unwrap();
    assert_eq!(load_github_polling_settings_from(&root).unwrap(), saved);
}

/// PSS-FR-CNZO: an interval outside the five values and a Project node id
/// that is not a non-empty string each read as unset; a malformed file stays
/// the typed error.
#[test]
fn invalid_stored_values_read_as_unset() {
    let (dir, root) = fresh();
    let path = dir.path().join(".synthesis/project.toml");
    for (body, expected) in [
        ("[githubPolling]\nprojectNodeId = \"\"\nintervalMinutes = 7\n", GithubPollingSettings::default()),
        ("[githubPolling]\nprojectNodeId = 12\nintervalMinutes = \"5\"\n", GithubPollingSettings::default()),
        ("[githubPolling]\nprojectNodeId = \"  \"\nintervalMinutes = -5\n", GithubPollingSettings::default()),
        (
            "[githubPolling]\nprojectNodeId = \"PVT_1\"\nintervalMinutes = 0\n",
            GithubPollingSettings { project_node_id: Some("PVT_1".into()), interval_minutes: None },
        ),
        (
            "[githubPolling]\nintervalMinutes = 60\n",
            GithubPollingSettings { project_node_id: None, interval_minutes: Some(60) },
        ),
        ("githubPolling = \"not a table\"\n", GithubPollingSettings::default()),
    ] {
        std::fs::write(&path, body).unwrap();
        assert_eq!(load_github_polling_settings_from(&root).unwrap(), expected, "{body}");
    }
    std::fs::write(&path, "this is = = not toml").unwrap();
    assert_eq!(
        load_github_polling_settings_from(&root),
        Err(ERR_MALFORMED_PROJECT_CONFIG.to_string())
    );
}

/// PSS-FR-TXBB: the pending claims are a project-local list; a write carries
/// every other project-local section through.
#[test]
fn the_pending_claims_are_project_local_and_merge_into_the_store() {
    let (dir, root) = fresh();
    assert!(load_github_pending_claims_from(&root).is_empty(), "a missing list reads as empty");
    save_drafts_panel_state_to(&root, drafts_state(DraftsStatusFilter::Archived, "x", &["UI"])).unwrap();

    let claims = vec![claim(4, None), claim(5, Some("d-5"))];
    save_github_pending_claims_to(&root, &claims).unwrap();
    assert_eq!(load_github_pending_claims_from(&root), claims);
    assert_eq!(load_drafts_panel_state_from(&root).text_filter, "x", "carried through");

    // A write of another section carries the claims through.
    save_drafts_panel_state_to(&root, DraftsPanelState::default()).unwrap();
    assert_eq!(load_github_pending_claims_from(&root), claims);

    // An empty list has no key.
    save_github_pending_claims_to(&root, &[]).unwrap();
    let text = std::fs::read_to_string(dir.path().join(".synthesis/local.toml")).unwrap();
    assert!(!text.contains("github_pending_claims"));
}

/// PSS-FR-TXJV: a malformed or incomplete record is dropped on read, and the
/// claims never reach the project-public store, whose file stays unwritten,
/// while the local store is gitignored.
#[test]
fn malformed_claims_are_dropped_and_claims_are_never_committed() {
    let (dir, root) = fresh();
    save_github_pending_claims_to(&root, &[claim(4, None)]).unwrap();
    assert!(!dir.path().join(".synthesis/project.toml").exists(), "never project-public");
    let ignore = std::fs::read_to_string(dir.path().join(".synthesis/.gitignore")).unwrap();
    assert!(ignore.contains("local.toml"), "the local store is gitignored");

    std::fs::write(
        dir.path().join(".synthesis/local.toml"),
        r#"
[[github_pending_claims]]
repositoryOwner = "acme"
repositoryName = "widgets"
issueNumber = 4
issueUrl = "https://github.com/acme/widgets/issues/4"
projectNodeId = "PVT_1"
claimedAt = "2026-10-02T09:00:00Z"

[[github_pending_claims]]
repositoryOwner = "acme"
repositoryName = "widgets"
issueNumber = "five"
issueUrl = "https://github.com/acme/widgets/issues/5"
projectNodeId = "PVT_1"
claimedAt = "2026-10-02T09:00:00Z"

[[github_pending_claims]]
repositoryOwner = "acme"
issueNumber = 6
issueUrl = "https://github.com/acme/widgets/issues/6"
projectNodeId = "PVT_1"
claimedAt = "2026-10-02T09:00:00Z"

[[github_pending_claims]]
repositoryOwner = "acme"
repositoryName = "widgets"
issueNumber = 7
issueUrl = ""
projectNodeId = "PVT_1"
claimedAt = "2026-10-02T09:00:00Z"
"#,
    )
    .unwrap();
    assert_eq!(load_github_pending_claims_from(&root), vec![claim(4, None)]);

    std::fs::write(dir.path().join(".synthesis/local.toml"), "github_pending_claims = 3\n").unwrap();
    assert!(load_github_pending_claims_from(&root).is_empty());
}

/// PSS-FR-TXBB: a claim with no shadow draft yet stores no draft id, and the
/// wire shape carries it as `null`.
#[test]
fn a_claim_without_a_draft_round_trips() {
    let (_dir, root) = fresh();
    save_github_pending_claims_to(&root, &[claim(4, None)]).unwrap();
    assert_eq!(load_github_pending_claims_from(&root), vec![claim(4, None)]);
    let json = serde_json::to_value(claim(4, None)).unwrap();
    assert_eq!(json["draftId"], serde_json::Value::Null);
    assert_eq!(json["issueNumber"], 4);
    assert_eq!(json["claimedAt"], "2026-10-02T09:00:00Z");
}
