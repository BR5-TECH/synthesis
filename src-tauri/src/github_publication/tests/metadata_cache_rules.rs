//! The pure rules of the metadata cache (GHP-FR-IQTK, GHP-FR-ROBR,
//! GHP-FR-AWEZ, GHP-FR-EYGR, GHP-FR-MSLY, GHP-FR-JIAU, GHP-FR-RVCL,
//! GHP-FR-JDMU).

use std::path::PathBuf;

use super::*;
use crate::github_publication::metadata_cache::*;

fn key(project: &str) -> CacheKey {
    CacheKey { project: project.to_string(), worktree: PathBuf::from(format!("/w/{project}")) }
}

fn parent(number: u64) -> PublicationParentIssue {
    PublicationParentIssue {
        number,
        title: format!("Parent {number}"),
        issue_type: "Feature".into(),
        url: format!("https://github.com/acme/widgets/issues/{number}"),
        milestone: None,
    }
}

fn lists(parents: &[u64], types: &[&str], milestones: &[u64]) -> CachedLists {
    CachedLists {
        remote_name: "origin".into(),
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        parents: MetadataList::loaded(parents.iter().map(|n| parent(*n)).collect()),
        issue_types: MetadataList::loaded(
            types.iter().map(|t| PublicationIssueType { name: t.to_string() }).collect(),
        ),
        milestones: MetadataList::loaded(
            milestones
                .iter()
                .map(|n| PublicationMilestone { number: *n, title: format!("v{n}") })
                .collect(),
        ),
        labels: MetadataList::loaded(vec![PublicationLabel {
            name: "bug".into(),
            color: "d73a4a".into(),
            description: None,
        }]),
    }
}

fn opened() -> CacheSlot<u32> {
    let mut slot = CacheSlot::default();
    slot.reset(Some(key("a")));
    slot
}

fn started(slot: &mut CacheSlot<u32>, trigger: RefreshTrigger) -> RefreshTicket {
    match slot.begin(trigger, None) {
        Some(RefreshStart::Started(ticket)) => ticket,
        other => panic!("expected a started refresh, got {other:?}"),
    }
}

/// Fill the cache with one successful refresh.
fn filled(fresh: CachedLists) -> CacheSlot<u32> {
    let mut slot = opened();
    let ticket = started(&mut slot, RefreshTrigger::Startup);
    slot.settle(&ticket, Ok(fresh)).unwrap();
    slot
}

// GHP-FR-EYGR: a loaded list replaces the cached list.
#[test]
fn a_loaded_list_replaces_the_cached_list() {
    let cached = lists(&[1, 2], &["Feature"], &[7]);
    let merged = merge(Some(&cached), lists(&[3], &["Task"], &[]));
    assert_eq!(merged, lists(&[3], &["Task"], &[]));
}

// GHP-FR-EYGR / GHP-FR-MSLY: a list that fails keeps the cached list, and
// does not change the other lists.
#[test]
fn a_failed_list_keeps_the_cached_list_and_the_others_still_replace() {
    let cached = lists(&[1, 2], &["Feature"], &[7]);
    let mut fresh = lists(&[3], &["Task"], &[8]);
    fresh.parents = MetadataList::failed(ERR_PARENT_ISSUES_UNREADABLE);
    let merged = merge(Some(&cached), fresh);
    assert_eq!(merged.parents, cached.parents);
    assert_eq!(merged.issue_types, lists(&[], &["Task"], &[]).issue_types);
    assert_eq!(merged.milestones, lists(&[], &[], &[8]).milestones);
}

// GHP-FR-EYGR: with no cached list, a failed list is stored as failed.
#[test]
fn a_failed_list_is_stored_as_failed_where_nothing_is_cached() {
    let mut fresh = lists(&[3], &["Task"], &[8]);
    fresh.milestones = MetadataList::failed(ERR_MILESTONES_UNREADABLE);
    let merged = merge(None, fresh.clone());
    assert_eq!(merged.milestones.state, MetadataState::Failed);
    assert_eq!(merged, fresh);
}

// GHP-FR-EYGR: a refresh for another repository replaces the whole cache,
// a failed list included.
#[test]
fn another_repository_replaces_the_whole_cache() {
    let cached = lists(&[1], &["Feature"], &[7]);
    let mut fresh = lists(&[9], &["Task"], &[]);
    fresh.repository_name = "gadgets".into();
    fresh.parents = MetadataList::failed(ERR_PARENT_ISSUES_UNREADABLE);
    assert_eq!(merge(Some(&cached), fresh.clone()), fresh);
}

