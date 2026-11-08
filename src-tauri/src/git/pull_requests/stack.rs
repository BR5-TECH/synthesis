//! The stack operations (`../../../specifications/core/GTC-git.md`
//! GTC-FR-TBQW, GTC-FR-JMVC, GTC-FR-LCYE, GTC-FR-OIMF, GTC-FR-UYGD,
//! GTC-FR-HEXN, GTC-FR-KQVA, GTC-FR-AEBW, GTC-FR-DPTN, GTC-FR-GJFM).
//!
//! A stack is read and changed only through GitHub's REST stacks endpoints and
//! the stack record of this machine (GTC-FR-SKDF). No `gh` state is read and no
//! `gh` program runs. A stack that GitHub cannot read is an answer with
//! `available` false, never an error, so no flow of the application stops for
//! it.

use serde::Serialize;
use serde_json::{json, Value};

use super::client::{read_pages, GithubFailure, GithubPullRequests, GithubWriteFailure};
use super::create::{create_pull_request_reported, NewPullRequest};
use super::model::is_from_fork;
use super::stack_model::{
    chain_up_to, first_stack, open_pull_request_of, resolve_branch_stack, stack_from_json,
    BranchStack, OpenPullRequest, StackInputs, SOURCE_LOCAL_RECORD,
};
use super::stack_record::{self, StackRecordEntry, ERR_STORE_UNAVAILABLE};
use super::{error_code, resolve_target, PullRequestAccess, Target, ERR_GITHUB_TOKEN_REJECTED};
use crate::changes::{self, ERR_UNKNOWN_BRANCH};
use crate::fs;
use crate::git::TRANSFER;
use crate::github_tokens;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogSink};

/// GitHub answered 404 for the stacks of the repository: stacks are not
/// enabled for it, or the token cannot see them.
pub const ERR_STACKS_NOT_ENABLED: &str = "stacks_not_enabled";
/// GTC-FR-LCYE / GTC-FR-KQVA: the stacks of the repository cannot be read.
pub const ERR_STACK_UNAVAILABLE: &str = "stack_unavailable";
/// GTC-FR-LCYE: the parent branch is in no stack.
pub const ERR_STACK_PARENT_NOT_IN_STACK: &str = "stack_parent_not_in_stack";
/// GTC-FR-LCYE / GTC-FR-KQVA: no open pull request has the parent as its head.
pub const ERR_STACK_PARENT_HAS_NO_PULL_REQUEST: &str = "stack_parent_has_no_pull_request";
/// GTC-FR-LCYE: the parent branch is not the top of its stack.
pub const ERR_STACK_PARENT_NOT_TOP: &str = "stack_parent_not_top";
/// GTC-FR-KQVA: no record of the head names the base as its parent.
pub const ERR_STACK_RECORD_MISSING: &str = "stack_record_missing";
/// GTC-FR-KQVA: GitHub answered 409, another request changes the stack.
pub const ERR_STACK_BUSY: &str = "stack_busy";
/// GTC-FR-KQVA: GitHub answered 422. The reason follows after a colon.
pub const ERR_STACK_REJECTED: &str = "stack_rejected";

/// GTC-FR-UYGD: the pull request that a submission to a stack created.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackedPullRequest {
    pub number: u64,
    pub url: String,
    pub stack: StackStep,
}

/// GTC-FR-HEXN / GTC-FR-KQVA: what the stack step did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackStep {
    /// `"added"`, `"created"`, or `"not_added"`.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StackStep {
    fn done(state: &str, number: u64) -> StackStep {
        StackStep { state: state.to_string(), stack_number: Some(number), error: None }
    }

    fn not_added(error: impl Into<String>) -> StackStep {
        StackStep { state: "not_added".to_string(), stack_number: None, error: Some(error.into()) }
    }
}

/// GTC-FR-JMVC: the answer for a stack that GitHub cannot read.
fn unavailable(branch: &str, cause: &str, own: Option<&OpenPullRequest>) -> BranchStack {
    BranchStack {
        branch: branch.to_string(),
        available: false,
        unavailable_cause: Some(error_code(cause).to_string()),
        pull_request: own.map(|pr| super::stack_model::StackPullRequest {
            number: pr.number,
            title: pr.title.clone(),
        }),
        ..BranchStack::default()
    }
}

