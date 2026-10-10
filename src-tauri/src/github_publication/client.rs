//! The GitHub Issues client (GHP-FR-QJNV, GHP-FR-MZPR, GHP-FR-IEQC,
//! GHP-FR-XOBH, GHP-FR-YPGL).
//!
//! Direct HTTPS against GitHub's REST API through `ureq`, the same client
//! `github_tokens` verifies a token with. The `gh` CLI is neither required nor
//! invoked, and nothing here shells out.
//!
//! The trait is what makes the whole publication flow testable without a
//! network: the production implementation is the only thing in this module that
//! opens a socket.

use std::time::Duration;

use super::records::*;

/// The most pages one list read follows, of [`PAGE_SIZE`] rows each
/// (GHP-github-publication.md, non-functional requirements).
const MAX_PAGES: u32 = 3;
const PAGE_SIZE: u32 = 100;

/// GHP-FR-MZPR: what the repository record read found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The repository accepts a new issue, and nothing says this token may not
    /// write one.
    Publishable,
    /// The repository record is not readable with this token.
    IssuesUnreadable,
    /// The repository is readable and accepts no new issue: Issues are turned
    /// off, or the repository is archived or disabled.
    IssuesDisabled,
    /// The repository accepts issues, and the scopes GitHub reports for this
    /// token hold none that may write one.
    CreateForbidden,
    /// The TLS check refused GitHub's certificate (AAP-FR-LRTC).
    TlsUntrusted(crate::tls::TlsCause),
}

/// An issue's parent, as GitHub reports it (GHP-FR-CMPR).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl ParentRef {
    /// Whether this parent is issue `number` of `owner/repo`; the repository
    /// match is case-insensitive.
    pub fn names(&self, owner: &str, repo: &str, number: u64) -> bool {
        self.number == number
            && self.owner.eq_ignore_ascii_case(owner)
            && self.repo.eq_ignore_ascii_case(repo)
    }
}

/// One issue, as much of it as publication needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueRef {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    /// GitHub's node-independent issue id, which the sub-issue relationship
    /// names (GHP-FR-OHGY).
    pub id: u64,
    pub open: bool,
    pub is_pull_request: bool,
    pub issue_type: Option<String>,
    pub milestone: Option<PublicationMilestone>,
    pub parent: Option<ParentRef>,
}

/// GHP-FR-OHGY: the metadata a create or an edit carries beyond the title and
/// the body. A `None` value is left out of the request, so it is left as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueFields {
    pub issue_type: Option<String>,
    pub milestone: Option<u64>,
}

/// The GitHub operations publication performs. Every method takes the secret
/// rather than holding it, so no implementation retains a credential.
pub trait GithubIssues: Send + Sync {
    /// A client for the API of `host` (normalized), or `None` where this client
    /// serves every host itself, as a test double does (GHP-FR-HSTA).
    fn for_host(&self, _host: &str) -> Option<std::sync::Arc<dyn GithubIssues>> {
        None
    }

    /// GHP-FR-MZPR: the read-only check. Mutates nothing.
    fn probe(&self, secret: &str, owner: &str, repo: &str) -> ProbeOutcome;

    /// GHP-FR-OWLB: the issue whose body carries `marker`, by exact match, or
    /// `None`. An error means the search could not answer, which is never read
    /// as "no issue exists" (GHP-FR-IEQC).
    fn find_by_marker(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        marker: &str,
    ) -> Result<Option<IssueRef>, String>;

