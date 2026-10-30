//! Propose prompt changes tool (`PPC-propose-prompt-changes-tool.md`).
//!
//! The tool an agent reaches for when it can see how a **prompt the project
//! already holds** would be better and wants the author to decide. It
//! **proposes** changes to that prompt and the turn ends there (CVL-FR-15). The
//! author decides each change separately (PPC-FR-CVHT), with a word of feedback
//! if they have one, and the decision that settles the last change brings the
//! agent back with the whole conversation in front of it.
//!
//! ## Why it takes hunks as well as a whole text
//!
//! A document can be far larger than the agent's context. With `hunks` the
//! agent names only the passages it changes, and the backend places them
//! against the file on disk (PPC-FR-RJCA). A `content` call is converted into
//! changes at record time (PCP-FR-CNVT), so the author reviews changes
//! whichever form the agent used (PPC-FR-JGSH, PPC-FR-CVHT).
//!
//! ## Why it can name a change it revises
//!
//! An author who discusses one change asks the agent to reconsider it. With
//! `revises` the agent replaces that one change in place rather than proposing
//! again (PPC-FR-RVKS, PCP-FR-RVSN), so the discussion about it stays attached.
//!
//! ## Why nothing it does changes the file
//!
//! This is the point of the tool rather than a limitation of it. A proposal is
//! material sitting beside the project until the author accepts it, and the
//! write that lands it is one *they* perform from the surface that showed it to
//! them (`PCR-prompt-change-review.md`). An agent whose proposal is declined has
//! cost the project nothing but a comment.
//!
//! ## Why it is constructed per turn
//!
//! Like `ask_user_comment`'s and `propose_draft_changes`'. This one proposes
//! *into* a conversation, and which is not a decision a model may make: the
//! conversation and the agent participant are bound by the constructor
//! (PPC-FR-03, TLC-FR-15), so no argument can post into a conversation the agent
//! was not asked about. What the model does name is the **target**, through
//! `path` alone — and that target has to exist and has to resolve as a Prompt
//! (PPC-FR-06).
//!
//! ## Why it reaches no draft
//!
//! `PDC-propose-draft-changes-tool.md`'s tool is the one that rewrites a draft's
//! prompt (PPC-FR-21). The two are attached to disjoint origin kinds
//! (CVL-FR-08), record into different stores, and produce different attachment
//! kinds, so an agent offered one is never offered the other.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    PROMPT_PROPOSAL_RATIONALE_BLANK, PROPOSAL_NO_CHANGE,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::Participant;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::prompt_proposals::{
    self, NewPromptProposal, ProposedChange, RecordRefusal, RevisionTarget,
};
use crate::text_hunks::hunks::{ContextSide, ProposedHunk};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (PPC contract surface)
// ---------------------------------------------------------------------------

/// PPC-FR-01.
pub const NAME: &str = "propose_prompt_changes";

/// PPC-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Propose changes to a prompt or a project document the project holds, when you can see how its text would be better and want the author to decide. It reaches a prompt or a project document and no other kind of file, and the file has to exist already — this offers changes and never a new file. Reach for it instead of describing the improvement in prose, and instead of asking permission to make one. Give the change in one of two forms, never both. Pass `hunks`, a list of changes: each one names, in `before`, the exact text it changes, copied character for character from the file, and gives in `after` the text that should stand in its place; to insert, give `after` alone and put the exact text it follows in `after_text`; to delete, give `before` alone. Or pass `content` with the complete new text of the file, and it is converted into changes for you. Use `hunks` for a large document: you do not need the whole file in front of you, only the parts you change, which you can read with `offset` and `limit` on `read_file`. The text you name must appear once in the file. Where it can appear more than once, put the exact text immediately before it in `lead`, or the exact text immediately after it in `trail`, up to 200 characters each. A change whose text is not found, or not found once, is refused and you are told which change to correct. The author decides each change separately, so send the changes you believe in together rather than holding some back. Say why the changes are worth making, in a sentence or two as you would put it to a colleague. Your proposal is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's decisions, and whatever they said about them, in front of you. When you are answering about one change the author asked you to reconsider, name that change in `revises` and send only the one change that replaces it in `hunks`. It changes nothing — the file is not rewritten unless the author accepts.";

