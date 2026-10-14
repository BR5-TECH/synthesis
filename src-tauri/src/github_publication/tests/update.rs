//! The update of a published draft's issue (GHP-FR-MZMW, GHP-FR-LVVD,
//! GHP-FR-ZRPL, GHP-FR-SVXP, GHP-FR-AXRU, GHP-FR-TCFV, GHP-FR-WUJQ,
//! GHP-FR-BCAV), run through the command body against the fake client.

use tauri::{Listener, Manager};

use super::publication_commands::{session, Session};
use super::*;
use crate::drafts::DraftStatus;
use crate::github_publication as gp;

fn update(s: &Session, update_closed: bool) -> Result<IssueUpdateOutcome, String> {
    gp::update::update_draft_on_github_impl(&s.handle(), s.id.clone(), update_closed)
}

/// A session whose draft is published once as a root issue. Returns the
/// issue number.
fn published(s: &Session) -> u64 {
    match s.publish(PublicationChoiceInput::default()).unwrap() {
        PublicationOutcome::Published { record } => record.issue_number,
        other => panic!("not published: {other:?}"),
    }
}

fn updated(outcome: Result<IssueUpdateOutcome, String>) -> PublicationRecord {
    match outcome.unwrap() {
        IssueUpdateOutcome::Updated { record } => *record,
        other => panic!("not updated: {other:?}"),
    }
}

/// The calls that edit or create on GitHub.
fn mutations(s: &Session) -> Vec<Call> {
    s.fake
        .calls()
        .into_iter()
        .filter(|c| {
            matches!(c, Call::Create(_) | Call::Update(_) | Call::Link(..) | Call::Unlink(..))
        })
        .collect()
}

/// GHP-FR-MZMW: a draft that holds no record is refused, and nothing is
/// requested.
#[test]
fn a_draft_with_no_record_is_refused() {
    let s = session();
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_NO_PUBLICATION_RECORD.to_string()));
    assert!(s.fake.calls().is_empty());
    assert_eq!(s.store(), PublicationStore::default());
}

/// GHP-FR-TCFV / GHP-FR-WUJQ / GHP-FR-UZMX / GHP-FR-BCAV: the edit names the
/// current name and prompt alone, and one record is appended that copies the
/// current one with an updated instant. The earlier record and the draft
/// status do not change.
#[test]
fn an_update_edits_title_and_body_and_appends_one_record() {
    let s = session();
    let number = published(&s);
    let before = s.store().publication[0].clone();
    crate::drafts::rename_draft_impl(&s.root(), &s.id, "Widget window v2").unwrap();
    save_prompt(&s.root(), &s.id, "Do the other thing.\n");
    s.fake.calls.lock().unwrap().clear();
    s.fake.fields.lock().unwrap().clear();

    let record = updated(update(&s, false));

    assert_eq!(s.fake.calls(), vec![Call::Read(number), Call::Update(number)]);
    assert_eq!(s.fake.fields.lock().unwrap().as_slice(), &[client::IssueFields::default()]);
    let issue = s.fake.issue(number);
    assert_eq!(issue.title, "Widget window v2");
    assert!(issue.body.starts_with("Do the other thing."));
    assert!(issue.body.ends_with(&flow::marker_line(&before.marker)));

    assert!(record.updated_at.is_some());
    assert_eq!(PublicationRecord { updated_at: None, ..record.clone() }, before);
    let store = s.store();
    assert_eq!(store.publication, vec![before, record]);
    assert!(store.attempt.is_none());
    let status = crate::drafts::draft_record(&s.root(), &s.id).unwrap().status;
    assert_eq!(status, DraftStatus::Published);
}

/// GHP-FR-MZMW / GHP-FR-QLDF: with two records, the update edits the issue of
/// the newest record and never the issue an earlier record names.
#[test]
fn an_update_edits_only_the_current_records_issue() {
    let s = session();
    let first = published(&s);
    let second = published(&s);
    assert_ne!(first, second);
    s.fake.calls.lock().unwrap().clear();

    let record = updated(update(&s, false));

    assert_eq!(record.issue_number, second);
    assert_eq!(mutations(&s), vec![Call::Update(second)]);
    assert_eq!(s.store().publication.len(), 3);
}

