//! The triggers and the schedule of the metadata cache: the invalidations,
//! the 30-minute timer, the refresh timeout, and the kept startup failure
//! (GHP-FR-JDMU, GHP-FR-RVCL, GHP-FR-MSLY, GHP-FR-HGVL, GHP-FR-ISJI,
//! GHP-FR-KPXF).
//!
//! The timer and the timeout run on paused Tokio time, so no test waits for
//! real time.

use std::time::Duration;

use tauri::Emitter;

use super::metadata_refresh::{
    begin, cached_session, record_events, refresh, select_github_project, wait_until, with_slot,
};
use super::*;
use crate::github_publication as gp;
use crate::github_publication::metadata_cache::*;
use crate::github_publication::metadata_refresh::{self as mr, REFRESH_PERIOD, REFRESH_TIMEOUT};

/// The refresh count, the last trigger, and the timer epoch.
fn starts(s: &super::publication_commands::Session) -> (u64, Option<RefreshTrigger>, u64) {
    mr::starts_for_test(&s.handle())
}

/// A session with the cache keyed to its project and no refresh started.
fn opened(github_project: bool) -> super::publication_commands::Session {
    let s = cached_session(github_project);
    mr::open_for_test(&s.handle());
    s
}

fn origin_selection() -> crate::project_settings::PublicationRemoteSelection {
    crate::project_settings::PublicationRemoteSelection {
        name: "origin".into(),
        // The normalized address that a publication saves (GHP-FR-HVQG).
        url: "github.com/acme/widgets".into(),
    }
}

fn set_parent_types(s: &super::publication_commands::Session, types: &[&str]) {
    gp::set_github_publication_settings_impl(
        &s.handle(),
        types.iter().map(|t| t.to_string()).collect(),
        "Task".into(),
        MilestonePolicy::InheritParent,
        None,
    )
    .unwrap();
}

// GHP-FR-JDMU: a changed list of parent issue Types starts one refresh and
// re-arms the timer. A save that keeps the list starts none and keeps the
// timer.
#[test]
fn a_parent_issue_types_change_starts_one_refresh_and_rearms_the_timer() {
    let s = opened(true);
    let (n0, _, e0) = starts(&s);
    set_parent_types(&s, &["Epic"]);
    let (n1, trigger, e1) = starts(&s);
    assert_eq!(n1, n0 + 1);
    assert_eq!(trigger, Some(RefreshTrigger::Invalidation));
    assert_eq!(e1, e0 + 1);

    wait_until(|| !with_slot(&s, |slot| slot.is_running()));
    set_parent_types(&s, &["Epic"]);
    assert_eq!(starts(&s), (n1, trigger, e1));
}

// GHP-FR-JDMU: a publication that persists a new remote selection starts one
// refresh and re-arms the timer.
#[test]
fn a_changed_saved_remote_starts_one_refresh_and_rearms_the_timer() {
    let s = opened(true);
    let (n0, _, e0) = starts(&s);
    gp::publish_draft_to_github_impl(
        &s.handle(),
        s.id.clone(),
        "origin".into(),
        true,
        PublicationChoiceInput::default(),
    )
    .unwrap();
    let (n1, trigger, e1) = starts(&s);
    assert_eq!(n1, n0 + 1);
    assert_eq!(trigger, Some(RefreshTrigger::Invalidation));
    assert_eq!(e1, e0 + 1);
}

// GHP-FR-JDMU: a publication that persists the remote already saved starts
// no refresh and keeps the timer.
#[test]
fn an_unchanged_saved_remote_starts_no_refresh() {
    let s = opened(true);
    crate::project_settings::save_publication_remote_selection_to(
        &s.root(),
        Some(origin_selection()),
    )
    .unwrap();
    let before = starts(&s);
    gp::publish_draft_to_github_impl(
        &s.handle(),
        s.id.clone(),
        "origin".into(),
        true,
        PublicationChoiceInput::default(),
    )
    .unwrap();
    assert_eq!(starts(&s), before);
}

// GHP-FR-JDMU: `"github tokens changed"` starts one refresh and re-arms the
// timer.
#[test]
fn a_token_change_starts_one_refresh_and_rearms_the_timer() {
    let s = opened(true);
    mr::listen_for_token_changes(&s.handle());
    let (n0, _, e0) = starts(&s);
    s.handle().emit(crate::github_tokens::GITHUB_TOKENS_CHANGED, ()).unwrap();
    wait_until(|| starts(&s).0 == n0 + 1);
    let (_, trigger, e1) = starts(&s);
    assert_eq!(trigger, Some(RefreshTrigger::Invalidation));
    assert_eq!(e1, e0 + 1);
}

