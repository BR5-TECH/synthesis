//! The metadata cache against a project, a fake GitHub, and a mock app
//! (GHP-FR-HGVL, GHP-FR-AWEZ, GHP-FR-JIAU, GHP-FR-ISJI, GHP-FR-JDMU,
//! GHP-FR-JTYL, GHP-FR-SIMK, GHP-FR-PVOA).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Listener, Manager};

use super::publication_commands::{session, Session};
use super::*;
use crate::github_publication as gp;
use crate::github_publication::metadata_cache::*;
use crate::github_publication::metadata_refresh::{self as mr, PublicationMetadataCache};

/// A session whose app holds the cache, with the cache keyed to its project.
pub(super) fn cached_session(github_project: bool) -> Session {
    let s = session();
    s.app.manage(PublicationMetadataCache::default());
    if github_project {
        select_github_project(&s, Some("PVT_1"));
    }
    s
}

pub(super) fn select_github_project(s: &Session, node_id: Option<&str>) {
    let settings = crate::project_settings::GithubPollingSettings {
        project_node_id: node_id.map(str::to_string),
        ..Default::default()
    };
    crate::project_settings::save_github_polling_settings_to(&s.root(), &settings).unwrap();
}

/// Open the cache for the session's project without starting a refresh, and
/// start one by hand, so a test settles it synchronously.
pub(super) fn begin(s: &Session, trigger: RefreshTrigger) -> RefreshTicket {
    let key = begin_key(s);
    with_slot(s, |slot| {
        if slot.key() != key.as_ref() {
            drop(slot.reset(key.clone()));
        }
        match slot.begin(trigger, None) {
            Some(RefreshStart::Started(ticket)) => ticket,
            other => panic!("expected a started refresh, got {other:?}"),
        }
    })
}

pub(super) fn with_slot<T>(
    s: &Session,
    f: impl FnOnce(&mut CacheSlot<tokio::sync::oneshot::Sender<RefreshOutcome>>) -> T,
) -> T {
    mr::with_slot_for_test(&s.handle(), f)
}

pub(super) fn refresh(s: &Session, trigger: RefreshTrigger) -> Option<RefreshOutcome> {
    let ticket = begin(s, trigger);
    let result = mr::read_fresh(&s.handle());
    mr::finish(&s.handle(), &ticket, result)
}

pub(super) fn record_events(s: &Session, name: &'static str) -> Arc<Mutex<Vec<String>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    s.handle()
        .listen_any(name, move |event| sink.lock().unwrap().push(event.payload().to_string()));
    seen
}

/// Wait for a state, never for a fixed time: the deadline only stops a test
/// that would otherwise hang.
pub(super) fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "the expected state never came");
        std::thread::sleep(Duration::from_millis(5));
    }
}

// GHP-FR-HGVL: with no GitHub Project selected, the updater reads nothing.
#[test]
fn no_selected_github_project_reads_nothing() {
    let s = cached_session(false);
    let result = mr::read_fresh(&s.handle());
    assert_eq!(result, Err(ERR_GITHUB_PROJECT_NOT_SELECTED.to_string()));
    assert!(s.fake.calls().is_empty());
}

// GHP-FR-HGVL / GHP-FR-AWEZ: with a GitHub Project selected, the refresh
// reads the three lists of the resolved remote through the metadata read.
#[test]
fn a_refresh_reads_the_three_lists_of_the_resolved_remote() {
    let s = cached_session(true);
    s.fake.seed("Feature", None);
    let lists = mr::read_fresh(&s.handle()).unwrap();
    assert_eq!(lists.remote_name, "origin");
    assert_eq!(
        (lists.repository_owner.as_str(), lists.repository_name.as_str()),
        ("acme", "widgets")
    );
    assert_eq!(lists.parents.items.len(), 1);
    assert_eq!(lists.issue_types.items.len(), 3);
    assert_eq!(lists.milestones.items.len(), 2);
    let calls = s.fake.calls();
    for call in [Call::ListTypes, Call::ListParents, Call::ListMilestones] {
        assert!(calls.contains(&call), "{call:?} missing from {calls:?}");
    }
}

// GHP-FR-HGVL: the remote resolves only for an eligible token.
#[test]
fn a_remote_that_does_not_resolve_fails_the_refresh() {
    let s = cached_session(true);
    *s.fake.probe.lock().unwrap() = ProbeOutcome::IssuesDisabled;
    assert_eq!(mr::read_fresh(&s.handle()), Err(ERR_ISSUES_DISABLED.to_string()));
}

