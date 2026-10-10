//! The push primitive and its streamed output (GTC-FR-04 / GTC-FR-22).

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Repository};
use serde::Serialize;

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
}

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
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let outcome = push_inner(sink, buffer, emit, registry, root, store, tokens, project_key);
    // GTC-FR-04: exactly one terminal event, whatever the outcome.
    emit(
        GIT_OPERATION_FINISHED,
        serde_json::to_value(GitOperationFinished {
            operation: OPERATION_PUSH.to_string(),
            ok: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
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
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
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

    let branch = match current_branch(&repo) {
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
    };

    // GTC-FR-09 / GTC-FR-10: the credential is resolved BEFORE any transport is
    // opened, so a project that resolves no token makes no network request and
    // the typed refusal reaches the caller unchanged.
    let token = github_tokens::resolve_remote_token(store, tokens, project_key, &url).inspect_err(
        |e| {
            log_failure(
                sink,
                buffer,
                TRANSFER,
                MSG_PUSH_FAILED,
                e,
                log_fields! { "remote" => &remote_name, "branch" => &branch },
            )
        },
    )?;

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
        Some(progress::Activation::GitPush {
            branch: branch.clone(),
        }),
        || {
            let mut remote = repo
                .find_remote(&remote_name)
                .map_err(|_| ERR_NO_REMOTE_CONFIGURED.to_string())?;
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
                    upstream_set = local
                        .set_upstream(Some(&format!("{remote_name}/{branch}")))
                        .is_ok();
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
            fields.insert(
                "upstreamSet".to_string(),
                serde_json::json!(upstream_set),
            );
            log_ok(sink, buffer, LogLevel::Info, TRANSFER, "branch pushed", fields);
        }
        // Either one of `classify_transfer_error`'s two constants or the
        // remote's own per-ref rejection, which `push_update_reference` already
        // put through `redact_credentials` (GTC-FR-11).
        Err(error) => log_failure(sink, buffer, TRANSFER, MSG_PUSH_FAILED, error, fields),
    }
    result
}
