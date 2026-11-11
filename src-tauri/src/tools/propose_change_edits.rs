//! Propose change edits tool (`PCE-propose-change-edits-tool.md`).
//!
//! The tool an agent in an OpenSpec change discussion uses to offer the author
//! edits to the change. One call records one proposal of anchored hunks across
//! the files of the change, or updates the open one, and the author decides
//! each hunk in the text it changes (`CEP-change-edit-proposals.md`). It is
//! `propose_prompt_changes` for a change: the same hunk rules, the same
//! updates, and the same one act of proposal and comment.
//!
//! The change, the conversation, and the agent are bound at construction
//! (PCE-FR-ABFJ). No argument names a stream or a change, so a model cannot
//! propose into a change it was not asked about.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::proposal_updates::{
    self, UpdateArg, UPDATE_CHANGE_SETTLED, UPDATE_NAMED_TWICE, UPDATE_NONE, UPDATE_NO_SUCH_CHANGE,
};
use super::propose_prompt_changes::{
    PROPOSAL_HUNKS_OVERLAP, PROPOSAL_LEAD_AND_TRAIL_TOO_LONG, PROPOSAL_LEAD_TOO_LONG,
    PROPOSAL_NO_HUNKS, PROPOSAL_TEXT_NOT_FOUND, PROPOSAL_TEXT_NOT_UNIQUE, PROPOSAL_TRAIL_TOO_LONG,
};
use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, FixedKind, ToolRefusal,
    PROMPT_PROPOSAL_RATIONALE_BLANK, PROPOSAL_NO_CHANGE, RATIONALE_NO_SPOKEN_PART,
};

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::change_proposals::{
    self as cep, AppSites, ChangeProposalUpdate, ChangeUpdateEntry, NewChangeProposal, PathedHunk,
    RecordRefusal, SiteResolver, UpdateRefusal,
};
use crate::comments::Participant;
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::text_hunks::hunks::{ContextSide, ProposedHunk};

/// PCE-FR-QYDD.
pub const NAME: &str = "propose_change_edits";

/// PCE-FR-MRWZ: the fixed text of the contract surface.
pub const DESCRIPTION: &str = "Propose edits to the OpenSpec change under discussion. Use this when you can show the author how the change should read: give the hunks in the files they belong to, across the proposal, the deltas, the design, and the tasks, so that the change stays coherent. Each hunk names its path relative to the change folder, for example `specs/auth/spec.md`. A path that the change does not hold yet creates a new file. The author decides each hunk in the text; nothing is written until they accept. To change the open proposal, call again with `updates`. Your turn ends when the proposal is recorded.";

const HUNKS_DESCRIPTION: &str = "The hunks, in the order they should be read. Each names `path`, relative to the change folder. In each, `before` is the exact existing text of that file you change, copied character for character, and `after` is what stands in its place. For an insertion give `after` and the exact text it follows in `after_text`; for a deletion give `before` alone. Where the text appears more than once in the file, add `lead` or `trail`, the exact text just before or after it, up to 200 characters each. For a new file give the path and `after` alone, which is the whole text of the file.";

const UPDATES_DESCRIPTION: &str = "Changes to your open proposal, in place of `hunks`. Each update gives `action`: `change` with the `id` of a hunk and its new fields, `add` with `path` and the fields of a new hunk, or `withdraw` with the `id` of a hunk you no longer offer. An accepted or withdrawn hunk can not be updated.";

const PATH_DESCRIPTION: &str =
    "The path of the file, relative to the change folder, for example `tasks.md` or `specs/auth/spec.md`.";

const RATIONALE_DESCRIPTION: &str = "Why the edits are worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";

/// PCE-FR-OXMY: exactly one of `hunks` and `updates`.
pub const PROPOSAL_NOT_ONE_FORM: &str =
    "Pass exactly one of `hunks`, the hunks of a new proposal, or `updates`, the updates of your open proposal.";
/// PCE-FR-FEAK: a path that leaves the change folder.
pub const PATH_OUTSIDE_CHANGE: &str = "The `path` you gave is not inside the change folder. Give a path relative to the change folder, with no `..` segment, for example `specs/auth/spec.md`.";
/// PCE-FR-FEAK: an `add` update with no path.
pub const UPDATE_PATH_MISSING: &str =
    "An `add` update names the `path` of its file, relative to the change folder.";
