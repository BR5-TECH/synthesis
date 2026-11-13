//! The merge notice of a stream merge that its archive step refused for the
//! content of a change (`OFP-openspec-failure-policy.md` OFP-FR-ETEN through
//! OFP-FR-HMED).
//!
//! `merge_work_stream` calls it alone, after it has released the repository
//! update guard and every other guard it holds: a run cannot claim the stream
//! under the guard (WKS-FR-RQVM), and the graduation write guard that a start
//! takes is not reentrant. The notice opens one whole-change discussion as the
//! Synthesis participant, and for a repair cause it starts one rework run in
//! that thread. The digest guard keeps it to one notice for one state of the
//! change files, so a repair that does not fix the change cannot start a loop.

use serde::Serialize;

use super::notice_record::{read_notice_record, write_notice_record, NoticeRecord};
use super::{change_folder_digest, ERR_ARCHIVE_FAILED, ERR_CHANGE_NOT_ARCHIVABLE};
use crate::comments::{self, Participant};
use crate::fs::RootFs;
use crate::log_fields;
use crate::logging::{self, Domain, BUFFER};
use crate::openspec::failure::REPAIR_CAUSES;
use crate::openspec::{ChangeLocation, FailureClass, OpenSpecFailure};

/// OFP contract surface `MergeNotice`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeNotice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_refusal: Option<String>,
}

impl MergeNotice {
    /// OFP-FR-MQGG: the notice on the failure of the merge refusal.
    pub fn apply_to(&self, failure: &mut OpenSpecFailure) {
        failure.thread_id = self.thread_id.clone();
        failure.repair_run_id = self.repair_run_id.clone();
        failure.repair_refusal = self.repair_refusal.clone();
    }

    fn of_record(record: &NoticeRecord) -> Self {
        MergeNotice {
            thread_id: Some(record.thread_id.clone()),
            repair_run_id: record.repair_run_id.clone(),
            repair_refusal: record.repair_refusal.clone(),
        }
    }
}

/// OFP-FR-ETEN: a merge refusal with `archive_failed` or
/// `change_not_archivable` whose classifying code is of the class `content`.
pub fn notice_applies(failure: &OpenSpecFailure) -> bool {
    (failure.code == ERR_ARCHIVE_FAILED || failure.code == ERR_CHANGE_NOT_ARCHIVABLE)
        && failure.change.is_some()
        && failure.class() == FailureClass::Content
}

/// OFP-FR-HQQN: a cause that a repair run can fix.
pub fn is_repair_cause(cause: &str) -> bool {
    REPAIR_CAUSES.contains(&cause)
}

/// OFP contract surface, fixed text: the notice text of `cause`, which for a
/// repair cause is also the instruction of the repair run.
pub fn notice_text(change: &str, cause: &str) -> String {
    let head =
        format!("The stream merge could not archive the OpenSpec change \"{change}\" ({cause}).");
    if is_repair_cause(cause) {
        format!(
            "{head} Change the deltas of \"{change}\" so that they apply to the live specifications and the change validates with \"openspec validate {change} --strict\". The OpenSpec CLI reported:"
        )
    } else {
        format!("{head} Nothing was merged. The OpenSpec CLI reported:")
    }
}

/// OFP-FR-EJPE: the first comment: the notice text, then one line for each
/// diagnostic.
pub fn notice_body(change: &str, failure: &OpenSpecFailure) -> String {
    let mut body = notice_text(change, failure.classifying_code());
    for diagnostic in &failure.diagnostics {
        body.push('\n');
        body.push_str(&diagnostic.line());
    }
    body
}

