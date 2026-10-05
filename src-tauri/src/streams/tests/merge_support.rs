//! What the merge tests share: a way to see everything a merge request could
//! write, and the streams the scenarios start from.

use std::collections::BTreeMap;

use super::*;

/// Everything a refused or settled request could have written, read back.
///
/// Two of these that are equal prove that the request wrote nothing: not a ref,
/// not a file of either working copy, not an index entry, not a registration,
/// and not a file of the application's store.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Observed {
    /// Every reference with the object it names.
    pub refs: BTreeMap<String, String>,
    /// The linked worktrees the repository registers.
    pub worktrees: Vec<String>,
    /// The files of the base worktree with their content.
    pub base_files: BTreeMap<String, String>,
    /// The files of the stream's working copy with their content.
    pub stream_files: BTreeMap<String, String>,
    /// What Git reports about each working copy, index included.
    pub base_status: Vec<(String, u32)>,
    pub stream_status: Vec<(String, u32)>,
    /// Every file of the application's store.
    pub store_files: Vec<String>,
}

fn status_of(repo: &git2::Repository) -> Vec<(String, u32)> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let mut out: Vec<(String, u32)> = repo
        .statuses(Some(&mut options))
        .expect("statuses")
        .iter()
        .map(|entry| {
            (
                entry.path().unwrap_or_default().to_string(),
                entry.status().bits(),
            )
        })
        .collect();
    out.sort();
    out
}