/// The typed cause of a failed stacks read.
fn stacks_read_error(failure: GithubFailure) -> String {
    match failure {
        GithubFailure::NotFound => ERR_STACKS_NOT_ENABLED.to_string(),
        GithubFailure::Unsupported => ERR_STACK_UNAVAILABLE.to_string(),
        GithubFailure::Rejected => ERR_GITHUB_TOKEN_REJECTED.to_string(),
        GithubFailure::Unreachable => github_tokens::ERR_GITHUB_UNREACHABLE.to_string(),
        GithubFailure::TlsUntrusted(failure) => failure.wire(),
    }
}

/// GTC-FR-KQVA: the typed cause of a failed stack write.
fn stack_write_error(failure: GithubWriteFailure) -> String {
    match failure {
        GithubWriteFailure::NotFound => ERR_STACK_UNAVAILABLE.to_string(),
        GithubWriteFailure::Rejected => ERR_GITHUB_TOKEN_REJECTED.to_string(),
        GithubWriteFailure::Conflict => ERR_STACK_BUSY.to_string(),
        GithubWriteFailure::Invalid(reason) if reason.trim().is_empty() => {
            ERR_STACK_REJECTED.to_string()
        }
        GithubWriteFailure::Invalid(reason) => format!("{ERR_STACK_REJECTED}: {}", reason.trim()),
        GithubWriteFailure::Unreachable => github_tokens::ERR_GITHUB_UNREACHABLE.to_string(),
        GithubWriteFailure::TlsUntrusted(failure) => failure.wire(),
    }
}

/// GTC-FR-TBQW: the open pull requests of the repository. A pull request from a
/// fork is left out: a stack holds the branches of one repository. A list that
/// stops at the page limit is `stack_unavailable` (GTC-FR-JMVC): a partial list
/// can miss the pull request of the branch, and then the stack is wrong.
fn read_open(
    client: &dyn GithubPullRequests,
    target: &Target,
) -> Result<Vec<OpenPullRequest>, String> {
    let path = format!(
        "/repos/{}/{}/pulls?state=open&sort=updated&direction=desc",
        target.owner, target.repo
    );
    let pages = read_pages(client, &target.secret, &path).map_err(stacks_read_error)?;
    if pages.truncated {
        return Err(ERR_STACK_UNAVAILABLE.to_string());
    }
    Ok(pages.items.iter().filter(|row| !is_from_fork(row)).filter_map(open_from_json).collect())
}

fn open_from_json(row: &Value) -> Option<OpenPullRequest> {
    let text = |side: &str| row.get(side)?.get("ref")?.as_str().map(str::to_string);
    Some(OpenPullRequest {
        number: row.get("number")?.as_u64()?,
        title: row.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
        head: text("head")?,
        base: text("base")?,
    })
}

/// GTC-FR-RNXK: the entries of the stack record whose branch is a local
/// branch. A record that cannot be read is empty, and is logged.
fn read_records<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    machine: Option<&fs::RootFs>,
    repo: &git2::Repository,
) -> Vec<StackRecordEntry>
where
    S: LogSink + Clone + Send + 'static,
{
    record_entries(sink, buffer, machine)
        .into_iter()
        .filter(|entry| repo.find_branch(&entry.branch, git2::BranchType::Local).is_ok())
        .collect()
}

/// GTC-FR-RNXK: every entry of the stack record. A record that cannot be read
/// is empty, and the cause is logged at `WARN`.
fn record_entries<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    machine: Option<&fs::RootFs>,
) -> Vec<StackRecordEntry>
where
    S: LogSink + Clone + Send + 'static,
{
    let read = machine
        .ok_or_else(|| ERR_STORE_UNAVAILABLE.to_string())
        .and_then(stack_record::read_entries);
    read.unwrap_or_else(|error| {
        logging::log_warn(
            sink,
            buffer,
            TRANSFER,
            "stack record read failed",
            log_fields! { "error" => error },
        );
        Vec::new()
    })
}