    fn create_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        fields: &IssueFields,
    ) -> Result<IssueRef, String>;

    fn update_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        body: &str,
        fields: &IssueFields,
    ) -> Result<IssueRef, String>;

    /// GHP-FR-RMVQ / GHP-FR-PWJG: one issue, or `None` where it does not exist.
    /// An error means the read could not answer.
    fn get_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Option<IssueRef>, String>;

    /// GHP-FR-FTMC: the open issues whose Type is one of `types`, pull requests
    /// excluded. Fails with `parent_issues_unreadable`.
    fn list_parent_issues(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        types: &[String],
    ) -> Result<Vec<IssueRef>, String>;

    /// GHP-FR-YSPJ: the issue Types of the repository's owner. Fails with
    /// `issue_types_unreadable`.
    fn list_issue_types(&self, secret: &str, owner: &str) -> Result<Vec<String>, String>;

    /// GHP-FR-GHEA: the open milestones. Fails with `milestones_unreadable`.
    fn list_milestones(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<PublicationMilestone>, String>;

    /// GHP-FR-OHGY: make `sub_issue_id` a sub-issue of `parent_number`. With
    /// `replace_parent`, an existing parent is replaced (GHP-FR-RCNL).
    fn link_sub_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
        replace_parent: bool,
    ) -> Result<(), String>;

    /// GHP-FR-RCNL: remove `sub_issue_id` from `parent_number`.
    fn unlink_sub_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
    ) -> Result<(), String>;
}

/// Managed-state seam, so a test drives the whole flow against a fake.
pub struct GithubIssuesSeam(pub std::sync::Arc<dyn GithubIssues>);

impl Default for GithubIssuesSeam {
    fn default() -> Self {
        GithubIssuesSeam(std::sync::Arc::new(HttpGithubIssues { api: API.to_string() }))
    }
}

/// The API base of `github.com`, the base of the default client.
const API: &str = "https://api.github.com";
/// The whole request budget. The UI has no way to cancel an in-flight `invoke`,
/// so a hung network must not wedge the publish flow indefinitely.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The production client, bound to the REST API base of one host
/// (GTS-FR-PDWB, GHP-FR-HSTA).
pub struct HttpGithubIssues {
    api: String,
}

impl HttpGithubIssues {
    /// The client for the API of `host` (normalized).
    pub fn for_api_host(host: &str) -> Self {
        Self { api: crate::github_tokens::github_api_base(host) }
    }
}

fn agent() -> ureq::Agent {
    crate::tls::ureq_config().timeout_global(Some(TIMEOUT)).build().into()
}

/// Every request carries the same four headers. GitHub rejects a request with
/// no User-Agent outright, which would otherwise read as a bad token.
fn get(secret: &str, url: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    agent()
        .get(url)
        .header("Authorization", &format!("Bearer {secret}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "synthesis")
        .call()
}

fn send_json(
    secret: &str,
    method: &str,
    url: &str,
    payload: serde_json::Value,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let agent = agent();
    let headers = |request: ureq::RequestBuilder<ureq::typestate::WithBody>| {
        request
            .header("Authorization", &format!("Bearer {secret}"))
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", "application/json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "synthesis")
    };
    let request = match method {
        "PATCH" => headers(agent.patch(url)),
        // A DELETE carries a body here: the sub-issue relationship names the
        // issue to remove in it (GHP-FR-RCNL).
        "DELETE" => headers(agent.delete(url).force_send_body()),
        _ => headers(agent.post(url)),
    };
    // Serialized here rather than through `send_json`, which sits behind a
    // `ureq` feature this build does not enable.
    request.send(payload.to_string())
}

/// GHP-FR-DHXK: a transport error never reaches a caller verbatim.
///
/// `ureq`'s own message echoes the request it made, which for an authenticated
/// call is the one place an `Authorization` header or a credentialed URL could
/// surface. A fixed code cannot leak one however the transport phrases itself.
pub fn transport_code(err: &ureq::Error) -> &'static str {
    match err {
        // Which of the two the caller presents differently: a credential the
        // repository refused, and a repository the token cannot see at all.
        ureq::Error::StatusCode(401 | 403) => ERR_ISSUES_CREATE_FORBIDDEN,
        ureq::Error::StatusCode(404) => ERR_ISSUES_INACCESSIBLE,
        _ => ERR_GITHUB_UNREACHABLE,
    }
}

