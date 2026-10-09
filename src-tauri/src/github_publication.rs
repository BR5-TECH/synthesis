//! GitHub publication — a draft's prompt as a GitHub issue
//! (`../../specifications/core/GHP-github-publication.md`).
//!
//! A draft is developed locally and then either **graduated** into a
//! specification by an agent that runs here, or **published** as a GitHub issue
//! for an agent that runs somewhere else. This module owns the second route:
//! which configured remote the issue goes to, whether the project's token may
//! create an issue there, the attempt record written before the first request,
//! the issue itself, and the append-only history of every issue a draft has
//! produced.
//!
//! Publication is a fact about a draft's **metadata**, never about its prompt.
//! Nothing here writes a byte of the prompt, and the marker that makes a retry
//! safe rides in the issue body rather than in the draft (GHP-FR-FQIZ).
//!
//! Three invariants carry the whole module:
//!
//! - **Nothing mutates while enumerating.** Listing remotes and checking a
//!   token's access are reads (GHP-FR-WKDE), so opening the publish flow and
//!   changing your mind has changed nothing anywhere.
//! - **The attempt record is written before the first request** (GHP-FR-RUYT),
//!   and a retry searches for its marker before it creates anything
//!   (GHP-FR-OWLB). A create whose response was lost therefore cannot become a
//!   duplicate issue.
//! - **History is append-only** (GHP-FR-UZMX). Re-publishing adds a record; it
//!   never rewrites one, so an issue an old record names stays a valid
//!   reference.
//!
//! Out of scope: the agent that reads the issue, media upload (v1 refuses a
//! draft holding local asset references rather than uploading them —
//! GHP-FR-AKUM), and hosts other than `github.com`.

use tauri::{Emitter, Manager, State};

use crate::github_tokens::{self, GithubTokens};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::project::ProjectState;

pub mod client;
pub mod flow;
pub mod records;
pub mod remotes;
pub mod store;

pub use client::{GithubIssues, GithubIssuesSeam, IssueRef, ProbeOutcome};
pub use records::*;
pub use remotes::ConfiguredRemote;

/// GHP-FR-CWTG: the event both surfaces follow instead of polling.
///
/// The wire name is kebab-case for the reason every other event in this
/// codebase is: Tauri rejects a space in an event name at `emit`, which would
/// leave a channel silently dead in production.
pub const DRAFT_PUBLICATION_CHANGED: &str = "draft-publication-changed";

const MSG_PUBLISH: &str = "draft publication";

/// A test's stand-in for the keychain-held token.
#[cfg(test)]
pub(crate) struct TestSecret(pub String);

fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, draft_id: &str) {
    let _ = app.emit(DRAFT_PUBLICATION_CHANGED, serde_json::json!({ "draftId": draft_id }));
}

