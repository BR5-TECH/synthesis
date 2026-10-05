//! The publication commands end to end: the chooser's metadata read, the
//! settings commands, a sub-issue publication through the command bodies, retry
//! and recovery keeping the saved choice, and restart recovery (GHP-FR-MDLD,
//! GHP-FR-KVRH, GHP-FR-NQWX, GHP-FR-PTYL, GHP-FR-CVYK, GHP-FR-ATCH,
//! GHP-FR-IBXN, GHP-FR-YPGL, GHP-FR-CDVT, GHP-FR-PVOA).

use std::sync::Arc;

use tauri::Manager;

use super::*;
use crate::github_publication as gp;

type Mock = tauri::test::MockRuntime;

struct Session {
    dir: TempDir,
    _store: TempDir,
    app: tauri::App<Mock>,
    fake: Arc<FakeGithub>,
    id: String,
}

impl Session {
    fn handle(&self) -> tauri::AppHandle<Mock> {
        self.app.handle().clone()
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.dir.path())
    }

    fn publish(&self, choice: PublicationChoiceInput) -> Result<PublicationOutcome, String> {
        gp::publish_draft_to_github_impl(&self.handle(), self.id.clone(), "origin".into(), false, choice)
    }

    fn store(&self) -> PublicationStore {
        store::read_store(&self.root(), &self.id).unwrap()
    }
}

/// A project in a real repository with a GitHub `origin`, one draft, and a
/// token the publication commands resolve.
fn session() -> Session {
    let dir = project();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/acme/widgets.git").unwrap();
    let store = TempDir::new().unwrap();
    let fake = Arc::new(FakeGithub::publishable());
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let store_root = std::fs::canonicalize(store.path()).unwrap();
    let app = tauri::test::mock_app();
    app.manage(crate::fs::FsAccessState::default());
    app.manage(crate::project::ProjectState::default());
    app.manage(crate::graduation::GraduationState::rooted_at(store_root.clone()));
    app.manage(GithubIssuesSeam(fake.clone()));
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    app.manage(crate::github_tokens::GithubTokens::default());
    // The eligibility cache is process-wide and keyed by the secret, so each
    // session owns a secret and no other case answers for it.
    static SESSIONS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SESSIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    app.manage(gp::TestSecret(format!("secret-{n}")));
    let handle = app.handle().clone();
    handle.state::<crate::fs::FsAccessState>().install_for_worktree_and(&root, &store_root).unwrap();
    let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
    let project_state = handle.state::<crate::project::ProjectState>();
    project_state.set_root_with_access(root.clone(), Some(access));
    project_state.set_anchor(root.to_string_lossy().into_owned());
    project_state.set_store(root);
    let fs_root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&fs_root, "Widget window");
    save_prompt(&fs_root, &id, "Do the thing.\n");
    Session { dir, _store: store, app, fake, id }
}

fn sub_of(parent: u64) -> PublicationChoiceInput {
    PublicationChoiceInput { parent_issue_number: Some(parent), ..Default::default() }
}

/// GHP-FR-MDLD / GHP-FR-DZLB: the chooser's read returns the repository, the
/// settings, and the three lists, creates nothing, and writes no attempt.
#[test]
fn the_metadata_command_reads_and_mutates_nothing() {
    let s = session();
    let parent = s.fake.seed("Feature", Some((7, "v1.2")));
    let metadata = gp::load_publication_metadata_impl(&s.handle(), s.id.clone(), "origin".into()).unwrap();

    assert_eq!((metadata.repository_owner.as_str(), metadata.repository_name.as_str()), ("acme", "widgets"));
    assert_eq!(metadata.settings, GithubPublicationSettings::default());
    assert_eq!(metadata.parents.items[0].number, parent);
    assert_eq!(metadata.sub_issue_type.resolved.as_deref(), Some("Task"));
    assert!(s.fake.calls().iter().all(|c| !matches!(c, Call::Create(_) | Call::Update(_) | Call::Link(..) | Call::Unlink(..))));
    assert_eq!(s.store(), PublicationStore::default());
}

