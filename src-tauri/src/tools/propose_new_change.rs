//! Propose new change tool (`PNC-propose-new-change-tool.md`).
//!
//! The tool an agent uses when the author has decided to split the OpenSpec
//! change under discussion. Each call proposes one new change, or changes or
//! withdraws the open proposal, through `crate::new_change_proposals`. Nothing
//! is created until the author accepts (PNC-FR-VVSC). The change, the
//! conversation, and the agent are bound at construction (PNC-FR-IADA).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, FixedKind, ToolRefusal,
    RATIONALE_NO_SPOKEN_PART,
};

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::change_proposals::{AppSites, SiteResolver};
use crate::comments::Participant;
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::new_change_proposals::{
    self as ncp, NewChange, NewChangeUpdate, NewChangeUpdateAction, ProposedFile,
};

/// PNC-FR-RTSB.
pub const NAME: &str = "propose_new_change";

/// PNC-FR-SODJ: the fixed text of the contract surface.
pub const DESCRIPTION: &str = "Propose one new OpenSpec change that takes over a part of the change under discussion, when the author has decided to split this change into two or more changes on the same work stream. Use it only after the author asked for a split and you understand how they want it split; ask them first where you are not sure. Each call proposes **one** change. Give its `name` in kebab-case, every file of its folder in `files` (at least `proposal.md`, and the `specs/`, `design.md`, and `tasks.md` files it needs), its `position` counted from 1, and the `total` number of new changes you expect now. Nothing is created until the author accepts. Remove the moved parts from this change with `propose_change_edits` in a later turn. To change or withdraw your open proposal, call again with `update`. Your turn ends when the proposal is recorded.";

const RATIONALE_DESCRIPTION: &str = "Why this change is the right next part, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";
const NAME_DESCRIPTION: &str = "The name of the new change, in kebab-case: lower-case letters and digits in groups joined by `-`. With `update` and `change`, give it only to replace the name.";
const FILES_DESCRIPTION: &str = "Every file of the new change folder: each a `path` relative to the folder, for example `proposal.md` or `specs/auth/spec.md`, and its full `text`. `proposal.md` is required. With `update` and `change`, give them only to replace every file.";
const POSITION_DESCRIPTION: &str = "The place of this change in the sequence, counted from 1.";
const TOTAL_DESCRIPTION: &str =
    "The number of new changes you expect the split to have now. It can change in a later call.";
const UPDATE_DESCRIPTION: &str = "Changes your proposal that is still open. `change` replaces the fields you give. `withdraw` takes the proposal back and gives no other field.";

/// The arguments, as the model composed them (PNC contract surface).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeNewChangeArgs {
    pub rationale: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<FileArg>>,
    #[serde(default)]
    pub position: Option<i64>,
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub update: Option<UpdateArg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileArg {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateArg {
    #[serde(default)]
    pub action: Option<String>,
}

/// PNC-FR-JPTJ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposeNewChangeOutput {
    pub proposed: bool,
}

/// TLC-FR-06 / PNC-FR-SODJ.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
            "name": { "type": "string", "description": NAME_DESCRIPTION },
            "files": {
                "type": "array",
                "description": FILES_DESCRIPTION,
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "text": { "type": "string" },
                    },
                    "required": ["path", "text"],
                },
            },
            "position": { "type": "integer", "minimum": 1, "description": POSITION_DESCRIPTION },
            "total": { "type": "integer", "minimum": 1, "description": TOTAL_DESCRIPTION },
            "update": {
                "type": "object",
                "description": UPDATE_DESCRIPTION,
                "properties": { "action": { "type": "string", "enum": ["change", "withdraw"] } },
                "required": ["action"],
            },
        },
        "required": ["rationale"],
    })
}

fn invalid(message: &'static str, code: &'static str) -> ToolRefusal {
    ToolRefusal::Fixed { kind: FixedKind::InvalidArgs, message, code, retryable: true }
}

fn denied(message: &'static str, code: &'static str, retryable: bool) -> ToolRefusal {
    ToolRefusal::Fixed { kind: FixedKind::PermissionDenied, message, code, retryable }
}

/// PNC-FR-BVYL / PNC-FR-RLNW: the store's refusal codes in this tool's terms.
fn refusal_for(code: &str) -> ToolRefusal {
    match code {
        ncp::ERR_PROPOSAL_PENDING => denied(
            "A new change you proposed is still waiting on the author. Wait for their decision, or change it with `update`. Nothing was proposed.",
            "new_change_proposal_pending",
            true,
        ),
        ncp::ERR_SPLIT_CONFIRMED => denied(
            "The author confirmed the split of this change, so it takes no new proposals. Tell the author; asking again will not succeed.",
            "split_confirmed",
            false,
        ),
        ncp::ERR_DISCUSSION_RESOLVED => ToolRefusal::ProposalConversationResolved,
        ncp::ERR_CHANGE_EXISTS => invalid(
            "A change of that name already exists on this stream or is already proposed. Choose another name.",
            "openspec_change_exists",
        ),
        ncp::ERR_INVALID_CHANGE_NAME => invalid(
            "`name` must be kebab-case: lower-case letters and digits in groups joined by `-`, and not `archive`.",
            "invalid_change_name",
        ),
        ncp::ERR_BLANK_NAME => invalid("Give the new change a name.", "blank_name"),
        ncp::ERR_BAD_POSITION => {
            invalid("`position` starts at 1 and can not be more than `total`.", "bad_position")
        }
        ncp::ERR_MISSING_PROPOSAL_FILE => {
            invalid("`files` must hold `proposal.md`.", "missing_proposal_file")
        }
        ncp::ERR_PATH_OUTSIDE_CHANGE => invalid(
            "A `path` in `files` is not inside the new change folder. Give paths relative to it, with no `..` segment.",
            "path_outside_change",
        ),
        ncp::ERR_DUPLICATE_PATH => {
            invalid("Two entries of `files` name the same path. Give each file once.", "duplicate_path")
        }
        ncp::ERR_BLANK_FILE => invalid("A file in `files` has no text. Give its full text.", "blank_file"),
        ncp::ERR_NO_OPEN_PROPOSAL => invalid(
            "No new change you proposed is open on this change, so there is nothing to update. Propose a new change without `update`.",
            "no_open_proposal",
        ),
        ncp::ERR_NO_CHANGE => invalid(
            "The fields you gave are the ones the open proposal already has, so there is nothing to decide.",
            "no_change",
        ),
        ncp::ERR_UNKNOWN_STREAM | ncp::ERR_CHANGE_NOT_FOUND => ToolRefusal::Fixed {
            kind: FixedKind::NotFound,
            message: "The OpenSpec change of this conversation is no longer in its work stream, so no new change can be proposed. Tell the author.",
            code: "change_not_found",
            retryable: false,
        },
        _ => ToolRefusal::ProposalNotRecorded,
    }
}