const PATH_DESCRIPTION: &str = "The project-relative path of the prompt or project document you are proposing changes to. The file has to exist and has to be a prompt or a project document; anything else is refused and nothing is created.";

const CONTENT_DESCRIPTION: &str = "The file's complete new text: the whole document as it should stand once every change lands. Not a patch, not a diff, not only the part you changed. It is converted into separate changes, and the author decides each of them. Leave it out when you pass `hunks`. Pass an empty text only if you mean to propose emptying the file.";

const HUNKS_DESCRIPTION: &str = "The changes, in the order they should be read, when you do not pass `content`. In each, `before` is the exact existing text you are changing, copied character for character from the file, and `after` is what should stand in its place. For an insertion give `after` and put the exact text it follows in `after_text`; for a deletion give `before` alone. The text you name must appear only once in the file. When it does not, give `lead`, the exact text immediately before it, or `trail`, the exact text immediately after it, or both, up to 200 characters each: the text `lead` + `before` + `trail` must then appear once, and only `before` changes. For an insertion, `lead` and `trail` stand around `after_text`. Two changes must not cover the same text.";

/// PPC-FR-RVKS.
const REVISES_DESCRIPTION: &str = "The id of a change you were asked about, when you are replacing it with a better one. A call that names a change carries exactly one change in `hunks` — the one that replaces it. Leave it out when you are proposing something new.";

const HUNK_DESCRIPTION: &str = "One change to the file. A replacement gives `before` and `after`; an insertion gives `after` and `after_text`; a deletion gives `before` alone.";

const KIND_DESCRIPTION: &str = "What this change does: `replace` puts `after` in place of `before`, `del` removes `before`, `add` inserts `after` immediately following `after_text`. The fields you send are what decide.";

const BEFORE_DESCRIPTION: &str = "The exact existing text this change replaces or deletes, copied character for character from the file. It must appear only once in the file; where it does not, add `lead` or `trail`. Leave it out for an insertion.";

const AFTER_DESCRIPTION: &str = "The exact text that should stand in place of `before`, or the text to insert. Leave it out for a deletion.";

const AFTER_TEXT_DESCRIPTION: &str = "For an insertion only: the exact existing text the new text goes immediately after, copied character for character from the file.";

const LEAD_DESCRIPTION: &str = "Optional. The exact text immediately before `before` (for an insertion, immediately before `after_text`), up to 200 characters. Give it when the text you change appears more than once. It is not changed.";

const TRAIL_DESCRIPTION: &str = "Optional. The exact text immediately after `before` (for an insertion, immediately after `after_text`), up to 200 characters. Give it when the text you change appears more than once. It is not changed.";

/// PPC-FR-XHHT.
pub const PROPOSAL_NOT_ONE_FORM: &str =
    "Pass exactly one of `content`, the complete new text, or `hunks`, the list of changes.";
/// PPC-FR-DLHS.
pub const PROPOSAL_NO_HUNKS: &str =
    "Pass at least one change. An empty list has nothing for the author to decide.";
/// PPC-FR-DLHS.
pub const PROPOSAL_TEXT_NOT_FOUND: &str =
    "The text you gave in `before` is not in the file. Copy it character for character from the file as you read it.";
/// PPC-FR-RXCS: the same refusal with and without `lead` and `trail`.
pub const PROPOSAL_TEXT_NOT_UNIQUE: &str =
    "The text you gave in `before` appears more than once in the file. Add `lead` or `trail` with the exact text around it so it names one place.";
/// PPC-FR-DLHS.
pub const PROPOSAL_HUNKS_OVERLAP: &str =
    "Two of your changes cover the same text. Combine them into one change.";
/// PPC-FR-KWQB.
pub const PROPOSAL_LEAD_TOO_LONG: &str =
    "The `lead` you gave is longer than 200 characters. Shorten `lead` to the few words just before the text you name.";
/// PPC-FR-KWQB.
pub const PROPOSAL_TRAIL_TOO_LONG: &str =
    "The `trail` you gave is longer than 200 characters. Shorten `trail` to the few words just after the text you name.";
