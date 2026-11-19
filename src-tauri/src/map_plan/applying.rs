//! MPR-FR-YCHW to MPR-FR-XPXP, MPR-FR-VMXC: the write of one accepted row.

use crate::delivery::github::delivery_target;
use crate::delivery::github::edits::{
    apply_row, CloseReason, IssueChangeRow, NewIssueRow, RowWrite,
};
use crate::fs::RootFs;
use crate::github_polling::records::is_configuration_error;
use crate::project::ProjectState;

use super::errors::{error, ERR_DRAFT_CREATE_FAILED, ERR_ROW_INVALID, ERR_ROW_NOT_FOUND};
use super::model::{
    CreateIssueFields, MapPlanError, MapPlanProposal, ModifyIssueFields, NewDraftFields, ParentRef,
    RowError, RowFields, RowResult,
};

/// What the write of a row came to.
pub(super) enum Applied {
    /// The row is `accepted` with this result.
    Done(RowResult),
    /// The row is `failed` with this error. It is final (MPR-FR-QUYY).
    Failed(RowError),
    /// The decision is refused, and the row stays `waiting` (MPR-FR-VMXC).
    Refused(MapPlanError),
}

fn failed(code: &str, message: &str) -> Applied {
    Applied::Failed(RowError { code: code.to_string(), message: message.to_string() })
}

/// Apply the row at `at`. A `new_draft` row makes no request to GitHub.
pub(super) fn apply_row_kind<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    store: &RootFs,
    proposal: &MapPlanProposal,
    at: usize,
) -> Applied {
    match &proposal.rows[at].fields {
        RowFields::NewDraft(fields) => create_draft(app, project, store, fields),
        RowFields::CreateIssue(fields) => match create_write(proposal, fields) {
            Ok(write) => write_issue_row(app, write),
            Err(applied) => applied,
        },
        RowFields::ModifyIssue(fields) => match modify_write(proposal, fields) {
            Ok(write) => write_issue_row(app, write),
            Err(applied) => applied,
        },
    }
}

/// MPR-FR-VMXC: an issue row needs a GitHub Project source with a valid
/// configuration, and a refusal leaves the row `waiting`. The write that follows
/// is final: its failure fails the row.
fn write_issue_row<R: tauri::Runtime>(app: &tauri::AppHandle<R>, write: RowWrite) -> Applied {
    let target = match delivery_target(app) {
        Ok(target) => target,
        Err(e) => return Applied::Refused(error(&e.code, &e.message)),
    };
    match apply_row(app, &target, &write) {
        Ok(done) => Applied::Done(RowResult {
            issue_number: Some(done.number),
            issue_url: done.url,
            draft_id: None,
            applied_changes: matches!(write, RowWrite::Modify(_)).then_some(done.fields),
        }),
        // MPR-FR-VMXC: a configuration that is not valid is a refusal.
        Err(e) if is_configuration_error(&e.code) => Applied::Refused(error(&e.code, &e.message)),
        Err(e) => Applied::Failed(RowError { code: e.code, message: e.message }),
    }
}

/// MPR-FR-EQAX: the real number of the issue that the parent row created.
fn parent_row_number(proposal: &MapPlanProposal, key: &str) -> Result<u64, Applied> {
    proposal
        .rows
        .iter()
        .find(|r| r.key == key)
        .and_then(|r| r.result.as_ref())
        .and_then(|r| r.issue_number)
        .ok_or_else(|| Applied::Refused(error(ERR_ROW_NOT_FOUND, "The parent row made no issue.")))
}