// GHP-FR-HGVL / GHP-FR-JDMU: with no GitHub Project selected, an invalidation
// starts no refresh, empties the cache, and tells every window.
#[test]
fn an_invalidation_with_no_selected_github_project_empties_the_cache() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    select_github_project(&s, None);
    let changed = record_events(&s, mr::PUBLICATION_METADATA_CHANGED);
    let calls = s.fake.calls().len();
    let (n0, _, e0) = starts(&s);
    mr::invalidate(&s.handle(), "github_token");
    let (n1, _, e1) = starts(&s);
    assert_eq!(n1, n0);
    // One emptied cache and one armed idle timer.
    assert_eq!(e1, e0 + 2);
    assert!(!with_slot(&s, |slot| slot.is_running()));
    assert!(mr::cached_metadata_for(&s.handle(), "origin").is_none());
    assert_eq!(changed.lock().unwrap().len(), 1);
    assert_eq!(s.fake.calls().len(), calls);
}

// GHP-FR-ROBR / GHP-FR-JDMU: a worktree change with a GitHub Project selected
// empties the cache and starts one refresh.
#[test]
fn a_worktree_change_with_a_selected_github_project_starts_one_refresh() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let (n0, _, e0) = starts(&s);
    mr::on_worktree_changed(&s.handle());
    let (n1, trigger, e1) = starts(&s);
    assert_eq!(n1, n0 + 1);
    assert_eq!(trigger, Some(RefreshTrigger::Invalidation));
    // One reset and one armed timer.
    assert_eq!(e1, e0 + 2);
}

// GHP-FR-HGVL / GHP-FR-ROBR: a worktree change with no GitHub Project
// selected empties the cache, starts no refresh, and arms the idle timer.
#[test]
fn a_worktree_change_with_no_selected_github_project_starts_nothing() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    select_github_project(&s, None);
    let calls = s.fake.calls().len();
    let (n0, _, e0) = starts(&s);
    mr::on_worktree_changed(&s.handle());
    let (n1, _, e1) = starts(&s);
    assert_eq!(n1, n0);
    // One reset and one armed timer.
    assert_eq!(e1, e0 + 2);
    assert!(!with_slot(&s, |slot| slot.is_running()));
    assert!(mr::cached_metadata_for(&s.handle(), "origin").is_none());
    assert_eq!(s.fake.calls().len(), calls);
}

// GHP-FR-RVCL: the tick starts no refresh before 30 minutes, and starts one
// periodic refresh when the 30 minutes end.
#[tokio::test(start_paused = true)]
async fn the_periodic_tick_starts_a_refresh_after_thirty_minutes() {
    let s = opened(true);
    let epoch = with_slot(&s, |slot| slot.arm_timer());
    let (n0, _, _) = starts(&s);

    let early = REFRESH_PERIOD - Duration::from_secs(1);
    let cut = tokio::time::timeout(early, mr::periodic_tick(s.handle(), epoch)).await;
    assert!(cut.is_err());
    assert_eq!(starts(&s).0, n0);

    let t0 = tokio::time::Instant::now();
    mr::periodic_tick(s.handle(), epoch).await;
    assert!(t0.elapsed() >= REFRESH_PERIOD);
    let (n1, trigger, e1) = starts(&s);
    assert_eq!(n1, n0 + 1);
    assert_eq!(trigger, Some(RefreshTrigger::Periodic));
    assert!(e1 > epoch);
}

// GHP-FR-RVCL / GHP-FR-JDMU: a tick whose timer a later start or a reset
// re-armed starts no refresh.
#[tokio::test(start_paused = true)]
async fn a_tick_of_an_old_epoch_starts_nothing() {
    let s = opened(true);
    let old = with_slot(&s, |slot| slot.arm_timer());
    with_slot(&s, |slot| slot.arm_timer());
    let (n0, _, _) = starts(&s);
    mr::periodic_tick(s.handle(), old).await;
    assert_eq!(starts(&s).0, n0);

    let before_reset = with_slot(&s, |slot| slot.arm_timer());
    mr::open_for_test(&s.handle());
    mr::periodic_tick(s.handle(), before_reset).await;
    assert_eq!(starts(&s).0, n0);
}

// GHP-FR-RVCL / GHP-FR-AWEZ: a tick that falls due while a refresh runs
// starts no second refresh.
#[tokio::test(start_paused = true)]
async fn a_tick_while_a_refresh_runs_starts_nothing() {
    let s = opened(true);
    let ticket = begin(&s, RefreshTrigger::Request);
    let epoch = with_slot(&s, |slot| slot.arm_timer());
    let (n0, _, _) = starts(&s);
    mr::periodic_tick(s.handle(), epoch).await;
    assert_eq!(starts(&s).0, n0);
    assert_eq!(ticket.trigger, RefreshTrigger::Request);
}