/// GHP-FR-MDLD / GHP-FR-CVYK: the read refuses a remote that is not eligible,
/// an archived draft, and a missing token with the existing typed errors.
#[test]
fn the_metadata_command_keeps_the_existing_refusals() {
    let s = session();
    *s.fake.probe.lock().unwrap() = ProbeOutcome::IssuesDisabled;
    assert_eq!(
        gp::load_publication_metadata_impl(&s.handle(), s.id.clone(), "origin".into()).map(|_| ()),
        Err(ERR_ISSUES_DISABLED.to_string())
    );
    let s = session();
    assert_eq!(
        gp::load_publication_metadata_impl(&s.handle(), s.id.clone(), "nowhere".into()).map(|_| ()),
        Err(ERR_NO_REMOTE.to_string())
    );
    assert_eq!(
        gp::load_publication_metadata_impl(&s.handle(), "missing".into(), "origin".into()).map(|_| ()),
        Err(ERR_DRAFT_NOT_FOUND.to_string())
    );
}

/// GHP-FR-UXOT: with every list failing, the read still answers, and a root
/// publication still goes through.
#[test]
fn a_root_publication_survives_every_failed_read() {
    let s = session();
    *s.fake.types.lock().unwrap() = None;
    *s.fake.milestones.lock().unwrap() = None;
    *s.fake.parents_fail.lock().unwrap() = true;
    let metadata = gp::load_publication_metadata_impl(&s.handle(), s.id.clone(), "origin".into()).unwrap();
    assert_eq!(metadata.parents.state, MetadataState::Failed);
    assert_eq!(metadata.issue_types.state, MetadataState::Failed);
    assert_eq!(metadata.milestones.state, MetadataState::Failed);

    assert!(matches!(s.publish(PublicationChoiceInput::default()), Ok(PublicationOutcome::Published { .. })));
}

/// GHP-FR-KVRH / GHP-FR-NQWX / PSS-FR-VCZM: the settings commands default,
/// persist, refuse, and never rewrite the polling settings.
#[test]
fn the_settings_commands_persist_and_refuse() {
    let s = session();
    let handle = s.handle();
    let get = || gp::get_github_publication_settings_impl(&handle);
    assert_eq!(get().unwrap(), GithubPublicationSettings::default());

    crate::project_settings::save_github_polling_settings_to(
        &s.root(),
        &crate::project_settings::GithubPollingSettings {
            project_node_id: Some("PVT_1".into()),
            interval_minutes: Some(15),
        },
    )
    .unwrap();
    let saved = gp::set_github_publication_settings_impl(
        &handle,
        vec!["Feature".into(), "Epic".into()],
        "Bug".into(),
        MilestonePolicy::AuthorSelected,
    )
    .unwrap();
    assert_eq!(get().unwrap(), saved);
    let polling = crate::project_settings::load_github_polling_settings_from(&s.root()).unwrap();
    assert_eq!(polling.project_node_id.as_deref(), Some("PVT_1"));
    assert_eq!(polling.interval_minutes, Some(15));

    assert_eq!(
        gp::set_github_publication_settings_impl(&handle, vec![], "Task".into(), MilestonePolicy::default())
            .map(|_| ()),
        Err(ERR_INVALID_PUBLICATION_SETTINGS.to_string())
    );
    assert_eq!(get().unwrap(), saved, "a refused write changes nothing");
}

/// GHP-FR-PTYL: the Type list of the publication repository's owner, and the
/// repository refusal when none resolves.
#[test]
fn the_issue_type_command_reads_the_owners_types() {
    let s = session();
    let list = gp::list_github_issue_types_impl(&s.handle()).unwrap();
    assert_eq!(list.state, MetadataState::Loaded);
    assert_eq!(list.items.len(), 3);

    *s.fake.types.lock().unwrap() = None;
    let failed = gp::list_github_issue_types_impl(&s.handle()).unwrap();
    assert_eq!(failed.error_code.as_deref(), Some(ERR_ISSUE_TYPES_UNREADABLE));

    let bare = session();
    git2::Repository::open(bare.dir.path()).unwrap().remote_delete("origin").unwrap();
    assert_eq!(gp::list_github_issue_types_impl(&bare.handle()).map(|_| ()), Err(ERR_NO_REMOTE.to_string()));
}