/// The project's configured Git remotes, by name and URL (GHP-FR-WKDE).
///
/// One `git2` read of the repository configuration. It contacts nothing and
/// writes nothing, which is what lets the publish flow open without a single
/// mutation anywhere.
pub fn configured_remotes(root: &crate::fs::RootFs) -> Vec<ConfiguredRemote> {
    let Ok(repo) = crate::changes::open_repo(root) else {
        return Vec::new();
    };
    let Ok(names) = repo.remotes() else {
        return Vec::new();
    };
    names
        .iter()
        .flatten()
        .filter_map(|name| {
            let name = name?;
            let remote = repo.find_remote(name).ok()?;
            Some(ConfiguredRemote {
                name: name.to_string(),
                url: remote.url().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// The project's token, or `None` where it resolves none.
///
/// A missing token is not an error here: it is one of the reasons a remote is
/// ineligible (GHP-FR-MZPR), and reporting it as a classification rather than
/// as a failure is what lets the picker explain every row at once.
fn project_secret<R: tauri::Runtime>(app: &tauri::AppHandle<R>, project_key: &str) -> Option<String> {
    #[cfg(test)]
    if let Some(secret) = app.try_state::<TestSecret>() {
        return Some(secret.0.clone());
    }
    let store = app.state::<GlobalSettingsStore>();
    let tokens = app.state::<GithubTokens>();
    github_tokens::resolve_github_token_secret(&store, &tokens, project_key).ok()
}

/// Run one command's body off the main thread.
///
/// Every operation here can make an HTTP request, and a `#[tauri::command]`
/// that is not `async` runs on the thread the window is drawn from — so a slow
/// GitHub would freeze the whole application for the length of the request
/// budget. The managed state is resolved **inside** the closure rather than
/// taken as a parameter, because a `State<'_, T>` borrows the invoke context
/// and cannot cross onto another thread, while the `AppHandle` can and reaches
/// the same values. This is the pattern `GTC-git.md`'s transfers already use.
async fn off_thread<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| ERR_GITHUB_UNREACHABLE.to_string())?
}

fn issues_client<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> std::sync::Arc<dyn GithubIssues> {
    app.state::<GithubIssuesSeam>().0.clone()
}

/// The project's root, or the typed refusal — reported, because a command that
/// answers nothing is a command nobody can debug from the Logs panel.
fn require_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
) -> Result<crate::fs::RootFs, String> {
    project.require_root().map_err(|_| fail(app, "", ERR_NO_PROJECT.to_string()))
}

/// GHP-FR-SRDL / GHP-FR-GJEO: the two preconditions every mutating operation
/// shares, and the draft's current status.
fn require_publishable_draft<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    draft_id: &str,
) -> Result<crate::drafts::DraftStatus, String> {
    // A draft an agent is working from right now takes no publication. A
    // `graduated` draft does — publication changes metadata, not the prompt or
    // the local graduation — which is why this is the run-only check rather
    // than the full draft-storage lock.
    // GHP-FR-BKLT: a GitHub-shadow draft is refused first, whatever its
    // status or its runs, so nothing is written or requested for it.
    flow::require_not_shadow(root, draft_id).map_err(|code| fail(app, draft_id, code))?;
    crate::graduation::require_no_running_graduation(app, draft_id)
        .map_err(|_| fail(app, draft_id, ERR_DRAFT_LOCKED.to_string()))?;
    let mut record = crate::drafts::draft_record(root, draft_id)
        .map_err(|_| fail(app, draft_id, ERR_DRAFT_NOT_FOUND.to_string()))?;
    crate::drafts::resolve_graduated_status(app, &mut record);
    if record.status == crate::drafts::DraftStatus::Archived {
        return Err(fail(app, draft_id, ERR_DRAFT_ARCHIVED.to_string()));
    }
    Ok(record.status)
}

/// GHP-FR-AKUM: the local-asset refusal, applied before the **first GitHub
/// request of an attempt** — which is every attempt, a retry and a recovery
/// answer included. A local image added between a failed attempt and its retry
/// must not reach an issue as a broken reference.
fn require_no_local_assets<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    draft_id: &str,
) -> Result<(), String> {
    let local = flow::local_asset_references(root, draft_id);
    if local.is_empty() {
        return Ok(());
    }
    logging::log_warn(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        MSG_PUBLISH,
        log_fields! {
            "draft_id" => draft_id,
            "refusal" => ERR_LOCAL_ASSETS,
            "local_assets" => local.len(),
        },
    );
    Err(ERR_LOCAL_ASSETS.to_string())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// GHP-FR-CWTG: everything a surface renders about one draft's publication.
#[tauri::command]
pub async fn get_draft_publication(
    draft_id: String,
    app: tauri::AppHandle,
) -> Result<DraftPublicationView, String> {
    off_thread(move || get_draft_publication_impl(&app, draft_id)).await
}

fn get_draft_publication_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
) -> Result<DraftPublicationView, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    let mut record = crate::drafts::draft_record(&root, &draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())?;
    crate::drafts::resolve_graduated_status(app, &mut record);
    let held = crate::graduation::require_no_running_graduation(app, &draft_id).is_err();
    let publication = store::read_store(&root, &draft_id)?;

    let eligibility = if record.github_issue.is_some() {
        // GHP-FR-BKLT: decided from the record alone, before any network read.
        flow::local_eligibility(&root, &draft_id, record.status, None)
    } else if held {
        PublicationEligibility::refused(
            ERR_DRAFT_LOCKED,
            "A graduation run is working from this draft.",
        )
    } else {
        let local = flow::local_eligibility(
            &root,
            &draft_id,
            record.status,
            publication.attempt.as_ref(),
        );
        // The remote checks cost a network read per GitHub remote, so they run
        // only once everything decidable from disk has passed.
        match local.publishable {
            false => local,
            true => {
                let secret = project_secret(app, &project_key);
                let resolution = flow::resolution_for(
                    &root,
                    &configured_remotes(&root),
                    secret.as_deref(),
                    issues_client(app).as_ref(),
                );
                match resolution.selection {
                    Some(_) => PublicationEligibility::publishable(),
                    None => {
                        let code = remotes::refusal_for(&resolution.remotes);
                        PublicationEligibility::refused(&code, remotes::refusal_reason(&code))
                    }
                }
            }
        }
    };

    Ok(DraftPublicationView {
        current: store::current_of(&publication),
        history: store::history_newest_first(&publication),
        attempt: publication.attempt,
        eligibility,
    })
}