/// PCE-FR-FEAK (as PPC-FR-09): the change's one slot is taken.
pub const CHANGE_PROPOSAL_PENDING: &str = "A proposal of yours on this change is still waiting on the author. Wait for their decision, or change the open proposal with `updates`. Say so plainly in your reply, and do not describe these edits as proposed, because they were not.";
/// PCE-FR-MVKT: a rework run of the change is not terminal.
pub const CHANGE_REWORK_RUNNING: &str = "A rework run of this change is running, and its result comes to the author as a proposal. Nothing was recorded. Answer with what you have.";
/// PCE-FR-ITEV: the refusal of a new proposal while an apply run of the change
/// is not terminal.
pub const CHANGE_APPLY_RUNNING: &str =
    "An apply run of this change is running. Nothing was recorded. Answer with what you have.";
/// PCE-FR-FEAK: no proposal is open, so there is nothing to update.
pub const CHANGE_NO_OPEN: &str = "No proposal is open on this change, so there is nothing to update. Send your edits as a new proposal in `hunks`.";
/// PCE-FR-FVPF: the conversation is not about a change.
pub const NOT_A_CHANGE_CONVERSATION: &str = "This conversation is not about an OpenSpec change, so there is no change to propose edits to. Asking again will not succeed.";
/// CEP-FR-LYSS: the stream or the change folder is gone.
pub const CHANGE_GONE: &str = "The OpenSpec change of this conversation is no longer in its work stream, so no edit can be proposed. Tell the author.";

/// The arguments, as the model composed them (PCE contract surface).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeChangeEditsArgs {
    #[serde(default)]
    pub hunks: Option<Vec<ChangeHunkArg>>,
    #[serde(default)]
    pub updates: Option<Vec<ChangeUpdateArg>>,
    pub rationale: String,
}

/// One hunk with its change-relative path.
#[derive(Debug, Clone, Deserialize)]
pub struct ChangeHunkArg {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub after_text: Option<String>,
    #[serde(default)]
    pub lead: Option<String>,
    #[serde(default)]
    pub trail: Option<String>,
}

impl ChangeHunkArg {
    fn to_pathed(&self) -> PathedHunk {
        PathedHunk {
            path: self.path.trim().to_string(),
            hunk: ProposedHunk::from_fields(
                self.before.clone(),
                self.after.clone(),
                self.after_text.clone(),
                self.lead.clone(),
                self.trail.clone(),
                None,
            ),
        }
    }
}

/// One update, with the path an `add` names.
#[derive(Debug, Clone, Deserialize)]
pub struct ChangeUpdateArg {
    #[serde(flatten)]
    pub update: UpdateArg,
    #[serde(default)]
    pub path: Option<String>,
}

/// PCE contract surface `ProposeChangeEditsOutput`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposeChangeEditsOutput {
    pub proposal_id: String,
    pub hunk_count: u32,
}

fn hunk_properties(with_path: bool) -> serde_json::Value {
    let mut properties = serde_json::json!({
        "kind": { "type": "string", "enum": ["replace", "del", "add"],
                  "description": "What the hunk does. The fields you send decide." },
        "before": { "type": "string", "description": "The exact existing text the hunk replaces or deletes." },
        "after": { "type": "string", "description": "The exact new text, or the whole text of a new file." },
        "after_text": { "type": "string", "description": "For an insertion: the exact text it follows." },
        "lead": { "type": "string", "description": "Optional. The exact text just before, up to 200 characters." },
        "trail": { "type": "string", "description": "Optional. The exact text just after, up to 200 characters." },
    });
    if let (true, Some(map)) = (with_path, properties.as_object_mut()) {
        map.insert(
            "path".into(),
            serde_json::json!({ "type": "string", "description": PATH_DESCRIPTION }),
        );
    }
    properties
}

/// TLC-FR-06 / PCE-FR-MRWZ: the JSON Schema of the arguments.
pub fn parameters() -> serde_json::Value {
    let mut updates = proposal_updates::schema(UPDATES_DESCRIPTION, false);
    if let Some(props) = updates
        .get_mut("items")
        .and_then(|items| items.get_mut("properties"))
        .and_then(|p| p.as_object_mut())
    {
        props.insert(
            "path".into(),
            serde_json::json!({ "type": "string", "description": PATH_DESCRIPTION }),
        );
    }
    serde_json::json!({
        "type": "object",
        "properties": {
            "hunks": {
                "type": "array",
                "description": HUNKS_DESCRIPTION,
                "items": { "type": "object", "properties": hunk_properties(true), "required": ["path"] },
            },
            "updates": updates,
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
        },
        "required": ["rationale"],
    })
}