/// GHP-FR-SVXP / GHP-FR-BCAV: a deleted issue, a moved issue, a refused read,
/// and a read that cannot answer each write nothing anywhere.
#[test]
fn a_failed_read_writes_nothing() {
    let s = session();
    let number = published(&s);
    let before = s.store();

    s.fake.edit(number, |issue| {
        issue.url = format!("https://github.com/other/place/issues/{number}")
    });
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_ISSUE_MOVED.to_string()));

    *s.fake.read_error.lock().unwrap() = Some(ERR_ISSUES_CREATE_FORBIDDEN.into());
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_ISSUE_UPDATE_FORBIDDEN.to_string()));
    *s.fake.read_error.lock().unwrap() = Some(ERR_GITHUB_UNREACHABLE.into());
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_GITHUB_UNREACHABLE.to_string()));
    *s.fake.read_error.lock().unwrap() = None;

    s.fake.issues.lock().unwrap().clear();
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_ISSUE_NOT_FOUND.to_string()));

    assert!(!mutations(&s).iter().any(|c| matches!(c, Call::Update(_))));
    assert_eq!(s.store(), before);
}

/// GHP-FR-SVXP: an issue is the record's issue only under the record's
/// repository and number; an address of another shape leaves the number to
/// decide.
#[test]
fn the_moved_check_reads_the_issue_address() {
    let s = session();
    published(&s);
    let record = s.store().publication[0].clone();
    let issue = |url: &str, number: u64| client::IssueRef {
        number,
        url: url.to_string(),
        ..Default::default()
    };
    let n = record.issue_number;
    let names = |i: &client::IssueRef| gp::update::names_record_issue(i, &record);
    assert!(names(&issue(&format!("https://github.com/ACME/Widgets/issues/{n}"), n)));
    assert!(!names(&issue(&format!("https://github.com/acme/other/issues/{n}"), n)));
    assert!(!names(&issue(&format!("https://github.com/acme/widgets/issues/{n}"), n + 1)));
    assert!(names(&issue("", n)));
}

/// GHP-FR-AXRU: a closed issue answers `issue_closed` and writes nothing;
/// asked again with `update_closed`, the update edits it and leaves it closed.
#[test]
fn a_closed_issue_waits_for_the_author() {
    let s = session();
    let number = published(&s);
    s.fake.edit(number, |issue| issue.open = false);
    let before = s.store();

    let answer = update(&s, false).unwrap();
    assert_eq!(
        answer,
        IssueUpdateOutcome::IssueClosed {
            issue_number: number,
            issue_url: before.publication[0].issue_url.clone(),
        }
    );
    assert!(mutations(&s).iter().all(|c| !matches!(c, Call::Update(_))));
    assert_eq!(s.store(), before);

    updated(update(&s, true));
    assert!(!s.fake.issue(number).open);
    assert_eq!(s.store().publication.len(), 2);
}

/// GHP-FR-TCFV / GHP-FR-BCAV: an edit that GitHub refuses appends no record.
#[test]
fn a_refused_edit_appends_nothing() {
    let s = session();
    published(&s);
    let before = s.store();
    *s.fake.update_error.lock().unwrap() = Some(ERR_ISSUE_UPDATE_FAILED.into());
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_ISSUE_UPDATE_FAILED.to_string()));
    assert_eq!(s.store(), before);
}

