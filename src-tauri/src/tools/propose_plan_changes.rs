//! `propose_plan_changes` — propose work on the delivery plan from a Map
//! discussion (`PPT-propose-plan-changes-tool.md`).
//!
//! One call carries a batch of rows: a new issue, a change to a plan issue, or
//! a new draft. The tool records the batch as one plan proposal and appends one
//! comment of the agent, through `crate::map_plan`. While a row of its
//! proposal waits, a call with `updates` changes that proposal in place. The
//! first successful call of a turn posts the card of the turn, and later calls
//! of the turn update it with no comment (PPT-FR-ZMBA). It writes nothing to
//! GitHub, creates no draft and no label, and does not change the read cache
//! of the plan (PPT-FR-WBYJ). The rows are applied only when the author accepts
//! them.

use std::collections::BTreeMap;
use std::sync::Arc;

use rig::tool::ToolErrorKind;
use serde::{Deserialize, Deserializer, Serialize};

use super::plan_refusals::{
    gate, log_recorded, log_refused, plan_is_unavailable, GateWords, PlanRefusal,
};
use super::{ToolRefusal, RATIONALE_NO_SPOKEN_PART};
use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::comments::{self, Participant};
use crate::delivery::github::plan_reads::{self, PlanFactsQuery};
use crate::delivery::DeliveryError;
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::map_plan::{self, MapPlanError, MapPlanProposal, ProposalComment, RowFields};
use updates::PlanUpdateArg;

pub mod rows;
pub mod turn_card;
pub mod updates;

pub use turn_card::{CardState, PlanTurnCard};

#[cfg(test)]
mod tests;

/// PPT-FR-STDD.
pub const NAME: &str = "propose_plan_changes";

/// PPT-FR-PMQL: the most rows of one call.
pub const MAX_ROWS: usize = 20;

/// PPT-FR-FFML: the fixed description of the contract surface.
pub const DESCRIPTION: &str = "Propose work on the delivery plan: new issues, changes to plan issues, and new drafts. One call carries a batch of rows. Each row is `create_issue`, `modify_issue`, or `new_draft`. The author accepts or rejects each row on its own, and nothing is written to GitHub and no draft is created until they accept. A new issue can set title, body, Type, milestone, parent, and labels. A change can set title, body, Type, milestone, parent, labels to add and to remove, a close reason, and the Status. Use only labels, Types, milestones, and Status options that exist: read them with `list_plan_issues`. A parent can be a plan issue (`parent_issue`), or the new issue of an earlier `create_issue` row of the same call (`parent_row`), for a new issue and for a change. Where a feature exists but is not fully built, propose a `new_draft` that finishes it, in place of an issue. Keep the discussion on one topic. Propose one complete step in one call: every row that the step needs, up to 20. A step of 5 to 10 related rows is normal. Do not split one step into several calls of a few rows, and do not propose the work of later steps before the author decided this one. While a row of your proposal still waits and the discussion asks for a different change, send `updates` in place of `rows`. Each update gives `action`: `change` with the `key` of a row and the full new row, `add` with a new row and a new `key`, or `withdraw` with the `key` of a row you no longer offer. A rejected row can be changed, and the author decides it again. An accepted or failed row can not be updated. Say what you propose and why, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn continues after a successful call: you can read issues and send `updates` to the same proposal. Only the first successful call of a turn posts its rationale. End the turn with a short reply. It is posted only where you changed the proposal after its rationale.";

const RATIONALE_DESCRIPTION: &str = "What you propose and why, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud. Required on the first successful call of a turn; a later call of the turn does not post it.";
const ROWS_DESCRIPTION: &str = "The rows of a new proposal, 1 to 20. The author decides each row on its own. Leave it out when you pass `updates`.";
const UPDATES_DESCRIPTION: &str = "Changes to your proposal that still has a waiting row, in place of `rows`, in the order they apply. A `change` gives the `key` of a row and the full new `row`, with the same `op`; the author decides it again. An `add` gives a new `key` and its `row`. A `withdraw` gives the `key` of a row you no longer offer, and no `row`. An accepted or failed row can not be updated.";
const ACTION_DESCRIPTION: &str =
    "`change` replaces a row, `add` adds a row, and `withdraw` takes a row out of the proposal.";
const UPDATE_KEY_DESCRIPTION: &str =
    "The `key` of the row to change or withdraw, or the new `key` of the row to add.";
const UPDATE_ROW_DESCRIPTION: &str = "The full row, with the fields of a row of `rows`. Its `key` is the `key` of the update. For `change` and `add` only.";
const KEY_DESCRIPTION: &str =
    "A name for the row that no other row of the call has. `parent_row` uses it.";
