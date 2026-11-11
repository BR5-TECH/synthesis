//! What `decide_draft_changes` and `decide_prompt_changes` share
//! (`DDC-decide-draft-changes-tool.md`, `DPC-decide-prompt-changes-tool.md`).
//!
//! The two tools decide hunks of an open proposal on an explicit instruction of
//! the author. They differ only in the store that holds the proposal, so the
//! arguments, the output, the per-entry refusals, the entry loop, and the log
//! records are here, and each tool supplies one [`ProposalBackend`].
//!
//! ## Why each entry runs on its own
//!
//! The author says "accept the first and the third". A refusal of the first
//! must not cost the author the third (DDC-FR-DSTU). So each entry reads the
//! proposal as it stands after the entries before it, and a refused entry
//! stops no other entry.

use rig::tool::{ToolErrorKind, ToolExecutionError};
use serde::{Deserialize, Serialize};

use super::{ToolRefusal, NO_PROJECT_OPEN};
use crate::agent_conversations::{instruction_is_well_formed, AuthorityRefusal};
use crate::comments::{self, Attachment, Discussion, Participant};
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer, LogSink};

/// DDC-FR-WRFY: the most entries one call carries.
pub const MAX_DECISIONS: usize = 50;

/// DDC contract surface: the body of the opening comment of a hunk discussion
/// that a `discuss` entry opens (DDC-FR-ILCB, DPC-FR-AFVA).
pub const DISCUSSION_BODY: &str = "The author asked to discuss this change.";

/// DDC-FR-WRFY: the shared **malformed arguments** refusal, in this tool's
/// words.
pub const DECISIONS_MALFORMED: &str = "Pass `instruction` with the author's words, up to 500 characters, and `decisions` with 1 to 50 entries, each with the `id` of a change and an `action`.";

/// DDC-FR-ZYOL: the speech-not-posted refusal.
pub const SPEECH_NOT_POSTED: &str = "The author is still speaking, and the comment of the speech has not arrived. Nothing was decided. Call again after the comment arrives.";
/// DDC-FR-ELAE: the not-authorized refusal.
pub const NOT_AUTHORIZED: &str = "The latest comment you received is not one the author wrote or spoke, so it can not authorize a decision. Nothing was decided. Tell the author what you would decide and let them decide.";
/// DDC-FR-NKAB: the instruction-not-found refusal.
pub const INSTRUCTION_NOT_FOUND: &str = "The words in `instruction` are not in the author's latest comment. Nothing was decided. Copy the words of that comment that ask for these decisions.";
/// DDC-FR-VONX: the instruction-used refusal.
pub const INSTRUCTION_USED: &str = "The author's latest comment already gave a decision. Nothing was decided. Ask the author what to do next.";

/// DDC-FR-ROLG / DPC-FR-HWOT: the parameter text of `instruction`.
const INSTRUCTION_DESCRIPTION: &str = "The words of the author's latest comment that ask for these decisions, copied exactly as the author wrote or said them, for example \"accept the second change\".";

/// DDC-FR-VAYO / DPC-FR-RJBJ: the parameter text of `decisions`.
const DECISIONS_DESCRIPTION: &str = "The decisions the author asked for in the latest comment, in the order the author gave them. Each entry names one change by its `id` and gives one `action`.";

/// DDC-FR-VAYO / DPC-FR-RJBJ: the parameter text of `id`.
const ID_DESCRIPTION: &str = "The id of a change of the open proposal, as your input lists it.";

/// TLC-FR-14: the domains of every record this pair emits (DDC-FR-TBVA).
const DOMAINS: [Domain; 2] = [Domain::Ai, Domain::Backend];

// ---------------------------------------------------------------------------
// The arguments (DDC-FR-WRFY)
// ---------------------------------------------------------------------------

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored (TLC-FR-07). There is no parameter that names a
/// draft, an artifact, a proposal, a conversation, or a participant
/// (DDC-FR-QRIY, DPC-FR-QMZE): the constructor binds all of them.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecideChangesArgs {
    /// DDC-FR-WRFY: an absent or `null` list decodes as no list, and the tool
    /// refuses it as malformed arguments rather than the decoder.
    #[serde(default)]
    pub decisions: Option<Vec<DecisionArg>>,
    /// DDC-FR-PZBH: the author's words that ask for the decisions. Absent is
    /// the malformed-arguments refusal.
    #[serde(default)]
    pub instruction: Option<String>,
}

