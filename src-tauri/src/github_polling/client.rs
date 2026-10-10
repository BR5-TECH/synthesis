//! The GitHub Projects client (GPP-FR-WOIM, GPP-FR-WYRP, GPP-FR-DEZO,
//! GPP-FR-XSKT, GPP-FR-IFVC, GPP-FR-ILGG).
//!
//! Direct HTTPS against GitHub's GraphQL API through `ureq`, the client the
//! rest of the application already uses. The trait is what makes the polling
//! and claim flows testable without a network: the production implementation
//! is the only thing in this module that opens a socket. Every method takes the
//! secret rather than holding it, so no implementation retains a credential.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::{json, Value};

use super::records::*;

/// One option of the Project's `Status` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusOption {
    pub id: String,
    pub name: String,
}

/// The Project's single-select `Status` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusField {
    pub field_id: String,
    pub options: Vec<StatusOption>,
}

/// GPP-FR-DEZO: what the Project configuration read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectShape {
    pub title: String,
    /// `None` where the Project holds no single-select field named `Status`.
    pub status_field: Option<StatusField>,
}

/// The issue one Project item holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemIssue {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// GitHub's issue state, `OPEN` or `CLOSED`.
    pub state: String,
    pub issue_type: Option<String>,
    pub repository_owner: String,
    pub repository_name: String,
}

/// GPP-FR-XSKT: one item of the selected Project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectItem {
    pub item_id: String,
    /// The item's `Status` value name, or `None` where it has none.
    pub status: Option<String>,
    /// `None` for a draft item or a pull request.
    pub issue: Option<ItemIssue>,
}

/// One Project item of a re-fetched issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueProjectItem {
    pub item_id: String,
    pub project_id: String,
    pub status: Option<String>,
}

/// GPP-FR-IFVC: an issue, re-fetched immediately before a claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchedIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub state: String,
    pub issue_type: Option<String>,
    pub repository_owner: String,
    pub repository_name: String,
    pub project_items: Vec<IssueProjectItem>,
}

/// A list read page by page, and whether the page cap stopped the read
/// before the last page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub truncated: bool,
}

/// Read pages until the last one or until `max_pages`. `fetch` gets the
/// cursor of the page to read (`None` for the first) and returns the page and
/// the cursor of the next page, if one exists.
pub fn paginate<T>(
    max_pages: usize,
    mut fetch: impl FnMut(Option<String>) -> Result<(Vec<T>, Option<String>), String>,
) -> Result<Paged<T>, String> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..max_pages {
        let (page, next) = fetch(cursor.take())?;
        items.extend(page);
        match next {
            Some(next) => cursor = Some(next),
            None => return Ok(Paged { items, truncated: false }),
        }
    }
    Ok(Paged { items, truncated: true })
}

/// The GitHub operations polling performs.
pub trait GithubProjects: Send + Sync {
    /// A client for the GraphQL API of `host` (normalized), or `None` where this
    /// client serves every host itself, as a test double does (GPP-FR-HSTD).
    fn for_host(&self, _host: &str) -> Option<std::sync::Arc<dyn GithubProjects>> {
        None
    }

    /// GPP-FR-WYRP: the viewer's own Projects. Changes nothing on GitHub.
    fn viewer_projects(&self, secret: &str) -> Result<Paged<GithubProjectOption>, String>;

    /// GPP-FR-WYRP: the Projects of one repository owner. Changes nothing on
    /// GitHub.
    fn owner_projects(&self, secret: &str, owner: &str)
        -> Result<Paged<GithubProjectOption>, String>;

    /// GPP-FR-DEZO: the Project's title and `Status` field. A Project that
    /// does not resolve, or that the token cannot read, is
    /// `project_unavailable`.
    fn project_shape(&self, secret: &str, project_id: &str) -> Result<ProjectShape, String>;

    /// GPP-FR-XSKT: every item of the Project, within a bounded page count.
    fn project_items(&self, secret: &str, project_id: &str) -> Result<Paged<ProjectItem>, String>;

    /// GPP-FR-IFVC: the issue, or `None` where the repository holds no such
    /// issue.
    fn fetch_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Option<FetchedIssue>, String>;

