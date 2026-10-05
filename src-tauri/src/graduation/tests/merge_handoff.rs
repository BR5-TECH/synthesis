//! The handoff of a conflict merge to a merge run, and what the run owns until
//! it ends (`WKS-work-streams.md` WKS-FR-ZLWT, WKS-FR-BPGM,
//! `GRB-graduation-rebase.md` GRB-FR-DZQE … GRB-FR-FPYA, GRB-FR-WDHU,
//! `GRD-graduation.md` GRD-FR-MRNQ, GRD-FR-VCTH, GRD-FR-KZPT).

use std::collections::BTreeMap;

use super::*;
use crate::streams::{StreamMergePublication, StreamMergeResult};

/// Write one file into a working copy and commit everything there.
pub(super) fn commit_file(repo: &git2::Repository, path: &str, content: &str, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    std::fs::write(workdir.join(path), content).unwrap();
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    index.update_all(["*"].iter(), None).expect("update");
    index.write().expect("write");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
    let signature = repo.signature().expect("signature");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .expect("commit");
}

/// A stream and a base branch in conflict over three paths, with one clean
/// change on each side.
///
/// - `README.md`: both sides changed the same line.
/// - `gone.txt`: the stream deleted it and the base changed it.
/// - `both-new.txt`: both sides added it with different content.
/// - `stream-only.txt` and `base-only.txt`: Git merges them alone.
pub(super) fn conflicted_stream(fx: &Fixture) -> crate::streams::WorkStream {
    commit_file(&fx.repo(), "gone.txt", "to be removed\nline two\n", "add gone");
    let stream = fx.stream("feature");
    let stream_repo = git2::Repository::open(stream.worktree()).expect("the stream's repo");
    commit_file(&stream_repo, "README.md", "stream side\n", "stream readme");
    std::fs::remove_file(stream.worktree().join("gone.txt")).unwrap();
    commit_file(&stream_repo, "both-new.txt", "stream new\n", "stream new and delete");
    commit_file(&stream_repo, "stream-only.txt", "kept\n", "stream only");
    let base = fx.repo();
    commit_file(&base, "README.md", "base side\n", "base readme");
    commit_file(&base, "gone.txt", "changed by the base\nline two\n", "base gone");
    commit_file(&base, "both-new.txt", "base new\n", "base new");
    commit_file(&base, "base-only.txt", "from the base\n", "base only");
    stream
}

/// Everything a handoff must leave as it was, read back.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Live {
    /// Every reference outside the private namespace and the run's own.
    refs: BTreeMap<String, String>,
    base_files: BTreeMap<String, String>,
    stream_files: BTreeMap<String, String>,
    base_status: Vec<(String, u32)>,
    stream_status: Vec<(String, u32)>,
}

fn status_of(repo: &git2::Repository) -> Vec<(String, u32)> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let mut out: Vec<(String, u32)> = repo
        .statuses(Some(&mut options))
        .expect("statuses")
        .iter()
        .map(|entry| (entry.path().unwrap_or_default().to_string(), entry.status().bits()))
        .collect();
    out.sort();
    out
}

pub(super) fn live(fx: &Fixture, stream: &crate::streams::WorkStream) -> Live {
    let repo = fx.repo();
    let mut refs = BTreeMap::new();
    for reference in repo.references().expect("references").flatten() {
        let name = reference.name().unwrap_or_default().to_string();
        if name.starts_with("refs/synthesis/") || name.contains("merge-run") {
            continue;
        }
        let target = reference
            .peel_to_commit()
            .map(|commit| commit.id().to_string())
            .unwrap_or_default();
        refs.insert(name, target);
    }
    let stream_repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    Live {
        refs,
        base_files: tree_under(&fx.root()),
        stream_files: tree_under(&stream.worktree()),
        base_status: status_of(&repo),
        stream_status: status_of(&stream_repo),
    }
}

