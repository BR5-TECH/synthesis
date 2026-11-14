//! The tests of the delivery state of a GitHub plan: the rules that read the
//! tracker facts of each item (`FDL-flow-delivery.md` FDL-FR-TTZZ,
//! FDL-FR-YIAZ, FDL-FR-WPSZ, FDL-FR-UEPM).

use serde_json::json;

use super::tests::{container, doc, draft, shadow_draft, skeleton, state_of, task};
use super::*;

fn issue(number: u64) -> Option<IssueReference> {
    Some(IssueReference {
        host: "github.com".into(),
        owner: "ACME".into(),
        repo: "Pay".into(),
        number,
    })
}

fn open_issue(number: u64) -> ItemTrackerFacts {
    ItemTrackerFacts { issue: issue(number), ..ItemTrackerFacts::default() }
}

fn closed(reason: Option<CloseReason>) -> ItemTrackerFacts {
    ItemTrackerFacts { state: TrackerState::Closed, close_reason: reason, ..Default::default() }
}

fn facts(entries: Vec<(&str, ItemTrackerFacts)>) -> TrackerFacts {
    entries.into_iter().map(|(id, f)| (id.to_string(), f)).collect()
}

/// FDL-FR-TTZZ: a leaf of a GitHub plan with no `draftId` takes the
/// GitHub-shadow draft of its issue; host, owner, and repository compare
/// without case, and another number or repository matches nothing. A
/// `draftId` still wins over the issue.
#[test]
fn a_github_leaf_takes_the_shadow_draft_of_its_issue() {
    let d = doc(
        skeleton(vec![
            task("t1", "f1", None),
            task("t2", "f1", None),
            task("t3", "f2", Some("a")),
            task("t4", "f2", None),
        ]),
        json!([]),
    );
    let mut other_repo = open_issue(51);
    other_repo.issue.as_mut().unwrap().repo = "web".into();
    let tracker = facts(vec![
        ("t1", open_issue(51)),
        ("t2", open_issue(52)),
        ("t3", open_issue(51)),
        ("t4", other_repo),
    ]);
    let drafts = vec![shadow_draft("s", 51), draft("a", DraftStatus::Active, None)];
    let s = delivery_state_with_tracker(&d, &tracker, &drafts);
    assert_eq!(state_of(&s, "t1").draft.as_ref().unwrap().id, "s");
    assert_eq!(state_of(&s, "t1").state, ItemState::Drafting);
    assert!(state_of(&s, "t2").draft.is_none());
    assert_eq!(state_of(&s, "t2").state, ItemState::NotStarted);
    assert_eq!(state_of(&s, "t3").draft.as_ref().unwrap().id, "a");
    assert!(state_of(&s, "t4").draft.is_none());
    assert!(!state_of(&s, "t1").draft_missing && !state_of(&s, "t2").draft_missing);
}

/// FDL-FR-YIAZ: a closed item is `dropped` where its close reason is
/// `not_planned`, and `done` otherwise, whatever its draft says; the rule
/// takes the place of FDL-FR-VGZI for a leaf and FDL-FR-JYKC for a container.
#[test]
fn a_closed_item_is_done_or_dropped_by_its_close_reason() {
    let d = doc(
        skeleton(vec![
            task("t1", "f1", Some("a")),
            task("t2", "f1", Some("b")),
            task("t3", "f2", None),
            task("t4", "f2", Some("c")),
        ]),
        json!([]),
    );
    let tracker = facts(vec![
        ("t1", closed(Some(CloseReason::Completed))),
        ("t2", closed(Some(CloseReason::NotPlanned))),
        ("t3", closed(None)),
        ("f2", closed(Some(CloseReason::NotPlanned))),
    ]);
    let drafts = vec![
        draft("a", DraftStatus::Active, Some(GraduationRunState::Working)),
        draft("b", DraftStatus::Graduated, None),
        draft("c", DraftStatus::Graduated, None),
    ];
    let s = delivery_state_with_tracker(&d, &tracker, &drafts);
    assert_eq!(state_of(&s, "t1").state, ItemState::Done);
    assert_eq!(state_of(&s, "t2").state, ItemState::Dropped);
    assert_eq!(state_of(&s, "t3").state, ItemState::Done, "no reason reads as completed");
    assert_eq!(state_of(&s, "t4").state, ItemState::Done);
    assert_eq!(
        state_of(&s, "f2").state,
        ItemState::Dropped,
        "a closed container follows its own reason, not its leaves"
    );
    assert_eq!(state_of(&s, "f1").state, ItemState::Done, "done and dropped leaves");
    // The leaf counts of a container still count its leaves.
    let counts = state_of(&s, "f2").leaf_counts.clone().unwrap();
    assert_eq!((counts.done, counts.dropped), (2, 0));
    // The draft of a closed leaf is still reported.
    assert_eq!(state_of(&s, "t1").run.as_ref().unwrap().state, GraduationRunState::Working);

    let unrelated = facts(vec![("f1", open_issue(9))]);
    let empty = doc(vec![container("m1", "milestone", None)], json!([]));
    assert_eq!(
        state_of(&delivery_state_with_tracker(&empty, &unrelated, &[]), "m1").state,
        ItemState::NotStarted,
        "an open item keeps the rule of its leaves"
    );
}

