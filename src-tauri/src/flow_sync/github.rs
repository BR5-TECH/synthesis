//! The GitHub tracker adapter (FTS-FR-ARMD, FTS-FR-ZLFX).
//!
//! The reads go through GraphQL, many items to one request (FTS NFR). A
//! relation list longer than one page is read page by page, so a list is
//! never cut. The writes go through REST: an issue's title, body, Type, and milestone, a
//! milestone, a sub-issue relation, and a blocked-by relation. No request
//! closes, reopens, or deletes an issue or a milestone.
//!
//! Every error is a fixed code. A transport error never reaches a caller as
//! text, because `ureq`'s message can echo the request it made (GHP-FR-DHXK).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::adapter::*;
use super::records::ERR_GITHUB_UNREACHABLE;

/// The whole request budget of one call.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The most issues one GraphQL read asks for.
const ISSUE_BATCH: usize = 50;
/// The most parents one sub-issue read asks for: each answer holds up to
/// [`CONNECTION_SIZE`] sub-issues with their own relations.
const PARENT_BATCH: usize = 10;
/// The most relations of one kind one page reads for one issue. A longer
/// list is read page by page, so no list is cut.
const CONNECTION_SIZE: usize = 50;
/// The page cursor of a connection.
const PAGE_INFO: &str = "pageInfo { hasNextPage endCursor }";
/// The fields of an issue that a relation names.
const KEY_FIELDS: &str = "number url repository { name owner { login } }";

/// The adapter of one GitHub repository, with the secret it sends.
pub struct GithubAdapter {
    host: String,
    owner: String,
    repo: String,
    secret: String,
    api: String,
    graphql: String,
    /// GitHub's numeric id of each issue read or created, which the
    /// relation endpoints name.
    ids: Mutex<HashMap<u64, u64>>,
}

impl GithubAdapter {
    pub fn new(host: &str, owner: &str, repo: &str, secret: String) -> Self {
        Self {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            secret,
            api: crate::github_tokens::github_api_base(host),
            graphql: crate::github_tokens::github_graphql_url(host),
            ids: Mutex::new(HashMap::new()),
        }
    }

    /// An adapter whose requests go to the REST base `api` and the GraphQL
    /// address `graphql`, so a test can answer them from a local server.
    #[cfg(test)]
    pub(crate) fn at(api: &str, graphql: &str, owner: &str, repo: &str) -> Self {
        Self {
            api: api.to_string(),
            graphql: graphql.to_string(),
            ..Self::new("github.com", owner, repo, "test-secret".to_string())
        }
    }

    /// Record GitHub's numeric id of an issue, as a read does.
    #[cfg(test)]
    pub(crate) fn remember_id(&self, number: u64, id: u64) {
        self.remember(number, id);
    }

    fn agent() -> ureq::Agent {
        crate::tls::ureq_config().timeout_global(Some(TIMEOUT)).build().into()
    }

    fn fail(&self, error: &ureq::Error, url: &str) -> String {
        crate::tls::ureq_wire(error, url).unwrap_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
    }

    /// The `repository` of a query whose variables are the bound repository.
    fn graphql(&self, query: &str) -> Result<Value, String> {
        let data = self.graphql_data(query, json!({ "owner": self.owner, "name": self.repo }))?;
        data.get("repository")
            .filter(|r| !r.is_null())
            .cloned()
            .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
    }