/// GHP-FR-BWNI / GHP-FR-ATCH / GHP-FR-OHGY / GHP-FR-HSCH: a sub-issue goes
/// through the command, saves its choice before the first mutation, creates the
/// issue with the configured Type and the inherited milestone, links it, and
/// the history record says so.
#[test]
fn a_sub_issue_publishes_through_the_command() {
    let s = session();
    let parent = s.fake.seed("Feature", Some((7, "v1.2")));
    let PublicationOutcome::Published { record } = s.publish(sub_of(parent)).unwrap() else {
        panic!("published")
    };
    let choice = record.choice.clone().unwrap();
    assert_eq!(choice.kind, PublicationKind::SubIssue);
    assert_eq!(choice.parent_issue_number, Some(parent));
    assert_eq!(choice.issue_type.as_deref(), Some("Task"));
    assert_eq!(choice.milestone_number, Some(7));
    let issue = s.fake.issue(record.issue_number);
    assert_eq!(issue.parent.unwrap().number, parent);
    assert_eq!(s.store().attempt, None);
    assert_eq!(s.store().publication.len(), 1);
}

/// GHP-FR-CVYK / GHP-FR-ATCH: a refused choice writes no attempt record,
/// persists no remote choice, and creates nothing.
#[test]
fn a_refused_choice_leaves_nothing_behind() {
    let s = session();
    let closed = s.fake.seed("Feature", None);
    s.fake.edit(closed, |i| i.open = false);
    let handle = s.handle();
    let result = gp::publish_draft_to_github_impl(&handle, s.id.clone(), "origin".into(), true, sub_of(closed));

    assert_eq!(result.map(|_| ()), Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string()));
    assert_eq!(s.store(), PublicationStore::default());
    assert!(crate::project_settings::load_publication_remote_selection_from(&s.root()).is_none());
    assert!(!s.fake.calls().iter().any(|c| matches!(c, Call::Create(_))));
}

/// GHP-FR-IBXN / GHP-FR-SWKU / GHP-FR-YPGL: a failed link leaves the attempt;
/// the retry command keeps the choice and offers recovery; **Publish as a new
/// issue** starts a new marker that keeps the same choice.
#[test]
fn retry_and_a_new_attempt_keep_the_saved_choice() {
    let s = session();
    let parent = s.fake.seed("Feature", Some((7, "v1.2")));
    *s.fake.link_error.lock().unwrap() = Some(ERR_SUB_ISSUE_LINK_FAILED.into());
    assert_eq!(s.publish(sub_of(parent)).map(|_| ()), Err(ERR_SUB_ISSUE_LINK_FAILED.to_string()));
    let first = s.store().attempt.unwrap();
    let saved = first.choice.clone().unwrap();
    assert_eq!(saved.parent_issue_number, Some(parent));

    *s.fake.link_error.lock().unwrap() = None;
    // The settings change after the attempt began; the retry does not read them.
    gp::set_github_publication_settings_impl(
        &s.handle(),
        vec!["Epic".into()],
        "Bug".into(),
        MilestonePolicy::NoMilestone,
    )
    .unwrap();
    let retried = gp::retry_draft_publication_impl(&s.handle(), s.id.clone()).unwrap();
    assert!(matches!(retried, PublicationOutcome::RecoveryRequired { ref mismatches, .. } if mismatches == &vec!["parent".to_string()]));
    assert_eq!(s.store().attempt.unwrap().choice, Some(saved.clone()));

    let answered = gp::resolve_draft_publication_conflict_impl(&s.handle(), s.id.clone(), RecoveryChoice::PublishNew).unwrap();
    let PublicationOutcome::Published { record } = answered else { panic!("published") };
    assert_ne!(record.marker, first.marker, "a new marker");
    assert_eq!(record.choice, Some(saved), "the same saved choice");
    assert_eq!(s.fake.issue(record.issue_number).parent.unwrap().number, parent);
}