fn context_too_long(side: ContextSide) -> &'static str {
    match side {
        ContextSide::Lead => PROPOSAL_LEAD_TOO_LONG,
        ContextSide::Trail => PROPOSAL_TRAIL_TOO_LONG,
        ContextSide::Both => PROPOSAL_LEAD_AND_TRAIL_TOO_LONG,
    }
}

fn pending_refusal() -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::PermissionDenied,
        message: CHANGE_PROPOSAL_PENDING,
        code: "proposal_pending",
        retryable: true,
    }
}

/// PCE-FR-MVKT: `PermissionDenied`, not retryable.
fn rework_running_refusal() -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::PermissionDenied,
        message: CHANGE_REWORK_RUNNING,
        code: "rework_running",
        retryable: false,
    }
}

/// PCE-FR-ITEV: `PermissionDenied`, not retryable.
fn apply_running_refusal() -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::PermissionDenied,
        message: CHANGE_APPLY_RUNNING,
        code: "apply_running",
        retryable: false,
    }
}

fn gone_refusal() -> ToolRefusal {
    ToolRefusal::Fixed {
        kind: FixedKind::NotFound,
        message: CHANGE_GONE,
        code: "change_not_found",
        retryable: false,
    }
}

/// PCE-FR-FEAK: the store's refusals in this tool's terms.
fn refusal_for(refusal: RecordRefusal) -> ToolRefusal {
    let at = |message: &'static str, at: usize, of: usize| ToolRefusal::InvalidArgumentsAt {
        message,
        at,
        of,
    };
    match refusal {
        RecordRefusal::ProposalPending => pending_refusal(),
        RecordRefusal::ReworkRunning => rework_running_refusal(),
        RecordRefusal::ApplyRunning => apply_running_refusal(),
        RecordRefusal::UnknownStream | RecordRefusal::ChangeNotFound => gone_refusal(),
        RecordRefusal::PathOutsideChange { at: n, of } => at(PATH_OUTSIDE_CHANGE, n, of),
        RecordRefusal::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        RecordRefusal::NoHunks => ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS),
        RecordRefusal::HunkAnchorLost { at: n, of } => at(PROPOSAL_TEXT_NOT_FOUND, n, of),
        RecordRefusal::HunkAmbiguous { at: n, of } => at(PROPOSAL_TEXT_NOT_UNIQUE, n, of),
        RecordRefusal::HunkOverlap { at: n, of } => at(PROPOSAL_HUNKS_OVERLAP, n, of),
        RecordRefusal::HunkContextTooLong { at: n, of, side } => at(context_too_long(side), n, of),
        RecordRefusal::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        RecordRefusal::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

