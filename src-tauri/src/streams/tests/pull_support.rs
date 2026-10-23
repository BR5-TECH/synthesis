//! What the pull tests share: an upstream that the fixture's branch tracks, and
//! commits written on either side without a network.
//!
//! The remote-tracking ref is written directly. A reconcile makes no network
//! request (`GTC-git.md` GTC-FR-NJUY), so what it reads is what the last fetch
//! left, and writing that ref is the same as a fetch that brought the commit.

use super::*;

/// The branch the fixture's primary worktree has out.
pub(super) fn branch_of(fx: &Fixture) -> String {
    fx.repo().head().unwrap().shorthand().unwrap().to_string()
}

pub(super) fn head_of(fx: &Fixture) -> git2::Oid {
    fx.repo().head().unwrap().peel_to_commit().unwrap().id()
}

/// Make the fixture's branch track `origin/<branch>`, standing at its tip.
pub(super) fn track_origin(fx: &Fixture) {
    let repo = fx.repo();
    let branch = branch_of(fx);
    repo.remote("origin", "file:///nowhere/synthesis-pull-test").unwrap();
    let tip = head_of(fx);
    repo.reference(&format!("refs/remotes/origin/{branch}"), tip, true, "test").unwrap();
    let mut config = repo.config().unwrap();
    config.set_str(&format!("branch.{branch}.remote"), "origin").unwrap();
    config.set_str(&format!("branch.{branch}.merge"), &format!("refs/heads/{branch}")).unwrap();
}

/// A commit on top of `parent` whose tree is the parent's with `files` written,
/// moving no reference.
pub(super) fn commit_on(
    repo: &git2::Repository,
    parent: git2::Oid,
    files: &[(&str, &str)],
    message: &str,
) -> git2::Oid {
    let parent = repo.find_commit(parent).unwrap();
    let tree = tree_with(repo, parent.tree().unwrap().id(), files);
    let tree = repo.find_tree(tree).unwrap();
    let signature = git2::Signature::now("Upstream Author", "upstream@example.com").unwrap();
    repo.commit(None, &signature, &signature, message, &tree, &[&parent]).unwrap()
}

/// The tree `base` holds with `files` written at its top level.
pub(super) fn tree_with(
    repo: &git2::Repository,
    base: git2::Oid,
    files: &[(&str, &str)],
) -> git2::Oid {
    let base = repo.find_tree(base).unwrap();
    let mut builder = repo.treebuilder(Some(&base)).unwrap();
    for (path, content) in files {
        let blob = repo.blob(content.as_bytes()).unwrap();
        builder.insert(path, blob, 0o100644).unwrap();
    }
    builder.write().unwrap()
}

/// A commit the upstream holds on top of the current remote-tracking tip, which
/// the remote-tracking ref then names.
pub(super) fn upstream_commit(fx: &Fixture, files: &[(&str, &str)], message: &str) -> git2::Oid {
    let repo = fx.repo();
    let name = format!("refs/remotes/origin/{}", branch_of(fx));
    let parent = repo.find_reference(&name).unwrap().peel_to_commit().unwrap().id();
    let tip = commit_on(&repo, parent, files, message);
    repo.reference(&name, tip, true, "test").unwrap();
    tip
}

/// Write `files` into the primary worktree and commit them on its branch.
pub(super) fn local_commit(fx: &Fixture, files: &[(&str, &str)], message: &str) -> git2::Oid {
    for (path, content) in files {
        std::fs::write(fx.root().join(path), content).unwrap();
    }
    commit_all(&fx.repo(), message)
}

pub(super) fn read(fx: &Fixture, path: &str) -> String {
    std::fs::read_to_string(fx.root().join(path)).unwrap_or_default()
}

/// Every reference of the repository with the commit it names.
pub(super) fn refs_of(fx: &Fixture) -> std::collections::BTreeMap<String, String> {
    let repo = fx.repo();
    let mut refs = std::collections::BTreeMap::new();
    for reference in repo.references().unwrap().flatten() {
        let name = reference.name().unwrap_or_default().to_string();
        let target = reference.peel_to_commit().map(|c| c.id().to_string()).unwrap_or_default();
        refs.insert(name, target);
    }
    refs
}

/// The merge data of a pull run over `local` and `remote`, as the hand-off
/// records it, for a test that applies a tree it settled itself.
pub(super) fn pull_data(
    fx: &Fixture,
    local: git2::Oid,
    remote: git2::Oid,
    unresolved: &[&str],
    strategy: crate::git::PullStrategy,
) -> crate::graduation::GraduationMergeData {
    let branch = branch_of(fx);
    let upstream = format!("origin/{branch}");
    crate::graduation::GraduationMergeData {
        name: crate::graduation::pull_title(&upstream, &branch),
        stream_branch: upstream.clone(),
        base_branch: branch.clone(),
        base_tip: local.to_string(),
        stream_tip: remote.to_string(),
        merge_base: String::new(),
        snapshot_commit: String::new(),
        publication: StreamMergePublication::Commit {
            message: format!("Merge {upstream} into {branch}"),
        },
        changed_paths: Vec::new(),
        unresolved_paths: unresolved.iter().map(|p| p.to_string()).collect(),
        conflicts: Vec::new(),
        result: None,
        pull: Some(crate::graduation::PullMergeSubject {
            branch,
            upstream,
            worktree_path: fx.root().to_string_lossy().into_owned(),
            strategy,
        }),
    }
}

/// A pull run's merge request over `local` and `remote`.
pub(super) fn pull_request(
    fx: &Fixture,
    local: git2::Oid,
    remote: git2::Oid,
    strategy: crate::git::PullStrategy,
) -> crate::graduation::MergeRunRequest {
    let data = pull_data(fx, local, remote, &["a.md"], strategy);
    let subject = data.pull.clone().unwrap();
    crate::graduation::MergeRunRequest {
        run_id: crate::graduation::new_run_id(),
        project_key: fx.app.state::<crate::project::ProjectState>().slot_key(),
        stream_id: String::new(),
        stream_name: String::new(),
        stream_branch: data.stream_branch,
        base_branch: data.base_branch,
        base_tip: data.base_tip,
        stream_tip: data.stream_tip,
        merge_base: String::new(),
        snapshot_commit: String::new(),
        publication: data.publication,
        changed_paths: Vec::new(),
        unresolved_paths: data.unresolved_paths,
        conflicts: Vec::new(),
        pull: Some((subject, "repo".to_string())),
    }
}
