//! The polling commands driven through a mock application (GPP-FR-ELWS,
//! GPP-FR-DATH, GPP-FR-XZTP, GPP-FR-RGNM, GPP-FR-PUXT, GPP-FR-IURX,
//! GPP-FR-HZDD, GPP-FR-WYRP, GPP-FR-TYOV, GPP-FR-UBDE, GRD-FR-JGEC,
//! DRS-FR-EZDB).

use tauri::Listener;

use super::super::{begin_poll, fetch_poll, finish_poll, list_projects_impl, poll_impl, set_settings_impl};
use super::*;

fn begun(h: &Harness) -> super::super::Begun<Mock> {
    match begin_poll(&h.handle()).expect("a start") {
        Ok(begun) => begun,
        Err(_) => panic!("a poll was already running"),
    }
}

fn numbers(view: &GithubPollingView) -> Vec<u64> {
    view.tasks.iter().map(|t| t.issue_number).collect()
}

/// GPP-FR-ELWS / GPP-FR-DATH: a poll that settles after the project switched
/// is discarded, and it never resets the session back to the old project.
#[test]
fn a_poll_settling_after_a_project_switch_leaves_the_new_session_alone() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    *h.fake.items.lock().unwrap() = Ok(vec![ready(1)]);
    let old_key = SessionKey {
        project: canonical(h.dir.path()).to_string_lossy().into_owned(),
        worktree: canonical(h.dir.path()),
    };
    let started = begun(&h);
    let outcome = fetch_poll(&h.handle(), &started);

    let other = project();
    git2::Repository::init(other.path()).unwrap();
    h.open(other.path());
    let new_key = SessionKey {
        project: canonical(other.path()).to_string_lossy().into_owned(),
        worktree: canonical(other.path()),
    };
    h.view();

    let view = finish_poll(&h.handle(), started, outcome).expect("the live view");
    assert!(view.tasks.is_empty(), "the old project's rows did not land");
    let handle = h.handle();
    let state = handle.state::<super::super::GithubPollingState>();
    let slot = state.slot();
    assert!(slot.session_for(&new_key).is_some(), "the slot still holds the new session");
    assert!(slot.session_for(&old_key).is_none());
}

/// GPP-FR-XZTP: a poll that panics no longer counts as in flight.
#[test]
fn a_panicked_poll_clears_its_in_flight_mark() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    let handle = h.handle();
    let joined = std::thread::spawn(move || {
        let _started = match begin_poll(&handle).unwrap() {
            Ok(begun) => begun,
            Err(_) => unreachable!(),
        };
        panic!("the poll failed hard");
    })
    .join();
    assert!(joined.is_err());
    assert!(!h.view().polling);
    assert!(matches!(begin_poll(&h.handle()).unwrap(), Ok(_)), "a new poll starts");
}

/// GPP-FR-ELWS: a settings write that races a poll discards its result.
#[test]
fn a_settings_write_during_a_poll_discards_its_result() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    *h.fake.items.lock().unwrap() = Ok(vec![ready(1)]);
    let started = begun(&h);
    h.select(Some(PROJECT));
    let outcome = fetch_poll(&h.handle(), &started);
    let view = finish_poll(&h.handle(), started, outcome).unwrap();
    assert!(view.tasks.is_empty());
    assert!(!view.polling);
}

/// GPP-FR-XZTP: a second poll while one runs returns `polling: true` and
/// makes no second items read.
#[test]
fn a_second_poll_while_one_runs_reads_nothing() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    *h.fake.items.lock().unwrap() = Ok(vec![ready(1)]);
    let started = begun(&h);
    let second = poll_impl(&h.handle()).unwrap();
    assert!(second.polling);
    assert_eq!(h.calls_matching(|c| matches!(c, Call::Items(_))), 0);
    let outcome = fetch_poll(&h.handle(), &started);
    let view = finish_poll(&h.handle(), started, outcome).unwrap();
    assert_eq!(numbers(&view), vec![1]);
    assert_eq!(h.calls_matching(|c| matches!(c, Call::Items(_))), 1);
}