/// PCE-FR-FEAK: the update refusals in this tool's terms.
fn update_refusal_for(refusal: UpdateRefusal) -> ToolRefusal {
    let at = |message: &'static str, at: usize, of: usize| ToolRefusal::InvalidUpdateAt {
        message,
        at,
        of,
    };
    match refusal {
        UpdateRefusal::NoOpenProposal => ToolRefusal::InvalidArguments(CHANGE_NO_OPEN),
        UpdateRefusal::ReworkRunning => rework_running_refusal(),
        UpdateRefusal::NoUpdates => ToolRefusal::InvalidArguments(UPDATE_NONE),
        UpdateRefusal::UnknownStream | UpdateRefusal::ChangeNotFound => gone_refusal(),
        UpdateRefusal::PathOutsideChange { at: n, of } => at(PATH_OUTSIDE_CHANGE, n, of),
        UpdateRefusal::PathMissing { at: n, of } => at(UPDATE_PATH_MISSING, n, of),
        UpdateRefusal::HunkNotFound { at: n, of } => at(UPDATE_NO_SUCH_CHANGE, n, of),
        UpdateRefusal::HunkNotUpdatable { at: n, of } => at(UPDATE_CHANGE_SETTLED, n, of),
        UpdateRefusal::HunkNamedTwice { at: n, of } => at(UPDATE_NAMED_TWICE, n, of),
        UpdateRefusal::NoChange { at: n, of } => at(PROPOSAL_NO_CHANGE, n, of),
        UpdateRefusal::HunkAnchorLost { at: n, of } => at(PROPOSAL_TEXT_NOT_FOUND, n, of),
        UpdateRefusal::HunkAmbiguous { at: n, of } => at(PROPOSAL_TEXT_NOT_UNIQUE, n, of),
        UpdateRefusal::HunkOverlap { at: n, of } => at(PROPOSAL_HUNKS_OVERLAP, n, of),
        UpdateRefusal::HunkContextTooLong { at: n, of, side } => at(context_too_long(side), n, of),
        UpdateRefusal::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        UpdateRefusal::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

/// PCE-FR-QYDD: `propose_change_edits` as a `rig` portable tool.
pub struct ProposeChangeEditsTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    sites: Option<Box<dyn SiteResolver + Send + Sync>>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposeChangeEditsTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// PCE-FR-ABFJ: bound to the change of the turn's target, the turn's
    /// conversation, and the turn's agent participant.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposeChangeEditsTool {
            app,
            roots,
            origin,
            author,
            proposed,
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

    /// The whole call, with its logging (PCE-FR-XBYT). No record holds hunk
    /// text or the rationale.
    fn run(&self, args: ProposeChangeEditsArgs) -> Result<ProposeChangeEditsOutput, ToolRefusal> {
        let (stream_id, change) = self.origin.target.change_owner().unwrap_or(("", ""));
        let mut fields = log_fields! {
            "streamId" => stream_id,
            "change" => change,
            "discussionId" => self.origin.discussion_id(),
            "hunks" => args.hunks.as_ref().map_or(0, Vec::len),
            "updates" => args.updates.as_ref().map_or(0, Vec::len),
        };
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                if let ToolRefusal::InvalidArgumentsAt { at, .. }
                | ToolRefusal::InvalidUpdateAt { at, .. } = refusal
                {
                    fields.insert("refusalAt".to_string(), serde_json::json!(at));
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn propose(
        &self,
        args: ProposeChangeEditsArgs,
    ) -> Result<ProposeChangeEditsOutput, ToolRefusal> {
        // PCE-FR-IIDO.
        require_open_project(&self.app)?;
        let Some((stream_id, change)) = self.origin.target.change_owner() else {
            return Err(ToolRefusal::Fixed {
                kind: FixedKind::PermissionDenied,
                message: NOT_A_CHANGE_CONVERSATION,
                code: "not_a_change_conversation",
                retryable: false,
            });
        };
        let thread_id = self.origin.discussion_id();
        // PCE-FR-OXMY: exactly one form.
        let (hunks, updates) = match (&args.hunks, &args.updates) {
            (Some(hunks), None) => (Some(hunks), None),
            (None, Some(updates)) => (None, Some(updates)),
            _ => return Err(ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM)),
        };
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROMPT_PROPOSAL_RATIONALE_BLANK));
        }
        // PCE-FR-XEYG (as PPC-FR-HMEH): the rationale is the comment's body.
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }
        let app_sites = AppSites { app: &self.app };
        let sites: &dyn SiteResolver = match &self.sites {
            Some(sites) => sites.as_ref(),
            None => &app_sites,
        };

        let recorded = if let Some(hunks) = hunks {
            if hunks.is_empty() {
                return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS));
            }
            let pathed: Vec<PathedHunk> = hunks.iter().map(ChangeHunkArg::to_pathed).collect();
            cep::record_change_proposal(
                &self.app,
                &self.roots.store,
                sites,
                NewChangeProposal {
                    stream_id,
                    change,
                    hunks: &pathed,
                    rationale: &args.rationale,
                    agent: &self.author,
                    thread_id,
                },
            )
            .map_err(refusal_for)?
        } else {
            let updates = updates.map(Vec::as_slice).unwrap_or_default();
            if updates.is_empty() {
                return Err(ToolRefusal::InvalidArguments(UPDATE_NONE));
            }
            let bases: Vec<UpdateArg> = updates.iter().map(|u| u.update.clone()).collect();
            let entries = proposal_updates::to_entries(&bases)?;
            let entries: Vec<ChangeUpdateEntry> = entries
                .into_iter()
                .zip(updates)
                .map(|(entry, arg)| ChangeUpdateEntry {
                    entry,
                    path: arg.path.as_deref().map(|p| p.trim().to_string()),
                })
                .collect();
            cep::update_change_proposal(
                &self.app,
                &self.roots.store,
                sites,
                ChangeProposalUpdate {
                    stream_id,
                    change,
                    updates: &entries,
                    rationale: &args.rationale,
                    agent: &self.author,
                    thread_id,
                },
            )
            .map_err(update_refusal_for)?
        };
        // PCE-FR-OXSO / CVL-FR-15: a recording ends the turn.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeChangeEditsOutput { proposal_id: recorded.id, hunk_count: recorded.hunk_count })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposeChangeEditsTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = ProposeChangeEditsArgs;
    type Output = ProposeChangeEditsOutput;
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
        // TLC-FR-15 / TLC-FR-16: records and returns, waiting for nothing.
        std::future::ready(self.run(arguments))
    }
}