    /// GPP-FR-ILGG: set one item's `Status` to one option.
    fn set_item_status(
        &self,
        secret: &str,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String>;
}

/// Managed-state seam, so a test drives the flows against a fake.
pub struct GithubProjectsSeam(pub std::sync::Arc<dyn GithubProjects>);

impl Default for GithubProjectsSeam {
    fn default() -> Self {
        GithubProjectsSeam(std::sync::Arc::new(HttpGithubProjects { url: GRAPHQL.to_string() }))
    }
}

/// The GraphQL URL of `github.com`, the URL of the default client.
const GRAPHQL: &str = "https://api.github.com/graphql";
/// The whole request budget of one call (GPP non-functional requirements).
const TIMEOUT: Duration = Duration::from_secs(20);
/// The most item pages one poll reads, 100 items each.
pub const MAX_ITEM_PAGES: usize = 20;
/// The most Project pages one listing reads per owner, 100 Projects each.
pub const MAX_PROJECT_PAGES: usize = 10;

/// The production client, bound to the GraphQL URL of one host
/// (GTS-FR-PDWB, GPP-FR-HSTD).
pub struct HttpGithubProjects {
    url: String,
}

impl HttpGithubProjects {
    /// The client for the GraphQL API of `host` (normalized).
    pub fn for_graphql_host(host: &str) -> Self {
        Self { url: crate::github_tokens::github_graphql_url(host) }
    }
}

fn agent() -> ureq::Agent {
    crate::tls::ureq_config().timeout_global(Some(TIMEOUT)).build().into()
}

/// One GraphQL request. GPP-FR-WKZF: a transport error never reaches a caller
/// verbatim, because `ureq`'s own message can echo the request it made.
fn graphql(url: &str, secret: &str, query: &str, variables: Value) -> Result<Value, String> {
    let payload = json!({ "query": query, "variables": variables });
    let response = agent()
        .post(url)
        .header("Authorization", &format!("Bearer {secret}"))
        .header("Accept", "application/vnd.github+json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "synthesis")
        // GPP-FR-BOQX: the issue type field sits behind this feature flag on
        // some GitHub deployments; it is harmless where it is not needed.
        .header("GraphQL-Features", "issue_types")
        .send(payload.to_string());
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(_)) => return Err(ERR_REQUEST_FAILED.to_string()),
        // AAP-FR-LRTC: a refused certificate is its own typed error.
        Err(e) => {
            return Err(crate::tls::ureq_wire(&e, url)
                .unwrap_or_else(|| ERR_GITHUB_UNREACHABLE.to_string()))
        }
    };
    let body = response.body_mut().read_to_string().map_err(|_| ERR_GITHUB_UNREACHABLE)?;
    serde_json::from_str(&body).map_err(|_| ERR_REQUEST_FAILED.to_string())
}

