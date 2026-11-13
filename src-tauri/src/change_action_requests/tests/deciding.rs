//! Confirming and declining a change action request (CAR-FR-MMPV through
//! CAR-FR-OKOV, CAR-FR-ZSIQ, CAR-FR-UCSJ, CAR-FR-ENOZ).

use tauri::Manager;

use super::*;

// CAR-FR-MMPV / CAR-FR-UPOV / CAR-FR-TQJZ / CAR-FR-ZSIQ: a confirmed apply
// starts `start_change_apply`, writes the request `started` with the run id and
// `decided_at`, appends one human decision comment that names the run, and
// answers the request, the run, the comment, and the origin kind.
#[test]
fn a_confirmed_apply_starts_its_run() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let seen = f.watch();
    let starter = FakeStarter::default();
    let outcome = f.confirm(&starter, &record.id).expect("started");
    assert_eq!(starter.calls(), vec![format!("apply {STREAM} {CHANGE}")]);
    let run = outcome.run.clone().expect("the run");
    assert_eq!(outcome.request.state, ChangeActionRequestState::Started);
    assert_eq!(outcome.request.run_id.as_deref(), Some(run.id.as_str()));
    assert!(outcome.request.decided_at.is_some());
    assert_eq!(outcome.origin_kind, "change_discussion");
    assert_eq!(f.requests(), vec![outcome.request.clone()], "durable");

    let thread = f.thread();
    let decision = thread.comments.last().expect("decision comment");
    assert_eq!(outcome.comment_id.as_deref(), Some(decision.id.as_str()));
    assert_eq!(decision.author, human());
    assert_eq!(
        decision.body,
        format!(
            "Confirmed the request to apply the OpenSpec change `{CHANGE}`. The run `Apply {CHANGE}` started."
        )
    );
    assert!(decision.attachments.is_empty());
    let events = seen.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["cause"], "decided");
    assert_eq!(events[0]["requestId"], record.id.as_str());

    // CAR contract surface: the outcome on the wire.
    let wire = serde_json::to_value(&outcome).expect("json");
    assert_eq!(wire["originKind"], "change_discussion");
    assert_eq!(wire["request"]["state"], "started");
    assert_eq!(wire["request"]["runId"], run.id.as_str());
    assert_eq!(wire["run"]["id"], run.id.as_str());
    assert!(wire["commentId"].is_string());
}

// CAR-FR-MMPV / OCR-FR-TBGA: a confirmed rework starts
// `start_change_rework_in_thread` with the instruction, the request's thread,
// and its agent as the requester.
#[test]
fn a_confirmed_rework_starts_in_the_request_thread_as_its_agent() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("recorded");
    let starter = FakeStarter::default();
    let outcome = f.confirm(&starter, &record.id).expect("started");
    assert_eq!(
        starter.calls(),
        vec![format!("rework {STREAM} {CHANGE} {} arch {INSTRUCTION}", f.thread_id)]
    );
    assert_eq!(outcome.request.state, ChangeActionRequestState::Started);
    assert!(f.thread().comments.last().unwrap().body.contains(&format!("`Rework {CHANGE}`")));
}

// CAR-FR-ARMK / CAR-FR-ZSIQ: a refused start keeps the request `pending` with
// the code as its last refusal, appends no comment, returns the typed refusal
// as the start gave it, and is announced as `refused`. The author can confirm
// the same request again.
#[test]
fn a_refused_start_keeps_the_request_pending() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let comments_before = f.thread().comments.len();
    let seen = f.watch();
    let refusing = FakeStarter::refusing("openspec_run_active: run-7");
    assert_eq!(f.confirm(&refusing, &record.id).unwrap_err(), "openspec_run_active: run-7");
    let held = f.requests().remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Pending);
    assert_eq!(held.last_refusal.as_deref(), Some("openspec_run_active"));
    assert!(held.decided_at.is_none() && held.run_id.is_none());
    assert_eq!(f.thread().comments.len(), comments_before, "no comment");
    assert_eq!(seen.lock().unwrap()[0]["cause"], "refused");

    let outcome = f.confirm(&FakeStarter::default(), &record.id).expect("started on retry");
    assert_eq!(outcome.request.state, ChangeActionRequestState::Started);
    assert!(outcome.request.last_refusal.is_none());
}

