//! MPR-FR-LJRO, MPR-FR-GIJN, MPR-FR-NWEZ, MPR-FR-QIJF, MPR-FR-LOUI,
//! CTA-FR-YWSU: the re-prepare request.

use super::super::reprepare::{request_body, TurnRequest};
use super::*;
use crate::comments::Comment;

/// A change to issue `number` whose `updated_at` is older than GitHub's, so
/// its accept fails with `changed_on_tracker`.
fn stale_row(key: &str, number: u64) -> MapPlanRow {
    let mut row = modify_row(key, number, vec![change("title", json!(format!("T{number}")))]);
    if let RowFields::ModifyIssue(f) = &mut row.fields {
        f.base_updated_at = "2025-12-31T00:00:00Z".into();
    }
    row
}

/// A change whose accept fails with `status_unknown`.
fn bad_status_row(key: &str, number: u64) -> MapPlanRow {
    modify_row(key, number, vec![change("status", json!("Limbo"))])
}

fn turns(plan: &Plan) -> Vec<TurnRequest> {
    plan.turns.lock().unwrap().clone()
}

fn comments(plan: &Plan) -> Vec<Comment> {
    crate::comments::fold_map_discussions(&plan.store())
        .into_iter()
        .find(|d| d.id == plan.discussion)
        .unwrap()
        .comments
}

fn request(plan: &Plan, proposal: &MapPlanProposal) -> Result<MapPlanProposal, MapPlanError> {
    request_reprepare(&plan.app(), &plan.discussion, &proposal.id)
}

/// Each "map plan proposals changed" event: the discussion it names, and
/// whether a proposal of that discussion recorded a request when the event
/// was emitted.
type Watched = Arc<Mutex<Vec<(String, bool)>>>;

fn watch(plan: &Plan) -> Watched {
    use tauri::Listener;
    let seen: Watched = Arc::default();
    let sink = Arc::clone(&seen);
    let store = plan.store();
    plan.app().listen(MAP_PLAN_PROPOSALS_CHANGED, move |event| {
        let payload: Value = serde_json::from_str(event.payload()).unwrap();
        let discussion = payload["discussionId"].as_str().unwrap_or_default().to_string();
        let requested = crate::map_plan::proposals_in(&store, &discussion)
            .iter()
            .any(|p| p.reprepare_requested_at.is_some());
        sink.lock().unwrap().push((discussion, requested));
    });
    seen
}

/// The events that `watch` saw. Tauri can queue an emit that finds its
/// listener table busy, and it delivers the queue in order on the next emit.
/// So when the fence event of this call is seen, each earlier event is seen.
fn watched(plan: &Plan, seen: &Watched) -> Vec<(String, bool)> {
    use tauri::{Emitter, Listener};
    const FENCE: &str = "reprepare-test-fence";
    let fenced = Arc::new(Mutex::new(false));
    let sink = Arc::clone(&fenced);
    plan.app().listen(FENCE, move |_| *sink.lock().unwrap() = true);
    plan.app().emit(FENCE, ()).unwrap();
    crate::delivery::github::tests::wait_until("the fence event", || *fenced.lock().unwrap());
    seen.lock().unwrap().clone()
}

/// The WARN records of the re-prepare operation in this process.
fn reprepare_warnings() -> Vec<crate::logging::LogRecord> {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..Default::default()
    };
    crate::logging::BUFFER
        .query(&filter, None, 1000)
        .unwrap()
        .records
        .into_iter()
        .filter(|r| r.level == crate::logging::LogLevel::Warn)
        .filter(|r| r.fields.get("operation") == Some(&json!("reprepare")))
        .collect()
}