pub(super) fn tip(repo: &git2::Repository, branch: &str) -> git2::Oid {
    repo.find_branch(branch, git2::BranchType::Local)
        .expect("branch")
        .get()
        .peel_to_commit()
        .expect("commit")
        .id()
}

/// The request, with the queue not offered a start: a test that drives the run
/// drives it with the agent scripted.
pub(super) fn request(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    publication: StreamMergePublication,
) -> Result<StreamMergeResult, String> {
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let result = crate::streams::merge_work_stream_blocking(&fx.app, &stream.id, publication);
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    result
}

pub(super) fn handed_off(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    publication: StreamMergePublication,
) -> (String, Vec<String>) {
    match request(fx, stream, publication).expect("the check settled") {
        StreamMergeResult::Conflicted {
            run_id,
            conflicted_paths,
        } => (run_id, conflicted_paths),
        other => panic!("expected a handoff, got {other:?}"),
    }
}

pub(super) fn index_entries(fx: &Fixture) -> Vec<QueueIndexEntry> {
    let fs = fx.app.state::<crate::fs::FsAccessState>().get().expect("instance");
    let base = crate::graduation::store_base(&fx.app).expect("a store");
    read_queue_index(&fs, &base, &fx.project_key()).runs
}

pub(super) fn snapshot_ref_stands(fx: &Fixture, run_id: &str) -> bool {
    fx.repo()
        .find_reference(&format!("refs/synthesis/merge/{run_id}"))
        .is_ok()
}

pub(super) fn registered(fx: &Fixture, name: &str) -> bool {
    fx.repo()
        .worktrees()
        .map(|list| list.iter().flatten().flatten().any(|registered| registered == name))
        .unwrap_or(false)
}

// WKS-FR-ZLWT, WKS-FR-QNHF, WKS-FR-RAOM, GRB-FR-FPYA, GRB-FR-UHFE, GRB-FR-RMQC:
// a conflict answers `conflicted` with the run and the unresolved paths, and
// leaves both branches and both working copies byte-identical. The private ref
// is the one thing the branches' repository gains, and no merge worktree exists
// yet.
#[test]
fn a_conflict_hands_off_and_leaves_both_branches_and_working_copies_as_they_were() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let before = live(&fx, &stream);

    let (run_id, conflicted) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);

    assert_eq!(conflicted, vec!["README.md", "both-new.txt", "gone.txt"]);
    assert_eq!(live(&fx, &stream), before);
    assert!(snapshot_ref_stands(&fx, &run_id));
    assert!(!registered(&fx, &format!("{run_id}-mw")), "no merge worktree yet");
    assert!(!crate::graduation::store_base(&fx.app).unwrap().merge_worktree(&run_id).exists());
    assert!(
        !fx.state_of_stream_is_busy(&stream),
        "the handoff claims nothing: a dispatch does"
    );
}

impl Fixture {
    fn state_of_stream_is_busy(&self, stream: &crate::streams::WorkStream) -> bool {
        crate::streams::get_work_stream(self.app.clone(), stream.id.clone())
            .expect("the stream")
            .busy_run_id
            .is_some()
    }
}

