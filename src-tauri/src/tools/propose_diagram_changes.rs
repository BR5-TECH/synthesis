//! Propose diagram changes tool (`PDM-propose-diagram-changes-tool.md`).
//!
//! The tool an agent reaches for when it can see how a **Mermaid diagram** in
//! the document under discussion would be better. It **proposes** the change,
//! the addition, or the removal of diagrams and ends the agent's turn there.
//! The document is the prompt of a draft, or a prompt or a project document the
//! project holds.
//!
//! ## Why a tool of its own
//!
//! The author must compare two drawings and not two pieces of source. A diagram
//! change is therefore a **diagram hunk** (DCP-FR-QRVD, PCP-FR-QRVD), which the
//! review draws as a diagram, with a button that opens a side-by-side
//! comparison. The text tools never carry a diagram change, and this tool never
//! carries a text change.
//!
//! ## Why it joins the open proposal
//!
//! A file holds at most one undecided proposal. A call with `diagrams` records
//! a proposal where none is open, and adds its hunks to the open proposal
//! otherwise, as `add` updates (PDM-FR-WLBN).
//!
//! ## Why it checks the source
//!
//! A new source that does not parse would leave the author with a hunk that
//! cannot be compared. The tool refuses it with the parse error while the model
//! can still correct it (PDM-FR-HOQI).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::proposal_updates::{
    self, updates_or_none, UpdateArg, UPDATE_CHANGE_SETTLED, UPDATE_MALFORMED, UPDATE_NAMED_TWICE,
    UPDATE_NONE, UPDATE_NO_SUCH_CHANGE,
};
use super::propose_draft_changes::{
    context_too_long, target_for, PATH_LOG_CHARS, PROPOSAL_HUNKS_OVERLAP, PROPOSAL_NO_HUNKS,
};
use super::propose_prompt_changes::is_project_relative;
use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    PROMPT_PROPOSAL_RATIONALE_BLANK, PROPOSAL_NO_CHANGE, PROPOSAL_RATIONALE_BLANK,
    RATIONALE_NO_SPOKEN_PART,
};
use crate::agent_conversations::ConversationOrigin;
use crate::comments::Participant;
use crate::draft_proposals::{self, NewProposal, ProposalUpdate};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::prompt_proposals::{self, NewPromptProposal, PromptProposalUpdate, ProposedChange};
use crate::text_hunks::hunks::ProposedHunk;
use crate::text_hunks::updates::UpdateEntry;

mod validate;
pub use validate::{check_source, SourceFault};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (PDM contract surface)
// ---------------------------------------------------------------------------

/// PDM-FR-ZXQE.
pub const NAME: &str = "propose_diagram_changes";

/// PDM-FR-MBHW: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Propose changes to Mermaid diagrams in the prompt of the draft under discussion, or in the prompt or project document under discussion, when you can see how a diagram would be better and want the author to decide. Use this tool for every change to a Mermaid diagram. A change to a diagram never goes in a text change, and never in the same call as text changes: send the text changes in their own call. Reach for it instead of describing the improvement in prose, and instead of asking permission to make one. Pass a list of diagram changes. To change a diagram, give in `before` its **current source**, copied character for character from the file as you read it, and give in `after` the **complete new source**. To remove a diagram, give `before` alone. To add a diagram, give `after` and put the exact text it follows in `after_text`. Give the source only: leave out the fence markers and the word `mermaid`. The source in `before` must belong to exactly one Mermaid diagram of the file. Where two diagrams hold the same source, put the exact text immediately before the diagram in `lead`, or the exact text immediately after it in `trail`, up to 200 characters each. The new source must be valid Mermaid. A source that does not parse is refused with the parse error, and you correct the source and call again. A diagram that is not Mermaid can not be changed with this tool. Say why the changes are worth making, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. They say what matters, and where the written text holds more detail they end with an offer to read it. A rationale with no tag is refused. When a proposal of this file is already open, your diagram changes join it, and the author decides each one on its own. Your proposal is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's decisions, and whatever they said about them, in front of you. When your proposal is still open and the discussion asks for a different diagram, send `updates` in place of `diagrams`. Each update gives `action`: `change` with the `id` of a diagram change and its new fields, `add` with the fields of a new diagram change, or `withdraw` with the `id` of a diagram change you no longer offer. It reaches only the file you were asked about, and it changes nothing — the file is not rewritten unless the author accepts.";

