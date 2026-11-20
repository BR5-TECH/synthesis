//! The push primitive and its streamed output (GTC-FR-04 / GTC-FR-22).

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Repository};
use serde::Serialize;

use crate::changes::repo_set::RepoSet;
use crate::changes::{self};
use crate::github_tokens::{self, GithubTokens};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Fields, LogBuffer, LogLevel, LogSink};
use crate::progress::{self, ProgressRegistry, ProgressSink};

use super::*;

// ---------------------------------------------------------------------------
// Push (GTC-FR-04 / GTC-FR-09 / GTC-FR-22)
// ---------------------------------------------------------------------------

/// GTC-FR-04: push and pull output, one line per event.
pub const GIT_OUTPUT_LINE: &str = "git-output-line";
/// GTC-FR-04: the single terminal event a transfer ends with.
pub const GIT_OPERATION_FINISHED: &str = "git-operation-finished";

/// Label the status bar renders while a push runs (GTC-FR-22). Names no remote,
/// for the reason `FETCH_LABEL` names none.
pub(crate) const PUSH_LABEL: &str = "Pushing";

/// One line of transfer output (GTC-FR-04). Carries no remote URL, so no
/// embedded credential can ride out on it (GTC-FR-11).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitOutputLine {
    /// `"push"` — the operation the line belongs to, so one output area can
    /// carry every transfer.
    pub operation: String,
    pub line: String,
}

/// The terminal status of a transfer (GTC-FR-04).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitOperationFinished {
    pub operation: String,
    pub ok: bool,
    /// The typed cause when `ok` is false. Never carries a token secret
    /// (GTC-FR-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// GTC-FR-PQQV: what the push of the current branch pushed and skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_summary: Option<PushSummary>,
}

/// GTC-FR-PQQV: the repositories that a push of the current branch pushed, in
/// the order pushed, and the ones it skipped.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushSummary {
    pub pushed: Vec<String>,
    pub skipped: Vec<SkippedRepository>,
}

/// GSM-FR-MJHC: one repository that the push skipped, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRepository {
    pub repository: String,
    /// `detached_head` or `no_remote`.
    pub reason: String,
}

/// What the push of the current branch does with one member.
enum PushDecision {
    Push,
    Skip(&'static str),
}

/// GSM-FR-REIT / GSM-FR-MJHC: the members to push, deepest first and the main
/// repository last, and the members to skip with the reason. A member with
/// nothing to publish is left out. A member whose state cannot be read stops
/// the plan: a parent pushed past it could record a pointer that its remote
/// does not hold (GSM-FR-GTPI).
fn push_plan(set: &RepoSet) -> Result<Vec<(usize, PushDecision)>, String> {
    let mut plan = Vec::new();
    for i in set.deepest_first() {
        let state = member_sync_state(&set.members[i])?;
        if state.branch.is_none() {
            plan.push((i, PushDecision::Skip(SKIP_DETACHED_HEAD)));
        } else if !state.has_remote {
            plan.push((i, PushDecision::Skip(SKIP_NO_REMOTE)));
        } else if state.is_affected() {
            plan.push((i, PushDecision::Push));
        }
    }
    Ok(plan)
}

/// GSM-FR-GHFY: the repositories that the push of the current branch writes,
/// as `(project-relative root, checkout)`, so the main branch write check runs
/// on each before the first push. A project with no submodule pushes its main
/// repository as it always did.
pub fn push_targets(root: &Path) -> Result<Vec<(String, std::path::PathBuf)>, String> {
    let set = RepoSet::open(root)?;
    if !set.has_submodules() {
        return Ok(vec![(String::new(), root.to_path_buf())]);
    }
    Ok(push_plan(&set)?
        .into_iter()
        .filter(|(_, decision)| matches!(decision, PushDecision::Push))
        .filter_map(|(i, _)| {
            let member = &set.members[i];
            let checkout = if member.is_main() {
                Some(root.to_path_buf())
            } else {
                member.repo.workdir().map(Path::to_path_buf)
            };
            checkout.map(|c| (member.path.clone(), c))
        })
        .collect())
}

/// GSM-FR-REIT: the sync state of one member of the set.
fn member_sync_state(
    member: &crate::changes::repo_set::Member,
) -> Result<RepositorySyncState, String> {
    let state = repository_sync_state(&member.repo)?;
    Ok(RepositorySyncState {
        repository: member.path.clone(),
        branch: member.branch(),
        has_remote: state.has_remote,
        has_upstream: state.has_upstream,
        ahead: state.ahead,
        behind: state.behind,
    })
}

/// GSM-FR-MJHC: the skip reason of a submodule with a detached `HEAD`.
pub const SKIP_DETACHED_HEAD: &str = "detached_head";
/// GSM-FR-MJHC: the skip reason of a submodule with no remote.
pub const SKIP_NO_REMOTE: &str = "no_remote";

/// The `"push"` operation name both event payloads carry.
pub const OPERATION_PUSH: &str = "push";

/// A detached or unborn `HEAD` names no branch, and a push publishes a branch.
pub const ERR_NO_BRANCH_CHECKED_OUT: &str = "no branch is checked out";

/// The active worktree's current branch, or `None` when `HEAD` is detached or
/// unborn — the two states that name no branch.
pub(crate) fn current_branch(repo: &Repository) -> Option<String> {
    if repo.head_detached().unwrap_or(false) {
        return None;
    }
    let head = repo.head().ok()?;
    head.shorthand().ok().map(|s| s.to_string())
}

/// Strip anything that could be a credential out of a line the transport
/// produced before it is published (GTC-FR-11).
///
/// Sideband text is the remote's own, and a remote that echoes a URL back can
/// echo one carrying userinfo. Rather than trying to recognise a secret, any
/// `scheme://userinfo@host` is rewritten with the userinfo removed.
pub fn redact_credentials(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(scheme_at) = rest.find("://") {
        let (before, after) = rest.split_at(scheme_at + 3);
        out.push_str(before);
        // Userinfo ends at the first `@`, and only when that `@` precedes the
        // end of the authority — a `@` after a `/` is part of the path.
        let authority_end = after.find(['/', ' ']).unwrap_or(after.len());
        match after[..authority_end].rfind('@') {
            Some(at) => {
                out.push_str(&after[at + 1..authority_end]);
            }
            None => out.push_str(&after[..authority_end]),
        }
        rest = &after[authority_end..];
    }
    out.push_str(rest);
    out
}

/// The checkout that an operation works in, and where the project's GitHub
/// token is resolved from (GTC-FR-09, GTC-FR-10).
#[derive(Clone, Copy)]
pub struct GithubAccess<'a> {
    pub root: &'a Path,
    pub store: &'a GlobalSettingsStore,
    pub tokens: &'a GithubTokens,
    pub project_key: &'a str,
}