// GRD-FR-MRNQ, GRD-FR-VCTH, GRD-FR-KZPT, WKS-FR-VQDE, WKS-FR-ZLWT, GRB-FR-FPYA:
// the run the handoff creates is queued, carries the merge data, holds no draft
// and no standing work, and is the only durable record.
#[test]
fn the_handoff_creates_one_queued_merge_run_with_the_whole_merge_data() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let repo = fx.repo();
    let base_tip = tip(&repo, &stream.base_branch);
    let stream_tip = tip(&repo, &stream.branch);

    let (run_id, _) = handed_off(
        &fx,
        &stream,
        StreamMergePublication::Commit {
            message: "Land feature".into(),
        },
    );
    let run = fx.reload(&run_id);

    assert_eq!(run.state, GraduationRunState::Queued);
    assert!(run.is_merge());
    assert_eq!(run.stream_id, stream.id);
    assert_eq!(run.stream_name, "feature");
    let data = run.merge.as_ref().expect("merge data");
    assert_eq!(data.name, "Merge feature");
    assert_eq!(data.stream_branch, stream.branch);
    assert_eq!(data.base_branch, stream.base_branch);
    assert_eq!(data.base_tip, base_tip.to_string());
    assert_eq!(data.stream_tip, stream_tip.to_string());
    assert_eq!(data.merge_base, repo.merge_base(base_tip, stream_tip).unwrap().to_string());
    assert_eq!(
        data.publication,
        StreamMergePublication::Commit {
            message: "Land feature".into()
        }
    );
    // The Git-clean path is a changed path and an unresolved one is both.
    assert_eq!(
        data.changed_paths,
        vec!["README.md", "both-new.txt", "gone.txt", "stream-only.txt"]
    );
    assert_eq!(data.unresolved_paths, vec!["README.md", "both-new.txt", "gone.txt"]);
    let sides: Vec<(&str, &str, &str)> = data
        .conflicts
        .iter()
        .map(|c| (c.path.as_str(), c.base_change.as_str(), c.stream_change.as_str()))
        .collect();
    assert_eq!(
        sides,
        vec![
            ("README.md", "updated", "updated"),
            ("both-new.txt", "created", "created"),
            ("gone.txt", "updated", "deleted"),
        ]
    );
    assert!(data.result.is_none());

    // No draft, no standing work, no direct target, no base commit yet.
    assert_eq!(run.input.draft_id, "");
    assert_eq!(run.input.draft_name, "");
    assert!(!run.input.prompt.is_empty());
    assert_eq!(run.input.prompt_checksum, crate::fs::sha256_bytes(run.input.prompt.as_bytes()));
    assert_eq!(run.standing_work, StandingWork::Keep);
    assert!(run.direct_target.is_none());
    assert!(run.base_commit.is_none());
    assert!(run.commits.is_empty());

    // The one durable record: the order names it as a merge run.
    let entries = index_entries(&fx);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].merge);
    assert_eq!(entries[0].run_id, run_id);
}

// GRB-FR-DZQE, GRB-FR-RMQC, GRD-FR-KZPT: the snapshot commit the run names has
// the pinned base tip and the pinned stream tip as parents, markers in the
// unresolved paths and the clean merge in the rest, and the private ref names it.
#[test]
fn the_snapshot_the_run_names_has_two_parents_and_markers_where_git_stopped() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let repo = fx.repo();
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let data = fx.reload(&run_id).merge.expect("merge data");

    let snapshot = git2::Oid::from_str(&data.snapshot_commit).unwrap();
    let commit = repo.find_commit(snapshot).unwrap();
    assert_eq!(
        commit.parent_ids().map(|id| id.to_string()).collect::<Vec<_>>(),
        vec![data.base_tip.clone(), data.stream_tip.clone()]
    );
    let blob = |path: &str| -> Option<String> {
        let entry = commit.tree().unwrap().get_path(Path::new(path)).ok()?;
        let blob = repo.find_blob(entry.id()).ok()?;
        Some(String::from_utf8_lossy(blob.content()).into_owned())
    };
    let readme = blob("README.md").expect("README.md");
    assert!(readme.contains("<<<<<<< ") && readme.contains("||||||| ") && readme.contains(">>>>>>> "));
    assert!(readme.contains("base side") && readme.contains("stream side"));
    // A delete-against-modify path is a marked file naming the deleted side.
    let gone = blob("gone.txt").expect("gone.txt");
    assert!(gone.contains("<<<<<<< ") && gone.contains("(this path was deleted on this side)"), "{gone}");
    assert!(gone.contains("changed by the base"), "{gone}");
    // An add/add path is marked with both contents.
    let both = blob("both-new.txt").expect("both-new.txt");
    assert!(both.contains("<<<<<<< ") && both.contains("base new") && both.contains("stream new"));
    // The clean paths carry no marker.
    assert_eq!(blob("stream-only.txt").as_deref(), Some("kept\n"));
    assert_eq!(blob("base-only.txt").as_deref(), Some("from the base\n"));
    assert_eq!(
        repo.find_reference(&format!("refs/synthesis/merge/{run_id}"))
            .unwrap()
            .target(),
        Some(snapshot)
    );
}