/// MPR-FR-LJRO, MPR-FR-NWEZ, MPR-FR-QIJF, CTA-FR-YWSU: the decision of the last
/// waiting row, where a row failed with `changed_on_tracker`, appends one
/// Synthesis comment that names each failed issue and the agent, dispatches
/// one turn of the agent that proposed with that comment as its trigger, and
/// records `reprepareRequestedAt`. No request is made while a row waits.
#[test]
fn the_last_decision_requests_a_reprepare_once() {
    let plan = setup();
    let seen = watch(&plan);
    let d = plan.discussion.clone();
    let proposal =
        record(&plan, vec![stale_row("m", 2), stale_row("n", 3), draft_row("d", "D", "x")]);
    let before = comments(&plan).len();
    let first = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(row_of(&first, "m").error.as_ref().unwrap().code, "changed_on_tracker");
    accept(&plan, &proposal, "n").unwrap();
    assert!(turns(&plan).is_empty(), "a row still waits");
    // Each event before the last decision is seen before that decision runs, so
    // the flag of each one is read before any request is recorded.
    assert_eq!(watched(&plan, &seen), [(d.clone(), false), (d.clone(), false), (d.clone(), false)]);
    assert_eq!(comments(&plan).len(), before);
    let last = reject(&plan, &proposal, "d").unwrap();
    let turn = turns(&plan);
    assert_eq!(turn.len(), 1);
    assert_eq!(turn[0].nickname, "planner");
    assert_eq!(turn[0].discussion_id, plan.discussion);
    let all = comments(&plan);
    assert_eq!(all.len(), before + 1);
    let asked = all.last().unwrap();
    assert_eq!(asked.id, turn[0].trigger_comment_id);
    assert_eq!(asked.author, crate::comments::Participant::synthesis());
    assert_eq!(
        asked.body,
        "#2 and #3 changed on GitHub after the proposal, so nothing was written. \
         @planner, read these issues again and propose the changes again."
    );
    assert!(last.reprepare_requested_at.is_some());
    assert_eq!(listed(&plan)[0], last);
    let wire = serde_json::to_value(&last).unwrap();
    assert_eq!(wire["reprepareRequestedAt"], json!(last.reprepare_requested_at));
    // MPR-FR-QIJF, MPR-FR-SGYA: the record, two accepts, and the reject each
    // emit once for the discussion. The proposal records the request before the
    // event of the decision that made it.
    assert_eq!(watched(&plan, &seen)[3..], [(d, true)]);
    assert_eq!(seen.lock().unwrap().len(), 4);
}

/// MPR-FR-LJRO: a proposal that records a request asks no second time, also
/// after a reopen and a new decision.
#[test]
fn a_recorded_request_is_not_made_again() {
    let plan = setup();
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    accept(&plan, &proposal, "m").unwrap();
    assert_eq!(turns(&plan).len(), 1);
    reopen_row(&plan.app(), &plan.discussion, &proposal.id, "d").unwrap();
    let again = reject(&plan, &proposal, "d").unwrap();
    assert_eq!(turns(&plan).len(), 1);
    assert!(again.reprepare_requested_at.is_some());
}

/// MPR-FR-LJRO: a failure other than `changed_on_tracker`, and a proposal with
/// no failed row, request nothing on their own.
#[test]
fn other_failures_request_nothing_on_their_own() {
    let plan = setup();
    let proposal = record(&plan, vec![bad_status_row("m", 2), draft_row("d", "D", "x")]);
    accept(&plan, &proposal, "m").unwrap();
    let decided = reject(&plan, &proposal, "d").unwrap();
    assert!(turns(&plan).is_empty());
    assert!(decided.reprepare_requested_at.is_none());
}

/// MPR-FR-LOUI, MPR-FR-LJRO: a turn that cannot be dispatched records no
/// request and leaves the decision a success. The comment that was appended
/// stays. A later decision may ask again.
#[test]
fn a_failed_dispatch_records_no_request() {
    let plan = setup();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_not_found".into());
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    let before = comments(&plan).len();
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Failed);
    assert!(decided.reprepare_requested_at.is_none());
    assert!(listed(&plan)[0].reprepare_requested_at.is_none());
    assert_eq!(turns(&plan).len(), 1, "the dispatch was tried");
    let all = comments(&plan);
    assert_eq!(all.len(), before + 1, "the appended comment stays");
    assert_eq!(all.last().unwrap().author, crate::comments::Participant::synthesis());
    *plan.dispatch_failure.lock().unwrap() = None;
    assert!(request(&plan, &proposal).unwrap().reprepare_requested_at.is_some());
}