/// GHP-FR-LVVD: a standing attempt, a local asset, and an archived draft are
/// refused before the first request, and no attempt record is written.
#[test]
fn the_publication_refusals_apply() {
    let s = session();
    published(&s);

    save_prompt(&s.root(), &s.id, "![local](./local/a.png)\n");
    s.fake.calls.lock().unwrap().clear();
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_LOCAL_ASSETS.to_string()));
    assert!(s.fake.calls().is_empty());
    save_prompt(&s.root(), &s.id, "Do the thing.\n");

    crate::drafts::set_draft_status_impl(&s.root(), &s.id, DraftStatus::Archived).unwrap();
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_DRAFT_ARCHIVED.to_string()));
    crate::drafts::set_draft_status_impl(&s.root(), &s.id, DraftStatus::Active).unwrap();

    *s.fake.create_error.lock().unwrap() = Some(ERR_ISSUE_CREATE_FAILED.into());
    assert!(s.publish(PublicationChoiceInput::default()).is_err());
    let standing = s.store();
    assert!(standing.attempt.is_some());
    s.fake.calls.lock().unwrap().clear();
    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_ATTEMPT_IN_PROGRESS.to_string()));
    assert!(s.fake.calls().is_empty());
    assert_eq!(s.store(), standing);
}

/// GHP-FR-ZRPL: a record on another host than the project token is refused
/// with `github_host_mismatch`, and nothing is requested.
#[test]
fn a_record_on_another_host_is_refused() {
    let s = session();
    published(&s);
    let mut moved = s.store();
    moved.publication[0].repository_host = "ghe.acme.example".into();
    store::write_store(&s.root(), &s.id, &moved).unwrap();
    s.fake.calls.lock().unwrap().clear();

    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_HOST_MISMATCH.to_string()));
    assert!(s.fake.calls().is_empty());
}

/// GHP-FR-WUJQ / GHP-FR-BCAV: the record of an update is refused with
/// `publication_store_write_failed` rather than appended when the current
/// record changed or an attempt started while the edit ran.
#[test]
fn the_append_checks_the_current_record() {
    let s = session();
    published(&s);
    let before = s.store().publication[0].clone();
    let stale = PublicationRecord { issue_number: 999, ..before.clone() };
    let record = PublicationRecord { updated_at: Some("2026-10-14T00:00:00Z".into()), ..before };
    assert_eq!(
        store::append_update(&s.root(), &s.id, &stale, record),
        Err(ERR_STORE_WRITE_FAILED.to_string())
    );
    assert_eq!(s.store().publication.len(), 1);

    let mut standing = s.store();
    let current = standing.publication[0].clone();
    standing.attempt = Some(PublicationAttempt {
        marker: "pub-new".into(),
        remote_name: "origin".into(),
        remote_url: "github.com/acme/widgets".into(),
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        state: AttemptState::Open,
        started_at: "2026-10-14T00:00:00Z".into(),
        updated_at: "2026-10-14T00:00:00Z".into(),
        choice: None,
    });
    store::write_store(&s.root(), &s.id, &standing).unwrap();
    let record =
        PublicationRecord { updated_at: Some("2026-10-14T00:00:00Z".into()), ..current.clone() };
    assert_eq!(
        store::append_update(&s.root(), &s.id, &current, record),
        Err(ERR_STORE_WRITE_FAILED.to_string())
    );
    assert_eq!(s.store(), standing);
}

/// GHP-FR-AXRU / GHP-FR-WUJQ: the wire shape of an update's answer.
#[test]
fn the_update_outcome_serializes_as_the_frontend_reads_it() {
    let closed = IssueUpdateOutcome::IssueClosed { issue_number: 7, issue_url: "u".into() };
    assert_eq!(
        serde_json::to_value(&closed).unwrap(),
        serde_json::json!({ "kind": "issueClosed", "issueNumber": 7, "issueUrl": "u" })
    );
}

/// GHP-FR-WUJQ: a successful update emits `"draft publication changed"`, and
/// its updated instant is RFC 3339 UTC.
#[test]
fn a_successful_update_announces_the_change() {
    let s = session();
    published(&s);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    s.handle().listen_any(gp::DRAFT_PUBLICATION_CHANGED, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });

    let record = updated(update(&s, false));

    let instant = record.updated_at.unwrap();
    // `YYYY-MM-DDTHH:MM:SSZ`, the form every other publication instant takes.
    let bytes = instant.as_bytes();
    assert_eq!(instant.len(), 20, "{instant}");
    assert_eq!((bytes[4], bytes[10], bytes[13], bytes[19]), (b'-', b'T', b':', b'Z'), "{instant}");
    assert!(seen.lock().unwrap().iter().any(|payload| payload.contains(&s.id)));
}