/// One entry, as the model composed it.
///
/// `action` is decoded as text, so a value outside the enum is the per-entry
/// `invalid_action` refusal and not a refusal of the whole call (DDC-FR-JCSV).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionArg {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
}

/// DDC-FR-VAYO / DPC-FR-RJBJ: the JSON Schema of the arguments, with the
/// description of `action` that names what an acceptance writes into.
pub fn parameters(action_description: &str) -> serde_json::Value {
    parameters_with(action_description, ID_DESCRIPTION, &["accept", "reject", "discuss"])
}

/// [`parameters`] with an `id` description and an `action` enum of the tool's
/// own (DDC contract surface, DPC-FR-RJBJ).
pub fn parameters_with(
    action_description: &str,
    id_description: &str,
    actions: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "instruction": {
                "type": "string",
                "description": INSTRUCTION_DESCRIPTION,
                "maxLength": crate::agent_conversations::MAX_INSTRUCTION_CHARS,
            },
            "decisions": {
                "type": "array",
                "description": DECISIONS_DESCRIPTION,
                "minItems": 1,
                "maxItems": MAX_DECISIONS,
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": id_description },
                        "action": {
                            "type": "string",
                            "enum": actions,
                            "description": action_description,
                        },
                    },
                    "required": ["id", "action"],
                },
            },
        },
        "required": ["instruction", "decisions"],
    })
}

/// One entry with its `id` and `action` present.
struct Entry<'a> {
    id: &'a str,
    action: &'a str,
}

/// DDC-FR-WRFY / DDC-FR-PZBH: the instruction of a well-formed call, or the
/// malformed-arguments refusal. Nothing is decided before this passes.
pub fn check_arguments(args: &DecideChangesArgs) -> Result<&str, CallRefusal> {
    entries(args).map_err(CallRefusal::from)?;
    args.instruction
        .as_deref()
        .filter(|i| instruction_is_well_formed(i))
        .ok_or(CallRefusal::Malformed)
}

/// DDC contract surface: the instruction line that ends a decision comment or
/// a confirmation comment that a call causes (DDC-FR-JJHZ, DCP-FR-HMUG,
/// PCP-FR-QKCT).
pub fn instruction_line(agent: &Participant, instruction: &str) -> String {
    let name = match agent {
        Participant::Agent { handle, .. } => handle.as_str(),
        Participant::Human { login, display_name, .. } => display_name.as_deref().unwrap_or(login),
    };
    format!("Decided by {name} on the author's instruction: “{}”", instruction.trim())
}

/// DDC-FR-VONX: whether a call decided at least one entry, which spends its
/// authorizing comment.
pub fn decided_any(output: &DecideChangesOutput) -> bool {
    output.decisions.iter().any(|row| row.outcome != Outcome::Refused)
}

/// DDC-FR-WRFY: the entries of a well-formed call, or the malformed-arguments
/// refusal. Nothing is decided before this passes.
fn entries(args: &DecideChangesArgs) -> Result<Vec<Entry<'_>>, ToolRefusal> {
    let malformed = || ToolRefusal::InvalidArguments(DECISIONS_MALFORMED);
    let list = args.decisions.as_deref().ok_or_else(malformed)?;
    if list.is_empty() || list.len() > MAX_DECISIONS {
        return Err(malformed());
    }
    list.iter()
        .map(|entry| match (entry.id.as_deref(), entry.action.as_deref()) {
            (Some(id), Some(action)) => Ok(Entry { id, action }),
            _ => Err(malformed()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The output (DDC contract surface)
// ---------------------------------------------------------------------------

/// A hunk's state, as `DCP-draft-change-proposals.md` and
/// `PCP-prompt-change-proposals.md` spell it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkStateView {
    Pending,
    Accepted,
    Rejected,
    Discussing,
    Withdrawn,
}

/// A proposal's state (DCP-FR-PWSF, PCP-FR-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalStateView {
    Pending,
    Accepted,
    Rejected,
}

/// TLC-FR-08 / DDC-FR-WSIA: the data object of a call that passed the
/// whole-call refusals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecideChangesOutput {
    /// The proposal's state after the last entry; absent where the call began
    /// with no open change proposal (DDC contract surface).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_state: Option<ProposalStateView>,
    /// True where an entry of this call closed the proposal.
    pub closed: bool,
    /// One row per entry, in call order.
    pub decisions: Vec<DecisionRow>,
}

