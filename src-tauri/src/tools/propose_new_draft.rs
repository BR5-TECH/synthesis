//! `propose_new_draft` — propose one new draft of a decomposition
//! (`PND-propose-new-draft-tool.md`).
//!
//! The tool records one new-draft proposal beside the draft under discussion
//! and posts one comment that refers to it, through
//! `crate::new_draft_proposals`. It creates no draft, changes nothing of the
//! original draft, and can not end the decomposition (PND-FR-UQCD).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    RATIONALE_NO_SPOKEN_PART,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant, ThreadRef};
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::new_draft_proposals::{self as nd, NewDraftInput, NewDraftUpdate};

#[cfg(test)]
mod tests;

/// PND-FR-UXPK.
pub const NAME: &str = "propose_new_draft";

/// PND-FR-XJHL: the fixed description of the contract surface.
pub const DESCRIPTION: &str = "Propose one new draft that replaces a part of the draft under discussion, when the author has decided to decompose this draft into two or more new drafts that do the same job one after another. Use it only after the author asked for a decomposition and you understand how they want it split; ask them first where you are not sure. Each call proposes **one** draft. Give its `name`, its full prompt in `body`, its `position` counted from 1, and the `total` number of new drafts you expect now. The author accepts, rejects, or discusses it, and nothing is created until they accept. Propose the next draft only after the author decided the one before it, and use what they said about it. You can propose a draft again at the same position after a rejection. To change the draft you proposed while it is still open, pass `update` with `action` `change` and the fields to replace. To take it back, pass `update` with `action` `withdraw`. You can not end the decomposition on your own: the author confirms it on the card, or tells you to confirm it, and then you use `decide_draft_changes`. Say why this draft is the right next part, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn ends when the proposal is recorded.";

const RATIONALE_DESCRIPTION: &str = "Why this draft is the right next part, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";
const NAME_DESCRIPTION: &str =
    "The name of the new draft. With `update` and `change`, give it only to replace the name.";
const BODY_DESCRIPTION: &str = "The full prompt of the new draft, in Markdown, as the author will work on it. With `update` and `change`, give it only to replace the whole prompt.";
const POSITION_DESCRIPTION: &str = "The place of this draft in the sequence, counted from 1.";
const TOTAL_DESCRIPTION: &str = "The number of new drafts you expect the decomposition to have now. It can change in a later call.";
const UPDATE_DESCRIPTION: &str = "Changes your proposal that is still open. `change` replaces the fields you give. `withdraw` takes the proposal back and gives no other field.";

// PND contract surface, "Refusals".
pub const NOT_A_DRAFT: &str = "This conversation is not about a draft, so there is no draft here to decompose. Say how you would split it instead; asking again will not succeed.";
pub const DRAFT_ARCHIVED: &str =
    "This draft is archived, so it can not be decomposed. Tell the author; asking again will not succeed.";
pub const DRAFT_GRADUATED: &str =
    "This draft is graduated, so it can not be decomposed. Tell the author; asking again will not succeed.";
pub const DRAFT_LOCKED: &str = "A graduation run holds this draft, so it can not be decomposed now. Tell the author; asking again will not succeed.";
pub const DRAFT_GITHUB_TASK: &str = "This draft mirrors a GitHub task, so it can not be decomposed. Tell the author; asking again will not succeed.";
pub const DECOMPOSITION_CONFIRMED: &str = "The author confirmed the decomposition of this draft, so it takes no new proposals. Tell the author; asking again will not succeed.";
pub const PROPOSAL_OPEN: &str = "A new draft you proposed is still waiting on the author. Wait for their decision, or change it with `update`. Nothing was proposed.";
pub const CONVERSATION_RESOLVED: &str = "This conversation is resolved and takes no further comments, so nothing was proposed. Answer with what you already have; proposing again will not succeed.";
pub const BLANK_NAME: &str = "Give the new draft a name.";
pub const BLANK_BODY: &str = "Give the full prompt of the new draft in `body`.";
pub const BAD_POSITION: &str = "`position` starts at 1 and can not be more than `total`.";
pub const BLANK_RATIONALE: &str =
    "Say why this draft is the right next part. The author reads this beside the proposal.";
pub const NO_OPEN_PROPOSAL: &str = "No new draft you proposed is open on this draft, so there is nothing to update. Propose a new draft without `update`.";
pub const UPDATE_MALFORMED: &str = "`update` gives `action`: `change` with at least one of `name`, `body`, `position`, and `total`, or `withdraw` with none of them.";
pub const NO_CHANGE: &str =
    "The fields you gave are the ones the open proposal already has, so there is nothing to decide.";
