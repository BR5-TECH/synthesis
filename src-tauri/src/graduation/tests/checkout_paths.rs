//! Where a run's throwaway checkouts stand, and what is reclaimed of them
//! (`../ai/GRL-graduation-loop.md` GRL-FR-YKRI, GRL-FR-BIQX, GRL-FR-PQLM,
//! GRL-FR-LYXG, GRL-FR-XJJU, GRL-FR-QEUP; `GRD-graduation.md` GRD-FR-XKGC).
//!
//! The helpers here are shared with the other suites, so an assertion about a
//! checkout reads the nonced names rather than a fixed path that no turn uses.

use super::*;

/// Whether `name` is a nonced checkout name of the stem `stem`: the stem, a
/// dash, and 8 lowercase hexadecimal digits. The bare stem is not one.
pub(crate) fn is_nonced_name(stem: &str, name: &str) -> bool {
    name != stem && crate::graduation::is_checkout_name(stem, name)
}

/// Whether `path` is a nonced checkout of the stem `stem`, directly under the
/// home of the run `run_id`.
pub(crate) fn is_nonced_checkout_of(
    app: &tauri::AppHandle<Runtime>,
    run_id: &str,
    stem: &str,
    path: &Path,
) -> bool {
    let home = store_base(app).expect("the run store").home(run_id);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.parent().map(canonical) == Some(canonical(&home)) && is_nonced_name(stem, &name)
}

/// Every directory under the run home with a checkout name of the stem `stem`,
/// the bare stem included. The run home must stand.
pub(crate) fn checkouts_left(
    app: &tauri::AppHandle<Runtime>,
    run_id: &str,
    stem: &str,
) -> Vec<PathBuf> {
    let home = store_base(app).expect("the run store").home(run_id);
    // A home that is not there holds nothing, which would make every "none is
    // left" check pass for a wrong path. So it must stand.
    let entries = std::fs::read_dir(&home)
        .unwrap_or_else(|e| panic!("the run home {} stands: {e}", home.display()));
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| is_checkout_name(stem, &entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// Every registration of the run with a checkout name of the stem `stem`:
/// `<run-id>-<stem>` or `<run-id>-<stem>-<nonce>`.
pub(crate) fn registrations_left(repo: &git2::Repository, run_id: &str, stem: &str) -> Vec<String> {
    let prefix = format!("{run_id}-");
    let mut found: Vec<String> = repo
        .worktrees()
        .map(|list| list.iter().filter_map(|n| n.ok().flatten().map(str::to_string)).collect())
        .unwrap_or_default();
    found
        .retain(|name| name.strip_prefix(&prefix).is_some_and(|rest| is_checkout_name(stem, rest)));
    found.sort();
    found
}

/// The warnings the application's diagnostic buffer holds for one run, with
/// the message `message`.
pub(crate) fn warnings(run_id: &str, message: &str) -> Vec<crate::logging::LogRecord> {
    let filter: crate::logging::LogFilter =
        serde_json::from_value(serde_json::json!({ "query": run_id })).unwrap();
    let page = crate::logging::BUFFER.query(&filter, None, 5000).expect("a page");
    page.records
        .into_iter()
        .filter(|r| r.message == message && r.level == crate::logging::LogLevel::Warn)
        .collect()
}

/// A run queued on a stream, with a base commit and no loop behind it: what a
/// process stopped during a review leaves.
fn resting_run(fx: &Fixture) -> (GraduationRun, crate::streams::WorkStream) {
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");
    run.base_commit = Some(
        git2::Repository::open(stream.worktree())
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string(),
    );
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    (run, stream)
}

/// A directory that refuses the removal of its entries until it is dropped.
#[cfg(unix)]
pub(crate) struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    pub(crate) fn new(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
        ReadOnly(path.to_path_buf())
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

// GRL-FR-LYXG, GRL-FR-PQLM, GRL-FR-BIQX: an earlier review checkout that cannot
// be removed is reported, and the review turn still runs, in a new checkout.
// The removable one beside it is gone once the run ends.
#[cfg(unix)]
#[test]
fn a_review_runs_when_an_earlier_checkout_cannot_be_removed() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_review_runs_when_an_earlier_checkout_cannot_be_removed",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");
    let home = store_base(&fx.app).unwrap().home(&run.id);
    let stuck = home.join("rv-0123abcd");
    std::fs::create_dir_all(stuck.join("locked")).unwrap();
    std::fs::write(stuck.join("locked/f.txt"), "x\n").unwrap();
    std::fs::create_dir_all(home.join("rv-89abcdef")).unwrap();
    std::fs::write(home.join("rv-89abcdef/f.txt"), "x\n").unwrap();
    let guard = ReadOnly::new(&stuck.join("locked"));

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "export const panel = 1;\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "blocker: {:?}", done.blocker);
    assert!(done.blocker.is_none());
    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 1);
    assert!(is_nonced_checkout_of(&fx.app, &run.id, "rv", &review[0].directory));
    assert_ne!(canonical(&review[0].directory), canonical(&stuck));
    assert!(!review[0].directory.exists(), "the turn's own checkout went");
    assert!(stuck.join("locked/f.txt").is_file(), "the stuck checkout stays");
    assert!(!home.join("rv-89abcdef").exists(), "the removable checkout went");
    assert!(registrations_left(&fx.repo(), &run.id, "rv").is_empty());

    // The buffer is shared by every test of the binary, so a record is looked
    // for rather than counted.
    let before = warnings(&run.id, "graduation could not remove an earlier review checkout");
    assert!(
        before.iter().any(|r| {
            let reason = r.fields.get("reason").and_then(|v| v.as_str()).unwrap_or_default();
            reason.contains("rv-0123abcd") && !reason.contains("rv-89abcdef")
        }),
        "a warning before the turn names the stuck checkout alone: {before:?}"
    );
    assert!(!warnings(&run.id, "graduation could not remove a review checkout").is_empty());
    drop(guard);
}