fn files_of(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == ".git" || rel.starts_with(".git/") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(rel, std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    out
}

/// Every file under a directory, named relative to it.
pub(super) fn names_under(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// Read the whole observable state around one stream.
pub(super) fn observe(fx: &Fixture, stream: &WorkStream) -> Observed {
    let repo = fx.repo();
    let mut refs = BTreeMap::new();
    for reference in repo.references().expect("references").flatten() {
        let name = reference.name().unwrap_or_default().to_string();
        let target = reference
            .peel_to_commit()
            .map(|commit| commit.id().to_string())
            .unwrap_or_else(|_| format!("{:?}", reference.target()));
        refs.insert(name, target);
    }
    let stream_repo = git2::Repository::open(&stream.worktree_path).ok();
    Observed {
        refs,
        worktrees: worktree_names(&repo),
        base_files: files_of(&fx.root()),
        stream_files: files_of(Path::new(&stream.worktree_path)),
        base_status: status_of(&repo),
        stream_status: stream_repo.as_ref().map(status_of).unwrap_or_default(),
        store_files: names_under(&canonical(fx.store.path())),
    }
}

/// A request left no run: the order names none, and the store holds no run
/// directory.
pub(super) fn assert_no_run_anywhere(fx: &Fixture) {
    let fs = fx
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("instance");
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    assert!(
        crate::graduation::read_queue_index(&fs, &base, &key)
            .runs
            .is_empty(),
        "the run order holds a run"
    );
    let runs: Vec<String> = names_under(&canonical(fx.store.path()))
        .into_iter()
        .filter(|name| name.ends_with("run.toml") || name.contains("/logs/"))
        .collect();
    assert!(runs.is_empty(), "the store holds run files: {runs:?}");
}

/// A stream that is ahead of its base by one commit that adds `added.txt`.
pub(super) fn stream_ahead(fx: &Fixture, name: &str) -> WorkStream {
    let stream = fx.create(name, None).expect("created");
    let worktree = git2::Repository::open(&stream.worktree_path).expect("stream repo");
    std::fs::write(Path::new(&stream.worktree_path).join("added.txt"), "work\n").unwrap();
    commit_all(&worktree, "stream work");
    stream
}

/// Write a file into a working copy and commit everything there.
pub(super) fn commit_file(repo: &git2::Repository, path: &str, content: &str, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    if let Some(parent) = workdir.join(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(workdir.join(path), content).unwrap();
    commit_all(repo, message);
}

/// Remove a file from a working copy and commit the removal.
pub(super) fn delete_file(repo: &git2::Repository, path: &str, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    std::fs::remove_file(workdir.join(path)).unwrap();
    // `add_all` stages no removal, so the removal is staged here.
    let mut index = repo.index().expect("index");
    index.remove_path(Path::new(path)).expect("removed from the index");
    index.write().expect("write");
    commit_all(repo, message);
}

/// The stream's own repository handle.
pub(super) fn stream_repo(stream: &WorkStream) -> git2::Repository {
    git2::Repository::open(&stream.worktree_path).expect("stream repo")
}

/// The revision a branch names, as text.
pub(super) fn tip_text(repo: &git2::Repository, branch: &str) -> String {
    tip_of(repo, branch).to_string()
}

/// A stream whose branch and whose base branch each changed the named files in
/// ways Git cannot merge, as the three kinds of conflict Git reports.
///
/// - `README.md`: both sides changed the same line.
/// - `gone.txt`: the base deleted it and the stream changed it.
/// - `both-new.txt`: both sides added it with different content.
/// The stream also adds `stream-only.txt`, and the base adds `base-only.txt`,
/// which Git merges without help.
pub(super) fn stream_with_three_conflicts(fx: &Fixture) -> WorkStream {
    commit_file(&fx.repo(), "gone.txt", "to be deleted\nline two\n", "add gone");
    let stream = fx.create("three conflicts", None).expect("created");
    let worktree = stream_repo(&stream);
    commit_file(&worktree, "README.md", "stream side\n", "stream readme");
    commit_file(&worktree, "gone.txt", "changed by the stream\nline two\n", "stream gone");
    commit_file(&worktree, "both-new.txt", "stream new\n", "stream new");
    commit_file(&worktree, "stream-only.txt", "kept\n", "stream only");
    let base = fx.repo();
    commit_file(&base, "README.md", "base side\n", "base readme");
    delete_file(&base, "gone.txt", "base deletes gone");
    commit_file(&base, "both-new.txt", "base new\n", "base new");
    commit_file(&base, "base-only.txt", "from the base\n", "base only");
    stream
}

/// A snapshot of the stream's merge, captured as the handoff captures it, with
/// the run id it was kept under.
pub(super) fn capture_snapshot(
    fx: &Fixture,
    stream: &WorkStream,
    run_id: &str,
) -> merge_snapshot::Snapshot {
    let repo = fx.repo();
    merge_snapshot::capture(
        &repo,
        tip_of(&repo, &stream.base_branch),
        tip_of(&repo, &stream.branch),
        run_id,
    )
    .expect("a snapshot")
}

/// The merge data a run would carry for this snapshot.
pub(super) fn merge_data_for(
    fx: &Fixture,
    stream: &WorkStream,
    snapshot: &merge_snapshot::Snapshot,
    publication: StreamMergePublication,
) -> crate::graduation::GraduationMergeData {
    let repo = fx.repo();
    crate::graduation::GraduationMergeData {
        name: format!("Merge {}", stream.name),
        stream_branch: stream.branch.clone(),
        base_branch: stream.base_branch.clone(),
        base_tip: tip_text(&repo, &stream.base_branch),
        stream_tip: tip_text(&repo, &stream.branch),
        merge_base: snapshot.merge_base.to_string(),
        snapshot_commit: snapshot.commit.to_string(),
        publication,
        changed_paths: snapshot.changed_paths.clone(),
        unresolved_paths: snapshot.unresolved_paths.clone(),
        conflicts: snapshot.conflicts.clone(),
        result: None,
    }
}

/// The content of one path in a commit, if the commit holds it.
pub(super) fn blob_text(repo: &git2::Repository, commit: git2::Oid, path: &str) -> Option<String> {
    let tree = repo.find_commit(commit).ok()?.tree().ok()?;
    let entry = tree.get_path(Path::new(path)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// The snapshot's tree with some top-level paths written over: a `Some` writes
/// that content and a `None` removes the path. This is the tree a merge worktree
/// settles into once a turn has resolved the conflicts.
pub(super) fn settled_tree(
    repo: &git2::Repository,
    snapshot: git2::Oid,
    edits: &[(&str, Option<&str>)],
) -> git2::Oid {
    let tree = repo.find_commit(snapshot).unwrap().tree().unwrap();
    let mut builder = repo.treebuilder(Some(&tree)).expect("a builder");
    for (path, content) in edits {
        match content {
            Some(content) => {
                let blob = repo.blob(content.as_bytes()).unwrap();
                builder.insert(path, blob, 0o100644).unwrap();
            }
            None => {
                let _ = builder.remove(path);
            }
        }
    }
    builder.write().expect("a tree")
}

/// A run of a stream in the named state, on the production record shape.
pub(super) fn save_plain_run(
    fx: &Fixture,
    stream: &WorkStream,
    state: crate::graduation::GraduationRunState,
) -> crate::graduation::GraduationRun {
    use crate::graduation::*;
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    let mut run = GraduationRun {
        id: new_run_id(),
        stream_id: stream.id.clone(),
        stream_name: stream.name.clone(),
        direct_target: None,
        target_hold: None,
        project_key: key,
        state,
        standing_work: StandingWork::default(),
        standing_work_message: None,
        standing_work_outcome: None,
        input: CapturedGraduationInput {
            draft_id: "d1".into(),
            draft_name: "a draft".into(),
            prompt: "do the work".into(),
            prompt_checksum: crate::fs::sha256_bytes(b"do the work"),
            captured_at: "t0".into(),
        },
        base_commit: None,
        commits: Vec::new(),
        auto_start: true,
        archived: false,
        archived_at: None,
        work_turns: 0,
        review_turns: 0,
        logs: GraduationLogIndexes::default(),
        checkpoint: GraduationCheckpoint::default(),
        observability: GraduationObservability::default(),
        escalation: None,
        blocker: None,
        interruption: None,
        restarted_from_run_id: None,
        failure: None,
        merge: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    save_run(&fx.app, &mut run).expect("a saved run");
    run
}

/// Write raw bytes into a working copy and commit everything there.
pub(super) fn commit_bytes(repo: &git2::Repository, path: &str, bytes: &[u8], message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    if let Some(parent) = workdir.join(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(workdir.join(path), bytes).unwrap();
    commit_all(repo, message);
}

/// Put a symbolic link at a path of a working copy, replacing what stands there,
/// and commit everything there.
#[cfg(unix)]
pub(super) fn commit_symlink(repo: &git2::Repository, path: &str, target: &str, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    let link = workdir.join(path);
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(target, &link).unwrap();
    commit_all(repo, message);
}

/// Record a submodule entry (a gitlink) at a path, naming `commit`, and commit
/// it. The directory a submodule stands in is made, empty, so the working copy
/// stays clean.
pub(super) fn commit_gitlink(repo: &git2::Repository, path: &str, commit: git2::Oid, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    std::fs::create_dir_all(workdir.join(path)).unwrap();
    let mut index = repo.index().expect("index");
    index
        .add(&git2::IndexEntry {
            ctime: git2::IndexTime::new(0, 0),
            mtime: git2::IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o160000,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: commit,
            flags: 0,
            flags_extended: 0,
            path: path.as_bytes().to_vec(),
        })
        .expect("the gitlink is staged");
    index.write().expect("write");
    let tree = repo
        .find_tree(index.write_tree().expect("tree"))
        .expect("tree");
    let signature = repo.signature().expect("signature");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .expect("commit");
}

/// Move a file inside a working copy and commit the move.
pub(super) fn rename_file(repo: &git2::Repository, from: &str, to: &str, message: &str) {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    std::fs::rename(workdir.join(from), workdir.join(to)).unwrap();
    let mut index = repo.index().expect("index");
    index.remove_path(Path::new(from)).expect("removed from the index");
    index.write().expect("write");
    commit_all(repo, message);
}

/// Every reference of the repository that stands in the private snapshot
/// namespace.
pub(super) fn snapshot_refs(repo: &git2::Repository) -> Vec<String> {
    repo.references_glob("refs/synthesis/merge/*")
        .expect("references")
        .flatten()
        .filter_map(|reference| reference.name().ok().map(str::to_string))
        .collect()
}

/// The bytes a commit holds at a path.
pub(super) fn blob_bytes(repo: &git2::Repository, commit: git2::Oid, path: &str) -> Option<Vec<u8>> {
    let tree = repo.find_commit(commit).ok()?.tree().ok()?;
    let entry = tree.get_path(Path::new(path)).ok()?;
    Some(repo.find_blob(entry.id()).ok()?.content().to_vec())
}
