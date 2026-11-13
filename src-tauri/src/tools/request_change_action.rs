//! `request_change_action` — ask the author to apply or rework the OpenSpec
//! change under discussion (`RCA-request-change-action-tool.md`).
//!
//! A run writes the stream and costs time and tokens, so the agent does not
//! start one. The tool records one change action request and posts it into
//! the turn's conversation through `crate::change_action_requests`
//! (RCA-FR-QEWH). Only the author's confirmation starts the run. The change,
//! the conversation, and the agent are bound at construction (RCA-FR-QANG).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, FixedKind, ToolRefusal,
};

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::change_action_requests::{self as car, ChangeAction, NewActionRequest};
use crate::change_proposals::{AppSites, SiteResolver};
use crate::comments::Participant;
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};

/// RCA-FR-UZJK.
pub const NAME: &str = "request_change_action";

/// RCA-FR-PTAK: the fixed text of the contract surface.
pub const DESCRIPTION: &str = "Ask the author to start a run on the OpenSpec change under discussion. Use `apply` when the change is ready to build: an apply run implements its tasks and commits the work on the stream. Use `rework` when the change itself must be revised: a rework run revises the files of the change on your instruction, and its result comes back as a proposal that the author decides. You cannot start a run: this posts a request, and the author confirms or declines it. For `rework`, write in `instruction` what the revision must do; give no `instruction` for `apply`. Tell the author in `rationale` why you ask. Put one `<spoken>…</spoken>` tag in `rationale`: one short sentence that a voice can read aloud. Your turn ends when the request is posted. Make one request at a time.";

const ACTION_DESCRIPTION: &str =
    "`apply` to implement the tasks of the change, or `rework` to revise the change itself.";
const INSTRUCTION_DESCRIPTION: &str = "For `rework` alone: what the revision must do, in the words a reviser can act on. Leave it out for `apply`.";
const RATIONALE_DESCRIPTION: &str = "Why you ask, in one or two sentences. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud.";

// RCA contract surface, "Refusals".
pub const BAD_ACTION: &str = "`action` is `apply` or `rework`.";
pub const NO_INSTRUCTION: &str =
    "A rework needs an instruction. Write in `instruction` what the revision must do.";
pub const APPLY_TAKES_NO_INSTRUCTION: &str = "An apply run builds the change as it stands and takes no instruction. Leave `instruction` out, or ask for `rework` to change the change first.";
pub const BLANK_RATIONALE: &str = "Tell the author in `rationale` why you ask.";
pub const NO_SPOKEN_PART: &str = "The rationale has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `rationale`.";
pub const NOT_A_CHANGE_CONVERSATION: &str =
    "This conversation is not about an OpenSpec change, so no run can be requested here.";
pub const CONVERSATION_RESOLVED: &str = "This conversation is resolved and takes no further comments, so nothing was requested. Answer with what you already have; asking again will not succeed.";
pub const STREAM_GONE: &str = "The work stream of this change is gone, so no run can be requested.";
pub const CHANGE_GONE: &str =
    "The change is not on its work stream any more, so no run can be requested.";
pub const REQUEST_PENDING: &str = "A request for this change waits for the author. Answer with what you have; the author decides that request first.";
pub const COULD_NOT_RECORD: &str = "The request could not be recorded.";

/// RCA-FR-ABXT: what a second call in one reply is told.
pub const ONE_AT_A_TIME: &str =
    "Only one request can be posted in a reply, so this one was not recorded.";

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07). No argument names a stream, a change, or a conversation.
#[derive(Debug, Clone, Deserialize)]
pub struct RequestChangeActionArgs {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub instruction: Option<String>,
    #[serde(default)]
    pub rationale: String,
}

/// RCA contract surface, "Output".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestChangeActionOutput {
    pub request_id: String,
}

/// TLC-FR-06 / RCA-FR-PTAK: the JSON Schema of the contract surface.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["apply", "rework"],
                "description": ACTION_DESCRIPTION,
            },
            "instruction": { "type": "string", "description": INSTRUCTION_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
        },
        "required": ["action", "rationale"],
    })
}

fn refusal(kind: FixedKind, message: &'static str, code: &'static str) -> ToolRefusal {
    // RCA contract surface: the argument refusals and `could not record` are
    // retryable, and the others are not.
    let retryable = matches!(kind, FixedKind::InvalidArgs | FixedKind::Other);
    ToolRefusal::Fixed { kind, message, code, retryable }
}

fn invalid(message: &'static str, code: &'static str) -> ToolRefusal {
    refusal(FixedKind::InvalidArgs, message, code)
}