/// FDL-FR-WPSZ: an open leaf that is `not_started` or `drafting` is
/// `in_progress` where its Status is the In Progress option. A leaf that is
/// already `done` or `dropped` by its draft, a closed leaf, and a leaf on
/// another Status keep their state.
#[test]
fn an_open_leaf_on_the_in_progress_status_is_in_progress() {
    let d = doc(
        skeleton(vec![
            task("t1", "f1", None),
            task("t2", "f1", Some("a")),
            task("t3", "f1", Some("b")),
            task("t4", "f2", Some("c")),
            task("t5", "f2", None),
            task("t6", "f2", None),
        ]),
        json!([]),
    );
    let on_board = |status: &str, in_progress: bool| ItemTrackerFacts {
        status: Some(status.into()),
        status_is_in_progress: in_progress,
        ..ItemTrackerFacts::default()
    };
    let mut closed_on_board = on_board("In Progress", true);
    closed_on_board.state = TrackerState::Closed;
    let tracker = facts(vec![
        ("t1", on_board("In Progress", true)),
        ("t2", on_board("In Progress", true)),
        ("t3", on_board("In Progress", true)),
        ("t4", on_board("In Progress", true)),
        ("t5", on_board("Todo", false)),
        ("t6", closed_on_board),
        ("f2", on_board("In Progress", true)),
    ]);
    let drafts = vec![
        draft("a", DraftStatus::Active, None),
        draft("b", DraftStatus::Graduated, None),
        draft("c", DraftStatus::Archived, None),
    ];
    let s = delivery_state_with_tracker(&d, &tracker, &drafts);
    let got: Vec<_> =
        ["t1", "t2", "t3", "t4", "t5", "t6"].iter().map(|id| state_of(&s, id).state).collect();
    assert_eq!(
        got,
        vec![
            ItemState::InProgress,
            ItemState::InProgress,
            ItemState::Done,
            ItemState::Dropped,
            ItemState::NotStarted,
            ItemState::Done,
        ]
    );
    // The rule is for leaves: a container comes from its leaves.
    assert_eq!(state_of(&s, "f2").state, ItemState::InProgress, "t6 is done");

    // A container on the In Progress option whose leaves have not started
    // stays `not_started`: FDL-FR-WPSZ is for leaves alone.
    let quiet = doc(skeleton(vec![task("t1", "f1", None)]), json!([]));
    let tracker = facts(vec![("f1", on_board("In Progress", true))]);
    let s = delivery_state_with_tracker(&quiet, &tracker, &[]);
    assert_eq!(state_of(&s, "f1").state, ItemState::NotStarted);
    assert_eq!(state_of(&s, "t1").state, ItemState::NotStarted);
}