/// MPR-FR-YCHW, MPR-FR-EQAX: the write of a new issue. A parent row gives the
/// real number of the issue that it created.
fn create_write(
    proposal: &MapPlanProposal,
    fields: &CreateIssueFields,
) -> Result<RowWrite, Applied> {
    let parent = match &fields.parent {
        None => None,
        Some(ParentRef::Issue(number)) => Some(*number),
        Some(ParentRef::Row(key)) => Some(parent_row_number(proposal, key)?),
    };
    Ok(RowWrite::Create(NewIssueRow {
        title: fields.title.clone(),
        body: fields.body.clone(),
        issue_type: fields.issue_type.clone(),
        milestone: fields.milestone.as_ref().map(|m| m.number),
        parent,
        labels: fields.labels.clone(),
    }))
}

/// A number from a stored value: `7`, `{ "number": 7 }`, or `{ "issue": 7 }`.
/// `null` is `Some(None)`: the value is removed.
fn number_or_null(value: &serde_json::Value) -> Option<Option<u64>> {
    if value.is_null() {
        return Some(None);
    }
    let number = value
        .as_u64()
        .or_else(|| value.get("number").and_then(serde_json::Value::as_u64))
        .or_else(|| value.get("issue").and_then(serde_json::Value::as_u64))?;
    Some(Some(number))
}

/// MPR-FR-FBSH, MPR-FR-RTGH, MPR-FR-WBKZ: the write of the changes of a row.
/// A parent change that names a row of the proposal writes the real number of
/// the issue of that row (MPR-FR-EQAX).
fn modify_write(
    proposal: &MapPlanProposal,
    fields: &ModifyIssueFields,
) -> Result<RowWrite, Applied> {
    let invalid = || failed(ERR_ROW_INVALID, "The row holds a change that cannot be applied.");
    let mut row = IssueChangeRow {
        number: fields.number,
        base_updated_at: fields.base_updated_at.clone(),
        ..IssueChangeRow::default()
    };
    for change in &fields.changes {
        let text = || change.after.as_str().map(str::to_string);
        match change.field.as_str() {
            "title" => row.title = Some(text().ok_or_else(invalid)?),
            "body" => row.body = Some(text().ok_or_else(invalid)?),
            "type" => row.issue_type = Some(text().ok_or_else(invalid)?),
            "milestone" => row.milestone = Some(number_or_null(&change.after).ok_or_else(invalid)?),
            "parent" => {
                row.parent = Some(match change.after.get("row").and_then(|k| k.as_str()) {
                    Some(key) => Some(parent_row_number(proposal, key)?),
                    None => number_or_null(&change.after).ok_or_else(invalid)?,
                })
            }
            "labels" => {
                row.labels_add = change.added.clone().unwrap_or_default();
                row.labels_remove = change.removed.clone().unwrap_or_default();
            }
            "status" => row.status = Some(text().ok_or_else(invalid)?),
            "close" => {
                row.close = Some(match change.reason.as_deref() {
                    Some("completed") => CloseReason::Completed,
                    Some("not_planned") => CloseReason::NotPlanned,
                    _ => return Err(invalid()),
                })
            }
            _ => return Err(invalid()),
        }
    }
    Ok(RowWrite::Modify(row))
}

/// MPR-FR-XPXP: one draft with the name and the prompt of the row, through the
/// draft storage. It makes no request to GitHub. A draft that cannot take its
/// prompt is taken back.
fn create_draft<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    store: &RootFs,
    fields: &NewDraftFields,
) -> Applied {
    let fail = || failed(ERR_DRAFT_CREATE_FAILED, "The draft could not be created.");
    let Ok(root) = project.require_root() else { return fail() };
    let Ok(created) = crate::drafts::create_draft_impl(&root, Some(&fields.name), None) else {
        return fail();
    };
    let id = created.draft.id.clone();
    if crate::drafts::save_draft_file_impl(&root, &id, &created.file, &fields.body).is_err() {
        let _ = crate::drafts::delete_draft_impl(&root, store, &id);
        return fail();
    }
    crate::drafts::git_storage::ensure_private_ignored(&root);
    crate::drafts::announce(app, crate::drafts::DraftChange::to(&id, vec![created.file]));
    Applied::Done(RowResult { draft_id: Some(id), ..RowResult::default() })
}