const PATH_DESCRIPTION: &str = "The path of the file that holds the diagrams: the draft-relative path of the draft's prompt, or the project-relative path of the prompt or project document under discussion.";

const RATIONALE_DESCRIPTION: &str = "Why the changes are worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";

const DIAGRAMS_DESCRIPTION: &str = "The diagram changes, in the order they should be read. To change a diagram, give `before`, its current source copied character for character from the file, and `after`, its complete new source. To remove one, give `before` alone. To add one, give `after` and put the exact text it follows in `after_text`. Give the source only, with no fence markers. The new source must be valid Mermaid. Where two diagrams hold the same source, give `lead`, the exact text immediately before the diagram, or `trail`, the exact text immediately after it, up to 200 characters each. Leave it out when you pass `updates`.";

const UPDATES_DESCRIPTION: &str = "Changes to your proposal that is still open, in place of `diagrams`. Each update gives `action`. `change` gives the `id` of a diagram change and its new `before`, `after`, `after_text`, `lead`, and `trail`, with the rules of `diagrams`; the change keeps its id and the author decides it again. `add` gives the fields of a new diagram change and no `id`. `withdraw` gives the `id` of a diagram change you no longer offer. A change that is accepted or withdrawn can not be updated, and a text change can not be updated with this tool.";

const DIAGRAM_DESCRIPTION: &str = "One diagram change. A change gives `before` and `after`; an addition gives `after` and `after_text`; a removal gives `before` alone. Never send an empty change.";

const BEFORE_DESCRIPTION: &str = "The current source of the Mermaid diagram this change alters or removes, copied character for character from the file as you read it. Give the source only, with no fence markers. Leave it out for an addition.";

const AFTER_DESCRIPTION: &str = "The complete new source of the diagram, which must be valid Mermaid. Give the source only, with no fence markers. Leave it out for a removal.";

const AFTER_TEXT_DESCRIPTION: &str = "For an addition only: the exact existing text the new diagram goes after, copied character for character from the file. The new diagram goes after the line that holds this text.";

const LEAD_DESCRIPTION: &str = "Optional. The exact text immediately before the diagram (for an addition, immediately before `after_text`), up to 200 characters. Give it when two diagrams hold the same source.";

const TRAIL_DESCRIPTION: &str = "Optional. The exact text immediately after the diagram (for an addition, immediately after `after_text`), up to 200 characters. Give it when two diagrams hold the same source.";

const NOTE_DESCRIPTION: &str = "What this one diagram change is for, in a few words. Optional; the author reads the rationale for the proposal as a whole.";

/// CVL-FR-15: what a second proposal in one reply is told. The loop never
/// dispatches it, so this is the result the model reads in place of one.
pub const ONE_AT_A_TIME: &str = "Only one change can be proposed at a time, so this one was not recorded. Propose the diagram changes that matter most, in one call.";

/// PDM-FR-SDJP: `diagrams` and `updates` together.
pub const NOT_ONE_FORM: &str =
    "Pass `diagrams` for new diagram changes or `updates` for your open proposal, not both.";
/// PDM-FR-BWOH: `updates` where no proposal is open.
pub const NO_OPEN_PROPOSAL: &str =
    "No proposal is open on this file, so there is nothing to update. Send your changes as a new proposal in `diagrams`.";
/// PDM-FR-RNUS: no Mermaid diagram of the file holds the source.
pub const DIAGRAM_NOT_FOUND: &str =
    "The source you gave in `before` is not the source of a Mermaid diagram of the file. Copy it character for character from the file as you read it.";
