//! Confirming and declining a change action request (CAR-FR-MMPV through
//! CAR-FR-XNQC).

use super::*;
use crate::graduation::GraduationRun;
use crate::openspec::Refusal;

/// CAR-FR-MMPV: the two starts of `OCR-openspec-change-runs.md` that a
/// confirmation calls. A seam, so a test can stand in for the run queue.
pub trait RunStarter {
    /// `start_change_apply` (OCR-FR-IDFC).
    fn start_apply(&self, stream_id: &str, change: &str) -> Result<GraduationRun, Refusal>;

    /// `start_change_rework_in_thread` (OCR-FR-TWDK).
    fn start_rework(
        &self,
        stream_id: &str,
        change: &str,
        instruction: &str,
        thread_id: &str,
        requester: &Participant,
    ) -> Result<GraduationRun, Refusal>;
}

/// The starts of the running application. Each takes the graduation write
/// guard and can reach the network, so a caller is on a blocking thread.
pub struct AppStarter<'a, R: tauri::Runtime> {
    pub app: &'a tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> RunStarter for AppStarter<'_, R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn start_apply(&self, stream_id: &str, change: &str) -> Result<GraduationRun, Refusal> {
        crate::graduation::start_change_apply_in_process(self.app, stream_id, change)
    }

    fn start_rework(
        &self,
        stream_id: &str,
        change: &str,
        instruction: &str,
        thread_id: &str,
        requester: &Participant,
    ) -> Result<GraduationRun, Refusal> {
        crate::graduation::start_change_rework_in_thread(
            self.app,
            stream_id,
            change,
            instruction,
            thread_id,
            requester,
        )
    }
}

/// The typed code of a refusal: the part before a colon (OCR contract
/// surface, typed refusals).
pub fn refusal_code(refusal: &str) -> &str {
    refusal.split(':').next().unwrap_or_default().trim()
}

/// CAR-FR-ARMK: the refusals that cannot clear, so the request fails.
fn is_final_refusal(code: &str) -> bool {
    code == ERR_UNKNOWN_STREAM || code == ERR_CHANGE_NOT_FOUND
}

/// What a decision comment says (CAR-FR-TQJZ).
enum Verdict<'a> {
    /// The title of the run that started.
    Started(&'a str),
    /// The code the request failed with.
    Failed(&'a str),
    Declined,
}

/// CAR-FR-TQJZ: the decision comment's body.
fn decision_body(
    record: &ChangeActionRequest,
    verdict: Verdict<'_>,
    feedback: Option<&str>,
) -> String {
    let action = record.action.as_str();
    let change = &record.change;
    let sentence = match verdict {
        Verdict::Started(title) => format!(
            "Confirmed the request to {action} the OpenSpec change `{change}`. The run `{title}` started."
        ),
        Verdict::Failed(code) => format!(
            "Confirmed the request to {action} the OpenSpec change `{change}`. The run did not start: `{code}`."
        ),
        Verdict::Declined => {
            format!("Declined the request to {action} the OpenSpec change `{change}`.")
        }
    };
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

/// CAR-FR-TQJZ: one human comment into the request's thread, on the terms of
/// NCP-FR-EBSA. A comment that does not append is logged, and the decision
/// stands.
fn append_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    thread: Option<&comments::Discussion>,
    record: &ChangeActionRequest,
    (body, by): (String, &Participant),
) -> Option<String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let subject = Subject::of(record);
    let Some(thread) = thread else {
        log_refusal(app, "decision_comment", comments::ERR_DISCUSSION_NOT_FOUND, subject);
        return None;
    };
    let comment_id = new_note_id();
    match comments::add_comment_with_id(
        store,
        thread.log_ref(),
        &record.thread_id,
        comment_id.clone(),
        body,
        by,
        &now_rfc3339(),
    ) {
        Ok(appended) => {
            crate::agent_conversations::retire_recoverable_for_human_comment(
                app,
                &record.thread_id,
            );
            comments::emit_discussion_changed(app, &appended);
            Some(comment_id)
        }
        Err(reason) => {
            log_refusal(app, "decision_comment", &reason, subject);
            None
        }
    }
}

/// CAR-FR-MMPV: confirm a pending request and start its run.
pub fn confirm_action_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    starter: &dyn RunStarter,
    request_id: &str,
    by: &Participant,
) -> Result<ChangeActionOutcome, Refusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (held, _) = read_request(store, request_id)
        .inspect_err(|code| log_refusal(app, "confirm", code, Subject::unknown(request_id)))?;
    // CAR-FR-OKOV: held across the start, so a second confirmation of the same
    // request reads it as decided.
    let _guard = change_lock(&held.stream_id, &held.change);
    let outcome = confirm_locked(app, store, starter, request_id, by);
    match &outcome {
        Ok(done) => log_record(app, "confirmed", &done.request),
        Err(refusal) => log_refusal(app, "confirm", refusal.code(), Subject::of(&held)),
    }
    outcome
}