/// What happened to one entry (DDC-FR-WSIA).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionRow {
    pub place: usize,
    pub id: String,
    pub action: String,
    pub outcome: Outcome,
    /// Absent for `unknown_hunk`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunk_state: Option<HunkStateView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<EntryRefusal>,
    /// DPC-FR-WNHU / DPC-FR-UEXT: on an accepted row of a prompt proposal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_owed: Option<bool>,
    /// DDC contract surface: the new-draft proposal's state after the entry,
    /// for a new-draft entry alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_draft_state: Option<&'static str>,
    /// DDC contract surface: the draft an accepted new-draft entry created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_draft_id: Option<String>,
    /// DCE contract surface: the change-relative path of a change edit hunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// DCE-FR-IZTA: the run a confirmed change action request started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl DecisionRow {
    /// A row for entry `place` of `total`, not yet run.
    pub fn new(place: usize, id: &str, action: &str) -> DecisionRow {
        DecisionRow {
            place,
            id: id.to_string(),
            action: action.to_string(),
            outcome: Outcome::Refused,
            hunk_state: None,
            discussion_id: None,
            refusal: None,
            comment_owed: None,
            new_draft_state: None,
            created_draft_id: None,
            path: None,
            run_id: None,
        }
    }

    /// DDC-FR-SLBB: refuse this row with `code`, its message naming the place.
    pub fn refuse(&mut self, code: RefusalCode, total: usize, text: &str, error: Option<String>) {
        self.outcome = Outcome::Refused;
        self.refusal = Some(EntryRefusal {
            code,
            error,
            message: format!("Decision {} of {total}: {text}", self.place),
            retryable: code != RefusalCode::DecisionFailed,
        });
    }
}

/// The outcome of one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Rejected,
    DiscussionOpened,
    /// DDC-FR-VEKP: a `confirm` entry confirmed the decomposition.
    Confirmed,
    /// DCE-FR-IZTA: a `confirm` entry started the run of an action request.
    Started,
    /// DCE-FR-IZTA: a `reject` entry declined an action request.
    Declined,
    Refused,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Accepted => "accepted",
            Outcome::Rejected => "rejected",
            Outcome::DiscussionOpened => "discussion_opened",
            Outcome::Confirmed => "confirmed",
            Outcome::Started => "started",
            Outcome::Declined => "declined",
            Outcome::Refused => "refused",
        }
    }
}

/// DDC-FR-SLBB: the refusal of one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryRefusal {
    pub code: RefusalCode,
    /// The backend's typed error, for `decision_failed` (DDC-FR-LYTA).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub message: String,
    pub retryable: bool,
}

/// DDC-FR-JCSV / DDC-FR-EDMT / DDC-FR-XBQV / DDC-FR-LYTA: why one entry was
/// refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCode {
    UnknownHunk,
    HunkLost,
    HunkAccepted,
    HunkWithdrawn,
    AlreadyInState,
    InvalidAction,
    NoOpenProposal,
    DecisionFailed,
}

impl RefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            RefusalCode::UnknownHunk => "unknown_hunk",
            RefusalCode::HunkLost => "hunk_lost",
            RefusalCode::HunkAccepted => "hunk_accepted",
            RefusalCode::HunkWithdrawn => "hunk_withdrawn",
            RefusalCode::AlreadyInState => "already_in_state",
            RefusalCode::InvalidAction => "invalid_action",
            RefusalCode::NoOpenProposal => "no_open_proposal",
            RefusalCode::DecisionFailed => "decision_failed",
        }
    }
}