// GHP-FR-RVCL / GHP-FR-HGVL: a tick with no GitHub Project selected starts no
// refresh and arms the timer again.
#[tokio::test(start_paused = true)]
async fn a_tick_with_no_selected_github_project_rearms_the_timer() {
    let s = opened(false);
    let epoch = with_slot(&s, |slot| slot.arm_timer());
    let (n0, _, _) = starts(&s);
    mr::periodic_tick(s.handle(), epoch).await;
    let (n1, _, e1) = starts(&s);
    assert_eq!(n1, n0);
    assert!(e1 > epoch);
    assert!(s.fake.calls().is_empty());
}

// GHP-FR-MSLY: a read that does not end in 120 seconds settles as failed, and
// every cached list stays as it was.
#[tokio::test(start_paused = true)]
async fn a_read_that_outlives_the_timeout_fails_and_keeps_the_cache() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let cached = mr::cached_metadata_for(&s.handle(), "origin").unwrap();
    let changed = record_events(&s, mr::PUBLICATION_METADATA_CHANGED);
    let ticket = begin(&s, RefreshTrigger::Periodic);

    let t0 = tokio::time::Instant::now();
    let never = std::future::pending::<Result<Result<CachedLists, String>, ()>>();
    let outcome = mr::settle_within_timeout(&s.handle(), &ticket, never).await;

    assert!(t0.elapsed() >= REFRESH_TIMEOUT);
    assert_eq!(outcome, Some(RefreshOutcome::failed(ERR_GITHUB_UNREACHABLE)));
    assert!(!with_slot(&s, |slot| slot.is_running()));
    assert_eq!(mr::cached_metadata_for(&s.handle(), "origin"), Some(cached));
    assert!(changed.lock().unwrap().is_empty());
}

// GHP-FR-MSLY: a read that ends just before the timeout settles normally.
#[tokio::test(start_paused = true)]
async fn a_read_that_ends_before_the_timeout_settles() {
    let s = cached_session(true);
    let ticket = begin(&s, RefreshTrigger::Periodic);
    let fresh = mr::read_fresh(&s.handle());
    let late = async move {
        tokio::time::sleep(REFRESH_TIMEOUT - Duration::from_secs(1)).await;
        Ok::<_, ()>(fresh)
    };
    let outcome = mr::settle_within_timeout(&s.handle(), &ticket, late).await.unwrap();
    assert_eq!(outcome.kind, RefreshOutcomeKind::Changed);
    assert!(mr::cached_metadata_for(&s.handle(), "origin").is_some());
}

// GHP-FR-MSLY: a read task that fails settles as failed and keeps the cache.
#[tokio::test(start_paused = true)]
async fn a_failed_read_task_keeps_the_cache() {
    let s = cached_session(true);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let cached = mr::cached_metadata_for(&s.handle(), "origin");
    let ticket = begin(&s, RefreshTrigger::Periodic);
    let failed = std::future::ready(Err::<Result<CachedLists, String>, ()>(()));
    let outcome = mr::settle_within_timeout(&s.handle(), &ticket, failed).await;
    assert_eq!(outcome, Some(RefreshOutcome::failed(ERR_GITHUB_UNREACHABLE)));
    assert_eq!(mr::cached_metadata_for(&s.handle(), "origin"), cached);
}

// GHP-FR-ISJI / GHP-FR-KPXF: a failed startup refresh keeps its failure, and
// the take answers it once.
#[test]
fn a_failed_startup_refresh_is_kept_until_it_is_taken() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    assert_eq!(mr::take_startup_failure(&s.handle()), None);
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let taken = mr::take_startup_failure(&s.handle()).unwrap();
    assert_eq!(taken.error_code, ERR_MILESTONES_UNREADABLE);
    assert_eq!(taken.error, mr::refresh_error_text(ERR_MILESTONES_UNREADABLE));
    assert_eq!(mr::take_startup_failure(&s.handle()), None);
}

// GHP-FR-KPXF: only the startup refresh keeps a failure.
#[test]
fn a_later_failed_refresh_keeps_nothing() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    refresh(&s, RefreshTrigger::Periodic).unwrap();
    refresh(&s, RefreshTrigger::Invalidation).unwrap();
    assert_eq!(mr::take_startup_failure(&s.handle()), None);
}