// GHP-FR-AWEZ: at most one refresh runs; a second request joins it and is
// answered when it ends.
#[test]
fn a_second_request_joins_the_running_refresh() {
    let mut slot: CacheSlot<u32> = opened();
    let ticket = match slot.begin(RefreshTrigger::Startup, Some(1)) {
        Some(RefreshStart::Started(ticket)) => ticket,
        other => panic!("{other:?}"),
    };
    assert!(slot.is_running());
    assert_eq!(slot.begin(RefreshTrigger::Request, Some(2)), Some(RefreshStart::Joined));
    let (_, waiters) = slot.settle(&ticket, Ok(lists(&[1], &[], &[]))).unwrap();
    assert_eq!(waiters, vec![1, 2]);
    assert!(!slot.is_running());
    assert!(matches!(slot.begin(RefreshTrigger::Request, None), Some(RefreshStart::Started(_))));
}

// GHP-FR-AWEZ / GHP-FR-ROBR: with no project open, no refresh starts.
#[test]
fn no_refresh_starts_without_an_open_project() {
    let mut slot: CacheSlot<u32> = CacheSlot::default();
    assert_eq!(slot.begin(RefreshTrigger::Request, Some(1)), None);
}

// GHP-FR-IQTK / GHP-FR-JIAU: the first successful refresh fills the cache
// and reports a change.
#[test]
fn the_first_refresh_fills_the_cache_and_reports_a_change() {
    let mut slot = opened();
    let ticket = started(&mut slot, RefreshTrigger::Startup);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[1], &["Feature"], &[7]))).unwrap();
    assert!(settlement.changed);
    assert_eq!(settlement.outcome.kind, RefreshOutcomeKind::Changed);
    assert_eq!(slot.lists(), Some(&lists(&[1], &["Feature"], &[7])));
}

// GHP-FR-JIAU: a refresh that changes nothing reports no change.
#[test]
fn an_identical_refresh_reports_no_change() {
    let mut slot = filled(lists(&[1, 2], &["Feature"], &[7]));
    let ticket = started(&mut slot, RefreshTrigger::Periodic);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[1, 2], &["Feature"], &[7]))).unwrap();
    assert!(!settlement.changed);
    assert_eq!(settlement.outcome.kind, RefreshOutcomeKind::Unchanged);
}

// GHP-FR-JIAU: a new order of the items is a change.
#[test]
fn a_new_order_of_the_items_is_a_change() {
    let mut slot = filled(lists(&[1, 2], &["Feature"], &[7]));
    let ticket = started(&mut slot, RefreshTrigger::Periodic);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[2, 1], &["Feature"], &[7]))).unwrap();
    assert!(settlement.changed);
    assert_eq!(slot.lists().unwrap().parents.items[0].number, 2);
}

// GHP-FR-MSLY: a refresh that fails before the lists are read, or times out,
// keeps every cached list.
#[test]
fn a_failed_refresh_keeps_every_cached_list() {
    let before = lists(&[1], &["Feature"], &[7]);
    let mut slot = filled(before.clone());
    let ticket = started(&mut slot, RefreshTrigger::Periodic);
    let (settlement, _) = slot.settle(&ticket, Err(ERR_GITHUB_UNREACHABLE.into())).unwrap();
    assert!(!settlement.changed);
    assert_eq!(settlement.outcome, RefreshOutcome::failed(ERR_GITHUB_UNREACHABLE));
    assert_eq!(slot.lists(), Some(&before));
}

// GHP-FR-MSLY: one list that fails makes the refresh fail, while the other
// lists still update the cache.
#[test]
fn one_failed_list_fails_the_refresh_and_the_others_still_update() {
    let mut slot = filled(lists(&[1], &["Feature"], &[7]));
    let ticket = started(&mut slot, RefreshTrigger::Request);
    let mut fresh = lists(&[1], &["Feature"], &[7, 8]);
    fresh.issue_types = MetadataList::failed(ERR_ISSUE_TYPES_UNREADABLE);
    let (settlement, _) = slot.settle(&ticket, Ok(fresh)).unwrap();
    assert_eq!(settlement.outcome, RefreshOutcome::failed(ERR_ISSUE_TYPES_UNREADABLE));
    assert!(settlement.changed);
    let cached = slot.lists().unwrap();
    assert_eq!(cached.issue_types.items.len(), 1);
    assert_eq!(cached.milestones.items.len(), 2);
}