/// FDL-FR-UEPM: an item that is not done or dropped is blocked where it has an
/// open external blocker, and the blocker is not listed in `blocked_by`. A
/// finished item is never blocked.
#[test]
fn an_open_external_blocker_blocks_an_unfinished_item() {
    let d = doc(
        skeleton(vec![task("t1", "f1", None), task("t2", "f2", None), task("t3", "f2", None)]),
        json!([{ "id": "e1", "from": "t1", "to": "t3", "type": "blocks" }]),
    );
    let blocked_outside = |count: usize| ItemTrackerFacts {
        open_external_blockers: count,
        ..ItemTrackerFacts::default()
    };
    let mut done_but_blocked = blocked_outside(2);
    done_but_blocked.state = TrackerState::Closed;
    let tracker = facts(vec![
        ("t1", blocked_outside(1)),
        ("t2", done_but_blocked),
        ("t3", blocked_outside(1)),
        ("f1", blocked_outside(0)),
    ]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    let t1 = state_of(&s, "t1");
    assert!(t1.blocked);
    assert!(t1.blocked_by.is_empty(), "an external blocker is not listed");
    assert!(!state_of(&s, "t2").blocked, "a done item is never blocked");
    let t3 = state_of(&s, "t3");
    assert!(t3.blocked);
    assert_eq!(t3.blocked_by, vec!["t1".to_string()]);
    assert!(!state_of(&s, "f1").blocked, "no open external blocker");
}

/// FDL-FR-PBSD: the same document, tracker facts, and drafts give the same
/// answer; and with no tracker facts the answer is that of a Flow file.
#[test]
fn the_tracker_answer_is_pure_and_matches_a_file_without_facts() {
    let d = doc(skeleton(vec![task("t1", "f1", Some("a"))]), json!([]));
    let drafts = vec![draft("a", DraftStatus::Active, Some(GraduationRunState::Working))];
    let tracker = facts(vec![("t1", closed(None))]);
    assert_eq!(
        delivery_state_with_tracker(&d, &tracker, &drafts),
        delivery_state_with_tracker(&d, &tracker, &drafts)
    );
    assert_eq!(
        delivery_state_with_tracker(&d, &TrackerFacts::new(), &drafts),
        delivery_state(&d, &drafts)
    );
}

/// FDL-FR-YIAZ, FDL-FR-PXHL, FDL-FR-UEPM: a closed `not_planned` item is
/// dropped, so a `blocks` edge from it blocks nothing; a dropped item with an
/// open external blocker is not blocked; and a closed container with open
/// leaves is `done`.
#[test]
fn a_closed_blocker_blocks_nothing_and_a_closed_container_is_finished() {
    let d = doc(
        skeleton(vec![task("t1", "f1", None), task("t2", "f2", None)]),
        json!([{ "id": "e1", "from": "t1", "to": "t2", "type": "blocks" }]),
    );
    let mut dropped_blocked = closed(Some(CloseReason::NotPlanned));
    dropped_blocked.open_external_blockers = 3;
    let tracker = facts(vec![("t1", dropped_blocked), ("f2", closed(None))]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    let t1 = state_of(&s, "t1");
    assert_eq!(t1.state, ItemState::Dropped);
    assert!(!t1.blocked, "a dropped item is never blocked");
    let t2 = state_of(&s, "t2");
    assert_eq!(t2.state, ItemState::NotStarted);
    assert!(!t2.blocked && t2.blocked_by.is_empty(), "a dropped blocker blocks nothing");
    assert_eq!(state_of(&s, "f2").state, ItemState::Done, "closed with an open leaf");
}

/// FDL-FR-UEPM: an external blocker of a container blocks the container
/// alone. It is not passed down to the leaves under it, as a `blocks` edge is
/// (FDL-FR-PXHL).
#[test]
fn an_external_blocker_of_a_container_does_not_block_its_leaves() {
    let d = doc(skeleton(vec![task("t1", "f1", None)]), json!([]));
    let tracker = facts(vec![(
        "f1",
        ItemTrackerFacts { open_external_blockers: 1, ..ItemTrackerFacts::default() },
    )]);
    let s = delivery_state_with_tracker(&d, &tracker, &[]);
    assert!(state_of(&s, "f1").blocked);
    assert!(!state_of(&s, "t1").blocked);
}

/// FDL-FR-TTZZ, FDL-FR-BRAQ: a GitHub leaf whose `draftId` names no draft is
/// `draft_missing`, and does not fall back to the shadow draft of its issue.
#[test]
fn a_missing_draft_id_does_not_fall_back_to_the_shadow_draft() {
    let d = doc(skeleton(vec![task("t1", "f1", Some("gone"))]), json!([]));
    let tracker = facts(vec![("t1", open_issue(51))]);
    let s = delivery_state_with_tracker(&d, &tracker, &[shadow_draft("s", 51)]);
    let t1 = state_of(&s, "t1");
    assert!(t1.draft_missing);
    assert!(t1.draft.is_none());
    assert_eq!(t1.state, ItemState::NotStarted);
}
