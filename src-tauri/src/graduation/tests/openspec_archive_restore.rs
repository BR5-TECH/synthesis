//! The restore of a stream merge whose archive did not complete, and the
//! writes of the CLI outside the folders of an archive
//! (`OAR-openspec-archive.md` OAR-FR-EUXZ, OAR-FR-KBDD, OAR-FR-BLBJ,
//! OAR-FR-MSOM), with the OpenSpec CLI faked.

use super::archive_support::*;
use super::*;
use crate::openspec::ProcessError;

/// The files of the stream's working copy, with their text.
fn files(stream: &crate::streams::WorkStream) -> Vec<(String, String)> {
    super::tree_under(&stream.worktree()).into_iter().collect()
}

/// The store folder of a change's records, and the archive folder of the
/// stream's records.
fn store_folders(fx: &Fixture, stream: &crate::streams::WorkStream) -> (PathBuf, PathBuf) {
    let changes = records_store(fx).path().join("changes").join(&stream.id);
    (changes.join("add-logout"), changes.join("archive"))
}

// OAR-FR-KBDD, OAR-FR-EUXZ: a half archive that moved one file of the change
// folder and changed the live spec, and that the CLI did not undo, is
// restored: the moved file is back in the change folder, the archive folder
// that the archive made is gone, the stream is clean, and its tip has not
// moved.
#[test]
fn a_half_archive_is_restored_with_every_tracked_file_of_the_change() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let stdout = r#"{ "archive": null, "status": [ { "code": "spec_merge_failed" } ] }"#;
    runner.set_archive("add-logout", Archive::MoveHalf { status: 1, stdout: stdout.into() });
    let before = files(&stream);
    let tip_before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: add-logout: spec_merge_failed");
    assert!(dirty(&stream.worktree()).is_empty(), "{:?}", dirty(&stream.worktree()));
    assert_eq!(files(&stream), before);
    assert_eq!(
        std::fs::read_to_string(stream.worktree().join("openspec/changes/add-logout/proposal.md"))
            .unwrap(),
        "# Why add-logout\n"
    );
    // A later archive of the change must not find its archive folder.
    assert!(!stream
        .worktree()
        .join(format!("openspec/changes/archive/{DATE}-add-logout"))
        .exists());
    assert_eq!(tip(&fx, &stream.branch), tip_before);
}

// OAR-FR-EUXZ, OAR-FR-KBDD, OAR-FR-MSOM: a commit that fails after a real
// archive, here because Git's own index lock is held, refuses with
// `commit_failed`; every tracked file of the change folder and the live spec
// are back, the stream tip has not moved, and the change's records stay in
// the store with nothing below its archive folder.
#[test]
fn a_commit_that_fails_after_a_real_archive_is_restored() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    write_record(&fx, &stream, "add-logout", "statistics.jsonl", "{}\n");
    let git_dir = git2::Repository::open(stream.worktree()).unwrap().path().to_path_buf();
    let lock = git_dir.join("index.lock");
    let held = lock.clone();
    *runner.during_archive.lock().unwrap() =
        Some(Box::new(move || std::fs::write(&held, b"").unwrap()));
    let before = files(&stream);
    let tip_before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();
    std::fs::remove_file(&lock).unwrap();

    assert_eq!(refusal, "archive_failed: add-logout: commit_failed");
    assert!(dirty(&stream.worktree()).is_empty(), "{:?}", dirty(&stream.worktree()));
    assert_eq!(files(&stream), before);
    assert!(!stream
        .worktree()
        .join(format!("openspec/changes/archive/{DATE}-add-logout"))
        .exists());
    assert_eq!(tip(&fx, &stream.branch), tip_before);
    let (folder, archive) = store_folders(&fx, &stream);
    assert!(folder.join("statistics.jsonl").is_file());
    assert!(!archive.exists());
    let record = one_record(&stream, "openspec archive restored the working copy");
    assert_eq!(record.fields["change"], "add-logout");
    assert!(record.fields["paths"].as_u64().unwrap() > 0);
    assert_eq!(record.fields["indexWritten"], false, "the lock held the index");
}