// OFP-FR-QLQJ, CAR-FR-ARMK: a start refused with an `OpenSpecFailure` makes
// the confirmation reject with that object whole, on the wire too. The request
// stays pending with the code, and no comment carries a diagnostic.
#[test]
fn a_start_refused_with_an_openspec_failure_rejects_with_the_object() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let mut failure = crate::openspec::OpenSpecFailure::new("change_not_ready").for_change(CHANGE);
    failure.detail = Some("design".into());
    failure.diagnostics =
        vec![crate::openspec::CliDiagnostic::of("error", "SECRET-DIAGNOSTIC design")];
    let comments_before = f.thread().comments.len();
    let refused = f.confirm(&FakeStarter::failing(failure.clone()), &record.id).unwrap_err();
    assert_eq!(refused.openspec(), Some(&failure));
    let wire = serde_json::to_value(&refused).unwrap();
    assert!(wire.is_object(), "{wire}");
    assert_eq!(wire["code"], "change_not_ready");
    assert_eq!(wire["diagnostics"][0]["message"], "SECRET-DIAGNOSTIC design");
    let held = f.requests().remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Pending);
    assert_eq!(held.last_refusal.as_deref(), Some("change_not_ready"));
    let thread = f.thread();
    assert_eq!(thread.comments.len(), comments_before, "a refusal that is not final posts nothing");
    assert!(thread.comments.iter().all(|c| !c.body.contains("SECRET")));
    assert!(super::backend_records("SECRET").is_empty(), "a record holds a diagnostic");
}

// CAR-FR-ARMK / CAR-FR-TQJZ / CAR-FR-KYJU: `unknown_stream` and
// `change_not_found` write the request `failed` with the code, append the
// decision comment that names the code, and the request takes no decision
// after.
#[test]
fn a_gone_stream_or_change_fails_the_request() {
    for code in [ERR_UNKNOWN_STREAM, ERR_CHANGE_NOT_FOUND] {
        let f = Fixture::new();
        let record = f.record(ChangeAction::Apply, None).expect("recorded");
        let seen = f.watch();
        assert_eq!(f.confirm(&FakeStarter::refusing(code), &record.id).unwrap_err(), code);
        let failed = f.requests().remove(0);
        assert_eq!(failed.state, ChangeActionRequestState::Failed);
        assert_eq!(failed.last_refusal.as_deref(), Some(code));
        assert!(failed.decided_at.is_some());
        assert_eq!(
            f.thread().comments.last().unwrap().body,
            format!(
                "Confirmed the request to apply the OpenSpec change `{CHANGE}`. The run did not start: `{code}`."
            )
        );
        assert_eq!(seen.lock().unwrap()[0]["cause"], "decided");
        // CAR-FR-ENOZ: the failure is a decision and a refusal.
        let mine = backend_records(&record.id);
        assert!(mine.iter().any(|r| r["level"] == "Info" && r["fields"]["state"] == "failed"));
        assert!(mine.iter().any(|r| r["level"] == "Warn" && r["fields"]["code"] == code));
        let starter = FakeStarter::default();
        assert_eq!(f.confirm(&starter, &record.id).unwrap_err(), ERR_ALREADY_DECIDED);
        assert!(starter.calls().is_empty());
    }
}