/// GHP-FR-BCAV: an edit GitHub accepted, followed by a store write that
/// fails, gives `publication_store_write_failed` and appends no record.
#[test]
fn an_accepted_edit_with_a_failed_store_write_is_reported() {
    let s = session();
    let number = published(&s);
    let root = s.root();
    let id = s.id.clone();
    *s.fake.after_update.lock().unwrap() = Some(Box::new(move || {
        let mut store = store::read_store(&root, &id).unwrap();
        store.attempt = Some(PublicationAttempt {
            marker: "pub-meanwhile".into(),
            remote_name: "origin".into(),
            remote_url: "github.com/acme/widgets".into(),
            repository_host: "github.com".into(),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            state: AttemptState::Open,
            started_at: "2026-10-14T00:00:00Z".into(),
            updated_at: "2026-10-14T00:00:00Z".into(),
            choice: None,
        });
        store::write_store(&root, &id, &store).unwrap();
    }));

    assert_eq!(update(&s, false).map(|_| ()), Err(ERR_STORE_WRITE_FAILED.to_string()));
    assert!(s.fake.calls().contains(&Call::Update(number)));
    assert_eq!(s.store().publication.len(), 1);
}

/// GHP-FR-LVVD: a GitHub-shadow draft is refused with `draft_github_shadow`
/// before any request.
#[test]
fn a_shadow_draft_is_refused() {
    let s = session();
    let link = crate::drafts::GithubIssueLink {
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: 4,
        issue_url: "https://github.com/acme/widgets/issues/4".into(),
        project_node_id: "PVT_1".into(),
        claim_state: crate::drafts::GithubClaimState::Claimed,
    };
    let id = crate::drafts::create_github_shadow_draft(&s.root(), "Task", "body", link)
        .unwrap()
        .draft
        .id;
    assert_eq!(
        gp::update::update_draft_on_github_impl(&s.handle(), id, false).map(|_| ()),
        Err(ERR_DRAFT_GITHUB_SHADOW.to_string())
    );
    assert!(s.fake.calls().is_empty());
}

/// GHP-FR-ZRPL: with no token for the project, the update is refused with
/// `token_unavailable` and nothing is requested.
#[test]
fn an_update_without_a_token_is_refused() {
    let s = session();
    published(&s);
    s.fake.calls.lock().unwrap().clear();

    // A second application on the same project, which resolves no token.
    let store_dir = TempDir::new().unwrap();
    let root = std::fs::canonicalize(s.dir.path()).unwrap();
    let store_root = std::fs::canonicalize(store_dir.path()).unwrap();
    let app = tauri::test::mock_app();
    app.manage(crate::fs::FsAccessState::default());
    app.manage(crate::project::ProjectState::default());
    app.manage(crate::graduation::GraduationState::rooted_at(store_root.clone()));
    app.manage(GithubIssuesSeam(s.fake.clone()));
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    app.manage(crate::github_tokens::GithubTokens::default());
    let handle = app.handle().clone();
    handle
        .state::<crate::fs::FsAccessState>()
        .install_for_worktree_and(&root, &store_root)
        .unwrap();
    let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
    let state = handle.state::<crate::project::ProjectState>();
    state.set_root_with_access(root.clone(), Some(access));
    state.set_anchor(root.to_string_lossy().into_owned());
    state.set_store(root);

    assert_eq!(
        gp::update::update_draft_on_github_impl(&handle, s.id.clone(), false).map(|_| ()),
        Err(ERR_TOKEN_UNAVAILABLE.to_string())
    );
    assert!(s.fake.calls().is_empty());
}