    /// The `data` of one GraphQL request.
    fn graphql_data(&self, query: &str, variables: Value) -> Result<Value, String> {
        let payload = json!({ "query": query, "variables": variables });
        let response = Self::agent()
            .post(&self.graphql)
            .header("Authorization", &format!("Bearer {}", self.secret))
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "synthesis")
            .header("GraphQL-Features", "issue_types,sub_issues")
            .send(payload.to_string());
        let mut response = response.map_err(|e| self.fail(&e, &self.graphql))?;
        let body =
            response.body_mut().read_to_string().map_err(|_| ERR_GITHUB_UNREACHABLE.to_string())?;
        let value: Value =
            serde_json::from_str(&body).map_err(|_| ERR_GITHUB_UNREACHABLE.to_string())?;
        // A number that names no item answers `null` with a NOT_FOUND error.
        // Every other error means the read could not answer.
        let errors = value.get("errors").and_then(Value::as_array).cloned().unwrap_or_default();
        if errors.iter().any(|e| e.get("type").and_then(Value::as_str) != Some("NOT_FOUND")) {
            return Err(ERR_GITHUB_UNREACHABLE.to_string());
        }
        value
            .get("data")
            .filter(|d| !d.is_null())
            .cloned()
            .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
    }

    /// The connection `connection` of the issue with the global id `node_id`,
    /// from the page after `after`.
    fn next_page(
        &self,
        node_id: &str,
        connection: &str,
        fields: &str,
        after: &str,
    ) -> Result<Value, String> {
        let n = CONNECTION_SIZE;
        let query = format!(
            "query($id: ID!, $after: String!) {{ node(id: $id) {{ ... on Issue {{ \
             {connection}(first: {n}, after: $after) {{ nodes {{ {fields} }} {PAGE_INFO} }} }} }} }}"
        );
        let data = self.graphql_data(&query, json!({ "id": node_id, "after": after }))?;
        data.get("node")
            .and_then(|node| node.get(connection))
            .filter(|c| !c.is_null())
            .cloned()
            .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
    }

    /// Every node of the connection `connection` of `issue`: the first page,
    /// which the read gave, and every page after it.
    fn all_of(&self, issue: &Value, connection: &str, fields: &str) -> Result<Vec<Value>, String> {
        let Some(first) = issue.get(connection).filter(|c| !c.is_null()) else {
            return Ok(Vec::new());
        };
        let node_id = str_of(issue, "id");
        all_nodes(first, |after| {
            if node_id.is_empty() {
                return Err(ERR_GITHUB_UNREACHABLE.to_string());
            }
            self.next_page(&node_id, connection, fields, after)
        })
    }

    /// One issue of an answer, with every page of its blockers.
    fn complete_issue(&self, value: &Value) -> Result<Option<TrackerIssue>, String> {
        let Some(mut issue) = issue_of(value) else { return Ok(None) };
        issue.blocked_by =
            self.all_of(value, "blockedBy", KEY_FIELDS)?.iter().filter_map(key_of).collect();
        Ok(Some(issue))
    }

    /// One REST request. A request with no payload sends no body: a `DELETE`
    /// with a body of `null` is refused by GitHub.
    fn rest(&self, method: &str, path: &str, payload: Option<Value>) -> Result<Value, String> {
        let url = format!("{}{path}", self.api);
        let agent = Self::agent();
        let auth = format!("Bearer {}", self.secret);
        let version = crate::github_tokens::GITHUB_API_VERSION;
        let result = match (method, payload) {
            ("GET", _) => rest_headers(agent.get(&url), &auth, version).call(),
            ("DELETE", None) => rest_headers(agent.delete(&url), &auth, version).call(),
            ("DELETE", Some(payload)) => {
                rest_headers(agent.delete(&url).force_send_body(), &auth, version)
                    .header("Content-Type", "application/json")
                    .send(payload.to_string())
            }
            (method, payload) => {
                let builder = if method == "PATCH" { agent.patch(&url) } else { agent.post(&url) };
                rest_headers(builder, &auth, version)
                    .header("Content-Type", "application/json")
                    .send(payload.unwrap_or_else(|| json!({})).to_string())
            }
        };
        let mut response = result.map_err(|e| self.fail(&e, &url))?;
        let body = response.body_mut().read_to_string().unwrap_or_default();
        Ok(serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn repo_path(&self) -> String {
        format!("/repos/{}/{}", self.owner, self.repo)
    }

    /// GitHub's numeric id of issue `number`, read once where no earlier read
    /// gave it.
    fn database_id(&self, number: u64) -> Result<u64, String> {
        if let Some(id) = self.ids.lock().ok().and_then(|ids| ids.get(&number).copied()) {
            return Ok(id);
        }
        let value = self.rest("GET", &format!("{}/issues/{number}", self.repo_path()), None)?;
        let id = value.get("id").and_then(Value::as_u64).ok_or(ERR_GITHUB_UNREACHABLE)?;
        self.remember(number, id);
        Ok(id)
    }

    fn remember(&self, number: u64, id: u64) {
        if let Ok(mut ids) = self.ids.lock() {
            ids.insert(number, id);
        }
    }
}

/// The headers of every REST request.
fn rest_headers<B>(
    builder: ureq::RequestBuilder<B>,
    auth: &str,
    version: &str,
) -> ureq::RequestBuilder<B> {
    builder
        .header("Authorization", auth)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", version)
        .header("User-Agent", "synthesis")
}

/// The issue fields every read asks for. `id` is the global id that a read
/// of the next page of a relation names.
fn issue_fields() -> String {
    let n = CONNECTION_SIZE;
    format!(
        "id databaseId number url state title body \
         repository {{ name owner {{ login }} }} \
         issueType {{ name }} milestone {{ number }} \
         parent {{ {KEY_FIELDS} }} \
         blockedBy(first: {n}) {{ nodes {{ {KEY_FIELDS} }} {PAGE_INFO} }}"
    )
}

/// The cursor of the page after `connection`, or `None` where it is the
/// last page. A connection that has a next page and no cursor cannot be
/// read to its end, so it is an error: a cut list never reads as complete.
pub fn next_cursor(connection: &Value) -> Result<Option<String>, String> {
    let info = connection.get("pageInfo");
    let more = info.and_then(|i| i.get("hasNextPage")).and_then(Value::as_bool);
    match more {
        Some(false) => Ok(None),
        Some(true) => info
            .and_then(|i| i.get("endCursor"))
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(|c| Some(c.to_string()))
            .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string()),
        None => Err(ERR_GITHUB_UNREACHABLE.to_string()),
    }
}