/// PPC-FR-KWQB.
pub const PROPOSAL_LEAD_AND_TRAIL_TOO_LONG: &str =
    "The `lead` and the `trail` you gave are each longer than 200 characters. Shorten both to the few words next to the text you name.";

/// PPC-FR-RVKS: the **no such change to revise** refusal.
pub const PROPOSAL_NO_SUCH_CHANGE: &str =
    "The change you named is not one of the changes you can still revise. Propose a new change instead.";
/// PPC-FR-RVKS: the **more than one change beside a revision** refusal.
pub const PROPOSAL_REVISE_ONE: &str =
    "You named a change to replace, so send only the one change that replaces it in `hunks`. Wait for the author to decide the changes already standing before you propose the rest.";

/// The refusal for an error of a revision (PPC-FR-RVKS, PCP-FR-RVSN).
///
/// A revision is placed against the file exactly as a recording is, so it
/// refuses in the same words for the same mistakes. A revision carries one
/// change, so a refusal names no place.
fn revision_refusal(error: &str) -> ToolRefusal {
    let side = error
        .strip_prefix(prompt_proposals::ERR_HUNK_CONTEXT_TOO_LONG)
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(ContextSide::parse);
    if let Some(side) = side {
        return ToolRefusal::InvalidArguments(context_too_long(side));
    }
    match error {
        prompt_proposals::ERR_ANCHOR_LOST => ToolRefusal::InvalidArguments(PROPOSAL_TEXT_NOT_FOUND),
        prompt_proposals::ERR_HUNK_AMBIGUOUS => {
            ToolRefusal::InvalidArguments(PROPOSAL_TEXT_NOT_UNIQUE)
        }
        prompt_proposals::ERR_HUNK_NOT_FOUND
        | prompt_proposals::ERR_HUNK_ALREADY_DECIDED
        | prompt_proposals::ERR_PROPOSAL_NOT_FOUND => {
            ToolRefusal::InvalidArguments(PROPOSAL_NO_SUCH_CHANGE)
        }
        _ => ToolRefusal::ProposalNotRecorded,
    }
}

/// The refusal text for the context that is too long (PDC-FR-DMTH, PPC-FR-KWQB).
fn context_too_long(side: ContextSide) -> &'static str {
    match side {
        ContextSide::Lead => PROPOSAL_LEAD_TOO_LONG,
        ContextSide::Trail => PROPOSAL_TRAIL_TOO_LONG,
        ContextSide::Both => PROPOSAL_LEAD_AND_TRAIL_TOO_LONG,
    }
}

/// PPC-FR-19: which of this tool's invalid-argument refusals fired, as a
/// stable code, so a record names the correction the model was asked to make
/// without carrying any of the text it sent.
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    let message = match refusal {
        ToolRefusal::InvalidArguments(message) => message,
        ToolRefusal::InvalidArgumentsAt { message, .. } => message,
        _ => return None,
    };
    Some(match *message {
        PROPOSAL_NOT_ONE_FORM => "not_one_form",
        PROPOSAL_NO_HUNKS => "no_changes",
        PROPOSAL_TEXT_NOT_FOUND => "text_not_found",
        PROPOSAL_TEXT_NOT_UNIQUE => "text_not_unique",
        PROPOSAL_HUNKS_OVERLAP => "changes_overlap",
        PROPOSAL_LEAD_TOO_LONG => "lead_too_long",
        PROPOSAL_TRAIL_TOO_LONG => "trail_too_long",
        PROPOSAL_LEAD_AND_TRAIL_TOO_LONG => "lead_and_trail_too_long",
        PROPOSAL_NO_SUCH_CHANGE => "no_such_change",
        PROPOSAL_REVISE_ONE => "revise_one",
        PROMPT_PROPOSAL_RATIONALE_BLANK => "rationale_blank",
        PROPOSAL_NO_CHANGE => "no_change",
        _ => "other",
    })
}

const RATIONALE_DESCRIPTION: &str = "Why the changes are worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment.";

