//! The rows of `propose_plan_changes`: their checks and their building
//! (`PPT-propose-plan-changes-tool.md` PPT-FR-PMQL to PPT-FR-QCTM).
//!
//! Two steps, both pure. [`check_shape`] checks what the arguments alone show
//! and says what the plan must be asked. [`build_rows`] checks the rows against
//! the facts of the plan and makes the rows of the proposal. A refusal names a
//! row by its place in the call, counted from 1, and a number, a label, a
//! Type, a milestone, or a Status that the model gave. It holds no other text
//! of the model.

use std::collections::HashSet;

use rig::tool::ToolErrorKind;
use serde_json::{json, Value};

use super::{PlanRowArg, MAX_ROWS};
use crate::delivery::github::plan_reads::{PlanFacts, PlanIssueDetail};
use crate::map_plan::{
    CreateIssueFields, MapPlanRow, MilestoneRef, ModifyIssueFields, NewDraftFields, ParentRef,
    RowChange, RowFields, RowState,
};
use crate::tools::bounded_argument;
use crate::tools::plan_refusals::PlanRefusal;

pub const OP_CREATE: &str = "create_issue";
pub const OP_MODIFY: &str = "modify_issue";
pub const OP_DRAFT: &str = "new_draft";

// PPT contract surface, "Refusals".
pub const BAD_ROWS: &str = "Give 1 to 20 rows, each with its own `key` and an `op`, and no label in both `labels_add` and `labels_remove`.";
pub const BLANK_TITLE: &str = "Give the row a title.";
pub const BLANK_NAME: &str = "Give the new draft a name.";
pub const BLANK_BODY: &str = "Give the full prompt of the new draft in `body`.";
pub const BAD_CLOSE_REASON: &str = "`close_reason` is `completed` or `not_planned`.";

/// The longest name of the model that a refusal repeats, in characters.
const ECHO_CHARS: usize = 100;

pub fn echo(name: &str) -> String {
    bounded_argument(name.trim(), ECHO_CHARS)
}

pub fn invalid(code: &'static str, message: impl Into<String>) -> PlanRefusal {
    PlanRefusal::invalid(code, message)
}

pub fn bad_rows() -> PlanRefusal {
    invalid("bad_rows", BAD_ROWS)
}

fn not_a_plan_issue(number: i64) -> PlanRefusal {
    PlanRefusal::new(
        ToolErrorKind::NotFound,
        "not_a_plan_issue",
        true,
        format!("Issue number {number} is not part of the delivery plan."),
    )
}

fn no_change(at: usize, number: u64) -> PlanRefusal {
    invalid("no_change", format!("Row {at} changes nothing in issue {number} as it stands."))
}

fn bad_parent_row(at: usize) -> PlanRefusal {
    invalid(
        "bad_parent_row",
        format!("`parent_row` of row {at} must name the `key` of an earlier `create_issue` row."),
    )
}

/// The code of the refusal of PPT-FR-ELJI, which PPT-FR-DGAA logs with a count.
pub const ROWS_REFUSED: &str = "rows_refused";

/// The rows that one call refused: the place of each, counted from 1, and its
/// refusal.
pub type Refused = Vec<(usize, PlanRefusal)>;

/// PPT-FR-ELJI, PPT-FR-KOCG: the refusal of a call whose rows break rules.
/// One refused row keeps its own refusal. Several refused rows make one
/// **rows refused** refusal, with one line for each, in the order of their
/// places. `line` writes the line of one refused row.
pub fn refusal_of(
    mut refused: Refused,
    line: impl Fn(usize, &PlanRefusal) -> String,
) -> Result<(), PlanRefusal> {
    refused.sort_by_key(|(at, _)| *at);
    match refused.len() {
        0 => Ok(()),
        1 => Err(refused.remove(0).1),
        count => {
            let mut message = format!(
                "{count} rows were refused. Correct each one and send the call again. Nothing was proposed."
            );
            for (at, refusal) in &refused {
                message.push('\n');
                message.push_str(&line(*at, refusal));
            }
            Err(invalid(ROWS_REFUSED, message))
        }
    }
}

