//! The archive of a stream merge that succeeds (`OAR-openspec-archive.md`
//! OAR-FR-KGDP through OAR-FR-SOFQ, OAR-FR-MSOM, OAR-FR-LUSO;
//! `WKS-work-streams.md` WKS-FR-PXYH, WKS-FR-PZKD, WKS-FR-FOTC), with the
//! OpenSpec CLI faked.

use super::archive_support::*;
use super::*;
use crate::openspec_archive::ArchivedChange;
use crate::streams::StreamMergeResult;

fn archived(change: &str) -> ArchivedChange {
    ArchivedChange { change: change.into(), archived_as: format!("{DATE}-{change}") }
}

// OAR-FR-KGDP, OAR-FR-SEQU, OAR-FR-AEOK, OAR-FR-IAKL, OAR-FR-VAGH,
// OAR-FR-LZLS, OAR-FR-BLBJ, OAR-FR-SOFQ, OAR-FR-HQAI, OSM-FR-GMKV,
// WKS-FR-PXYH: the change of a completed
// apply run is checked with one `instructions apply`, archived with one
// `archive <change> --json --yes` in the stream's working copy, and committed
// on the stream branch as one commit of exactly the archive's paths; the base
// branch gets it in the merge, and the result names it.
#[test]
fn a_built_change_is_archived_on_the_stream_and_merged() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let stream_before = tip(&fx, &stream.branch);

    let result = merge(&fx, &stream, commit_publication()).expect("merged");

    let StreamMergeResult::Merged { archived: list, commit, merged_paths } = result else {
        panic!("expected a merge, got {result:?}");
    };
    assert_eq!(list, vec![archived("add-logout")]);
    assert!(commit.is_some());
    assert!(merged_paths.contains(&format!("openspec/changes/archive/{DATE}-add-logout/tasks.md")));
    assert_eq!(
        messages_since(&fx, &stream.branch, stream_before),
        ["Archive OpenSpec change add-logout"]
    );
    let archive_commit = tip(&fx, &stream.branch);
    let message = fx.repo().find_commit(archive_commit).unwrap().message().unwrap().to_string();
    assert_eq!(message, "Archive OpenSpec change add-logout", "one line with no body");
    assert_eq!(
        commit_paths(&fx, archive_commit),
        [
            "openspec/changes/add-logout/proposal.md".to_string(),
            "openspec/changes/add-logout/specs/auth/spec.md".to_string(),
            "openspec/changes/add-logout/tasks.md".to_string(),
            format!("openspec/changes/archive/{DATE}-add-logout/proposal.md"),
            format!("openspec/changes/archive/{DATE}-add-logout/specs/auth/spec.md"),
            format!("openspec/changes/archive/{DATE}-add-logout/tasks.md"),
            "openspec/specs/auth/spec.md".to_string(),
        ]
    );
    // The base branch holds the code, the live spec, and the archive.
    let base = fx.root();
    assert_eq!(std::fs::read_to_string(base.join("src.txt")).unwrap(), "built by editor\n");
    assert!(std::fs::read_to_string(base.join("openspec/specs/auth/spec.md"))
        .unwrap()
        .contains("From add-logout"));
    assert!(!base.join("openspec/changes/add-logout").exists());
    assert!(dirty(&stream.worktree()).is_empty(), "the stream is clean");
    // OAR-FR-AEOK: after the CLI check, one `instructions apply` and one
    // `archive`, both in the stream's working copy.
    let calls: Vec<_> =
        runner.inner.calls().into_iter().filter(|c| c.args[0] != "--version").collect();
    let names: Vec<&str> = calls.iter().map(|c| c.args[0].as_str()).collect();
    assert_eq!(names, ["instructions", "archive"]);
    assert_eq!(calls[0].args[1], "apply");
    assert!(calls[0].args.contains(&"add-logout".to_string()), "{:?}", calls[0].args);
    assert_eq!(calls[1].args, ["archive", "add-logout", "--json", "--yes"]);
    for call in &calls {
        assert_eq!(canonical(&call.cwd), canonical(&stream.worktree()), "{:?}", call.args);
    }
    // OAR-FR-HQAI: the commit is told with the stream, the change, the archive
    // folder, the count of committed paths, and the duration.
    let record = one_record(&stream, "openspec archive committed");
    assert_eq!(record.level, crate::logging::LogLevel::Info);
    assert_eq!(record.fields["change"], "add-logout");
    assert_eq!(record.fields["archivedAs"], format!("{DATE}-add-logout").as_str());
    assert_eq!(record.fields["paths"], 7);
    assert!(record.fields["durationMs"].is_u64());
    // OSM-FR-GMKV: the change workspace now reads the change as archived.
    let read = tauri::async_runtime::block_on(crate::openspec::change::read_openspec_change(
        fx.app.clone(),
        crate::openspec::ChangeLocation::Stream { stream_id: stream.id.clone() },
        "add-logout".into(),
    ));
    let failure = read.unwrap_err();
    assert_eq!(failure.code, "change_archived");
    assert_eq!(failure.detail, Some(format!("{DATE}-add-logout")));
    // WKS-FR-PZKD: the archive commit is no record of the merge: no merge run.
    let key = fx.project_key();
    let fs = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    assert!(read_queue_index(&fs, &base, &key).runs.iter().all(|entry| !entry.merge));
}

