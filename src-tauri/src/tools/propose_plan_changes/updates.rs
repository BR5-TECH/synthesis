//! The `updates` form of `propose_plan_changes` (`PPT-propose-plan-changes-tool.md`
//! PPT-FR-STLY to PPT-FR-KOCG): the checks of the updates of the open proposal,
//! and their building into the updates of `crate::map_plan`.
//!
//! Two steps, both pure, in the shape of `rows`. [`check_updates`] checks what
//! the arguments and the open proposal show, and says what the plan must be
//! asked. [`build_updates`] checks the rows against the facts of the plan. A
//! refusal about one update names its place: *Update 2 of 4*.

use std::collections::HashSet;

use serde::Deserialize;

use super::rows::{self, bad_rows, invalid, Needs, Refused, OP_CREATE, OP_DRAFT, OP_MODIFY};
use super::{PlanRowArg, MAX_ROWS};
use crate::delivery::github::plan_reads::PlanFacts;
use crate::map_plan::{MapPlanError, MapPlanProposal, MapPlanUpdate, RowFields, RowState};
use crate::tools::plan_refusals::PlanRefusal;

pub const ACTION_CHANGE: &str = "change";
pub const ACTION_ADD: &str = "add";
pub const ACTION_WITHDRAW: &str = "withdraw";

// PPT contract surface, "Refusals".
pub const NOT_ONE_FORM: &str =
    "Pass `rows` for a new proposal or `updates` for your open proposal, not both.";
pub const NO_OPEN_PROPOSAL: &str = "No row of your proposals waits for the author, so there is nothing to update. Send a new proposal in `rows`.";
pub const NO_ROW_LEFT: &str = "This update withdraws every row. Keep at least one row; the author can reject the rows they do not want.";

/// One update, as the model composed it. Unknown fields are ignored (TLC-FR-07).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlanUpdateArg {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub row: Option<PlanRowArg>,
}

pub fn not_one_form() -> PlanRefusal {
    invalid("not_one_form", NOT_ONE_FORM)
}

pub fn no_open_proposal() -> PlanRefusal {
    invalid("no_open_proposal", NO_OPEN_PROPOSAL)
}

fn no_row_left() -> PlanRefusal {
    invalid("no_row_left", NO_ROW_LEFT)
}

/// PPT-FR-KOCG: the place of one update in the list that the model sent.
fn place(at: usize, of: usize) -> String {
    format!("Update {at} of {of}")
}

/// PPT-FR-KOCG: a refusal about one update names its place.
fn at_place(refusal: PlanRefusal, at: usize, of: usize) -> PlanRefusal {
    PlanRefusal { message: format!("{}: {}", place(at, of), refusal.message), ..refusal }
}

fn malformed(at: usize, of: usize) -> PlanRefusal {
    invalid(
        "update_malformed",
        format!(
            "{}: give `action` and `key`, and a `row` for `change` and `add`, and no `row` for `withdraw`.",
            place(at, of)
        ),
    )
}

fn no_such_row(at: usize, of: usize, key: &str) -> PlanRefusal {
    invalid(
        "no_such_row",
        format!(
            "{}: the key {} names no row of your open proposal. Name one of its rows, or use `add`.",
            place(at, of),
            rows::echo(key)
        ),
    )
}

fn row_named_twice(key: &str) -> PlanRefusal {
    invalid(
        "row_named_twice",
        format!(
            "Two of your updates name the row {}. Put what you want for it in one update.",
            rows::echo(key)
        ),
    )
}

fn row_settled(at: usize, of: usize, key: &str) -> PlanRefusal {
    invalid(
        "row_settled",
        format!(
            "{}: the row {} is accepted or failed, so it can not be updated. Use `add` for a new row.",
            place(at, of),
            rows::echo(key)
        ),
    )
}

fn op_changed(at: usize, of: usize, key: &str) -> PlanRefusal {
    invalid(
        "op_changed",
        format!(
            "{}: a `change` keeps the `op` of the row {}. Withdraw it and add a new row instead.",
            place(at, of),
            rows::echo(key)
        ),
    )
}

fn op_of(fields: &RowFields) -> &'static str {
    match fields {
        RowFields::CreateIssue(_) => OP_CREATE,
        RowFields::ModifyIssue(_) => OP_MODIFY,
        RowFields::NewDraft(_) => OP_DRAFT,
    }
}

