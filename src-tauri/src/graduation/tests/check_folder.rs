//! The check folder through a driven run (`../ai/GRL-graduation-loop.md`
//! GRL-FR-XQMT, GRL-FR-LXWU, GRL-FR-DCKG, GRL-FR-KYYW, GRL-FR-AMQE,
//! GRL-FR-FESZ).

use super::*;
use crate::graduation::driver::checks_folder::{DIR, EXCLUDE_LINE, MISSING, REPORT_PATH};

const REPORT: &str = "| command | exit |\n| cargo test | 0 |\n";
const LOG_PATH: &str = ".synthesis-checks/logs/cargo-test.log";

/// Whether a commit's tree holds the check folder.
fn commits_check_folder(worktree: &Path, revision: &str) -> bool {
    let repo = git2::Repository::open(worktree).unwrap();
    let tree = repo.find_commit(git2::Oid::from_str(revision).unwrap()).unwrap().tree().unwrap();
    tree.get_path(Path::new(DIR)).is_ok()
}

// GRL-FR-AMQE, GRL-FR-FESZ, GRL-FR-DCKG, GRL-FR-LXWU: the review stands in a
// checkout that holds the work turn's record, is told where the record is, and
// sees no part of it in the change set. The commit holds none of it either.
#[test]
fn the_review_reads_the_record_the_work_turn_wrote_and_the_commit_leaves_it_out() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("src/panel.ts", "export const panel = 1;\n")
            .writing(REPORT_PATH, REPORT)
            .writing(LOG_PATH, "test result: ok. 3 passed\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);

    // The work turn is given no `checks`.
    let work = &dispatch.of_part("work")[0];
    assert!(!work.input.contains_key("checks"), "a work turn carries no checks");

    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.input.get("checks").and_then(|v| v.as_str()), Some(REPORT_PATH));
    assert_eq!(review.tree.get(REPORT_PATH).map(String::as_str), Some(REPORT));
    assert_eq!(review.tree.get(LOG_PATH).map(String::as_str), Some("test result: ok. 3 passed\n"));
    assert_eq!(review.diff_paths, vec!["src/panel.ts".to_string()], "the folder is no change");
    assert_eq!(review.path_list("changed_paths"), vec!["src/panel.ts".to_string()]);
    assert!(review.path_list("hidden_paths").iter().all(|p| !p.starts_with(DIR)));

    // The commit holds the work and nothing of the folder.
    assert!(!commits_check_folder(&stream.worktree(), run.commits.last().unwrap()));

    // The repository hides the folder through its private exclude file.
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let exclude = std::fs::read_to_string(repo.commondir().join("info/exclude")).unwrap();
    assert_eq!(exclude.lines().filter(|l| l.trim() == EXCLUDE_LINE).count(), 1);
    assert!(
        !stream.worktree().join(".gitignore").exists()
            || !std::fs::read_to_string(stream.worktree().join(".gitignore"))
                .unwrap()
                .contains(DIR),
        "no .gitignore names the folder"
    );
}

// GRL-FR-FESZ: a work turn that wrote no record leaves the review a `missing`
// report.
#[test]
fn a_work_turn_that_recorded_nothing_leaves_the_review_a_missing_report() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch =
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.input.get("checks").and_then(|v| v.as_str()), Some(MISSING));
    assert!(!review.tree.keys().any(|path| path.starts_with(DIR)));
}

// GRL-FR-KYYW: a record left in the stream before the run is gone before the
// work turn starts, so the review cannot read a result of an earlier turn.
#[test]
fn a_stale_record_is_removed_before_the_work_turn() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    std::fs::create_dir_all(stream.worktree().join(DIR)).unwrap();
    std::fs::write(stream.worktree().join(REPORT_PATH), "stale\n").unwrap();
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch =
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]);
    fx.drive(&run, dispatch.clone());

    let work = &dispatch.of_part("work")[0];
    assert!(!work.tree.keys().any(|path| path.starts_with(DIR)), "the work turn sees none");
    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.input.get("checks").and_then(|v| v.as_str()), Some(MISSING));
}

// GRL-FR-KYYW, GRL-FR-AMQE: each pass's review reads the record of its own work
// turn, and never the record of the pass before.
#[test]
fn each_review_reads_the_record_of_its_own_pass() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n").writing(REPORT_PATH, "first\n"),
        Turn::revise(ReviewSeverity::Major, "The empty state is missing."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    let works = dispatch.of_part("work");
    assert!(
        !works[1].tree.keys().any(|path| path.starts_with(DIR)),
        "the second work turn starts with no record"
    );
    let reviews = dispatch.of_part("review");
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0].tree.get(REPORT_PATH).map(String::as_str), Some("first\n"));
    assert_eq!(
        reviews[1].input.get("checks").and_then(|v| v.as_str()),
        Some(MISSING),
        "the second work turn wrote no record, and the first one's is gone"
    );
}

