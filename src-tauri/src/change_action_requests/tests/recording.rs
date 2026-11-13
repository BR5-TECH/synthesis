//! Recording a change action request (CAR-FR-VQJC through CAR-FR-MNTP,
//! CAR-FR-ZSIQ, CAR-FR-ALHA, CAR-FR-IFGR, CAR-FR-ENOZ).

use super::*;

// CAR-FR-VQJC / CAR-FR-TJAD / CAR-FR-KYJU / CAR-FR-KZOZ / CMS-FR-QVZP: a
// recording writes one record below `action-requests/` of the change, pending,
// and appends one agent comment whose body is the rationale and whose one
// attachment names the request, its stream, its change, and its action.
#[test]
fn a_recording_writes_the_record_and_one_agent_comment() {
    let f = Fixture::new();
    let record =
        f.record(ChangeAction::Rework, Some(&format!("  {INSTRUCTION}  "))).expect("recorded");
    assert_eq!(record.state, ChangeActionRequestState::Pending);
    assert_eq!(record.action, ChangeAction::Rework);
    assert_eq!(record.instruction.as_deref(), Some(INSTRUCTION), "trimmed");
    assert_eq!(record.rationale, RATIONALE);
    assert_eq!(record.thread_id, f.thread_id);
    assert_eq!(record.agent, agent());
    assert!(
        record.run_id.is_none() && record.last_refusal.is_none() && record.decided_at.is_none()
    );
    assert_eq!(f.files(), vec![format!("{}.toml", record.id)]);
    assert_eq!(f.requests(), vec![record.clone()], "the file reads back whole");

    let thread = f.thread();
    assert_eq!(thread.comments.len(), 2);
    let comment = &thread.comments[1];
    assert_eq!(comment.id, record.comment_id);
    assert_eq!(comment.author, agent());
    assert_eq!(comment.body, RATIONALE);
    assert_eq!(
        comment.attachments,
        vec![comments::Attachment::ChangeActionRequest {
            request_id: record.id.clone(),
            stream_id: STREAM.into(),
            change: CHANGE.into(),
            action: "rework".into(),
        }]
    );
    // CMS-FR-QVZP: the wire kind the frontend reads.
    let wire = serde_json::to_value(&comment.attachments[0]).expect("json");
    assert_eq!(wire["kind"], "changeActionRequest");
    assert_eq!(wire["requestId"], record.id.as_str());
    assert_eq!(wire["streamId"], STREAM);
    assert_eq!(wire["action"], "rework");
}

// CAR-FR-TJAD / CAR contract surface: the record on the wire is camelCase, and
// an apply holds no instruction.
#[test]
fn an_apply_records_no_instruction_and_serves_camel_case() {
    let f = Fixture::new();
    let record = f.record(ChangeAction::Apply, Some("ignored")).expect("recorded");
    assert!(record.instruction.is_none());
    let wire = serde_json::to_value(&record).expect("json");
    for key in [
        "id",
        "streamId",
        "change",
        "action",
        "rationale",
        "threadId",
        "commentId",
        "agent",
        "state",
        "createdAt",
    ] {
        assert!(wire.get(key).is_some(), "{key} missing from {wire}");
    }
    assert_eq!(wire["action"], "apply");
    assert_eq!(wire["state"], "pending");
    for key in ["instruction", "runId", "lastRefusal", "decidedAt"] {
        assert!(wire.get(key).is_none(), "{key} present on a pending apply");
    }
}