/// The `type` of every GraphQL error a response carries.
pub fn error_types(response: &Value) -> Vec<String> {
    response
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .map(|e| e.get("type").and_then(Value::as_str).unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn has_errors(response: &Value) -> bool {
    response.get("errors").and_then(Value::as_array).is_some_and(|e| !e.is_empty())
}

fn str_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

const PROJECT_FIELDS: &str = "pageInfo { hasNextPage endCursor } nodes { id title number owner { ... on User { login } ... on Organization { login } } }";

/// The cursor of the next page a connection names, if it has one.
fn next_cursor(connection: Option<&Value>) -> Option<String> {
    let page = connection?.get("pageInfo")?;
    match page.get("hasNextPage").and_then(Value::as_bool) {
        Some(true) => str_of(page, "endCursor"),
        _ => None,
    }
}

/// GPP-FR-WYRP: the Projects a `projectsV2` connection holds.
pub fn projects_of(connection: Option<&Value>) -> Vec<GithubProjectOption> {
    let Some(nodes) = connection.and_then(|c| c.get("nodes")).and_then(Value::as_array) else {
        return Vec::new();
    };
    nodes
        .iter()
        .filter_map(|node| {
            Some(GithubProjectOption {
                node_id: str_of(node, "id")?,
                title: str_of(node, "title").unwrap_or_default(),
                owner_login: node
                    .get("owner")
                    .and_then(|o| str_of(o, "login"))
                    .unwrap_or_default(),
                number: node.get("number").and_then(Value::as_u64).unwrap_or_default(),
            })
        })
        .collect()
}

/// GPP-FR-WYRP: one list without duplicates, first occurrence kept.
pub fn dedupe_projects(projects: Vec<GithubProjectOption>) -> Vec<GithubProjectOption> {
    let mut seen = HashSet::new();
    projects.into_iter().filter(|p| seen.insert(p.node_id.clone())).collect()
}

/// GPP-FR-DEZO: the Project shape a `node` query answered with.
pub fn shape_of(response: &Value) -> Result<ProjectShape, String> {
    let node = response.get("data").and_then(|d| d.get("node"));
    let title = node.and_then(|n| str_of(n, "title"));
    let Some(title) = title else {
        // A missing node, a node that is not a Project, and a Project the
        // token cannot read all answer the same way. Any other GraphQL
        // failure is the request's, not the Project's.
        let types = error_types(response);
        let unavailable = types
            .iter()
            .all(|t| t == "NOT_FOUND" || t == "FORBIDDEN" || t.is_empty());
        return Err(match unavailable {
            true => ERR_PROJECT_UNAVAILABLE,
            false => ERR_REQUEST_FAILED,
        }
        .to_string());
    };
    let status_field = node.and_then(|n| n.get("field")).and_then(|field| {
        let field_id = str_of(field, "id")?;
        let options = field
            .get("options")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(|o| Some(StatusOption { id: str_of(o, "id")?, name: str_of(o, "name")? }))
            .collect();
        Some(StatusField { field_id, options })
    });
    Ok(ProjectShape { title, status_field })
}

/// GPP-FR-XSKT: the items of one page, and the cursor of the next page.
pub fn items_page_of(response: &Value) -> Result<(Vec<ProjectItem>, Option<String>), String> {
    let Some(items) = response
        .get("data")
        .and_then(|d| d.get("node"))
        .and_then(|n| n.get("items"))
    else {
        return Err(match error_types(response).iter().all(|t| t == "NOT_FOUND" || t == "FORBIDDEN")
        {
            true if has_errors(response) => ERR_PROJECT_UNAVAILABLE,
            _ => ERR_REQUEST_FAILED,
        }
        .to_string());
    };
    let nodes = items.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
    let parsed = nodes
        .iter()
        .filter_map(|node| {
            Some(ProjectItem {
                item_id: str_of(node, "id")?,
                status: node.get("fieldValueByName").and_then(|v| str_of(v, "name")),
                issue: node.get("content").and_then(item_issue_of),
            })
        })
        .collect();
    let page = items.get("pageInfo");
    let next = match page.and_then(|p| p.get("hasNextPage")).and_then(Value::as_bool) {
        Some(true) => page.and_then(|p| str_of(p, "endCursor")),
        _ => None,
    };
    Ok((parsed, next))
}

fn item_issue_of(content: &Value) -> Option<ItemIssue> {
    let repository = content.get("repository")?;
    Some(ItemIssue {
        number: content.get("number").and_then(Value::as_u64)?,
        title: str_of(content, "title").unwrap_or_default(),
        url: str_of(content, "url")?,
        state: str_of(content, "state").unwrap_or_default(),
        issue_type: content.get("issueType").and_then(|t| str_of(t, "name")),
        repository_owner: repository.get("owner").and_then(|o| str_of(o, "login"))?,
        repository_name: str_of(repository, "name")?,
    })
}

/// GPP-FR-IFVC: the issue a re-fetch answered with.
pub fn fetched_issue_of(response: &Value) -> Result<Option<FetchedIssue>, String> {
    let Some(repository) = response
        .get("data")
        .and_then(|d| d.get("repository"))
        .filter(|r| !r.is_null())
    else {
        return Err(ERR_REQUEST_FAILED.to_string());
    };
    let Some(issue) = repository.get("issue").filter(|i| !i.is_null()) else {
        return Ok(None);
    };
    let owner = repository.get("owner").and_then(|o| str_of(o, "login")).unwrap_or_default();
    let project_items = issue
        .get("projectItems")
        .and_then(|p| p.get("nodes"))
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| {
                    Some(IssueProjectItem {
                        item_id: str_of(node, "id")?,
                        project_id: node.get("project").and_then(|p| str_of(p, "id"))?,
                        status: node.get("fieldValueByName").and_then(|v| str_of(v, "name")),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(FetchedIssue {
        number: issue.get("number").and_then(Value::as_u64).ok_or(ERR_REQUEST_FAILED)?,
        title: str_of(issue, "title").unwrap_or_default(),
        body: str_of(issue, "body").unwrap_or_default(),
        url: str_of(issue, "url").ok_or(ERR_REQUEST_FAILED)?,
        state: str_of(issue, "state").unwrap_or_default(),
        issue_type: issue.get("issueType").and_then(|t| str_of(t, "name")),
        repository_owner: owner,
        repository_name: str_of(repository, "name").unwrap_or_default(),
        project_items,
    }))
}

impl GithubProjects for HttpGithubProjects {
    fn for_host(&self, host: &str) -> Option<std::sync::Arc<dyn GithubProjects>> {
        Some(std::sync::Arc::new(HttpGithubProjects::for_graphql_host(host)))
    }

    fn viewer_projects(&self, secret: &str) -> Result<Paged<GithubProjectOption>, String> {
        let query = format!(
            "query($cursor: String) {{ viewer {{ projectsV2(first: 100, after: $cursor) {{ {PROJECT_FIELDS} }} }} }}"
        );
        paginate(MAX_PROJECT_PAGES, |cursor| {
            let response = graphql(&self.url, secret, &query, json!({ "cursor": cursor }))?;
            let connection =
                response.get("data").and_then(|d| d.get("viewer")).and_then(|v| v.get("projectsV2"));
            if connection.is_none() {
                return Err(ERR_REQUEST_FAILED.to_string());
            }
            Ok((projects_of(connection), next_cursor(connection)))
        })
    }

    fn owner_projects(
        &self,
        secret: &str,
        owner: &str,
    ) -> Result<Paged<GithubProjectOption>, String> {
        let query = format!(
            "query($owner: String!, $cursor: String) {{ repositoryOwner(login: $owner) {{ \
             ... on Organization {{ projectsV2(first: 100, after: $cursor) {{ {PROJECT_FIELDS} }} }} \
             ... on User {{ projectsV2(first: 100, after: $cursor) {{ {PROJECT_FIELDS} }} }} }} }}"
        );
        paginate(MAX_PROJECT_PAGES, |cursor| {
            let response = graphql(&self.url, secret, &query, json!({ "owner": owner, "cursor": cursor }))?;
            let connection = response
                .get("data")
                .and_then(|d| d.get("repositoryOwner"))
                .and_then(|o| o.get("projectsV2"));
            if connection.is_none() {
                return Err(ERR_REQUEST_FAILED.to_string());
            }
            Ok((projects_of(connection), next_cursor(connection)))
        })
    }

    fn project_shape(&self, secret: &str, project_id: &str) -> Result<ProjectShape, String> {
        let query = "query($id: ID!) { node(id: $id) { ... on ProjectV2 { title \
                     field(name: \"Status\") { ... on ProjectV2SingleSelectField { id options { id name } } } } } }";
        shape_of(&graphql(&self.url, secret, query, json!({ "id": project_id }))?)
    }

    fn project_items(&self, secret: &str, project_id: &str) -> Result<Paged<ProjectItem>, String> {
        let query = "query($id: ID!, $cursor: String) { node(id: $id) { ... on ProjectV2 { \
                     items(first: 100, after: $cursor) { pageInfo { hasNextPage endCursor } \
                     nodes { id fieldValueByName(name: \"Status\") { \
                     ... on ProjectV2ItemFieldSingleSelectValue { name optionId } } \
                     content { ... on Issue { number title url state issueType { name } \
                     repository { name owner { login } } } } } } } } }";
        paginate(MAX_ITEM_PAGES, |cursor| {
            let response = graphql(&self.url, secret, query, json!({ "id": project_id, "cursor": cursor }))?;
            items_page_of(&response)
        })
    }

    fn fetch_issue(
        &self,
        secret: &str,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Option<FetchedIssue>, String> {
        let query = "query($owner: String!, $name: String!, $number: Int!) { \
                     repository(owner: $owner, name: $name) { name owner { login } \
                     issue(number: $number) { number title body url state issueType { name } \
                     projectItems(first: 50) { nodes { id project { id } \
                     fieldValueByName(name: \"Status\") { \
                     ... on ProjectV2ItemFieldSingleSelectValue { name optionId } } } } } } }";
        let response = graphql(
            &self.url,
            secret,
            query,
            json!({ "owner": owner, "name": repo, "number": number }),
        )?;
        fetched_issue_of(&response)
    }

    fn set_item_status(
        &self,
        secret: &str,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String> {
        let query = "mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) { \
                     updateProjectV2ItemFieldValue(input: { projectId: $project, itemId: $item, \
                     fieldId: $field, value: { singleSelectOptionId: $option } }) { \
                     projectV2Item { id } } }";
        let response = graphql(
            &self.url,
            secret,
            query,
            json!({ "project": project_id, "item": item_id, "field": field_id, "option": option_id }),
        )
        .map_err(|e| crate::tls::keep_tls(e, ERR_STATUS_UPDATE_FAILED))?;
        let updated = response
            .get("data")
            .and_then(|d| d.get("updateProjectV2ItemFieldValue"))
            .and_then(|u| u.get("projectV2Item"))
            .is_some_and(|i| !i.is_null());
        match updated && !has_errors(&response) {
            true => Ok(()),
            false => Err(ERR_STATUS_UPDATE_FAILED.to_string()),
        }
    }
}