// GRL-FR-DCKG: the change set and the standing-work commit leave the folder out
// also where the repository's exclude file does not hide it.
#[test]
fn the_change_set_leaves_the_folder_out_without_the_exclude_line() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let worktree = stream.worktree();
    let repo = git2::Repository::open(&worktree).unwrap();
    let base = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    let _ = std::fs::remove_file(repo.commondir().join("info/exclude"));

    std::fs::create_dir_all(worktree.join(DIR).join("logs")).unwrap();
    std::fs::write(worktree.join(REPORT_PATH), REPORT).unwrap();
    std::fs::write(worktree.join(LOG_PATH), "ok\n").unwrap();

    // Only the folder: the change set is empty.
    assert!(crate::graduation::commit::run_tree(&worktree, &base).unwrap().is_none());
    assert!(
        crate::graduation::commit::commit_author_work(&worktree, "standing").unwrap().is_none(),
        "the folder alone is no standing work"
    );

    // With work beside it, the work is the change set and the commit holds it alone.
    std::fs::write(worktree.join("src.ts"), "1\n").unwrap();
    let derived = crate::graduation::commit::run_tree(&worktree, &base).unwrap().unwrap();
    assert_eq!(derived.paths, vec!["src.ts".to_string()]);
    let revision = crate::graduation::commit::commit_author_work(&worktree, "standing")
        .unwrap()
        .expect("the work is committed");
    assert!(!commits_check_folder(&worktree, &revision));
}

// GRL-FR-FESZ: only a review part may carry `checks`, and an input without it
// leaves the field out.
#[test]
fn only_a_review_part_carries_checks() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Rework the panel.");
    run.checkpoint.pass = 1;
    for (part, allowed) in [(driver::PART_WORK, false), (driver::PART_REVIEW, true)] {
        let mut input = driver::compose_input(&run, part, driver::PURPOSE_GENERATE);
        assert!(input.validate(2).is_ok(), "{part} without checks");
        assert!(!input.to_map().contains_key("checks"), "{part} leaves an absent field out");
        input.checks = Some(REPORT_PATH.into());
        match input.validate(2) {
            Ok(()) => assert!(allowed, "{part} accepts checks"),
            Err(reason) => {
                assert!(!allowed, "{part} refuses checks: {reason}");
                assert!(reason.contains("check report"), "{reason}");
            }
        }
    }
    let merge = super::merge_input::bare_merge_run();
    for (part, allowed) in [(driver::PART_WORK, false), (driver::PART_REVIEW, true)] {
        let mut input = driver::compose_input(&merge, part, driver::PURPOSE_GENERATE);
        input.checks = Some(REPORT_PATH.into());
        assert_eq!(input.validate(0).is_ok(), allowed, "merge part {}", input.part);
    }
}

// GRL-FR-DCKG, GRL-FR-LXWU: where the exclude line cannot be written, the run
// still completes, and the change set and the commit still leave the folder out.
#[test]
fn a_run_whose_exclude_line_cannot_be_written_still_leaves_the_folder_out() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let exclude = repo.commondir().join("info/exclude");
    let _ = std::fs::remove_file(&exclude);
    std::fs::create_dir_all(&exclude).unwrap();
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n").writing(REPORT_PATH, REPORT),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);
    assert!(exclude.is_dir(), "the exclude file could not be written");

    let review = &dispatch.of_part("review")[0];
    assert_eq!(review.path_list("changed_paths"), vec!["src/panel.ts".to_string()]);
    assert_eq!(review.input.get("checks").and_then(|v| v.as_str()), Some(REPORT_PATH));
    assert!(!commits_check_folder(&stream.worktree(), run.commits.last().unwrap()));
}

// GRL-FR-LXWU: the exclude line stands before the first work turn starts.
#[test]
fn the_exclude_line_stands_before_the_first_turn() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let exclude = repo.commondir().join("info/exclude");
    let _ = std::fs::remove_file(&exclude);

    let seen = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let record = seen.clone();
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n").doing(move || {
            *record.lock().unwrap() = std::fs::read_to_string(&exclude).ok();
        }),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch);

    let at_dispatch = seen.lock().unwrap().clone().expect("the exclude file stood at the turn");
    assert!(at_dispatch.lines().any(|line| line.trim() == EXCLUDE_LINE), "{at_dispatch:?}");
}
