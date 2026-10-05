//! What a review turn stands in, and what is reclaimed when it ends
//! (`../ai/GRL-graduation-loop.md` GRL-FR-YKRI).

use super::*;

/// GRL-FR-YKRI: the review stands in the run's change set, as uncommitted work
/// on the run's base commit.
///
/// The work turn's output is not committed until a `ready` verdict, so a
/// checkout taken from the stream's own revision would hold the base tree and
/// nothing else — the reviewer would judge an empty change set every time.
#[test]
fn the_review_stands_in_the_change_set_the_work_turn_wrote() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("src/panel.ts", "export const panel = 1;\n")
            .writing("README.md", "seed\nand more\n")
            .deleting(".gitignore"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 1);
    let seen = &review[0];

    // It is a checkout of its own, not the stream's working copy.
    assert_ne!(seen.directory, stream.worktree());
    assert_eq!(seen.turn_kind, TurnKind::Review);

    // It stands on the run's base commit.
    assert_eq!(seen.head.as_deref(), run.base_commit.as_deref());

    // What the work turn wrote is in front of it, created, changed and deleted.
    assert_eq!(
        seen.tree.get("src/panel.ts").map(String::as_str),
        Some("export const panel = 1;\n"),
        "a created path is there"
    );
    assert_eq!(
        seen.tree.get("README.md").map(String::as_str),
        Some("seed\nand more\n"),
        "a changed path holds the change"
    );
    assert!(
        !seen.tree.contains_key(".gitignore"),
        "a deleted path is gone"
    );

    // And `git diff HEAD` names exactly the run's change set.
    assert_eq!(
        seen.diff_paths,
        vec![
            ".gitignore".to_string(),
            "README.md".to_string(),
            "src/panel.ts".to_string()
        ]
    );
    assert_eq!(seen.path_list("changed_paths"), seen.diff_paths);
}

/// GRL-FR-XNQU: the tree the review stands in is the tree a `ready` verdict
/// commits, so what was judged and what landed cannot be two different things.
#[test]
fn the_review_sees_the_tree_the_commit_will_hold() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("src/panel.ts", "export const panel = 1;\n")
            .deleting(".gitignore"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let reviewed = dispatch.of_part("review")[0]
        .worktree_tree
        .clone()
        .expect("the tree the reviewer stood in");
    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let committed = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap()
        .id()
        .to_string();
    assert_eq!(reviewed, committed);
}

/// GRL-FR-YKRI: a run with an empty change set still gets a checkout, so the
/// reviewer can report that nothing was delivered.
#[test]
fn a_run_that_wrote_nothing_still_gets_a_review_checkout() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work(),
        Turn::revise(ReviewSeverity::Critical, "Nothing was delivered."),
        Turn::work(),
        Turn::revise(ReviewSeverity::Critical, "Nothing was delivered."),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 2, "both reviews ran");
    assert!(review[0].diff_paths.is_empty());
    assert_eq!(run.state, GraduationRunState::AwaitingAuthor);
    assert!(run.blocker.is_none(), "no checkout failed: {:?}", run.blocker);
}

/// GRL-FR-YKRI: a second pass gets a review checkout of its own.
///
/// Removing the directory alone leaves Git's registration and the branch the
/// worktree was made on, and the next request for that name is refused.
#[test]
fn a_second_pass_gets_its_own_review_checkout() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Not yet."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.review_turns, 2);
    assert!(run.blocker.is_none(), "no checkout failed: {:?}", run.blocker);
    let review = dispatch.of_part("review");
    assert_eq!(
        review[1].tree.get("src/panel.ts").map(String::as_str),
        Some("2\n"),
        "the second review reads the second work turn's output"
    );
}