/// RCA-FR-YRTD through RCA-FR-DJOP: the store's refusal codes in this tool's
/// terms.
fn refusal_for(code: &str) -> ToolRefusal {
    match code {
        car::ERR_ACTION_REQUEST_PENDING => {
            refusal(FixedKind::PermissionDenied, REQUEST_PENDING, "request_pending")
        }
        car::ERR_DISCUSSION_RESOLVED => {
            refusal(FixedKind::PermissionDenied, CONVERSATION_RESOLVED, "conversation_resolved")
        }
        car::ERR_UNKNOWN_STREAM => refusal(FixedKind::NotFound, STREAM_GONE, "stream_gone"),
        car::ERR_CHANGE_NOT_FOUND => refusal(FixedKind::NotFound, CHANGE_GONE, "change_gone"),
        car::ERR_BLANK_INSTRUCTION => invalid(NO_INSTRUCTION, "no_instruction"),
        _ => refusal(FixedKind::Other, COULD_NOT_RECORD, "could_not_record"),
    }
}

/// RCA-FR-UZJK: `request_change_action` as a `rig` portable tool.
/// `requested` is how the loop learns that a request went out and the turn
/// ends (CVL-FR-15).
pub struct RequestChangeActionTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    agent: Participant,
    requested: Arc<AtomicBool>,
    sites: Option<Box<dyn SiteResolver + Send + Sync>>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> RequestChangeActionTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// RCA-FR-QANG: bound to the change of the turn's target, the turn's
    /// conversation, and the turn's agent participant.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        agent: Participant,
        requested: Arc<AtomicBool>,
    ) -> Self {
        RequestChangeActionTool {
            app,
            roots,
            origin,
            agent,
            requested,
            sites: None,
            buffer: &BUFFER,
        }
    }

    /// The same tool with the sites and the log buffer fixed.
    #[cfg(test)]
    pub fn for_test(
        mut self,
        sites: Box<dyn SiteResolver + Send + Sync>,
        buffer: &'static LogBuffer,
    ) -> Self {
        self.sites = Some(sites);
        self.buffer = buffer;
        self
    }

    /// RCA-FR-ZWAF: the call with its log records. No record holds the
    /// instruction or the rationale.
    fn run(&self, args: RequestChangeActionArgs) -> Result<RequestChangeActionOutput, ToolRefusal> {
        let (stream_id, change) = self.origin.target.change_owner().unwrap_or(("", ""));
        let mut fields = log_fields! { "streamId" => stream_id, "change" => change };
        let outcome = self.request(&args);
        match &outcome {
            Ok(_) => {
                let action = ChangeAction::parse(&args.action).map_or("", ChangeAction::as_str);
                fields.insert("action".to_string(), serde_json::json!(action));
                log_tool_success(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields),
        }
        outcome
    }

    fn request(
        &self,
        args: &RequestChangeActionArgs,
    ) -> Result<RequestChangeActionOutput, ToolRefusal> {
        // RCA-FR-XXIP.
        require_open_project(&self.app)?;
        // RCA-FR-CGCE.
        let Some((stream_id, change)) = self.origin.target.change_owner() else {
            return Err(refusal(
                FixedKind::PermissionDenied,
                NOT_A_CHANGE_CONVERSATION,
                "not_a_change_conversation",
            ));
        };
        // RCA-FR-BYYI.
        let action =
            ChangeAction::parse(&args.action).ok_or_else(|| invalid(BAD_ACTION, "bad_action"))?;
        // RCA-FR-KTJP.
        let instruction = args.instruction.as_deref().map(str::trim).filter(|i| !i.is_empty());
        match (action, instruction) {
            (ChangeAction::Rework, None) => return Err(invalid(NO_INSTRUCTION, "no_instruction")),
            (ChangeAction::Apply, Some(_)) => {
                return Err(invalid(APPLY_TAKES_NO_INSTRUCTION, "apply_takes_no_instruction"))
            }
            _ => {}
        }
        // RCA-FR-XDQZ.
        if args.rationale.trim().is_empty() {
            return Err(invalid(BLANK_RATIONALE, "blank_rationale"));
        }
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(invalid(NO_SPOKEN_PART, "no_spoken_part"));
        }
        let app_sites = AppSites { app: &self.app };
        let sites: &dyn SiteResolver = match &self.sites {
            Some(sites) => sites.as_ref(),
            None => &app_sites,
        };
        // RCA-FR-QEWH: the loop has already removed the citation markers from
        // every string argument (CVL-FR-VSSU).
        let record = car::record_change_action_request(
            &self.app,
            &self.roots.store,
            sites,
            NewActionRequest {
                stream_id,
                change,
                action,
                instruction,
                rationale: &args.rationale,
                agent: &self.agent,
                thread_id: self.origin.discussion_id(),
            },
        )
        .map_err(|code| refusal_for(&code))?;
        // RCA-FR-IKUP / CVL-FR-15: set only after the record and its comment
        // both landed.
        self.requested.store(true, Ordering::SeqCst);
        Ok(RequestChangeActionOutput { request_id: record.id })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for RequestChangeActionTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = RequestChangeActionArgs;
    type Output = RequestChangeActionOutput;
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
        // RCA-FR-IKUP: it records and returns; nothing waits.
        std::future::ready(self.run(arguments))
    }
}