/// GHP-FR-WKDE: every configured remote, classified, and which one the next
/// attempt would use.
#[tauri::command]
pub async fn list_publication_remotes(
    draft_id: String,
    app: tauri::AppHandle,
) -> Result<PublicationRemoteResolution, String> {
    off_thread(move || list_publication_remotes_impl(&app, draft_id)).await
}

fn list_publication_remotes_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
) -> Result<PublicationRemoteResolution, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    require_publishable_draft(app, &root, &draft_id)?;
    let secret = project_secret(app, &project_key);
    let configured = configured_remotes(&root);
    let resolution =
        flow::resolution_for(&root, &configured, secret.as_deref(), issues_client(app).as_ref());
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "publication remotes enumerated",
        log_fields! {
            "draft_id" => &draft_id,
            "remotes" => resolution.remotes.len(),
            "selected" => resolution.selection.is_some(),
            // Why each remote was refused, which is the question an author asks
            // when the action is disabled and the one this line could not
            // answer before. Eligibility values are a fixed vocabulary
            // (GHP-FR-MZPR); no remote name, address, or secret joins them, so
            // the line stays safe to export (GHP-FR-DHXK).
            "eligibility" => resolution
                .remotes
                .iter()
                .filter_map(|remote| {
                    // The value the surface itself reads, taken through serde so
                    // the line and the payload cannot disagree.
                    serde_json::to_value(remote.eligibility)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                })
                .collect::<Vec<_>>()
                .join(","),
        },
    );
    Ok(resolution)
}

/// GHP-FR-CVYK: start a **new** attempt against the named remote.
#[tauri::command]
pub async fn publish_draft_to_github(
    draft_id: String,
    remote_name: String,
    persist_remote: bool,
    publication_choice: PublicationChoiceInput,
    app: tauri::AppHandle,
) -> Result<PublicationOutcome, String> {
    off_thread(move || {
        publish_draft_to_github_impl(&app, draft_id, remote_name, persist_remote, publication_choice)
    })
    .await
}