// GHP-FR-JDMU / GHP-FR-HGVL: with no GitHub Project selected, the cache
// empties, and the change is reported only where it held lists.
#[test]
fn no_selected_github_project_empties_the_cache() {
    let mut slot = filled(lists(&[1], &[], &[]));
    let ticket = started(&mut slot, RefreshTrigger::Invalidation);
    let (settlement, _) =
        slot.settle(&ticket, Err(ERR_GITHUB_PROJECT_NOT_SELECTED.into())).unwrap();
    assert!(settlement.changed);
    assert_eq!(slot.lists(), None);

    let ticket = started(&mut slot, RefreshTrigger::Invalidation);
    let (settlement, _) =
        slot.settle(&ticket, Err(ERR_GITHUB_PROJECT_NOT_SELECTED.into())).unwrap();
    assert!(!settlement.changed);
}

// GHP-FR-ROBR: a project switch empties the cache, and a refresh that started
// before it is discarded and changes nothing.
#[test]
fn a_project_switch_discards_the_running_refresh() {
    let mut slot = filled(lists(&[1], &[], &[]));
    let ticket = started(&mut slot, RefreshTrigger::Periodic);
    slot.begin(RefreshTrigger::Request, Some(5));
    let dropped = slot.reset(Some(key("b")));
    assert_eq!(dropped, vec![5]);
    assert_eq!(slot.lists(), None);
    assert_eq!(slot.settle(&ticket, Ok(lists(&[2], &[], &[]))), None);
    assert_eq!(slot.lists(), None);
    assert!(!slot.is_running());
}

// GHP-FR-ROBR: a project close empties the cache and accepts no refresh.
#[test]
fn a_project_close_empties_the_cache() {
    let mut slot = filled(lists(&[1], &[], &[]));
    slot.reset(None);
    assert_eq!(slot.lists(), None);
    assert_eq!(slot.key(), None);
    assert_eq!(slot.begin(RefreshTrigger::Periodic, None), None);
}

// GHP-FR-RVCL / GHP-FR-JDMU: each arm restarts the timer, so only the newest
// timer is live, and a reset stops every timer.
#[test]
fn only_the_newest_timer_is_live() {
    let mut slot = opened();
    let first = slot.arm_timer();
    let second = slot.arm_timer();
    assert!(!slot.is_current_timer(first));
    assert!(slot.is_current_timer(second));
    slot.reset(Some(key("a")));
    assert!(!slot.is_current_timer(second));
    let third = slot.arm_timer();
    slot.reset(None);
    assert!(!slot.is_current_timer(third));
}

// GHP-FR-MSLY: the typed error of the first failed list.
#[test]
fn the_first_failed_list_names_the_failure() {
    let mut fresh = lists(&[], &[], &[]);
    assert_eq!(fresh.first_failure(), None);
    fresh.milestones = MetadataList::failed(ERR_MILESTONES_UNREADABLE);
    fresh.parents = MetadataList::failed(ERR_PARENT_ISSUES_UNREADABLE);
    assert_eq!(fresh.first_failure().as_deref(), Some(ERR_PARENT_ISSUES_UNREADABLE));
}

// GHP-FR-JDMU: an invalidation while a refresh runs asks for one more refresh
// after it, and a request or a tick does not.
#[test]
fn an_invalidation_during_a_refresh_asks_for_one_more() {
    let mut slot = opened();
    let ticket = started(&mut slot, RefreshTrigger::Startup);
    slot.begin(RefreshTrigger::Request, None);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[1], &[], &[]))).unwrap();
    assert!(!settlement.rerun);

    let ticket = started(&mut slot, RefreshTrigger::Periodic);
    assert_eq!(slot.begin(RefreshTrigger::Invalidation, None), Some(RefreshStart::Joined));
    let (settlement, _) = slot.settle(&ticket, Err(ERR_GITHUB_UNREACHABLE.into())).unwrap();
    assert!(settlement.rerun);

    let ticket = started(&mut slot, RefreshTrigger::Invalidation);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[1], &[], &[]))).unwrap();
    assert!(!settlement.rerun);
}

// GHP-FR-ROBR / GHP-FR-JDMU: a reset drops the request for one more refresh.
#[test]
fn a_reset_drops_the_pending_rerun() {
    let mut slot = opened();
    started(&mut slot, RefreshTrigger::Startup);
    slot.begin(RefreshTrigger::Invalidation, None);
    slot.reset(Some(key("a")));
    let ticket = started(&mut slot, RefreshTrigger::Startup);
    let (settlement, _) = slot.settle(&ticket, Ok(lists(&[], &[], &[]))).unwrap();
    assert!(!settlement.rerun);
}