fn confirm_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    starter: &dyn RunStarter,
    request_id: &str,
    by: &Participant,
) -> Result<ChangeActionOutcome, Refusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (mut record, dir) = read_request(store, request_id)?;
    if record.state != ChangeActionRequestState::Pending {
        return Err(ERR_ALREADY_DECIDED.into());
    }
    let started = match record.action {
        ChangeAction::Apply => starter.start_apply(&record.stream_id, &record.change),
        ChangeAction::Rework => starter.start_rework(
            &record.stream_id,
            &record.change,
            record.instruction.as_deref().unwrap_or_default(),
            &record.thread_id,
            &record.agent,
        ),
    };
    let thread = locate_thread(store, &record.stream_id, &record.change, &record.thread_id);
    match started {
        // CAR-FR-UPOV.
        Ok(run) => {
            record.state = ChangeActionRequestState::Started;
            record.run_id = Some(run.id.clone());
            record.last_refusal = None;
            record.decided_at = Some(now_rfc3339());
            // The run is enqueued now, so a record that does not write must not
            // turn the start into a refusal: the author would confirm again and
            // start a second run. One more try, then the decision goes on with
            // an `ERROR` record.
            if write_record(store, &dir, &record).is_err()
                && write_record(store, &dir, &record).is_err()
            {
                log_error(app, "record_started", ERR_WRITE_FAILED, &record);
            }
            let title = run.change_run_title().unwrap_or_default();
            let body = decision_body(&record, Verdict::Started(&title), None);
            let comment_id = append_decision(app, store, thread.as_ref(), &record, (body, by));
            announce(app, &record, ChangeActionCause::Decided);
            Ok(ChangeActionOutcome {
                request: record,
                run: Some(run),
                comment_id,
                origin_kind: origin_kind_of(thread.as_ref()),
            })
        }
        // CAR-FR-ARMK.
        Err(refusal) => {
            let code = refusal.code().to_string();
            record.last_refusal = Some(code.clone());
            if is_final_refusal(&code) {
                record.state = ChangeActionRequestState::Failed;
                record.decided_at = Some(now_rfc3339());
                write_record(store, &dir, &record)?;
                let body = decision_body(&record, Verdict::Failed(&code), None);
                append_decision(app, store, thread.as_ref(), &record, (body, by));
                // CAR-FR-ENOZ: a decision, beside the `WARN` of the refusal.
                log_record(app, "failed", &record);
                announce(app, &record, ChangeActionCause::Decided);
            } else {
                // The request stays `pending`, so a record that does not
                // write only loses the code of this refusal.
                if write_record(store, &dir, &record).is_err() {
                    log_refusal(app, "record_refusal", ERR_WRITE_FAILED, Subject::of(&record));
                }
                announce(app, &record, ChangeActionCause::Refused);
            }
            Err(refusal)
        }
    }
}

/// CAR-FR-WXDA: decline a pending request. Nothing starts.
pub fn decline_action_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    request_id: &str,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<ChangeActionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (held, _) = read_request(store, request_id)
        .inspect_err(|code| log_refusal(app, "decline", code, Subject::unknown(request_id)))?;
    let _guard = change_lock(&held.stream_id, &held.change);
    let outcome = decline_locked(app, store, request_id, feedback, by);
    match &outcome {
        Ok(done) => log_record(app, "declined", &done.request),
        Err(code) => log_refusal(app, "decline", code, Subject::of(&held)),
    }
    outcome
}

fn decline_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    request_id: &str,
    feedback: Option<&str>,
    by: &Participant,
) -> Result<ChangeActionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (mut record, dir) = read_request(store, request_id)?;
    if record.state != ChangeActionRequestState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    record.state = ChangeActionRequestState::Declined;
    record.decided_at = Some(now_rfc3339());
    write_record(store, &dir, &record)?;
    let thread = locate_thread(store, &record.stream_id, &record.change, &record.thread_id);
    let body = decision_body(&record, Verdict::Declined, feedback);
    let comment_id = append_decision(app, store, thread.as_ref(), &record, (body, by));
    announce(app, &record, ChangeActionCause::Decided);
    Ok(ChangeActionOutcome {
        request: record,
        run: None,
        comment_id,
        origin_kind: origin_kind_of(thread.as_ref()),
    })
}