fn publish_draft_to_github_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
    remote_name: String,
    persist_remote: bool,
    publication_choice: PublicationChoiceInput,
) -> Result<PublicationOutcome, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    let status = require_publishable_draft(app, &root, &draft_id)?;

    // GHP-FR-AKUM: the local-asset refusal happens before the first request and
    // before the attempt record, so a refused draft leaves nothing behind.
    let existing = store::read_store(&root, &draft_id)?;
    let local = flow::local_eligibility(&root, &draft_id, status, existing.attempt.as_ref());
    if !local.publishable {
        return Err(fail(app, &draft_id, local.reason_code.unwrap_or_default()));
    }

    let Some(secret) = project_secret(app, &project_key) else {
        return Err(fail(app, &draft_id, ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    let client = issues_client(app);
    let classified =
        remotes::classify(&configured_remotes(&root), Some(&secret), client.as_ref());
    let remote = flow::resolve_remote_for(&classified, &remote_name)
        .map_err(|code| fail(app, &draft_id, code))?;

    // GHP-FR-ATCH: every read that resolves the choice happens here, before the
    // attempt record, so a refused choice leaves nothing behind.
    let settings = crate::project_settings::load_github_publication_settings_from(&root)
        .map_err(|_| fail(app, &draft_id, ERR_INVALID_PUBLICATION_SETTINGS.to_string()))?;
    let choice = flow::resolve_choice(
        &publication_choice,
        &settings,
        client.as_ref(),
        &secret,
        remote.repository_owner.as_deref().unwrap_or_default(),
        remote.repository_name.as_deref().unwrap_or_default(),
    )
    .map_err(|code| fail(app, &draft_id, code))?;

    flow::persist_choice(&root, &remote, persist_remote)?;
    let attempt =
        flow::open_attempt_with(&root, &draft_id, &remote, flow::new_marker(), Some(choice.clone()))?;
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        "draft publication attempt opened",
        log_fields! {
            "draft_id" => &draft_id,
            "marker" => &attempt.marker,
            "remote" => &attempt.remote_name,
            "repository" => format!("{}/{}", attempt.repository_owner, attempt.repository_name),
            "kind" => format!("{:?}", choice.kind),
            "parent" => choice.parent_issue_number.map(|n| n.to_string()).unwrap_or_default(),
            "typed" => choice.issue_type.is_some(),
            "milestoned" => choice.milestone_number.is_some(),
        },
    );
    announce(app, &draft_id);

    let outcome = flow::publish_with_attempt(&root, &draft_id, &attempt, &secret, client.as_ref());
    settle(app, &root, &draft_id, status, outcome)
}

/// GHP-FR-MDLD: everything the publication chooser renders, read-only.
#[tauri::command]
pub async fn load_publication_metadata(
    draft_id: String,
    remote_name: String,
    app: tauri::AppHandle,
) -> Result<PublicationMetadata, String> {
    off_thread(move || load_publication_metadata_impl(&app, draft_id, remote_name)).await
}

fn load_publication_metadata_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
    remote_name: String,
) -> Result<PublicationMetadata, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    require_publishable_draft(app, &root, &draft_id)?;
    let Some(secret) = project_secret(app, &project_key) else {
        return Err(fail(app, &draft_id, ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    let client = issues_client(app);
    let classified =
        remotes::classify(&configured_remotes(&root), Some(&secret), client.as_ref());
    let remote = flow::resolve_remote_for(&classified, &remote_name)
        .map_err(|code| fail(app, &draft_id, code))?;
    let settings = crate::project_settings::load_github_publication_settings_from(&root)
        .map_err(|_| fail(app, &draft_id, ERR_INVALID_PUBLICATION_SETTINGS.to_string()))?;
    let metadata = flow::load_metadata(
        client.as_ref(),
        &secret,
        remote.repository_owner.as_deref().unwrap_or_default(),
        remote.repository_name.as_deref().unwrap_or_default(),
        settings,
    );
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "publication metadata read",
        log_fields! {
            "draft_id" => &draft_id,
            "parents" => metadata.parents.items.len(),
            "parents_failed" => metadata.parents.state == MetadataState::Failed,
            "types" => metadata.issue_types.items.len(),
            "types_failed" => metadata.issue_types.state == MetadataState::Failed,
            "milestones" => metadata.milestones.items.len(),
            "milestones_failed" => metadata.milestones.state == MetadataState::Failed,
        },
    );
    Ok(metadata)
}

/// GHP-FR-KVRH: the stored publication settings, defaults applied.
#[tauri::command]
pub fn get_github_publication_settings(
    app: tauri::AppHandle,
) -> Result<GithubPublicationSettings, String> {
    get_github_publication_settings_impl(&app)
}

fn get_github_publication_settings_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<GithubPublicationSettings, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    crate::project_settings::load_github_publication_settings_from(&root).map_err(|error| {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "github publication settings could not be read",
            log_fields! { "refusal" => ERR_INVALID_PUBLICATION_SETTINGS },
        );
        error
    })
}

/// GHP-FR-NQWX: persist the three settings, or refuse and write nothing.
#[tauri::command]
pub fn set_github_publication_settings(
    parent_issue_types: Vec<String>,
    sub_issue_type: String,
    sub_issue_milestone_policy: MilestonePolicy,
    app: tauri::AppHandle,
) -> Result<GithubPublicationSettings, String> {
    set_github_publication_settings_impl(
        &app,
        parent_issue_types,
        sub_issue_type,
        sub_issue_milestone_policy,
    )
}

