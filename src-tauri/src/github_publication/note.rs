//! Note publication — a note moved into a GitHub issue
//! (`../../specifications/core/GHP-github-publication.md` GHP-FR-CNPX through
//! GHP-FR-MAMT).
//!
//! A note publication creates one issue in the publication repository, links
//! it to a parent where the author chose one, and then deletes the note through
//! the deletion transaction of `NTC-notes-storage.md` (NTC-FR-21). It reuses the
//! token, the remote resolution, and the REST client of draft publication. It
//! writes no attempt record, no marker, and no history (GHP-FR-GCLP): a second
//! call is a new publication.
//!
//! No log line here carries the title, the body, or any text of the note
//! (GHP-FR-MAMT).

use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::client::{GithubIssues, IssueFields};
use super::records::*;
use super::{flow, issues_client, off_thread, project_token, resolve_publication_repository_with};
use crate::github_tokens::ProjectToken;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::project::ProjectState;

/// GHP-FR-HKMC: the most characters a title holds.
pub const MAX_TITLE_CHARS: usize = 256;
/// GHP-FR-HKMC: the most characters a body holds.
pub const MAX_BODY_CHARS: usize = 65_536;

/// The error code of a note deletion that failed for a reason other than its
/// discussion cleanup. The store phrases such a failure with a filesystem
/// path, so the path stays out of the answer.
pub const ERR_NOTE_DELETE_FAILED: &str = "note_delete_failed";

const MSG_NOTE_PUBLICATION: &str = "note publication";

/// GHP-FR-CNPX: whether a note can be published now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotePublicationAvailability {
    pub available: bool,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    pub remote_name: Option<String>,
    pub repository_host: Option<String>,
    pub repository_owner: Option<String>,
    pub repository_name: Option<String>,
}

impl NotePublicationAvailability {
    fn available(repository: &PublicationRepository) -> Self {
        Self {
            available: true,
            reason_code: None,
            reason: None,
            remote_name: Some(repository.remote_name.clone()),
            repository_host: Some(repository.repository_host.clone()),
            repository_owner: Some(repository.repository_owner.clone()),
            repository_name: Some(repository.repository_name.clone()),
        }
    }

    fn refused(code: &str) -> Self {
        Self {
            available: false,
            reason_code: Some(code.to_string()),
            reason: Some(super::metadata_refresh::refresh_error_text(code)),
            remote_name: None,
            repository_host: None,
            repository_owner: None,
            repository_name: None,
        }
    }
}

/// GHP-FR-JGNI: the choice a caller sends with a note publication.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotePublicationChoiceInput {
    pub parent_issue_number: Option<u64>,
    pub issue_type: Option<String>,
    pub milestone_number: Option<u64>,
    pub labels: Vec<String>,
}

/// GHP-FR-QWFD / GHP-FR-WJBM / GHP-FR-GIVY: what a note publication answers
/// once the issue exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotePublicationOutcome {
    #[serde(rename_all = "camelCase")]
    Published { issue_number: u64, issue_url: String },
    #[serde(rename_all = "camelCase")]
    SubIssueLinkFailed { issue_number: u64, issue_url: String },
    #[serde(rename_all = "camelCase")]
    NoteNotRemoved { issue_number: u64, issue_url: String, error_code: String },
}

/// GHP-FR-HKMC: the trimmed title, or `invalid_note_publication`.
pub fn validated_content(title: &str, body: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty()
        || title.chars().count() > MAX_TITLE_CHARS
        || body.chars().count() > MAX_BODY_CHARS
    {
        return Err(ERR_INVALID_NOTE_PUBLICATION.to_string());
    }
    Ok(title.to_string())
}

/// GHP-FR-VLDP / GHP-FR-WXJD: the fields of the create request, from the
/// choice. Every read happens here, before anything is created, and a refusal
/// leaves nothing behind.
pub fn resolve_fields(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    settings: &GithubPublicationSettings,
    choice: &NotePublicationChoiceInput,
) -> Result<IssueFields, String> {
    if let Some(parent_number) = choice.parent_issue_number {
        // GHP-FR-WXJD: the terms of GHP-FR-RMVQ for the parent.
        let usable = client
            .get_issue(secret, owner, repo, parent_number)?
            .filter(|parent| parent.open && !parent.is_pull_request)
            .filter(|parent| {
                parent.issue_type.as_deref().is_some_and(|found| {
                    settings.parent_issue_types.iter().any(|t| t.eq_ignore_ascii_case(found))
                })
            });
        if usable.is_none() {
            return Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string());
        }
    }
    let wanted_type = choice.issue_type.as_deref().map(str::trim).filter(|name| !name.is_empty());
    let issue_type = match wanted_type {
        None => None,
        Some(wanted) => {
            let types = client
                .list_issue_types(secret, owner)
                .map_err(|_| ERR_ISSUE_TYPE_UNAVAILABLE.to_string())?;
            let found = types.into_iter().find(|t| t.eq_ignore_ascii_case(wanted));
            Some(found.ok_or_else(|| ERR_ISSUE_TYPE_UNAVAILABLE.to_string())?)
        }
    };
    let milestone = flow::open_milestone(client, secret, owner, repo, choice.milestone_number)?;
    let labels = resolve_labels(client, secret, owner, repo, &choice.labels)?;
    Ok(IssueFields { issue_type, milestone: milestone.map(|m| m.number), labels })
}