// GHP-FR-JIAU / GHP-FR-JTYL: a changing refresh emits the event once with the
// new cache, and the cache read answers the lists for that remote alone.
#[test]
fn a_changing_refresh_emits_the_new_cache_and_the_read_answers_it() {
    let s = cached_session(true);
    let changed = record_events(&s, mr::PUBLICATION_METADATA_CHANGED);
    s.fake.seed("Feature", Some((7, "v1.2")));

    let outcome = refresh(&s, RefreshTrigger::Request).unwrap();

    assert_eq!(outcome.kind, RefreshOutcomeKind::Changed);
    let events = changed.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert!(events[0].contains("\"remoteName\":\"origin\""), "{}", events[0]);
    let cached = mr::cached_metadata_for(&s.handle(), "origin").unwrap();
    assert_eq!(cached.parents.items.len(), 1);
    assert_eq!(cached.sub_issue_type.resolved.as_deref(), Some("Task"));
    assert_eq!(cached.settings.parent_issue_types, vec!["Feature".to_string()]);
    assert!(mr::cached_metadata_for(&s.handle(), "upstream").is_none());
}

// GHP-FR-JIAU: a refresh that changes nothing emits nothing.
#[test]
fn an_unchanged_refresh_emits_nothing() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let changed = record_events(&s, mr::PUBLICATION_METADATA_CHANGED);
    let outcome = refresh(&s, RefreshTrigger::Periodic).unwrap();
    assert_eq!(outcome.kind, RefreshOutcomeKind::Unchanged);
    assert!(changed.lock().unwrap().is_empty());
}

// GHP-FR-JTYL: the cache read reads the current settings, so a new root
// issue Type shows without a refresh, and it contacts no network.
#[test]
fn the_cache_read_uses_the_current_settings_without_a_request() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let mut settings =
        crate::project_settings::load_github_publication_settings_from(&s.root()).unwrap();
    settings.root_issue_type = Some("bug".into());
    crate::project_settings::save_github_publication_settings_to(&s.root(), &settings).unwrap();
    let before = s.fake.calls().len();
    let cached = mr::cached_metadata_for(&s.handle(), "origin").unwrap();
    assert_eq!(cached.root_issue_type.unwrap().resolved.as_deref(), Some("Bug"));
    assert_eq!(s.fake.calls().len(), before);
}

// GHP-FR-ISJI: a startup refresh that fails emits the startup event once,
// with the typed error and its displayable text.
#[test]
fn a_failed_startup_refresh_emits_the_startup_event() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    let outcome = refresh(&s, RefreshTrigger::Startup).unwrap();
    assert_eq!(outcome, RefreshOutcome::failed(ERR_MILESTONES_UNREADABLE));
    let events = failed.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert!(events[0].contains(ERR_MILESTONES_UNREADABLE), "{}", events[0]);
    assert!(events[0].contains("milestones of this repository"), "{}", events[0]);
}

// GHP-FR-ISJI: a missing remote with a GitHub Project selected is a startup
// failure too.
#[test]
fn a_startup_refresh_with_no_resolved_remote_emits_the_startup_event() {
    let s = cached_session(true);
    *s.fake.probe.lock().unwrap() = ProbeOutcome::IssuesDisabled;
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    assert_eq!(failed.lock().unwrap().len(), 1);
}

// GHP-FR-RVCL / GHP-FR-JDMU: only the startup refresh tells the main window
// of a failure; a periodic or invalidation failure is logged alone.
#[test]
fn a_later_failed_refresh_emits_no_startup_event() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    refresh(&s, RefreshTrigger::Periodic).unwrap();
    refresh(&s, RefreshTrigger::Invalidation).unwrap();
    refresh(&s, RefreshTrigger::Request).unwrap();
    assert!(failed.lock().unwrap().is_empty());
}

// GHP-FR-ISJI / GHP-FR-HGVL: a project that opens with no GitHub Project
// selected runs no refresh and emits nothing.
#[test]
fn opening_with_no_selected_github_project_runs_nothing() {
    let s = cached_session(false);
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    mr::on_project_opened(&s.handle());
    assert!(!with_slot(&s, |slot| slot.is_running()));
    assert!(s.fake.calls().is_empty());
    assert!(failed.lock().unwrap().is_empty());
}

// GHP-FR-ISJI / GHP-FR-AWEZ: a project that opens with a GitHub Project
// selected fills the cache in the background.
#[test]
fn opening_with_a_selected_github_project_fills_the_cache() {
    let s = cached_session(true);
    mr::on_project_opened(&s.handle());
    wait_until(|| mr::cached_metadata_for(&s.handle(), "origin").is_some());
}