/// PPT-FR-ELJI: the line of one refused row of `rows`.
pub fn row_line(at: usize, refusal: &PlanRefusal) -> String {
    format!("Row {at}: {}", refusal.message)
}

/// PPT-FR-DGAA: the count of refused rows that a refusal carries, for the log
/// record. It is 1 for the refusal of one row.
pub fn refused_count(refusal: &PlanRefusal) -> usize {
    if refusal.code == ROWS_REFUSED {
        refusal.message.lines().count().saturating_sub(1)
    } else {
        1
    }
}

/// What the plan must be asked, from the rows (PPT-NFR: at most the labels, the
/// Types, and the Status options).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub types: bool,
    pub labels: bool,
    /// The plan issues that the modify rows name.
    pub numbers: Vec<u64>,
}

fn names(list: &Option<Vec<String>>) -> &[String] {
    list.as_deref().unwrap_or(&[])
}

fn given(text: &Option<String>) -> Option<&str> {
    text.as_deref().map(str::trim).filter(|t| !t.is_empty())
}

/// A field that only another kind of row has.
fn has_stray_field(op: &str, row: &PlanRowArg) -> bool {
    let create_only = row.labels.is_some();
    let modify_only = row.number.is_some()
        || row.labels_add.is_some()
        || row.labels_remove.is_some()
        || row.close_reason.is_some()
        || row.status.is_some();
    let draft_only = row.name.is_some();
    match op {
        OP_CREATE => modify_only || draft_only,
        OP_MODIFY => create_only || draft_only,
        _ => create_only || modify_only,
    }
}

/// PPT-FR-PMQL, PPT-FR-OVKT to PPT-FR-QCTM: the checks that the arguments
/// alone decide. A rule of the call as a whole refuses at once (PPT-FR-MSQT).
/// The rules of one row are checked for every row, and the rows that break
/// one come back with their refusals (PPT-FR-ELJI).
pub fn check_shape(rows: &[PlanRowArg]) -> Result<(Needs, Refused), PlanRefusal> {
    if rows.is_empty() || rows.len() > MAX_ROWS {
        return Err(bad_rows());
    }
    let mut keys = HashSet::new();
    for row in rows {
        let key = row.key.trim();
        if key.is_empty() || !keys.insert(key) {
            return Err(bad_rows());
        }
    }
    let mut needs = Needs::default();
    let mut refused = Refused::new();
    let mut creates: HashSet<&str> = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        let at = index + 1;
        if let Err(refusal) = check_row(row, at, &creates, &mut needs) {
            refused.push((at, refusal));
        }
        if row.op == OP_CREATE {
            creates.insert(row.key.trim());
        }
    }
    Ok((needs, refused))
}

/// PPT-FR-OVKT to PPT-FR-QCTM: the checks of one row that the arguments alone
/// decide. `creates` holds the keys of the `create_issue` rows before it, which
/// `parent_row` can name. The key is checked by the caller.
pub fn check_row(
    row: &PlanRowArg,
    at: usize,
    creates: &HashSet<&str>,
    needs: &mut Needs,
) -> Result<(), PlanRefusal> {
    let op = row.op.as_str();
    if ![OP_CREATE, OP_MODIFY, OP_DRAFT].contains(&op) || has_stray_field(op, row) {
        return Err(bad_rows());
    }
    if op != OP_DRAFT {
        return check_issue_row(op, row, at, creates, needs);
    }
    if given(&row.name).is_none() {
        return Err(invalid("blank_name", BLANK_NAME));
    }
    if given(&row.body).is_none() {
        return Err(invalid("blank_body", BLANK_BODY));
    }
    Ok(())
}