/// GHP-FR-VLDP: each named label in the repository's own spelling, each once.
fn resolve_labels(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    names: &[String],
) -> Result<Vec<String>, String> {
    let names: Vec<&str> =
        names.iter().map(|name| name.trim()).filter(|name| !name.is_empty()).collect();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let known =
        client.list_labels(secret, owner, repo).map_err(|_| ERR_LABEL_UNAVAILABLE.to_string())?;
    let mut resolved: Vec<String> = Vec::new();
    for name in names {
        let found = known
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| ERR_LABEL_UNAVAILABLE.to_string())?;
        if !resolved.contains(&found.name) {
            resolved.push(found.name.clone());
        }
    }
    Ok(resolved)
}

/// GHP-FR-WJTX through GHP-FR-GIVY: create the issue, link it to its parent,
/// and then delete the note through `delete_note`. A create that fails returns
/// its typed error, and the note stays. After the create, every answer names
/// the issue.
#[allow(clippy::too_many_arguments)] // Each value is one part of one request; a struct adds nothing.
pub fn publish_with(
    client: &dyn GithubIssues,
    secret: &str,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    fields: &IssueFields,
    parent_issue_number: Option<u64>,
    delete_note: impl FnOnce() -> Result<(), String>,
) -> Result<NotePublicationOutcome, String> {
    let issue = client.create_issue(secret, owner, repo, title, body, fields)?;
    if let Some(parent) = parent_issue_number {
        if client.link_sub_issue(secret, owner, repo, parent, issue.id, false).is_err() {
            return Ok(NotePublicationOutcome::SubIssueLinkFailed {
                issue_number: issue.number,
                issue_url: issue.url,
            });
        }
    }
    match delete_note() {
        Ok(()) => Ok(NotePublicationOutcome::Published {
            issue_number: issue.number,
            issue_url: issue.url,
        }),
        Err(code) => Ok(NotePublicationOutcome::NoteNotRemoved {
            issue_number: issue.number,
            issue_url: issue.url,
            error_code: deletion_code(&code),
        }),
    }
}

/// GHP-FR-GIVY: the typed code of a failed deletion.
fn deletion_code(code: &str) -> String {
    match code {
        crate::notes::ERR_DISCUSSION_CLEANUP_FAILED => code.to_string(),
        _ => ERR_NOTE_DELETE_FAILED.to_string(),
    }
}

/// GHP-FR-MAMB: whether `url` is an HTTPS issue address of `owner/repo` on
/// `host`: `https://<host>/<owner>/<repo>/issues/<number>`, with no
/// credential, no query, and no fragment.
pub fn is_repository_issue_url(url: &str, host: &str, owner: &str, repo: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    if rest.contains(['?', '#', '@', '\\']) {
        return false;
    }
    let parts: Vec<&str> = rest.split('/').collect();
    let [authority, found_owner, found_repo, "issues", number] = parts.as_slice() else {
        return false;
    };
    authority.eq_ignore_ascii_case(host)
        && found_owner.eq_ignore_ascii_case(owner)
        && found_repo.eq_ignore_ascii_case(repo)
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// GHP-FR-HGVL: the run condition, with the repository and the token it
/// resolves, or the typed refusal.
fn run_condition<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(crate::fs::RootFs, PublicationRepository, ProjectToken), String> {
    let project = app.state::<ProjectState>();
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT.to_string())?;
    let polling = crate::project_settings::load_github_polling_settings_from(&root)
        .map_err(|_| ERR_INVALID_PUBLICATION_SETTINGS.to_string())?;
    if polling.project_node_id.is_none() {
        return Err(ERR_GITHUB_PROJECT_NOT_SELECTED.to_string());
    }
    let token = project_token(app, &project.slot_key());
    let repository = resolve_publication_repository_with(app, token.as_ref())?;
    let token = token.ok_or_else(|| ERR_TOKEN_UNAVAILABLE.to_string())?;
    Ok((root, repository, token))
}

fn warn<R: tauri::Runtime>(app: &tauri::AppHandle<R>, note_id: &str, code: &str) -> String {
    logging::log_warn(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        MSG_NOTE_PUBLICATION,
        log_fields! { "note_id" => note_id, "refusal" => code },
    );
    code.to_string()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GHP-FR-CNPX: whether **Publish to GitHub** is available on a note row.
#[tauri::command]
pub async fn get_note_publication_availability(
    app: tauri::AppHandle,
) -> Result<NotePublicationAvailability, String> {
    off_thread(move || get_note_publication_availability_impl(&app)).await
}

pub(super) fn get_note_publication_availability_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<NotePublicationAvailability, String> {
    let answer = match run_condition(app) {
        Ok((_, repository, _)) => NotePublicationAvailability::available(&repository),
        Err(code) if code == ERR_NO_PROJECT => return Err(code),
        Err(code) => NotePublicationAvailability::refused(&code),
    };
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "note publication availability read",
        log_fields! {
            "available" => answer.available,
            "refusal" => answer.reason_code.clone().unwrap_or_default(),
        },
    );
    Ok(answer)
}