/// DDC contract surface: the text after "Decision N of M: " for each code.
///
/// `file` is "the prompt" for a draft and "the file" for a prompt or a
/// project document (DPC contract surface).
fn refusal_text(code: RefusalCode, file: &str, already: Option<Action>) -> String {
    match code {
        RefusalCode::UnknownHunk => {
            "the id you gave is not a change of the open proposal. Use an id from your input."
                .to_string()
        }
        RefusalCode::HunkLost => format!(
            "the text this change replaces is no longer in {file}, so it can not be decided here. Tell the author to clear it in the review."
        ),
        RefusalCode::HunkAccepted => {
            "this change is already accepted, and an accepted change is final.".to_string()
        }
        RefusalCode::HunkWithdrawn => {
            "this change was withdrawn, so there is nothing to decide.".to_string()
        }
        RefusalCode::AlreadyInState => match already {
            Some(Action::Discuss) => "this change already has a discussion, so nothing changed.",
            _ => "this change is already rejected, so nothing changed.",
        }
        .to_string(),
        // DDC contract surface: a draft also takes `confirm`. A file has no
        // decomposition, so its message names the three it takes (DPC-FR-VJIG).
        RefusalCode::InvalidAction if file == "the prompt" => INVALID_DRAFT_ACTION.to_string(),
        RefusalCode::InvalidAction => {
            "the action must be `accept`, `reject`, or `discuss`.".to_string()
        }
        RefusalCode::NoOpenProposal => {
            "an earlier decision of this call closed the proposal, so this one was not made."
                .to_string()
        }
        RefusalCode::DecisionFailed => "the decision could not be made".to_string(),
    }
}

// ---------------------------------------------------------------------------
// The whole-call refusals (DDC-FR-OEJB, DDC-FR-WRFY, DDC-FR-ZGXF)
// ---------------------------------------------------------------------------

/// DDC-FR-OEJB: the not-a-draft-conversation refusal.
pub const NOT_A_DRAFT_CONVERSATION: &str = "This conversation is not about a draft, so there is no proposal here to decide. Asking again will not succeed.";
/// DPC-FR-JJGY: the not-an-artifact-conversation refusal.
pub const NOT_AN_ARTIFACT_CONVERSATION: &str = "This conversation is not about a prompt or a project document, so there is no proposal here to decide. Asking again will not succeed.";
/// DDC-FR-ZGXF: no open proposal on the draft.
pub const NO_OPEN_DRAFT_PROPOSAL: &str = "No proposal, no proposed draft, and no decomposition to confirm is open on this draft, so there is nothing to decide. Tell the author that nothing is waiting for a decision.";
/// DDC-FR-JCSV: the `invalid_action` text of a draft entry.
pub const INVALID_DRAFT_ACTION: &str = "the action must be `accept`, `reject`, or `discuss` for a change or a proposed draft, and `confirm` for the decomposition.";
/// DPC-FR-RGGP: no open proposal on the file.
/// DCE-FR-XVLE: no open proposal on the change.
pub const NO_OPEN_CHANGE_PROPOSAL: &str = "No proposal is open on this OpenSpec change, so there is no edit to decide. Tell the author that nothing is waiting for a decision.";
/// DCE-FR-WYJP: the conversation is not about a change.
pub const NOT_A_CHANGE_CONVERSATION: &str = "This conversation is not about an OpenSpec change, so there is no change edit to decide. Asking again will not succeed.";
pub const NO_OPEN_FILE_PROPOSAL: &str = "No proposal is open on this file, so there is no change to decide. Tell the author that nothing is waiting for a decision.";