fn check_issue_row(
    op: &str,
    row: &PlanRowArg,
    at: usize,
    creates: &HashSet<&str>,
    needs: &mut Needs,
) -> Result<(), PlanRefusal> {
    if row.title.is_some() && given(&row.title).is_none() {
        return Err(invalid("blank_title", BLANK_TITLE));
    }
    if op == OP_CREATE && given(&row.title).is_none() {
        return Err(invalid("blank_title", BLANK_TITLE));
    }
    // PPT-FR-DPQP: a row gives `parent_issue` or `parent_row`, never both.
    if row.parent_issue.is_some() && row.parent_row.is_some() {
        return Err(bad_rows());
    }
    // PPT-FR-DPQP: on a new issue and on a changed issue alike.
    if let Some(parent) = &row.parent_row {
        if !creates.contains(parent.trim()) {
            return Err(bad_parent_row(at));
        }
    }
    if given(&row.type_name).is_some() {
        needs.types = true;
    }
    if !names(&row.labels).is_empty() {
        needs.labels = true;
    }
    if op == OP_CREATE {
        return Ok(());
    }
    let Some(number) = row.number else {
        return Err(bad_rows());
    };
    if let Some(reason) = &row.close_reason {
        if !["completed", "not_planned"].contains(&reason.trim()) {
            return Err(invalid("bad_close_reason", BAD_CLOSE_REASON));
        }
    }
    // PPT-FR-XOMU: a label in both lists.
    let lower = |l: &String| l.trim().to_lowercase();
    let removed: HashSet<String> = names(&row.labels_remove).iter().map(lower).collect();
    if names(&row.labels_add).iter().any(|l| removed.contains(&lower(l))) {
        return Err(bad_rows());
    }
    if let Ok(number) = u64::try_from(number) {
        needs.numbers.push(number);
        if !names(&row.labels_add).is_empty() || !names(&row.labels_remove).is_empty() {
            needs.labels = true;
        }
    }
    Ok(())
}

/// The spelling of the plan for a name of the model: an exact match, else a
/// match without case.
fn canonical<'a>(list: &'a [String], name: &str) -> Option<&'a str> {
    let name = name.trim();
    list.iter()
        .find(|c| c.as_str() == name)
        .or_else(|| list.iter().find(|c| c.eq_ignore_ascii_case(name)))
        .map(String::as_str)
}

fn unknown_type(name: &str) -> PlanRefusal {
    invalid("unknown_type", format!("The Type {} is not offered by the repository.", echo(name)))
}

fn unknown_label(name: &str) -> PlanRefusal {
    invalid(
        "unknown_label",
        format!(
            "The label {} does not exist in the repository. This tool creates no label. Use an existing label.",
            echo(name)
        ),
    )
}

fn unknown_milestone(title: &str) -> PlanRefusal {
    invalid("unknown_milestone", format!("No milestone of the plan has the title {}.", echo(title)))
}

fn unknown_status(name: &str) -> PlanRefusal {
    invalid("unknown_status", format!("The Status {} is not an option of the Project.", echo(name)))
}

fn type_of(facts: &PlanFacts, name: &str) -> Result<String, PlanRefusal> {
    let offered = facts.types.as_deref().unwrap_or(&[]);
    canonical(offered, name).map(str::to_string).ok_or_else(|| unknown_type(name))
}

/// PPT-FR-OVKT: a milestone by its exact title.
fn milestone_of(facts: &PlanFacts, title: &str) -> Result<MilestoneRef, PlanRefusal> {
    facts
        .milestones
        .iter()
        .find(|m| m.title == title || m.title == title.trim())
        .map(|m| MilestoneRef { number: m.number, title: m.title.clone() })
        .ok_or_else(|| unknown_milestone(title))
}

/// PPT-FR-XOMU: label names in the spelling of the repository, once each.
fn labels_of(facts: &PlanFacts, wanted: &[String]) -> Result<Vec<String>, PlanRefusal> {
    let known = facts.labels.as_deref().unwrap_or(&[]);
    let mut out: Vec<String> = Vec::new();
    for name in wanted {
        let label = canonical(known, name).ok_or_else(|| unknown_label(name))?;
        if !out.iter().any(|l| l == label) {
            out.push(label.to_string());
        }
    }
    Ok(out)
}

/// PPT-FR-DPQP: a plan issue named as a parent.
fn plan_parent(facts: &PlanFacts, number: i64) -> Result<u64, PlanRefusal> {
    u64::try_from(number)
        .ok()
        .filter(|n| facts.plan_numbers.contains(n))
        .ok_or_else(|| not_a_plan_issue(number))
}