// CAR-FR-WXDA / CAR-FR-TQJZ: a decline writes the request `declined` with
// `decided_at`, appends the decision comment with the feedback, and starts
// nothing.
#[test]
fn a_decline_starts_nothing_and_carries_the_feedback() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("recorded");
    let seen = f.watch();
    let outcome = f.decline(&record.id, Some("  Not yet.  ")).expect("declined");
    assert_eq!(outcome.request.state, ChangeActionRequestState::Declined);
    assert!(outcome.request.decided_at.is_some());
    assert!(outcome.run.is_none());
    let decision = f.thread().comments.last().unwrap().clone();
    assert_eq!(outcome.comment_id.as_deref(), Some(decision.id.as_str()));
    assert_eq!(
        decision.body,
        format!("Declined the request to rework the OpenSpec change `{CHANGE}`.\n\nNot yet.")
    );
    assert_eq!(seen.lock().unwrap()[0]["cause"], "decided");
    assert_eq!(f.requests()[0].state, ChangeActionRequestState::Declined);
}

// CAR-FR-XNQC / CAR-FR-KYJU: a decision on a request that is not pending is
// `already_decided`, and on no request `request_not_found`; neither writes.
#[test]
fn a_decision_needs_a_pending_request() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    f.decline(&record.id, None).expect("declined");
    let before = f.requests();
    let comments_before = f.thread().comments.len();
    assert_eq!(f.decline(&record.id, None).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(f.confirm(&FakeStarter::default(), &record.id).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(f.decline("no-such-id", None).unwrap_err(), ERR_REQUEST_NOT_FOUND);
    assert_eq!(f.confirm(&FakeStarter::default(), "../x").unwrap_err(), ERR_REQUEST_NOT_FOUND);
    assert_eq!(f.requests(), before);
    assert_eq!(f.thread().comments.len(), comments_before);
}

/// A run queue that holds its first start until the test releases it, and
/// says so on `events`. A later start answers at once and also says so, so a
/// broken exclusion shows as a second start and not as a hang.
struct HeldStarter {
    calls: Mutex<Vec<String>>,
    events: Mutex<std::sync::mpsc::Sender<&'static str>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl RunStarter for HeldStarter {
    fn start_apply(
        &self,
        stream_id: &str,
        change: &str,
    ) -> Result<GraduationRun, crate::openspec::Refusal> {
        let first = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(format!("apply {stream_id} {change}"));
            calls.len() == 1
        };
        if first {
            let _ = self.events.lock().unwrap().send("held");
            let _ = self.release.lock().unwrap().recv();
        } else {
            let _ = self.events.lock().unwrap().send("second start");
        }
        Ok(fake_run(crate::graduation::PHASE_APPLY))
    }

    fn start_rework(
        &self,
        _stream_id: &str,
        _change: &str,
        _instruction: &str,
        _thread_id: &str,
        _requester: &Participant,
    ) -> Result<GraduationRun, crate::openspec::Refusal> {
        Err("not_expected".into())
    }
}

/// Run a confirmation of `id` that is held inside its start, and `other` on a
/// second thread at the same time. The start is released when `other` waits
/// for the exclusion of the change, starts a run of its own, or ends.
/// It answers the confirmation, the result of `other`, the event that released
/// the start, and the starts.
fn while_a_start_is_held<T: Send>(
    f: &Fixture,
    id: &str,
    other: impl FnOnce(&HeldStarter) -> T + Send,
) -> (Result<ChangeActionOutcome, crate::openspec::Refusal>, T, &'static str, Vec<String>) {
    let (events_tx, events) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let starter = HeldStarter {
        calls: Mutex::default(),
        events: Mutex::new(events_tx.clone()),
        release: Mutex::new(release_rx),
    };
    let (app, store, by) = (f.handle(), f.store.clone(), human());
    let (starter_ref, app_ref, store_ref, by_ref) = (&starter, &app, &store, &by);
    let (first, second, released) = std::thread::scope(|scope| {
        let first = scope
            .spawn(move || confirm_action_request(app_ref, store_ref, starter_ref, id, by_ref));
        assert_eq!(events.recv().unwrap(), "held");
        let second = scope.spawn(move || {
            let waiting = events_tx.clone();
            super::super::LOCK_WAIT.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    let _ = waiting.send("waiting");
                }));
            });
            let out = other(starter_ref);
            let _ = events_tx.send("other finished");
            out
        });
        let released = events.recv().unwrap();
        release_tx.send(()).unwrap();
        (first.join().unwrap(), second.join().unwrap(), released)
    });
    let calls = starter.calls.lock().unwrap().clone();
    (first, second, released, calls)
}