pub const NOT_RECORDED: &str = "The proposed draft could not be recorded.";

/// PND-FR-YPCH: what a second proposal in one reply is told.
pub const ONE_AT_A_TIME: &str = "Only one new draft can be proposed at a time, so this one was not recorded. Propose the next draft after the author decided this one.";

/// The `update` argument.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateArg {
    #[serde(default)]
    pub action: Option<String>,
}

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07). There is no parameter that names a draft (PND-FR-JNGE).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeNewDraftArgs {
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub position: Option<i64>,
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub update: Option<UpdateArg>,
}

/// PND-FR-KGRN: the data object of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposeNewDraftOutput {
    pub proposed: bool,
}

/// TLC-FR-06: the JSON Schema of the contract surface.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
            "name": { "type": "string", "description": NAME_DESCRIPTION },
            "body": { "type": "string", "description": BODY_DESCRIPTION },
            "position": { "type": "integer", "description": POSITION_DESCRIPTION },
            "total": { "type": "integer", "description": TOTAL_DESCRIPTION },
            "update": {
                "type": "object",
                "description": UPDATE_DESCRIPTION,
                "properties": {
                    "action": { "type": "string", "enum": ["change", "withdraw"] },
                },
                "required": ["action"],
            },
        },
        "required": ["rationale"],
    })
}

fn denied(message: &'static str, code: &'static str, retryable: bool) -> ToolRefusal {
    ToolRefusal::Denied { message, code, retryable }
}

/// The typed errors of `crate::new_draft_proposals` in this tool's terms.
fn refusal_for(error: &str) -> ToolRefusal {
    match error {
        nd::ERR_DRAFT_ARCHIVED => denied(DRAFT_ARCHIVED, "draft_archived", false),
        nd::ERR_DRAFT_GRADUATED => denied(DRAFT_GRADUATED, "draft_graduated", false),
        nd::ERR_DRAFT_LOCKED => denied(DRAFT_LOCKED, "draft_locked_by_graduation", false),
        nd::ERR_DRAFT_GITHUB_SHADOW => denied(DRAFT_GITHUB_TASK, "draft_github_shadow", false),
        nd::ERR_DECOMPOSITION_CONFIRMED => {
            denied(DECOMPOSITION_CONFIRMED, "decomposition_confirmed", false)
        }
        nd::ERR_PROPOSAL_PENDING => denied(PROPOSAL_OPEN, "proposal_open", true),
        nd::ERR_DISCUSSION_RESOLVED => {
            denied(CONVERSATION_RESOLVED, "conversation_resolved", false)
        }
        nd::ERR_BLANK_NAME => ToolRefusal::InvalidArguments(BLANK_NAME),
        nd::ERR_BLANK_BODY => ToolRefusal::InvalidArguments(BLANK_BODY),
        nd::ERR_BAD_POSITION => ToolRefusal::InvalidArguments(BAD_POSITION),
        nd::ERR_NO_OPEN_PROPOSAL => ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL),
        nd::ERR_NO_CHANGE => ToolRefusal::InvalidArguments(NO_CHANGE),
        _ => ToolRefusal::NotRecordedWith(NOT_RECORDED),
    }
}

/// The stable code of an invalid-argument refusal, for the log (PND-FR-HGCC).
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    let ToolRefusal::InvalidArguments(message) = refusal else {
        return None;
    };
    Some(match *message {
        BLANK_NAME => "blank_name",
        BLANK_BODY => "blank_body",
        BAD_POSITION => "bad_position",
        BLANK_RATIONALE => "rationale_blank",
        RATIONALE_NO_SPOKEN_PART => "no_spoken_part",
        NO_OPEN_PROPOSAL => "no_open_proposal",
        UPDATE_MALFORMED => "update_malformed",
        NO_CHANGE => "no_change",
        _ => "other",
    })
}

