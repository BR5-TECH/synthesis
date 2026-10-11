//! GitHub-shadow drafts (DRS-FR-INCJ, DRS-FR-WFLY, DRS-FR-XDWS,
//! DRS-FR-QPSC, DRS-FR-JYIO, DRS-FR-EZDB).

use super::*;

fn link(number: u64) -> GithubIssueLink {
    GithubIssueLink {
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        project_node_id: "PVT_1".into(),
        claim_state: GithubClaimState::Claimed,
    }
}

fn shadow(root: &crate::fs::RootFs, title: &str, number: u64) -> DraftCreated {
    create_github_shadow_draft(root, title, "Line one\nLine two\n", link(number)).unwrap()
}

/// Every file under the drafts root with its bytes, so a refusal can be shown
/// to have changed nothing.
fn drafts_tree(dir: &TempDir) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn walk(path: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&dir.path().join(".synthesis/drafts"), &mut out);
    out.sort();
    out
}

/// DRS-FR-INCJ: the draft is created at the drafts root with status
/// `github_shadow`, the given name, the link in its record, and the prompt
/// normalised to the project's line endings — without the template.
#[test]
fn a_shadow_draft_is_created_at_the_root_with_its_link_and_no_template() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: project_settings::LineEndings::Crlf,
            draft_template: Some("# Template\n".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    create_drafts_folder_impl(root, "", "UI").unwrap();

    let created = shadow(root, "Fix the login page", 4);
    assert_eq!(created.draft.name, "Fix the login page");
    assert_eq!(created.draft.status, DraftStatus::GithubShadow);
    assert_eq!(created.draft.github_issue, Some(link(4)));
    assert_eq!(folder_of(root, &created.draft.id), "");
    let body = std::fs::read_to_string(
        draft_dir(root, &created.draft.id).unwrap().join(FILES_DIR).join(&created.file),
    )
    .unwrap();
    assert_eq!(body, "Line one\r\nLine two\r\n");
    assert!(!body.contains("Template"));

    // A title with no text takes the default name; a very long one is cut.
    let blank = shadow(root, "   ", 5);
    assert!(blank.draft.name.starts_with("Untitled"));
    let long = shadow(root, &"x".repeat(400), 6);
    assert!(long.draft.name.len() <= 200);
}

/// DRS-FR-WFLY: `github_shadow` is set by creation alone. `set_draft_status`
/// cannot set it, and cannot move a shadow draft out of it.
#[test]
fn the_shadow_status_is_set_by_creation_alone() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let ordinary = draft(root, "d");
    assert_eq!(
        set_draft_status_impl(root, &ordinary, DraftStatus::GithubShadow),
        Err(ERR_STATUS_NOT_SETTABLE.to_string())
    );
    let created = shadow(root, "Task", 1);
    for status in [DraftStatus::Active, DraftStatus::Archived] {
        assert_eq!(
            set_draft_status_impl(root, &created.draft.id, status),
            Err(ERR_GITHUB_SHADOW.to_string())
        );
    }
    assert_eq!(serde_json::to_value(DraftStatus::GithubShadow).unwrap(), "github_shadow");
    // The stored value reads back as itself.
    assert_eq!(read_record(root, &created.draft.id).unwrap().status, DraftStatus::GithubShadow);
}

/// DRS-FR-XDWS: the link is reported by `list_drafts`, `open_draft`, and
/// `draft_record`, on the wire as `githubIssue`, and survives graduation.
#[test]
fn the_link_is_reported_everywhere_and_never_removed() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = shadow(root, "Task", 3);
    let ordinary = draft(root, "d");

    let row = listed(root).into_iter().find(|d| d.id == created.draft.id).unwrap();
    assert_eq!(row.github_issue, Some(link(3)));
    assert_eq!(open_draft_impl(root, &created.draft.id).unwrap().github_issue, Some(link(3)));
    assert_eq!(draft_record(root, &created.draft.id).unwrap().github_issue, Some(link(3)));

    let json = serde_json::to_value(&row).unwrap();
    assert_eq!(
        json["githubIssue"],
        serde_json::json!({
            "repositoryHost": "github.com",
            "repositoryOwner": "acme", "repositoryName": "widgets", "issueNumber": 3,
            "issueUrl": "https://github.com/acme/widgets/issues/3",
            "projectNodeId": "PVT_1", "claimState": "claimed"
        })
    );
    let plain = listed(root).into_iter().find(|d| d.id == ordinary).unwrap();
    assert!(serde_json::to_value(&plain).unwrap().get("githubIssue").is_none());

    set_draft_graduated(root, &created.draft.id, "run-1").unwrap();
    assert_eq!(draft_record(root, &created.draft.id).unwrap().github_issue, Some(link(3)));
    assert!(is_github_shadow(root, &created.draft.id));
    assert!(!is_github_shadow(root, &ordinary));
}