/// Every node of a connection: the nodes of `first`, then the nodes of each
/// page that `next_page` reads for the cursor of the page before it. A page
/// that gives the same cursor again is an error, so the read always ends.
pub fn all_nodes(
    first: &Value,
    mut next_page: impl FnMut(&str) -> Result<Value, String>,
) -> Result<Vec<Value>, String> {
    let nodes_of = |c: &Value| -> Result<Vec<Value>, String> {
        c.get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| ERR_GITHUB_UNREACHABLE.to_string())
    };
    let mut out = nodes_of(first)?;
    let mut cursor = next_cursor(first)?;
    while let Some(after) = cursor {
        let page = next_page(&after)?;
        out.extend(nodes_of(&page)?);
        cursor = next_cursor(&page)?;
        if cursor.as_deref() == Some(after.as_str()) {
            return Err(ERR_GITHUB_UNREACHABLE.to_string());
        }
    }
    Ok(out)
}

fn str_of(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn key_of(value: &Value) -> Option<IssueKey> {
    let repository = value.get("repository")?;
    Some(IssueKey {
        owner: repository.get("owner").map(|o| str_of(o, "login")).unwrap_or_default(),
        repo: str_of(repository, "name"),
        number: value.get("number")?.as_u64()?,
        url: str_of(value, "url"),
    })
}

/// One issue of a GraphQL answer.
pub fn issue_of(value: &Value) -> Option<TrackerIssue> {
    if value.is_null() {
        return None;
    }
    let key = key_of(value)?;
    Some(TrackerIssue {
        number: key.number,
        url: key.url,
        owner: key.owner,
        repo: key.repo,
        title: str_of(value, "title"),
        body: str_of(value, "body"),
        state: str_of(value, "state").to_ascii_lowercase(),
        issue_type: value
            .get("issueType")
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        milestone: value.get("milestone").and_then(|m| m.get("number")).and_then(Value::as_u64),
        parent: value.get("parent").filter(|p| !p.is_null()).and_then(key_of),
        blocked_by: value
            .get("blockedBy")
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| nodes.iter().filter_map(key_of).collect())
            .unwrap_or_default(),
    })
}

/// One milestone of a GraphQL answer. The due date keeps its calendar date.
pub fn milestone_of(value: &Value) -> Option<TrackerMilestone> {
    if value.is_null() {
        return None;
    }
    Some(TrackerMilestone {
        number: value.get("number")?.as_u64()?,
        url: str_of(value, "url"),
        title: str_of(value, "title"),
        description: str_of(value, "description"),
        state: str_of(value, "state").to_ascii_lowercase(),
        due: value
            .get("dueOn")
            .and_then(Value::as_str)
            .and_then(|d| d.get(..10))
            .map(str::to_string),
    })
}

/// The `due_on` value REST takes for a calendar date. Noon UTC keeps the same
/// date in every time zone GitHub shows it in.
fn due_on(due: Option<&str>) -> Value {
    due.map_or(Value::Null, |d| Value::String(format!("{d}T12:00:00Z")))
}

impl TrackerAdapter for GithubAdapter {
    fn repository(&self) -> (&str, &str, &str) {
        (&self.host, &self.owner, &self.repo)
    }