/// GRL-FR-YKRI: the checkout, its registration and its branch all go when the
/// turn ends.
#[test]
fn a_review_checkout_leaves_no_worktree_and_no_branch() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::revise(ReviewSeverity::Major, "Not yet."),
        Turn::work().writing("src/panel.ts", "2\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    let checkout = dispatch.of_part("review")[0].directory.clone();

    assert!(!checkout.exists(), "the directory is gone");
    let repo = fx.repo();
    let registered: Vec<String> = repo
        .worktrees()
        .unwrap()
        .iter()
        .filter_map(|n| n.ok().flatten().map(str::to_string))
        .collect();
    assert!(
        !registered.iter().any(|name| name.ends_with("-rv")),
        "no review registration is left: {registered:?}"
    );
    let branches: Vec<String> = repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .flatten()
        .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
        .collect();
    assert!(
        !branches.iter().any(|name| name.starts_with("synthesis/review/")),
        "no review branch is left: {branches:?}"
    );
    assert!(
        branches.iter().any(|name| name == &stream.branch),
        "the stream's own branch stands: {branches:?}"
    );
    assert_eq!(run.state, GraduationRunState::Completed);
}

/// GRL-FR-BRJO: the review writes nothing into the stream, whatever it does in
/// its own checkout.
#[test]
fn what_the_review_writes_reaches_nothing_of_the_stream() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        // The reviewer runs the project's checks, which write build output.
        Turn::ready()
            .writing("target/build.log", "ran the tests\n")
            .writing("src/panel.ts", "the reviewer tampered\n"),
    ]);
    let run = fx.drive(&run, dispatch);

    assert_eq!(
        std::fs::read_to_string(stream.worktree().join("src/panel.ts")).unwrap(),
        "1\n",
        "the stream holds what the work turn wrote"
    );
    assert!(!stream.worktree().join("target/build.log").exists());
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let tree = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert!(tree.get_path(Path::new("target/build.log")).is_err());
}

/// GRL-FR-CKBL: authored work the repository's ignore rules hide is named to
/// the review, because it is absent from the change set and no commit writes it.
#[test]
fn the_review_is_told_what_the_ignore_rules_kept_out() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("src/panel.ts", "1\n")
            // The seed repository ignores `target/`, so this is authored work
            // that nobody would otherwise be told about.
            .writing("target/generated.ts", "authored, and hidden\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let review = &dispatch.of_part("review")[0];
    assert_eq!(
        review.path_list("hidden_paths"),
        vec!["target/".to_string()],
        "an ignored directory reads as one entry"
    );
    assert!(
        !review.path_list("changed_paths").iter().any(|p| p.starts_with("target/")),
        "and it is not in the change set"
    );
    // Nor is it committed, which is the reason the reviewer must be told.
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let tree = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert!(tree.get_path(Path::new("target/generated.ts")).is_err());
}

/// GRD-FR-ARLT: deriving the run's tree leaves the stream's own index alone,
/// and the commit brings it to what was committed.
///
/// A stream whose index was left at the revision before the commit shows the
/// author every committed path as an uncommitted change of their own.
#[test]
fn the_stream_is_clean_after_a_run_commits() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![
            Turn::work()
                .writing("src/panel.ts", "1\n")
                .writing("README.md", "seed\nand more\n")
                .deleting(".gitignore"),
            Turn::ready(),
        ]),
    );
    assert_eq!(run.state, GraduationRunState::Completed);

    let repo = git2::Repository::open(stream.worktree()).expect("stream repo");
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts)).expect("statuses");
    let dirty: Vec<String> = statuses
        .iter()
        .filter(|entry| !entry.status().is_empty())
        .map(|entry| entry.path().unwrap_or_default().to_string())
        .collect();
    assert!(
        dirty.is_empty(),
        "the stream reads back clean after the run: {dirty:?}"
    );
}