// OAR-FR-SURB, OAR-FR-KGDP: the changes are archived in the order in which the
// first apply run of each completed, once each; a reverted run, a run landed
// on the base branch, and a change whose folder is gone are not archived.
#[test]
fn the_changes_are_archived_in_the_order_their_first_run_completed() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream =
        stream_with_changes(&fx, "editor", &["first-done", "second-done", "reverted", "landed"]);
    completed_apply(&fx, &stream, "second-done", "2026-11-10T12:00:00Z");
    completed_apply(&fx, &stream, "first-done", "2026-11-10T11:00:00Z");
    completed_apply(&fx, &stream, "second-done", "2026-11-10T09:00:00Z");
    completed_apply(&fx, &stream, "missing-folder", "2026-11-10T08:00:00Z");
    let mut reverted = completed_apply(&fx, &stream, "reverted", "2026-11-10T07:00:00Z");
    reverted.reverts.push(Landing {
        branch: stream.branch.clone(),
        commit: tip(&fx, &stream.branch).to_string(),
        at: "2026-11-10T07:30:00Z".into(),
    });
    save_run(&fx.app, &mut reverted).unwrap();
    let mut landed = completed_apply(&fx, &stream, "landed", "2026-11-10T06:00:00Z");
    landed.landings.push(Landing {
        branch: stream.base_branch.clone(),
        commit: tip(&fx, &stream.base_branch).to_string(),
        at: "2026-11-10T06:30:00Z".into(),
    });
    save_run(&fx.app, &mut landed).unwrap();

    let result = merge(&fx, &stream, commit_publication()).expect("merged");

    let StreamMergeResult::Merged { archived: list, .. } = result else { panic!("{result:?}") };
    assert_eq!(list, vec![archived("second-done"), archived("first-done")]);
    let archives: Vec<String> = runner.archive_calls().into_iter().map(|a| a[1].clone()).collect();
    assert_eq!(archives, ["second-done", "first-done"]);
    assert_eq!(runner.inner.calls_of("instructions"), 2);
    assert!(stream.worktree().join("openspec/changes/reverted").is_dir());
    assert!(stream.worktree().join("openspec/changes/landed").is_dir());
}

