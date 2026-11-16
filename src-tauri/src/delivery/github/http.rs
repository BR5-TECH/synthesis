//! The `github` tracker adapter (`GDP-github-delivery-plan.md` GDP-FR-UPXR,
//! GDP-FR-ZGJB).
//!
//! Reads go through GraphQL, page by page, so a Project of several hundred
//! issues takes a number of requests that grows with the pages and not with
//! the issues (GDP non-functional requirements). Writes of issues, milestones,
//! sub-issues, and blocked-by relations go through REST; Project item writes go
//! through GraphQL. Every request goes through `crate::http`, the one HTTP
//! path of the application (AAP-FR-DTJA), to the host of the polling
//! repository (GTS-FR-PDWB).
//!
//! Every error is a fixed code. A transport error never reaches a caller as
//! text, because the client's message can echo the request it made. The codes
//! are those of `GPP-github-polling.md` GPP-FR-SUFH:
//!
//! - `github_unreachable`: no answer, or an answer with no readable body.
//! - a TLS code of AAP-FR-PKWE: the server's certificate was refused.
//! - `github_request_failed`: a 4xx or 5xx answer, an answer that is not the
//!   expected JSON, a GraphQL error other than `NOT_FOUND`, a page that names
//!   its own cursor again, and a Project with more items than
//!   `MAX_ITEM_PAGES` pages hold (GDP-FR-BGIT: a cut list never reads as
//!   whole).
//! - `project_unavailable`: the Project node is not found or not readable.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::github_polling::records::{
    ERR_GITHUB_UNREACHABLE, ERR_PROJECT_UNAVAILABLE, ERR_REQUEST_FAILED,
};

use super::adapter::*;

mod detail;

/// The whole request budget of one call.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The items of one Project page. Each item carries its relations, so a page
/// is smaller than the GraphQL maximum.
const ITEM_PAGE: usize = 50;
/// The most Project pages one read takes: 2 000 items.
const MAX_ITEM_PAGES: usize = 40;
/// The relations of one kind that one page reads for one issue.
const CONNECTION_SIZE: usize = 50;
const PAGE_INFO: &str = "pageInfo { hasNextPage endCursor }";
const LINK_FIELDS: &str = "number url title state repository { name owner { login } }";

/// The production connector.
pub struct HttpConnector;

impl TrackerConnector for HttpConnector {
    fn connect(&self, repository: &TrackerRepository, secret: &str) -> Arc<dyn TrackerAdapter> {
        Arc::new(GithubTracker::new(repository.clone(), secret.to_string()))
    }
}

/// The adapter of one GitHub repository, with the secret it sends.
pub struct GithubTracker {
    repository: TrackerRepository,
    secret: String,
    api: String,
    graphql: String,
    /// GitHub's numeric id of each issue, which the relation endpoints name.
    ids: Mutex<HashMap<u64, u64>>,
    /// GDP-FR-UPXR: in a test, the transport that records each request and
    /// gives its answer, so no request leaves the process.
    #[cfg(test)]
    transport: Option<Arc<Mutex<tests::Recorder>>>,
}

impl GithubTracker {
    pub fn new(repository: TrackerRepository, secret: String) -> Self {
        Self {
            api: crate::github_tokens::github_api_base(&repository.host),
            graphql: crate::github_tokens::github_graphql_url(&repository.host),
            repository,
            secret,
            ids: Mutex::new(HashMap::new()),
            #[cfg(test)]
            transport: None,
        }
    }

    /// The text of the answer to `request`. A body that is not UTF-8 is
    /// `HttpError::Body`.
    fn exchange(&self, request: crate::http::Request) -> Result<String, crate::http::HttpError> {
        #[cfg(test)]
        if let Some(transport) = &self.transport {
            return transport.lock().unwrap_or_else(|e| e.into_inner()).answer(request.test_view());
        }
        let response = request.send()?;
        response.text().map(str::to_string)
    }