/// The refspec that publishes `branch`, and the ref it lands on.
pub(crate) fn push_refspec(branch: &str) -> String {
    format!("refs/heads/{branch}:refs/heads/{branch}")
}

/// GTC-FR-YHDU: push the branch a checkout holds, to the project's primary
/// remote, streaming its output and reporting its progress.
///
/// **The one push primitive**, parameterised by the checkout it pushes from.
/// `push_current_branch` composes it for the active worktree, and the
/// graduation run driver composes it for a work stream's checkout
/// (`../specifications/core/GRD-graduation.md` GRD-FR-PXVJ). Each reports where
/// its own caller reports: what a caller does with `emit` is what decides
/// whether the Git panel hears about the transfer.
pub fn push_branch_at<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &impl Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    access: GithubAccess<'_>,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    push_finished(emit, push_inner(sink, buffer, emit, registry, access, None))
}

/// GTC-FR-YHDU / GTC-FR-RZNQ: the same primitive for a named local branch,
/// which the checkout at `access.root` does not need to have out.
pub fn push_named_branch_at<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &impl Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    access: GithubAccess<'_>,
    branch: &str,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    push_finished(emit, push_inner(sink, buffer, emit, registry, access, Some(branch)))
}

/// GTC-FR-PQQV: push each affected repository of the set of the checkout at
/// `access.root`, the deepest first and the main repository last
/// (GSM-FR-REIT). A failure stops the run (GSM-FR-GTPI). One terminal event
/// carries what was pushed and skipped.
pub fn push_affected_at<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &impl Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    access: GithubAccess<'_>,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let mut summary = PushSummary::default();
    let outcome = push_affected_inner(sink, buffer, emit, registry, access, &mut summary);
    finished_with(emit, outcome, Some(summary))
}

