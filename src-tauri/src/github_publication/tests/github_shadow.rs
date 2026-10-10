//! GitHub-shadow drafts and the publication repository without a draft
//! (GHP-FR-BKLT, GHP-FR-YDAN, GHP-FR-MJTB, GHP-FR-PVOA, and
//! `GPP-github-polling.md` GPP-FR-RGNM).

use super::*;
use crate::drafts::DraftStatus;

fn link(number: u64) -> crate::drafts::GithubIssueLink {
    crate::drafts::GithubIssueLink {
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        project_node_id: "PVT_1".into(),
        claim_state: crate::drafts::GithubClaimState::Claimed,
    }
}

fn shadow(root: &crate::fs::RootFs) -> String {
    crate::drafts::create_github_shadow_draft(root, "Task", "body", link(4)).unwrap().draft.id
}

/// GHP-FR-BKLT / GHP-FR-PVOA: a shadow draft is not publishable, with the
/// reason `draft_github_shadow`, whatever its status; every flow step that
/// writes refuses it before anything is written.
#[test]
fn a_shadow_draft_is_refused_by_publication() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = shadow(&root);
    for status in [DraftStatus::GithubShadow, DraftStatus::Graduated] {
        let refused = flow::local_eligibility(&root, &id, status, None);
        assert!(!refused.publishable);
        assert_eq!(refused.reason_code.as_deref(), Some(ERR_DRAFT_GITHUB_SHADOW));
        assert!(refused.reason.is_some());
    }
    assert_eq!(flow::require_not_shadow(&root, &id), Err("draft_github_shadow".to_string()));
    assert_eq!(ERR_DRAFT_GITHUB_SHADOW, "draft_github_shadow");
    // Nothing was written: no publication store exists for it.
    assert!(store::read_store(&root, &id).unwrap().attempt.is_none());
    let path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    assert!(!path.exists());

    // An ordinary draft passes.
    let ordinary = draft(&root, "widget");
    assert_eq!(flow::require_not_shadow(&root, &ordinary), Ok(()));
    assert!(flow::local_eligibility(&root, &ordinary, DraftStatus::Active, None).publishable);
}

/// GHP-FR-MJTB: `open_publication_issue` accepts the URL a shadow draft's
/// link holds, and still refuses any URL the draft does not hold.
#[test]
fn a_shadow_drafts_issue_link_is_an_openable_url() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = shadow(&root);
    assert_eq!(
        flow::is_recorded_issue_url(&root, &id, "https://github.com/acme/widgets/issues/4"),
        Ok(true)
    );
    assert_eq!(
        flow::is_recorded_issue_url(&root, &id, "https://github.com/acme/widgets/issues/5"),
        Ok(false)
    );
    let ordinary = draft(&root, "widget");
    assert_eq!(
        flow::is_recorded_issue_url(&root, &ordinary, "https://github.com/acme/widgets/issues/4"),
        Ok(false)
    );
}

/// GHP-FR-YDAN / GPP-FR-RGNM: the publication repository resolves without a
/// draft to the remote name and the owner and name, or to the GHP-FR-ZRFP
/// refusal, and nothing is created on GitHub.
#[test]
fn the_publication_repository_resolves_without_a_draft() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let fake = FakeGithub::publishable();
    let remotes = vec![configured("origin", "git@github.com:acme/ydan-widgets.git")];

    let resolved = flow::resolve_repository_from(&root, &remotes, Some("secret"), &fake).unwrap();
    assert_eq!(
        resolved,
        PublicationRepository {
            repository_host: "github.com".into(),
            remote_name: "origin".into(),
            repository_owner: "acme".into(),
            repository_name: "ydan-widgets".into(),
        }
    );
    assert!(fake
        .calls()
        .iter()
        .all(|c| matches!(c, Call::Probe(_))), "only the read-only probe");

    assert_eq!(
        flow::resolve_repository_from(&root, &[], Some("secret"), &fake),
        Err(ERR_NO_REMOTE.to_string())
    );
    assert_eq!(
        flow::resolve_repository_from(
            &root,
            &[configured("origin", "https://git.internal/acme/widgets")],
            Some("secret"),
            &fake
        ),
        Err(ERR_NO_GITHUB_REMOTE.to_string())
    );
    assert_eq!(
        flow::resolve_repository_from(&root, &remotes, None, &fake),
        Err(ERR_TOKEN_UNAVAILABLE.to_string())
    );
    // It writes nothing: no remote choice is persisted.
    assert!(crate::project_settings::load_publication_remote_selection_from(&root).is_none());
}