/// OFP-FR-ETEN through OFP-FR-HMED: give the merge notice of `failure` for
/// `change` on `stream_id`, and answer what the merge refusal carries. A
/// failure that gives no notice answers an empty notice and writes nothing.
pub fn give_merge_notice<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    failure: &OpenSpecFailure,
) -> MergeNotice {
    use tauri::Manager;
    if !notice_applies(failure) {
        return MergeNotice::default();
    }
    let cause = failure.classifying_code().to_string();
    let store =
        app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_store().ok());
    let location = ChangeLocation::Stream { stream_id: stream_id.to_string() };
    let digest = crate::openspec::change::working_copy(app, &location)
        .ok()
        .and_then(|at| change_folder_digest(&at, change));
    let (Some(store), Some(digest)) = (store, digest) else {
        warn(
            app,
            stream_id,
            change,
            &cause,
            "merge notice could not read the change",
            "store_unavailable",
        );
        return MergeNotice::default();
    };
    // OFP-FR-PSVR: the same change files give no second notice and no second
    // run.
    if let Some(record) = read_notice_record(&store, stream_id, change) {
        if record.digest == digest {
            let notice = MergeNotice::of_record(&record);
            info(
                app,
                stream_id,
                change,
                &cause,
                "merge notice already given for these change files",
                &notice,
            );
            return notice;
        }
    }
    // OFP-FR-EJPE, OFP-FR-PWAV: the discussion, or no notice at all.
    let body = notice_body(change, failure);
    let thread_id = match open_notice_thread(app, &store, stream_id, change, &body) {
        Ok(thread_id) => thread_id,
        Err(code) => {
            warn(
                app,
                stream_id,
                change,
                &cause,
                "merge notice could not open its discussion",
                &code,
            );
            return MergeNotice::default();
        }
    };
    let mut notice = MergeNotice { thread_id: Some(thread_id.clone()), ..MergeNotice::default() };
    info(app, stream_id, change, &cause, "merge notice given", &notice);
    // OFP-FR-XQDY, OFP-FR-PCQY: the repair run, or the code that refused it.
    if is_repair_cause(&cause) {
        match crate::graduation::start_change_rework_in_thread(
            app,
            stream_id,
            change,
            &body,
            &thread_id,
            &Participant::synthesis(),
        ) {
            Ok(run) => {
                notice.repair_run_id = Some(run.id);
                info(app, stream_id, change, &cause, "merge notice started a repair run", &notice);
            }
            Err(refusal) => {
                let code = refusal.code().to_string();
                warn(app, stream_id, change, &cause, "merge notice repair start refused", &code);
                notice.repair_refusal = Some(code);
            }
        }
    }
    // OFP-FR-HVHV: the record, after the notice.
    let record = NoticeRecord {
        digest,
        cause: cause.clone(),
        thread_id,
        repair_run_id: notice.repair_run_id.clone(),
        repair_refusal: notice.repair_refusal.clone(),
        recorded_at: crate::notes::now_rfc3339(),
    };
    match write_notice_record(&store, stream_id, change, &record) {
        Ok(()) => info(app, stream_id, change, &cause, "merge notice record written", &notice),
        Err(code) => warn(app, stream_id, change, &cause, "merge notice record not written", &code),
    }
    notice
}

/// OFP-FR-EJPE: a new whole-change discussion whose first comment is `body`,
/// appended as the Synthesis participant (CMS-FR-26).
fn open_notice_thread<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &RootFs,
    stream_id: &str,
    change: &str,
    body: &str,
) -> Result<String, String> {
    let target = comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: change.to_string(),
    };
    let discussion = comments::open_discussion_unchecked(
        store,
        &target,
        None,
        body.to_string(),
        Vec::new(),
        &Participant::synthesis(),
        &crate::notes::now_rfc3339(),
    )
    .map_err(|_| crate::graduation::ERR_WRITE_FAILED.to_string())?;
    comments::emit_discussion_changed(app, &discussion);
    Ok(discussion.id)
}

/// OFP-FR-HMED: an `INFO` record with the ids. No record holds a diagnostic
/// or comment text.
fn info<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    cause: &str,
    message: &str,
    notice: &MergeNotice,
) {
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        message,
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "change" => change.to_string(),
            "cause" => cause.to_string(),
            "thread_id" => notice.thread_id.clone().unwrap_or_default(),
            "run_id" => notice.repair_run_id.clone().unwrap_or_default(),
        },
    );
}

/// OFP-FR-HMED: a `WARN` record with the failure code.
fn warn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    cause: &str,
    message: &str,
    code: &str,
) {
    logging::log_warn(
        app,
        &BUFFER,
        &[Domain::Backend],
        message,
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "change" => change.to_string(),
            "cause" => cause.to_string(),
            "code" => code.to_string(),
        },
    );
}
