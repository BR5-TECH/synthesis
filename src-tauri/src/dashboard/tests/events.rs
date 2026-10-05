//! A settled refresh result: the event it emits, and the results that are
//! discarded instead.
//!
//! One part of `../tests/mod.rs`.

use super::*;

fn root(key: &str) -> RootIdentity {
    RootIdentity {
        project_key: key.to_string(),
        worktree: key.to_string(),
    }
}

#[test]
fn a_fresh_result_for_the_open_root_emits_its_widgets_refresh_event() {
    // PST-FR-37: applied, and its event emitted, when the sequence is the
    // newest for the widget and the identity is still the open one.
    assert_eq!(
        settlement(true, &root("/p/a"), &root("/p/a"), Ok(())),
        Settlement::Changed,
    );
}

#[test]
fn a_failed_refresh_reaches_the_user_rather_than_only_the_log() {
    // PST-FR-37, DSH-FR-18: the failure carries the typed error the loader returned,
    // and the widget's ordinary refresh event is NOT emitted for it.
    assert_eq!(
        settlement(true, &root("/p/a"), &root("/p/a"), Err("git is unwell".into())),
        Settlement::Failed("git is unwell".to_string()),
    );
}

#[test]
fn an_overtaken_result_is_discarded_whatever_it_carries() {
    // PST-FR-37: a slow refresh returning after a fresher one emits neither
    // its refresh event nor a failure event.
    assert_eq!(
        settlement(false, &root("/p/a"), &root("/p/a"), Ok(())),
        Settlement::Discarded,
    );
    assert_eq!(
        settlement(false, &root("/p/a"), &root("/p/a"), Err("boom".into())),
        Settlement::Discarded,
        "a stale FAILURE is discarded too — it describes a reading nobody wants"
    );
}

#[test]
fn a_result_for_a_root_no_longer_open_is_discarded() {
    // PST-FR-37: a refresh dispatched for project A that returns after the
    // user has switched to B is discarded rather than reported as B's data,
    // and the same holds for a change of worktree inside one project.
    assert_eq!(
        settlement(true, &root("/p/b"), &root("/p/a"), Ok(())),
        Settlement::Discarded,
        "a project switch"
    );
    let worktree_b = RootIdentity {
        project_key: "/p/a".into(),
        worktree: "/p/a-wt".into(),
    };
    assert_eq!(
        settlement(true, &worktree_b, &root("/p/a"), Ok(())),
        Settlement::Discarded,
        "a worktree change is an identity change, not only a project change"
    );
}

#[test]
fn a_result_that_settles_with_no_project_open_emits_nothing() {
    // PST-FR-36: a closed project leaves nothing for an event to describe,
    // even for the refresh that was dispatched while it was open.
    assert_eq!(
        settlement(true, &RootIdentity::default(), &root("/p/a"), Ok(())),
        Settlement::Discarded,
    );
    assert!(!RootIdentity::default().is_open());
    assert!(root("/p/a").is_open());
}

#[test]
fn only_the_two_timer_driven_widgets_are_dispatchable() {
    // PST-FR-35 / PST-FR-36: the filesystem-driven pair is refreshed by the
    // watchers that observe the disk. A timer tick reads neither, so
    // dispatching one would emit a refresh event for a read that never
    // happened.
    assert!(DashboardWidget::AgentActivity.is_timer_driven());
    assert!(DashboardWidget::PendingGit.is_timer_driven());
    assert!(!DashboardWidget::RecentlyEdited.is_timer_driven());
    assert!(!DashboardWidget::ActiveDrafts.is_timer_driven());
}