fn push_affected_inner<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &impl Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    access: GithubAccess<'_>,
    summary: &mut PushSummary,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let set = RepoSet::open(access.root)
        .inspect_err(|e| log_failure(sink, buffer, TRANSFER, MSG_PUSH_FAILED, e, Fields::new()))?;
    if !set.has_submodules() {
        // A project with no submodule pushes its branch as it always did, its
        // own failures included.
        push_inner(sink, buffer, emit, registry, access, None)?;
        summary.pushed.push(String::new());
        return Ok(());
    }
    for (i, decision) in push_plan(&set)? {
        let member = &set.members[i];
        let label = log_fields! { "repository" => &member.path };
        let root = match decision {
            PushDecision::Skip(reason) => {
                // GSM-FR-WPKJ: each skip is logged with the repository path.
                logging::log_info(
                    sink,
                    buffer,
                    TRANSFER,
                    "push skipped a repository",
                    log_fields! { "repository" => &member.path, "reason" => reason },
                );
                summary.skipped.push(SkippedRepository {
                    repository: member.path.clone(),
                    reason: reason.to_string(),
                });
                continue;
            }
            PushDecision::Push => {
                if member.is_main() {
                    Some(access.root)
                } else {
                    member.repo.workdir()
                }
            }
        };
        let Some(root) = root else { continue };
        logging::log_info(sink, buffer, TRANSFER, "pushing a repository", label.clone());
        let checkout = GithubAccess { root, ..access };
        if let Err(e) = push_inner(sink, buffer, emit, registry, checkout, None) {
            // GSM-FR-WPKJ, GSM-FR-GTPI: the repository that stopped the run.
            // The error stays the typed cause: the panel reads its code to
            // route a token failure (CHG-FR-43), and `pushed` says how far the
            // run came.
            log_failure(sink, buffer, TRANSFER, MSG_PUSH_FAILED, &e, label);
            return Err(e);
        }
        // GSM-FR-WPKJ: each push made in a repository.
        logging::log_info(sink, buffer, TRANSFER, "pushed a repository", label);
        summary.pushed.push(member.path.clone());
    }
    Ok(())
}

fn push_finished(
    emit: &impl Fn(&str, serde_json::Value),
    outcome: Result<(), String>,
) -> Result<(), String> {
    finished_with(emit, outcome, None)
}

fn finished_with(
    emit: &impl Fn(&str, serde_json::Value),
    outcome: Result<(), String>,
    push_summary: Option<PushSummary>,
) -> Result<(), String> {
    // GTC-FR-04: exactly one terminal event, whatever the outcome.
    emit(
        GIT_OPERATION_FINISHED,
        serde_json::to_value(GitOperationFinished {
            operation: OPERATION_PUSH.to_string(),
            ok: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
            push_summary,
        })
        .unwrap_or_default(),
    );
    outcome
}

