//! The graduation message of a direct run (`GRD-graduation.md` GRD-FR-PQMX):
//! the run's own name and nothing else.

use super::*;

/// A fixture whose statistics logs, kept inside the worktree, Git ignores, so
/// that no commit of a run depends on whether the writer got there first.
fn fixture() -> Fixture {
    let fx = Fixture::new();
    let exclude = fx.repo().path().join("info/exclude");
    std::fs::create_dir_all(exclude.parent().expect("a folder")).expect("the folder");
    std::fs::write(&exclude, "statistics/\n").expect("the exclude file");
    fx
}

/// The revision the worktree's branch head holds.
fn head_id(root: &Path) -> String {
    let repo = git2::Repository::open(root).expect("the repository");
    let id = repo.head().and_then(|head| head.peel_to_commit()).expect("the head").id();
    id.to_string()
}

/// The full message of the commit the worktree's branch head holds.
fn head_message(root: &Path) -> String {
    let repo = git2::Repository::open(root).expect("the repository");
    let message = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("the head")
        .message()
        .unwrap_or_default()
        .to_string();
    message
}

// GRD-FR-PQMX, GRD-FR-ARLT: a direct run that the review lets through commits
// under exactly the run's name, with no body and no line of the prompt.
#[test]
fn a_completed_direct_run_commits_under_its_own_name() {
    let fx = fixture();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("# Add the empty state\n\nMore detail.", &draft);

    let run = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );

    assert_eq!(run.state, GraduationRunState::Completed);
    assert_eq!(run.commits.len(), 1);
    assert_eq!(head_id(&fx.root()), run.commits[0], "the head is the run's commit");
    assert_eq!(head_message(&fx.root()), run.input.draft_name);
    assert_eq!(run.input.draft_name, "editor draft");
}

// GRD-FR-PQMX, GRD-FR-WQTN, GRD-FR-SWOJ: a direct run whose work stands in its
// own abandoned-turn commit completes by rewriting that commit's message to
// exactly the run's name.
#[test]
fn a_direct_runs_abandoned_turn_is_retitled_to_its_own_name() {
    let fx = fixture();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("Add the empty state.", &draft);

    let interrupted = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::answering(Answer::Process(ProcessOutcome::Timeout))
            .writing("src/panel.ts", "1\n")]),
    );
    assert_eq!(interrupted.state, GraduationRunState::Interrupted);
    assert_eq!(interrupted.commits.len(), 1, "the abandoned turn is committed");
    assert!(head_message(&fx.root()).starts_with("Abandoned graduation turn"));
    let abandoned = interrupted.commits[0].clone();

    let continued = fx.continue_run(&interrupted);
    let completed = fx.drive(&continued, ScriptedDispatch::new(vec![Turn::work(), Turn::ready()]));

    assert_eq!(completed.state, GraduationRunState::Completed);
    assert_eq!(completed.commits.len(), 1, "no second commit was made");
    assert_ne!(completed.commits[0], abandoned, "the rewrite is a revision of its own");
    assert_eq!(head_id(&fx.root()), completed.commits[0], "the head is the rewrite");
    assert_eq!(head_message(&fx.root()), "editor draft");
}

// GRD-FR-PQMX: a direct run's name is one line in its commit message. A line
// break or any other control character in the name is read as a space, and a
// name of nothing but those falls back to `Graduation run`.
#[test]
fn a_direct_runs_name_is_one_line_in_its_commit_message() {
    for (name, expected) in [
        ("  two\nlines\t ", "two lines"),
        ("\n\u{7}\n", "Graduation run"),
    ] {
        let fx = fixture();
        let draft = fx.committed_draft("direct");
        let mut run = fx.direct("Add the empty state.", &draft);
        run.input.draft_name = name.to_string();
        crate::graduation::save_run(&fx.app, &mut run).expect("a saved run");

        let run = fx.drive(
            &run,
            ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
        );

        assert_eq!(run.state, GraduationRunState::Completed, "{name:?}");
        assert_eq!(head_message(&fx.root()), expected, "{name:?}");
    }
}

// GRD-FR-PQMX: a run that is not direct commits under the first non-empty line
// of its prompt, with the draft named in the body. A prompt with no usable line
// falls back to `Graduation run`.
#[test]
fn a_stream_runs_message_is_its_prompt_line_and_its_draft() {
    for (prompt, title) in [
        ("\n\n# Add the empty state\n\nMore detail.", "Add the empty state"),
        ("###\n\nbody", "Graduation run"),
    ] {
        let fx = fixture();
        let stream = fx.stream("pqmx");
        let run = fx.enqueue(&stream, prompt);

        let run = fx.drive(
            &run,
            ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
        );

        assert_eq!(run.state, GraduationRunState::Completed, "{prompt:?}");
        let worktree = stream.worktree();
        assert_eq!(head_id(&worktree), run.commits[0], "{prompt:?}");
        assert_eq!(
            head_message(&worktree),
            format!("{title}\n\nGraduated from the draft \"{}\".", run.input.draft_name),
        );
    }
}