// GHP-FR-ISJI: a project close, a project open, and a worktree change discard
// a kept failure.
#[test]
fn a_project_or_worktree_change_discards_the_kept_failure() {
    type Change = fn(&tauri::AppHandle<tauri::test::MockRuntime>);
    let changes: [Change; 3] =
        [mr::on_project_closed, |app| mr::on_project_opened(app), mr::on_worktree_changed];
    for change in changes {
        let s = cached_session(false);
        *s.fake.probe.lock().unwrap() = ProbeOutcome::IssuesDisabled;
        select_github_project(&s, Some("PVT_1"));
        refresh(&s, RefreshTrigger::Startup).unwrap();
        select_github_project(&s, None);
        change(&s.handle());
        assert_eq!(mr::take_startup_failure(&s.handle()), None);
    }
}

// GHP-FR-KPXF: the failure is kept before the event, so a window that takes
// it when the event comes finds it.
#[test]
fn the_failure_is_kept_before_the_startup_event() {
    use tauri::Listener;
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    let taken = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = taken.clone();
    let app = s.handle();
    s.handle().listen_any(mr::PUBLICATION_METADATA_STARTUP_FAILED, move |_| {
        sink.lock().unwrap().push(mr::take_startup_failure(&app));
    });
    refresh(&s, RefreshTrigger::Startup).unwrap();
    let taken = taken.lock().unwrap().clone();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].as_ref().map(|f| f.error_code.as_str()), Some(ERR_MILESTONES_UNREADABLE));
}

// GHP-FR-ISJI / GHP-FR-KPXF: a project that opens with a GitHub Project
// selected runs the startup refresh in the background, and its failure is
// emitted once and kept.
#[test]
fn a_failed_startup_refresh_in_the_background_is_emitted_once_and_kept() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    mr::on_project_opened(&s.handle());
    wait_until(|| !failed.lock().unwrap().is_empty());
    wait_until(|| !with_slot(&s, |slot| slot.is_running()));
    assert_eq!(failed.lock().unwrap().len(), 1);
    let kept = mr::take_startup_failure(&s.handle()).unwrap();
    assert_eq!(kept.error_code, ERR_MILESTONES_UNREADABLE);
}

// GHP-FR-KPXF: a change that leaves no GitHub Project selected empties the
// cache but keeps the startup failure until it is taken.
#[test]
fn a_deselection_keeps_the_startup_failure() {
    let s = cached_session(true);
    *s.fake.milestones.lock().unwrap() = None;
    refresh(&s, RefreshTrigger::Startup).unwrap();
    select_github_project(&s, None);
    mr::invalidate(&s.handle(), "github_project");
    assert!(mr::take_startup_failure(&s.handle()).is_some());
}

// GHP-FR-HGVL / GHP-FR-ISJI: polling settings that cannot be read do not state
// that no GitHub Project is selected. The startup refresh runs and fails with
// `invalid_publication_settings`, so the author is told.
#[test]
fn unreadable_polling_settings_fail_the_startup_refresh() {
    let s = cached_session(false);
    std::fs::write(s.dir.path().join(".synthesis").join("project.toml"), "not = [valid").unwrap();
    let failed = record_events(&s, mr::PUBLICATION_METADATA_STARTUP_FAILED);
    let (n0, _, _) = starts(&s);
    mr::on_project_opened(&s.handle());
    assert_eq!(starts(&s).0, n0 + 1);
    wait_until(|| !failed.lock().unwrap().is_empty());
    let kept = mr::take_startup_failure(&s.handle()).unwrap();
    assert_eq!(kept.error_code, ERR_INVALID_PUBLICATION_SETTINGS);
}

// GHP-FR-SIMK / GHP-FR-HGVL: a request whose refresh a deselection discards
// answers `github_project_not_selected`, not `no_project_open`.
#[test]
fn a_request_discarded_by_a_deselection_answers_not_selected() {
    let s = opened(true);
    // A refresh that runs, so the request joins it and waits.
    let ticket = begin(&s, RefreshTrigger::Periodic);
    let handle = s.handle();
    let request = std::thread::spawn(move || {
        tauri::async_runtime::block_on(mr::refresh_and_wait(&handle)).unwrap()
    });
    wait_until(|| with_slot(&s, |slot| slot.waiter_count()) == 1);
    select_github_project(&s, None);
    mr::invalidate(&s.handle(), "github_project");
    let answer = request.join().unwrap();
    assert_eq!(answer.error_code.as_deref(), Some(ERR_GITHUB_PROJECT_NOT_SELECTED));
    assert_eq!(mr::finish(&s.handle(), &ticket, Err(ERR_GITHUB_UNREACHABLE.into())), None);
}
