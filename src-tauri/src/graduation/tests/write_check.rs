//! The main branch write check of a graduation start (`GRD-graduation.md`
//! GRD-FR-WJLO, GRD-FR-MOWZ).
//!
//! The fixture repository has no remote, so its local `main` or `master` is
//! the default branch and no GitHub read is made.

use super::*;
use crate::git::write_guard::commands::WriteGuardState;

impl Fixture {
    /// Ask for the main branch write check on this application.
    fn arm_write_guard(&self) {
        self.app.manage(WriteGuardState::offline());
    }

    /// The author confirms writes to the project's own branch.
    fn confirm_main_write(&self) {
        let session = self.app.state::<WriteGuardState>().session();
        crate::git::write_guard::confirm_for_test(&session, &self.root(), &self.branch());
    }

    fn queued_runs(&self) -> usize {
        crate::graduation::list_graduation_queue(self.app.clone()).expect("the queue").runs.len()
    }
}

// GRD-FR-WJLO: a start on an unconfirmed main branch is refused before anything
// is enqueued, and no graduation-start commit is asked for.
#[test]
fn a_start_on_an_unconfirmed_main_branch_enqueues_nothing() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.arm_write_guard();
    let stream = fx.stream("editor");
    let draft = fx.draft("guarded");
    fx.hold_stream(&stream.id);

    let start = || {
        crate::graduation::start_graduation(
            fx.app.clone(),
            draft.clone(),
            stream.id.clone(),
            StandingWork::Keep,
            None,
            None,
        )
    };
    let refused = start().unwrap_err();
    assert!(
        refused.to_string().starts_with(crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED),
        "{refused}"
    );
    assert_eq!(fx.queued_runs(), 0, "a refused start made no run");
    let root = crate::fs::RootFs::for_root(fx.root());
    let started =
        crate::storage_floor::commit::pending_events(&root).into_iter().any(|(id, e, _)| {
            id == draft && e == crate::storage_floor::commit::DraftEvent::GraduationStarted
        });
    assert!(!started, "a refused start asks for no commit");

    fx.confirm_main_write();
    assert!(start().is_ok(), "the confirmed branch takes the start");
    assert_eq!(fx.queued_runs(), 1);
}

// GRD-FR-WJLO: a direct start checks the pinned branch, and refuses before it
// creates anything.
#[test]
fn a_direct_start_on_an_unconfirmed_main_branch_is_refused() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.arm_write_guard();
    let draft = fx.committed_draft("direct");
    let start = || {
        crate::graduation::start_direct_graduation(
            fx.app.clone(),
            draft.clone(),
            fx.root().to_string_lossy().into_owned(),
            fx.branch(),
        )
    };
    let refused = start().unwrap_err();
    assert!(
        refused.to_string().starts_with(crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED),
        "{refused}"
    );
    assert_eq!(fx.queued_runs(), 0);

    fx.confirm_main_write();
    assert!(start().is_ok());
}

// GRD-FR-MOWZ: a run that is already enqueued commits with no further check,
// also when the session holds no confirmation, as after a relaunch.
#[test]
fn an_enqueued_run_commits_with_no_further_check() {
    let fx = Fixture::new();
    let exclude = fx.repo().path().join("info/exclude");
    std::fs::create_dir_all(exclude.parent().expect("a folder")).expect("the folder");
    std::fs::write(&exclude, "statistics/\n").expect("the exclude file");
    fx.arm_write_guard();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.commits.len(), 1, "the run committed on the unconfirmed main branch");
}

// GRD-FR-WJLO: the pinned branch of a direct run is checked when it is not the
// branch of the active worktree, and the active worktree of another project is
// not checked.
#[test]
fn the_pinned_branch_is_checked_and_another_projects_worktree_is_not() {
    let fx = Fixture::new();
    fx.arm_write_guard();
    let main = fx.branch();
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    let pinned = DirectTarget {
        worktree_path: fx.root().to_string_lossy().into_owned(),
        worktree_name: "project".into(),
        branch: main.clone(),
    };

    let refused = crate::graduation::write_check::require_start_write(
        &fx.app,
        &fx.project_key(),
        Some(&pinned),
        "start_direct_graduation",
    )
    .unwrap_err();
    assert!(
        refused.to_string().starts_with(crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED),
        "{refused}"
    );
    assert!(refused.contains(&format!("\"branch\":\"{main}\"")));

    // The active branch is `feature`, which is no main branch, and a run of
    // another project does not check this project's worktree at all.
    assert!(crate::graduation::write_check::require_start_write(
        &fx.app,
        &fx.project_key(),
        None,
        "start_graduation",
    )
    .is_ok());
    repo.set_head(&format!("refs/heads/{main}")).unwrap();
    assert!(crate::graduation::write_check::require_start_write(
        &fx.app,
        "/another/project",
        None,
        "restart_graduation_run",
    )
    .is_ok());
}

// GRD-FR-WJLO: a restart of a discarded direct run checks the branch the run
// pinned. A pin to an unconfirmed main branch is refused before a run is made,
// also when the active worktree holds another branch.
#[test]
fn a_restart_of_a_direct_run_pinned_to_an_unconfirmed_main_branch_is_refused() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let pinned = fx.branch();
    let draft = fx.committed_draft("direct");
    fx.other_branch("elsewhere");
    let original = fx.direct("Add the empty state.", &draft);
    crate::graduation::discard_graduation_run(fx.app.clone(), original.id.clone()).unwrap();
    fx.check_out("elsewhere");
    fx.arm_write_guard();
    let before = fx.queued_runs();
    let restart = || {
        crate::graduation::restart_graduation_run(
            fx.app.clone(),
            original.id.clone(),
            String::new(),
            StandingWork::Keep,
            None,
        )
    };

    let refused = restart().unwrap_err();
    assert!(
        refused.to_string().starts_with(crate::git::ERR_MAIN_BRANCH_WRITE_UNCONFIRMED),
        "{refused}"
    );
    assert!(refused.contains(&format!("\"branch\":\"{pinned}\"")), "{refused}");
    assert_eq!(fx.queued_runs(), before, "a refused restart made no run");

    let session = fx.app.state::<WriteGuardState>().session();
    crate::git::write_guard::confirm_for_test(&session, &fx.root(), &pinned);
    let restarted = restart().expect("the confirmed pin takes the restart");
    assert_eq!(restarted.direct_target.as_ref().unwrap().branch, pinned);
}

// GRD-FR-MOWZ, GRD-FR-BLCR: the revert of a run that started past the check
// runs no check, also after a relaunch. The revert commit lands on the
// unconfirmed main branch.
#[test]
fn a_revert_after_a_relaunch_runs_no_check() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let main = fx.branch();
    // The session after a relaunch holds no confirmation.
    fx.arm_write_guard();
    let guard = crate::git::write_guard::commands::require_branch_write(
        &fx.app,
        &fx.root(),
        Some(&main),
        "commit_paths",
    );
    assert!(guard.is_err(), "the branch is an unconfirmed main branch");

    let reverted = crate::graduation::revert_graduation_run(fx.app.clone(), done.id.clone())
        .expect("the revert runs no check");

    assert_eq!(reverted.reverts.len(), 1);
    assert_eq!(reverted.reverts[0].branch, main);
    let tip = super::merge_handoff::tip(&fx.repo(), &main).to_string();
    assert_eq!(reverted.reverts[0].commit, tip, "the revert commit is on {main}");
}
