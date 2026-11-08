//! Which stack a branch is in (`../../../specifications/core/GTC-git.md`
//! GTC-FR-SKDF, GTC-FR-WHEO, GTC-FR-XCMQ, GTC-FR-ZPUA).
//!
//! Pure: the open pull requests, the one GitHub stack that holds the branch's
//! pull request, and the stack record come in, and the answer goes out. No
//! request and no file read happens here.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use super::stack_record::StackRecordEntry;

/// One open pull request, as far as a stack needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenPullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) head: String,
    pub(crate) base: String,
}

/// One GitHub stack: its number, its base branch, and the head of each pull
/// request in it from the bottom to the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GithubStack {
    pub(crate) number: u64,
    pub(crate) base: String,
    pub(crate) heads: Vec<String>,
}

/// The stack of one branch (GTC-FR-TBQW).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchStack {
    pub branch: String,
    /// GTC-FR-JMVC: GitHub answered the stack reads.
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_cause: Option<String>,
    pub in_stack: bool,
    /// `"github"`, `"local_record"`, or `"pull_request_chain"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    pub is_top: bool,
    /// GTC-FR-ZPUA: an open pull request has the top as its head.
    pub top_has_pull_request: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<StackPullRequest>,
}

/// The open pull request whose head is the branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackPullRequest {
    pub number: u64,
    pub title: String,
}

pub(crate) const SOURCE_GITHUB: &str = "github";
pub(crate) const SOURCE_LOCAL_RECORD: &str = "local_record";
pub(crate) const SOURCE_CHAIN: &str = "pull_request_chain";

/// The open pull request whose head is `branch`. When GitHub lists several, the
/// one with the lowest number.
pub(crate) fn open_pull_request_of<'a>(
    open: &'a [OpenPullRequest],
    branch: &str,
) -> Option<&'a OpenPullRequest> {
    open.iter().filter(|pr| pr.head == branch).min_by_key(|pr| pr.number)
}

/// GTC-FR-TBQW: the first stack of a `GET .../stacks` answer, or `None` for an
/// empty list. An answer that is not a list of stacks is `Err`.
pub(crate) fn first_stack(answer: &Value) -> Result<Option<GithubStack>, ()> {
    let Value::Array(stacks) = answer else {
        return Err(());
    };
    let Some(stack) = stacks.first() else {
        return Ok(None);
    };
    stack_from_json(stack).map(Some).ok_or(())
}

/// One stack object of GitHub's answer.
pub(crate) fn stack_from_json(stack: &Value) -> Option<GithubStack> {
    let number = stack.get("number")?.as_u64()?;
    let base = stack.get("base")?.get("ref")?.as_str()?.to_string();
    let heads = stack
        .get("pull_requests")?
        .as_array()?
        .iter()
        .map(|pr| pr.get("head")?.get("ref")?.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    Some(GithubStack { number, base, heads })
}

/// The inputs of one resolution.
pub(crate) struct StackInputs<'a> {
    pub(crate) open: &'a [OpenPullRequest],
    /// The GitHub stack that holds the branch's open pull request, if any.
    pub(crate) github: Option<&'a GithubStack>,
    /// The entries of the stack record whose branch is a local branch.
    pub(crate) records: &'a [StackRecordEntry],
}

/// GTC-FR-WHEO, GTC-FR-XCMQ, GTC-FR-ZPUA: the stack of `branch`, with
/// `available` true.
pub(crate) fn resolve_branch_stack(branch: &str, inputs: &StackInputs<'_>) -> BranchStack {
    let own = open_pull_request_of(inputs.open, branch);
    let mut answer = BranchStack {
        branch: branch.to_string(),
        available: true,
        pull_request: own.map(|pr| StackPullRequest { number: pr.number, title: pr.title.clone() }),
        ..BranchStack::default()
    };
    let github = inputs.github.filter(|stack| stack.heads.iter().any(|head| head == branch));
    let record = inputs.records.iter().find(|entry| entry.branch == branch);
    if let Some(stack) = github {
        let at = stack.heads.iter().position(|head| head == branch).unwrap_or(0);
        answer.source = Some(SOURCE_GITHUB.to_string());
        answer.stack_number = Some(stack.number);
        answer.parent =
            Some(if at == 0 { stack.base.clone() } else { stack.heads[at - 1].clone() });
    } else if let Some(entry) = record {
        answer.source = Some(SOURCE_LOCAL_RECORD.to_string());
        answer.stack_number = entry.stack;
        answer.parent = Some(entry.parent.clone());
    } else if let Some(pr) = own.filter(|pr| in_chain(inputs.open, pr)) {
        answer.source = Some(SOURCE_CHAIN.to_string());
        answer.parent = Some(pr.base.clone());
    } else {
        return answer;
    }
    answer.in_stack = true;
    let top = top_of(branch, github, inputs);
    answer.is_top = top == branch;
    answer.top_has_pull_request = open_pull_request_of(inputs.open, &top).is_some();
    answer.top = Some(top);
    answer
}

/// GTC-FR-WHEO: the base of `pr` is the head of another open pull request, or
/// another open pull request has its head as its base.
fn in_chain(open: &[OpenPullRequest], pr: &OpenPullRequest) -> bool {
    open.iter()
        .any(|other| other.number != pr.number && (other.head == pr.base || other.base == pr.head))
}

/// GTC-FR-ZPUA: go up from `branch` until no branch stands on the current one.
fn top_of(branch: &str, github: Option<&GithubStack>, inputs: &StackInputs<'_>) -> String {
    let mut current = branch.to_string();
    let mut seen: HashSet<String> = HashSet::from([current.clone()]);
    while let Some(next) = next_up(&current, github, inputs) {
        // A record that loops back on itself ends the walk where it stands.
        if !seen.insert(next.clone()) {
            break;
        }
        current = next;
    }
    current
}

fn next_up(
    current: &str,
    github: Option<&GithubStack>,
    inputs: &StackInputs<'_>,
) -> Option<String> {
    if let Some(stack) = github {
        if let Some(at) = stack.heads.iter().position(|head| head == current) {
            if let Some(above) = stack.heads.get(at + 1) {
                return Some(above.clone());
            }
        }
    }
    if let Some(pr) = inputs.open.iter().filter(|pr| pr.base == current).min_by_key(|pr| pr.number)
    {
        return Some(pr.head.clone());
    }
    inputs
        .records
        .iter()
        .filter(|entry| entry.parent == current)
        .map(|entry| entry.branch.clone())
        .min()
}

/// GTC-FR-HEXN: the open pull requests of the chain from its bottom up to the
/// pull request whose head is `base`, by number.
pub(crate) fn chain_up_to(open: &[OpenPullRequest], base: &str) -> Vec<u64> {
    let mut chain: Vec<u64> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut current = open_pull_request_of(open, base);
    while let Some(pr) = current {
        if !seen.insert(pr.number) {
            break;
        }
        chain.push(pr.number);
        current = open_pull_request_of(open, &pr.base);
    }
    chain.reverse();
    chain
}