const OP_DESCRIPTION: &str = "`create_issue` proposes a new issue, `modify_issue` proposes a change to a plan issue, and `new_draft` proposes a new draft.";
const TITLE_DESCRIPTION: &str = "The title of the issue. Required for `create_issue`. On `modify_issue`, give it only to replace the title.";
const BODY_DESCRIPTION: &str = "The body of the issue in Markdown. For `new_draft`, the full prompt of the draft, which is required. On `modify_issue`, give it only to replace the whole body.";
const TYPE_DESCRIPTION: &str =
    "The Type of the issue. It must be a Type that the repository offers.";
const MILESTONE_DESCRIPTION: &str = "The exact title of a milestone of the plan. `null` removes the milestone of an existing issue.";
const PARENT_ISSUE_DESCRIPTION: &str = "The number of a plan issue to set as the parent. `null` removes the parent of an existing issue. Do not give it with `parent_row`.";
const PARENT_ROW_DESCRIPTION: &str = "The `key` of an earlier `create_issue` row of this call: the parent of a new issue, or the new parent of a changed issue. Do not give it with `parent_issue`.";
const LABELS_DESCRIPTION: &str =
    "Labels of the new issue. Each must already exist in the repository. `create_issue` only.";
const NUMBER_DESCRIPTION: &str = "The number of the plan issue to change. `modify_issue` only.";
const LABELS_ADD_DESCRIPTION: &str = "Labels to add. Each must already exist in the repository.";
const LABELS_REMOVE_DESCRIPTION: &str =
    "Labels to remove. Each must already exist in the repository.";
const CLOSE_REASON_DESCRIPTION: &str = "Closes the issue with this reason.";
const STATUS_DESCRIPTION: &str =
    "The Status of the issue on the Project. It must be an option of the Status field.";
const NAME_DESCRIPTION: &str = "The name of the new draft. `new_draft` only. Its prompt is `body`.";

// PPT contract surface, "Refusals".
pub const NOT_A_MAP: &str = "This conversation is not a Map discussion, so there is no plan to change. Say what you would change instead.";
pub const PLAN_UNAVAILABLE: &str = "The project has no valid GitHub Project for its plan. Tell the author; asking again will not succeed.";
pub const CONVERSATION_RESOLVED: &str =
    "This conversation is resolved and takes no further comments, so nothing was proposed.";
pub const CARD_OPEN: &str = "A proposal of yours still waits for the decisions of the author. To change it, send `updates` in place of `rows`. Otherwise wait for the decisions, then propose again. Nothing was proposed.";
pub const ONE_CARD_A_TURN: &str = "This turn already posted a plan proposal. Change it with `updates`, or end your turn with a short reply. Nothing was proposed.";
pub const BLANK_RATIONALE: &str =
    "Say what you propose and why. The author reads this beside the card.";
pub const NOT_RECORDED: &str = "The proposal could not be recorded.";

const WORDS: GateWords = GateWords { not_a_map: NOT_A_MAP, plan_unavailable: PLAN_UNAVAILABLE };

/// A value that can be absent, `null`, or given: `None`, `Some(None)`, or
/// `Some(Some(_))`.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// One row, as the model composed it. Unknown fields are ignored (TLC-FR-07).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlanRowArg {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub op: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub milestone: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub parent_issue: Option<Option<i64>>,
    #[serde(default)]
    pub parent_row: Option<String>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub number: Option<i64>,
    #[serde(default)]
    pub labels_add: Option<Vec<String>>,
    #[serde(default)]
    pub labels_remove: Option<Vec<String>>,
    #[serde(default)]
    pub close_reason: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// The arguments. There is no parameter that names a discussion, a repository,
/// a conversation, or a path (PPT-FR-VFOX).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProposePlanChangesArgs {
    /// PPT-FR-NXPZ: required before the turn has a card.
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub rows: Option<Vec<PlanRowArg>>,
    /// PPT-FR-STLY: the updates of the open proposal, in place of `rows`.
    #[serde(default)]
    pub updates: Option<Vec<PlanUpdateArg>>,
}

/// The data object of the contract surface: one shape for a new proposal, and
/// one for an update (PPT contract surface, "Output").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ProposePlanChangesOutput {
    Proposed { proposed: bool, rows: usize },
    Updated { updated: bool, changed: usize, added: usize, withdrawn: usize },
}