/// A refusal of the whole call. Nothing is decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallRefusal {
    /// TLC-FR-13: the shared refusal.
    NoProjectOpen,
    /// DDC-FR-OEJB / DPC-FR-JJGY: the turn's origin is of the wrong kind.
    WrongConversation(&'static str),
    /// DDC-FR-WRFY: the shared malformed-arguments refusal.
    Malformed,
    /// DDC-FR-ZGXF / DPC-FR-RGGP.
    NoOpenProposal(&'static str),
    /// DDC-FR-ZYOL, DDC-FR-ELAE, DDC-FR-NKAB, DDC-FR-VONX.
    Authority(AuthorityRefusal),
}

impl CallRefusal {
    /// A stable code for the log record (DDC-FR-TBVA).
    pub fn reason(&self) -> &'static str {
        match self {
            CallRefusal::NoProjectOpen => "no_project_open",
            CallRefusal::WrongConversation(_) => "wrong_conversation",
            CallRefusal::Malformed => "invalid_arguments",
            CallRefusal::NoOpenProposal(_) => "no_open_proposal",
            CallRefusal::Authority(refusal) => refusal.reason(),
        }
    }

    /// TLC-FR-10 / TLC-FR-11: the refusal as `rig` delivers it.
    pub fn to_execution_error(&self) -> ToolExecutionError {
        match self {
            CallRefusal::NoProjectOpen => ToolRefusal::NoProjectOpen.to_execution_error(),
            CallRefusal::Malformed => {
                ToolRefusal::InvalidArguments(DECISIONS_MALFORMED).to_execution_error()
            }
            CallRefusal::WrongConversation(message) => {
                ToolExecutionError::new(ToolErrorKind::PermissionDenied, *message)
                    .with_retryable(false)
            }
            CallRefusal::NoOpenProposal(message) => {
                ToolExecutionError::new(ToolErrorKind::InvalidArgs, *message).with_retryable(true)
            }
            CallRefusal::Authority(refusal) => {
                let (kind, retryable) = match refusal {
                    AuthorityRefusal::SpeechNotPosted => (ToolErrorKind::Other, true),
                    AuthorityRefusal::NotAuthorized => (ToolErrorKind::PermissionDenied, false),
                    AuthorityRefusal::InstructionNotFound => (ToolErrorKind::InvalidArgs, true),
                    AuthorityRefusal::InstructionUsed => (ToolErrorKind::PermissionDenied, false),
                };
                ToolExecutionError::new(kind, authority_message(*refusal)).with_retryable(retryable)
            }
        }
    }
}

impl std::fmt::Display for CallRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallRefusal::NoProjectOpen => f.write_str(NO_PROJECT_OPEN),
            CallRefusal::Malformed => f.write_str(DECISIONS_MALFORMED),
            CallRefusal::WrongConversation(message) | CallRefusal::NoOpenProposal(message) => {
                f.write_str(message)
            }
            CallRefusal::Authority(refusal) => f.write_str(authority_message(*refusal)),
        }
    }
}

/// The DDC contract surface text of each authority refusal.
fn authority_message(refusal: AuthorityRefusal) -> &'static str {
    match refusal {
        AuthorityRefusal::SpeechNotPosted => SPEECH_NOT_POSTED,
        AuthorityRefusal::NotAuthorized => NOT_AUTHORIZED,
        AuthorityRefusal::InstructionNotFound => INSTRUCTION_NOT_FOUND,
        AuthorityRefusal::InstructionUsed => INSTRUCTION_USED,
    }
}

impl std::error::Error for CallRefusal {}

impl From<ToolRefusal> for CallRefusal {
    fn from(refusal: ToolRefusal) -> Self {
        match refusal {
            ToolRefusal::NoProjectOpen => CallRefusal::NoProjectOpen,
            _ => CallRefusal::Malformed,
        }
    }
}

// ---------------------------------------------------------------------------
// The backend a tool supplies
// ---------------------------------------------------------------------------

/// One proposal as it stands: its state and each hunk's id and state, in
/// proposal order.
#[derive(Debug, Clone)]
pub struct ProposalView {
    pub id: String,
    pub state: ProposalStateView,
    pub hunks: Vec<(String, HunkStateView)>,
}

impl ProposalView {
    fn state_of(&self, hunk_id: &str) -> Option<HunkStateView> {
        self.hunks.iter().find(|(id, _)| id == hunk_id).map(|(_, state)| *state)
    }
}

/// What a successful acceptance reports.
#[derive(Debug, Clone, Copy, Default)]
pub struct Accepted {
    /// DPC-FR-WNHU: the decision comment is still owed. `None` for a draft,
    /// whose output carries no such field.
    pub comment_owed: Option<bool>,
}

