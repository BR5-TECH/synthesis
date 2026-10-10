//! What one poll keeps, and the configuration it validates (GPP-FR-BOQX,
//! GPP-FR-XSKT, GPP-FR-DEZO, GPP-FR-EHRC, GPP-FR-QCAM, GPP-FR-WOLE,
//! GPP-FR-WYRP, GPP-FR-UBDE, GPP-FR-HZDD, GPP-FR-NLPG, GPP-FR-SUFH).

use super::*;

/// GPP-FR-BOQX: only an open Task of the polling repository whose Status is
/// exactly `Ready` is eligible; no label is read. The repository match is
/// case-insensitive.
#[test]
fn only_open_ready_tasks_of_the_polling_repository_are_eligible() {
    let items = vec![
        ready(1),
        item(2, Some("Ready"), "CLOSED", Some("Task"), "acme", "widgets"),
        item(3, Some("Ready"), "OPEN", Some("Bug"), "acme", "widgets"),
        item(4, Some("Ready"), "OPEN", None, "acme", "widgets"),
        item(5, Some("Ready"), "OPEN", Some("Task"), "acme", "gadgets"),
        item(6, Some("Ready"), "OPEN", Some("Task"), "other", "widgets"),
        item(7, Some("ready"), "OPEN", Some("Task"), "acme", "widgets"),
        item(8, Some("In Progress"), "OPEN", Some("Task"), "acme", "widgets"),
        item(9, None, "OPEN", Some("Task"), "acme", "widgets"),
        item(10, Some("Ready"), "OPEN", Some("Task"), "ACME", "Widgets"),
        ProjectItem { item_id: "ITEM_draft".into(), status: Some("Ready".into()), issue: None },
    ];
    let numbers: Vec<u64> =
        eligible_tasks(&items, &repo()).iter().map(|t| t.issue_number).collect();
    assert_eq!(numbers, vec![1, 10]);
}

/// GPP-FR-XSKT: each kept row carries the repository, the number, the title,
/// the URL, and the Status name.
#[test]
fn a_kept_row_carries_everything_the_panel_renders() {
    let rows = eligible_tasks(&[ready(12)], &repo());
    assert_eq!(
        rows,
        vec![GithubReadyTask {
            repository_host: "github.com".into(),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            issue_number: 12,
            title: "Task 12".into(),
            url: "https://github.com/acme/widgets/issues/12".into(),
            status: "Ready".into(),
        }]
    );
}

/// GPP-FR-XSKT / GPP-FR-DEZO: a poll validates the configuration, then reads
/// the items, and keeps the eligible ones.
#[test]
fn a_poll_validates_then_reads_and_keeps_eligible_issues() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let fake = FakeProjects::with_items(vec![ready(1), ready(2), item(3, Some("Todo"), "OPEN", Some("Task"), "acme", "widgets")]);
    let found = poll_once(&root, &fake, SECRET, &repo(), PROJECT).expect("a poll");
    assert_eq!(found.project_title, "Roadmap");
    assert_eq!(found.tasks.iter().map(|t| t.issue_number).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(fake.calls(), vec![Call::Shape(PROJECT.into()), Call::Items(PROJECT.into())]);
}

/// GPP-FR-DEZO / GPP-FR-EHRC: each configuration defect is its own typed
/// error with text that says what to change in Project settings.
#[test]
fn each_configuration_defect_is_named() {
    let mut no_field = valid_shape();
    no_field.status_field = None;
    assert_eq!(validate_shape(&no_field).unwrap_err().code, ERR_STATUS_FIELD_MISSING);

    let mut no_ready = valid_shape();
    no_ready.status_field.as_mut().unwrap().options.retain(|o| o.name != "Ready");
    assert_eq!(validate_shape(&no_ready).unwrap_err().code, ERR_READY_OPTION_MISSING);

    let mut no_progress = valid_shape();
    no_progress.status_field.as_mut().unwrap().options.retain(|o| o.name != "In Progress");
    let failure = validate_shape(&no_progress).unwrap_err();
    assert_eq!(failure.code, ERR_IN_PROGRESS_OPTION_MISSING);
    assert_eq!(failure.project_title.as_deref(), Some("Roadmap"));

    // Exact names: a differently cased option does not count.
    let mut cased = valid_shape();
    for option in &mut cased.status_field.as_mut().unwrap().options {
        option.name = option.name.to_lowercase();
    }
    assert_eq!(validate_shape(&cased).unwrap_err().code, ERR_READY_OPTION_MISSING);

    let valid = validate_shape(&valid_shape()).unwrap();
    assert_eq!(valid.in_progress_option_id, "OPT_progress");
    assert_eq!(valid.ready_option_id, "OPT_ready");

    // A Project that does not resolve is `project_unavailable`.
    let fake = FakeProjects::new();
    *fake.shape.lock().unwrap() = Err(ERR_PROJECT_UNAVAILABLE.into());
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let failure = poll_once(&root, &fake, SECRET, &repo(), PROJECT).unwrap_err();
    assert_eq!(failure.code, ERR_PROJECT_UNAVAILABLE);
    assert_eq!(fake.calls(), vec![Call::Shape(PROJECT.into())], "no items read");

    for code in [
        ERR_PROJECT_UNAVAILABLE,
        ERR_STATUS_FIELD_MISSING,
        ERR_READY_OPTION_MISSING,
        ERR_IN_PROGRESS_OPTION_MISSING,
    ] {
        assert!(is_configuration_error(code));
        let text = error_text(code);
        assert!(text.contains("Project"), "{code}: {text}");
    }
}