// CAR-FR-OKOV: a second confirmation of one request, made while the first
// is inside its start, waits for the exclusion of the change and then reads
// the request as decided: one run starts, and the second is
// `already_decided`.
#[test]
fn two_confirmations_start_one_run() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let (app, store, by) = (f.handle(), f.store.clone(), human());
    let (first, second, released, calls) = while_a_start_is_held(&f, &record.id, |starter| {
        confirm_action_request(&app, &store, starter, &record.id, &by)
    });
    assert_eq!(released, "waiting", "the second confirmation waits for the first");
    assert_eq!(calls.len(), 1, "one start: {calls:?}");
    assert_eq!(first.expect("started").request.state, ChangeActionRequestState::Started);
    assert_eq!(second.unwrap_err(), ERR_ALREADY_DECIDED);
}

// CAR-FR-OKOV: a recording made while a confirmation of the same change is
// inside its start waits for it, so it reads the request as started and
// records a new pending request.
#[test]
fn a_recording_waits_for_a_confirmation_of_its_change() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let (app, store, sites, thread_id) =
        (f.handle(), f.store.clone(), &f.sites, f.thread_id.clone());
    let (first, second, released, calls) = while_a_start_is_held(&f, &record.id, |_| {
        record_change_action_request(
            &app,
            &store,
            sites,
            NewActionRequest {
                stream_id: STREAM,
                change: CHANGE,
                action: ChangeAction::Apply,
                instruction: None,
                rationale: RATIONALE,
                agent: &agent(),
                thread_id: &thread_id,
            },
        )
    });
    assert_eq!(released, "waiting", "the recording waits for the confirmation");
    assert_eq!(calls.len(), 1);
    assert_eq!(first.expect("started").request.state, ChangeActionRequestState::Started);
    let recorded = second.expect("recorded after the confirmation");
    let states: Vec<(String, ChangeActionRequestState)> =
        f.requests().into_iter().map(|r| (r.id, r.state)).collect();
    assert!(states.contains(&(record.id.clone(), ChangeActionRequestState::Started)), "{states:?}");
    assert!(states.contains(&(recorded.id, ChangeActionRequestState::Pending)), "{states:?}");
}

// CAR-FR-UCSJ: with no project open a decision errors with
// `no_project_open`.
#[test]
fn a_decision_needs_an_open_project() {
    let app = crate::tools::tests::mock_app();
    assert_eq!(decision_context(app.handle()).unwrap_err(), ERR_NO_PROJECT_OPEN);
    app.manage(crate::project::ProjectState::default());
    assert_eq!(decision_context(app.handle()).unwrap_err(), ERR_NO_PROJECT_OPEN);
}

// CAR-FR-UCSJ: `list_change_action_requests` with no project open errors with
// `no_project_open`.
#[test]
fn the_list_needs_an_open_project() {
    let app = crate::tools::tests::mock_app();
    app.manage(crate::project::ProjectState::default());
    let listed = list_change_action_requests(STREAM.into(), CHANGE.into(), app.state());
    assert_eq!(listed.unwrap_err(), ERR_NO_PROJECT_OPEN);
}