/// GPP-FR-RGNM / GPP-FR-PUXT: where the repository resolves no GitHub remote,
/// the poll fails with that refusal, the rows are kept, and they are stale.
#[test]
fn no_github_remote_fails_the_poll_and_keeps_the_rows() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    *h.fake.items.lock().unwrap() = Ok(vec![ready(1), ready(2)]);
    let first = poll_impl(&h.handle()).unwrap();
    assert_eq!(numbers(&first), vec![1, 2]);
    assert_eq!(first.repository, Some(repo()));

    let repo = git2::Repository::open(h.dir.path()).unwrap();
    repo.remote_set_url("origin", "https://git.internal/acme/widgets.git").unwrap();
    let view = poll_impl(&h.handle()).unwrap();
    assert_eq!(view.last_error_code.as_deref(), Some("no_github_remote"));
    assert!(view.last_error.is_some());
    assert!(view.stale);
    assert_eq!(numbers(&view), vec![1, 2]);
}

/// GPP-FR-IURX / GPP-FR-HZDD: a bad interval writes nothing; a selected
/// Project is validated at once; `null` is unset; an unreachable GitHub
/// leaves it unchecked.
#[test]
fn a_settings_write_validates_and_records_the_configuration() {
    let h = Harness::new();
    let valid = h.select(Some(PROJECT));
    assert_eq!(valid.configuration.state, ConfigurationState::Valid);
    assert_eq!(valid.configuration.project_title.as_deref(), Some("Roadmap"));
    assert_eq!(valid.settings, settings(Some(PROJECT)));

    let path = h.dir.path().join(".synthesis/project.toml");
    let before = std::fs::read(&path).unwrap();
    let refused = set_settings_impl(&h.handle(), Some("PVT_other".into()), Some(7));
    assert_eq!(refused.unwrap_err(), ERR_INVALID_INTERVAL);
    assert_eq!(std::fs::read(&path).unwrap(), before, "project.toml is byte-identical");

    let mut no_field = valid_shape();
    no_field.status_field = None;
    *h.fake.shape.lock().unwrap() = Ok(no_field);
    let invalid = h.select(Some(PROJECT));
    assert_eq!(invalid.configuration.state, ConfigurationState::Invalid);
    assert_eq!(invalid.configuration.error_code.as_deref(), Some(ERR_STATUS_FIELD_MISSING));

    assert_eq!(h.select(None).configuration.state, ConfigurationState::Unset);

    *h.fake.shape.lock().unwrap() = Err(ERR_GITHUB_UNREACHABLE.into());
    assert_eq!(h.select(Some(PROJECT)).configuration.state, ConfigurationState::Unchecked);
}

fn option(id: &str) -> GithubProjectOption {
    GithubProjectOption { node_id: id.into(), title: id.into(), owner_login: "acme".into(), number: 1 }
}

/// GPP-FR-WYRP: the listing reads the viewer's and the repository owner's
/// Projects without duplicates; with no GitHub remote it reads the viewer's
/// alone.
#[test]
fn the_listing_reads_viewer_and_owner_projects() {
    let h = Harness::new();
    *h.fake.viewer.lock().unwrap() = vec![option("P1"), option("P2")];
    *h.fake.owned.lock().unwrap() = vec![option("P1"), option("P3")];
    let listed = list_projects_impl(&h.handle()).unwrap();
    assert_eq!(listed.iter().map(|p| p.node_id.as_str()).collect::<Vec<_>>(), vec!["P1", "P2", "P3"]);
    assert_eq!(h.fake.calls(), vec![Call::Viewer, Call::Owner("acme".into())]);

    git2::Repository::open(h.dir.path()).unwrap().remote_delete("origin").unwrap();
    h.fake.calls.lock().unwrap().clear();
    let viewer_only = list_projects_impl(&h.handle()).unwrap();
    assert_eq!(viewer_only.len(), 2);
    assert_eq!(h.fake.calls(), vec![Call::Viewer]);
}