/// The operations of the review chips, for one store
/// (DCP-FR-LAWR, PCP-FR-MXSB).
pub trait ProposalBackend {
    /// "the prompt" or "the file", for the refusal messages.
    fn file_word(&self) -> &'static str;
    /// DDC-FR-ZGXF / DPC-FR-RGGP: the no-open-proposal message.
    fn no_open_message(&self) -> &'static str;
    /// The open proposal of the bound target, if one is open.
    fn open_proposal(&self) -> Option<ProposalView>;
    /// One proposal as it stands now.
    fn proposal(&self, proposal_id: &str) -> Option<ProposalView>;
    /// DCP-FR-BMLX: the hunk resolves nowhere in the file.
    fn is_lost(&self, proposal_id: &str, hunk_id: &str) -> bool;
    fn accept(&self, proposal_id: &str, hunk_id: &str) -> Result<Accepted, String>;
    fn reject(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String>;
    /// The discussions of the bound target, resolved ones included.
    fn discussions(&self) -> Vec<Discussion>;
    /// Opens a new hunk discussion and returns its id.
    fn open_discussion(&self, proposal_id: &str, hunk_id: &str) -> Result<String, String>;
    /// "hold change hunk" for a pending hunk.
    fn hold(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String>;
    /// DDC-FR-LYTA / DPC-FR-JEXJ: whether a typed error is worth a retry.
    fn retryable_error(&self, error: &str) -> bool;
    /// DDC-FR-ZGXF: whether something besides a change proposal is open, so a
    /// call without an open change proposal still runs.
    fn side_open(&self) -> bool {
        false
    }
    /// DDC-FR-GGWO: run an entry that names something besides a hunk — the
    /// new-draft proposal of a draft. `None`: the entry is about a hunk.
    fn side_entry(
        &self,
        _id: &str,
        _action: &str,
        _place: usize,
        _total: usize,
    ) -> Option<DecisionRow> {
        None
    }
}

/// DCP-FR-FHWN / DPC-FR-NXZF: a hunk discussion is one whose **opening**
/// comment carries a `proposal_hunk` that names the hunk.
pub fn is_hunk_discussion(discussion: &Discussion, proposal_id: &str, hunk_id: &str) -> bool {
    discussion.comments.first().is_some_and(|opening| {
        opening.attachments.iter().any(|attachment| {
            matches!(
                attachment,
                Attachment::ProposalHunk { proposal_id: p, hunk_id: h, .. }
                    if p == proposal_id && h == hunk_id
            )
        })
    })
}

/// The opening comment's attachment for a draft or an artifact hunk
/// (CMS-FR-TQVB). Exactly one of the two targets is present.
pub fn hunk_attachment(
    proposal_id: &str,
    hunk_id: &str,
    target: &comments::DiscussionTarget,
    path: &str,
) -> comments::AttachmentInput {
    let (draft_id, artifact_id, change) = match target {
        comments::DiscussionTarget::Draft { draft_id } => (Some(draft_id.clone()), None, None),
        comments::DiscussionTarget::Artifact { artifact_id } => {
            (None, Some(artifact_id.clone()), None)
        }
        comments::DiscussionTarget::Note { .. } => (None, None, None),
        // CMS-FR-YQAX: a hunk of a change edit proposal names its change.
        comments::DiscussionTarget::Change { stream_id, change } => (
            None,
            None,
            Some(comments::ChangeOwnerRef { stream_id: stream_id.clone(), change: change.clone() }),
        ),
    };
    comments::AttachmentInput::ProposalHunk {
        proposal_id: proposal_id.to_string(),
        hunk_id: hunk_id.to_string(),
        draft_id,
        artifact_id,
        change,
        path: path.to_string(),
    }
}

/// The action of one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Accept,
    Reject,
    Discuss,
}

impl Action {
    fn parse(text: &str) -> Option<Action> {
        match text {
            "accept" => Some(Action::Accept),
            "reject" => Some(Action::Reject),
            "discuss" => Some(Action::Discuss),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The entry loop (DDC-FR-DSTU)
// ---------------------------------------------------------------------------

/// DDC-FR-WRFY, DDC-FR-ZGXF, DDC-FR-DSTU: run the entries of one call, in
/// order, against the open proposal.
pub fn run_entries<B: ProposalBackend>(
    backend: &B,
    args: &DecideChangesArgs,
) -> Result<DecideChangesOutput, CallRefusal> {
    let entries = entries(args).map_err(CallRefusal::from)?;
    let open = backend.open_proposal();
    if open.is_none() && !backend.side_open() {
        return Err(CallRefusal::NoOpenProposal(backend.no_open_message()));
    }
    let total = entries.len();
    let mut rows = Vec::with_capacity(total);
    let mut closed = false;
    let mut last_state = open.as_ref().map(|o| o.state);
    for (index, entry) in entries.iter().enumerate() {
        let place = index + 1;
        // DDC-FR-GGWO: an entry naming the new-draft proposal.
        if let Some(row) = backend.side_entry(entry.id, entry.action, place, total) {
            rows.push(row);
            continue;
        }
        let Some(open) = &open else {
            let mut row = DecisionRow::new(place, entry.id, entry.action);
            let code = match Action::parse(entry.action) {
                None => RefusalCode::InvalidAction,
                Some(_) => RefusalCode::UnknownHunk,
            };
            row.refuse(code, total, &refusal_text(code, backend.file_word(), None), None);
            rows.push(row);
            continue;
        };
        // DDC-FR-DSTU: the proposal as it stands after the entries before.
        let now = backend.proposal(&open.id).unwrap_or_else(|| open.clone());
        let row = run_entry(backend, &now, entry, place, total);
        if let Some(after) = backend.proposal(&open.id) {
            if now.state == ProposalStateView::Pending && after.state != ProposalStateView::Pending
            {
                closed = true;
            }
            last_state = Some(after.state);
        }
        rows.push(row);
    }
    Ok(DecideChangesOutput { proposal_state: last_state, closed, decisions: rows })
}

/// One entry, refused or run (DDC-FR-JCSV, DDC-FR-EDMT, DDC-FR-XBQV,
/// DDC-FR-LYTA).
fn run_entry<B: ProposalBackend>(
    backend: &B,
    now: &ProposalView,
    entry: &Entry<'_>,
    place: usize,
    total: usize,
) -> DecisionRow {
    let mut row = DecisionRow {
        place,
        id: entry.id.to_string(),
        action: entry.action.to_string(),
        outcome: Outcome::Refused,
        hunk_state: now.state_of(entry.id),
        discussion_id: None,
        refusal: None,
        comment_owed: None,
        new_draft_state: None,
        created_draft_id: None,
        path: None,
        run_id: None,
    };
    let refuse = |row: &mut DecisionRow, code: RefusalCode, action: Option<Action>| {
        row.refusal = Some(EntryRefusal {
            code,
            error: None,
            message: format!(
                "Decision {place} of {total}: {}",
                refusal_text(code, backend.file_word(), action)
            ),
            // DDC-FR-SLBB: every refusal of these three groups is retryable.
            retryable: true,
        });
    };
    // DDC-FR-JCSV: the code that applies first.
    let Some(action) = Action::parse(entry.action) else {
        refuse(&mut row, RefusalCode::InvalidAction, None);
        return row;
    };
    // DDC-FR-XBQV: an earlier entry of this call closed the proposal.
    if now.state != ProposalStateView::Pending {
        refuse(&mut row, RefusalCode::NoOpenProposal, Some(action));
        return row;
    }
    let Some(state) = row.hunk_state else {
        refuse(&mut row, RefusalCode::UnknownHunk, Some(action));
        return row;
    };
    let code = match state {
        HunkStateView::Withdrawn => Some(RefusalCode::HunkWithdrawn),
        HunkStateView::Accepted => Some(RefusalCode::HunkAccepted),
        // The confirmed rule: a lost hunk is refused for every action.
        _ if backend.is_lost(&now.id, entry.id) => Some(RefusalCode::HunkLost),
        // DDC-FR-EDMT: a reject of a rejected hunk.
        HunkStateView::Rejected if action == Action::Reject => Some(RefusalCode::AlreadyInState),
        _ => None,
    };
    if let Some(code) = code {
        refuse(&mut row, code, Some(action));
        return row;
    }
    // DDC-FR-EDMT: a discuss of a hunk that already has a hunk discussion, a
    // resolved one included. The discussions are read for a discuss alone.
    if action == Action::Discuss
        && backend.discussions().iter().any(|d| is_hunk_discussion(d, &now.id, entry.id))
    {
        refuse(&mut row, RefusalCode::AlreadyInState, Some(action));
        return row;
    }
    let ran = match action {
        Action::Accept => backend.accept(&now.id, entry.id).map(|accepted| {
            row.outcome = Outcome::Accepted;
            row.comment_owed = accepted.comment_owed;
        }),
        Action::Reject => backend.reject(&now.id, entry.id).map(|()| {
            row.outcome = Outcome::Rejected;
        }),
        Action::Discuss => backend.open_discussion(&now.id, entry.id).map(|discussion_id| {
            // DDC-FR-NKIA / DPC-FR-QPNB: a pending hunk is held after the
            // discussion opens. A rejected hunk stays rejected. A hold that
            // fails leaves the discussion standing, as the chip does.
            if state == HunkStateView::Pending {
                let _ = backend.hold(&now.id, entry.id);
            }
            row.outcome = Outcome::DiscussionOpened;
            row.discussion_id = Some(discussion_id);
        }),
    };
    if let Err(error) = ran {
        // DDC-FR-LYTA: the backend's typed error, retryable for a few.
        let retryable = backend.retryable_error(&error);
        row.refusal = Some(EntryRefusal {
            code: RefusalCode::DecisionFailed,
            message: format!(
                "Decision {place} of {total}: {} ({error}).",
                refusal_text(RefusalCode::DecisionFailed, backend.file_word(), Some(action))
            ),
            error: Some(error),
            retryable,
        });
        row.outcome = Outcome::Refused;
        row.comment_owed = None;
        row.discussion_id = None;
    }
    // The hunk's state after the entry.
    if let Some(after) = backend.proposal(&now.id) {
        row.hunk_state = after.state_of(entry.id).or(row.hunk_state);
    }
    row
}

// ---------------------------------------------------------------------------
// The log records (DDC-FR-TBVA, DPC-FR-CIRL)
// ---------------------------------------------------------------------------

/// DDC-FR-TBVA: one `INFO` record for the call, with the count of each
/// outcome, and one `WARN` record for each refused entry. No record carries
/// hunk text, comment text, the `instruction`, or the agent's input.
pub fn log_call<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    target: (&'static str, &str),
    outcome: &Result<DecideChangesOutput, CallRefusal>,
) {
    log_call_with(sink, buffer, tool, &[target], outcome);
}

/// [`log_call`] with several target fields: DCE-FR-GAHP names the stream id
/// and the change name.
pub fn log_call_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    targets: &[(&'static str, &str)],
    outcome: &Result<DecideChangesOutput, CallRefusal>,
) {
    let put = |fields: &mut crate::logging::Fields| {
        for (key, value) in targets {
            if !value.is_empty() {
                fields.insert(key.to_string(), serde_json::json!(value));
            }
        }
    };
    match outcome {
        Ok(output) => {
            for row in &output.decisions {
                if let Some(refusal) = &row.refusal {
                    let mut fields = log_fields! {
                        "tool" => tool,
                        "code" => refusal.code.as_str(),
                        "place" => row.place,
                        "retryable" => refusal.retryable,
                    };
                    put(&mut fields);
                    if let Some(error) = &refusal.error {
                        fields.insert("error".to_string(), serde_json::json!(error));
                    }
                    logging::log_warn(sink, buffer, &DOMAINS, "tool decision refused", fields);
                }
            }
            let count = |outcome: Outcome| {
                output.decisions.iter().filter(|row| row.outcome == outcome).count()
            };
            let mut fields = log_fields! {
                "tool" => tool,
                "entries" => output.decisions.len(),
                "closed" => output.closed,
            };
            for outcome in [
                Outcome::Accepted,
                Outcome::Rejected,
                Outcome::DiscussionOpened,
                Outcome::Confirmed,
                Outcome::Started,
                Outcome::Declined,
                Outcome::Refused,
            ] {
                fields.insert(outcome.as_str().to_string(), serde_json::json!(count(outcome)));
            }
            put(&mut fields);
            logging::log_info(sink, buffer, &DOMAINS, "tool call completed", fields);
        }
        Err(refusal) => {
            let mut fields = log_fields! {
                "tool" => tool,
                "reason" => refusal.reason(),
            };
            put(&mut fields);
            logging::log_warn(sink, buffer, &DOMAINS, "tool call refused", fields);
        }
    }
}

#[cfg(test)]
mod tests;