    fn read_issues(&self, numbers: &[u64]) -> Result<Vec<(u64, Option<TrackerIssue>)>, String> {
        let mut out = Vec::new();
        let fields = issue_fields();
        for chunk in numbers.chunks(ISSUE_BATCH) {
            let aliases: String = chunk
                .iter()
                .enumerate()
                .map(|(i, n)| format!("i{i}: issue(number: {n}) {{ {fields} }} "))
                .collect();
            let query =
                format!("query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ {aliases} }} }}");
            let repository = self.graphql(&query)?;
            for (i, number) in chunk.iter().enumerate() {
                let value = repository.get(format!("i{i}")).cloned().unwrap_or(Value::Null);
                if let Some(id) = value.get("databaseId").and_then(Value::as_u64) {
                    self.remember(*number, id);
                }
                out.push((*number, self.complete_issue(&value)?));
            }
        }
        Ok(out)
    }

    fn read_milestones(
        &self,
        numbers: &[u64],
    ) -> Result<Vec<(u64, Option<TrackerMilestone>)>, String> {
        let mut out = Vec::new();
        for chunk in numbers.chunks(ISSUE_BATCH) {
            let aliases: String = chunk
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    format!("m{i}: milestone(number: {n}) {{ number url title description dueOn state }} ")
                })
                .collect();
            let query =
                format!("query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ {aliases} }} }}");
            let repository = self.graphql(&query)?;
            for (i, number) in chunk.iter().enumerate() {
                out.push((*number, repository.get(format!("m{i}")).and_then(milestone_of)));
            }
        }
        Ok(out)
    }

    fn read_sub_issues(&self, parents: &[u64]) -> Result<Vec<(u64, Vec<TrackerIssue>)>, String> {
        let mut out = Vec::new();
        let fields = issue_fields();
        let n = CONNECTION_SIZE;
        for chunk in parents.chunks(PARENT_BATCH) {
            let aliases: String = chunk
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    format!("i{i}: issue(number: {p}) {{ id subIssues(first: {n}) {{ nodes {{ {fields} }} {PAGE_INFO} }} }} ")
                })
                .collect();
            let query =
                format!("query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ {aliases} }} }}");
            let repository = self.graphql(&query)?;
            for (i, parent) in chunk.iter().enumerate() {
                let value = repository.get(format!("i{i}")).cloned().unwrap_or(Value::Null);
                let nodes = self.all_of(&value, "subIssues", &fields)?;
                let mut children = Vec::new();
                for node in &nodes {
                    // A sub-issue in another repository has a number of its
                    // own repository, so only an own one is remembered.
                    let own = key_of(node).is_some_and(|k| {
                        k.owner.eq_ignore_ascii_case(&self.owner)
                            && k.repo.eq_ignore_ascii_case(&self.repo)
                    });
                    if let (true, Some(number), Some(id)) = (
                        own,
                        node.get("number").and_then(Value::as_u64),
                        node.get("databaseId").and_then(Value::as_u64),
                    ) {
                        self.remember(number, id);
                    }
                    if let Some(issue) = self.complete_issue(node)? {
                        children.push(issue);
                    }
                }
                out.push((*parent, children));
            }
        }
        Ok(out)
    }

    fn create_issue(&self, issue: &NewIssue) -> Result<Created, String> {
        let mut payload = json!({ "title": issue.title, "body": issue.body });
        if let Some(t) = &issue.issue_type {
            payload["type"] = Value::String(t.clone());
        }
        if let Some(m) = issue.milestone {
            payload["milestone"] = Value::from(m);
        }
        let value = self.rest("POST", &format!("{}/issues", self.repo_path()), Some(payload))?;
        let number = value.get("number").and_then(Value::as_u64).ok_or(ERR_GITHUB_UNREACHABLE)?;
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            self.remember(number, id);
        }
        Ok(Created { number, url: str_of(&value, "html_url") })
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
            payload["type"] = t.clone().map_or(Value::Null, Value::String);
        }
        self.rest("PATCH", &format!("{}/issues/{number}", self.repo_path()), Some(payload))
            .map(|_| ())
    }

    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueKey>,
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
            // The current parent can be in another repository: the relation
            // is removed on the parent's own repository.
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

    fn create_milestone(&self, milestone: &NewMilestone) -> Result<Created, String> {
        let mut payload = json!({ "title": milestone.title, "description": milestone.description });
        if milestone.due.is_some() {
            payload["due_on"] = due_on(milestone.due.as_deref());
        }
        let value =
            self.rest("POST", &format!("{}/milestones", self.repo_path()), Some(payload))?;
        let number = value.get("number").and_then(Value::as_u64).ok_or(ERR_GITHUB_UNREACHABLE)?;
        Ok(Created { number, url: str_of(&value, "html_url") })
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

    fn list_issue_types(&self) -> Result<Vec<String>, String> {
        let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { issueTypes(first: 100) { nodes { name isEnabled } } } }";
        let repository = self.graphql(query)?;
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
}