/// TLC-FR-06: the JSON Schema of the contract surface.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
            "rows": {
                "type": "array",
                "description": ROWS_DESCRIPTION,
                "minItems": 1,
                "maxItems": MAX_ROWS,
                "items": row_schema(),
            },
            "updates": {
                "type": "array",
                "description": UPDATES_DESCRIPTION,
                "minItems": 1,
                "maxItems": MAX_ROWS,
                "items": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": [updates::ACTION_CHANGE, updates::ACTION_ADD, updates::ACTION_WITHDRAW],
                            "description": ACTION_DESCRIPTION,
                        },
                        "key": { "type": "string", "description": UPDATE_KEY_DESCRIPTION },
                        "row": described(row_schema(), UPDATE_ROW_DESCRIPTION),
                    },
                    "required": ["action", "key"],
                },
            },
        },
    })
}

/// `schema` with its description.
fn described(mut schema: serde_json::Value, description: &str) -> serde_json::Value {
    schema["description"] = serde_json::json!(description);
    schema
}

/// TLC-FR-06: the schema of one row, for `rows` and for the `row` of an update.
fn row_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "key": { "type": "string", "description": KEY_DESCRIPTION },
            "op": {
                "type": "string",
                "enum": [rows::OP_CREATE, rows::OP_MODIFY, rows::OP_DRAFT],
                "description": OP_DESCRIPTION,
            },
            "title": { "type": "string", "description": TITLE_DESCRIPTION },
            "body": { "type": "string", "description": BODY_DESCRIPTION },
            "type": { "type": "string", "description": TYPE_DESCRIPTION },
            "milestone": { "type": ["string", "null"], "description": MILESTONE_DESCRIPTION },
            "parent_issue": { "type": ["integer", "null"], "description": PARENT_ISSUE_DESCRIPTION },
            "parent_row": { "type": "string", "description": PARENT_ROW_DESCRIPTION },
            "labels": { "type": "array", "items": { "type": "string" }, "description": LABELS_DESCRIPTION },
            "number": { "type": "integer", "description": NUMBER_DESCRIPTION },
            "labels_add": { "type": "array", "items": { "type": "string" }, "description": LABELS_ADD_DESCRIPTION },
            "labels_remove": { "type": "array", "items": { "type": "string" }, "description": LABELS_REMOVE_DESCRIPTION },
            "close_reason": { "type": "string", "enum": ["completed", "not_planned"], "description": CLOSE_REASON_DESCRIPTION },
            "status": { "type": "string", "description": STATUS_DESCRIPTION },
            "name": { "type": "string", "description": NAME_DESCRIPTION },
        },
        "required": ["key", "op"],
    })
}

fn not_recorded() -> PlanRefusal {
    PlanRefusal::from(ToolRefusal::NotRecordedWith(NOT_RECORDED))
}

/// A failure of a read of the plan, in the terms of this tool.
fn plan_refusal(error: &DeliveryError) -> PlanRefusal {
    if error.code == crate::delivery::ERR_NO_PROJECT_OPEN {
        return ToolRefusal::NoProjectOpen.into();
    }
    if plan_is_unavailable(error) {
        return PlanRefusal::new(
            ToolErrorKind::NotFound,
            "plan_unavailable",
            false,
            PLAN_UNAVAILABLE,
        );
    }
    not_recorded()
}

fn card_open() -> PlanRefusal {
    PlanRefusal::new(ToolErrorKind::PermissionDenied, "card_open", true, CARD_OPEN)
}

fn one_card_a_turn() -> PlanRefusal {
    PlanRefusal::invalid("one_card_a_turn", ONE_CARD_A_TURN)
}

fn resolved() -> PlanRefusal {
    PlanRefusal::new(
        ToolErrorKind::PermissionDenied,
        "conversation_resolved",
        false,
        CONVERSATION_RESOLVED,
    )
}

/// The typed errors of `crate::map_plan` in this tool's terms (PPT-FR-AYQT,
/// PPT-FR-BSSO, PPT-FR-UZEK).
fn record_refusal(error: &MapPlanError) -> PlanRefusal {
    match error.code.as_str() {
        "proposal_open" => card_open(),
        comments::ERR_DISCUSSION_RESOLVED => resolved(),
        _ => not_recorded(),
    }
}