/// [`transport_code`], except that a refused certificate becomes the typed error
/// `tls_untrusted` with its host and cause (AAP-FR-LRTC).
fn transport_error(err: &ureq::Error, api: &str) -> String {
    crate::tls::ureq_wire(err, api).unwrap_or_else(|| transport_code(err).to_string())
}

/// The typed error `tls_untrusted` for a refused certificate, or `fallback`.
fn transport_or(err: &ureq::Error, api: &str, fallback: &str) -> String {
    crate::tls::ureq_wire(err, api).unwrap_or_else(|| fallback.to_string())
}

/// GHP-FR-RQDV: the scopes a response reports, or `None` where it reports none
/// at all.
///
/// A fine-grained token sends no header. A header that arrives present and
/// empty names no scope either, and the two are one case here: the requirement
/// turns on whether GitHub named a scope, and refusing a token because a header
/// arrived empty rather than absent is the misattribution this whole check
/// exists to avoid.
pub fn scopes_from_header(header: Option<&str>) -> Option<Vec<String>> {
    let scopes = crate::github_tokens::parse_scopes(header?);
    match scopes.is_empty() {
        true => None,
        false => Some(scopes),
    }
}

/// GHP-FR-TKBW / GHP-FR-RQDV: what a repository record, and the scopes GitHub
/// reports for the token that read it, say about creating an issue.
///
/// `scopes` is `None` where the response carried no scope header at all, which
/// is what a fine-grained token gives. Pure, so the decision the whole
/// eligibility answer rests on is testable without a network.
///
/// An absent field never makes a refusal here. The repository's own
/// `permissions` are deliberately not read: they describe **code-write**
/// access, and GHP-FR-RQDV requires only read access to a repository that
/// accepts issues.
pub fn issue_capability(
    repository: &serde_json::Value,
    scopes: Option<&[String]>,
) -> ProbeOutcome {
    // A record with no `has_issues` at all is one this application cannot
    // decide from. That is a statement about neither the repository nor the
    // token, so it answers as an unreadable repository.
    let Some(has_issues) = repository.get("has_issues").and_then(|v| v.as_bool()) else {
        return ProbeOutcome::IssuesUnreadable;
    };
    // GHP-FR-TKBW: an archived repository keeps `has_issues` true and still
    // takes no new issue, and so does one GitHub itself has disabled.
    let archived = repository.get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
    let disabled = repository.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if !has_issues || archived || disabled {
        return ProbeOutcome::IssuesDisabled;
    }
    // GHP-FR-RQDV: no scope header is a fine-grained token, which is not
    // inferred against.
    let Some(scopes) = scopes else {
        return ProbeOutcome::Publishable;
    };
    let private = repository.get("private").and_then(|v| v.as_bool()).unwrap_or(false);
    // Exact equality, never a prefix: `repo:status` and `repo_deployment` are
    // their own scopes and neither may write an issue.
    let may_write = scopes.iter().any(|s| s == "repo")
        || (!private && scopes.iter().any(|s| s == "public_repo"));
    match may_write {
        true => ProbeOutcome::Publishable,
        false => ProbeOutcome::CreateForbidden,
    }
}

/// GHP-FR-FQIZ: the marker is confirmed by **exact substring** against each
/// candidate's own body before one is accepted, whichever endpoint produced the
/// candidates — a full-text index answers approximately.
pub fn exact_match(items: &[serde_json::Value], marker: &str) -> Option<IssueRef> {
    items
        .iter()
        .filter_map(issue_from_json)
        .find(|issue| issue.body.contains(marker))
}

pub fn issue_from_json(value: &serde_json::Value) -> Option<IssueRef> {
    Some(IssueRef {
        number: value.get("number")?.as_u64()?,
        url: value.get("html_url")?.as_str()?.to_string(),
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        body: value.get("body").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        id: value.get("id").and_then(|v| v.as_u64()).unwrap_or_default(),
        open: value.get("state").and_then(|v| v.as_str()) == Some("open"),
        is_pull_request: value.get("pull_request").is_some_and(|v| !v.is_null()),
        issue_type: value
            .get("type")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        milestone: value.get("milestone").and_then(milestone_from_json),
        parent: value
            .get("parent_issue_url")
            .and_then(|v| v.as_str())
            .and_then(parent_from_url),
    })
}