// GRB-FR-SRVN: a path the stream deleted and the base changed is unresolved too.
#[test]
fn a_path_the_stream_deleted_and_the_base_changed_is_unresolved() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);

    let (run_id, conflicted) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);

    assert!(conflicted.contains(&"gone.txt".to_string()));
    let data = fx.reload(&run_id).merge.unwrap();
    let gone = data.conflicts.iter().find(|c| c.path == "gone.txt").unwrap();
    assert_eq!((gone.base_change.as_str(), gone.stream_change.as_str()), ("updated", "deleted"));
}

// WKS-FR-BPGM, GRB-FR-MSNP, GRB-FR-UHFE: the image preflight is asked before
// anything is built. Each refusal is typed, creates no run, leaves no snapshot
// ref and no worktree, and moves nothing.
#[test]
fn a_refused_preflight_creates_no_run_and_leaves_no_snapshot_ref() {
    use crate::agentic::{AgenticRecord, PathOrigin};
    let fx = Fixture::new();
    let stream = conflicted_stream(&fx);
    let store = fx.app.state::<crate::global_settings::GlobalSettingsStore>();

    let refused = |fx: &Fixture, before: &Live| -> String {
        let refusal = request(fx, &stream, StreamMergePublication::Uncommitted).expect_err("refused");
        assert!(index_entries(fx).is_empty(), "a run was created");
        assert_eq!(&live(fx, &stream), before);
        let snapshot_refs = fx
            .repo()
            .references_glob("refs/synthesis/merge/*")
            .unwrap()
            .count();
        assert_eq!(snapshot_refs, 0, "a snapshot ref stands");
        assert!(fx.repo().worktrees().unwrap().iter().flatten().flatten().all(|n| !n.ends_with("-mw")));
        refusal
    };

    assert_eq!(refused(&fx, &live(&fx, &stream)), ERR_VENDOR_EXECUTION_UNSUPPORTED, "no vendor configured");

    store
        .save_agentic_registry(
            vec![AgenticRecord {
                vendor: "claude_code".into(),
                binary_path: Some("/opt/agents/claude".into()),
                path_origin: PathOrigin::Detected,
                ..AgenticRecord::default()
            }],
            Some("claude_code".into()),
        )
        .expect("a registry");
    assert_eq!(refused(&fx, &live(&fx, &stream)), ERR_VENDOR_IMAGE_UNCONFIGURED, "no image committed");

    let access = fx.app.state::<crate::fs::FsAccessState>().get().expect("instance");
    crate::project_settings::save_project_vendor_image_to(
        &crate::fs::RootFs::new(fx.root(), access),
        "claude_code",
        &crate::project_settings::images::ProjectVendorImage {
            image_name: "acme/agent".into(),
            tag: Some("latest".into()),
            dockerfile: None,
        },
    )
    .expect("an image");
    // The setting is a file of the base worktree, so it is committed like the
    // author's own work: an untracked file would make the base dirty.
    commit_file(&fx.repo(), "image-setting.txt", "committed\n", "commit the image setting");
    assert_eq!(refused(&fx, &live(&fx, &stream)), ERR_DOCKER_BACKEND_UNVERIFIED, "no verified backend");
}

// WKS-FR-GKPX, WKS-FR-VQDE: a second request while the merge run has not ended
// is refused `stream_busy` and creates no second run.
#[test]
fn a_second_merge_while_the_run_has_not_ended_is_refused() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let before = live(&fx, &stream);

    let refusal = request(&fx, &stream, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, crate::streams::ERR_STREAM_BUSY);
    assert_eq!(live(&fx, &stream), before);
    assert_eq!(index_entries(&fx).len(), 1);
    assert!(snapshot_ref_stands(&fx, &run_id));
}