/// One checked update: its place, its action, its key, and its row with the key
/// of the update.
#[derive(Debug, Clone)]
pub struct CheckedUpdate {
    pub at: usize,
    pub action: &'static str,
    pub key: String,
    pub row: Option<PlanRowArg>,
}

/// PPT-FR-KOCG: the line of one refused update. Its message names its place.
pub fn update_line(_at: usize, refusal: &PlanRefusal) -> String {
    refusal.message.clone()
}

/// PPT-FR-STLY to PPT-FR-KOCG: the checks of one update that the update and
/// the open proposal decide.
fn check_update(
    update: &PlanUpdateArg,
    at: usize,
    of: usize,
    open: &MapPlanProposal,
    named: &mut HashSet<String>,
) -> Result<CheckedUpdate, PlanRefusal> {
    let key = update.key.trim().to_string();
    let action = match update.action.trim() {
        ACTION_CHANGE => ACTION_CHANGE,
        ACTION_ADD => ACTION_ADD,
        ACTION_WITHDRAW => ACTION_WITHDRAW,
        _ => return Err(malformed(at, of)),
    };
    if key.is_empty() || (action == ACTION_WITHDRAW) == update.row.is_some() {
        return Err(malformed(at, of));
    }
    if !named.insert(key.clone()) {
        return Err(row_named_twice(&key));
    }
    let existing = open.rows.iter().find(|r| r.key == key);
    match (action, existing) {
        (ACTION_ADD, Some(_)) => return Err(at_place(bad_rows(), at, of)),
        (ACTION_ADD, None) => {}
        (_, None) => return Err(no_such_row(at, of, &key)),
        (_, Some(row)) => {
            if !matches!(row.state, RowState::Waiting | RowState::Rejected) {
                return Err(row_settled(at, of, &key));
            }
        }
    }
    let row = update.row.clone().map(|mut row| {
        row.key = key.clone();
        row
    });
    if let (ACTION_CHANGE, Some(existing), Some(row)) = (action, existing, &row) {
        if row.op != op_of(&existing.fields) {
            return Err(op_changed(at, of, &key));
        }
    }
    Ok(CheckedUpdate { at, action, key, row })
}

/// PPT-FR-STLY to PPT-FR-KOCG: the checks that the updates and the open
/// proposal decide, in the order the model sent them. Every update is checked
/// before the call refuses. The rows of the updates that pass are then checked
/// as rows of `rows`, and the ones that break a rule come back with their
/// refusals.
pub fn check_updates(
    updates: &[PlanUpdateArg],
    open: &MapPlanProposal,
) -> Result<(Vec<CheckedUpdate>, Needs, Refused), PlanRefusal> {
    let of = updates.len();
    if of == 0 || of > MAX_ROWS {
        return Err(bad_rows());
    }
    let mut checked = Vec::with_capacity(of);
    let mut refused = Refused::new();
    let mut named: HashSet<String> = HashSet::new();
    for (index, update) in updates.iter().enumerate() {
        let at = index + 1;
        match check_update(update, at, of, open, &mut named) {
            Ok(update) => checked.push(update),
            Err(refusal) => refused.push((at, refusal)),
        }
    }
    // PPT-FR-KOCG: the rows of the updates that passed are checked too, so one
    // refusal names every refused update. A rule of the rows left as a whole
    // decides only where no update was refused, because the rows left are not
    // known then.
    let left = check_rows_left(&checked, open, of);
    if !refused.is_empty() {
        if let Ok((_, more)) = left {
            refused.extend(more);
        }
        return Err(rows::refusal_of(refused, update_line).err().unwrap_or_else(bad_rows));
    }
    let (needs, more) = left?;
    Ok((checked, needs, more))
}