/// GHP-FR-FTMC: one milestone, as much of it as the chooser renders.
pub fn milestone_from_json(value: &serde_json::Value) -> Option<PublicationMilestone> {
    Some(PublicationMilestone {
        number: value.get("number")?.as_u64()?,
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
    })
}

/// GHP-FR-CMPR: the parent an issue reports in `parent_issue_url`, which reads
/// `https://api.github.com/repos/<owner>/<repo>/issues/<number>`.
pub fn parent_from_url(url: &str) -> Option<ParentRef> {
    let (_, tail) = url.split_once("/repos/")?;
    let mut parts = tail.split('/');
    let owner = parts.next().filter(|p| !p.is_empty())?;
    let repo = parts.next().filter(|p| !p.is_empty())?;
    (parts.next()? == "issues").then_some(())?;
    let number = parts.next()?.parse::<u64>().ok()?;
    Some(ParentRef { owner: owner.to_string(), repo: repo.to_string(), number })
}

/// GHP-FR-YSPJ: the Type names an organization's `issue-types` answer holds.
/// A Type that is switched off is not offered.
pub fn issue_type_names(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("is_enabled").and_then(|v| v.as_bool()) != Some(false))
                .filter_map(|item| item.get("name").and_then(|v| v.as_str()))
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// GHP-FR-FTMC: a parent candidate is open, is not a pull request, and has a
/// Type in `types`, compared without regard to case. The request filters by
/// Type already; this check is what keeps a looser answer from reaching the
/// chooser.
pub fn is_parent_candidate(issue: &IssueRef, types: &[String]) -> bool {
    issue.open
        && !issue.is_pull_request
        && issue
            .issue_type
            .as_deref()
            .is_some_and(|t| types.iter().any(|wanted| wanted.eq_ignore_ascii_case(t)))
}