// WKS-FR-ZLWT, GRD-FR-DLWB: the guard is released after the handoff, and the run
// stands in the stream's own queue like any run.
#[test]
fn the_guard_is_released_and_the_run_stands_in_the_streams_queue() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);

    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);

    let state = fx.app.state::<crate::streams::StreamState>();
    assert!(
        state
            .begin_repository_update(&stream.id, crate::streams::Reconciliation::Update)
            .is_ok(),
        "the guard is free after the handoff"
    );
    let queue = crate::graduation::project_queue(&fx.app).expect("a queue");
    let queued: Vec<&str> = crate::graduation::queue_of(&queue, &stream.id)
        .iter()
        .map(|run| run.id.as_str())
        .collect();
    assert_eq!(queued, vec![run_id.as_str()]);
}

/// The merge worktree of a run, made as the first dispatch makes it.
pub(super) fn merge_worktree_of(fx: &Fixture, run_id: &str) -> PathBuf {
    let run = fx.reload(run_id);
    crate::graduation::driver::merge_workspace::ensure(&fx.app, &run).expect("a merge worktree")
}

// GRB-FR-KOGU, GRD-FR-KZPT: the merge worktree stands at
// `short_data_dir()/g/<run-id>/mw/`, is registered as `<run-id>-mw`, is seeded
// from the snapshot, and writes neither live branch nor live worktree.
#[test]
fn the_merge_worktree_is_seeded_from_the_snapshot_and_touches_nothing_live() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let before = live(&fx, &stream);

    let worktree = merge_worktree_of(&fx, &run_id);

    assert_eq!(worktree, crate::graduation::store_base(&fx.app).unwrap().merge_worktree(&run_id));
    assert!(worktree.ends_with(format!("g/{run_id}/mw")) || worktree.to_string_lossy().ends_with(&format!("{run_id}/mw")));
    assert!(registered(&fx, &format!("{run_id}-mw")));
    assert!(fx
        .repo()
        .find_branch(&format!("synthesis/merge-run/{run_id}"), git2::BranchType::Local)
        .is_ok());
    let data = fx.reload(&run_id).merge.unwrap();
    let checkout = git2::Repository::open(&worktree).expect("the worktree");
    assert_eq!(
        checkout.head().unwrap().peel_to_commit().unwrap().id().to_string(),
        data.snapshot_commit
    );
    let files = tree_under(&worktree);
    assert!(files["README.md"].contains("<<<<<<< "), "markers stand in the unresolved paths");
    assert_eq!(files["stream-only.txt"], "kept\n");
    assert_eq!(live(&fx, &stream), before, "neither live branch nor live worktree moved");
}

// GRB-FR-WDHU, GRB-FR-RMQC, GRD-FR-KZPT, GRD-FR-EWTN: discarding the run
// removes its merge worktree, its registration, its scratch branch and its
// snapshot ref, and writes neither live branch nor live worktree.
#[test]
fn discarding_a_merge_run_reclaims_everything_the_run_owned() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let worktree = merge_worktree_of(&fx, &run_id);
    let before = live(&fx, &stream);

    let discarded =
        crate::graduation::discard_graduation_run(fx.app.clone(), run_id.clone()).expect("discarded");

    assert_eq!(discarded.state, GraduationRunState::Discarded);
    assert!(!worktree.exists(), "the worktree directory is gone");
    assert!(!registered(&fx, &format!("{run_id}-mw")));
    assert!(fx
        .repo()
        .find_branch(&format!("synthesis/merge-run/{run_id}"), git2::BranchType::Local)
        .is_err());
    assert!(!snapshot_ref_stands(&fx, &run_id));
    assert_eq!(live(&fx, &stream), before);
}