/// PND-FR-UXPK: `propose_new_draft` as a `rig` portable tool. `proposed` is
/// how the loop learns that a proposal went out and the turn ends (CVL-FR-15).
pub struct ProposeNewDraftTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposeNewDraftTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// PND-FR-JNGE: bound to the turn's draft, conversation, and agent.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposeNewDraftTool { app, roots, origin, author, proposed, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// PND-FR-HGCC: the call with its logging. The fields are numbers, ids,
    /// and kinds, and never the name, the body, or the rationale.
    fn run(&self, args: ProposeNewDraftArgs) -> Result<ProposeNewDraftOutput, ToolRefusal> {
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "position" => args.position,
            "total" => args.total,
            "bodyChars" => args.body.as_deref().map_or(0, |b| b.chars().count()),
            "rationaleChars" => args.rationale.trim().chars().count(),
        };
        if let Some(draft_id) = self.origin.draft_id() {
            fields.insert("draftId".into(), serde_json::json!(draft_id));
        }
        if let Some(update) = &args.update {
            // PND-FR-HGCC: a kind, never the text the model wrote.
            let action = match update.action.as_deref() {
                Some("change") => "change",
                Some("withdraw") => "withdraw",
                _ => "invalid",
            };
            fields.insert("updateAction".into(), serde_json::json!(action));
        }
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".into(), serde_json::json!(code));
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn propose(&self, args: ProposeNewDraftArgs) -> Result<ProposeNewDraftOutput, ToolRefusal> {
        require_open_project(&self.app)?;
        // PND-FR-YFES.
        let Some(draft_id) = self.origin.draft_id() else {
            return Err(denied(NOT_A_DRAFT, "not_a_draft_conversation", false));
        };
        let thread_id = self.origin.discussion_id();
        // PND-FR-DDUJ: for both forms.
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(BLANK_RATIONALE));
        }
        if comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }
        let mut file_rel = String::new();
        let target = self.target(draft_id, thread_id, &mut file_rel)?;
        let worktree = &self.roots.worktree;
        let store = &self.roots.store;
        match &args.update {
            None => {
                // PND-FR-INGY / PND-FR-AUFP / PND-FR-AFEX: the checks the core
                // applies are the refusals of the contract surface.
                let input = NewDraftInput {
                    draft_id,
                    name: args.name.as_deref().unwrap_or(""),
                    body: args.body.as_deref().unwrap_or(""),
                    position: args.position.unwrap_or(0),
                    total: args.total.unwrap_or(0),
                    rationale: &args.rationale,
                    agent: &self.author,
                    target,
                    thread_id,
                };
                nd::record_new_draft_proposal(&self.app, worktree, store, input)
                    .map_err(|e| refusal_for(&e))?;
            }
            Some(update) => {
                let update = Self::update_of(update, &args)?;
                nd::update_new_draft_proposal(
                    &self.app,
                    worktree,
                    store,
                    draft_id,
                    update,
                    &args.rationale,
                    &self.author,
                    (target, thread_id),
                )
                .map_err(|e| refusal_for(&e))?;
            }
        }
        // CVL-FR-15: set only after the record and its comment both landed.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeNewDraftOutput { proposed: true })
    }

    /// PND-FR-HVWR / PND-FR-WQJQ / PND-FR-FMRO: the update the model asked for.
    fn update_of(
        update: &UpdateArg,
        args: &ProposeNewDraftArgs,
    ) -> Result<NewDraftUpdate, ToolRefusal> {
        let carries = args.name.is_some()
            || args.body.is_some()
            || args.position.is_some()
            || args.total.is_some();
        match update.action.as_deref() {
            Some("change") if carries => Ok(NewDraftUpdate::Change {
                name: args.name.clone(),
                body: args.body.clone(),
                position: args.position,
                total: args.total,
            }),
            Some("withdraw") if !carries => Ok(NewDraftUpdate::Withdraw),
            _ => Err(ToolRefusal::InvalidArguments(UPDATE_MALFORMED)),
        }
    }

    /// The conversation the comment is appended into: the whole-draft
    /// discussion, or the fragment discussion of the draft's prompt.
    fn target<'a>(
        &'a self,
        draft_id: &'a str,
        thread_id: &str,
        file_rel: &'a mut String,
    ) -> Result<ThreadRef<'a>, ToolRefusal> {
        if self.origin.fragment_target.is_none() {
            return Ok(ThreadRef::discussion(draft_id));
        }
        let found =
            comments::locate_discussion_of(&self.roots.store, &self.origin.target, thread_id)
                .and_then(|d| d.fragment_target.map(|f| f.path))
                .ok_or(ToolRefusal::NotRecordedWith(NOT_RECORDED))?;
        *file_rel = found;
        Ok(ThreadRef::draft_file(draft_id, file_rel))
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposeNewDraftTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = ProposeNewDraftArgs;
    type Output = ProposeNewDraftOutput;
    type Error = ToolRefusal;

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
        // PND-FR-UQCD: it records and returns; nothing waits.
        std::future::ready(self.run(arguments))
    }
}
