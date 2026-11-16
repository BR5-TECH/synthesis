//! The tests of work items (`FDL-flow-delivery.md` FDL-FR-KWMT): a leaf, or a
//! container that holds no item, and how they feed the state and the counts
//! of the containers above them. Also the closed-container rule of
//! FDL-FR-YIAZ and the Blocked status of FDL-FR-SBLQ.

use serde_json::json;

use super::tests::{container, doc, draft, skeleton, state_of, task};
use super::*;

fn closed(reason: Option<CloseReason>) -> ItemTrackerFacts {
    ItemTrackerFacts { state: TrackerState::Closed, close_reason: reason, ..Default::default() }
}

fn on_status(in_progress: bool, blocked: bool) -> ItemTrackerFacts {
    ItemTrackerFacts {
        status: Some("x".into()),
        status_is_in_progress: in_progress,
        status_is_blocked: blocked,
        ..Default::default()
    }
}

fn facts(entries: Vec<(&str, ItemTrackerFacts)>) -> TrackerFacts {
    entries.into_iter().map(|(id, f)| (id.to_string(), f)).collect()
}

/// FDL-FR-KWMT, FDL-FR-JYKC, FDL-FR-IJXW: a feature that holds no item is a
/// work item. It counts in the milestone above it, and a milestone with a
/// done task and an open empty feature is not done.
#[test]
fn an_empty_feature_is_a_work_item_of_its_milestone() {
    let d = doc(skeleton(vec![task("t1", "f1", None)]), json!([]));
    let tracker = facts(vec![("t1", closed(None))]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    let f2 = state_of(&s, "f2");
    assert_eq!(f2.state, ItemState::NotStarted);
    assert_eq!(f2.leaf_counts, Some(LeafCounts::default()), "it counts nothing under it");
    let m1 = state_of(&s, "m1");
    assert_eq!(m1.state, ItemState::InProgress);
    let counts = m1.leaf_counts.clone().unwrap();
    assert_eq!((counts.done, counts.not_started), (1, 1));
}

/// FDL-FR-KWMT: an epic that holds only an empty feature is not a work item;
/// the feature is. The milestone counts one work item, not two.
#[test]
fn a_container_that_holds_an_item_is_not_a_work_item() {
    let d = doc(
        vec![
            container("m1", "milestone", None),
            container("e1", "epic", Some("m1")),
            container("f1", "feature", Some("e1")),
        ],
        json!([]),
    );
    let s = delivery_state(&d, &[]);
    let counts = state_of(&s, "m1").leaf_counts.clone().unwrap();
    assert_eq!(counts.not_started, 1);
    assert_eq!(state_of(&s, "e1").leaf_counts.clone().unwrap().not_started, 1);
}

/// FDL-FR-KWMT, FDL-FR-YIAZ, FDL-FR-WPSZ: an empty container takes the state
/// of a leaf with no draft: closed is done or dropped by its close reason, and
/// open on the In Progress option is in progress.
#[test]
fn an_empty_container_takes_the_leaf_rules() {
    let d = doc(
        vec![
            container("m1", "milestone", None),
            container("f1", "feature", Some("m1")),
            container("f2", "feature", Some("m1")),
            container("f3", "feature", Some("m1")),
        ],
        json!([]),
    );
    let tracker = facts(vec![
        ("f1", closed(None)),
        ("f2", closed(Some(CloseReason::NotPlanned))),
        ("f3", on_status(true, false)),
    ]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    assert_eq!(state_of(&s, "f1").state, ItemState::Done);
    assert_eq!(state_of(&s, "f2").state, ItemState::Dropped);
    assert_eq!(state_of(&s, "f3").state, ItemState::InProgress);
    let counts = state_of(&s, "m1").leaf_counts.clone().unwrap();
    assert_eq!((counts.done, counts.dropped, counts.in_progress), (1, 1, 1));
    assert_eq!(state_of(&s, "m1").state, ItemState::InProgress);
}

/// FDL-FR-YIAZ, FDL-FR-JYKC: a closed milestone with unfinished work takes
/// its state from the work. Closed with every work item finished, it takes
/// its close reason, and closed with no item, it is done.
#[test]
fn a_closed_container_with_unfinished_work_takes_the_state_of_its_work() {
    let d = doc(skeleton(vec![task("t1", "f1", Some("a")), task("t2", "f2", None)]), json!([]));
    let drafts = vec![draft("a", DraftStatus::Graduated, None)];
    let open_work = facts(vec![("m1", closed(None))]);
    let s = delivery_state_with_tracker(&d, &open_work, &drafts);
    assert_eq!(state_of(&s, "m1").state, ItemState::InProgress);

    let finished = facts(vec![("m1", closed(Some(CloseReason::NotPlanned))), ("t2", closed(None))]);
    let s = delivery_state_with_tracker(&d, &finished, &drafts);
    assert_eq!(state_of(&s, "m1").state, ItemState::Dropped, "the close reason decides");

    let empty = doc(vec![container("m9", "milestone", None)], json!([]));
    let s = delivery_state_with_tracker(&empty, &facts(vec![("m9", closed(None))]), &[]);
    assert_eq!(state_of(&s, "m9").state, ItemState::Done);
}

/// FDL-FR-SBLQ: an open item on the Blocked option is blocked and lists
/// nothing in `blocked_by`. A closed item on that option and an open item on
/// another option are not blocked.
#[test]
fn an_open_item_on_the_blocked_status_is_blocked() {
    let d = doc(
        skeleton(vec![task("t1", "f1", None), task("t2", "f1", None), task("t3", "f1", None)]),
        json!([]),
    );
    let mut closed_blocked = closed(None);
    closed_blocked.status_is_blocked = true;
    let tracker = facts(vec![
        ("t1", on_status(false, true)),
        ("t2", closed_blocked),
        ("t3", on_status(false, false)),
        ("f2", on_status(false, true)),
    ]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    let t1 = state_of(&s, "t1");
    assert!(t1.blocked);
    assert!(t1.blocked_by.is_empty());
    assert!(!state_of(&s, "t2").blocked, "a closed item is finished");
    assert!(!state_of(&s, "t3").blocked);
    assert!(state_of(&s, "f2").blocked, "an empty feature on the option");
    assert!(!state_of(&s, "f1").blocked, "the Status of a leaf blocks no container");
}

/// FDL-FR-SBLQ, FDL-FR-YIAZ: an open container that holds items is blocked
/// by its own Status. A closed container whose work is unfinished takes the
/// state of its work, but it is not open, so its Status blocks nothing.
#[test]
fn the_blocked_status_of_a_container_needs_it_open() {
    let d = doc(skeleton(vec![task("t1", "f1", None), task("t2", "f2", None)]), json!([]));
    let mut closed_on_blocked = closed(None);
    closed_on_blocked.status_is_blocked = true;
    let tracker = facts(vec![("f1", on_status(false, true)), ("f2", closed_on_blocked)]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    assert!(state_of(&s, "f1").blocked, "open, on the Blocked option");
    let f2 = state_of(&s, "f2");
    assert_eq!(f2.state, ItemState::NotStarted, "closed with unfinished work");
    assert!(!f2.blocked, "a closed item is not blocked by its Status");
}