// GRB-FR-WDHU, WKS-FR-ENRU, GRD-FR-PFMD: the first queue read after a launch
// reclaims what an ended merge run still owns, and leaves a run that rests in
// `awaiting_author` or `interrupted` its worktree and its snapshot ref.
#[test]
fn the_first_queue_read_reclaims_ended_runs_and_keeps_resting_ones() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (ended_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let ended_worktree = merge_worktree_of(&fx, &ended_id);
    let mut ended = fx.reload(&ended_id);
    // The application stopped between the write that ended the run and the
    // removal, so the run is terminal on disk and everything it owned stands.
    ended.state = GraduationRunState::Failed;
    crate::graduation::save_run(&fx.app, &mut ended).expect("saved");
    assert!(ended_worktree.exists());

    let resting = [GraduationRunState::AwaitingAuthor, GraduationRunState::Interrupted];
    let mut kept = Vec::new();
    for state in resting {
        // One run per resting state, on a stream of its own.
        let other = fx.stream(&format!("rest-{}", state.as_str()));
        let other_repo = git2::Repository::open(other.worktree()).unwrap();
        commit_file(&other_repo, "README.md", "stream\n", "stream");
        commit_file(&fx.repo(), "README.md", &format!("base {}\n", state.as_str()), "base");
        let (id, _) = handed_off(&fx, &other, StreamMergePublication::Uncommitted);
        let worktree = merge_worktree_of(&fx, &id);
        let mut run = fx.reload(&id);
        run.state = state;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");
        kept.push((id, worktree));
    }

    crate::graduation::list_graduation_queue(fx.app.clone()).expect("a queue");

    assert!(!ended_worktree.exists(), "the ended run's worktree is reclaimed");
    assert!(!registered(&fx, &format!("{ended_id}-mw")));
    assert!(!snapshot_ref_stands(&fx, &ended_id));
    for (id, worktree) in kept {
        assert!(worktree.exists(), "{id} keeps its worktree");
        assert!(registered(&fx, &format!("{id}-mw")));
        assert!(snapshot_ref_stands(&fx, &id), "{id} keeps its snapshot ref");
    }
}

// WKS-FR-DAKP, GRD-FR-PFMD, WKS-FR-ENRU: a merge run found `working` with no loop
// behind it is `interrupted` with the reason `execution_abandoned` on the first
// queue read, and keeps what it owns so that Continue can resume it.
#[test]
fn a_merge_run_found_working_with_no_loop_is_interrupted_and_keeps_its_worktree() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let (run_id, _) = handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let worktree = merge_worktree_of(&fx, &run_id);
    let mut run = fx.reload(&run_id);
    run.state = GraduationRunState::Working;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");

    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("a queue");

    let found = queue.runs.iter().find(|r| r.id == run_id).expect("the run");
    assert_eq!(found.state, GraduationRunState::Interrupted);
    let reloaded = fx.reload(&run_id);
    assert_eq!(
        reloaded.interruption.as_ref().map(|i| i.reason),
        Some(GraduationInterruptionReason::ExecutionAbandoned)
    );
    assert!(worktree.exists());
    assert!(snapshot_ref_stands(&fx, &run_id));
    let data = reloaded.merge.expect("merge data");
    assert_eq!(data.base_tip, tip(&fx.repo(), &stream.base_branch).to_string(), "neither branch moved");
}

// WKS-FR-YSUB, WKS-FR-GKPX: a direct run that targets a stream's working copy
// holds the stream as a stream run does, so a merge of that stream is refused
// with `stream_busy` while the run has not ended, and writes nothing.
#[test]
fn a_merge_is_refused_while_a_direct_run_targets_the_streams_working_copy() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let run = fx.direct_in(&stream.worktree(), &stream.branch, Some(&stream), "work", "d1");
    let before = live(&fx, &stream);

    let refusal = request(&fx, &stream, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, crate::streams::ERR_STREAM_BUSY);
    assert_eq!(live(&fx, &stream), before);
    assert!(index_entries(&fx).iter().all(|entry| !entry.merge));
    let mut ended = fx.reload(&run.id);
    ended.state = GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut ended).expect("saved");
    handed_off(&fx, &stream, StreamMergePublication::Uncommitted);
}
