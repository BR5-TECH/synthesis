//! The update of a published draft's issue (`GHP-github-publication.md`
//! GHP-FR-MZMW through GHP-FR-BCAV).
//!
//! An update edits the issue that the draft's **current record** names, and no
//! other issue. It sends the draft's current name and prompt as the title and
//! the body, and nothing else, so the Type, the milestone, the parent, and the
//! labels stay as GitHub holds them. It writes no attempt record: an update
//! creates nothing, so there is no duplicate for a retry to find.
//!
//! The update reads the issue before it writes. A closed issue is not edited
//! until the caller asks again with `update_closed` (GHP-FR-AXRU).

use tauri::Manager;

use super::client::{GithubIssues, IssueEdit, IssueFields, IssueRef};
use super::records::*;
use super::{flow, store};
use crate::fs;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::project::ProjectState;

/// GHP-FR-MZMW: edit the issue of the draft's current record.
#[tauri::command]
pub async fn update_draft_on_github(
    draft_id: String,
    update_closed: bool,
    app: tauri::AppHandle,
) -> Result<IssueUpdateOutcome, String> {
    super::off_thread(move || update_draft_on_github_impl(&app, draft_id, update_closed)).await
}

pub(super) fn update_draft_on_github_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
    update_closed: bool,
) -> Result<IssueUpdateOutcome, String> {
    let project = app.state::<ProjectState>();
    let root = super::require_root(app, &project)?;
    let project_key = project.slot_key();
    // GHP-FR-LVVD: the refusals of a publication, before the first request.
    super::require_publishable_draft(app, &root, &draft_id)?;
    let machine = super::require_machine_store(app, &project, &draft_id)?;
    let publication = store::read_store(&root, &machine, &draft_id)
        .map_err(|code| super::fail(app, &draft_id, code))?;
    if publication.attempt.is_some() {
        return Err(super::fail(app, &draft_id, ERR_ATTEMPT_IN_PROGRESS.to_string()));
    }
    super::require_no_local_assets(app, &root, &draft_id)?;
    let Some(current) = store::current_of(&publication) else {
        return Err(super::fail(app, &draft_id, ERR_NO_PUBLICATION_RECORD.to_string()));
    };
    // GHP-FR-ZRPL: the record names the repository, and the project token must
    // belong to the host of that repository.
    let Some(token) = super::project_token(app, &project_key) else {
        return Err(super::fail(app, &draft_id, ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    if current.repository_host != token.host {
        return Err(super::fail(app, &draft_id, ERR_HOST_MISMATCH.to_string()));
    }
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "draft issue update started",
        log_fields! {
            "draft_id" => &draft_id,
            "repository" => format!("{}/{}", current.repository_owner, current.repository_name),
            "issue" => current.issue_number,
            "update_closed" => update_closed,
        },
    );
    let client = super::issues_client(app, &token.host);
    let outcome = update_current_issue(
        &root,
        &machine,
        &draft_id,
        &current,
        update_closed,
        &token.secret,
        client.as_ref(),
    );
    match outcome {
        Ok(IssueUpdateOutcome::Updated { record }) => {
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "draft issue updated on GitHub",
                log_fields! { "draft_id" => &draft_id, "issue" => record.issue_number },
            );
            super::announce(app, &draft_id);
            Ok(IssueUpdateOutcome::Updated { record })
        }
        Ok(closed) => {
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "draft issue update waits on a closed issue",
                log_fields! { "draft_id" => &draft_id, "issue" => current.issue_number },
            );
            Ok(closed)
        }
        Err(code) => {
            // GHP-FR-BCAV: past the edit, a failed store write leaves GitHub and
            // the disk apart, which is an error rather than a refusal.
            let level =
                if code == ERR_STORE_WRITE_FAILED { logging::log_error } else { logging::log_warn };
            level(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "draft issue update failed",
                log_fields! {
                    "draft_id" => &draft_id,
                    "issue" => current.issue_number,
                    "refusal" => &code,
                },
            );
            Err(code)
        }
    }
}

/// GHP-FR-SVXP through GHP-FR-WUJQ: read the issue, edit its title and body,
/// and append the record. Pure of Tauri, so it is tested against the fake
/// client.
pub fn update_current_issue(
    root: &fs::RootFs,
    machine: &fs::RootFs,
    draft_id: &str,
    current: &PublicationRecord,
    update_closed: bool,
    secret: &str,
    client: &dyn GithubIssues,
) -> Result<IssueUpdateOutcome, String> {
    let owner = current.repository_owner.as_str();
    let repo = current.repository_name.as_str();
    // GHP-FR-SVXP: the issue is read before anything is written.
    let issue = match client.get_issue(secret, owner, repo, current.issue_number) {
        Ok(Some(issue)) => issue,
        Ok(None) => return Err(ERR_ISSUE_NOT_FOUND.to_string()),
        // The read reports a refused token in the vocabulary of creation; for
        // an update the same refusal is about this issue.
        Err(code) if code == ERR_ISSUES_CREATE_FORBIDDEN => {
            return Err(ERR_ISSUE_UPDATE_FORBIDDEN.to_string())
        }
        Err(code) => return Err(code),
    };
    if !names_record_issue(&issue, current) {
        return Err(ERR_ISSUE_MOVED.to_string());
    }
    // GHP-FR-AXRU: a closed issue is edited only when the caller asks again.
    if !issue.open && !update_closed {
        return Ok(IssueUpdateOutcome::IssueClosed {
            issue_number: current.issue_number,
            issue_url: current.issue_url.clone(),
        });
    }
    // GHP-FR-TCFV: the title and the body alone. Empty fields leave the Type
    // and the milestone out of the request, so GitHub keeps them.
    let content = flow::issue_content(root, draft_id, &current.marker)?;
    let fields = IssueFields::default();
    let edit = IssueEdit { title: &content.title, body: &content.body, fields: &fields };
    client.update_issue(secret, owner, repo, current.issue_number, &edit)?;
    // GHP-FR-WUJQ: the record of the update copies the current record and
    // adds the instant of the update.
    let record =
        PublicationRecord { updated_at: Some(crate::notes::now_rfc3339()), ..current.clone() };
    let record = store::append_update(root, machine, draft_id, current, record)?;
    Ok(IssueUpdateOutcome::Updated { record: Box::new(record) })
}

/// GHP-FR-SVXP: whether the issue GitHub returned is the issue the record
/// names. GitHub follows a moved issue to its new address, so the address it
/// returns is what tells a moved issue apart.
pub fn names_record_issue(issue: &IssueRef, record: &PublicationRecord) -> bool {
    if issue.number != record.issue_number || issue.is_pull_request {
        return false;
    }
    // The address reads `<base>/<owner>/<repo>/issues/<number>`. An address of
    // another shape says nothing about a move, so only the number decides.
    let segments: Vec<&str> = issue.url.trim_end_matches('/').rsplit('/').take(4).collect();
    match segments.as_slice() {
        [number, "issues", repo, owner] => {
            owner.eq_ignore_ascii_case(&record.repository_owner)
                && repo.eq_ignore_ascii_case(&record.repository_name)
                && *number == record.issue_number.to_string()
        }
        _ => true,
    }
}