/// CVL-FR-15: what a second proposal in one reply is told.
///
/// The loop never dispatches it, so this is the result the model reads in place
/// of one. It exists for the reason `ask_user_comment`'s and
/// `propose_draft_changes`' twins do: a reply *can* propose twice, and were the
/// second carried out after the first landed, one turn would make two
/// contributions (AGC-FR-01).
pub const ONE_AT_A_TIME: &str = "Only one change can be proposed at a time, so this one was not recorded. Propose the single change that matters most, in one call.";

/// PPC-FR-19: how much of `path` reaches a log record.
///
/// A project-relative path is short by construction, and the bound is what keeps
/// a model-composed value from growing a record past `LGC-logging.md`'s
/// per-record ceiling (LGC-FR-08) and costing it the tool name and the reason.
const PATH_LOG_CHARS: usize = 200;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). There is
/// deliberately no parameter naming a conversation, a thread, a participant, or
/// an agent: see the module docs.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposePromptChangesArgs {
    pub path: String,
    /// PPC-FR-RVKS: the id of a change of the standing proposal this call
    /// replaces.
    #[serde(default)]
    pub revises: Option<String>,
    /// PPC-FR-XHHT: the complete new text, or absent when `hunks` is given.
    #[serde(default)]
    pub content: Option<String>,
    /// PPC-FR-XHHT: the changes, or absent when `content` is given.
    #[serde(default)]
    pub hunks: Option<Vec<PromptHunkArg>>,
    pub rationale: String,
}

/// One change as the model composed it (PPC-FR-RJCA). A `note` the model sends
/// is an unknown field and is ignored (TLC-FR-07): the rationale is what the
/// author reads.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptHunkArg {
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

impl PromptHunkArg {
    /// The change in the neutral hunk shape, its kind read from its fields.
    fn to_proposed(&self) -> ProposedHunk {
        ProposedHunk::from_fields(
            self.before.clone(),
            self.after.clone(),
            self.after_text.clone(),
            self.lead.clone(),
            self.trail.clone(),
            None,
        )
    }
}

/// TLC-FR-08 / PPC-FR-15: a JSON object with stable named fields, carrying the
/// identity `PCP-prompt-change-proposals.md` recorded and no part of the
/// candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProposePromptChangesOutput {
    pub proposed: bool,
    pub proposal_id: String,
}

/// TLC-FR-06: the JSON Schema for [`ProposePromptChangesArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": PATH_DESCRIPTION },
            "revises": { "type": "string", "description": REVISES_DESCRIPTION },
            "content": { "type": "string", "description": CONTENT_DESCRIPTION },
            "hunks": {
                "type": "array",
                "description": HUNKS_DESCRIPTION,
                "items": {
                    "type": "object",
                    "description": HUNK_DESCRIPTION,
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["replace", "del", "add"],
                            "description": KIND_DESCRIPTION,
                        },
                        "before": { "type": "string", "description": BEFORE_DESCRIPTION },
                        "after": { "type": "string", "description": AFTER_DESCRIPTION },
                        "after_text": { "type": "string", "description": AFTER_TEXT_DESCRIPTION },
                        "lead": { "type": "string", "description": LEAD_DESCRIPTION },
                        "trail": { "type": "string", "description": TRAIL_DESCRIPTION },
                    },
                },
            },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
        },
        // PPC-FR-XHHT: exactly one of `content` and `hunks`, which the tool
        // checks itself, so neither is listed as required.
        "required": ["path", "rationale"],
    })
}

// ---------------------------------------------------------------------------
// The conversation (PPC-FR-03, PPC-FR-07)
// ---------------------------------------------------------------------------

/// The conversation a turn is about, or the refusal for one that is about
/// not about an artifact.
///
/// PPC-FR-07: a draft or note origin is the not-retryable refusal. It is
/// unreachable while CVL-FR-08 attaches this tool to artifact origins alone, and
/// checked anyway — a tool's refusals are its own contract, and a set selected
/// elsewhere is not a guarantee this tool may rest on.
fn thread_of(origin: &ConversationOrigin) -> Result<&str, ToolRefusal> {
    if origin.is_artifact() {
        Ok(origin.discussion_id())
    } else {
        Err(ToolRefusal::NotAnArtifactConversation)
    }
}