fn set_github_publication_settings_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    parent_issue_types: Vec<String>,
    sub_issue_type: String,
    sub_issue_milestone_policy: MilestonePolicy,
) -> Result<GithubPublicationSettings, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let settings =
        flow::validated_settings(parent_issue_types, sub_issue_type, sub_issue_milestone_policy)
            .map_err(|code| fail(app, "", code))?;
    // The store phrases its failures with a filesystem path, so the surface
    // gets the module's own code (GHP-FR-PVOA).
    crate::project_settings::save_github_publication_settings_to(&root, &settings).map_err(
        |_| {
            logging::log_warn(
                app,
                &logging::BUFFER,
                &[Domain::Backend],
                "github publication settings could not be saved",
                log_fields! { "refusal" => ERR_STORE_WRITE_FAILED },
            );
            ERR_STORE_WRITE_FAILED.to_string()
        },
    )?;
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        "github publication settings saved",
        log_fields! {
            "parent_types" => settings.parent_issue_types.len(),
            "policy" => format!("{:?}", settings.sub_issue_milestone_policy),
        },
    );
    Ok(settings)
}

/// GHP-FR-PTYL: the issue Types of the publication repository's owner.
#[tauri::command]
pub async fn list_github_issue_types(
    app: tauri::AppHandle,
) -> Result<MetadataList<PublicationIssueType>, String> {
    off_thread(move || list_github_issue_types_impl(&app)).await
}

