//! How a merge run is stored and how it is told to a reader
//! (`GRD-graduation.md` GRD-FR-MRNQ, GRD-FR-KZPT, GRD-FR-AQNW, GRD-FR-BHCS,
//! GRD-FR-OYPY).

use serde_json::json;

use super::merge_input::{bare_draft_run, bare_merge_run};
use super::*;

/// A merge run of the fixture's project, whose merge has been applied.
fn applied_run(fx: &Fixture) -> GraduationRun {
    let mut run = bare_merge_run();
    run.project_key = fx.project_key();
    run.state = GraduationRunState::Completed;
    run.base_commit = run.merge.as_ref().map(|m| m.snapshot_commit.clone());
    run.commits = vec!["e".repeat(40)];
    run.merge.as_mut().unwrap().result = Some(GraduationMergeResult {
        published: MergePublished::Commit,
        commit: Some("e".repeat(40)),
        merged_paths: vec!["README.md".into(), "src/queue.rs".into()],
    });
    run
}

fn read_back(fx: &Fixture, run: &GraduationRun) -> GraduationRun {
    let fs = crate::graduation::store_fs(&fx.app).expect("a store");
    let base = crate::graduation::store_base(&fx.app).expect("a store");
    crate::graduation::store::write_run_record(&fs, &base, run).expect("written");
    crate::graduation::store::read_run_record(&fs, &base, &run.id).expect("read")
}

// GRD-FR-MRNQ, GRD-FR-KZPT, GRD-FR-AQNW: a run with merge data round-trips
// through the run store's TOML record with every field of the merge data and of
// its result.
#[test]
fn a_run_with_merge_data_round_trips_through_the_toml_store() {
    let fx = Fixture::new();
    for run in [bare_merge_run(), applied_run(&fx)] {
        let back = read_back(&fx, &run);

        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&run).unwrap()
        );
        assert_eq!(back.merge, run.merge);
        assert!(back.is_merge());
    }
}

// GRD-FR-MRNQ, GRD-FR-KZPT: the merge data is camelCase on the wire, with the
// publication tagged by `kind`, and `result` is absent until the merge is applied.
#[test]
fn the_merge_data_is_camel_case_and_has_no_result_before_the_apply() {
    let before = serde_json::to_value(bare_merge_run()).unwrap();
    let merge = &before["merge"];

    assert_eq!(merge["name"], "Merge feature");
    assert_eq!(merge["streamBranch"], "synthesis/stream/feature");
    assert_eq!(merge["baseBranch"], "main");
    assert_eq!(merge["baseTip"], "a".repeat(40));
    assert_eq!(merge["streamTip"], "b".repeat(40));
    assert_eq!(merge["mergeBase"], "c".repeat(40));
    assert_eq!(merge["snapshotCommit"], "d".repeat(40));
    assert_eq!(merge["publication"], json!({ "kind": "commit", "message": "Land feature" }));
    assert_eq!(merge["changedPaths"], json!(["README.md", "src/call.rs", "src/queue.rs"]));
    assert_eq!(merge["unresolvedPaths"], json!(["src/queue.rs"]));
    assert_eq!(
        merge["conflicts"],
        json!([{ "path": "src/queue.rs", "baseChange": "updated", "streamChange": "deleted" }])
    );
    assert!(merge.get("result").is_none(), "no result before the apply: {merge}");
    for snake in ["stream_branch", "base_tip", "changed_paths", "unresolved_paths"] {
        assert!(merge.get(snake).is_none(), "{snake} is not on the wire");
    }

    let fx = Fixture::new();
    let after = serde_json::to_value(applied_run(&fx)).unwrap();
    assert_eq!(
        after["merge"]["result"],
        json!({
            "published": "commit",
            "commit": "e".repeat(40),
            "mergedPaths": ["README.md", "src/queue.rs"]
        })
    );
    let uncommitted = GraduationMergeResult {
        published: MergePublished::Uncommitted,
        commit: None,
        merged_paths: vec!["a.txt".into()],
    };
    assert_eq!(
        serde_json::to_value(uncommitted).unwrap(),
        json!({ "published": "uncommitted", "mergedPaths": ["a.txt"] })
    );
}