pub(crate) fn push_inner<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &impl Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    access: GithubAccess<'_>,
    named: Option<&str>,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let GithubAccess { root, store, tokens, project_key } = access;
    // Every failure below is reported before it is returned, so a push that
    // never reached the wire is as visible in the panel as one the remote
    // refused — the two look identical to an author whose branch did not move.
    let repo = changes::open_repo(root)
        .inspect_err(|e| log_failure(sink, buffer, TRANSFER, MSG_PUSH_FAILED, e, Fields::new()))?;
    let Some(remote_name) = changes::primary_remote_name(&repo) else {
        log_failure(
            sink,
            buffer,
            TRANSFER,
            MSG_PUSH_FAILED,
            ERR_NO_REMOTE_CONFIGURED,
            Fields::new(),
        );
        return Err(ERR_NO_REMOTE_CONFIGURED.to_string());
    };
    let url = match repo.find_remote(&remote_name) {
        Ok(remote) => remote.url().unwrap_or_default().to_string(),
        Err(_) => String::new(),
    };

    let branch = match named {
        // GTC-FR-KUVE: a named branch must be a local branch, checked before
        // any transport is opened.
        Some(name) => {
            if name.is_empty() || repo.find_branch(name, BranchType::Local).is_err() {
                log_failure(
                    sink,
                    buffer,
                    TRANSFER,
                    MSG_PUSH_FAILED,
                    changes::ERR_UNKNOWN_BRANCH,
                    log_fields! { "remote" => &remote_name, "branch" => name },
                );
                return Err(changes::ERR_UNKNOWN_BRANCH.to_string());
            }
            name.to_string()
        }
        None => match current_branch(&repo) {
            Some(branch) => branch,
            None => {
                log_failure(
                    sink,
                    buffer,
                    TRANSFER,
                    MSG_PUSH_FAILED,
                    ERR_NO_BRANCH_CHECKED_OUT,
                    log_fields! { "remote" => &remote_name },
                );
                return Err(ERR_NO_BRANCH_CHECKED_OUT.to_string());
            }
        },
    };

    // GTC-FR-09 / GTC-FR-10: the credential is resolved BEFORE any transport is
    // opened, so a project that resolves no token makes no network request and
    // the typed refusal reaches the caller unchanged.
    let token =
        github_tokens::resolve_remote_token(store, tokens, project_key, &url).inspect_err(|e| {
            log_failure(
                sink,
                buffer,
                TRANSFER,
                MSG_PUSH_FAILED,
                e,
                log_fields! { "remote" => &remote_name, "branch" => &branch },
            )
        })?;

    let line = |text: String| {
        emit(
            GIT_OUTPUT_LINE,
            serde_json::to_value(GitOutputLine {
                operation: OPERATION_PUSH.to_string(),
                line: redact_credentials(&text),
            })
            .unwrap_or_default(),
        );
    };
    line(format!("Pushing {branch} to {remote_name}"));
    logging::log_info(
        sink,
        buffer,
        TRANSFER,
        "pushing branch",
        log_fields! {
            "branch" => &branch,
            "remote" => &remote_name,
            "authenticated" => token.is_some(),
        },
    );
    let started = Instant::now();

    // GTC-FR-22: attributed under `kind = "git"` as well as streaming, so a push
    // is visible in the status bar for as long as it runs.
    let result = progress::attribute_with_activation(
        sink,
        registry,
        PROGRESS_KIND_GIT,
        PUSH_LABEL,
        Some(root.to_path_buf()),
        // PRG-FR-KXQW / GTC-FR-22: the Git panel selects this branch.
        Some(progress::Activation::GitPush { branch: branch.clone() }),
        || {
            let mut remote =
                repo.find_remote(&remote_name).map_err(|_| ERR_NO_REMOTE_CONFIGURED.to_string())?;
            // The remote's per-ref verdict, declared ahead of the callbacks that
            // borrow it so it outlives them.
            //
            // A push can succeed at the transport and still be refused for a ref
            // (a non-fast-forward), which libgit2 reports only here.
            let rejection = RefCell::new(None::<String>);
            let mut callbacks = git2::RemoteCallbacks::new();
            install_credentials(&mut callbacks, token.as_deref());
            callbacks.sideband_progress(|data| {
                for chunk in String::from_utf8_lossy(data).split(['\r', '\n']) {
                    if !chunk.trim().is_empty() {
                        line(chunk.trim().to_string());
                    }
                }
                true
            });
            callbacks.push_update_reference(|reference, status| {
                match status {
                    None => line(format!("{reference} updated")),
                    Some(reason) => {
                        let reason = redact_credentials(reason);
                        line(format!("{reference} rejected: {reason}"));
                        *rejection.borrow_mut() = Some(reason);
                    }
                }
                Ok(())
            });

            let mut opts = git2::PushOptions::new();
            opts.remote_callbacks(callbacks);
            remote
                .push(&[push_refspec(&branch).as_str()], Some(&mut opts))
                .map_err(|e| classify_transfer_error(&e))?;
            if let Some(reason) = rejection.borrow().clone() {
                return Err(reason);
            }
            Ok(())
        },
    );

    let mut upstream_set = false;
    match &result {
        Ok(()) => {
            // A branch that had no upstream now has one, so the next read of the
            // sync state (GTC-FR-21) reports it as published rather than as
            // never pushed.
            if let Ok(mut local) = repo.find_branch(&branch, BranchType::Local) {
                if local.upstream().is_err() {
                    upstream_set =
                        local.set_upstream(Some(&format!("{remote_name}/{branch}"))).is_ok();
                }
            }
            line("Done".to_string());
        }
        Err(e) => line(format!("Push failed: {e}")),
    }

    let fields = log_fields! {
        "branch" => &branch,
        "remote" => &remote_name,
        "durationMs" => duration_ms(started),
    };
    match &result {
        Ok(()) => {
            let mut fields = fields;
            // Whether the push published a branch that had never been published
            // before — the state change behind a Changes panel that stops
            // offering "publish" and starts offering "push" (CHG-FR-37).
            fields.insert("upstreamSet".to_string(), serde_json::json!(upstream_set));
            log_ok(sink, buffer, LogLevel::Info, TRANSFER, "branch pushed", fields);
        }
        // Either one of `classify_transfer_error`'s two constants or the
        // remote's own per-ref rejection, which `push_update_reference` already
        // put through `redact_credentials` (GTC-FR-11).
        Err(error) => log_failure(sink, buffer, TRANSFER, MSG_PUSH_FAILED, error, fields),
    }
    result
}