/// GPP-FR-TYOV: an event follows every change of the view, and only a
/// successful poll's event names new issues.
#[test]
fn an_event_follows_every_change_of_the_view() {
    let h = Harness::new();
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
    let sink = seen.clone();
    h.app.listen(GITHUB_POLLING_CHANGED, move |event| {
        sink.lock().unwrap().push(serde_json::from_str(event.payload()).unwrap());
    });
    let take = || std::mem::take(&mut *seen.lock().unwrap());

    h.select(Some(PROJECT));
    assert_eq!(take(), vec![serde_json::json!({ "newIssues": [] })], "settings");

    *h.fake.items.lock().unwrap() = Ok(vec![ready(4)]);
    poll_impl(&h.handle()).unwrap();
    let poll = take();
    assert_eq!(poll.last().unwrap(), &serde_json::json!({ "newIssues": [{ "issueNumber": 4, "title": "Task 4" }] }));

    *h.fake.items.lock().unwrap() = Err(ERR_REQUEST_FAILED.into());
    poll_impl(&h.handle()).unwrap();
    assert!(take().iter().all(|e| e["newIssues"] == serde_json::json!([])), "a failure names nothing");

    h.fake.hold_issue(4, "Ready", Some("Task"), "OPEN");
    super::super::claim_impl(&h.handle(), 4).unwrap();
    assert_eq!(take().len(), 1, "claim");
    super::super::retry_impl(&h.handle(), 4).unwrap();
    assert_eq!(take().len(), 1, "retry");
    super::super::acknowledge_impl(&h.handle(), 4).unwrap();
    assert_eq!(take().len(), 1, "acknowledgement");
}

/// GRD-FR-JGEC / DRS-FR-EZDB / GPP-FR-UBDE: a shadow draft with a committed
/// run reports `graduated` through the drafts listing and in the view's
/// shadow rows; once the run is discarded it reports `github_shadow`.
#[test]
fn a_committed_run_makes_the_shadow_row_graduated() {
    let h = Harness::new();
    let root = h.root();
    let draft = crate::drafts::create_github_shadow_draft(&root, "Task 1", "b", link(1)).unwrap().draft.id;
    let handle = h.handle();
    let mut run = crate::graduation::GraduationRun::new_for_test("g1", &draft, "2026-10-02T09:00:00Z");
    run.project_key = handle.state::<crate::project::ProjectState>().slot_key();
    run.commits = vec!["abc123".into()];
    run.state = crate::graduation::GraduationRunState::Completed;
    crate::graduation::save_run(&handle, &mut run).unwrap();

    let mut listed = crate::drafts::list_drafts_impl(&root).drafts;
    crate::drafts::attach_graduation(&handle, &mut listed);
    assert_eq!(listed[0].status, crate::drafts::DraftStatus::Graduated);
    let view = h.view();
    assert_eq!(view.shadows.len(), 1);
    assert_eq!(view.shadows[0].status, crate::drafts::DraftStatus::Graduated);
    assert!(!view.shadows[0].locked);

    run.state = crate::graduation::GraduationRunState::Discarded;
    crate::graduation::save_run(&handle, &mut run).unwrap();
    assert_eq!(h.view().shadows[0].status, crate::drafts::DraftStatus::GithubShadow);
}

/// GPP-FR-JOKP / PSS-FR-VCZM: the publication settings change no polling
/// setting, configuration state, or row, and a polling write keeps the
/// publication settings.
#[test]
fn the_publication_settings_leave_polling_unchanged() {
    use crate::project_settings::{
        load_github_publication_settings_from, save_github_publication_settings_to,
        GithubPublicationSettings, MilestonePolicy,
    };
    let h = Harness::new();
    *h.fake.items.lock().unwrap() = Ok(vec![ready(1)]);
    h.select(Some(PROJECT));
    let polled = poll_impl(&h.handle()).unwrap();
    let before = h.view();

    let saved = GithubPublicationSettings {
        parent_issue_types: vec!["Epic".into()],
        sub_issue_type: "Bug".into(),
        sub_issue_milestone_policy: MilestonePolicy::NoMilestone,
    };
    save_github_publication_settings_to(&h.root(), &saved).unwrap();

    let after = h.view();
    assert_eq!(after.settings, before.settings);
    assert_eq!(after.configuration, before.configuration);
    assert_eq!(numbers(&after), numbers(&polled));

    let reselected = h.select(Some(PROJECT));
    assert_eq!(reselected.settings, settings(Some(PROJECT)));
    assert_eq!(load_github_publication_settings_from(&h.root()).unwrap(), saved);
}