/// GHP-FR-JGNI: publish a note as an issue, then delete the note.
#[tauri::command]
pub async fn publish_note_to_github(
    note_id: String,
    title: String,
    body: String,
    choice: NotePublicationChoiceInput,
    app: tauri::AppHandle,
) -> Result<NotePublicationOutcome, String> {
    off_thread(move || publish_note_to_github_impl(&app, note_id, title, body, choice)).await
}

pub(super) fn publish_note_to_github_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    note_id: String,
    title: String,
    body: String,
    choice: NotePublicationChoiceInput,
) -> Result<NotePublicationOutcome, String> {
    let (root, repository, token) =
        run_condition(app).map_err(|code| warn(app, &note_id, &code))?;
    crate::notes::note_record(&root, &note_id)
        .map_err(|_| warn(app, &note_id, ERR_NOTE_NOT_FOUND))?;
    let title = validated_content(&title, &body).map_err(|code| warn(app, &note_id, &code))?;
    let settings = crate::project_settings::load_github_publication_settings_from(&root)
        .map_err(|_| warn(app, &note_id, ERR_INVALID_PUBLICATION_SETTINGS))?;
    let client = issues_client(app, &token.host);
    let owner = repository.repository_owner.as_str();
    let repo = repository.repository_name.as_str();
    let fields = resolve_fields(client.as_ref(), &token.secret, owner, repo, &settings, &choice)
        .map_err(|code| warn(app, &note_id, &code))?;
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "note publication started",
        log_fields! {
            "note_id" => &note_id,
            "repository" => format!("{owner}/{repo}"),
            "parent" => choice.parent_issue_number.map(|n| n.to_string()).unwrap_or_default(),
            "typed" => fields.issue_type.is_some(),
            "milestoned" => fields.milestone.is_some(),
            "labels" => fields.labels.len(),
        },
    );
    let outcome = publish_with(
        client.as_ref(),
        &token.secret,
        owner,
        repo,
        &title,
        &body,
        &fields,
        choice.parent_issue_number,
        || crate::notes::delete_note_transaction(app, &note_id),
    );
    // The repository has just been written to, or has just refused a write.
    super::remotes::forget_probes();
    let outcome = outcome.map_err(|code| warn(app, &note_id, &code))?;
    let (message, issue_number, error_code) = match &outcome {
        NotePublicationOutcome::Published { issue_number, .. } => {
            ("note published to GitHub", *issue_number, "")
        }
        NotePublicationOutcome::SubIssueLinkFailed { issue_number, .. } => {
            ("note issue not linked to its parent", *issue_number, ERR_SUB_ISSUE_LINK_FAILED)
        }
        NotePublicationOutcome::NoteNotRemoved { issue_number, error_code, .. } => {
            ("published note not removed", *issue_number, error_code.as_str())
        }
    };
    let fields = log_fields! {
        "note_id" => &note_id,
        "repository" => format!("{owner}/{repo}"),
        "issue" => issue_number,
        "error_code" => error_code,
    };
    if error_code.is_empty() {
        logging::log_info(
            app,
            &logging::BUFFER,
            &[Domain::Backend, Domain::Remote],
            message,
            fields,
        );
    } else {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend, Domain::Remote],
            message,
            fields,
        );
    }
    Ok(outcome)
}

/// GHP-FR-MAMB: open an issue of the publication repository in the system
/// browser. No draft is needed.
#[tauri::command]
pub async fn open_publication_repository_issue(
    url: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let checked = url.clone();
    let check_app = app.clone();
    off_thread(move || require_repository_issue_url(&check_app, &checked)).await?;
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(&url, None::<&str>).map_err(|_| {
        // The URL passed the check above, so it carries no credential.
        logging::log_warn(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "publication repository issue could not be opened",
            log_fields! { "issue_url" => &url },
        );
        ERR_GITHUB_UNREACHABLE.to_string()
    })
}

/// GHP-FR-MAMB: the check of [`open_publication_repository_issue`].
pub(super) fn require_repository_issue_url<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
) -> Result<(), String> {
    let project = app.state::<ProjectState>();
    project.require_root().map_err(|_| ERR_NO_PROJECT.to_string())?;
    let token = project_token(app, &project.slot_key());
    let repository = resolve_publication_repository_with(app, token.as_ref())?;
    if is_repository_issue_url(
        url,
        &repository.repository_host,
        &repository.repository_owner,
        &repository.repository_name,
    ) {
        return Ok(());
    }
    // The refused URL is not logged: it did not pass the check, so it may
    // carry a credential (GHP-FR-DHXK).
    logging::log_warn(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        "publication repository issue refused",
        log_fields! { "refusal" => ERR_ISSUE_URL_REFUSED },
    );
    Err(ERR_ISSUE_URL_REFUSED.to_string())
}