/// GTC-FR-TBQW: the stack of `branch` against an already resolved target.
fn stack_for(
    client: &dyn GithubPullRequests,
    target: &Target,
    records: &[StackRecordEntry],
    branch: &str,
) -> BranchStack {
    let open = match read_open(client, target) {
        Ok(open) => open,
        Err(cause) => return unavailable(branch, &cause, None),
    };
    let own = open_pull_request_of(&open, branch);
    let path = match own {
        Some(pr) => {
            format!("/repos/{}/{}/stacks?pull_request={}", target.owner, target.repo, pr.number)
        }
        None => format!("/repos/{}/{}/stacks?per_page=1", target.owner, target.repo),
    };
    let answer = match client.get_json(&target.secret, &path) {
        Ok(answer) => answer,
        Err(failure) => return unavailable(branch, &stacks_read_error(failure), own),
    };
    let github = match (own, first_stack(&answer)) {
        (_, Err(())) => {
            return unavailable(branch, github_tokens::ERR_GITHUB_UNREACHABLE, own);
        }
        (Some(_), Ok(stack)) => stack,
        (None, Ok(_)) => None,
    };
    resolve_branch_stack(branch, &StackInputs { open: &open, github: github.as_ref(), records })
}

/// The stack of `branch`, logged for `operation` (GTC-FR-GJFM). The record
/// entry of `replaced`, when given, is left out, because the caller writes a
/// new entry in its place (GTC-FR-OIMF).
#[allow(clippy::too_many_arguments)] // The two log targets and the inputs of one read.
fn compute_branch_stack<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    operation: &str,
    access: &PullRequestAccess<'_>,
    machine: Option<&fs::RootFs>,
    repo: &git2::Repository,
    branch: &str,
    replaced: Option<&str>,
) -> BranchStack
where
    S: LogSink + Clone + Send + 'static,
{
    let (stack, target) = match resolve_target(access.project) {
        Err(cause) => (unavailable(branch, &cause, None), None),
        Ok(target) => {
            let mut records = read_records(sink, buffer, machine, repo);
            if let Some(replaced) = replaced {
                records.retain(|entry| entry.branch != replaced);
            }
            let scoped = access.client.for_host(&target.host);
            let client = scoped.as_deref().unwrap_or(access.client);
            (stack_for(client, &target, &records, branch), Some(target))
        }
    };
    log_stack(sink, buffer, operation, &stack, target.as_ref());
    stack
}

fn log_stack<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    operation: &str,
    stack: &BranchStack,
    target: Option<&Target>,
) where
    S: LogSink + Clone + Send + 'static,
{
    let fields = log_fields! {
        "operation" => operation,
        "owner" => target.map(|t| t.owner.as_str()),
        "repo" => target.map(|t| t.repo.as_str()),
        "branch" => stack.branch,
        "available" => stack.available,
        "cause" => stack.unavailable_cause,
        "inStack" => stack.in_stack,
        "source" => stack.source,
        "stackNumber" => stack.stack_number,
        "parent" => stack.parent,
        "top" => stack.top,
        "pullRequest" => stack.pull_request.as_ref().map(|pr| pr.number),
    };
    if stack.available {
        logging::log_info(sink, buffer, TRANSFER, "branch stack read", fields);
    } else {
        // GTC-FR-JMVC: the cause is worth knowing, and no flow stops for it.
        logging::log_warn(sink, buffer, TRANSFER, "branch stack unavailable", fields);
    }
}

fn refuse<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    operation: &str,
    branch: &str,
    error: &str,
) -> String
where
    S: LogSink + Clone + Send + 'static,
{
    logging::log_warn(
        sink,
        buffer,
        TRANSFER,
        "branch stack refused",
        log_fields! { "operation" => operation, "branch" => branch, "error" => error },
    );
    error.to_string()
}

/// GTC-FR-TBQW / GTC-FR-JMVC: `get_branch_stack(branch)`.
pub(crate) fn get_branch_stack_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    access: &PullRequestAccess<'_>,
    machine: Option<&fs::RootFs>,
    branch: &str,
) -> Result<BranchStack, String>
where
    S: LogSink + Clone + Send + 'static,
{
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(refuse(sink, buffer, "get_branch_stack", branch, ERR_UNKNOWN_BRANCH));
    }
    let repo = changes::open_repo(access.project.root)
        .map_err(|error| refuse(sink, buffer, "get_branch_stack", branch, &error))?;
    Ok(compute_branch_stack(sink, buffer, "get_branch_stack", access, machine, &repo, branch, None))
}

