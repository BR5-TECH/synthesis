//! The wire shapes of a merge request and of its answer
//! (`WKS-work-streams.md` WKS-FR-QNHF, WKS-FR-GKPX, WKS-FR-KHJS, WKS-FR-SGCM).

use serde_json::json;

use super::*;

// WKS-FR-QNHF: the answer of a merge check is tagged with a snake_case `kind`,
// and every other field is camelCase, for all three kinds.
#[test]
fn the_answer_of_a_merge_check_has_a_snake_case_kind_and_camel_case_fields() {
    assert_eq!(
        serde_json::to_value(StreamMergeResult::NothingToMerge).unwrap(),
        json!({ "kind": "nothing_to_merge" })
    );

    let committed = serde_json::to_value(StreamMergeResult::Merged {
        merged_paths: vec!["a.txt".into(), "dir/b.txt".into()],
        commit: Some("abc123".into()),
    })
    .unwrap();
    assert_eq!(
        committed,
        json!({ "kind": "merged", "mergedPaths": ["a.txt", "dir/b.txt"], "commit": "abc123" })
    );

    let uncommitted = serde_json::to_value(StreamMergeResult::Merged {
        merged_paths: vec!["a.txt".into()],
        commit: None,
    })
    .unwrap();
    assert_eq!(uncommitted["kind"], "merged");
    assert!(
        uncommitted.get("commit").is_none_or(|value| value.is_null()),
        "no commit is named where none was made: {uncommitted}"
    );

    assert_eq!(
        serde_json::to_value(StreamMergeResult::Conflicted {
            run_id: "g1".into(),
            conflicted_paths: vec!["README.md".into()],
        })
        .unwrap(),
        json!({ "kind": "conflicted", "runId": "g1", "conflictedPaths": ["README.md"] })
    );
}

// WKS-FR-GKPX, GRB-FR-ASWC: the publication a request names is read from a
// `kind` tag, and there is no default: a request without one is refused as
// unreadable.
#[test]
fn the_publication_is_read_from_its_kind_and_has_no_default() {
    assert_eq!(
        serde_json::from_value::<StreamMergePublication>(json!({ "kind": "uncommitted" })).unwrap(),
        StreamMergePublication::Uncommitted
    );
    assert_eq!(
        serde_json::from_value::<StreamMergePublication>(
            json!({ "kind": "commit", "message": "Merge it" })
        )
        .unwrap(),
        StreamMergePublication::Commit {
            message: "Merge it".into()
        }
    );
    assert!(serde_json::from_value::<StreamMergePublication>(json!({})).is_err());
    assert!(serde_json::from_value::<StreamMergePublication>(json!({ "kind": "commit" })).is_err());
    assert!(
        serde_json::from_value::<StreamMergePublication>(json!({ "kind": "squash" })).is_err()
    );
}

// WKS-FR-KHJS, WKS-FR-SGCM: a summary carries the link to the merge run as
// `mergeRun { runId, name, state }` and no `merge` field, and its other fields
// are camelCase.
#[test]
fn a_summary_serializes_the_merge_run_link_and_no_merge_record() {
    let fx = Fixture::new();
    let stream = fx.create("shaped", None).expect("created");
    let mut summary = summary_of(&fx, &stream.id);
    summary.merge_run = Some(StreamMergeRunLink {
        run_id: "g42".into(),
        name: "Merge shaped".into(),
        state: crate::graduation::GraduationRunState::AwaitingAuthor,
    });

    let json = serde_json::to_value(&summary).unwrap();

    assert_eq!(
        json["mergeRun"],
        json!({ "runId": "g42", "name": "Merge shaped", "state": "awaiting_author" })
    );
    assert!(json.get("merge").is_none());
    for key in ["stream", "queuedRunCount", "aheadOfBase", "behindBase", "baseTipRevision", "missingCommits"] {
        assert!(json.get(key).is_some(), "{key} is in {json}");
    }
}

// WKS-FR-VQDE: the typed errors a merge request can answer are the contract's
// names.
#[test]
fn the_typed_refusals_of_a_merge_have_the_contract_names() {
    assert_eq!(ERR_STREAM_BUSY, "stream_busy");
    assert_eq!(ERR_STREAM_DIRTY, "stream_dirty");
    assert_eq!(ERR_BASE_DIRTY, "base_dirty");
    assert_eq!(ERR_STREAM_MISSING, "stream_missing");
    assert_eq!(ERR_BASE_NOT_CHECKED_OUT, "base_not_checked_out");
    assert_eq!(ERR_MERGE_IN_PROGRESS, "merge_in_progress");
    assert_eq!(ERR_UPDATE_IN_PROGRESS, "update_in_progress");
    assert_eq!(ERR_MERGE_BRANCH_MOVED, "merge_branch_moved");
    assert_eq!(ERR_STREAM_HAS_RUNS, "stream_has_runs");
    assert_eq!(ERR_UNKNOWN_STREAM, "unknown_stream");
}

// WKS-FR-MWYD, WKS-FR-VQDE, GRB-FR-OHWT: the operations and the event of the old
// merge record do not exist any more: no command reads, answers, retries, clears
// or cancels a stream merge, and no merge progress event is emitted.
#[test]
fn the_merge_record_commands_and_the_merge_progress_event_are_gone() {
    let registered = include_str!("../../lib.rs");
    let names = include_str!("../../command_names.rs");
    for gone in [
        "get_work_stream_merge",
        "answer_work_stream_merge_escalation",
        "retry_work_stream_merge",
        "clear_work_stream_merge",
        "cancel_work_stream_merge",
    ] {
        assert!(!registered.contains(gone), "{gone} is still registered");
        assert!(!names.contains(gone), "{gone} is still named");
    }
    assert!(registered.contains("streams::merge_work_stream,"), "the one merge command stays");
    let streams = include_str!("../../streams.rs");
    assert!(!streams.contains("work-stream-merge-progress"));
    assert!(!streams.contains("WORK_STREAM_MERGE_PROGRESS"));
    assert!(streams.contains("work-stream-update-progress"), "the update keeps its event");
}
