//! The refresh slots (PST-FR-36 / PST-FR-37).
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// The refresh slots (PST-FR-36 / PST-FR-37)
// ------------------------------------------------------------------

#[test]
fn several_ticks_during_one_long_refresh_coalesce_into_exactly_one() {
    // PST-FR-36, PST-FR-37: two ticks fall due inside one long refresh; exactly one
    // pending refresh is recorded, and it starts the moment the long one
    // settles.
    let mut slots = RefreshSlots::default();
    let first = slots.begin(DashboardWidget::PendingGit);
    assert_eq!(first, Some(1), "the first tick runs");
    assert_eq!(slots.begin(DashboardWidget::PendingGit), None, "tick 2 coalesces");
    assert_eq!(slots.begin(DashboardWidget::PendingGit), None, "tick 3 coalesces");

    let (fresh, next) = slots.settle(DashboardWidget::PendingGit, 1);
    assert!(fresh, "the only settled reading is the freshest");
    assert_eq!(next, Some(2), "exactly one pending refresh starts, at once");

    let (_, after) = slots.settle(DashboardWidget::PendingGit, 2);
    assert_eq!(after, None, "and nothing is left queued behind it");
}

#[test]
fn the_two_widgets_are_independent() {
    // PST-FR-37: a slow Pending Git read delays no agent-activity refresh.
    let mut slots = RefreshSlots::default();
    assert_eq!(slots.begin(DashboardWidget::PendingGit), Some(1));
    assert_eq!(
        slots.begin(DashboardWidget::AgentActivity),
        Some(1),
        "the other widget's slot is untouched"
    );
}

#[test]
fn a_result_overtaken_by_a_later_one_is_discarded() {
    // PST-FR-37: a refresh returning after a later one has already settled
    // is discarded — it emits nothing and leaves the widget's data as it is.
    let mut slots = RefreshSlots::default();
    assert_eq!(slots.begin(DashboardWidget::AgentActivity), Some(1));
    let (_, next) = slots.settle(DashboardWidget::AgentActivity, 1);
    assert_eq!(next, None);
    assert_eq!(slots.begin(DashboardWidget::AgentActivity), Some(2));
    let (fresh_second, _) = slots.settle(DashboardWidget::AgentActivity, 2);
    assert!(fresh_second);
    let (fresh_stale, _) = slots.settle(DashboardWidget::AgentActivity, 1);
    assert!(
        !fresh_stale,
        "a sequence already overtaken is never applied, however late it returns"
    );
}

#[test]
fn at_most_one_read_of_a_source_runs_at_a_time() {
    // PST-FR-36, PST-FR-37: the in-flight seat is handed straight to the coalesced
    // refresh, so no window exists in which two reads of one source run.
    let mut slots = RefreshSlots::default();
    assert_eq!(slots.begin(DashboardWidget::PendingGit), Some(1));
    assert_eq!(slots.begin(DashboardWidget::PendingGit), None);
    let (_, next) = slots.settle(DashboardWidget::PendingGit, 1);
    assert_eq!(next, Some(2));
    assert_eq!(
        slots.begin(DashboardWidget::PendingGit),
        None,
        "the slot is still in flight, so a tick behind the pending one coalesces too"
    );
}

#[test]
fn the_interval_is_exactly_five_minutes() {
    // PST-FR-36: exactly 5 minutes, and stated as a constant so a change to
    // it is a change to this line rather than a drift.
    assert_eq!(REFRESH_INTERVAL, Duration::from_secs(300));
}

#[test]
fn widget_names_match_the_contract_surface() {
    // PST-FR-37: the failure payload names one of exactly four widgets.
    assert_eq!(DashboardWidget::RecentlyEdited.as_str(), "recently_edited");
    assert_eq!(DashboardWidget::ActiveDrafts.as_str(), "active_drafts");
    assert_eq!(DashboardWidget::AgentActivity.as_str(), "agent_activity");
    assert_eq!(DashboardWidget::PendingGit.as_str(), "pending_git");
    assert_eq!(
        serde_json::to_value(DashboardWidget::PendingGit).unwrap(),
        serde_json::json!("pending_git"),
        "the wire spelling is the contract's"
    );
}

#[test]
fn pending_git_counts_are_absent_rather_than_zero_when_unavailable() {
    // PST-FR-34 / DSH-FR-14: unavailable is not zero.
    let none = PendingGitActivity::default();
    assert_eq!(none.modified_artifacts, None);
    assert_eq!(none.unpushed_commits, None);
    assert_eq!(
        serde_json::to_value(none).unwrap(),
        serde_json::json!({
            "modifiedArtifacts": null,
            "modifiedSourceFiles": null,
            "unpushedCommits": null,
            "fetchableCommits": null,
        }),
    );
}

/// PST-FR-31: an artifact the filesystem cannot describe is omitted, the
/// rest are returned in order, no error is raised, and each omission is
/// accounted for exactly once.
#[test]
fn an_artifact_the_filesystem_cannot_describe_is_omitted_and_accounted_for() {
    let nodes = vec![
        ("gone.spec.md".to_string(), "gone.spec.md".to_string(), scanning::ArtifactType::Spec),
        ("kept-a.spec.md".to_string(), "kept-a.spec.md".to_string(), scanning::ArtifactType::Spec),
        ("unstattable.spec.md".to_string(), "unstattable.spec.md".to_string(), scanning::ArtifactType::Spec),
        ("kept-b.spec.md".to_string(), "kept-b.spec.md".to_string(), scanning::ArtifactType::Spec),
    ];
    let (readings, omitted) = stat_artifacts(nodes, |id| match id {
        "kept-a.spec.md" => Some(at(1_000)),
        "kept-b.spec.md" => Some(at(2_000)),
        // The filesystem describes neither: one has been removed, one
        // cannot be statted. Both are the same answer to this loader.
        _ => None,
    });

    assert_eq!(
        omitted,
        vec!["gone.spec.md".to_string(), "unstattable.spec.md".to_string()],
        "each omission is named exactly once, which is what the log records"
    );
    let items = recently_edited_from(readings);
    assert_eq!(
        items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        vec!["kept-b.spec.md", "kept-a.spec.md"],
        "the readable artifacts are still returned, in modification-time order"
    );
}