// CAR-FR-UCSJ / CMS-FR-12: a decision through its command returns the
// identity refusal of its comment, and writes nothing and starts nothing:
// the request stays pending, and the thread gains no comment.
#[test]
fn a_decision_command_returns_the_identity_refusal_and_writes_nothing() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let comments_before = f.thread().comments.len();
    let app = crate::tools::tests::mock_app();
    let project = crate::project::ProjectState::default();
    project.set_root(f.store.path().to_path_buf());
    app.manage(project);
    // Two tokens and no binding: the author must choose an identity first.
    let settings = crate::global_settings::GlobalSettingsStore::in_memory();
    let token = |id: &str| crate::github_tokens::GithubTokenRecord {
        id: id.into(),
        label: id.into(),
        account_login: Some(id.into()),
        ..Default::default()
    };
    settings.save_github_token_registry(vec![token("a"), token("b")]).unwrap();
    app.manage(settings);
    let selection = crate::github_tokens::ERR_SELECTION_REQUIRED;

    let declined = decline_change_action_request(app.handle().clone(), record.id.clone(), None);
    assert_eq!(declined.unwrap_err(), selection);
    let confirmed = tauri::async_runtime::block_on(confirm_change_action_request(
        app.handle().clone(),
        record.id.clone(),
    ));
    assert_eq!(confirmed.unwrap_err(), selection);
    assert_eq!(f.requests()[0].state, ChangeActionRequestState::Pending);
    assert!(f.requests()[0].run_id.is_none(), "no run started");
    assert_eq!(f.thread().comments.len(), comments_before, "no comment");
}

// CAR-FR-ENOZ: a decision logs an `INFO` record with the ids, the action, the
// state, and the run id, and a refusal a `WARN` record with the code alone. No
// record holds the instruction, the rationale, the feedback, or the detail of
// a refusal.
#[test]
fn a_decision_logs_ids_and_codes_alone() {
    crate::logging::BUFFER.clear();
    let f = Fixture::new();
    let record = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("recorded");
    let refusing = FakeStarter::refusing("main_branch_write_unconfirmed: a sentence of prose");
    assert!(f.confirm(&refusing, &record.id).is_err());
    let outcome = f.confirm(&FakeStarter::default(), &record.id).expect("started");
    let second = f.record(ChangeAction::Apply, None).expect("second");
    f.decline(&second.id, Some("Too early, the feedback text.")).expect("declined");
    let mine = backend_records(CHANGE);
    let confirmed =
        mine.iter().find(|r| r["fields"]["operation"] == "confirmed").expect("confirmed record");
    assert_eq!(confirmed["level"], "Info");
    assert_eq!(confirmed["fields"]["requestId"], record.id.as_str());
    assert_eq!(confirmed["fields"]["action"], "rework");
    assert_eq!(confirmed["fields"]["state"], "started");
    assert_eq!(confirmed["fields"]["runId"], outcome.run.unwrap().id.as_str());
    let refused = mine
        .iter()
        .find(|r| r["level"] == "Warn" && r["fields"]["operation"] == "confirm")
        .expect("refused record");
    assert_eq!(refused["fields"]["code"], "main_branch_write_unconfirmed");
    assert!(mine.iter().any(|r| r["fields"]["operation"] == "declined"));
    let all = serde_json::to_string(&mine).expect("json");
    for text in ["Split task", "Shall I build", "the feedback text", "a sentence of prose"] {
        assert!(!all.contains(text), "{text} in {all}");
    }
}

// CAR-FR-UPOV / CAR-FR-ENOZ: a start that lands but whose record does not
// write still answers the run, appends the decision comment, and is
// announced, with an `ERROR` record; the author is not led to confirm again.
#[cfg(unix)]
#[test]
fn a_started_run_is_answered_when_its_record_does_not_write() {
    use std::os::unix::fs::PermissionsExt;
    // A chmod does not bind root (a Docker CI image): there the write does
    // not fail, and this test has nothing to check.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_started_run_is_answered_when_its_record_does_not_write",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    let dir = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/action-requests"));
    let seen = f.watch();
    let starter = FakeStarter::default();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let outcome = f.confirm(&starter, &record.id);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let outcome = outcome.expect("the start is answered");
    assert_eq!(starter.calls().len(), 1);
    assert!(outcome.run.is_some());
    assert_eq!(outcome.request.state, ChangeActionRequestState::Started);
    assert!(f.thread().comments.last().expect("comment").body.starts_with("Confirmed"));
    assert_eq!(seen.lock().unwrap()[0]["cause"], "decided");
    assert!(!backend_records("change action request not stored").is_empty());
}