fn read_json(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<serde_json::Value, String> {
    let body = response.body_mut().read_to_string().map_err(|_| ERR_GITHUB_UNREACHABLE)?;
    serde_json::from_str(&body).map_err(|_| ERR_GITHUB_UNREACHABLE.to_string())
}

impl GithubIssues for HttpGithubIssues {
    fn for_host(&self, host: &str) -> Option<std::sync::Arc<dyn GithubIssues>> {
        Some(std::sync::Arc::new(HttpGithubIssues::for_api_host(host)))
    }

    fn probe(&self, secret: &str, owner: &str, repo: &str) -> ProbeOutcome {
        let api = self.api.as_str();
        // One read. The repository record carries every fact the decision rests
        // on — whether Issues are on, whether the repository is archived,
        // whether it is private — and the response itself carries what GitHub
        // granted this token. Listing the repository's issues would add
        // nothing: that endpoint also lists pull requests, so it answers for a
        // repository with Issues turned off just the same.
        let response = match get(secret, &format!("{api}/repos/{owner}/{repo}")) {
            Ok(response) => response,
            // AAP-FR-LRTC: a refused certificate is not an unreadable repository.
            Err(e) => {
                return match crate::tls::classify_ureq(&e) {
                    Some(cause) => ProbeOutcome::TlsUntrusted(cause),
                    None => ProbeOutcome::IssuesUnreadable,
                }
            }
        };
        // Taken before the body, which consumes the response. A classic token
        // reports its scopes on this header; a fine-grained token sends none.
        let scopes = scopes_from_header(
            response.headers().get("x-oauth-scopes").and_then(|value| value.to_str().ok()),
        );
        let Ok(value) = read_json(response) else {
            return ProbeOutcome::IssuesUnreadable;
        };
        issue_capability(&value, scopes.as_deref())
    }

    fn find_by_marker(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        marker: &str,
    ) -> Result<Option<IssueRef>, String> {
        let api = self.api.as_str();
        let query = format!("repo:{owner}/{repo} in:body \"{marker}\"");
        let url = format!("{api}/search/issues?per_page=20&q={}", urlencode(&query));
        let response = get(secret, &url).map_err(|e| transport_error(&e, api))?;
        let value = read_json(response)?;
        let items = value.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let found = match exact_match(&items, marker) {
            Some(issue) => Some(issue),
            // GitHub's search is an **asynchronous index**: an issue created a
            // moment ago is routinely absent from it for seconds or minutes.
            // That is exactly the window GHP-FR-OWLB exists for — a create
            // whose response was lost, retried at once — so an empty search is
            // confirmed against the repository's own most recent issues before
            // it is read as "no such issue". Without this the retry would post
            // a duplicate.
            None => {
                let recent = format!(
                    "{api}/repos/{owner}/{repo}/issues\
                     ?state=all&sort=created&direction=desc&per_page=50"
                );
                let response =
                    get(secret, &recent).map_err(|e| transport_error(&e, api))?;
                let value = read_json(response)?;
                let items = value.as_array().cloned().unwrap_or_default();
                exact_match(&items, marker)
            }
        };
        // GHP-FR-CMPR: the issue is read again on its own, because a search
        // answer need not carry the parent, Type, and milestone the comparison
        // rests on.
        match found {
            None => Ok(None),
            Some(candidate) => match self.get_issue(secret, owner, repo, candidate.number)? {
                Some(full) => Ok(Some(full)),
                None => Ok(Some(candidate)),
            },
        }
    }

    fn create_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        fields: &IssueFields,
    ) -> Result<IssueRef, String> {
        let api = self.api.as_str();
        let url = format!("{api}/repos/{owner}/{repo}/issues");
        let payload = issue_payload(title, body, fields);
        let response =
            send_json(secret, "POST", &url, payload).map_err(|e| transport_or(&e, api, ERR_ISSUE_CREATE_FAILED))?;
        let value = read_json(response).map_err(|_| ERR_ISSUE_CREATE_FAILED.to_string())?;
        issue_from_json(&value).ok_or_else(|| ERR_ISSUE_CREATE_FAILED.to_string())
    }

    fn update_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        body: &str,
        fields: &IssueFields,
    ) -> Result<IssueRef, String> {
        let api = self.api.as_str();
        let url = format!("{api}/repos/{owner}/{repo}/issues/{number}");
        let payload = issue_payload(title, body, fields);
        let response =
            send_json(secret, "PATCH", &url, payload).map_err(|e| transport_or(&e, api, ERR_ISSUE_UPDATE_FAILED))?;
        let value = read_json(response).map_err(|_| ERR_ISSUE_UPDATE_FAILED.to_string())?;
        issue_from_json(&value).ok_or_else(|| ERR_ISSUE_UPDATE_FAILED.to_string())
    }

    fn get_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Option<IssueRef>, String> {
        let api = self.api.as_str();
        let url = format!("{api}/repos/{owner}/{repo}/issues/{number}");
        match get(secret, &url) {
            Ok(response) => {
                let value = read_json(response)?;
                issue_from_json(&value)
                    .map(Some)
                    .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
            }
            Err(ureq::Error::StatusCode(404 | 410)) => Ok(None),
            Err(e) => Err(transport_error(&e, api)),
        }
    }

    fn list_parent_issues(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        types: &[String],
    ) -> Result<Vec<IssueRef>, String> {
        let api = self.api.as_str();
        let mut found: Vec<IssueRef> = Vec::new();
        for issue_type in types {
            for page in 1..=MAX_PAGES {
                let url = format!(
                    "{api}/repos/{owner}/{repo}/issues?state=open&type={}&per_page={PAGE_SIZE}&page={page}",
                    urlencode(issue_type)
                );
                let response = get(secret, &url).map_err(|e| transport_or(&e, api, ERR_PARENT_ISSUES_UNREADABLE))?;
                let value = read_json(response)
                    .map_err(|_| ERR_PARENT_ISSUES_UNREADABLE.to_string())?;
                let items = value.as_array().cloned().unwrap_or_default();
                let rows = items.len();
                for issue in items.iter().filter_map(issue_from_json) {
                    if is_parent_candidate(&issue, types)
                        && !found.iter().any(|known| known.number == issue.number)
                    {
                        found.push(issue);
                    }
                }
                if rows < PAGE_SIZE as usize {
                    break;
                }
            }
        }
        found.sort_by(|a, b| b.number.cmp(&a.number));
        Ok(found)
    }

    fn list_issue_types(&self, secret: &str, owner: &str) -> Result<Vec<String>, String> {
        let api = self.api.as_str();
        let url = format!("{api}/orgs/{owner}/issue-types");
        match get(secret, &url) {
            Ok(response) => {
                let value = read_json(response)
                    .map_err(|_| ERR_ISSUE_TYPES_UNREADABLE.to_string())?;
                Ok(issue_type_names(&value))
            }
            // GHP-FR-YSPJ: a user account has no organization to hold Types,
            // and GitHub answers 404 for it. That is an owner with no Type, not
            // a read that failed.
            Err(ureq::Error::StatusCode(404)) => Ok(Vec::new()),
            Err(e) => Err(transport_or(&e, api, ERR_ISSUE_TYPES_UNREADABLE)),
        }
    }

    fn list_milestones(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<PublicationMilestone>, String> {
        let api = self.api.as_str();
        let mut found = Vec::new();
        for page in 1..=MAX_PAGES {
            let url = format!(
                "{api}/repos/{owner}/{repo}/milestones?state=open&per_page={PAGE_SIZE}&page={page}"
            );
            let response =
                get(secret, &url).map_err(|e| transport_or(&e, api, ERR_MILESTONES_UNREADABLE))?;
            let value =
                read_json(response).map_err(|_| ERR_MILESTONES_UNREADABLE.to_string())?;
            let items = value.as_array().cloned().unwrap_or_default();
            let rows = items.len();
            found.extend(items.iter().filter_map(milestone_from_json));
            if rows < PAGE_SIZE as usize {
                break;
            }
        }
        Ok(found)
    }

    fn link_sub_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
        replace_parent: bool,
    ) -> Result<(), String> {
        let api = self.api.as_str();
        let url = format!("{api}/repos/{owner}/{repo}/issues/{parent_number}/sub_issues");
        let payload = serde_json::json!({
            "sub_issue_id": sub_issue_id,
            "replace_parent": replace_parent,
        });
        send_json(secret, "POST", &url, payload)
            .map(|_| ())
            .map_err(|e| transport_or(&e, api, ERR_SUB_ISSUE_LINK_FAILED))
    }

    fn unlink_sub_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
    ) -> Result<(), String> {
        let api = self.api.as_str();
        let url = format!("{api}/repos/{owner}/{repo}/issues/{parent_number}/sub_issue");
        let payload = serde_json::json!({ "sub_issue_id": sub_issue_id });
        send_json(secret, "DELETE", &url, payload)
            .map(|_| ())
            .map_err(|e| transport_or(&e, api, ERR_SUB_ISSUE_LINK_FAILED))
    }
}

/// GHP-FR-OHGY / GHP-FR-PLQE: the request body of a create or an edit. A value
/// that is not named is left out, so a root issue with no Type and no milestone
/// sends the title and the body alone.
pub fn issue_payload(title: &str, body: &str, fields: &IssueFields) -> serde_json::Value {
    let mut payload = serde_json::json!({ "title": title, "body": body });
    if let Some(issue_type) = &fields.issue_type {
        payload["type"] = serde_json::Value::String(issue_type.clone());
    }
    if let Some(milestone) = fields.milestone {
        payload["milestone"] = serde_json::Value::from(milestone);
    }
    payload
}

/// Percent-encode a search query. Small and local rather than a dependency:
/// the only thing that reaches it is a repository address and a generated
/// marker, both of which are ASCII by construction.
pub fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
