//! Direct graduation: restart, the draft lifecycle and the queue events
//! (`GSU-graduation-start.md` GSU-FR-PVFP, `GRD-graduation.md` GRD-FR-BSNI).

use super::*;
use tauri::Listener;

/// GRD-FR-PNMU, GSU-FR-NPFB: a restart of a discarded direct run works on the
/// worktree and branch the discarded run pinned, whatever is active now, and
/// makes no clean check.
#[test]
fn a_restart_uses_the_pinned_worktree_and_branch() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let pinned = fx.branch();
    let draft = fx.committed_draft("direct");
    // Made after the commit, so the branch it checks out still holds the
    // project's own settings.
    fx.other_branch("elsewhere");
    let original = fx.start_direct(&draft).expect("a direct run");
    crate::graduation::discard_graduation_run(
        fx.app.clone(),
        original.id.clone(),
        Default::default(),
    )
    .unwrap();
    // The discarded run's work stays in the worktree, and the author moved on.
    std::fs::write(fx.root().join("left-over.md"), "kept\n").unwrap();
    fx.check_out("elsewhere");

    let restarted = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        original.id.clone(),
        "a-stream-the-caller-names".into(),
        StandingWork::CommitAndPush,
        Some("ignored".into()),
        Default::default(),
    )
    .expect("restarted");

    assert_eq!(restarted.direct_target, original.direct_target);
    assert_eq!(restarted.direct_target.as_ref().unwrap().branch, pinned);
    assert_eq!(restarted.standing_work, StandingWork::Keep);
    assert!(restarted.standing_work_message.is_none());
    assert_eq!(restarted.restarted_from_run_id.as_deref(), Some(original.id.as_str()));
    assert_eq!(fx.reload(&original.id).state, GraduationRunState::Discarded);
}

/// GSU-FR-NPFB: a worktree whose directory is gone cannot be restarted onto.
#[test]
fn a_restart_onto_a_missing_worktree_is_refused() {
    let fx = Fixture::new();
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = fx.direct_in(Path::new("/no/such/worktree"), "main", None, "p", "d1");
    crate::graduation::discard_graduation_run(fx.app.clone(), run.id.clone(), Default::default())
        .unwrap();

    let refused = crate::graduation::restart_graduation_run(
        fx.app.clone(),
        run.id,
        String::new(),
        StandingWork::Keep,
        None,
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(refused, ERR_DIRECT_WORKTREE_MISSING);
}

/// DRS-FR-NZSQ, DRS-FR-KQTW: a direct run locks its draft, graduates it when it
/// commits, and releases it when it is discarded, as a stream run does.
#[test]
fn a_direct_run_applies_the_graduated_draft_lifecycle() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("work", &draft);
    assert!(crate::graduation::require_unlocked_draft(&fx.app, &draft).is_err());

    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    let queue = crate::graduation::project_queue(&fx.app).unwrap();
    assert!(crate::graduation::draft_graduation_stands(&queue, &draft));

    let access = fx.app.state::<crate::fs::FsAccessState>().get().unwrap();
    let root = crate::fs::RootFs::new(fx.root(), access);
    let mut record = crate::drafts::draft_record(&root, &draft).unwrap();
    crate::drafts::resolve_graduated_status(&fx.app, &mut record);
    assert_eq!(record.status, crate::drafts::DraftStatus::Graduated);
}

/// DRS-FR-KQTW: a discarded direct run releases the status it held.
#[test]
fn discarding_the_only_committing_direct_run_releases_the_draft() {
    let fx = Fixture::new();
    let draft = fx.committed_draft("direct");
    let run = fx.direct("work", &draft);
    let done = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().writing("src/panel.ts", "1\n"), Turn::ready()]),
    );
    assert_eq!(done.state, GraduationRunState::Completed);
    let mut stored = fx.reload(&run.id);
    stored.state = GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut stored).unwrap();
    let queue = crate::graduation::project_queue(&fx.app).unwrap();
    assert!(!crate::graduation::draft_graduation_stands(&queue, &draft));
}

/// GRD-FR-EFAU: a queue change names the stream and the worktree it is about.
#[test]
fn a_queue_event_names_the_worktree_of_a_direct_run() {
    let fx = Fixture::new();
    let seen = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let sink = seen.clone();
    fx.app.listen(GRADUATION_QUEUE_CHANGED, move |event| {
        sink.lock().unwrap().push(serde_json::from_str(event.payload()).unwrap());
    });
    let stream = fx.stream("editor");
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let direct = fx.direct("work", "d1");
    let mut again = fx.reload(&direct.id);
    again.auto_start = false;
    crate::graduation::set_graduation_auto_start(
        fx.app.clone(),
        direct.id.clone(),
        false,
        Default::default(),
    )
    .unwrap();
    let _ = stream;

    let events = seen.lock().unwrap().clone();
    let payload = events.last().expect("a queue event");
    assert_eq!(payload["streamId"], "");
    assert_eq!(payload["worktreePath"], fx.root().to_string_lossy().as_ref());
}

/// GSU-FR-GTRO: a refused start deletes no stream, whichever start refused.
#[test]
fn a_refused_start_keeps_the_stream_that_stood_before_it() {
    let fx = Fixture::new();
    let stream = fx.stream("created in the dialog");
    // The image preflight refuses on a machine with nothing configured.
    let draft = fx.committed_draft("direct");
    let refused = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
        None,
        Default::default(),
    );
    assert!(refused.is_err());
    let refused_direct = fx.start_direct(&draft);
    assert!(refused_direct.is_err());
    let listed = crate::streams::list_work_streams(fx.app.clone(), Default::default()).unwrap();
    assert!(listed.iter().any(|summary| summary.stream.id == stream.id));
}