fn list_github_issue_types_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<MetadataList<PublicationIssueType>, String> {
    let project = app.state::<ProjectState>();
    let secret = project_secret(app, &project.slot_key());
    let repository =
        resolve_publication_repository_with(app, secret.as_deref()).map_err(|code| fail(app, "", code))?;
    let Some(secret) = secret else {
        return Err(fail(app, "", ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    let list = flow::issue_types_list(
        issues_client(app).as_ref(),
        &secret,
        &repository.repository_owner,
    );
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        "github issue types read",
        log_fields! { "types" => list.items.len(), "failed" => list.state == MetadataState::Failed },
    );
    Ok(list)
}

/// GHP-FR-OWLB: continue the standing attempt, reusing its marker.
#[tauri::command]
pub async fn retry_draft_publication(
    draft_id: String,
    app: tauri::AppHandle,
) -> Result<PublicationOutcome, String> {
    off_thread(move || retry_draft_publication_impl(&app, draft_id)).await
}

fn retry_draft_publication_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
) -> Result<PublicationOutcome, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    let status = require_publishable_draft(app, &root, &draft_id)?;
    require_no_local_assets(app, &root, &draft_id)?;
    let Some(attempt) = store::read_store(&root, &draft_id)?.attempt else {
        return Err(fail(app, &draft_id, ERR_NO_ATTEMPT.to_string()));
    };
    let Some(secret) = project_secret(app, &project_key) else {
        return Err(fail(app, &draft_id, ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    let client = issues_client(app);
    let outcome = flow::publish_with_attempt(&root, &draft_id, &attempt, &secret, client.as_ref());
    settle(app, &root, &draft_id, status, outcome)
}

/// GHP-FR-YPGL: the author's answer to a recovery choice.
#[tauri::command]
pub async fn resolve_draft_publication_conflict(
    draft_id: String,
    choice: RecoveryChoice,
    app: tauri::AppHandle,
) -> Result<PublicationOutcome, String> {
    off_thread(move || resolve_draft_publication_conflict_impl(&app, draft_id, choice)).await
}

fn resolve_draft_publication_conflict_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
    choice: RecoveryChoice,
) -> Result<PublicationOutcome, String> {
    let project = app.state::<ProjectState>();
    let root = require_root(app, &project)?;
    let project_key = project.slot_key();
    let status = require_publishable_draft(app, &root, &draft_id)?;
    require_no_local_assets(app, &root, &draft_id)?;
    let Some(attempt) = store::read_store(&root, &draft_id)?.attempt else {
        return Err(fail(app, &draft_id, ERR_NO_ATTEMPT.to_string()));
    };
    // A recovery choice answers a question that was actually asked. Against an
    // `open` attempt there is no found issue to update and no deliberate
    // re-publication to start, so the answer is refused rather than acted on.
    if attempt.state != AttemptState::AwaitingChoice {
        return Err(fail(app, &draft_id, ERR_NO_ATTEMPT.to_string()));
    }
    let Some(secret) = project_secret(app, &project_key) else {
        return Err(fail(app, &draft_id, ERR_TOKEN_UNAVAILABLE.to_string()));
    };
    let client = issues_client(app);

    let outcome = match choice {
        RecoveryChoice::UpdateExisting => {
            flow::update_existing(&root, &draft_id, &attempt, &secret, client.as_ref())
        }
        // GHP-FR-QLDF: a deliberate re-publication is a **new** attempt with a
        // new marker, so the issue it creates is a second issue rather than an
        // edit of the first. The abandoned attempt appends no history record.
        RecoveryChoice::PublishNew => {
            let remote = PublicationRemote {
                name: attempt.remote_name.clone(),
                url: attempt.remote_url.clone(),
                kind: RemoteKind::Github,
                repository_owner: Some(attempt.repository_owner.clone()),
                repository_name: Some(attempt.repository_name.clone()),
                eligibility: RemoteEligibility::Eligible,
                reason: None,
                tls_failure: None,
            };
            // GHP-FR-YPGL: the saved choice travels to the new attempt, which
            // replaces the old one in a single write.
            let fresh = flow::restart_attempt(
                &root,
                &draft_id,
                &remote,
                flow::new_marker(),
                attempt.choice.clone(),
            )?;
            flow::publish_with_attempt(&root, &draft_id, &fresh, &secret, client.as_ref())
        }
    };
    settle(app, &root, &draft_id, status, outcome)
}

/// GHP-FR-EBSA: cancelling the choice leaves the attempt recoverable.
#[tauri::command]
pub fn cancel_draft_publication_conflict(
    draft_id: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    cancel_draft_publication_conflict_impl(&app, draft_id)
}

fn cancel_draft_publication_conflict_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
) -> Result<(), String> {
    let app = app.clone();
    let project = app.state::<ProjectState>();
    let root = require_root(&app, &project)?;
    flow::require_not_shadow(&root, &draft_id).map_err(|code| fail(&app, &draft_id, code))?;
    flow::set_attempt_state(&root, &draft_id, AttemptState::Open)
        .map_err(|code| fail(&app, &draft_id, code))?;
    announce(&app, &draft_id);
    Ok(())
}

/// GHP-FR-NAXT: abandon the standing attempt, appending nothing.
#[tauri::command]
pub fn cancel_draft_publication_attempt(
    draft_id: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    cancel_draft_publication_attempt_impl(&app, draft_id)
}

fn cancel_draft_publication_attempt_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    draft_id: String,
) -> Result<(), String> {
    let app = app.clone();
    let project = app.state::<ProjectState>();
    let root = require_root(&app, &project)?;
    flow::require_not_shadow(&root, &draft_id).map_err(|code| fail(&app, &draft_id, code))?;
    flow::clear_attempt(&root, &draft_id).map_err(|code| fail(&app, &draft_id, code))?;
    logging::log_info(
        &app,
        &logging::BUFFER,
        &[Domain::Backend],
        "draft publication attempt abandoned",
        log_fields! { "draft_id" => &draft_id },
    );
    announce(&app, &draft_id);
    Ok(())
}

/// GHP-FR-YDAN: the project's publication repository, resolved without a
/// draft. Not a Tauri command: `crate::github_polling` (GPP-FR-RGNM) is the
/// only caller. It may read GitHub to classify remotes, so it runs off the
/// window thread at its caller's side.
pub fn resolve_publication_repository<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<PublicationRepository, String> {
    let project = app.state::<ProjectState>();
    let secret = project_secret(app, &project.slot_key());
    resolve_publication_repository_with(app, secret.as_deref())
}