// GHP-FR-SIMK: the refresh command answers with its outcome and the cache.
#[test]
fn the_refresh_command_answers_with_the_outcome_and_the_cache() {
    let s = cached_session(true);
    with_slot(&s, |slot| drop(slot.reset(begin_key(&s))));
    let answer = tauri::async_runtime::block_on(mr::refresh_and_wait(&s.handle())).unwrap();
    assert_eq!(answer.outcome, RefreshOutcomeKind::Changed);
    assert_eq!(answer.remote_name.as_deref(), Some("origin"));
    assert!(answer.metadata.is_some());
    assert_eq!(answer.error_code, None);
}

fn begin_key(s: &Session) -> Option<CacheKey> {
    let handle = s.handle();
    let project = handle.state::<crate::project::ProjectState>();
    Some(CacheKey {
        project: project.slot_key(),
        worktree: project.require_root().unwrap().path().to_path_buf(),
    })
}

// GHP-FR-SIMK / GHP-FR-PVOA: with no GitHub Project selected, the command
// answers `github_project_not_selected` and reads nothing.
#[test]
fn the_refresh_command_refuses_with_no_selected_github_project() {
    let s = cached_session(false);
    let answer = tauri::async_runtime::block_on(mr::refresh_and_wait(&s.handle())).unwrap();
    assert_eq!(answer.outcome, RefreshOutcomeKind::Failed);
    assert_eq!(answer.error_code.as_deref(), Some(ERR_GITHUB_PROJECT_NOT_SELECTED));
    assert_eq!(answer.error.as_deref(), Some("No GitHub Project is selected in Project settings."));
    assert!(s.fake.calls().is_empty());
}

// GHP-FR-JDMU: a change of the parent issue Types starts a refresh that reads
// the parent list again with the new Types.
#[test]
fn a_change_of_the_parent_issue_types_refreshes_the_cache() {
    let s = cached_session(true);
    with_slot(&s, |slot| drop(slot.reset(begin_key(&s))));
    gp::set_github_publication_settings_impl(
        &s.handle(),
        vec!["Epic".into()],
        "Task".into(),
        MilestonePolicy::InheritParent,
        None,
        Some("Feature".into()),
    )
    .unwrap();
    wait_until(|| {
        s.fake.parent_filters.lock().unwrap().iter().any(|f| f == &vec!["Epic".to_string()])
    });
    wait_until(|| mr::cached_metadata_for(&s.handle(), "origin").is_some());
}

// GHP-FR-JDMU: a save that keeps the parent issue Types starts no refresh.
#[test]
fn a_save_that_keeps_the_parent_issue_types_starts_no_refresh() {
    let s = cached_session(true);
    with_slot(&s, |slot| drop(slot.reset(begin_key(&s))));
    gp::set_github_publication_settings_impl(
        &s.handle(),
        vec!["Feature".into()],
        "Bug".into(),
        MilestonePolicy::NoMilestone,
        None,
        Some("Feature".into()),
    )
    .unwrap();
    assert!(!with_slot(&s, |slot| slot.is_running()));
    assert!(s.fake.calls().is_empty());
}

// GHP-FR-JDMU / GHP-FR-ROBR: a change that leaves no GitHub Project selected
// empties the cache and tells every window.
#[test]
fn deselecting_the_github_project_empties_the_cache() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let changed = record_events(&s, mr::PUBLICATION_METADATA_CHANGED);
    select_github_project(&s, None);
    refresh(&s, RefreshTrigger::Invalidation);
    assert!(mr::cached_metadata_for(&s.handle(), "origin").is_none());
    let events = changed.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert!(events[0].contains("\"metadata\":null"), "{}", events[0]);
}

// GHP-FR-ROBR: a project close empties the cache.
#[test]
fn a_project_close_empties_the_shared_cache() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    mr::on_project_closed(&s.handle());
    assert!(mr::cached_metadata_for(&s.handle(), "origin").is_none());
}

// GHP-FR-SIMK / GHP-FR-PVOA: every refresh failure has fixed displayable text.
#[test]
fn every_refresh_failure_has_displayable_text() {
    for code in [
        ERR_GITHUB_PROJECT_NOT_SELECTED,
        ERR_GITHUB_UNREACHABLE,
        ERR_PARENT_ISSUES_UNREADABLE,
        ERR_TOKEN_UNAVAILABLE,
        ERR_NO_REMOTE,
        ERR_INVALID_PUBLICATION_SETTINGS,
    ] {
        let text = mr::refresh_error_text(code);
        assert!(!text.is_empty() && text != "Publication is not available.", "{code}: {text}");
    }
}
