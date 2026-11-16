//! The writes of an accepted plan row (`MPR-map-plan-proposals.md` MPR-FR-YCHW,
//! MPR-FR-FBSH, MPR-FR-RTGH).
//!
//! A new issue goes through the same create path as `create_item`, with a Type
//! that the row names. A change to an issue runs the field edits of
//! `GDP-github-delivery-plan.md` one after the other, in the order of the
//! requirement, and stops at the first failure (GDP-FR-MSVS).

use super::*;

/// MPR-FR-YCHW: the values of a new issue. Parent and milestone are numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewIssueRow {
    pub title: String,
    pub body: Option<String>,
    pub issue_type: Option<String>,
    pub milestone: Option<u64>,
    pub parent: Option<u64>,
    pub labels: Vec<String>,
}

/// MPR-FR-FBSH: the changed fields of a plan issue. A field that is `None` is
/// not changed. For `milestone` and `parent`, `Some(None)` removes the value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IssueChangeRow {
    pub number: u64,
    /// MPR-FR-RTGH: the `updated_at` of the issue at the proposal.
    pub base_updated_at: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub issue_type: Option<String>,
    pub milestone: Option<Option<u64>>,
    pub parent: Option<Option<u64>>,
    pub labels_add: Vec<String>,
    pub labels_remove: Vec<String>,
    pub status: Option<String>,
    pub close: Option<CloseReason>,
}

/// One accepted row, ready to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowWrite {
    Create(NewIssueRow),
    Modify(IssueChangeRow),
}

/// What an applied row produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowApplied {
    pub number: u64,
    /// The URL of a new issue.
    pub url: Option<String>,
    /// The fields that a change wrote, in the order they were written.
    pub fields: Vec<String>,
}