// CAR-FR-MNTP: a recording is refused with `action_request_pending`,
// `discussion_resolved`, `unknown_stream`, `change_not_found`, and
// `blank_instruction`, and a refusal writes no record and no comment.
#[test]
fn a_recording_is_refused_and_writes_nothing() {
    let f = Fixture::new();
    // `blank_instruction`.
    assert_eq!(f.record(ChangeAction::Rework, Some("  \n")).unwrap_err(), ERR_BLANK_INSTRUCTION);
    assert_eq!(f.record(ChangeAction::Rework, None).unwrap_err(), ERR_BLANK_INSTRUCTION);
    // `unknown_stream` and `change_not_found`.
    let other = |stream_id: &str, change: &str| {
        record_change_action_request(
            &f.handle(),
            &f.store,
            &f.sites,
            NewActionRequest {
                stream_id,
                change,
                action: ChangeAction::Apply,
                instruction: None,
                rationale: RATIONALE,
                agent: &agent(),
                thread_id: &f.thread_id,
            },
        )
    };
    let gone = comments::open_discussion_unchecked(
        &f.store,
        &DiscussionTarget::Change { stream_id: "s2".into(), change: CHANGE.into() },
        None,
        "@arch".into(),
        Vec::new(),
        &human(),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    let gone_record = record_change_action_request(
        &f.handle(),
        &f.store,
        &f.sites,
        NewActionRequest {
            stream_id: "s2",
            change: CHANGE,
            action: ChangeAction::Apply,
            instruction: None,
            rationale: RATIONALE,
            agent: &agent(),
            thread_id: &gone.id,
        },
    );
    assert_eq!(gone_record.unwrap_err(), ERR_UNKNOWN_STREAM);
    let missing = comments::open_discussion_unchecked(
        &f.store,
        &DiscussionTarget::Change { stream_id: STREAM.into(), change: "no-such".into() },
        None,
        "@arch".into(),
        Vec::new(),
        &human(),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    let missing_record = record_change_action_request(
        &f.handle(),
        &f.store,
        &f.sites,
        NewActionRequest {
            stream_id: STREAM,
            change: "no-such",
            action: ChangeAction::Apply,
            instruction: None,
            rationale: RATIONALE,
            agent: &agent(),
            thread_id: &missing.id,
        },
    );
    assert_eq!(missing_record.unwrap_err(), ERR_CHANGE_NOT_FOUND);
    // A thread of another change is not the turn's conversation.
    assert_eq!(other(STREAM, "no-such").unwrap_err(), comments::ERR_DISCUSSION_NOT_FOUND);
    assert!(f.requests().is_empty());
    assert_eq!(f.thread().comments.len(), 1, "no comment");

    // `action_request_pending`, checked first.
    let first = f.record(ChangeAction::Apply, None).expect("recorded");
    assert_eq!(f.record(ChangeAction::Apply, None).unwrap_err(), ERR_ACTION_REQUEST_PENDING);
    assert_eq!(f.record(ChangeAction::Rework, Some("  ")).unwrap_err(), ERR_ACTION_REQUEST_PENDING);
    assert_eq!(f.requests(), vec![first.clone()]);
    f.decline(&first.id, None).expect("declined");

    // `discussion_resolved`.
    let thread = f.thread();
    comments::set_resolution_to(
        &f.store,
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
    let before = f.thread().comments.len();
    assert_eq!(f.record(ChangeAction::Apply, None).unwrap_err(), ERR_DISCUSSION_RESOLVED);
    assert_eq!(f.requests().len(), 1);
    assert_eq!(f.thread().comments.len(), before);
}

// CAR-FR-KZOZ: a comment that does not append removes the record, so the
// record and the comment land as one act.
#[test]
fn a_failed_append_removes_the_record() {
    let f = Fixture::new();
    let seen = f.watch();
    crate::change_action_requests::FAIL_APPEND.with(|fail| fail.set(true));
    assert_eq!(f.record(ChangeAction::Apply, None).unwrap_err(), ERR_WRITE_FAILED);
    assert!(f.files().is_empty(), "no record stays: {:?}", f.files());
    assert_eq!(f.thread().comments.len(), 1);
    assert!(seen.lock().unwrap().is_empty(), "nothing is announced");
    // The next recording is not held by the failed one.
    f.record(ChangeAction::Apply, None).expect("recorded");
}

// CAR-FR-ZSIQ: a recording emits `"change action requests changed"` with the
// stream, the change, the request, and the cause `recorded`.
#[test]
fn a_recording_is_announced() {
    let f = Fixture::new();
    let seen = f.watch();
    let record = f.record(ChangeAction::Apply, None).expect("recorded");
    assert_eq!(CHANGE_ACTION_REQUESTS_CHANGED, "change-action-requests-changed");
    let events = seen.lock().unwrap().clone();
    assert_eq!(
        events,
        vec![serde_json::json!({
            "streamId": STREAM,
            "change": CHANGE,
            "requestId": record.id,
            "cause": "recorded",
        })]
    );
}

// CAR-FR-ALHA: the list of a change is newest first and writes nothing; a
// change with none lists none.
#[test]
fn the_list_is_newest_first_and_writes_nothing() {
    let f = Fixture::new();
    let first = f.record(ChangeAction::Apply, None).expect("first");
    f.decline(&first.id, None).expect("declined");
    let second = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("second");
    let before = f.files();
    let listed = f.requests();
    assert_eq!(listed.iter().map(|r| r.id.clone()).collect::<Vec<_>>(), vec![second.id, first.id]);
    assert_eq!(f.files(), before);
    assert!(list_requests_impl(&f.store, STREAM, "other-change").is_empty());
    assert!(list_requests_impl(&f.store, "s9", CHANGE).is_empty());
}

// CAR-FR-IFGR: the read of a turn gives each request with the state of its
// run where one started, and writes nothing.
#[test]
fn the_read_of_a_turn_carries_the_run_state() {
    let f = Fixture::new();
    let first = f.record(ChangeAction::Apply, None).expect("first");
    let outcome = f.confirm(&FakeStarter::default(), &first.id).expect("started");
    let run_id = outcome.run.expect("run").id;
    let second = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("second");
    set_test_run_state(&run_id, "working");
    let before = f.files();
    let read = read_change_action_requests(&f.store, STREAM, CHANGE);
    assert_eq!(read.len(), 2);
    assert_eq!(read[0].request.id, second.id);
    assert_eq!(read[0].run_state, None);
    assert_eq!(read[1].request.run_id.as_deref(), Some(run_id.as_str()));
    assert_eq!(read[1].run_state.as_deref(), Some("working"));
    assert_eq!(f.files(), before);
}

// CAR-FR-ENOZ: a recording logs an `INFO` record with the ids, the action,
// and the state, and a refusal a `WARN` record with its code. No record holds
// the instruction or the rationale.
#[test]
fn a_recording_logs_ids_and_codes_alone() {
    crate::logging::BUFFER.clear();
    let f = Fixture::new();
    let record = f.record(ChangeAction::Rework, Some(INSTRUCTION)).expect("recorded");
    assert!(f.record(ChangeAction::Apply, None).is_err());
    let mine = backend_records(CHANGE);
    let info = mine
        .iter()
        .find(|r| r["level"] == "Info" && r["fields"]["operation"] == "recorded")
        .expect("info");
    assert_eq!(info["fields"]["streamId"], STREAM);
    assert_eq!(info["fields"]["requestId"], record.id.as_str());
    assert_eq!(info["fields"]["action"], "rework");
    assert_eq!(info["fields"]["state"], "pending");
    let warn = mine.iter().find(|r| r["level"] == "Warn").expect("warn");
    assert_eq!(warn["fields"]["code"], ERR_ACTION_REQUEST_PENDING);
    let all = serde_json::to_string(&mine).expect("json");
    assert!(!all.contains("Split task") && !all.contains("Shall I build"), "{all}");
}
