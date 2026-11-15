//! The reads and writes of the `github` adapter that GDP-FR-ICDW to GDP-FR-QFQF
//! add: the repository labels, the `Status` field, one issue in full, the
//! label writes, and the `Status` write. They use the request helpers of the
//! parent module, so every request goes through the same path and every error
//! is a fixed code.

use serde_json::{json, Value};

use crate::github_polling::records::{ERR_PROJECT_UNAVAILABLE, ERR_REQUEST_FAILED};

use super::*;

/// The labels, and the sub-issues, that one detail read takes at most.
const DETAIL_PAGE: usize = 100;

/// GDP-FR-QFQF: the `Status` field of a Project `node` value, or `None`.
pub(super) fn status_field_of(node: &Value) -> Option<StatusFieldRead> {
    let field = node.get("field").filter(|f| !f.is_null())?;
    let field_id = field.get("id").and_then(Value::as_str)?.to_string();
    let options = field
        .get("options")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|o| {
            Some(StatusChoice { id: str_of(o, "id"), name: o.get("name")?.as_str()?.into() })
        })
        .collect();
    Some(StatusFieldRead { field_id, options })
}

/// GDP-FR-QFQF: one issue in full from a GraphQL issue value. The blockers are
/// the first page; the caller completes them. Pure.
pub(super) fn detail_of(value: &Value, project_id: &str) -> Option<IssueDetail> {
    let base = issue_of(value)?;
    let names = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes.iter().map(|n| str_of(n, "name")).filter(|n| !n.is_empty()).collect()
            })
            .unwrap_or_default()
    };
    let sub_issues = value
        .get("subIssues")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
        .map(|nodes| nodes.iter().filter_map(link_of).collect())
        .unwrap_or_default();
    let status = value
        .get("projectItems")
        .and_then(|c| c.get("nodes"))
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes.iter().find(|n| {
                n.get("project").and_then(|p| p.get("id")).and_then(Value::as_str)
                    == Some(project_id)
            })
        })
        .and_then(|n| n.get("fieldValueByName"))
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(IssueDetail {
        number: base.number,
        url: base.url,
        owner: base.owner,
        repo: base.repo,
        title: base.title,
        body: base.body,
        state: base.state,
        state_reason: base.state_reason,
        issue_type: base.issue_type,
        labels: names("labels"),
        milestone: base.milestone,
        milestone_title: value
            .get("milestone")
            .filter(|m| !m.is_null())
            .map(|m| str_of(m, "title"))
            .filter(|t| !t.is_empty()),
        parent: base.parent,
        sub_issues,
        blocked_by: base.blocked_by,
        status,
        updated_at: base.updated_at,
    })
}

impl GithubTracker {
    /// GDP-FR-QFQF: every label name of the repository, page by page.
    pub(super) fn labels(&self) -> Result<Vec<String>, String> {
        let query = format!(
            "query($owner: String!, $name: String!, $cursor: String) {{ repository(owner: $owner, name: $name) {{ \
             labels(first: 100, after: $cursor) {{ nodes {{ name }} {PAGE_INFO} }} }} }}"
        );
        let first = self.repo_query(&query, json!({ "cursor": Value::Null }))?;
        let connection = first.get("labels").cloned().unwrap_or(Value::Null);
        let nodes = all_nodes(&connection, |after| {
            let page = self.repo_query(&query, json!({ "cursor": after }))?;
            Ok(page.get("labels").cloned().unwrap_or(Value::Null))
        })?;
        Ok(nodes.iter().map(|n| str_of(n, "name")).filter(|n| !n.is_empty()).collect())
    }

    /// GDP-FR-QFQF: the `Status` field of the Project.
    pub(super) fn status_field(&self, project_id: &str) -> Result<Option<StatusFieldRead>, String> {
        let query = "query($id: ID!) { node(id: $id) { ... on ProjectV2 { id \
                     field(name: \"Status\") { ... on ProjectV2SingleSelectField { id options { id name } } } } } }";
        let data = self.graphql_data(query, json!({ "id": project_id }))?;
        let node = data
            .get("node")
            .filter(|n| n.get("id").is_some())
            .ok_or_else(|| ERR_PROJECT_UNAVAILABLE.to_string())?;
        Ok(status_field_of(node))
    }

    /// GDP-FR-QFQF: one issue in full.
    pub(super) fn issue_detail(
        &self,
        number: u64,
        project_id: &str,
    ) -> Result<Option<IssueDetail>, String> {
        let fields = issue_fields();
        let query = format!(
            "query($owner: String!, $name: String!, $number: Int!) {{ repository(owner: $owner, name: $name) {{ \
             issueOrPullRequest(number: $number) {{ __typename ... on Issue {{ {fields} \
             milestone {{ number title }} \
             labels(first: {DETAIL_PAGE}) {{ nodes {{ name }} }} \
             subIssues(first: {DETAIL_PAGE}) {{ nodes {{ {LINK_FIELDS} }} }} \
             projectItems(first: 20) {{ nodes {{ project {{ id }} \
             fieldValueByName(name: \"Status\") {{ ... on ProjectV2ItemFieldSingleSelectValue {{ name }} }} }} }} \
             }} }} }} }}"
        );
        let repository = self.repo_query(&query, json!({ "number": number }))?;
        let value = repository.get("issueOrPullRequest").cloned().unwrap_or(Value::Null);
        if value.get("__typename").and_then(Value::as_str) != Some("Issue") {
            return Ok(None);
        }
        let Some(mut detail) = detail_of(&value, project_id) else { return Ok(None) };
        detail.blocked_by = self.all_blockers(&value)?;
        Ok(Some(detail))
    }

    /// GDP-FR-ICDW: add labels to an issue.
    pub(super) fn labels_added(&self, number: u64, labels: &[String]) -> Result<(), String> {
        self.rest(
            "POST",
            &format!("{}/issues/{number}/labels", self.repo_path()),
            Some(json!({ "labels": labels })),
        )
        .map(|_| ())
    }

    /// GDP-FR-ICDW: remove one label from an issue. The name is percent-encoded
    /// in the address.
    pub(super) fn label_removed(&self, number: u64, label: &str) -> Result<(), String> {
        let name = crate::github_publication::client::urlencode(label);
        self.rest("DELETE", &format!("{}/issues/{number}/labels/{name}", self.repo_path()), None)
            .map(|_| ())
    }

    /// GDP-FR-KOWL: set the `Status` of a Project item.
    pub(super) fn status_set(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String> {
        let query = "mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) { \
                     updateProjectV2ItemFieldValue(input: { projectId: $project, itemId: $item, \
                     fieldId: $field, value: { singleSelectOptionId: $option } }) { projectV2Item { id } } }";
        let data = self.graphql_data(
            query,
            json!({ "project": project_id, "item": item_id, "field": field_id, "option": option_id }),
        )?;
        data.get("updateProjectV2ItemFieldValue")
            .filter(|u| !u.is_null())
            .map(|_| ())
            .ok_or_else(|| ERR_REQUEST_FAILED.to_string())
    }
}