/// PNC-FR-RTSB: `propose_new_change` as a `rig` portable tool.
pub struct ProposeNewChangeTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    sites: Option<Box<dyn SiteResolver + Send + Sync>>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposeNewChangeTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// PNC-FR-IADA.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposeNewChangeTool { app, roots, origin, author, proposed, sites: None, buffer: &BUFFER }
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

    /// PNC-FR-VHLS: the call with its log records. No record holds file text
    /// or the rationale.
    fn run(&self, args: ProposeNewChangeArgs) -> Result<ProposeNewChangeOutput, ToolRefusal> {
        let (stream_id, change) = self.origin.target.change_owner().unwrap_or(("", ""));
        let mut fields = log_fields! {
            "streamId" => stream_id,
            "change" => change,
            "name" => args.name.clone().unwrap_or_default(),
            "files" => args.files.as_ref().map_or(0, Vec::len),
            "position" => args.position.unwrap_or(0),
            "total" => args.total.unwrap_or(0),
            "update" => args.update.as_ref().and_then(|u| u.action.clone()).unwrap_or_default(),
        };
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                fields.insert("refusal".to_string(), serde_json::json!(refusal.reason()));
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn propose(&self, args: ProposeNewChangeArgs) -> Result<ProposeNewChangeOutput, ToolRefusal> {
        require_open_project(&self.app)?;
        let Some((stream_id, change)) = self.origin.target.change_owner() else {
            return Err(denied(
                "This conversation is not about an OpenSpec change, so there is no change here to split. Say how you would split it instead; asking again will not succeed.",
                "not_a_change_conversation",
                false,
            ));
        };
        if args.rationale.trim().is_empty() {
            return Err(invalid(
                "Say why this change is the right next part. The author reads this beside the proposal.",
                "blank_rationale",
            ));
        }
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }
        let to_u32 = |n: Option<i64>| n.map(|v| u32::try_from(v).unwrap_or(0));
        let files: Option<Vec<ProposedFile>> = args.files.as_ref().map(|files| {
            files
                .iter()
                .map(|f| ProposedFile { path: f.path.trim().to_string(), text: f.text.clone() })
                .collect()
        });
        let app_sites = AppSites { app: &self.app };
        let sites: &dyn SiteResolver = match &self.sites {
            Some(sites) => sites.as_ref(),
            None => &app_sites,
        };
        let thread_id = self.origin.discussion_id();
        match &args.update {
            None => {
                let (Some(name), Some(files), Some(position), Some(total)) = (
                    args.name.as_deref(),
                    files.as_ref(),
                    to_u32(args.position),
                    to_u32(args.total),
                ) else {
                    return Err(invalid(
                        "Give `name`, `files`, `position`, and `total` to propose a new change, or `update` to change your open proposal.",
                        "fields_missing",
                    ));
                };
                ncp::record_new_change_proposal(
                    &self.app,
                    &self.roots.store,
                    sites,
                    NewChange {
                        stream_id,
                        change,
                        name: name.trim(),
                        files,
                        position,
                        total,
                        rationale: &args.rationale,
                        agent: &self.author,
                        thread_id,
                    },
                )
                .map_err(|code| refusal_for(&code))?;
            }
            Some(update) => {
                let carries_fields = args.name.is_some()
                    || files.is_some()
                    || args.position.is_some()
                    || args.total.is_some();
                let action = match update.action.as_deref() {
                    Some("withdraw") if !carries_fields => NewChangeUpdateAction::Withdraw,
                    Some("change") if carries_fields => NewChangeUpdateAction::Change {
                        name: args.name.as_deref().map(|n| n.trim().to_string()),
                        files,
                        position: to_u32(args.position),
                        total: to_u32(args.total),
                    },
                    _ => {
                        return Err(invalid(
                            "`update` gives `action`: `change` with at least one of `name`, `files`, `position`, and `total`, or `withdraw` with none of them.",
                            "update_malformed",
                        ))
                    }
                };
                ncp::update_new_change_proposal(
                    &self.app,
                    &self.roots.store,
                    sites,
                    NewChangeUpdate {
                        stream_id,
                        change,
                        action,
                        rationale: &args.rationale,
                        agent: &self.author,
                        thread_id,
                    },
                )
                .map_err(|code| refusal_for(&code))?;
            }
        }
        // PNC-FR-JPTJ / CVL-FR-15: a recording ends the turn.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeNewChangeOutput { proposed: true })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposeNewChangeTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = ProposeNewChangeArgs;
    type Output = ProposeNewChangeOutput;
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
        std::future::ready(self.run(arguments))
    }
}