/// MPR-FR-LOUI: a comment that cannot be appended starts no turn and records
/// no request. The decision stays a success.
#[test]
fn a_failed_comment_starts_no_turn() {
    let plan = setup();
    let proposal = record(&plan, vec![stale_row("m", 2), draft_row("d", "D", "x")]);
    reject(&plan, &proposal, "d").unwrap();
    crate::comments::set_resolution_to(
        &plan.store(),
        crate::comments::ThreadRef::map_discussion(),
        &plan.discussion,
        true,
        &human(),
        AT,
    )
    .unwrap();
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Failed);
    assert!(decided.reprepare_requested_at.is_none());
    assert!(turns(&plan).is_empty());
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "discussion_resolved");
    assert!(turns(&plan).is_empty());
}

/// MPR-FR-GIJN, MPR-FR-NWEZ, MPR-FR-QIJF, MPR-FR-SGYA: the author can ask for
/// any failed row. The comment names a row that could not be applied, and the
/// agent. The proposal records `reprepareRequestedAt`, and the module emits
/// "map plan proposals changed" for the discussion.
#[test]
fn the_author_requests_a_reprepare() {
    let plan = setup();
    let seen = watch(&plan);
    let d = plan.discussion.clone();
    let proposal = record(&plan, vec![bad_status_row("m", 2)]);
    accept(&plan, &proposal, "m").unwrap();
    assert!(turns(&plan).is_empty());
    assert_eq!(watched(&plan, &seen), [(d.clone(), false), (d.clone(), false)]);
    let asked = request(&plan, &proposal).unwrap();
    // MPR-FR-QIJF, MPR-FR-SGYA: the request emits once for the discussion, after
    // the proposal records it.
    assert_eq!(watched(&plan, &seen)[2..], [(d, true)]);
    assert_eq!(seen.lock().unwrap().len(), 3);
    assert!(asked.reprepare_requested_at.is_some());
    assert_eq!(listed(&plan)[0], asked);
    let turn = turns(&plan);
    assert_eq!(turn.len(), 1);
    let last = comments(&plan).pop().unwrap();
    assert_eq!(last.id, turn[0].trigger_comment_id);
    assert_eq!(
        last.body,
        "#2 could not be applied. @planner, read this issue again and propose the change again."
    );
}

/// MPR-FR-GIJN: a request is refused with `nothing_failed`, `proposal_open`, or
/// `reprepare_requested`, and a refusal appends nothing and dispatches nothing.
#[test]
fn a_request_of_the_author_is_refused() {
    let plan = setup();
    let none_failed = record(&plan, vec![draft_row("a", "A", "x")]);
    reject(&plan, &none_failed, "a").unwrap();
    assert_eq!(request(&plan, &none_failed).unwrap_err().code, "nothing_failed");
    let open = record(&plan, vec![bad_status_row("m", 2), draft_row("d", "D", "x")]);
    accept(&plan, &open, "m").unwrap();
    let before = comments(&plan).len();
    assert_eq!(request(&plan, &open).unwrap_err().code, "proposal_open");
    reject(&plan, &open, "d").unwrap();
    request(&plan, &open).unwrap();
    let after = comments(&plan).len();
    assert_eq!(request(&plan, &open).unwrap_err().code, "reprepare_requested");
    assert_eq!(after, before + 1);
    assert_eq!(comments(&plan).len(), after);
    assert_eq!(turns(&plan).len(), 1);
    let app = plan.app();
    assert_eq!(
        request_reprepare(&app, &plan.discussion, "nope").unwrap_err().code,
        "proposal_not_found"
    );
    plan.h.close();
    assert_eq!(request(&plan, &open).unwrap_err().code, "no_project_open");
}