/// PPC-FR-05: whether `path` is a project-relative path this tool will act on at
/// all.
///
/// An absolute path, a path escaping the project root, and a path with no
/// content are each the same answer as a path naming nothing: `resolve_under`
/// would refuse them, and this says so before a byte is read.
fn is_project_relative(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return false;
    }
    // A Windows drive letter is absolute too, and `resolve_under` treats it so.
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        return false;
    }
    !trimmed.split(['/', '\\']).any(|segment| segment == ".." || segment == ".")
}

/// PPC-FR-19: which form the model sent, as a code a record can carry.
fn form_of(args: &ProposePromptChangesArgs) -> &'static str {
    match (&args.content, &args.hunks) {
        (Some(_), None) => "content",
        (None, Some(_)) => "hunks",
        (Some(_), Some(_)) => "both",
        (None, None) => "neither",
    }
}

/// `crate::prompt_proposals`' refusals in this tool's terms (PPC-FR-06,
/// PPC-FR-08, PPC-FR-09, PPC-FR-10, PPC-FR-12).
fn refusal_for(refusal: RecordRefusal) -> ToolRefusal {
    match refusal {
        RecordRefusal::ProposalPending => ToolRefusal::PromptProposalPending,
        RecordRefusal::ArtifactNotFound => ToolRefusal::PromptProposalPathMissing,
        RecordRefusal::NotAPromptArtifact => ToolRefusal::NotAPromptArtifact,
        RecordRefusal::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        RecordRefusal::NoHunks => ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS),
        // PPC-FR-DLHS: which of the changes, as the model counted them.
        RecordRefusal::HunkAnchorLost { at, of } => {
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_FOUND, at, of }
        }
        RecordRefusal::HunkAmbiguous { at, of } => {
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_UNIQUE, at, of }
        }
        RecordRefusal::HunkOverlap { at, of } => {
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_HUNKS_OVERLAP, at, of }
        }
        RecordRefusal::HunkContextTooLong { at, of, side } => {
            ToolRefusal::InvalidArgumentsAt { message: context_too_long(side), at, of }
        }
        RecordRefusal::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        RecordRefusal::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// PPC-FR-01: `propose_prompt_changes` as a `rig` portable tool.