/// GPP-FR-QCAM: an issue a shadow draft or a pending claim names is not a
/// snapshot row, by repository owner, name, and number.
#[test]
fn an_issue_a_shadow_draft_or_a_pending_claim_names_is_excluded() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    crate::drafts::create_github_shadow_draft(&root, "Task 1", "body", link(1)).unwrap();
    crate::project_settings::save_github_pending_claims_to(&root, &[pending(2, None)]).unwrap();
    let fake = FakeProjects::with_items(vec![ready(1), ready(2), ready(3)]);
    let found = poll_once(&root, &fake, SECRET, &repo(), PROJECT).unwrap();
    assert_eq!(found.tasks.iter().map(|t| t.issue_number).collect::<Vec<_>>(), vec![3]);

    // The same issue number in another repository is not excluded.
    let other = vec![GithubReadyTask {
        repository_host: "github.com".into(),
        repository_owner: "other".into(),
        repository_name: "widgets".into(),
        issue_number: 1,
        title: "t".into(),
        url: "u".into(),
        status: "Ready".into(),
    }];
    assert_eq!(exclude_claimed(other.clone(), &[link(1)], &[]), other);
}

/// GPP-FR-WOLE: only an issue the snapshot rows or a pending claim hold opens,
/// and only at a `github.com` issue address of the polling repository.
#[test]
fn only_a_listed_issue_of_the_polling_repository_opens() {
    let tasks = eligible_tasks(&[ready(4)], &repo());
    let claims = vec![pending(9, Some("d1"))];
    assert_eq!(
        listed_issue_url(4, &repo(), &tasks, &claims).unwrap(),
        "https://github.com/acme/widgets/issues/4"
    );
    assert_eq!(
        listed_issue_url(9, &repo(), &tasks, &claims).unwrap(),
        "https://github.com/acme/widgets/issues/9"
    );
    assert_eq!(listed_issue_url(5, &repo(), &tasks, &claims), Err(ERR_ISSUE_NOT_LISTED.into()));

    // A URL of another host, another repository, or another kind of page is
    // refused even when a row holds it.
    for url in [
        "https://evil.example/acme/widgets/issues/4",
        "https://github.com/acme/other/issues/4",
        "https://github.com/acme/widgets/pull/4",
        "file:///etc/passwd",
        "https://user@github.com/acme/widgets/issues/4",
    ] {
        let mut bad = tasks.clone();
        bad[0].url = url.into();
        assert_eq!(
            listed_issue_url(4, &repo(), &bad, &[]),
            Err(ERR_ISSUE_NOT_LISTED.into()),
            "{url}"
        );
    }
}

/// GPP-FR-WYRP: the listing carries node id, title, owner login, and number,
/// with no duplicate.
#[test]
fn the_project_listing_has_no_duplicates() {
    let viewer = serde_json::json!({ "nodes": [
        { "id": "P1", "title": "Roadmap", "number": 1, "owner": { "login": "acme" } },
        { "id": "P2", "title": "Personal", "number": 3, "owner": { "login": "me" } },
    ]});
    let owner = serde_json::json!({ "nodes": [
        { "id": "P1", "title": "Roadmap", "number": 1, "owner": { "login": "acme" } },
        { "id": "P3", "title": "Ops", "number": 7, "owner": { "login": "acme" } },
    ]});
    let mut all = client::projects_of(Some(&viewer));
    all.extend(client::projects_of(Some(&owner)));
    let listed = client::dedupe_projects(all);
    assert_eq!(listed.iter().map(|p| p.node_id.as_str()).collect::<Vec<_>>(), vec!["P1", "P2", "P3"]);
    assert_eq!(
        listed[2],
        GithubProjectOption { node_id: "P3".into(), title: "Ops".into(), owner_login: "acme".into(), number: 7 }
    );
    let json = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(json, serde_json::json!({ "nodeId": "P1", "title": "Roadmap", "ownerLogin": "acme", "number": 1 }));
}