fn waiting(key: &str, fields: RowFields) -> MapPlanRow {
    MapPlanRow {
        key: key.trim().to_string(),
        fields,
        state: RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    }
}

/// PPT-FR-UZEK: the rows of the proposal, every one `waiting`. The rows at the
/// places of `refused` are not built again. Each row that the facts refuse is
/// added to `refused` (PPT-FR-ELJI).
pub fn build_rows(
    rows: &[PlanRowArg],
    facts: &PlanFacts,
    refused: &mut Refused,
) -> Vec<MapPlanRow> {
    let skipped: HashSet<usize> = refused.iter().map(|(at, _)| *at).collect();
    let mut built = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let at = index + 1;
        if skipped.contains(&at) {
            continue;
        }
        match build_row(row, at, facts) {
            Ok(row) => built.push(row),
            Err(refusal) => refused.push((at, refusal)),
        }
    }
    built
}

/// PPT-FR-UZEK: one `waiting` row, checked against the facts of the plan. `at`
/// is the place that a refusal names.
pub fn build_row(
    row: &PlanRowArg,
    at: usize,
    facts: &PlanFacts,
) -> Result<MapPlanRow, PlanRefusal> {
    let fields = match row.op.as_str() {
        OP_CREATE => RowFields::CreateIssue(create_fields(row, facts)?),
        OP_MODIFY => RowFields::ModifyIssue(modify_fields(row, at, facts)?),
        _ => RowFields::NewDraft(NewDraftFields {
            name: given(&row.name).unwrap_or_default().to_string(),
            body: row.body.clone().unwrap_or_default(),
        }),
    };
    Ok(waiting(&row.key, fields))
}

/// PPT-FR-OVKT.
fn create_fields(row: &PlanRowArg, facts: &PlanFacts) -> Result<CreateIssueFields, PlanRefusal> {
    let issue_type = given(&row.type_name).map(|t| type_of(facts, t)).transpose()?;
    let milestone = match &row.milestone {
        Some(Some(title)) => Some(milestone_of(facts, title)?),
        _ => None,
    };
    let parent = match (&row.parent_issue, &row.parent_row) {
        (Some(Some(number)), _) => Some(ParentRef::Issue(plan_parent(facts, *number)?)),
        (_, Some(key)) => Some(ParentRef::Row(key.trim().to_string())),
        _ => None,
    };
    Ok(CreateIssueFields {
        title: given(&row.title).unwrap_or_default().to_string(),
        body: row.body.clone().filter(|b| !b.trim().is_empty()),
        issue_type,
        milestone,
        parent,
        labels: labels_of(facts, names(&row.labels))?,
    })
}

fn milestone_json(title: Option<&str>, number: Option<u64>) -> Value {
    match (title, number) {
        (Some(title), Some(number)) => json!({ "number": number, "title": title }),
        _ => Value::Null,
    }
}

fn change(field: &str, before: Value, after: Value) -> RowChange {
    RowChange { field: field.to_string(), before, after, added: None, removed: None, reason: None }
}

