//! `propose_plan_changes` — propose work on the delivery plan from a Map
//! discussion (`PPT-propose-plan-changes-tool.md`).
//!
//! One call carries a batch of rows: a new issue, a change to a plan issue, or
//! a new draft. The tool records the batch as one plan proposal and appends one
//! comment of the agent, through `crate::map_plan`. It writes nothing to
//! GitHub, creates no draft and no label, and does not change the read cache
//! of the plan (PPT-FR-WBYJ). The rows are applied only when the author accepts
//! them.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::map_plan::{self, MapPlanError, ProposalComment, RowFields};

pub mod rows;

#[cfg(test)]
mod tests;

/// PPT-FR-STDD.
pub const NAME: &str = "propose_plan_changes";

/// PPT-FR-PMQL: the most rows of one call.
pub const MAX_ROWS: usize = 20;

/// PPT-FR-FFML: the fixed description of the contract surface.
pub const DESCRIPTION: &str = "Propose work on the delivery plan: new issues, changes to plan issues, and new drafts. One call carries a batch of rows. Each row is `create_issue`, `modify_issue`, or `new_draft`. The author accepts or rejects each row on its own, and nothing is written to GitHub and no draft is created until they accept. A new issue can set title, body, Type, milestone, parent, and labels. A change can set title, body, Type, milestone, parent, labels to add and to remove, a close reason, and the Status. Use only labels, Types, milestones, and Status options that exist: read them with `list_plan_issues`. A parent can be a plan issue (`parent_issue`) or an earlier new issue of the same call (`parent_row`). Where a feature exists but is not fully built, propose a `new_draft` that finishes it, in place of an issue. Do not propose all work at once. Keep the discussion on one topic, propose one step, and propose again after the author decided every row. Say what you propose and why, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn ends when the proposal is recorded.";

const RATIONALE_DESCRIPTION: &str = "What you propose and why, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";
const ROWS_DESCRIPTION: &str =
    "The rows of the proposal, 1 to 20. The author decides each row on its own.";
const KEY_DESCRIPTION: &str =
    "A name for the row that no other row of the call has. `parent_row` uses it.";
const OP_DESCRIPTION: &str = "`create_issue` proposes a new issue, `modify_issue` proposes a change to a plan issue, and `new_draft` proposes a new draft.";
const TITLE_DESCRIPTION: &str = "The title of the issue. Required for `create_issue`. On `modify_issue`, give it only to replace the title.";
const BODY_DESCRIPTION: &str = "The body of the issue in Markdown. For `new_draft`, the full prompt of the draft, which is required. On `modify_issue`, give it only to replace the whole body.";
const TYPE_DESCRIPTION: &str =
    "The Type of the issue. It must be a Type that the repository offers.";
const MILESTONE_DESCRIPTION: &str = "The exact title of a milestone of the plan. `null` removes the milestone of an existing issue.";
const PARENT_ISSUE_DESCRIPTION: &str = "The number of a plan issue to set as the parent. `null` removes the parent of an existing issue. Do not give it with `parent_row`.";
const PARENT_ROW_DESCRIPTION: &str = "The `key` of an earlier `create_issue` row of this call to set as the parent. Do not give it with `parent_issue`.";
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
pub const CARD_OPEN: &str = "A proposal of yours still waits for the decisions of the author. Wait for them, then propose again. Nothing was proposed.";
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
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub rows: Vec<PlanRowArg>,
}

/// The data object of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposePlanChangesOutput {
    pub proposed: bool,
    pub rows: usize,
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
                "items": {
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
                },
            },
        },
        "required": ["rationale", "rows"],
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

/// PPT-FR-STDD: `propose_plan_changes` as a `rig` portable tool. `proposed` is
/// how the loop learns that a proposal went out and the turn ends (CVL-FR-15).
pub struct ProposePlanChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
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
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposePlanChangesTool { app, roots, origin, author, proposed, buffer: &BUFFER }
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
            "rows" => args.rows.len(),
        };
        let outcome = self.propose(args).await;
        match &outcome {
            Ok((_, kinds)) => {
                for (kind, count) in kinds {
                    fields.insert((*kind).to_string(), serde_json::json!(count));
                }
                log_recorded(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => log_refused(&self.app, self.buffer, NAME, refusal, fields),
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

    async fn propose(
        &self,
        args: ProposePlanChangesArgs,
    ) -> Result<(ProposePlanChangesOutput, BTreeMap<&'static str, usize>), PlanRefusal> {
        gate(&self.app, &self.origin, &WORDS)?;
        let discussion_id = self.origin.discussion_id();
        // PPT-FR-BSSO.
        let folded =
            comments::locate_discussion_of(&self.roots.store, &self.origin.target, discussion_id);
        if folded.is_some_and(|d| d.resolved) {
            return Err(resolved());
        }
        // PPT-FR-AYQT.
        if map_plan::open_row_exists(&self.app, discussion_id) {
            return Err(card_open());
        }
        // PPT-FR-NXPZ.
        if args.rationale.trim().is_empty() {
            return Err(PlanRefusal::invalid("rationale_blank", BLANK_RATIONALE));
        }
        if comments::spoken_part(&args.rationale).is_none() {
            return Err(PlanRefusal::invalid("no_spoken_part", RATIONALE_NO_SPOKEN_PART));
        }
        let needs = rows::check_shape(&args.rows)?;
        let facts = plan_reads::plan_facts(
            self.app.clone(),
            PlanFactsQuery {
                need_types: needs.types,
                need_labels: needs.labels,
                numbers: needs.numbers,
                label_numbers: needs.label_numbers,
            },
        )
        .await
        .map_err(|e| plan_refusal(&e))?;
        let built = rows::build_rows(&args.rows, &facts)?;
        let kinds = Self::kinds(&built);
        let count = built.len();
        // PPT-FR-CXFG: the rationale as the model wrote it, less its citation
        // markers, with its spoken part in place.
        let comment = ProposalComment {
            body: crate::agent_conversations::strip_citation_markers(&args.rationale).into_owned(),
            author: self.author.clone(),
            spoken_original: None,
        };
        map_plan::record_map_plan_proposal(&self.app, discussion_id, built, comment)
            .map_err(|e| record_refusal(&e))?;
        // CVL-FR-15: set only after the record and its comment both landed.
        self.proposed.store(true, Ordering::SeqCst);
        Ok((ProposePlanChangesOutput { proposed: true, rows: count }, kinds))
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

/// PPT-FR-PRPX: what a second proposal in one reply is told.
pub const ONE_AT_A_TIME: &str = "Only one plan proposal can be recorded in a turn, so this one was not recorded. Propose again after the author decided the rows.";