impl TrackerEdit<'_> {
    /// The repository spelling of the Type `name`, or `issue_type_unavailable`.
    fn canonical_type(&self, name: &str) -> Result<String, DeliveryError> {
        let types = self.adapter.list_issue_types().map_err(|c| github_error(&c))?;
        types.into_iter().find(|t| t.eq_ignore_ascii_case(name.trim())).ok_or_else(|| {
            refusal(
                ERR_ISSUE_TYPE_UNAVAILABLE,
                "The repository does not offer the Type of the row.",
            )
        })
    }

    /// MPR-FR-YCHW: create the issue of a row and add it to the Project.
    fn create_row(&mut self, row: &NewIssueRow) -> Result<(u64, String), DeliveryError> {
        let issue_type = row.issue_type.as_deref().map(|t| self.canonical_type(t)).transpose()?;
        let labels = self.canonical_labels(&row.labels)?;
        self.write_new_issue(
            NewIssue {
                title: row.title.clone(),
                body: row.body.clone().unwrap_or_default(),
                issue_type,
                milestone: row.milestone,
                labels,
            },
            row.parent,
        )
    }

    /// MPR-FR-FBSH: write the Type of an issue.
    fn set_type_of(
        &mut self,
        number: u64,
        issue_type: &str,
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let issue = PlanRef::Issue(number);
        self.check_fresh(issue, updated_at)?;
        let name = self.canonical_type(issue_type)?;
        self.with_refresh(&[issue], |e, w| {
            w.run(e.adapter.update_issue(
                number,
                &IssueChange { issue_type: Some(name), ..Default::default() },
            ))
        })
    }

    /// Run one field edit of a change. The first edit names the `updated_at` of
    /// the proposal. A later edit names the value that the cache took after the
    /// edit before it. A failure after a write is `partial_edit`.
    fn run_field(
        &mut self,
        row: &IssueChangeRow,
        done: &mut Vec<String>,
        field: &str,
        edit: impl FnOnce(&mut Self, &str) -> Result<(), DeliveryError>,
    ) -> Result<(), DeliveryError> {
        let stamp = if done.is_empty() {
            row.base_updated_at.clone()
        } else {
            self.read.issue(row.number).map(|i| i.updated_at.clone()).unwrap_or_default()
        };
        match edit(self, &stamp) {
            Ok(()) => {
                done.push(field.to_string());
                Ok(())
            }
            Err(error) if done.is_empty() || error.code == ERR_PARTIAL_EDIT => Err(error),
            Err(error) => Err(refusal(
                ERR_PARTIAL_EDIT,
                &format!(
                    "The change stopped after {} of its fields: {}",
                    done.len(),
                    error.message
                ),
            )),
        }
    }

    /// MPR-FR-FBSH: apply each changed field in the order title, body, Type,
    /// milestone, parent, labels, Status, close. `done` names the fields
    /// written, also where a later field fails.
    fn modify_row(
        &mut self,
        row: &IssueChangeRow,
        done: &mut Vec<String>,
    ) -> Result<(), DeliveryError> {
        let id = issue_id(row.number);
        let n = row.number;
        if let Some(title) = &row.title {
            self.run_field(row, done, "title", |e, at| {
                e.apply(&GithubDeliveryEdit::Rename {
                    item_id: id.clone(),
                    name: title.clone(),
                    updated_at: at.to_string(),
                })
                .map(|_| ())
            })?;
        }
        if let Some(body) = &row.body {
            self.run_field(row, done, "body", |e, at| {
                e.apply(&GithubDeliveryEdit::SetDescription {
                    item_id: id.clone(),
                    description: body.clone(),
                    updated_at: at.to_string(),
                })
                .map(|_| ())
            })?;
        }
        if let Some(issue_type) = &row.issue_type {
            self.run_field(row, done, "type", |e, at| e.set_type_of(n, issue_type, at))?;
        }
        if let Some(milestone) = row.milestone {
            self.run_field(row, done, "milestone", |e, at| {
                e.set_milestone_of(&id, milestone.map(milestone_id).as_deref(), at)
            })?;
        }
        if let Some(parent) = row.parent {
            self.run_field(row, done, "parent", |e, at| {
                e.set_parent_of(&id, parent.map(issue_id).as_deref(), at)
            })?;
        }
        if !row.labels_add.is_empty() || !row.labels_remove.is_empty() {
            self.run_field(row, done, "labels", |e, at| {
                e.set_labels(&id, &row.labels_add, &row.labels_remove, at)
            })?;
        }
        if let Some(status) = &row.status {
            self.run_field(row, done, "status", |e, at| e.set_status(&id, status, at))?;
        }
        if let Some(reason) = row.close {
            self.run_field(row, done, "close", |e, at| e.close_item(&id, reason, at))?;
        }
        Ok(())
    }
}

/// MPR-FR-YCHW, MPR-FR-FBSH, MPR-FR-SGYA: write one accepted row for the target.
/// The call takes the turn of the edit queue (GDP-FR-IRDD), and it stores the
/// read cache where the row changed it (GDP-FR-VNNU). On a failure, the writes
/// that were made stay on GitHub, and the cache holds them.
pub(crate) fn apply_row<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    write: &RowWrite,
) -> Result<RowApplied, DeliveryError> {
    let state = app.project_store::<GithubDeliveryState>();
    let _turn = state.edits.enter();
    let store = machine_store(app)?;
    let mut read = match cached(app, target)? {
        Some(read) => read,
        None => shared_read(app, target)?,
    };
    let overlay = read_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id)?;
    let plan = build_plan(&read, &target.settings, &overlay, &target.statuses);
    let adapter = connect(app)?;
    let before = read.clone();
    let mut edit = TrackerEdit {
        adapter: adapter.as_ref(),
        project_id: &target.project_node_id,
        settings: &target.settings,
        read: &mut read,
        plan: &plan,
    };
    let result = match write {
        RowWrite::Create(row) => edit.create_row(row).map(|(number, url)| RowApplied {
            number,
            url: Some(url),
            fields: Vec::new(),
        }),
        RowWrite::Modify(row) => {
            let mut done = Vec::new();
            edit.modify_row(row, &mut done).map(|_| RowApplied {
                number: row.number,
                url: None,
                fields: done,
            })
        }
    };
    if read != before {
        store_edit(app, target, &read)?;
    }
    result
}