/// GHP-FR-PWJG: the retry command against a parent that was closed returns the
/// typed error, keeps the attempt `open`, and creates nothing.
#[test]
fn the_retry_command_refuses_a_closed_parent() {
    let s = session();
    let parent = s.fake.seed("Feature", None);
    *s.fake.search_error.lock().unwrap() = Some(ERR_GITHUB_UNREACHABLE.into());
    assert!(s.publish(sub_of(parent)).is_err());
    *s.fake.search_error.lock().unwrap() = None;
    s.fake.edit(parent, |i| i.open = false);

    assert_eq!(
        gp::retry_draft_publication_impl(&s.handle(), s.id.clone()).map(|_| ()),
        Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string())
    );
    assert_eq!(s.store().attempt.unwrap().state, AttemptState::Open);
    assert!(!s.fake.calls().iter().any(|c| matches!(c, Call::Create(_))));
}

/// GHP-FR-ZFPI / GHP-FR-CDVT: a second application on the same project, which is
/// what a relaunch amounts to, retries the standing attempt with its choice.
#[test]
fn restart_recovery_retries_with_the_saved_choice() {
    let s = session();
    let parent = s.fake.seed("Feature", Some((7, "v1.2")));
    *s.fake.create_error.lock().unwrap() = Some(ERR_ISSUE_CREATE_FAILED.into());
    assert_eq!(s.publish(sub_of(parent)).map(|_| ()), Err(ERR_ISSUE_CREATE_FAILED.to_string()));
    *s.fake.create_error.lock().unwrap() = None;

    // A fresh application over the same files and the same GitHub.
    let relaunched = {
        let store = TempDir::new().unwrap();
        let root = std::fs::canonicalize(s.dir.path()).unwrap();
        let store_root = std::fs::canonicalize(store.path()).unwrap();
        let app = tauri::test::mock_app();
        app.manage(crate::fs::FsAccessState::default());
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::graduation::GraduationState::rooted_at(store_root.clone()));
        app.manage(GithubIssuesSeam(s.fake.clone()));
        app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
        app.manage(crate::github_tokens::GithubTokens::default());
        app.manage(gp::TestSecret("secret".into()));
        let handle = app.handle().clone();
        handle.state::<crate::fs::FsAccessState>().install_for_worktree_and(&root, &store_root).unwrap();
        let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
        let state = handle.state::<crate::project::ProjectState>();
        state.set_root_with_access(root.clone(), Some(access));
        state.set_anchor(root.to_string_lossy().into_owned());
        state.set_store(root);
        (app, store)
    };
    let handle = relaunched.0.handle().clone();
    let view = gp::get_draft_publication_impl(&handle, s.id.clone()).unwrap();
    assert_eq!(view.attempt.unwrap().choice.unwrap().parent_issue_number, Some(parent));

    let PublicationOutcome::Published { record } = gp::retry_draft_publication_impl(&handle, s.id.clone()).unwrap() else {
        panic!("published")
    };
    assert_eq!(s.fake.issue(record.issue_number).parent.unwrap().number, parent);
}

/// Unchanged root publication: with no choice made, the command publishes a root
/// issue with the title and body alone, and appends one record (GHP-FR-PLQE,
/// GHP-FR-JAWD).
#[test]
fn a_default_root_publication_is_unchanged() {
    let s = session();
    let PublicationOutcome::Published { record } = s.publish(PublicationChoiceInput::default()).unwrap() else {
        panic!("published")
    };
    assert_eq!(record.issue_number, 1);
    assert_eq!(s.fake.fields.lock().unwrap().as_slice(), &[client::IssueFields::default()]);
    assert_eq!(record.choice.unwrap().kind, PublicationKind::Root);
    assert!(!s.fake.calls().iter().any(|c| matches!(c, Call::Link(..) | Call::Read(_))));
}