/// PPT-FR-KOTV, PPT-FR-JDUV: the rows that the proposal holds after the
/// updates, in their order, each new row checked as a row of `rows` is. A
/// `parent_row` names an earlier `create_issue` row of that list.
fn check_rows_left(
    checked: &[CheckedUpdate],
    open: &MapPlanProposal,
    of: usize,
) -> Result<(Needs, Refused), PlanRefusal> {
    let by_key = |key: &str| checked.iter().find(|u| u.key == key);
    let mut left: Vec<(String, Option<&CheckedUpdate>, &'static str)> = Vec::new();
    for row in &open.rows {
        match by_key(&row.key) {
            Some(u) if u.action == ACTION_WITHDRAW => {}
            Some(u) => left.push((row.key.clone(), Some(u), op_of(&row.fields))),
            None => left.push((row.key.clone(), None, op_of(&row.fields))),
        }
    }
    for update in checked.iter().filter(|u| u.action == ACTION_ADD) {
        let op = update.row.as_ref().map_or("", |r| r.op.as_str());
        let op = [OP_CREATE, OP_MODIFY, OP_DRAFT].into_iter().find(|o| *o == op).unwrap_or("");
        left.push((update.key.clone(), Some(update), op));
    }
    if left.is_empty() {
        return Err(no_row_left());
    }
    if left.len() > MAX_ROWS {
        return Err(bad_rows());
    }
    let mut needs = Needs::default();
    let mut refused = Refused::new();
    let mut creates: HashSet<&str> = HashSet::new();
    for (key, update, op) in &left {
        match update.and_then(|u| u.row.as_ref().map(|row| (u.at, row))) {
            Some((at, row)) => {
                if let Err(refusal) = rows::check_row(row, at, &creates, &mut needs) {
                    refused.push((at, at_place(refusal, at, of)));
                }
            }
            None => check_kept_parent(open, key, &creates)?,
        }
        if *op == OP_CREATE {
            creates.insert(key.as_str());
        }
    }
    Ok((needs, refused))
}

/// MPR-FR-HQXX: a row that the update keeps must still find its parent row.
fn check_kept_parent(
    open: &MapPlanProposal,
    key: &str,
    creates: &HashSet<&str>,
) -> Result<(), PlanRefusal> {
    let Some(row) = open.rows.iter().find(|r| r.key == key) else { return Ok(()) };
    match crate::map_plan::parent_row_of(&row.fields) {
        Some(parent) if !creates.contains(parent) => Err(invalid(
            "bad_parent_row",
            format!(
                "The row {} keeps the row {} as its parent, so that row can not be withdrawn.",
                rows::echo(key),
                rows::echo(parent)
            ),
        )),
        _ => Ok(()),
    }
}

/// PPT-FR-KOTV: the updates of `crate::map_plan`, each row checked against the
/// facts of the plan and read again from GitHub. `of` is the count of updates
/// that the model sent. The updates at the places of `refused` are not built
/// again, and each row that the facts refuse is added to `refused`.
pub fn build_updates(
    checked: &[CheckedUpdate],
    of: usize,
    facts: &PlanFacts,
    refused: &mut Refused,
) -> Vec<MapPlanUpdate> {
    let skipped: HashSet<usize> = refused.iter().map(|(at, _)| *at).collect();
    let mut built = Vec::with_capacity(checked.len());
    for update in checked.iter().filter(|u| !skipped.contains(&u.at)) {
        let row = |row: &PlanRowArg| {
            rows::build_row(row, update.at, facts).map_err(|r| at_place(r, update.at, of))
        };
        let result = match (update.action, &update.row) {
            (ACTION_CHANGE, Some(r)) => row(r).map(MapPlanUpdate::Change),
            (ACTION_ADD, Some(r)) => row(r).map(MapPlanUpdate::Add),
            _ => Ok(MapPlanUpdate::Withdraw(update.key.clone())),
        };
        match result {
            Ok(update) => built.push(update),
            Err(refusal) => refused.push((update.at, refusal)),
        }
    }
    built
}

/// The counts of the updates by action, for the output and the log record.
pub fn counts(checked: &[CheckedUpdate]) -> (usize, usize, usize) {
    let count = |action: &str| checked.iter().filter(|u| u.action == action).count();
    (count(ACTION_CHANGE), count(ACTION_ADD), count(ACTION_WITHDRAW))
}

/// The typed errors of an update in this tool's terms. The tool checked each
/// one already, so these answer a decision that ran between the check and the
/// update.
pub fn update_refusal(error: &MapPlanError) -> Option<PlanRefusal> {
    match error.code.as_str() {
        "no_open_proposal" => Some(no_open_proposal()),
        "no_row_left" => Some(no_row_left()),
        "row_decided" => Some(invalid(
            "row_settled",
            "A row that you named was decided while you wrote the update. Read the rows again and update the rows that wait.",
        )),
        "row_not_found" | "key_taken" | "kind_changed" | "parent_withdrawn" => Some(bad_rows()),
        _ => None,
    }
}