// GRL-FR-XJJU, GRL-FR-PQLM: the stranded sweep takes a checkout directory that
// has no registration.
#[test]
fn the_sweep_takes_a_stranded_checkout_directory_with_no_registration() {
    let fx = Fixture::new();
    let (run, stream) = resting_run(&fx);
    let home = store_base(&fx.app).unwrap().home(&run.id);
    std::fs::create_dir_all(home.join("rv-0123abcd")).unwrap();
    std::fs::write(home.join("rv-0123abcd/f.txt"), "x\n").unwrap();

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    assert!(checkouts_left(&fx.app, &run.id, "rv").is_empty());
    assert!(home.is_dir(), "the run home stays");
    assert!(stream.worktree().is_dir(), "the stream is untouched");
}

// GRL-FR-XJJU, GRL-FR-PQLM: the stranded sweep takes a registration whose
// checkout directory is already gone, with its scratch branch.
#[test]
fn the_sweep_takes_a_stranded_registration_with_no_directory() {
    let fx = Fixture::new();
    let (run, stream) = resting_run(&fx);
    let checkout =
        crate::graduation::driver::review_checkout::create(&fx.app, &run, &stream.worktree())
            .expect("a checkout");
    std::fs::remove_dir_all(&checkout).unwrap();
    assert_eq!(registrations_left(&fx.repo(), &run.id, "rv").len(), 1);

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    let repo = fx.repo();
    assert!(registrations_left(&repo, &run.id, "rv").is_empty());
    assert!(repo
        .find_branch(&format!("synthesis/review/{}", run.id), git2::BranchType::Local)
        .is_err());
    assert!(repo.find_branch(&stream.branch, git2::BranchType::Local).is_ok());
}

// GRL-FR-BIQX, GRL-FR-PQLM: a later review checkout of a run never stands at
// the path of an earlier one, also when the earlier one was left standing.
#[test]
fn a_review_checkout_made_again_gets_a_new_path_and_the_old_one_goes() {
    let fx = Fixture::new();
    let (run, stream) = resting_run(&fx);
    let review = crate::graduation::driver::review_checkout::create;
    let first = review(&fx.app, &run, &stream.worktree()).expect("a checkout");
    let second = review(&fx.app, &run, &stream.worktree()).expect("a second checkout");

    assert_ne!(first, second);
    assert!(is_nonced_checkout_of(&fx.app, &run.id, "rv", &first));
    assert!(is_nonced_checkout_of(&fx.app, &run.id, "rv", &second));
    assert!(!first.exists(), "the earlier checkout went");
    assert!(second.is_dir());
    let registered = registrations_left(&fx.repo(), &run.id, "rv");
    let name = second.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(registered, vec![format!("{}-{name}", run.id)]);
}

// GRL-FR-XJJU, GRL-FR-PQLM: a stranded review checkout of the name without a
// nonce, as an earlier version of the application left it, is reclaimed at
// the queue read with its registration and scratch branch.
#[test]
fn the_sweep_takes_a_stranded_checkout_of_the_earlier_name() {
    let fx = Fixture::new();
    let (run, stream) = resting_run(&fx);
    let repo = fx.repo();
    let home = store_base(&fx.app).unwrap().home(&run.id);
    let base = git2::Oid::from_str(run.base_commit.as_deref().unwrap()).unwrap();
    let branch = format!("synthesis/review/{}", run.id);
    repo.branch(&branch, &repo.find_commit(base).unwrap(), true).unwrap();
    let reference = repo.find_branch(&branch, git2::BranchType::Local).unwrap().into_reference();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    repo.worktree(&format!("{}-rv", run.id), &home.join("rv"), Some(&opts)).unwrap();
    git2::Repository::open(home.join("rv")).unwrap().set_head_detached(base).unwrap();

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    assert!(!home.join("rv").exists());
    let repo = fx.repo();
    assert!(registrations_left(&repo, &run.id, "rv").is_empty());
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_err());
    assert!(stream.worktree().is_dir(), "the stream is untouched");
}