// OAR-FR-LZLW, WKS-FR-PZKD: a stream with no change to archive starts no CLI
// process, makes no archive commit, and merges as before.
#[test]
fn a_stream_with_no_change_to_archive_merges_as_before() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    let before = runner.inner.calls().len();
    let stream_before = tip(&fx, &stream.branch);

    let result = merge(&fx, &stream, commit_publication()).expect("merged");

    let StreamMergeResult::Merged { archived: list, .. } = result else { panic!("{result:?}") };
    assert!(list.is_empty());
    assert_eq!(runner.inner.calls().len(), before, "no CLI process");
    assert_eq!(tip(&fx, &stream.branch), stream_before, "no archive commit");
    assert!(fx.root().join("openspec/changes/add-logout").is_dir());
}

// WKS-FR-FOTC, OAR-FR-SEQU: a stream that holds nothing the base does not
// archives nothing, whatever its runs name.
#[test]
fn nothing_to_merge_archives_nothing() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = fx.stream("idle");
    std::fs::create_dir_all(stream.worktree().join("openspec/changes/add-logout")).unwrap();
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let before = runner.inner.calls().len();

    let result = merge(&fx, &stream, commit_publication()).expect("settled");

    assert_eq!(result, StreamMergeResult::NothingToMerge);
    assert_eq!(runner.inner.calls().len(), before);
}

// OAR-FR-MSOM, OAR-FR-LUSO, CST-FR-YKGS, CAR-FR-MKQC, CEP-FR-KBZY,
// NCP-FR-CYQJ, CMS-FR-ZILU: the records of the change move to
// `changes/<stream-id>/archive/<archivedAs>/` with the commit, so a change of
// the same name starts with none; a target that exists gets the lowest free
// suffix from `-2`.
#[test]
fn the_records_of_the_change_move_to_the_archive_folder() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    write_record(&fx, &stream, "add-logout", "statistics.jsonl", "{}\n");
    write_record(&fx, &stream, "add-logout", "comments/t1.jsonl", "{}\n");
    write_record(
        &fx,
        &stream,
        "add-logout",
        "proposals/p1.toml",
        &edit_proposal(&stream, "add-logout", "accepted", false),
    );
    write_record(&fx, &stream, "add-logout", "split.toml", "x = 1\n");
    let archive_root = records_store(&fx).path().join("changes").join(&stream.id).join("archive");
    std::fs::create_dir_all(archive_root.join(format!("{DATE}-add-logout"))).unwrap();
    std::fs::create_dir_all(archive_root.join(format!("{DATE}-add-logout-2"))).unwrap();

    merge(&fx, &stream, commit_publication()).expect("merged");

    let moved = archive_root.join(format!("{DATE}-add-logout-3"));
    for rel in ["statistics.jsonl", "comments/t1.jsonl", "proposals/p1.toml", "split.toml"] {
        assert!(moved.join(rel).is_file(), "{rel} moved");
    }
    let store = records_store(&fx);
    assert!(!store.path().join("changes").join(&stream.id).join("add-logout").exists());
    assert!(
        crate::change_proposals::list_proposals_impl(&store, &stream.id, "add-logout").is_empty()
    );
}

// OAR-FR-LUSO: a change with no store folder moves nothing, and the target
// rule picks the first free name.
#[test]
fn a_change_with_no_records_moves_nothing() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    merge(&fx, &stream, commit_publication()).expect("merged");
    let store = records_store(&fx);
    assert!(!store.path().join("changes").join(&stream.id).exists());
    let target = crate::openspec_archive::store_target(&store, &stream.id, "2026-11-11-x");
    assert_eq!(target, Path::new("changes").join(&stream.id).join("archive/2026-11-11-x"));
}