    fn request(&self, method: &str, url: &str) -> crate::http::Request {
        let call = match method {
            "GET" => crate::http::Request::get(url, TIMEOUT),
            "DELETE" => crate::http::Request::delete(url, TIMEOUT),
            "PATCH" => crate::http::Request::patch(url, TIMEOUT),
            _ => crate::http::Request::post(url, TIMEOUT),
        };
        call.bearer(&self.secret)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "synthesis")
    }

    /// GPP-FR-SUFH: a fixed code for a failed request.
    fn fail(error: &crate::http::HttpError) -> String {
        match error {
            crate::http::HttpError::Status(_) => ERR_REQUEST_FAILED.to_string(),
            other => other.tls_wire().unwrap_or_else(|| ERR_GITHUB_UNREACHABLE.to_string()),
        }
    }

    /// GDP-FR-ZGJB: one GraphQL request to the GraphQL base of the host.
    fn graphql_request(&self, query: &str, variables: Value) -> crate::http::Request {
        let payload = json!({ "query": query, "variables": variables });
        self.request("POST", &self.graphql)
            .header("Content-Type", "application/json")
            .header("GraphQL-Features", "issue_types,sub_issues")
            .json(&payload)
    }

    /// The whole answer of one GraphQL request.
    fn graphql_raw(&self, query: &str, variables: Value) -> Result<Value, String> {
        let body = match self.exchange(self.graphql_request(query, variables)) {
            Ok(body) => body,
            Err(crate::http::HttpError::Body) => return Err(ERR_GITHUB_UNREACHABLE.to_string()),
            Err(e) => return Err(Self::fail(&e)),
        };
        serde_json::from_str(&body).map_err(|_| ERR_REQUEST_FAILED.to_string())
    }

    /// The `data` of one GraphQL request.
    fn graphql_data(&self, query: &str, variables: Value) -> Result<Value, String> {
        data_of(self.graphql_raw(query, variables)?)
    }

    /// The `repository` of a query whose variables name the bound repository.
    fn repo_query(&self, query: &str, mut variables: Value) -> Result<Value, String> {
        variables["owner"] = Value::String(self.repository.owner.clone());
        variables["name"] = Value::String(self.repository.name.clone());
        let data = self.graphql_data(query, variables)?;
        data.get("repository")
            .filter(|r| !r.is_null())
            .cloned()
            .ok_or_else(|| ERR_REQUEST_FAILED.into())
    }

    /// GDP-FR-ZGJB: one REST request to the REST base of the host. A `GET`,
    /// and a `DELETE` with no payload, send no body.
    fn rest_request(
        &self,
        method: &str,
        path: &str,
        payload: Option<Value>,
    ) -> crate::http::Request {
        let url = format!("{}{path}", self.api);
        let call = self
            .request(method, &url)
            .header("X-GitHub-Api-Version", crate::github_tokens::GITHUB_API_VERSION);
        match (method, payload) {
            ("GET", _) | ("DELETE", None) => call,
            (_, payload) => call
                .header("Content-Type", "application/json")
                .json(&payload.unwrap_or_else(|| json!({}))),
        }
    }

    /// One REST request. An answer with no body reads as `null`.
    fn rest(&self, method: &str, path: &str, payload: Option<Value>) -> Result<Value, String> {
        let body = match self.exchange(self.rest_request(method, path, payload)) {
            Ok(body) => body,
            Err(crate::http::HttpError::Body) => return Ok(Value::Null),
            Err(e) => return Err(Self::fail(&e)),
        };
        Ok(serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn repo_path(&self) -> String {
        format!("/repos/{}/{}", self.repository.owner, self.repository.name)
    }

    fn database_id(&self, number: u64) -> Result<u64, String> {
        if let Some(id) = self.ids.lock().ok().and_then(|ids| ids.get(&number).copied()) {
            return Ok(id);
        }
        let value = self.rest("GET", &format!("{}/issues/{number}", self.repo_path()), None)?;
        let id = value.get("id").and_then(Value::as_u64).ok_or(ERR_REQUEST_FAILED)?;
        if let Ok(mut ids) = self.ids.lock() {
            ids.insert(number, id);
        }
        Ok(id)
    }

    /// Every blocker of the issue `value`: the first page and each page after.
    fn all_blockers(&self, value: &Value) -> Result<Vec<IssueLink>, String> {
        let Some(first) = value.get("blockedBy").filter(|c| !c.is_null()) else {
            return Ok(Vec::new());
        };
        let node_id = str_of(value, "id");
        let query = format!(
            "query($id: ID!, $after: String!) {{ node(id: $id) {{ ... on Issue {{ \
             blockedBy(first: {CONNECTION_SIZE}, after: $after) {{ nodes {{ {LINK_FIELDS} }} {PAGE_INFO} }} }} }} }}"
        );
        let nodes = all_nodes(first, |after| {
            let data = self.graphql_data(&query, json!({ "id": node_id, "after": after }))?;
            data.get("node")
                .and_then(|n| n.get("blockedBy"))
                .filter(|c| !c.is_null())
                .cloned()
                .ok_or_else(|| ERR_REQUEST_FAILED.to_string())
        })?;
        Ok(nodes.iter().filter_map(link_of).collect())
    }

    /// GDP-FR-BGIT: the Project read, with `graphql` for the answer of one
    /// GraphQL request. It takes at most `MAX_ITEM_PAGES` pages.
    fn read_project_with(
        &self,
        project_id: &str,
        mut graphql: impl FnMut(&str, Value) -> Result<Value, String>,
    ) -> Result<ProjectRead, String> {
        let fields = issue_fields();
        let query = format!(
            "query($id: ID!, $cursor: String) {{ node(id: $id) {{ ... on ProjectV2 {{ \
             id title shortDescription number url owner {{ ... on User {{ login }} ... on Organization {{ login }} }} \
             field(name: \"Status\") {{ ... on ProjectV2SingleSelectField {{ id options {{ id name }} }} }} \
             items(first: {ITEM_PAGE}, after: $cursor) {{ {PAGE_INFO} nodes {{ id \
             fieldValueByName(name: \"Status\") {{ ... on ProjectV2ItemFieldSingleSelectValue {{ name }} }} \
             content {{ __typename ... on Issue {{ {fields} }} }} }} }} }} }} }}"
        );
        read_project_pages(
            MAX_ITEM_PAGES,
            |cursor| graphql(&query, json!({ "id": project_id, "cursor": cursor })),
            |raw| self.all_blockers(raw),
        )
    }

    /// One issue of an answer, with every page of its blockers.
    fn complete_issue(&self, value: &Value) -> Result<Option<TrackerIssue>, String> {
        let Some(mut issue) = issue_of(value) else { return Ok(None) };
        issue.blocked_by = self.all_blockers(value)?;
        Ok(Some(issue))
    }
}

/// The `data` of one GraphQL answer. A `NOT_FOUND` error leaves its field
/// `null`; every other error, and an answer with no `data`, fail the request.
/// Pure.
pub fn data_of(value: Value) -> Result<Value, String> {
    let errors = value.get("errors").and_then(Value::as_array).cloned().unwrap_or_default();
    if errors.iter().any(|e| e.get("type").and_then(Value::as_str) != Some("NOT_FOUND")) {
        return Err(ERR_REQUEST_FAILED.to_string());
    }
    value.get("data").filter(|d| !d.is_null()).cloned().ok_or_else(|| ERR_REQUEST_FAILED.into())
}

fn issue_fields() -> String {
    format!(
        "id number url state stateReason title body updatedAt \
         repository {{ name owner {{ login }} }} issueType {{ name }} milestone {{ number }} \
         parent {{ {LINK_FIELDS} }} \
         blockedBy(first: {CONNECTION_SIZE}) {{ nodes {{ {LINK_FIELDS} }} {PAGE_INFO} }}"
    )
}

const MILESTONE_FIELDS: &str = "number url title description dueOn state updatedAt";

fn str_of(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The cursor of the page after `connection`, or `None` on the last page. A
/// next page with no cursor is an error, so a cut list never reads as whole.
pub fn next_cursor(connection: &Value) -> Result<Option<String>, String> {
    let info = connection.get("pageInfo");
    match info.and_then(|i| i.get("hasNextPage")).and_then(Value::as_bool) {
        Some(false) | None => Ok(None),
        Some(true) => info
            .and_then(|i| i.get("endCursor"))
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(|c| Some(c.to_string()))
            .ok_or_else(|| ERR_REQUEST_FAILED.to_string()),
    }
}

/// Every node of a connection, page by page. A page that gives the same cursor
/// again is an error, so the read always ends.
pub fn all_nodes(
    first: &Value,
    mut next_page: impl FnMut(&str) -> Result<Value, String>,
) -> Result<Vec<Value>, String> {
    let nodes_of =
        |c: &Value| c.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = nodes_of(first);
    let mut cursor = next_cursor(first)?;
    while let Some(after) = cursor {
        let page = next_page(&after)?;
        out.extend(nodes_of(&page));
        cursor = next_cursor(&page)?;
        if cursor.as_deref() == Some(after.as_str()) {
            return Err(ERR_REQUEST_FAILED.to_string());
        }
    }
    Ok(out)
}

fn link_of(value: &Value) -> Option<IssueLink> {
    let repository = value.get("repository")?;
    Some(IssueLink {
        owner: repository.get("owner").map(|o| str_of(o, "login")).unwrap_or_default(),
        repo: str_of(repository, "name"),
        number: value.get("number")?.as_u64()?,
        url: str_of(value, "url"),
        title: str_of(value, "title"),
        state: str_of(value, "state").to_ascii_lowercase(),
    })
}

fn reason_of(text: &str) -> Option<StateReason> {
    Some(match text.to_ascii_uppercase().as_str() {
        "COMPLETED" => StateReason::Completed,
        "NOT_PLANNED" => StateReason::NotPlanned,
        "DUPLICATE" => StateReason::Duplicate,
        "REOPENED" => StateReason::Reopened,
        _ => return None,
    })
}

/// One issue of a GraphQL answer, with the first page of its blockers.
pub fn issue_of(value: &Value) -> Option<TrackerIssue> {
    if value.is_null() || value.get("number").is_none() {
        return None;
    }
    let link = link_of(value)?;
    Some(TrackerIssue {
        number: link.number,
        url: link.url,
        node_id: str_of(value, "id"),
        owner: link.owner,
        repo: link.repo,
        title: link.title,
        body: str_of(value, "body"),
        state: link.state,
        state_reason: value.get("stateReason").and_then(Value::as_str).and_then(reason_of),
        issue_type: value
            .get("issueType")
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        milestone: value.get("milestone").and_then(|m| m.get("number")).and_then(Value::as_u64),
        parent: value.get("parent").filter(|p| !p.is_null()).and_then(link_of),
        blocked_by: value
            .get("blockedBy")
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| nodes.iter().filter_map(link_of).collect())
            .unwrap_or_default(),
        updated_at: str_of(value, "updatedAt"),
    })
}

/// One milestone of a GraphQL or REST answer. The due date keeps its
/// calendar date.
pub fn milestone_of(value: &Value) -> Option<TrackerMilestone> {
    if value.is_null() {
        return None;
    }
    let due = ["dueOn", "due_on"].iter().find_map(|k| value.get(*k).and_then(Value::as_str));
    let url = ["url", "html_url"].iter().map(|k| str_of(value, k)).find(|u| !u.is_empty());
    let updated =
        ["updatedAt", "updated_at"].iter().map(|k| str_of(value, k)).find(|u| !u.is_empty());
    Some(TrackerMilestone {
        number: value.get("number")?.as_u64()?,
        url: url.unwrap_or_default(),
        title: str_of(value, "title"),
        description: str_of(value, "description"),
        state: str_of(value, "state").to_ascii_lowercase(),
        due: due.and_then(|d| d.get(..10)).map(str::to_string),
        updated_at: updated.unwrap_or_default(),
    })
}

/// One page of a Project read: the Project with no items, the items of the
/// page with their raw issue values, and the cursor of the next page.
pub struct ProjectPage {
    pub project: ProjectRead,
    pub items: Vec<(ProjectItemRead, Value)>,
    pub next: Option<String>,
}

/// The Project fields of a `node` answer, with the items of one page.
pub fn project_of(response: &Value) -> Result<ProjectPage, String> {
    let node = response.get("data").and_then(|d| d.get("node")).filter(|n| !n.is_null());
    let Some(node) = node.filter(|n| n.get("title").is_some()) else {
        let types = crate::github_polling::client::error_types(response);
        let unavailable =
            types.iter().all(|t| t == "NOT_FOUND" || t == "FORBIDDEN" || t.is_empty());
        return Err(if unavailable { ERR_PROJECT_UNAVAILABLE } else { ERR_REQUEST_FAILED }.into());
    };
    let status_field = detail::status_field_of(node);
    let (items, next) = items_of(node.get("items"))?;
    Ok(ProjectPage {
        project: ProjectRead {
            node_id: str_of(node, "id"),
            title: str_of(node, "title"),
            short_description: str_of(node, "shortDescription"),
            owner_login: node.get("owner").map(|o| str_of(o, "login")).unwrap_or_default(),
            number: node.get("number").and_then(Value::as_u64).unwrap_or_default(),
            url: str_of(node, "url"),
            status_field,
            items: Vec::new(),
        },
        items,
        next,
    })
}

/// Project items with the raw issue value of each.
type RawItems = Vec<(ProjectItemRead, Value)>;

/// The items of one page of a Project's `items` connection, with the raw
/// issue values, and the cursor of the next page.
fn items_of(connection: Option<&Value>) -> Result<(RawItems, Option<String>), String> {
    let Some(connection) = connection.filter(|c| !c.is_null()) else {
        return Err(ERR_REQUEST_FAILED.to_string());
    };
    let nodes = connection.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for node in nodes {
        let content = node.get("content").cloned().unwrap_or(Value::Null);
        let kind = content.get("__typename").and_then(Value::as_str).unwrap_or_default();
        let parsed = match kind {
            "Issue" => issue_of(&content).map(ItemContent::Issue).unwrap_or(ItemContent::Draft),
            "PullRequest" => ItemContent::PullRequest,
            _ => ItemContent::Draft,
        };
        out.push((
            ProjectItemRead {
                item_id: str_of(&node, "id"),
                status: node
                    .get("fieldValueByName")
                    .and_then(|v| v.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: parsed,
            },
            content,
        ));
    }
    Ok((out, next_cursor(connection)?))
}

/// GDP-FR-BGIT: every item of a Project, page by page. `fetch_page` gives the
/// answer of the page after a cursor (`None` for the first page), and
/// `blockers` gives every blocker of one raw issue value.
///
/// A Project that has pages after `max_pages` is an error
/// (`github_request_failed`), so a cut item list never reads as whole and is
/// never stored as the read cache. The load then follows GDP-FR-GCJO: it
/// answers from the cache, marked stale, or it refuses. Pure over its inputs.
pub fn read_project_pages(
    max_pages: usize,
    mut fetch_page: impl FnMut(Option<&str>) -> Result<Value, String>,
    mut blockers: impl FnMut(&Value) -> Result<Vec<IssueLink>, String>,
) -> Result<ProjectRead, String> {
    let mut project: Option<ProjectRead> = None;
    let mut cursor: Option<String> = None;
    for _ in 0..max_pages {
        let response = fetch_page(cursor.as_deref())?;
        let page = project_of(&response)?;
        let mut items = Vec::new();
        for (mut item, raw) in page.items {
            if let ItemContent::Issue(issue) = &mut item.content {
                issue.blocked_by = blockers(&raw)?;
            }
            items.push(item);
        }
        match project.as_mut() {
            Some(p) => p.items.extend(items),
            None => project = Some(ProjectRead { items, ..page.project }),
        }
        match page.next {
            // A page that names its own cursor again would never end.
            Some(next) if cursor.as_deref() == Some(next.as_str()) => {
                return Err(ERR_REQUEST_FAILED.to_string());
            }
            Some(next) => cursor = Some(next),
            None => return project.ok_or_else(|| ERR_REQUEST_FAILED.to_string()),
        }
    }
    // Pages remain after the limit: the list is cut.
    Err(ERR_REQUEST_FAILED.to_string())
}

impl TrackerAdapter for GithubTracker {
    fn repository(&self) -> &TrackerRepository {
        &self.repository
    }

    fn read_project(&self, project_id: &str) -> Result<ProjectRead, String> {
        self.read_project_with(project_id, |query, variables| self.graphql_raw(query, variables))
    }

    fn read_milestones(&self) -> Result<Vec<TrackerMilestone>, String> {
        let query = format!(
            "query($owner: String!, $name: String!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ \
             milestones(first: 100, after: $cursor, states: [OPEN, CLOSED]) {{ nodes {{ {MILESTONE_FIELDS} }} {PAGE_INFO} }} }} }}"
        );
        let first = self.repo_query(&query, json!({ "cursor": Value::Null }))?;
        let connection = first.get("milestones").cloned().unwrap_or(Value::Null);
        let nodes = all_nodes(&connection, |after| {
            let page = self.repo_query(&query, json!({ "cursor": after }))?;
            Ok(page.get("milestones").cloned().unwrap_or(Value::Null))
        })?;
        Ok(nodes.iter().filter_map(milestone_of).collect())
    }

    fn read_issue(&self, number: u64) -> Result<Option<TrackerIssue>, String> {
        let fields = issue_fields();
        let query = format!(
            "query($owner: String!, $name: String!, $number: Int!) {{ repository(owner: $owner, name: $name) {{ \
             issueOrPullRequest(number: $number) {{ __typename ... on Issue {{ {fields} }} }} }} }}"
        );
        let repository = self.repo_query(&query, json!({ "number": number }))?;
        let value = repository.get("issueOrPullRequest").cloned().unwrap_or(Value::Null);
        if value.get("__typename").and_then(Value::as_str) != Some("Issue") {
            return Ok(None);
        }
        self.complete_issue(&value)
    }

    fn read_milestone(&self, number: u64) -> Result<Option<TrackerMilestone>, String> {
        let query = format!(
            "query($owner: String!, $name: String!, $number: Int!) {{ repository(owner: $owner, name: $name) {{ \
             milestone(number: $number) {{ {MILESTONE_FIELDS} }} }} }}"
        );
        let repository = self.repo_query(&query, json!({ "number": number }))?;
        Ok(repository.get("milestone").and_then(milestone_of))
    }

    fn list_issue_types(&self) -> Result<Vec<String>, String> {
        let query =
            "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { \
                     issueTypes(first: 100) { nodes { name isEnabled } } } }";
        let repository = self.repo_query(query, json!({}))?;
        Ok(repository
            .get("issueTypes")
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter(|n| n.get("isEnabled").and_then(Value::as_bool) != Some(false))
                    .map(|n| str_of(n, "name"))
                    .filter(|name| !name.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default())
    }

    fn create_issue(&self, issue: &NewIssue) -> Result<TrackerIssue, String> {
        let mut payload = json!({ "title": issue.title, "body": issue.body });
        if let Some(t) = &issue.issue_type {
            payload["type"] = Value::String(t.clone());
        }
        if let Some(m) = issue.milestone {
            payload["milestone"] = Value::from(m);
        }
        if !issue.labels.is_empty() {
            payload["labels"] = json!(issue.labels);
        }
        let value = self.rest("POST", &format!("{}/issues", self.repo_path()), Some(payload))?;
        let number = value.get("number").and_then(Value::as_u64).ok_or(ERR_REQUEST_FAILED)?;
        if let (Some(id), Ok(mut ids)) = (value.get("id").and_then(Value::as_u64), self.ids.lock())
        {
            ids.insert(number, id);
        }
        Ok(TrackerIssue {
            number,
            url: str_of(&value, "html_url"),
            node_id: str_of(&value, "node_id"),
            owner: self.repository.owner.clone(),
            repo: self.repository.name.clone(),
            title: str_of(&value, "title"),
            body: str_of(&value, "body"),
            state: str_of(&value, "state").to_ascii_lowercase(),
            state_reason: None,
            issue_type: value
                .get("type")
                .and_then(|t| t.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            milestone: value.get("milestone").and_then(|m| m.get("number")).and_then(Value::as_u64),
            parent: None,
            blocked_by: Vec::new(),
            updated_at: str_of(&value, "updated_at"),
        })
    }

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String> {
        let mut payload = json!({});
        if let Some(title) = &change.title {
            payload["title"] = Value::String(title.clone());
        }
        if let Some(body) = &change.body {
            payload["body"] = Value::String(body.clone());
        }
        if let Some(t) = &change.issue_type {
            payload["type"] = Value::String(t.clone());
        }
        self.rest("PATCH", &format!("{}/issues/{number}", self.repo_path()), Some(payload))
            .map(|_| ())
    }

    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueLink>,
        parent: Option<u64>,
    ) -> Result<(), String> {
        let id = self.database_id(number)?;
        match (parent, current) {
            (Some(parent), _) => self
                .rest(
                    "POST",
                    &format!("{}/issues/{parent}/sub_issues", self.repo_path()),
                    Some(json!({ "sub_issue_id": id, "replace_parent": true })),
                )
                .map(|_| ()),
            (None, Some(current)) => self
                .rest(
                    "DELETE",
                    &format!(
                        "/repos/{}/{}/issues/{}/sub_issue",
                        current.owner, current.repo, current.number
                    ),
                    Some(json!({ "sub_issue_id": id })),
                )
                .map(|_| ()),
            (None, None) => Ok(()),
        }
    }

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String> {
        let payload = json!({ "milestone": milestone });
        self.rest("PATCH", &format!("{}/issues/{number}", self.repo_path()), Some(payload))
            .map(|_| ())
    }

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        let id = self.database_id(blocker)?;
        self.rest(
            "POST",
            &format!("{}/issues/{number}/dependencies/blocked_by", self.repo_path()),
            Some(json!({ "issue_id": id })),
        )
        .map(|_| ())
    }

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        let id = self.database_id(blocker)?;
        self.rest(
            "DELETE",
            &format!("{}/issues/{number}/dependencies/blocked_by/{id}", self.repo_path()),
            None,
        )
        .map(|_| ())
    }

    fn add_project_item(&self, project_id: &str, issue_node_id: &str) -> Result<String, String> {
        let query = "mutation($project: ID!, $content: ID!) { addProjectV2ItemById(input: \
                     { projectId: $project, contentId: $content }) { item { id } } }";
        let data =
            self.graphql_data(query, json!({ "project": project_id, "content": issue_node_id }))?;
        data.get("addProjectV2ItemById")
            .and_then(|a| a.get("item"))
            .and_then(|i| i.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ERR_REQUEST_FAILED.to_string())
    }

    fn remove_project_item(&self, project_id: &str, item_id: &str) -> Result<(), String> {
        let query = "mutation($project: ID!, $item: ID!) { deleteProjectV2Item(input: \
                     { projectId: $project, itemId: $item }) { deletedItemId } }";
        self.graphql_data(query, json!({ "project": project_id, "item": item_id })).map(|_| ())
    }

    fn create_milestone(&self, milestone: &NewMilestone) -> Result<TrackerMilestone, String> {
        let mut payload = json!({ "title": milestone.title, "description": milestone.description });
        if let Some(due) = &milestone.due {
            payload["due_on"] = due_on(Some(due));
        }
        let value =
            self.rest("POST", &format!("{}/milestones", self.repo_path()), Some(payload))?;
        milestone_of(&value).ok_or_else(|| ERR_REQUEST_FAILED.to_string())
    }

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String> {
        let mut payload = json!({});
        if let Some(title) = &change.title {
            payload["title"] = Value::String(title.clone());
        }
        if let Some(description) = &change.description {
            payload["description"] = Value::String(description.clone());
        }
        if let Some(due) = &change.due {
            payload["due_on"] = due_on(due.as_deref());
        }
        self.rest("PATCH", &format!("{}/milestones/{number}", self.repo_path()), Some(payload))
            .map(|_| ())
    }

    fn close_milestone(&self, number: u64) -> Result<(), String> {
        self.rest(
            "PATCH",
            &format!("{}/milestones/{number}", self.repo_path()),
            Some(json!({ "state": "closed" })),
        )
        .map(|_| ())
    }

    fn close_issue(&self, number: u64, reason: CloseWith) -> Result<(), String> {
        let reason = match reason {
            CloseWith::Completed => "completed",
            CloseWith::NotPlanned => "not_planned",
        };
        let payload = json!({ "state": "closed", "state_reason": reason });
        self.rest("PATCH", &format!("{}/issues/{number}", self.repo_path()), Some(payload))
            .map(|_| ())
    }

    fn list_labels(&self) -> Result<Vec<String>, String> {
        self.labels()
    }

    fn read_status_field(&self, project_id: &str) -> Result<Option<StatusFieldRead>, String> {
        self.status_field(project_id)
    }

    fn read_issue_detail(
        &self,
        number: u64,
        project_id: &str,
    ) -> Result<Option<IssueDetail>, String> {
        self.issue_detail(number, project_id)
    }

    fn add_labels(&self, number: u64, labels: &[String]) -> Result<(), String> {
        self.labels_added(number, labels)
    }

    fn remove_label(&self, number: u64, label: &str) -> Result<(), String> {
        self.label_removed(number, label)
    }

    fn set_project_status(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String> {
        self.status_set(project_id, item_id, field_id, option_id)
    }
}

/// The `due_on` value REST takes for a calendar date. Noon UTC keeps the same
/// date in every time zone GitHub shows it in.
fn due_on(due: Option<&str>) -> Value {
    due.map_or(Value::Null, |d| Value::String(format!("{d}T12:00:00Z")))
}

#[cfg(test)]
mod tests;