/// PPT-FR-STDD: `propose_plan_changes` as a `rig` portable tool. `card` is the
/// plan card of the turn, which the loop reads when the turn ends
/// (CVL-FR-KYBE).
pub struct ProposePlanChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    card: Arc<PlanTurnCard>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposePlanChangesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// PPT-FR-VFOX: bound to the turn's Map discussion and agent.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        card: Arc<PlanTurnCard>,
    ) -> Self {
        ProposePlanChangesTool { app, roots, origin, author, card, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// PPT-FR-DGAA: the call with its logging. The fields are counts and ids,
    /// and never a title, a body, a label, a draft name, or the rationale.
    async fn run(
        &self,
        args: ProposePlanChangesArgs,
    ) -> Result<ProposePlanChangesOutput, PlanRefusal> {
        let mut fields = crate::log_fields! {
            "discussionId" => self.origin.discussion_id(),
            "rows" => args.rows.as_ref().map_or(0, Vec::len),
            "updates" => args.updates.as_ref().map_or(0, Vec::len),
        };
        let outcome = self.propose(args).await;
        match &outcome {
            Ok((_, counts)) => {
                for (name, count) in counts {
                    fields.insert((*name).to_string(), serde_json::json!(count));
                }
                log_recorded(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => {
                // PPT-FR-DGAA: the count of refused rows, for **rows refused**.
                if refusal.code == rows::ROWS_REFUSED {
                    fields.insert("rowsRefused".to_string(), rows::refused_count(refusal).into());
                }
                log_refused(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome.map(|(output, _)| output)
    }

    /// The count of rows of each kind that a proposal holds.
    fn kinds(rows: &[map_plan::MapPlanRow]) -> BTreeMap<&'static str, usize> {
        let mut kinds = BTreeMap::new();
        for row in rows {
            let kind = match row.fields {
                RowFields::CreateIssue(_) => "createRows",
                RowFields::ModifyIssue(_) => "modifyRows",
                RowFields::NewDraft(_) => "draftRows",
            };
            *kinds.entry(kind).or_insert(0) += 1;
        }
        kinds
    }

    /// PPT-FR-CXFG: the rationale as the model wrote it, less its citation
    /// markers, with its spoken part in place.
    fn comment_of(&self, rationale: &str) -> ProposalComment {
        ProposalComment {
            body: crate::agent_conversations::strip_citation_markers(rationale).into_owned(),
            author: self.author.clone(),
            spoken_original: None,
        }
    }

    /// PPT-FR-NXPZ.
    fn check_rationale(rationale: &str) -> Result<(), PlanRefusal> {
        if rationale.trim().is_empty() {
            return Err(PlanRefusal::invalid("rationale_blank", BLANK_RATIONALE));
        }
        if comments::spoken_part(rationale).is_none() {
            return Err(PlanRefusal::invalid("no_spoken_part", RATIONALE_NO_SPOKEN_PART));
        }
        Ok(())
    }

    /// The facts of the plan that the rows need, read from GitHub.
    async fn facts(&self, needs: rows::Needs) -> Result<plan_reads::PlanFacts, PlanRefusal> {
        plan_reads::plan_facts(
            self.app.clone(),
            PlanFactsQuery {
                need_types: needs.types,
                need_labels: needs.labels,
                numbers: needs.numbers,
            },
        )
        .await
        .map_err(|e| plan_refusal(&e))
    }

    /// The open proposal of the discussion. The store reads are synchronous and
    /// an accept can hold the lock for a while: they run off the async runtime.
    async fn open_proposal(&self) -> Result<Option<MapPlanProposal>, PlanRefusal> {
        let (app, id) = (self.app.clone(), self.origin.discussion_id().to_string());
        crate::project_scope::context::spawn_blocking(move || {
            map_plan::read_open_proposal(&app, &id)
        })
        .await
        .map_err(|_| not_recorded())
    }

    async fn propose(
        &self,
        args: ProposePlanChangesArgs,
    ) -> Result<(ProposePlanChangesOutput, BTreeMap<&'static str, usize>), PlanRefusal> {
        gate(&self.app, &self.origin, &WORDS)?;
        // PPT-FR-MYQS.
        let form = match (args.rows, args.updates) {
            (Some(rows), None) => Ok(rows),
            (None, Some(updates)) => Err(updates),
            _ => return Err(updates::not_one_form()),
        };
        // PPT-FR-BSSO.
        let discussion_id = self.origin.discussion_id();
        let folded =
            comments::locate_discussion_of(&self.roots.store, &self.origin.target, discussion_id);
        if folded.is_some_and(|d| d.resolved) {
            return Err(resolved());
        }
        let card = self.card.snapshot();
        // PPT-FR-ZMBA: a turn posts one card.
        if form.is_ok() && card.proposal_id.is_some() {
            return Err(one_card_a_turn());
        }
        let open = self.open_proposal().await?;
        // PPT-FR-NXPZ: the rationale of the first successful call of a turn.
        if card.proposal_id.is_none() {
            Self::check_rationale(&args.rationale)?;
        }
        match form {
            Ok(rows) => {
                // PPT-FR-AYQT.
                if open.is_some() {
                    return Err(card_open());
                }
                self.record(&args.rationale, &rows).await
            }
            // PPT-FR-STLY, PPT-FR-ZMBA: a later call changes the card of the
            // turn only.
            Err(updates) => {
                let open = open
                    .filter(|o| card.proposal_id.as_ref().is_none_or(|id| *id == o.id))
                    .ok_or_else(updates::no_open_proposal)?;
                let comment = card.proposal_id.is_none().then(|| self.comment_of(&args.rationale));
                self.update(comment, &updates, &open).await
            }
        }
    }

    /// PPT-FR-PMQL to PPT-FR-UZEK: record a new proposal.
    async fn record(
        &self,
        rationale: &str,
        rows: &[PlanRowArg],
    ) -> Result<(ProposePlanChangesOutput, BTreeMap<&'static str, usize>), PlanRefusal> {
        let (needs, mut refused) = rows::check_shape(rows)?;
        // PPT-FR-ELJI: the facts are read only for the rows that the shape
        // checks let through.
        let built = if refused.len() < rows.len() {
            let facts = self.facts(needs).await?;
            rows::build_rows(rows, &facts, &mut refused)
        } else {
            Vec::new()
        };
        rows::refusal_of(refused, rows::row_line)?;
        let kinds = Self::kinds(&built);
        let count = built.len();
        let comment = self.comment_of(rationale);
        let (app, id) = (self.app.clone(), self.origin.discussion_id().to_string());
        let proposal = crate::project_scope::context::spawn_blocking(move || {
            map_plan::record_map_plan_proposal(&app, &id, built, comment)
        })
        .await
        .map_err(|_| not_recorded())?
        .map_err(|e| record_refusal(&e))?;
        // PPT-FR-ZMBA: set only after the record and its comment both landed.
        self.card.set(&proposal.id);
        Ok((ProposePlanChangesOutput::Proposed { proposed: true, rows: count }, kinds))
    }

    /// PPT-FR-STLY to PPT-FR-ISZR, PPT-FR-IRFE: update the open proposal in
    /// place. `comment` is the comment of the first successful call of a turn,
    /// and a later call of the turn has none.
    async fn update(
        &self,
        comment: Option<ProposalComment>,
        updates: &[PlanUpdateArg],
        open: &MapPlanProposal,
    ) -> Result<(ProposePlanChangesOutput, BTreeMap<&'static str, usize>), PlanRefusal> {
        let (checked, needs, mut refused) = updates::check_updates(updates, open)?;
        let facts = self.facts(needs).await?;
        let built = updates::build_updates(&checked, updates.len(), &facts, &mut refused);
        rows::refusal_of(refused, updates::update_line)?;
        let new_rows: Vec<map_plan::MapPlanRow> = built
            .iter()
            .filter_map(|u| match u {
                map_plan::MapPlanUpdate::Change(row) | map_plan::MapPlanUpdate::Add(row) => {
                    Some(row.clone())
                }
                map_plan::MapPlanUpdate::Withdraw(_) => None,
            })
            .collect();
        let mut counts = Self::kinds(&new_rows);
        let (changed, added, withdrawn) = updates::counts(&checked);
        counts.insert("changeUpdates", changed);
        counts.insert("addUpdates", added);
        counts.insert("withdrawUpdates", withdrawn);
        let silent = comment.is_none();
        counts.insert("silentUpdates", usize::from(silent));
        let (app, id) = (self.app.clone(), self.origin.discussion_id().to_string());
        let proposal = crate::project_scope::context::spawn_blocking(move || {
            map_plan::update_map_plan_proposal(&app, &id, built, comment)
        })
        .await
        .map_err(|_| not_recorded())?
        .map_err(|e| updates::update_refusal(&e).unwrap_or_else(|| record_refusal(&e)))?;
        // PPT-FR-ZMBA, PPT-FR-IRFE: set only after the update landed. The
        // first call of a turn makes its proposal the card, and a later call
        // adds its counts.
        if silent {
            self.card.add(changed, added, withdrawn);
        } else {
            self.card.set(&proposal.id);
        }
        let output = ProposePlanChangesOutput::Updated { updated: true, changed, added, withdrawn };
        Ok((output, counts))
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposePlanChangesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = ProposePlanChangesArgs;
    type Output = ProposePlanChangesOutput;
    type Error = PlanRefusal;

    fn description(&self) -> String {
        DESCRIPTION.to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        parameters()
    }

    fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
        error.to_execution_error()
    }

    fn call(
        &self,
        arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        self.run(arguments)
    }
}