// OAR-FR-KSZI: a store move that fails is told with `store_move_failed`, does
// not refuse the merge, and leaves the folder and the commit where they are.
#[cfg(unix)]
#[test]
fn a_store_move_that_fails_does_not_refuse_the_merge() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    write_record(&fx, &stream, "add-logout", "statistics.jsonl", "{}\n");
    let folder = records_store(&fx).path().join("changes").join(&stream.id).join("add-logout");
    // A link in the tree is refused by the move under the filesystem policy.
    std::os::unix::fs::symlink(folder.join("statistics.jsonl"), folder.join("link")).unwrap();

    let result = merge(&fx, &stream, commit_publication()).expect("merged");

    assert!(matches!(&result, StreamMergeResult::Merged { archived, .. } if archived.len() == 1));
    assert!(folder.join("statistics.jsonl").is_file(), "the folder stays");
    assert_eq!(
        fx.repo().find_commit(tip(&fx, &stream.branch)).unwrap().summary().ok().flatten(),
        Some("Archive OpenSpec change add-logout")
    );
    let warned: Vec<_> = stream_records(&stream)
        .into_iter()
        .filter(|r| r.fields.get("code") == Some(&serde_json::json!("store_move_failed")))
        .collect();
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert_eq!(warned[0].level, crate::logging::LogLevel::Warn);
    assert_eq!(warned[0].fields["stream_id"], stream.id.as_str());
    assert_eq!(warned[0].fields["change"], "add-logout");
}

// OAR-FR-SOFQ, WKS-FR-ZLWT, OAR-FR-OQVF: a conflict merge names the archived
// changes in `conflicted`, and the archive commit stays on the stream branch
// while the base branch does not move.
#[test]
fn a_conflict_merge_names_the_archived_changes() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    fx.allow_execution();
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    std::fs::write(stream.worktree().join("README.md"), "stream side\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "stream edit");
    std::fs::write(fx.root().join("README.md"), "base side\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "base edit");
    let base_before = tip(&fx, &stream.base_branch);

    let result = merge(&fx, &stream, commit_publication()).expect("handed off");

    let StreamMergeResult::Conflicted { archived: list, conflicted_paths, run_id } = result else {
        panic!("{result:?}");
    };
    assert_eq!(list, vec![archived("add-logout")]);
    assert_eq!(conflicted_paths, ["README.md"]);
    assert_eq!(tip(&fx, &stream.base_branch), base_before);
    let run = fx.reload(&run_id);
    assert_eq!(
        run.merge.unwrap().stream_tip,
        tip(&fx, &stream.branch).to_string(),
        "the run pins the stream tip with its archive commit"
    );
    assert_eq!(
        fx.repo().find_commit(tip(&fx, &stream.branch)).unwrap().summary().ok().flatten(),
        Some("Archive OpenSpec change add-logout")
    );
}

// OAR-FR-VSSH, WKS-FR-MWYD: while the archive runs, the one operation in
// flight is `Merging <stream name>`; the step reports none of its own.
#[test]
fn the_archive_step_reports_no_operation_of_its_own() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (app, into) = (fx.app.clone(), seen.clone());
    *runner.during_archive.lock().unwrap() = Some(Box::new(move || {
        let registry = app.state::<crate::progress::ProgressRegistry>();
        into.lock().unwrap().extend(registry.in_flight().into_iter().map(|op| op.label));
    }));

    merge(&fx, &stream, commit_publication()).expect("merged");

    assert_eq!(*seen.lock().unwrap(), ["Merging editor"]);
}

// OPU-FR-JBIU, OAR-FR-GTGQ: an apply run at rest on a plan update holds its
// stream, so the merge is refused with `stream_busy` and archives nothing.
#[test]
fn a_plan_rest_refuses_the_merge_before_the_archive_step() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout", "add-sso"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let mut resting = fx.enqueue(&stream, &crate::graduation::apply_prompt("add-sso"));
    resting.openspec = Some(RunOpenSpec::of_phase(crate::graduation::PHASE_APPLY, "add-sso"));
    resting.state = GraduationRunState::AwaitingAuthor;
    save_run(&fx.app, &mut resting).unwrap();
    let before = tip(&fx, &stream.branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, crate::streams::ERR_STREAM_BUSY);
    assert_eq!(tip(&fx, &stream.branch), before);
    assert!(runner.archive_calls().is_empty());
    assert_eq!(runner.inner.calls_of("instructions"), 0);
}