// GRD-FR-MRNQ: a record with no merge data reads back unchanged, writes no
// `merge` table, and a record written before merge runs existed reads as a run
// that is not a merge run.
#[test]
fn a_run_without_merge_data_is_stored_and_read_unchanged() {
    let fx = Fixture::new();
    let mut run = bare_draft_run();
    run.project_key = fx.project_key();

    let back = read_back(&fx, &run);

    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&run).unwrap()
    );
    assert!(back.merge.is_none());
    assert!(!back.is_merge());
    let base = crate::graduation::store_base(&fx.app).unwrap();
    let text = std::fs::read_to_string(base.record(&run.id)).unwrap();
    assert!(!text.contains("[merge") && !text.contains("merge."), "no merge table in the record:\n{text}");
    assert!(serde_json::to_value(&run).unwrap().get("merge").is_none());
}

// GRD-FR-BHCS, GRD-FR-LGDV: `get_graduation_run` and `list_graduation_queue`
// answer for a merge run with its merge data, its title and its state, like
// every run; `merge.result` stands from the moment the run is `completed`.
#[test]
fn a_merge_run_is_listed_and_read_with_its_merge_data() {
    let fx = Fixture::new();
    let mut run = bare_merge_run();
    run.project_key = fx.project_key();
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");

    let read = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone()).expect("a run");
    assert_eq!(read.merge.as_ref().map(|m| m.name.as_str()), Some("Merge feature"));
    assert!(read.merge.as_ref().unwrap().result.is_none());
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("a queue");
    let listed = queue.runs.iter().find(|r| r.id == run.id).expect("listed");
    assert_eq!(listed.merge, run.merge);
    assert_eq!(listed.state, GraduationRunState::Queued);

    let mut done = applied_run(&fx);
    done.id = run.id.clone();
    crate::graduation::save_run(&fx.app, &mut done).expect("saved");
    let read = crate::graduation::get_graduation_run(fx.app.clone(), run.id.clone()).expect("a run");
    assert_eq!(read.state, GraduationRunState::Completed);
    assert_eq!(
        read.merge.unwrap().result.unwrap().merged_paths,
        vec!["README.md".to_string(), "src/queue.rs".to_string()]
    );
}

// GRD-FR-OYPY, GRD-FR-MRNQ: the run order index carries whether a run is a merge
// run and whether it is archived, so a reader needs no run record for either.
#[test]
fn a_saved_merge_run_is_marked_in_the_run_order_index() {
    let fx = Fixture::new();
    let mut merge = bare_merge_run();
    merge.project_key = fx.project_key();
    let mut draft = bare_draft_run();
    draft.id = "gdraft1".into();
    draft.project_key = fx.project_key();
    crate::graduation::save_run(&fx.app, &mut merge).expect("saved");
    crate::graduation::save_run(&fx.app, &mut draft).expect("saved");

    let fs = crate::graduation::store_fs(&fx.app).unwrap();
    let base = crate::graduation::store_base(&fx.app).unwrap();
    let entry = |id: &str| {
        read_queue_index(&fs, &base, &fx.project_key())
            .runs
            .into_iter()
            .find(|e| e.run_id == id)
            .expect("an entry")
    };
    assert!(entry(&merge.id).merge);
    assert!(!entry(&draft.id).merge);
    assert!(!entry(&merge.id).archived);

    merge.archived = true;
    merge.archived_at = Some("t1".into());
    crate::graduation::save_run(&fx.app, &mut merge).expect("saved");
    assert!(entry(&merge.id).archived);
    assert!(entry(&merge.id).merge);
}

// GRD-FR-VCTH, GRD-FR-NHRY: a merge run's title is its merge name and never a
// draft name, and the title is a function of the stream name.
#[test]
fn the_title_of_a_merge_run_is_merge_and_the_stream_name() {
    assert_eq!(crate::graduation::merge_title("feature"), "Merge feature");
    assert_eq!(crate::graduation::merge_title("two words"), "Merge two words");
    let run = bare_merge_run();
    assert_eq!(run.merge.as_ref().unwrap().name, crate::graduation::merge_title(&run.stream_name));
    assert_eq!(run.input.draft_name, "");
}
