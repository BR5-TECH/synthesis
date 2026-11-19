//! The tests of the plan proposals (`MPR-map-plan-proposals.md`), against the
//! fake GitHub tracker and a mock application with a project open. No test
//! reaches a network.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::Manager;

use super::*;
use crate::comments::{Opening, Participant, Stamp};
use crate::delivery::github::tests::fake::{Harness, PROJECT};

mod decisions;
mod drafts;
mod follow_up;
mod issue_rows;
mod modify_parents;
mod recording;
mod refusals;
mod reopening;
mod reprepare;
mod updating;

const AT: &str = "2026-03-01T10:00:00Z";

pub(super) fn human() -> Participant {
    Participant::Human { login: "author".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "planner".into(),
        model: None,
        title: Some(String::new()),
    }
}

/// A GitHub plan (issues 1, 2, 3 and milestone 1) with a Map thread. Each turn
/// that a re-prepare request starts goes to `turns`, and no model is called.
pub(super) struct Plan {
    pub h: Harness,
    pub discussion: String,
    pub turns: Arc<Mutex<Vec<super::reprepare::TurnRequest>>>,
    /// The failure that the next dispatch gives, or `None` for a turn that
    /// starts.
    pub dispatch_failure: Arc<Mutex<Option<String>>>,
}

impl Plan {
    pub fn app(&self) -> tauri::AppHandle<crate::delivery::github::tests::fake::Mock> {
        self.h.handle()
    }

    pub fn store(&self) -> crate::fs::RootFs {
        self.app().state::<crate::project::ProjectState>().require_store().unwrap()
    }
}

pub(super) fn setup() -> Plan {
    let h = Harness::github_plan();
    let store = h.handle().state::<crate::project::ProjectState>().require_store().unwrap();
    let write = crate::comments::open_map_discussion_in(
        &store,
        None,
        Opening {
            body: "Plan the next step".into(),
            attachments: Vec::new(),
            spoken_original: None,
        },
        Stamp { by: &human(), at: AT },
    )
    .unwrap();
    let turns: Arc<Mutex<Vec<super::reprepare::TurnRequest>>> = Arc::default();
    let dispatch_failure: Arc<Mutex<Option<String>>> = Arc::default();
    let (seen, failure) = (Arc::clone(&turns), Arc::clone(&dispatch_failure));
    h.handle().manage(super::reprepare::DispatchSeam(Arc::new(move |turn| {
        seen.lock().unwrap().push(turn.clone());
        match failure.lock().unwrap().clone() {
            Some(code) => Err(code),
            None => Ok(()),
        }
    })));
    Plan { h, discussion: write.discussion.id, turns, dispatch_failure }
}

pub(super) fn comment() -> ProposalComment {
    ProposalComment {
        body: "I propose these rows. <spoken>Here is a step.</spoken>".into(),
        author: agent(),
        spoken_original: Some("Here is a step.".into()),
    }
}

pub(super) fn new_issue(title: &str) -> CreateIssueFields {
    CreateIssueFields {
        title: title.into(),
        body: None,
        issue_type: None,
        milestone: None,
        parent: None,
        labels: Vec::new(),
    }
}

fn row(key: &str, fields: RowFields) -> MapPlanRow {
    MapPlanRow {
        key: key.into(),
        fields,
        state: RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    }
}

pub(super) fn create_row(key: &str, fields: CreateIssueFields) -> MapPlanRow {
    row(key, RowFields::CreateIssue(fields))
}

pub(super) fn draft_row(key: &str, name: &str, body: &str) -> MapPlanRow {
    row(key, RowFields::NewDraft(NewDraftFields { name: name.into(), body: body.into() }))
}

pub(super) fn change(field: &str, after: Value) -> RowChange {
    RowChange {
        field: field.into(),
        before: Value::Null,
        after,
        added: None,
        removed: None,
        reason: None,
    }
}

pub(super) fn label_change(added: &[&str], removed: &[&str]) -> RowChange {
    RowChange {
        added: Some(added.iter().map(|s| s.to_string()).collect()),
        removed: Some(removed.iter().map(|s| s.to_string()).collect()),
        ..change("labels", json!(null))
    }
}

pub(super) fn close_change(reason: &str) -> RowChange {
    RowChange { reason: Some(reason.into()), ..change("close", json!("closed")) }
}

/// The `updated_at` that the fake gives issue `number` at the start.
pub(super) fn base(number: u64) -> String {
    format!("2026-01-01T00:00:{number:02}Z")
}

pub(super) fn modify_row(key: &str, number: u64, changes: Vec<RowChange>) -> MapPlanRow {
    row(
        key,
        RowFields::ModifyIssue(ModifyIssueFields {
            number,
            title: format!("Issue {number}"),
            base_updated_at: base(number),
            changes,
        }),
    )
}

pub(super) fn record(plan: &Plan, rows: Vec<MapPlanRow>) -> MapPlanProposal {
    record_map_plan_proposal(&plan.app(), &plan.discussion, rows, comment()).unwrap()
}

pub(super) fn accept(
    plan: &Plan,
    proposal: &MapPlanProposal,
    key: &str,
) -> Result<MapPlanProposal, MapPlanError> {
    accept_row(&plan.app(), &plan.discussion, &proposal.id, key)
}

pub(super) fn reject(
    plan: &Plan,
    proposal: &MapPlanProposal,
    key: &str,
) -> Result<MapPlanProposal, MapPlanError> {
    reject_row(&plan.app(), &plan.discussion, &proposal.id, key)
}

pub(super) fn state_of(proposal: &MapPlanProposal, key: &str) -> RowState {
    proposal.rows.iter().find(|r| r.key == key).unwrap().state
}

pub(super) fn row_of<'a>(proposal: &'a MapPlanProposal, key: &str) -> &'a MapPlanRow {
    proposal.rows.iter().find(|r| r.key == key).unwrap()
}

/// The proposals of the discussion as a read returns them.
pub(super) fn listed(plan: &Plan) -> Vec<MapPlanProposal> {
    commands::list_proposals(&plan.app(), &plan.discussion).unwrap()
}

#[allow(dead_code)]
pub(super) const PROJECT_ID: &str = PROJECT;