/// PDM-FR-RNUS: more than one Mermaid diagram holds the source.
pub const DIAGRAM_NOT_UNIQUE: &str =
    "The source you gave in `before` belongs to more than one Mermaid diagram of the file. Add `lead` or `trail` with the exact text around the diagram so it names one diagram.";
/// PDM-FR-CEZB: a line of the new source would end the fence.
pub const FENCE_IN_SOURCE: &str =
    "The new source has a line that would end the code fence. Remove the line of backticks or tildes and give the source alone.";
/// PDM-FR-NQEK: an update of a text hunk.
pub const CHANGE_IS_TEXT: &str =
    "The change you named is a text change, so this tool can not update it. Update it with the tool for text changes.";

/// PDM-FR-PEWN: which of this tool's invalid-argument refusals fired, as a
/// stable code.
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    let message = match refusal {
        ToolRefusal::InvalidArguments(message) => message,
        ToolRefusal::InvalidDiagramAt { message, .. }
        | ToolRefusal::InvalidUpdateAt { message, .. } => message,
        ToolRefusal::NotValidMermaid { .. } => return Some("not_valid_mermaid"),
        _ => return None,
    };
    Some(match *message {
        PROPOSAL_NO_HUNKS => "no_changes",
        DIAGRAM_NOT_FOUND => "diagram_not_found",
        DIAGRAM_NOT_UNIQUE => "diagram_not_unique",
        FENCE_IN_SOURCE => "fence_in_source",
        CHANGE_IS_TEXT => "change_is_text_change",
        PROPOSAL_HUNKS_OVERLAP => "changes_overlap",
        NOT_ONE_FORM => "not_one_form",
        NO_OPEN_PROPOSAL => "no_open_proposal",
        UPDATE_NONE => "no_updates",
        UPDATE_MALFORMED => "update_malformed",
        UPDATE_NO_SUCH_CHANGE => "no_such_change",
        UPDATE_CHANGE_SETTLED => "change_already_settled",
        UPDATE_NAMED_TWICE => "change_named_twice",
        PROPOSAL_RATIONALE_BLANK | PROMPT_PROPOSAL_RATIONALE_BLANK => "rationale_blank",
        RATIONALE_NO_SPOKEN_PART => "no_spoken_part",
        PROPOSAL_NO_CHANGE => "no_change",
        _ => "other",
    })
}

/// The arguments, as the model composed them (TLC-FR-07: unknown fields are
/// ignored). There is deliberately no parameter naming a draft, an artifact, or
/// a conversation: the absence is the containment (PDM-FR-LJGN).
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeDiagramChangesArgs {
    #[serde(default, deserialize_with = "text_or_none")]
    pub path: String,
    #[serde(default, deserialize_with = "text_or_none")]
    pub rationale: String,
    #[serde(default, deserialize_with = "updates_or_none")]
    pub updates: Option<Vec<UpdateArg>>,
    #[serde(default, deserialize_with = "diagrams_or_none")]
    pub diagrams: Vec<DiagramArg>,
}

/// Text a model left out, an explicit `null` reading as blank (TLC-FR-07).
fn text_or_none<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A list of changes, an explicit `null` reading as none of them (TLC-FR-07).
fn diagrams_or_none<'de, D>(deserializer: D) -> Result<Vec<DiagramArg>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<DiagramArg>>::deserialize(deserializer)?.unwrap_or_default())
}

/// One diagram change as the model composed it. Text only (PDM-FR-DWAF).
#[derive(Debug, Clone, Deserialize)]
pub struct DiagramArg {
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
    #[serde(default)]
    pub note: Option<String>,
}

impl DiagramArg {
    /// PDM-FR-DWAF: the hunk the fields describe, the kind read from them.
    fn to_proposed(&self) -> ProposedHunk {
        ProposedHunk::diagram_fields(
            self.before.clone(),
            self.after.clone(),
            self.after_text.clone(),
            self.lead.clone(),
            self.trail.clone(),
            self.note.clone(),
        )
    }
}