///
/// `proposed` is how the turn's loop learns that a proposal actually went out.
/// CVL-FR-15 turns on the difference between a call that recorded one — which
/// ends the turn — and one that refused, which ends nothing.
pub struct ProposePromptChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposePromptChangesTool<R> {
    /// TLC-FR-19 / TLC-FR-15: this tool's own constructor, naming the
    /// conversation it posts into and the participant it proposes as.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposePromptChangesTool { app, roots, origin, author, proposed, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// The whole call, with its logging (TLC-FR-14, PPC-FR-19).
    fn run(
        &self,
        args: ProposePromptChangesArgs,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        // PPC-FR-19: the conversation, bound at construction rather than
        // composed by the model, plus `path` — this tool's one loggable
        // argument, because a record about a proposed rewrite is unfollowable
        // without saying which file was rewritten. Never the `content`, the
        // `rationale`, or any part of the file being replaced. The form and the
        // count of hunks are a shape and carry no text.
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "path" => super::bounded_argument(&args.path, PATH_LOG_CHARS),
            "form" => form_of(&args),
            "hunks" => args.hunks.as_ref().map_or(0, Vec::len),
            "revising" => args.revises.is_some(),
        };
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".to_string(), serde_json::json!(code));
                }
                // PPC-FR-19: the place of the hunk a refusal is about, and no
                // part of what the model wrote.
                if let ToolRefusal::InvalidArgumentsAt { at, .. } = refusal {
                    fields.insert("refusalAt".to_string(), serde_json::json!(at));
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn propose(
        &self,
        args: ProposePromptChangesArgs,
    ) -> Result<ProposePromptChangesOutput, ToolRefusal> {
        // PPC-FR-17: no project, no artifact to propose to, so the shared
        // refusal (TLC-FR-13). Checked per call rather than at construction: the
        // root this tool holds was bound when the turn began, and a turn can
        // outlive the project being open.
        require_open_project(&self.app)?;
        let thread_id = thread_of(&self.origin)?;

        // PPC-FR-XHHT: one form, never both and never neither. Checked first,
        // because no other answer means anything until the model has said
        // which change it is making.
        let hunks: Vec<ProposedHunk> = match (&args.content, &args.hunks) {
            (Some(_), None) => Vec::new(),
            (None, Some(hunks)) => hunks.iter().map(PromptHunkArg::to_proposed).collect(),
            _ => return Err(ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM)),
        };
        // PPC-FR-DLHS: an empty list is nothing for the author to decide.
        if args.hunks.as_ref().is_some_and(Vec::is_empty) {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS));
        }
        // PPC-FR-08, before anything is resolved on disk. An empty `content` is
        // deliberately **not** among these: emptying a prompt is a change like
        // any other, and it meets the no-change refusal on the one term every
        // candidate meets it on.
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROMPT_PROPOSAL_RATIONALE_BLANK));
        }
        // PPC-FR-05: a path that is not a project-relative path names no file
        // this tool will act on, which is the same answer as a path naming
        // nothing.
        if !is_project_relative(&args.path) {
            return Err(ToolRefusal::PromptProposalPathMissing);
        }

        // PPC-FR-RVKS (on the terms of PDC-FR-GMWR): `revises` is read for what
        // it can mean on the artifact as it stands. A name the artifact's
        // standing proposal does not hold as a revisable change is refused;
        // where no proposal stands, the field is ignored (TLC-FR-07), so a model
        // that filled it by mistake can still propose.
        let artifact_id = args.path.trim();
        let named = args.revises.as_deref().map(str::trim).filter(|id| !id.is_empty());
        let target = named.map(|id| {
            (id, prompt_proposals::revision_target(&self.roots.worktree, artifact_id, id))
        });
        if let Some((_, RevisionTarget::NoSuchChange)) = target {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_SUCH_CHANGE));
        }
        if let Some((revises, RevisionTarget::Revisable)) = target {
            // The one-change rule is checked once the name has resolved, so a
            // model that named a change wrongly is told that first. A `content`
            // call is a whole file, never the one change that replaces another.
            if args.content.is_some() || hunks.len() != 1 {
                return Err(ToolRefusal::InvalidArguments(PROPOSAL_REVISE_ONE));
            }
            let revised = prompt_proposals::revise_prompt_change_hunk(
                &self.roots.worktree,
                artifact_id,
                revises,
                &hunks[0],
            )
            .map_err(|error| revision_refusal(&error))?;
            // PCP-FR-17: a revision is announced like a decision, and it posts
            // no comment.
            prompt_proposals::announce(&self.app, self.buffer, &revised);
            self.proposed.store(true, Ordering::SeqCst);
            return Ok(ProposePromptChangesOutput { proposed: true, proposal_id: revised.id });
        }

        // PPC-FR-RJCA / PCP-FR-CNVT: hunks are resolved against the file on
        // disk by the recording path, and a `content` change is converted into
        // hunks there.
        let change = match &args.content {
            Some(content) => ProposedChange::Content(content),
            None => ProposedChange::Hunks(&hunks),
        };

        // PPC-FR-12: the candidate and the comment are one act, and neither this
        // tool nor the model performs either write — `crate::prompt_proposals`
        // does, so a failure of one leaves neither behind (PCP-FR-06).
        let recorded = prompt_proposals::record_prompt_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            NewPromptProposal {
                artifact_id,
                change,
                rationale: &args.rationale,
                agent: &self.author,
                thread_id,
            },
        )
        .map_err(refusal_for)?;

        // CVL-FR-15: the loop reads this to tell a recorded proposal from a
        // refused one. Set only after the record and its comment have both
        // landed, so a turn is never ended awaiting a decision on nothing.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposePromptChangesOutput { proposed: true, proposal_id: recorded.id })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposePromptChangesTool<R> {
    const NAME: &'static str = NAME;

    type Args = ProposePromptChangesArgs;
    type Output = ProposePromptChangesOutput;
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
        // PPC-FR-11: records and returns. It waits for no decision, holds no
        // timer, and opens no watcher, so it resolves before the future is ever
        // polled.
        std::future::ready(self.run(arguments))
    }
}