// WKS-FR-BPGM, WKS-FR-RAOM, OAR-FR-OQVF, OAR-FR-SOFQ: a conflict merge that
// the image preflight refuses carries the refusal code alone, writes nothing
// beyond the archive commit, and the commit stays on the stream branch.
#[test]
fn a_preflight_refusal_after_an_archive_keeps_the_archive_commit() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    std::fs::write(stream.worktree().join("README.md"), "stream side\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "stream edit");
    std::fs::write(fx.root().join("README.md"), "base side\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "base edit");
    let stream_before = tip(&fx, &stream.branch);
    let base_before = tip(&fx, &stream.base_branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, crate::graduation::ERR_VENDOR_EXECUTION_UNSUPPORTED);
    assert_eq!(tip(&fx, &stream.base_branch), base_before);
    assert_eq!(
        messages_since(&fx, &stream.branch, stream_before),
        ["Archive OpenSpec change add-logout"]
    );
    assert!(dirty(&stream.worktree()).is_empty());
    assert!(dirty(&fx.root()).is_empty());
    let key = fx.project_key();
    let fs = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    assert!(read_queue_index(&fs, &base, &key).runs.iter().all(|entry| !entry.merge));
}

// OAR-FR-OQVF, OAR-FR-KGDP, OAR-FR-SOFQ: after a conflict merge run that
// archived a change is discarded, the archive commit stays on the stream, and
// the next merge carries it, archives nothing again, and names nothing.
#[test]
fn a_merge_after_a_discarded_merge_run_does_not_archive_again() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    fx.allow_execution();
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    std::fs::write(stream.worktree().join("README.md"), "stream side\n").unwrap();
    super::change_runs::commit_all(&stream.worktree(), "stream edit");
    std::fs::write(fx.root().join("README.md"), "base side\n").unwrap();
    super::change_runs::commit_all(&fx.root(), "base edit");
    let result = merge(&fx, &stream, commit_publication()).expect("handed off");
    let StreamMergeResult::Conflicted { run_id, archived: first, .. } = result else {
        panic!("{result:?}");
    };
    assert_eq!(first, vec![archived("add-logout")]);
    let archive_tip = tip(&fx, &stream.branch);
    crate::graduation::discard_graduation_run(fx.app.clone(), run_id).expect("discarded");

    let again = merge(&fx, &stream, commit_publication()).expect("handed off again");

    let StreamMergeResult::Conflicted { archived: second, run_id, .. } = again else {
        panic!("{again:?}");
    };
    assert!(second.is_empty());
    assert_eq!(runner.archive_calls().len(), 1, "one archive in both merges");
    assert_eq!(tip(&fx, &stream.branch), archive_tip, "the archive commit stays");
    assert_eq!(fx.reload(&run_id).merge.unwrap().stream_tip, archive_tip.to_string());
}

// OAR-FR-SOFQ, OAR-FR-LZLS: a clean merge with the `uncommitted`
// publication names the archived change, makes no merge commit, and the base
// worktree holds the archive and the live spec as uncommitted work.
#[test]
fn an_uncommitted_merge_names_the_archived_change() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let base_before = tip(&fx, &stream.base_branch);

    let result =
        merge(&fx, &stream, crate::streams::StreamMergePublication::Uncommitted).expect("merged");

    let StreamMergeResult::Merged { archived: list, commit, .. } = result else {
        panic!("{result:?}");
    };
    assert_eq!(list, vec![archived("add-logout")]);
    assert_eq!(commit, None);
    assert_eq!(tip(&fx, &stream.base_branch), base_before);
    let root = fx.root();
    assert!(root.join(format!("openspec/changes/archive/{DATE}-add-logout/tasks.md")).is_file());
    assert!(!root.join("openspec/changes/add-logout").exists());
    assert!(std::fs::read_to_string(root.join("openspec/specs/auth/spec.md"))
        .unwrap()
        .contains("From add-logout"));
    assert!(!dirty(&root).is_empty(), "the merge is uncommitted work");
}