// OAR-FR-BLBJ, OAR-FR-OQVF, WKS-FR-UZHT: a CLI that also writes a tracked
// file and an untracked file outside the three folders gets an archive commit
// that holds neither; the stream is then dirty, so the merge is refused with
// `stream_dirty`, the commit stays, the two writes stay, a WARN record tells
// it, and the base branch does not move.
#[test]
fn a_write_outside_the_archive_folders_refuses_the_merge() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    runner.set_archive("add-logout", Archive::SucceedWritingOutside);
    let stream_before = tip(&fx, &stream.branch);
    let base_before = tip(&fx, &stream.base_branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    let other = "openspec/changes/archive/2020-01-01-other/x.md";
    assert_eq!(refusal, format!("stream_dirty: {other}, src.txt"));
    assert_eq!(
        messages_since(&fx, &stream.branch, stream_before),
        ["Archive OpenSpec change add-logout"]
    );
    let committed = commit_paths(&fx, tip(&fx, &stream.branch));
    assert!(!committed.iter().any(|p| p == "src.txt" || p == other), "{committed:?}");
    assert_eq!(dirty(&stream.worktree()), [other.to_string(), "src.txt".to_string()]);
    assert_eq!(tip(&fx, &stream.base_branch), base_before);
    let record = one_record(&stream, "openspec archive left paths outside its folders");
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(record.fields["code"], "archive_wrote_outside");
    assert_eq!(record.fields["paths"], 2);
}

// OAR-FR-EUXZ: an archive process that times out refuses with
// `cli_timed_out`, and writes nothing.
#[test]
fn an_archive_that_times_out_refuses_with_cli_timed_out() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    runner.set_archive("add-logout", Archive::Answer(Err(ProcessError::timed_out())));
    let tip_before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: add-logout: cli_timed_out");
    assert_eq!(tip(&fx, &stream.branch), tip_before);
    assert!(dirty(&stream.worktree()).is_empty());
}

// OAR-FR-EUXZ, OAR-FR-IAKL: where the mode goes off after the check passed,
// the archive refuses with `openspec_mode_inactive`, and no archive process
// starts.
#[test]
fn a_mode_that_goes_off_after_the_check_refuses_the_archive() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let root = fx.root();
    *runner.after_instructions.lock().unwrap() = Some(Box::new(move || {
        std::fs::create_dir_all(root.join(".synthesis")).unwrap();
        std::fs::write(root.join(".synthesis/project.toml"), "openspecMode = \"off\"\n").unwrap();
    }));
    let tip_before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: add-logout: openspec_mode_inactive");
    assert!(runner.archive_calls().is_empty());
    assert_eq!(tip(&fx, &stream.branch), tip_before);
}

// OFP-FR-IJYA, OFP-FR-PQNS, OFP-FR-MJZU, OAR-FR-KBDD: a transient archive
// that wrote files runs once more only after its restore is complete: the
// second archive moves the change folder to the archive folder that the first
// wrote into, which only works where the restore removed what the first
// wrote. One wait, two archive processes, and the archive commit holds none
// of the writes of the first.
#[test]
fn a_transient_archive_runs_again_only_after_a_real_restore() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    runner.set_archive(
        "add-logout",
        Archive::Then(
            Box::new(Archive::FailWriting { status: 2, stdout: String::new() }),
            Box::new(Archive::Succeed),
        ),
    );

    let merged = merge(&fx, &stream, commit_publication()).expect("the merge goes on");

    assert!(matches!(merged, crate::streams::StreamMergeResult::Merged { .. }), "{merged:?}");
    assert_eq!(runner.archive_calls().len(), 2, "one archive, run once more");
    assert_eq!(super::change_runs::waits(&fx), vec![2_000], "one wait of 2 seconds");
    let repo = fx.repo();
    let mut walk = repo.revwalk().unwrap();
    walk.push(tip(&fx, &stream.base_branch)).unwrap();
    let archive_commit = walk
        .flatten()
        .find(|oid| {
            repo.find_commit(*oid).unwrap().summary().ok().flatten()
                == Some("Archive OpenSpec change add-logout")
        })
        .expect("the archive commit");
    let paths = commit_paths(&fx, archive_commit);
    assert!(paths.iter().any(|p| p.contains("/archive/")), "{paths:?}");
    assert!(
        !paths.iter().any(|p| p.ends_with("stray.md") || p.ends_with("new-cap.md")),
        "{paths:?}"
    );
    assert!(dirty(&stream.worktree()).is_empty(), "{:?}", dirty(&stream.worktree()));
}

// OFP-FR-IJYA: a transient archive whose restore cannot clean a path, here a
// folder that stands where the live spec file is, ends with its first outcome
// and does not run again.
#[test]
fn a_restore_that_fails_ends_the_archive_with_its_first_outcome() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    runner.set_archive(
        "add-logout",
        Archive::Then(Box::new(Archive::FolderForSpec { status: 2 }), Box::new(Archive::Succeed)),
    );
    let tip_before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: add-logout: cli_failed");
    assert_eq!(runner.archive_calls().len(), 1, "no second archive after a failed restore");
    assert!(super::change_runs::waits(&fx).is_empty(), "no wait");
    assert_eq!(tip(&fx, &stream.branch), tip_before);
    let record = one_record(&stream, "openspec archive could not restore the working copy");
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(record.fields["code"], "restore_failed");
}