/// MPR-FR-GIJN, MPR-FR-LOUI: a request of the author whose turn cannot be
/// dispatched is refused with the typed error of the dispatch and records no
/// request.
#[test]
fn a_failed_dispatch_refuses_the_request_of_the_author() {
    let plan = setup();
    let proposal = record(&plan, vec![bad_status_row("m", 2)]);
    accept(&plan, &proposal, "m").unwrap();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_unavailable".into());
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "agent_unavailable");
    assert!(listed(&plan)[0].reprepare_requested_at.is_none());
}

/// MPR-FR-NWEZ: the text names each failed row, the changed ones first, and a
/// row that makes something new by its name.
#[test]
fn the_request_names_each_failed_row() {
    let plan = setup();
    let mut proposal = record(&plan, vec![stale_row("m", 18), stale_row("n", 11)]);
    let fail = |row: &mut MapPlanRow, code: &str| {
        row.state = RowState::Failed;
        row.error = Some(crate::map_plan::RowError { code: code.into(), message: String::new() });
    };
    let mut created = create_row("c", new_issue("Docs"));
    fail(&mut created, "label_unknown");
    let mut drafted = draft_row("d", "Relay", "x");
    fail(&mut drafted, "draft_create_failed");
    let mut other = bad_status_row("o", 5);
    fail(&mut other, "status_unknown");
    proposal.rows.iter_mut().for_each(|r| fail(r, "changed_on_tracker"));
    proposal.rows.extend([created, drafted, other, draft_row("w", "Waits", "y")]);
    assert_eq!(
        request_body(&proposal, "planner"),
        "#18 and #11 changed on GitHub after the proposal, so nothing was written. \
         The new issue \"Docs\", the new change \"Relay\", and #5 could not be applied. \
         @planner, read these issues again and propose the changes again."
    );
}

/// MPR-FR-QUYY: the field is absent on the wire where no request was made, and
/// a stored proposal without it reads as no request.
#[test]
fn a_proposal_without_a_request_has_no_field() {
    let plan = setup();
    let proposal = record(&plan, vec![draft_row("a", "A", "x")]);
    let wire = serde_json::to_value(&proposal).unwrap();
    assert!(wire.get("reprepareRequestedAt").is_none());
    let back: MapPlanProposal = serde_json::from_value(wire).unwrap();
    assert!(back.reprepare_requested_at.is_none());
}

/// MPR-FR-LOUI, MPR-FR-KZRN: a dispatch that fails logs the typed error, for a
/// request on its own and for a request of the author. No record carries a
/// title, a label name, or the text of the comment. No request is recorded, and
/// each appended Synthesis comment stays in the log.
#[test]
fn a_failed_dispatch_is_logged_with_its_typed_error_and_no_content() {
    let plan = setup();
    *plan.dispatch_failure.lock().unwrap() = Some("agent_unavailable".into());
    let mut fields = new_issue("TITLE-SECRET-7");
    fields.labels = vec!["LABEL-SECRET-8".into()];
    let proposal = record(&plan, vec![stale_row("m", 2), create_row("c", fields)]);
    let before = comments(&plan).len();
    accept(&plan, &proposal, "c").unwrap();
    let decided = accept(&plan, &proposal, "m").unwrap();
    assert_eq!(state_of(&decided, "m"), RowState::Failed);
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "agent_unavailable");
    assert!(listed(&plan)[0].reprepare_requested_at.is_none(), "no request is recorded");

    let all = comments(&plan);
    assert_eq!(all.len(), before + 2, "both appended comments stay");
    let turn = turns(&plan);
    assert_eq!(turn.len(), 2, "each dispatch was tried");
    for (asked, turn) in all[before..].iter().zip(&turn) {
        assert_eq!(asked.author, crate::comments::Participant::synthesis());
        assert_eq!(asked.id, turn.trigger_comment_id);
        assert!(
            asked.body.contains("new issue \"TITLE-SECRET-7\" could not be applied"),
            "{}",
            asked.body
        );
    }

    let warned = reprepare_warnings();
    let shape: Vec<(Value, Value, Value)> = warned
        .iter()
        .map(|r| (r.fields["outcome"].clone(), r.fields["kind"].clone(), r.fields["error"].clone()))
        .collect();
    assert_eq!(
        shape,
        [
            (json!("failed"), json!("automatic"), json!("agent_unavailable")),
            (json!("refused"), json!("manual"), json!("agent_unavailable")),
        ]
    );
    let text = serde_json::to_string(&warned).unwrap();
    for content in ["TITLE-SECRET-7", "LABEL-SECRET-8", "@planner", "could not be applied"] {
        assert!(!text.contains(content), "a record carries {content}");
    }
}