/// GPP-FR-UBDE: every shadow draft is a row, whatever its status, with its
/// link and whether a graduation run holds it.
#[test]
fn every_shadow_draft_is_a_row() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let shadow = crate::drafts::create_github_shadow_draft(&root, "Task 1", "b", link(1)).unwrap();
    crate::drafts::create_draft_impl(&root, Some("ordinary"), None).unwrap();
    let mut drafts = crate::drafts::list_drafts_impl(&root).drafts;
    // One locked by a run, as `attach_graduation` would report it.
    for draft in &mut drafts {
        if draft.id == shadow.draft.id {
            draft.graduation = Some(crate::graduation::DraftGraduation {
                run_id: "run-1".into(),
                state: crate::graduation::GraduationRunState::Queued,
                locked: true,
                graduated: false,
            });
        }
    }
    let rows = view::shadow_rows(&drafts);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.draft_id, shadow.draft.id);
    assert_eq!(row.status, crate::drafts::DraftStatus::GithubShadow);
    assert_eq!(row.issue_number, 1);
    assert_eq!(row.project_node_id, PROJECT);
    assert!(row.locked);
    let json = serde_json::to_value(row).unwrap();
    assert_eq!(json["status"], "github_shadow");
    assert_eq!(json["issueUrl"], "https://github.com/acme/widgets/issues/1");

    // A graduated shadow reports `graduated`.
    drafts.iter_mut().for_each(|d| d.status = crate::drafts::DraftStatus::Graduated);
    assert_eq!(view::shadow_rows(&drafts)[0].status, crate::drafts::DraftStatus::Graduated);
}

/// GPP-FR-HZDD: an interval is unset or one of 1, 5, 15, 30, 60.
#[test]
fn only_the_five_intervals_are_accepted() {
    for minutes in [None, Some(1), Some(5), Some(15), Some(30), Some(60)] {
        assert_eq!(super::super::validate_interval(minutes), Ok(()));
    }
    for minutes in [0, 2, 10, 45, 61, 120] {
        assert_eq!(
            super::super::validate_interval(Some(minutes)),
            Err(ERR_INVALID_INTERVAL.to_string())
        );
    }
}

/// GPP-FR-NLPG: the settings are the Project node id and the interval, stored
/// project-public through PSS.
#[test]
fn the_settings_are_project_public() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let saved = settings(Some(PROJECT));
    crate::project_settings::save_github_polling_settings_to(&root, &saved).unwrap();
    assert_eq!(crate::project_settings::load_github_polling_settings_from(&root).unwrap(), saved);
    let public = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(public.contains(PROJECT));
    assert!(!dir.path().join(".synthesis/local.toml").exists());
    let json = serde_json::to_value(&saved).unwrap();
    assert_eq!(json, serde_json::json!({ "projectNodeId": PROJECT, "intervalMinutes": 5 }));
}

/// GPP-FR-SUFH: the vocabulary is exactly the codes the contract names, each
/// with displayable text.
#[test]
fn the_error_vocabulary_is_exact() {
    let codes = [
        (ERR_NO_PROJECT, "no_project_open"),
        (ERR_UNCONFIGURED, "polling_unconfigured"),
        (ERR_CONFIGURATION_INVALID, "polling_configuration_invalid"),
        (ERR_INVALID_INTERVAL, "invalid_interval"),
        (ERR_PROJECT_UNAVAILABLE, "project_unavailable"),
        (ERR_STATUS_FIELD_MISSING, "status_field_missing"),
        (ERR_READY_OPTION_MISSING, "ready_option_missing"),
        (ERR_IN_PROGRESS_OPTION_MISSING, "in_progress_option_missing"),
        (ERR_GITHUB_UNREACHABLE, "github_unreachable"),
        (ERR_REQUEST_FAILED, "github_request_failed"),
        (ERR_TASK_NOT_READY, "task_not_ready"),
        (ERR_CLAIM_PENDING, "claim_pending"),
        (ERR_CLAIM_IN_PROGRESS, "claim_in_progress"),
        (ERR_NO_PENDING_CLAIM, "no_pending_claim"),
        (ERR_STATUS_UPDATE_FAILED, "status_update_failed"),
        (ERR_PENDING_CLAIM_WRITE_FAILED, "pending_claim_write_failed"),
        (ERR_SHADOW_CREATE_FAILED, "shadow_draft_create_failed"),
        (ERR_ISSUE_NOT_LISTED, "issue_not_listed"),
    ];
    for (constant, spelled) in codes {
        assert_eq!(constant, spelled);
        assert!(!error_text(constant).is_empty());
    }
    // The GHP remote refusals pass through with their own text.
    for code in ["no_remote_configured", "no_github_remote", "token_unavailable", "issues_inaccessible"] {
        assert_ne!(error_text(code), "Publication is not available.", "{code}");
    }
}