/// DRS-FR-QPSC: a shadow draft refuses a prompt write, a rename, a status
/// change, a move, and a deletion with `draft_github_shadow`, and nothing on
/// disk changes.
#[test]
fn a_shadow_draft_refuses_every_write_and_changes_nothing() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    create_drafts_folder_impl(root, "", "UI").unwrap();
    let created = shadow(root, "Task", 1);
    let id = &created.draft.id;
    let before = drafts_tree(&dir);
    let refusal = Err(ERR_GITHUB_SHADOW.to_string());

    assert_eq!(save_draft_file_impl(root, id, &created.file, "new text").map(|_| ()), refusal);
    assert_eq!(rename_draft_impl(root, id, "renamed").map(|_| ()), refusal);
    assert_eq!(set_draft_status_impl(root, id, DraftStatus::Archived).map(|_| ()), refusal);
    assert_eq!(move_draft_to_folder_impl(root, id, "UI").map(|_| ()), refusal);
    assert_eq!(delete_draft_impl(root, root, id), refusal);
    assert_eq!(require_not_github_shadow(root, id), refusal);
    assert_eq!(drafts_tree(&dir), before, "nothing changed");

    // Also once it has graduated: the refusal holds whatever its status.
    set_draft_graduated(root, id, "run-1").unwrap();
    assert_eq!(rename_draft_impl(root, id, "renamed").map(|_| ()), refusal);

    // An ordinary draft passes the guard.
    let ordinary = draft(root, "d");
    assert_eq!(require_not_github_shadow(root, &ordinary), Ok(()));
    assert_eq!(require_not_github_shadow(root, "no-such-draft"), Ok(()));
}

/// DRS-FR-JYIO: every read answers for a shadow draft as for any other.
#[test]
fn every_read_answers_for_a_shadow_draft() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = shadow(root, "Task", 1);
    let prompt = read_draft_prompt(root, &created.draft.id).unwrap();
    assert_eq!(prompt.content, "Line one\nLine two\n");
    assert_eq!(prompt.prompt_path, created.file);
    let contents = load_draft_file_impl(root, &created.draft.id, &created.file).unwrap();
    assert_eq!(contents.body, "Line one\nLine two\n");
    assert!(!listed(root)[0].inconsistent);
    assert_eq!(
        search_drafts_impl(root, "Line two").iter().map(|m| m.draft_id.clone()).collect::<Vec<_>>(),
        vec![created.draft.id.clone()]
    );
}

/// DRS-FR-EZDB: a shadow draft reports `graduated` while a committed run
/// stands and `github_shadow` otherwise — never `active` — and a release keeps
/// its prompt, link, and folder.
#[test]
fn a_shadow_draft_reports_graduated_or_github_shadow() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let created = shadow(root, "Task", 1);
    let id = &created.draft.id;
    let mut run = crate::graduation::GraduationRun::new_for_test("g1", id, "2026-10-02T09:00:00Z");
    run.commits = vec!["abc123".into()];
    let standing =
        crate::graduation::GraduationQueue { project_key: "p".into(), runs: vec![run.clone()] };
    assert_eq!(
        resolved_shadow_status_for_test(&standing, id, DraftStatus::GithubShadow),
        DraftStatus::Graduated
    );
    // Released: the run is discarded, and the stored `graduated` reads as the
    // shadow status rather than `active`.
    set_draft_graduated(root, id, "g1").unwrap();
    run.state = crate::graduation::GraduationRunState::Discarded;
    let released = crate::graduation::GraduationQueue { project_key: "p".into(), runs: vec![run] };
    assert_eq!(
        resolved_shadow_status_for_test(&released, id, DraftStatus::Graduated),
        DraftStatus::GithubShadow
    );
    // An ordinary draft in the same position reads `active`.
    assert_eq!(
        resolved_status_for_test(&released, id, DraftStatus::Graduated),
        DraftStatus::Active
    );
    assert_eq!(draft_record(root, id).unwrap().github_issue, Some(link(1)));
    assert_eq!(read_draft_prompt(root, id).unwrap().content, "Line one\nLine two\n");
    assert_eq!(folder_of(root, id), "");
}
