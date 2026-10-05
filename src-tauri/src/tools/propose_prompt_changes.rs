//! Propose prompt changes tool (`PPC-propose-prompt-changes-tool.md`).
//!
//! The tool an agent reaches for when it can see how a **prompt the project
//! already holds** would be better and wants the author to decide. It
//! **proposes** a new version of that prompt — its complete text as it should
//! stand once the change lands — and the turn ends there (CVL-FR-15). The author
//! accepts it or declines it, with a word of feedback if they have one, and that
//! decision brings the agent back with the whole conversation in front of it.
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
use crate::prompt_proposals::{self, NewPromptProposal, RecordRefusal};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (PPC contract surface)
// ---------------------------------------------------------------------------

/// PPC-FR-01.
pub const NAME: &str = "propose_prompt_changes";

/// PPC-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Propose an improved version of a prompt the project holds, when you can see how its text would be better and want the author to decide. It reaches a file whose type is Prompt and no other kind of file, and the file has to exist already — this offers a rewrite and never a new file. Reach for it instead of describing the improvement in prose, and instead of asking permission to make one. Pass its complete new text — every line the prompt should hold once the change lands, and not a patch, not a diff, not the changed section on its own, and not a description of what to change: whatever you pass is what the prompt becomes if the author accepts. Say why the change is worth making, in a sentence or two as you would put it to a colleague. Your proposal is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's decision, and whatever they said about it, in front of you. Propose one change at a time: because your turn ends here, anything else you asked for in the same message is not carried out. It changes nothing — the file is not rewritten unless the author accepts.";

const PATH_DESCRIPTION: &str = "The project-relative path of the prompt you are proposing a new version of. The file has to exist and its type has to be Prompt; anything else is refused and nothing is created.";

const CONTENT_DESCRIPTION: &str = "The prompt's complete new text: the whole document as it should stand once the change lands. Not a patch, not a diff, not only the part you changed. If the author accepts, this becomes the file's entire contents. Pass nothing at all only if you mean to propose emptying the file, which is a change like any other and is offered to the author the same way.";

const RATIONALE_DESCRIPTION: &str = "Why the change is worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment, beside the comparison.";

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
    pub content: String,
    pub rationale: String,
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
            "content": { "type": "string", "description": CONTENT_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
        },
        "required": ["path", "content", "rationale"],
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
    !trimmed
        .split(['/', '\\'])
        .any(|segment| segment == ".." || segment == ".")
}

/// `crate::prompt_proposals`' refusals in this tool's terms (PPC-FR-06,
/// PPC-FR-08, PPC-FR-09, PPC-FR-10, PPC-FR-12).
fn refusal_for(refusal: RecordRefusal) -> ToolRefusal {
    match refusal {
        RecordRefusal::ProposalPending => ToolRefusal::PromptProposalPending,
        RecordRefusal::ArtifactNotFound => ToolRefusal::PromptProposalPathMissing,
        RecordRefusal::NotAPromptArtifact => ToolRefusal::NotAPromptArtifact,
        RecordRefusal::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        RecordRefusal::ThreadLocked => ToolRefusal::ProposalConversationLocked,
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
        ProposePromptChangesTool {
            app,
            roots,
            origin,
            author,
            proposed,
            buffer: &BUFFER,
        }
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
        // `rationale`, or any part of the file being replaced.
        let fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "path" => super::bounded_argument(&args.path, PATH_LOG_CHARS),
        };
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields),
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

        // PPC-FR-08, before anything is resolved on disk. An empty `content` is
        // deliberately **not** among these: emptying a prompt is a change like
        // any other, and it meets the no-change refusal on the one term every
        // candidate meets it on.
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(
                PROMPT_PROPOSAL_RATIONALE_BLANK,
            ));
        }
        // PPC-FR-05: a path that is not a project-relative path names no file
        // this tool will act on, which is the same answer as a path naming
        // nothing.
        if !is_project_relative(&args.path) {
            return Err(ToolRefusal::PromptProposalPathMissing);
        }

        // PPC-FR-12: the candidate and the comment are one act, and neither this
        // tool nor the model performs either write — `crate::prompt_proposals`
        // does, so a failure of one leaves neither behind (PCP-FR-06).
        let recorded = prompt_proposals::record_prompt_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            NewPromptProposal {
                artifact_id: args.path.trim(),
                content: &args.content,
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
        Ok(ProposePromptChangesOutput {
            proposed: true,
            proposal_id: recorded.id,
        })
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