/// PPT-FR-KVFB to PPT-FR-YIFZ, MPR-FR-GKOL: a change to a plan issue, with the
/// value before and the value after of each changed field. A field that does
/// not change is not part of the row.
fn modify_fields(
    row: &PlanRowArg,
    at: usize,
    facts: &PlanFacts,
) -> Result<ModifyIssueFields, PlanRefusal> {
    let wanted = row.number.unwrap_or_default();
    let issue = plan_parent(facts, wanted)
        .ok()
        .and_then(|n| facts.issues.get(&n))
        .ok_or_else(|| not_a_plan_issue(wanted))?;
    let mut changes: Vec<RowChange> = Vec::new();
    if let Some(title) = given(&row.title).filter(|t| *t != issue.title) {
        changes.push(change("title", json!(issue.title), json!(title)));
    }
    if let Some(body) = row.body.as_ref().filter(|b| **b != issue.body) {
        changes.push(change("body", json!(issue.body), json!(body)));
    }
    if let Some(wanted) = given(&row.type_name) {
        let issue_type = type_of(facts, wanted)?;
        let same = issue.issue_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case(&issue_type));
        if !same {
            changes.push(change("type", json!(issue.issue_type), json!(issue_type)));
        }
    }
    if let Some(change) = milestone_change(row, issue, facts)? {
        changes.push(change);
    }
    if let Some(change) = parent_change(row, issue, facts)? {
        changes.push(change);
    }
    if let Some(change) = labels_change(row, issue, facts)? {
        changes.push(change);
    }
    if let Some(wanted) = given(&row.status) {
        let status = canonical(&facts.statuses, wanted).ok_or_else(|| unknown_status(wanted))?;
        if issue.status.as_deref() != Some(status) {
            changes.push(change("status", json!(issue.status), json!(status)));
        }
    }
    if let Some(reason) = given(&row.close_reason) {
        // PPT-FR-JPFE: a close of an issue that is closed already.
        if issue.state != "open" {
            return Err(no_change(at, issue.number));
        }
        let mut close = change("close", json!("open"), json!("closed"));
        close.reason = Some(reason.to_string());
        changes.push(close);
    }
    if changes.is_empty() {
        return Err(no_change(at, issue.number));
    }
    Ok(ModifyIssueFields {
        number: issue.number,
        title: issue.title.clone(),
        base_updated_at: issue.updated_at.clone(),
        changes,
    })
}

fn milestone_change(
    row: &PlanRowArg,
    issue: &PlanIssueDetail,
    facts: &PlanFacts,
) -> Result<Option<RowChange>, PlanRefusal> {
    let before = milestone_json(issue.milestone_title.as_deref(), issue.milestone);
    let after = match &row.milestone {
        None => return Ok(None),
        Some(None) => None,
        Some(Some(title)) => Some(milestone_of(facts, title)?),
    };
    if after.as_ref().map(|m| m.number) == issue.milestone {
        return Ok(None);
    }
    let after = after.map_or(Value::Null, |m| milestone_json(Some(&m.title), Some(m.number)));
    Ok(Some(change("milestone", before, after)))
}

fn parent_change(
    row: &PlanRowArg,
    issue: &PlanIssueDetail,
    facts: &PlanFacts,
) -> Result<Option<RowChange>, PlanRefusal> {
    let before = issue.parent.map_or(Value::Null, |p| json!({ "issue": p }));
    let after = match (&row.parent_issue, &row.parent_row) {
        (Some(Some(number)), _) => {
            let parent = plan_parent(facts, *number)?;
            // A plan issue is never its own parent.
            if parent == issue.number {
                return Err(bad_rows());
            }
            if issue.parent == Some(parent) {
                return Ok(None);
            }
            json!({ "issue": parent })
        }
        (Some(None), _) => {
            if issue.parent.is_none() {
                return Ok(None);
            }
            Value::Null
        }
        (None, Some(key)) => json!({ "row": key.trim() }),
        (None, None) => return Ok(None),
    };
    Ok(Some(change("parent", before, after)))
}

/// PPT-FR-XOMU, MPR-FR-NGSK: labels to add and to remove, kept only where they
/// change the labels the issue has.
fn labels_change(
    row: &PlanRowArg,
    issue: &PlanIssueDetail,
    facts: &PlanFacts,
) -> Result<Option<RowChange>, PlanRefusal> {
    let add = labels_of(facts, names(&row.labels_add))?;
    let remove = labels_of(facts, names(&row.labels_remove))?;
    let held = |label: &String| issue.labels.iter().any(|l| l.eq_ignore_ascii_case(label));
    let added: Vec<String> = add.into_iter().filter(|l| !held(l)).collect();
    let removed: Vec<String> = remove.into_iter().filter(|l| held(l)).collect();
    if added.is_empty() && removed.is_empty() {
        return Ok(None);
    }
    let after: Vec<String> = issue
        .labels
        .iter()
        .filter(|l| !removed.iter().any(|r| r.eq_ignore_ascii_case(l)))
        .cloned()
        .chain(added.iter().cloned())
        .collect();
    let mut labels = change("labels", json!(issue.labels), json!(after));
    labels.added = Some(added);
    labels.removed = Some(removed);
    Ok(Some(labels))
}