/// GRL-FR-YKRI: a review checkout the application never finished is reclaimed
/// at the first read of the project's queue.
#[test]
fn a_stranded_review_checkout_is_reclaimed_at_the_next_queue_read() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let mut run = fx.enqueue(&stream, "Write the panel.");

    // What a process killed mid-review leaves: a base commit, a checkout, a
    // registration and a branch, with no loop behind the run.
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
    let checkout = crate::graduation::driver::review_checkout::create(
        &fx.app,
        &run,
        &stream.worktree(),
    )
    .expect("a checkout");
    assert!(checkout.is_dir());

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::sweep_abandoned_runs(&fx.app, &queue);

    assert!(!checkout.exists(), "the stranded checkout is gone");
    let repo = fx.repo();
    assert!(
        !crate::streams::registers_worktree(&repo, &format!("{}-rv", run.id)),
        "and so is its registration"
    );
    assert!(
        repo.find_branch(&format!("synthesis/review/{}", run.id), git2::BranchType::Local)
            .is_err(),
        "and its branch"
    );
    // The stream itself is untouched.
    assert!(stream.worktree().is_dir());
    assert!(repo.find_branch(&stream.branch, git2::BranchType::Local).is_ok());
}

/// GRL-FR-YKRI: a checkout of a run that is being reviewed is not stranded.
///
/// `is_driving` knows only about this process, so a second window reading the
/// queue must not reclaim a checkout a turn elsewhere is standing in.
#[test]
fn a_checkout_of_a_reviewing_run_is_left_alone() {
    let fx = Fixture::new();
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
    run.state = GraduationRunState::Reviewing;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    let checkout = crate::graduation::driver::review_checkout::create(
        &fx.app,
        &run,
        &stream.worktree(),
    )
    .expect("a checkout");

    let queue = crate::graduation::project_queue(&fx.app).expect("the queue");
    crate::graduation::driver::review_checkout::sweep_stranded(&fx.app, &queue);

    assert!(checkout.is_dir(), "the live checkout stands");
    assert!(crate::streams::registers_worktree(
        &fx.repo(),
        &format!("{}-rv", run.id)
    ));
}

/// GRL-FR-XNQU: the tree the review stands in is the tree the commit holds, for
/// a change set Git has to settle rather than copy — an executable bit, a
/// symbolic link, and a file that is not text.
#[test]
fn a_change_set_of_modes_links_and_binary_reaches_the_review_whole() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");
    let worktree = stream.worktree();

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .writing("build.sh", "#!/bin/sh\necho hello\n")
            .doing({
                let worktree = worktree.clone();
                move || {
                    use std::os::unix::fs::PermissionsExt;
                    let script = worktree.join("build.sh");
                    let mut mode = std::fs::metadata(&script).unwrap().permissions();
                    mode.set_mode(0o755);
                    std::fs::set_permissions(&script, mode).unwrap();
                    std::fs::write(worktree.join("logo.bin"), [0u8, 159, 146, 150, 0, 255]).unwrap();
                    std::os::unix::fs::symlink("build.sh", worktree.join("build-link")).unwrap();
                }
            }),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);

    // The reviewer's tree and the committed tree are one tree.
    let reviewed = dispatch.of_part("review")[0]
        .worktree_tree
        .clone()
        .expect("the tree the reviewer stood in");
    let repo = git2::Repository::open(&worktree).unwrap();
    let tree = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert_eq!(reviewed, tree.id().to_string());

    // And Git settled each of the three, rather than any of them being copied
    // as text.
    let mode_of = |path: &str| {
        tree.get_path(Path::new(path))
            .unwrap_or_else(|_| panic!("{path} is in the commit"))
            .filemode()
    };
    assert_eq!(mode_of("build.sh"), i32::from(git2::FileMode::BlobExecutable));
    assert_eq!(mode_of("build-link"), i32::from(git2::FileMode::Link));
    assert_eq!(mode_of("logo.bin"), i32::from(git2::FileMode::Blob));
    let blob = repo
        .find_blob(tree.get_path(Path::new("logo.bin")).unwrap().id())
        .unwrap();
    assert_eq!(blob.content(), [0u8, 159, 146, 150, 0, 255]);
}