/// A mock application with the project open and `fake` behind the issues
/// seam, for the command bodies.
fn mock_app_for(dir: &tempfile::TempDir, store: &tempfile::TempDir, fake: std::sync::Arc<FakeGithub>) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let store_root = std::fs::canonicalize(store.path()).unwrap();
    let app = tauri::test::mock_app();
    app.manage(crate::fs::FsAccessState::default());
    app.manage(crate::project::ProjectState::default());
    app.manage(crate::graduation::GraduationState::rooted_at(store_root.clone()));
    app.manage(GithubIssuesSeam(fake));
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    app.manage(crate::github_tokens::GithubTokens::default());
    let handle = app.handle().clone();
    handle.state::<crate::fs::FsAccessState>().install_for_worktree_and(&root, &store_root).unwrap();
    let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
    let project = handle.state::<crate::project::ProjectState>();
    project.set_root_with_access(root.clone(), Some(access));
    project.set_anchor(root.to_string_lossy().into_owned());
    project.set_store(root);
    app
}

/// GHP-FR-BKLT: publishing, retrying, answering a recovery, and both cancels
/// refuse a shadow draft with `draft_github_shadow`, make no GitHub call, and
/// write no publication file.
#[test]
fn every_publication_command_refuses_a_shadow_draft() {
    let dir = project();
    let store = tempfile::TempDir::new().unwrap();
    let fake = std::sync::Arc::new(FakeGithub::publishable());
    let app = mock_app_for(&dir, &store, fake.clone());
    let handle = app.handle().clone();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = shadow(&root);
    let refusal = Err("draft_github_shadow".to_string());

    assert_eq!(
        crate::github_publication::publish_draft_to_github_impl(&handle, id.clone(), "origin".into(), true, Default::default()).map(|_| ()),
        refusal
    );
    assert_eq!(crate::github_publication::retry_draft_publication_impl(&handle, id.clone()).map(|_| ()), refusal);
    assert_eq!(
        crate::github_publication::resolve_draft_publication_conflict_impl(
            &handle,
            id.clone(),
            RecoveryChoice::PublishNew
        )
        .map(|_| ()),
        refusal
    );
    assert_eq!(crate::github_publication::cancel_draft_publication_conflict_impl(&handle, id.clone()), refusal);
    assert_eq!(crate::github_publication::cancel_draft_publication_attempt_impl(&handle, id.clone()), refusal);
    assert_eq!(
        crate::github_publication::list_publication_remotes_impl(&handle, id.clone()).map(|_| ()),
        refusal
    );
    let view = crate::github_publication::get_draft_publication_impl(&handle, id.clone()).unwrap();
    assert_eq!(view.eligibility.reason_code.as_deref(), Some("draft_github_shadow"));

    assert!(fake.calls().is_empty(), "no GitHub call");
    assert!(!crate::drafts::draft_publication_path(&root, &id).unwrap().exists());
    assert!(crate::project_settings::load_publication_remote_selection_from(&root).is_none());
}

/// GHP-FR-MJTB: a shadow link that holds an address other than a
/// `github.com` issue is refused.
#[test]
fn a_shadow_link_that_is_not_a_github_issue_is_refused() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    for url in [
        "https://evil.example/acme/widgets/issues/4",
        "file:///etc/passwd",
        "https://github.com/acme/widgets/pull/4",
        "https://user@github.com/acme/widgets/issues/4",
    ] {
        let mut hostile = link(4);
        hostile.issue_url = url.into();
        let id = crate::drafts::create_github_shadow_draft(&root, "Task", "b", hostile).unwrap().draft.id;
        assert_eq!(flow::is_recorded_issue_url(&root, &id, url), Ok(false), "{url}");
    }
}