/// GTC-FR-LCYE / GTC-FR-OIMF: `add_branch_to_stack(branch, parent)`.
pub(crate) fn add_branch_to_stack_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    access: &PullRequestAccess<'_>,
    machine: Option<&fs::RootFs>,
    branch: &str,
    parent: &str,
) -> Result<BranchStack, String>
where
    S: LogSink + Clone + Send + 'static,
{
    const OPERATION: &str = "add_branch_to_stack";
    let (branch, parent) = (branch.trim(), parent.trim());
    let repo = changes::open_repo(access.project.root)
        .map_err(|error| refuse(sink, buffer, OPERATION, branch, &error))?;
    if branch.is_empty() || repo.find_branch(branch, git2::BranchType::Local).is_err() {
        return Err(refuse(sink, buffer, OPERATION, branch, ERR_UNKNOWN_BRANCH));
    }
    // GTC-FR-OIMF: an earlier entry of `branch` is replaced, so it does not
    // stand on `parent` while the parent is checked.
    let below =
        compute_branch_stack(sink, buffer, OPERATION, access, machine, &repo, parent, Some(branch));
    let refusal = if !below.available {
        Some(ERR_STACK_UNAVAILABLE)
    } else if !below.in_stack {
        Some(ERR_STACK_PARENT_NOT_IN_STACK)
    } else if below.pull_request.is_none() {
        Some(ERR_STACK_PARENT_HAS_NO_PULL_REQUEST)
    } else if !below.is_top {
        Some(ERR_STACK_PARENT_NOT_TOP)
    } else {
        None
    };
    if let Some(error) = refusal {
        return Err(refuse(sink, buffer, OPERATION, branch, error));
    }
    let entry = StackRecordEntry {
        branch: branch.to_string(),
        parent: parent.to_string(),
        stack: below.stack_number,
    };
    machine
        .ok_or_else(|| ERR_STORE_UNAVAILABLE.to_string())
        .and_then(|machine| stack_record::put_entry(machine, entry))
        .map_err(|error| refuse(sink, buffer, OPERATION, branch, &error))?;
    logging::log_info(
        sink,
        buffer,
        TRANSFER,
        "branch added to stack",
        log_fields! {
            "branch" => branch,
            "parent" => parent,
            "stackNumber" => below.stack_number,
        },
    );
    Ok(BranchStack {
        branch: branch.to_string(),
        available: true,
        in_stack: true,
        source: Some(SOURCE_LOCAL_RECORD.to_string()),
        stack_number: below.stack_number,
        parent: Some(parent.to_string()),
        top: Some(branch.to_string()),
        is_top: true,
        ..BranchStack::default()
    })
}

/// GTC-FR-UYGD: `submit_pull_request_to_stack(title, body, base, head, draft)`.
pub(crate) fn submit_pull_request_to_stack_reported<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    access: &PullRequestAccess<'_>,
    machine: Option<&fs::RootFs>,
    request: &NewPullRequest<'_>,
) -> Result<StackedPullRequest, String>
where
    S: LogSink + Clone + Send + 'static,
{
    let created = create_pull_request_reported(sink, buffer, access, request)?;
    let (head, base) = (request.head.trim(), request.base.trim());
    let step = stack_step(sink, buffer, access, machine, head, base, created.number);
    let fields = log_fields! {
        "head" => head,
        "base" => base,
        "number" => created.number,
        "state" => step.state,
        "stackNumber" => step.stack_number,
        "error" => step.error.as_deref().map(error_code),
    };
    if step.error.is_some() {
        logging::log_warn(sink, buffer, TRANSFER, "pull request not added to stack", fields);
    } else {
        logging::log_info(sink, buffer, TRANSFER, "pull request added to stack", fields);
    }
    Ok(StackedPullRequest { number: created.number, url: created.url, stack: step })
}