/// [`resolve_publication_repository`] with the secret the caller already
/// resolved, so a caller resolves the token once.
pub fn resolve_publication_repository_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    secret: Option<&str>,
) -> Result<PublicationRepository, String> {
    let project = app.state::<ProjectState>();
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT.to_string())?;
    flow::resolve_repository_from(&root, &configured_remotes(&root), secret, issues_client(app).as_ref())
}

/// GHP-FR-MJTB: open one recorded issue URL outside the application.
///
/// A URL no record of **this draft** holds is refused, so the command cannot be
/// used to open an arbitrary address from the frontend.
#[tauri::command]
pub fn open_publication_issue(
    draft_id: String,
    url: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = require_root(&app, &project)?;
    // Two conditions, and both must hold. The store is committed content that
    // reaches this machine from whoever else works on the project, so being
    // named by a record — or by the draft's GitHub-shadow issue link — is not
    // on its own evidence that a URL is safe to hand to the operating system
    // (GHP-FR-MJTB).
    if !flow::is_recorded_issue_url(&root, &draft_id, &url)? {
        return Err(fail(&app, &draft_id, ERR_DRAFT_NOT_FOUND.to_string()));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(&url, None::<&str>).map_err(|_| {
        // The URL is one this application recorded, so it is safe to name in a
        // log — it carries no credential (GHP-FR-DHXK).
        logging::log_warn(
            &app,
            &logging::BUFFER,
            &[Domain::Backend],
            "publication issue could not be opened",
            log_fields! { "draft_id" => &draft_id, "issue_url" => &url },
        );
        ERR_GITHUB_UNREACHABLE.to_string()
    })
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

/// GHP-FR-VNCS: report a refusal and hand the code back unchanged.
fn fail<R: tauri::Runtime>(app: &tauri::AppHandle<R>, draft_id: &str, code: String) -> String {
    logging::log_warn(
        app,
        &logging::BUFFER,
        &[Domain::Backend, Domain::Remote],
        MSG_PUBLISH,
        log_fields! { "draft_id" => draft_id, "refusal" => &code },
    );
    code
}

/// GHP-FR-BSYH / GHP-FR-VNCS: what happens after the GitHub calls settle.
///
/// A success over an `active` draft sets `published` **only now**, once the
/// issue exists; a failure keeps the previous status and leaves the attempt
/// standing, so the retry path of GHP-FR-OWLB still applies.
fn settle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    draft_id: &str,
    status: crate::drafts::DraftStatus,
    outcome: Result<PublicationOutcome, String>,
) -> Result<PublicationOutcome, String> {
    // The repository has just been written to, or has just refused a write, so
    // the cached eligibility answer is no longer the freshest thing known about
    // it (`remotes::forget_probes`).
    remotes::forget_probes();
    match outcome {
        Ok(PublicationOutcome::Published { record }) => {
            if status == crate::drafts::DraftStatus::Active {
                // A status write that fails does not undo a published issue:
                // the record is already appended and the issue already exists,
                // so the failure is reported and the draft keeps its status.
                if crate::drafts::set_draft_published(root, draft_id).is_err() {
                    logging::log_warn(
                        app,
                        &logging::BUFFER,
                        &[Domain::Backend],
                        "draft status not advanced after publication",
                        // The draft rather than the message: a store error
                        // phrases itself with a filesystem path, and the record
                        // this names is what a reader needs to look at.
                        log_fields! { "draft_id" => draft_id },
                    );
                } else {
                    crate::drafts::announce_drafts_changed(app, draft_id);
                }
            }
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "draft published to GitHub",
                log_fields! {
                    "draft_id" => draft_id,
                    "repository" => format!("{}/{}", record.repository_owner, record.repository_name),
                    "issue" => record.issue_number,
                },
            );
            announce(app, draft_id);
            Ok(PublicationOutcome::Published { record })
        }
        Ok(recovery) => {
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "draft publication needs recovery",
                log_fields! { "draft_id" => draft_id },
            );
            announce(app, draft_id);
            Ok(recovery)
        }
        Err(code) => {
            announce(app, draft_id);
            Err(fail(app, draft_id, code))
        }
    }
}

#[cfg(test)]
mod tests;