/// MPR-FR-QIJF, MPR-FR-GIJN: where the comment of the proposal was not written
/// by an agent, the request is refused with `discussion_not_found`. It appends
/// nothing, dispatches nothing, and records no request. A proposal of an agent
/// in the same discussion is still asked for.
#[test]
fn a_proposal_not_written_by_an_agent_is_not_reprepared() {
    let plan = setup();
    let by_agent = record(&plan, vec![bad_status_row("a", 3)]);
    accept(&plan, &by_agent, "a").unwrap();
    let by_human = ProposalComment { author: human(), ..comment() };
    let proposal = record_map_plan_proposal(
        &plan.app(),
        &plan.discussion,
        vec![bad_status_row("m", 2)],
        by_human,
    )
    .unwrap();
    accept(&plan, &proposal, "m").unwrap();
    let before = comments(&plan);
    assert_eq!(request(&plan, &proposal).unwrap_err().code, "discussion_not_found");
    assert_eq!(comments(&plan), before, "nothing is appended");
    assert!(turns(&plan).is_empty());
    assert!(listed(&plan).iter().all(|p| p.reprepare_requested_at.is_none()));
    assert!(request(&plan, &by_agent).unwrap().reprepare_requested_at.is_some());
    assert_eq!(turns(&plan).len(), 1);
    assert_eq!(comments(&plan).len(), before.len() + 1);
}

/// MPR-FR-NWEZ: the comment that a request appends names a failed new issue and
/// a failed new draft by their titles, and a failed change by its number.
#[test]
fn the_request_comment_names_new_issues_and_drafts_by_title() {
    let plan = setup();
    let mut fields = new_issue("Docs");
    fields.labels = vec!["no-such-label".into()];
    let proposal = record(
        &plan,
        vec![create_row("c", fields), draft_row("d", "Relay", "x"), bad_status_row("m", 2)],
    );
    assert_eq!(state_of(&accept(&plan, &proposal, "c").unwrap(), "c"), RowState::Failed);
    assert_eq!(state_of(&accept(&plan, &proposal, "m").unwrap(), "m"), RowState::Failed);
    // DRS-FR-39: a project settings file that cannot be read fails the draft.
    let settings = ".synthesis/project.toml";
    let kept = std::fs::read_to_string(plan.h.dir.path().join(settings)).ok();
    plan.h.write(settings, "this is = not [ toml");
    let drafted = accept(&plan, &proposal, "d").unwrap();
    match kept {
        Some(text) => plan.h.write(settings, &text),
        None => std::fs::remove_file(plan.h.dir.path().join(settings)).unwrap(),
    }
    assert_eq!(row_of(&drafted, "d").error.as_ref().unwrap().code, "draft_create_failed");
    assert!(turns(&plan).is_empty(), "no row failed with changed_on_tracker");
    request(&plan, &proposal).unwrap();
    let asked = comments(&plan).pop().unwrap();
    assert_eq!(asked.id, turns(&plan)[0].trigger_comment_id);
    assert_eq!(
        asked.body,
        "The new issue \"Docs\", the new change \"Relay\", and #2 could not be applied. \
         @planner, read these issues again and propose the changes again."
    );
}