/// GTC-FR-HEXN / GTC-FR-KQVA / GTC-FR-AEBW: add pull request `number` to the
/// stack of `base`.
fn stack_step<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    access: &PullRequestAccess<'_>,
    machine: Option<&fs::RootFs>,
    head: &str,
    base: &str,
    number: u64,
) -> StackStep
where
    S: LogSink + Clone + Send + 'static,
{
    // GTC-FR-RNXK: a record that cannot be read is empty, and is logged.
    let entry = record_entries(sink, buffer, machine)
        .into_iter()
        .find(|entry| entry.branch == head && entry.parent == base);
    let Some(entry) = entry else {
        return StackStep::not_added(ERR_STACK_RECORD_MISSING);
    };
    let target = match resolve_target(access.project) {
        Ok(target) => target,
        Err(error) => return StackStep::not_added(error_code(&error)),
    };
    let scoped = access.client.for_host(&target.host);
    let client = scoped.as_deref().unwrap_or(access.client);
    let (write, step) = match entry.stack {
        Some(stack) => ("add", add_to(client, &target, stack, number)),
        None => add_to_stack_of_base(sink, buffer, client, &target, base, number),
    };
    // GTC-FR-GJFM: each stack write, with its kind and its typed cause.
    logging::log_info(
        sink,
        buffer,
        TRANSFER,
        "stack write",
        log_fields! {
            "owner" => target.owner,
            "repo" => target.repo,
            "write" => write,
            "head" => head,
            "base" => base,
            "number" => number,
            "state" => step.state,
            "stackNumber" => step.stack_number,
            "error" => step.error.as_deref().map(error_code),
        },
    );
    if step.error.is_none() {
        if let Some(machine) = machine {
            if let Err(error) = stack_record::remove_entry(machine, head) {
                logging::log_warn(
                    sink,
                    buffer,
                    TRANSFER,
                    "stack record entry not removed",
                    log_fields! { "branch" => head, "error" => error },
                );
            }
        }
    }
    step
}

/// GTC-FR-HEXN: `POST .../stacks/{stack}/add` with the new pull request.
fn add_to(client: &dyn GithubPullRequests, target: &Target, stack: u64, number: u64) -> StackStep {
    let path = format!("/repos/{}/{}/stacks/{stack}/add", target.owner, target.repo);
    match client.post_json(&target.secret, &path, &json!({ "pull_requests": [number] })) {
        Ok(answer) => StackStep::done(
            "added",
            stack_from_json(&answer).map(|stack| stack.number).unwrap_or(stack),
        ),
        Err(failure) => StackStep::not_added(stack_write_error(failure)),
    }
}

/// GTC-FR-HEXN: read the stack of `base`, then add to it, or create a stack of
/// the chain below `base` and the new pull request. Gives the kind of the
/// write with the step.
fn add_to_stack_of_base<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    client: &dyn GithubPullRequests,
    target: &Target,
    base: &str,
    number: u64,
) -> (&'static str, StackStep)
where
    S: LogSink + Clone + Send + 'static,
{
    // GTC-FR-KQVA: a failed read is `stack_unavailable`; its own cause is logged.
    let unavailable = |cause: &str| {
        logging::log_warn(
            sink,
            buffer,
            TRANSFER,
            "stack read for the stack step failed",
            log_fields! { "owner" => target.owner, "repo" => target.repo, "base" => base, "cause" => cause },
        );
        ("read", StackStep::not_added(ERR_STACK_UNAVAILABLE))
    };
    let open = match read_open(client, target) {
        Ok(open) => open,
        Err(cause) => return unavailable(&cause),
    };
    let Some(below) = open_pull_request_of(&open, base) else {
        return ("read", StackStep::not_added(ERR_STACK_PARENT_HAS_NO_PULL_REQUEST));
    };
    let path =
        format!("/repos/{}/{}/stacks?pull_request={}", target.owner, target.repo, below.number);
    let existing = match client.get_json(&target.secret, &path) {
        Ok(answer) => match first_stack(&answer) {
            Ok(existing) => existing,
            Err(()) => return unavailable(github_tokens::ERR_GITHUB_UNREACHABLE),
        },
        Err(failure) => return unavailable(&stacks_read_error(failure)),
    };
    if let Some(stack) = existing {
        return ("add", add_to(client, target, stack.number, number));
    }
    let mut pull_requests = chain_up_to(&open, base);
    pull_requests.push(number);
    let path = format!("/repos/{}/{}/stacks", target.owner, target.repo);
    let step =
        match client.post_json(&target.secret, &path, &json!({ "pull_requests": pull_requests })) {
            // GitHub created the stack also when its answer names no number.
            Ok(answer) => StackStep {
                state: "created".to_string(),
                stack_number: stack_from_json(&answer).map(|stack| stack.number),
                error: None,
            },
            Err(failure) => StackStep::not_added(stack_write_error(failure)),
        };
    ("create", step)
}