/// TLC-FR-08: a JSON object with stable named fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposeDiagramChangesOutput {
    pub proposed: bool,
}

/// TLC-FR-06: the JSON Schema for [`ProposeDiagramChangesArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": PATH_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
            "updates": proposal_updates::schema(UPDATES_DESCRIPTION, true),
            "diagrams": {
                "type": "array",
                "description": DIAGRAMS_DESCRIPTION,
                "minItems": 1,
                "items": {
                    "type": "object",
                    "description": DIAGRAM_DESCRIPTION,
                    "properties": diagram_properties(),
                },
            },
        },
        // PDM-FR-SDJP: a call carries `diagrams` or `updates`, so neither is
        // required on its own.
        "required": ["path", "rationale"],
    })
}

/// The properties of one diagram change, which `diagrams` and the `change` and
/// `add` entries of `updates` share.
fn diagram_properties() -> serde_json::Value {
    serde_json::json!({
        "before": { "type": "string", "description": BEFORE_DESCRIPTION },
        "after": { "type": "string", "description": AFTER_DESCRIPTION },
        "after_text": { "type": "string", "description": AFTER_TEXT_DESCRIPTION },
        "lead": { "type": "string", "description": LEAD_DESCRIPTION },
        "trail": { "type": "string", "description": TRAIL_DESCRIPTION },
        "note": { "type": "string", "description": NOTE_DESCRIPTION },
    })
}

// ---------------------------------------------------------------------------
// The target (PDM-FR-LJGN, PDM-FR-TPYC)
// ---------------------------------------------------------------------------

/// What the turn's conversation is about, and so which module records.
enum Target<'a> {
    /// A draft: its prompt is the file.
    Draft { draft_id: &'a str, thread_id: &'a str },
    /// An artifact: a prompt or a project document.
    Artifact { thread_id: &'a str },
}

fn target_of(origin: &ConversationOrigin) -> Result<Target<'_>, ToolRefusal> {
    if let Some(draft_id) = origin.draft_id() {
        return Ok(Target::Draft { draft_id, thread_id: origin.discussion_id() });
    }
    if origin.is_artifact() {
        return Ok(Target::Artifact { thread_id: origin.discussion_id() });
    }
    Err(ToolRefusal::NotADocumentConversation)
}

/// The proposal tool of the other subject, as a refusal names it.
fn blank_rationale(target: &Target<'_>) -> ToolRefusal {
    ToolRefusal::InvalidArguments(match target {
        Target::Draft { .. } => PROPOSAL_RATIONALE_BLANK,
        Target::Artifact { .. } => PROMPT_PROPOSAL_RATIONALE_BLANK,
    })
}

fn path_missing(target: &Target<'_>) -> ToolRefusal {
    match target {
        Target::Draft { .. } => ToolRefusal::ProposalPathMissing,
        Target::Artifact { .. } => ToolRefusal::PromptProposalPathMissing,
    }
}

fn path_is_valid(target: &Target<'_>, path: &str) -> bool {
    match target {
        Target::Draft { .. } => crate::drafts::is_valid_draft_path(path),
        Target::Artifact { .. } => is_project_relative(path),
    }
}

// ---------------------------------------------------------------------------
// The refusals of the two modules, in this tool's terms
// ---------------------------------------------------------------------------