/// GRL-FR-XNQU, GRD-FR-ARLT: a turn that removes a directory and renames a file
/// is reviewed and committed as what it did, both sides of the rename included.
#[test]
fn a_removed_directory_and_a_rename_reach_the_review_and_the_commit() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");

    // A tree with something to remove and something to rename.
    let worktree = stream.worktree();
    std::fs::create_dir_all(worktree.join("legacy")).unwrap();
    std::fs::write(worktree.join("legacy/old.ts"), "old\n").unwrap();
    std::fs::write(worktree.join("panel.ts"), "the panel\n").unwrap();
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work()
            .removing_dir("legacy")
            .writing("src/panel.ts", "the panel\n")
            .deleting("panel.ts"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);

    // The review saw both sides of the rename and the removed directory.
    let review = &dispatch.of_part("review")[0];
    assert_eq!(
        review.diff_paths,
        vec![
            "legacy/old.ts".to_string(),
            "panel.ts".to_string(),
            "src/panel.ts".to_string()
        ]
    );
    assert!(!review.tree.contains_key("legacy/old.ts"));
    assert!(!review.tree.contains_key("panel.ts"));
    assert_eq!(review.tree.get("src/panel.ts").map(String::as_str), Some("the panel\n"));

    // And so does the commit.
    let repo = git2::Repository::open(&worktree).unwrap();
    let tree = repo
        .find_commit(git2::Oid::from_str(run.commits.last().unwrap()).unwrap())
        .unwrap()
        .tree()
        .unwrap();
    assert!(tree.get_path(Path::new("legacy/old.ts")).is_err());
    assert!(tree.get_path(Path::new("panel.ts")).is_err());
    assert!(tree.get_path(Path::new("src/panel.ts")).is_ok());
    assert_eq!(review.worktree_tree.as_deref(), Some(tree.id().to_string().as_str()));
}

/// GRL-FR-YKRI / FSA-FR-ZUCF: a review that installed a dependency store of
/// symbolic links leaves a checkout the application can still reclaim.
///
/// This is the defect the whole primitive exists for. The review turn is given
/// a writable checkout precisely so it can run the project's own build and test
/// commands, and a `pnpm install` inside it writes hundreds of links. A removal
/// that refused over one left the checkout standing, and the *next* turn's
/// `create` — which reclaims before it asks for the name again — then failed
/// hard and blocked the run.
#[cfg(unix)]
#[test]
fn a_review_that_installed_symlinks_is_still_reclaimed() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "export const panel = 1;\n"),
        // What the review's own `pnpm install` writes into its checkout.
        Turn::ready()
            .linking("node_modules/mdn-data", "../.pnpm/mdn-data/index.js")
            .linking("node_modules/.bin/vite", "../vite/bin/vite.js"),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(run.state, GraduationRunState::Completed);
    let checkout = crate::graduation::store_base(&fx.app)
        .expect("the store")
        .review_checkout(&run.id);
    assert!(
        !checkout.exists(),
        "the checkout was left standing at {}",
        checkout.display()
    );
}

/// GRL-FR-YKRI: the reclaim at the head of `create` takes what an earlier turn
/// left, so a second review turn of the same run gets its checkout.
///
/// The latch this stands against: the first release failed silently and the
/// second turn died on what it left, so a run could never review again.
#[cfg(unix)]
#[test]
fn a_second_review_turn_reclaims_what_the_first_left() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Rework the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "export const panel = 1;\n"),
        Turn::revise(ReviewSeverity::Major, "The empty state is missing.")
            .linking("node_modules/mdn-data", "../.pnpm/mdn-data/index.js"),
        Turn::work().writing("src/empty.ts", "export const empty = 1;\n"),
        Turn::ready().linking("node_modules/mdn-data", "../.pnpm/mdn-data/index.js"),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    assert_eq!(
        dispatch.of_part("review").len(),
        2,
        "the second review never got a checkout to stand in"
    );
    assert_eq!(run.state, GraduationRunState::Completed);
    assert!(run.blocker.is_none(), "blocker: {:?}", run.blocker);
}