fn draft_record_refusal(refusal: draft_proposals::RecordRefusal) -> ToolRefusal {
    use draft_proposals::RecordRefusal as R;
    let place = |message, at, of| ToolRefusal::InvalidDiagramAt { message, at, of };
    match refusal {
        R::ProposalPending => ToolRefusal::ProposalPending,
        R::PathMissing => ToolRefusal::ProposalPathMissing,
        R::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        R::NoHunks => ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS),
        R::HunkAnchorLost { at, of } => place(DIAGRAM_NOT_FOUND, at, of),
        R::HunkAmbiguous { at, of } => place(DIAGRAM_NOT_UNIQUE, at, of),
        R::HunkOverlap { at, of } => place(PROPOSAL_HUNKS_OVERLAP, at, of),
        R::HunkContextTooLong { at, of, side } => place(context_too_long(side), at, of),
        R::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        R::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

fn prompt_record_refusal(refusal: prompt_proposals::RecordRefusal) -> ToolRefusal {
    use prompt_proposals::RecordRefusal as R;
    let place = |message, at, of| ToolRefusal::InvalidDiagramAt { message, at, of };
    match refusal {
        R::ProposalPending => ToolRefusal::PromptProposalPending,
        R::ArtifactNotFound => ToolRefusal::PromptProposalPathMissing,
        R::NotAPromptArtifact => ToolRefusal::NotAPromptArtifact,
        R::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        R::NoHunks => ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS),
        R::HunkAnchorLost { at, of } => place(DIAGRAM_NOT_FOUND, at, of),
        R::HunkAmbiguous { at, of } => place(DIAGRAM_NOT_UNIQUE, at, of),
        R::HunkOverlap { at, of } => place(PROPOSAL_HUNKS_OVERLAP, at, of),
        R::HunkContextTooLong { at, of, side } => place(context_too_long(side), at, of),
        R::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        R::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

/// PDM-FR-UITL: a join sends its hunks to the update path as `add` entries, and
/// a refusal of one of them is about a diagram change of `diagrams`. It names
/// that place, as a recording names it, and not the place of an update.
fn as_diagram_place(refusal: ToolRefusal) -> ToolRefusal {
    match refusal {
        ToolRefusal::InvalidUpdateAt { message, at, of } => {
            ToolRefusal::InvalidDiagramAt { message, at, of }
        }
        other => other,
    }
}

/// The refusal of one entry of `updates`, with its place (PDM-FR-BWOH).
fn entry_refusal(message: &'static str, at: usize, of: usize) -> ToolRefusal {
    ToolRefusal::InvalidUpdateAt { message, at, of }
}

fn draft_update_refusal(refusal: draft_proposals::UpdateRefusal) -> ToolRefusal {
    use draft_proposals::UpdateRefusal as R;
    match refusal {
        R::NoOpenProposal => ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL),
        R::NoUpdates => ToolRefusal::InvalidArguments(UPDATE_NONE),
        R::PathMissing => ToolRefusal::ProposalPathMissing,
        R::HunkNotFound { at, of } => entry_refusal(UPDATE_NO_SUCH_CHANGE, at, of),
        R::HunkSubjectMismatch { at, of } => entry_refusal(CHANGE_IS_TEXT, at, of),
        R::HunkNotUpdatable { at, of } => entry_refusal(UPDATE_CHANGE_SETTLED, at, of),
        R::HunkNamedTwice { at, of } => entry_refusal(UPDATE_NAMED_TWICE, at, of),
        R::NoChange { at, of } => entry_refusal(PROPOSAL_NO_CHANGE, at, of),
        R::HunkAnchorLost { at, of } => entry_refusal(DIAGRAM_NOT_FOUND, at, of),
        R::HunkAmbiguous { at, of } => entry_refusal(DIAGRAM_NOT_UNIQUE, at, of),
        R::HunkOverlap { at, of } => entry_refusal(PROPOSAL_HUNKS_OVERLAP, at, of),
        R::HunkContextTooLong { at, of, side } => entry_refusal(context_too_long(side), at, of),
        R::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        R::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

fn prompt_update_refusal(refusal: prompt_proposals::UpdateRefusal) -> ToolRefusal {
    use prompt_proposals::UpdateRefusal as R;
    match refusal {
        R::NoOpenProposal => ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL),
        R::NoUpdates => ToolRefusal::InvalidArguments(UPDATE_NONE),
        R::ArtifactNotFound => ToolRefusal::PromptProposalPathMissing,
        R::NotAPromptArtifact => ToolRefusal::NotAPromptArtifact,
        R::HunkNotFound { at, of } => entry_refusal(UPDATE_NO_SUCH_CHANGE, at, of),
        R::HunkSubjectMismatch { at, of } => entry_refusal(CHANGE_IS_TEXT, at, of),
        R::HunkNotUpdatable { at, of } => entry_refusal(UPDATE_CHANGE_SETTLED, at, of),
        R::HunkNamedTwice { at, of } => entry_refusal(UPDATE_NAMED_TWICE, at, of),
        R::NoChange { at, of } => entry_refusal(PROPOSAL_NO_CHANGE, at, of),
        R::HunkAnchorLost { at, of } => entry_refusal(DIAGRAM_NOT_FOUND, at, of),
        R::HunkAmbiguous { at, of } => entry_refusal(DIAGRAM_NOT_UNIQUE, at, of),
        R::HunkOverlap { at, of } => entry_refusal(PROPOSAL_HUNKS_OVERLAP, at, of),
        R::HunkContextTooLong { at, of, side } => entry_refusal(context_too_long(side), at, of),
        R::ThreadResolved => ToolRefusal::ProposalConversationResolved,
        R::NotRecorded => ToolRefusal::ProposalNotRecorded,
    }
}

// ---------------------------------------------------------------------------
// The check of the sources (PDM-FR-HOQI, PDM-FR-CEZB)
// ---------------------------------------------------------------------------

/// The refusal for a new source that does not pass, with its place.
fn source_refusal(fault: SourceFault, at: usize, of: usize, update: bool) -> ToolRefusal {
    match fault {
        SourceFault::NotMermaid(error) => ToolRefusal::NotValidMermaid { error, at, of, update },
        SourceFault::FenceInSource if update => entry_refusal(FENCE_IN_SOURCE, at, of),
        SourceFault::FenceInSource => {
            ToolRefusal::InvalidDiagramAt { message: FENCE_IN_SOURCE, at, of }
        }
    }
}

/// PDM-FR-DWAF: a change that carries neither source is a diagram that is not
/// found, whatever else it carries.
fn carries_a_source(before: &Option<String>, after: &Option<String>) -> bool {
    [before, after].iter().any(|s| s.as_deref().is_some_and(|s| !s.trim().is_empty()))
}

/// PDM-FR-HOQI: the check of every new source of a call with `diagrams`, in the
/// order the model sent them.
fn check_diagrams(diagrams: &[DiagramArg]) -> Result<(), ToolRefusal> {
    let of = diagrams.len();
    for (index, diagram) in diagrams.iter().enumerate() {
        let at = index + 1;
        if !carries_a_source(&diagram.before, &diagram.after) {
            return Err(ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at, of });
        }
        if let Some(after) = diagram.after.as_deref().filter(|s| !s.is_empty()) {
            check_source(after).map_err(|fault| source_refusal(fault, at, of, false))?;
        }
    }
    Ok(())
}

/// PDM-FR-HOQI: the same for the `change` and `add` entries of `updates`.
fn check_updates(updates: &[UpdateArg]) -> Result<(), ToolRefusal> {
    let of = updates.len();
    for (index, update) in updates.iter().enumerate() {
        let action = update.action.as_deref().map(str::trim).map(str::to_ascii_lowercase);
        if !matches!(action.as_deref(), Some("change" | "add")) {
            continue;
        }
        if let Some(after) = update.after.as_deref().filter(|s| !s.is_empty()) {
            check_source(after).map_err(|fault| source_refusal(fault, index + 1, of, true))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// PDM-FR-ZXQE: `propose_diagram_changes` as a `rig` portable tool.
///
/// `proposed` is how the turn's loop learns that a proposal went out
/// (CVL-FR-15). It is set once, after the record and its comment have landed.
pub struct ProposeDiagramChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposeDiagramChangesTool<R> {
    /// TLC-FR-19: this tool's own constructor, naming the conversation it posts
    /// into and the participant it proposes as.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposeDiagramChangesTool { app, roots, origin, author, proposed, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// The whole call, with its logging (TLC-FR-14, PDM-FR-PEWN).
    fn run(
        &self,
        args: ProposeDiagramChangesArgs,
    ) -> Result<ProposeDiagramChangesOutput, ToolRefusal> {
        // PDM-FR-PEWN: the origin and `path`, which is this tool's one loggable
        // argument. The counts are a shape and carry no source, no parse error,
        // no rationale, and no part of the file.
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "path" => super::bounded_argument(&args.path, PATH_LOG_CHARS),
            "diagrams" => args.diagrams.len(),
            "updates" => args.updates.as_ref().map_or(0, Vec::len),
        };
        if let Some(updates) = &args.updates {
            let (changes, adds, withdraws) = proposal_updates::action_counts(updates);
            fields.insert("updateChanges".to_string(), serde_json::json!(changes));
            fields.insert("updateAdds".to_string(), serde_json::json!(adds));
            fields.insert("updateWithdraws".to_string(), serde_json::json!(withdraws));
        }
        let outcome = self.propose(args);
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, fields),
            Err(refusal) => {
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".to_string(), serde_json::json!(code));
                }
                match refusal {
                    ToolRefusal::InvalidDiagramAt { at, .. }
                    | ToolRefusal::InvalidUpdateAt { at, .. }
                    | ToolRefusal::NotValidMermaid { at, .. } => {
                        fields.insert("refusalAt".to_string(), serde_json::json!(at));
                    }
                    _ => {}
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn propose(
        &self,
        args: ProposeDiagramChangesArgs,
    ) -> Result<ProposeDiagramChangesOutput, ToolRefusal> {
        // PDM-FR-OHSB: no project, no file to propose to (TLC-FR-13).
        require_open_project(&self.app)?;
        let target = target_of(&self.origin)?;

        // PDM-FR-SDJP: one form per call.
        if let Some(updates) = &args.updates {
            if !args.diagrams.is_empty() {
                return Err(ToolRefusal::InvalidArguments(NOT_ONE_FORM));
            }
            if updates.is_empty() {
                return Err(ToolRefusal::InvalidArguments(UPDATE_NONE));
            }
        } else if args.diagrams.is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS));
        }
        // PDM-FR-XRVZ: the rationale is the comment's body.
        if args.rationale.trim().is_empty() {
            return Err(blank_rationale(&target));
        }
        if !path_is_valid(&target, &args.path) {
            return Err(path_missing(&target));
        }
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }

        match &args.updates {
            Some(updates) => {
                check_updates(updates)?;
                self.update(&target, &args, updates)?;
            }
            None => {
                // PDM-FR-HOQI: no proposal is recorded for a source that does
                // not parse.
                check_diagrams(&args.diagrams)?;
                let hunks: Vec<ProposedHunk> =
                    args.diagrams.iter().map(DiagramArg::to_proposed).collect();
                self.record_or_join(&target, &args, &hunks)?;
            }
        }
        // CVL-FR-15: set only after the record and its comment have landed, so
        // a turn is never ended awaiting a decision on nothing.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeDiagramChangesOutput { proposed: true })
    }

    /// PDM-FR-WLBN: record a proposal where the file holds none, and add the
    /// hunks to the open one otherwise.
    fn record_or_join(
        &self,
        target: &Target<'_>,
        args: &ProposeDiagramChangesArgs,
        hunks: &[ProposedHunk],
    ) -> Result<(), ToolRefusal> {
        let path = args.path.trim();
        match target {
            Target::Draft { draft_id, thread_id } => {
                if let Some(open) = draft_proposals::pending_for(&self.roots.worktree, draft_id) {
                    if open.path != path {
                        return Err(ToolRefusal::ProposalPathMissing);
                    }
                    let entries: Vec<UpdateEntry> =
                        hunks.iter().cloned().map(UpdateEntry::Add).collect();
                    return self
                        .update_draft(draft_id, thread_id, args, &entries)
                        .map_err(as_diagram_place);
                }
                let mut file_rel = String::new();
                let thread = target_for(
                    &self.roots.store,
                    &self.origin,
                    draft_id,
                    thread_id,
                    &mut file_rel,
                )?;
                draft_proposals::record_proposal(
                    &self.app,
                    &self.roots.worktree,
                    &self.roots.store,
                    NewProposal {
                        draft_id,
                        path,
                        hunks,
                        rationale: &args.rationale,
                        agent: &self.author,
                        target: thread,
                        thread_id,
                    },
                )
                .map(|_| ())
                .map_err(draft_record_refusal)
            }
            Target::Artifact { thread_id } => {
                if prompt_proposals::pending_for(&self.roots.worktree, path).is_some() {
                    let entries: Vec<UpdateEntry> =
                        hunks.iter().cloned().map(UpdateEntry::Add).collect();
                    return self
                        .update_artifact(path, thread_id, args, &entries)
                        .map_err(as_diagram_place);
                }
                prompt_proposals::record_prompt_proposal(
                    &self.app,
                    &self.roots.worktree,
                    &self.roots.store,
                    NewPromptProposal {
                        artifact_id: path,
                        change: ProposedChange::Hunks(hunks),
                        rationale: &args.rationale,
                        agent: &self.author,
                        thread_id,
                    },
                )
                .map(|_| ())
                .map_err(prompt_record_refusal)
            }
        }
    }

    /// PDM-FR-BWOH: update the open proposal in one call.
    fn update(
        &self,
        target: &Target<'_>,
        args: &ProposeDiagramChangesArgs,
        updates: &[UpdateArg],
    ) -> Result<(), ToolRefusal> {
        let path = args.path.trim();
        // PDM-FR-UOBH rules of the text tools: a malformed entry is refused
        // with its place. Every entry names a diagram hunk (PDM-FR-NQEK).
        let entries = proposal_updates::to_entries_for(updates, true)?;
        match target {
            Target::Draft { draft_id, thread_id } => {
                let Some(open) = draft_proposals::pending_for(&self.roots.worktree, draft_id)
                else {
                    return Err(ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL));
                };
                if open.path != path {
                    return Err(ToolRefusal::ProposalPathMissing);
                }
                self.update_draft(draft_id, thread_id, args, &entries)
            }
            Target::Artifact { thread_id } => {
                if prompt_proposals::pending_for(&self.roots.worktree, path).is_none() {
                    return Err(ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL));
                }
                self.update_artifact(path, thread_id, args, &entries)
            }
        }
    }

    fn update_draft(
        &self,
        draft_id: &str,
        thread_id: &str,
        args: &ProposeDiagramChangesArgs,
        entries: &[UpdateEntry],
    ) -> Result<(), ToolRefusal> {
        let mut file_rel = String::new();
        let thread =
            target_for(&self.roots.store, &self.origin, draft_id, thread_id, &mut file_rel)?;
        draft_proposals::update_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            ProposalUpdate {
                draft_id,
                updates: entries,
                rationale: &args.rationale,
                agent: &self.author,
                target: thread,
                thread_id,
            },
        )
        .map(|_| ())
        .map_err(draft_update_refusal)
    }

    fn update_artifact(
        &self,
        artifact_id: &str,
        thread_id: &str,
        args: &ProposeDiagramChangesArgs,
        entries: &[UpdateEntry],
    ) -> Result<(), ToolRefusal> {
        prompt_proposals::update_prompt_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            PromptProposalUpdate {
                artifact_id,
                updates: entries,
                rationale: &args.rationale,
                agent: &self.author,
                thread_id,
            },
        )
        .map(|_| ())
        .map_err(prompt_update_refusal)
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposeDiagramChangesTool<R> {
    const NAME: &'static str = NAME;

    type Args = ProposeDiagramChangesArgs;
    type Output = ProposeDiagramChangesOutput;
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
        // PDM-FR-JLYU: records and returns. It waits for no decision, holds no
        // timer, and opens no watcher.
        std::future::ready(self.run(arguments))
    }
}
