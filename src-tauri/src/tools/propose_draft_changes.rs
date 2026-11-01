//! Propose draft changes tool (`PDC-propose-draft-changes-tool.md`).
//!
//! The tool an agent reaches for when it can see how a file of the draft under
//! discussion would be better and wants the author to decide. It **proposes** a
//! new version of one draft file — the file's complete text as it should stand
//! once the change lands — and the turn ends there (CVL-FR-15). The author
//! accepts it or declines it, with a word of feedback if they have one, and that
//! decision brings the agent back with the whole conversation in front of it.
//!
//! ## Why nothing it does changes a file
//!
//! This is the point of the tool rather than a limitation of it. A proposal is
//! material sitting beside the draft until the author accepts it, and the write
//! that lands it is one *they* perform from the surface that showed it to them
//! (`DCR-draft-change-review.md`). An agent whose proposal is declined has cost
//! the draft nothing but a comment — which is the difference between a
//! collaborator offering a rewrite and one that edits your work and tells you
//! afterwards.
//!
//! ## Why it is constructed per turn
//!
//! Like `ask_user_comment`'s and unlike the read-only tools of this group. This
//! one proposes *somewhere*, and where is not a decision a model may make: the
//! draft, the conversation, and the agent participant are bound by the
//! constructor (PDC-FR-03, TLC-FR-15), so no argument can reach a draft the agent
//! was not asked about. There is deliberately no `draft_id` parameter — its
//! absence is the containment.
//!
//! ## Why a proposal is a list of changes
//!
//! A proposal is an ordered list of changes, each naming the exact prompt text
//! it alters (PDC-FR-04). Two things follow, and both are the point.
//!
//! The author reads each change in the passage it changes, and decides it on its
//! own: one objection holds up one change rather than every change offered with
//! it. And because a change names its text rather than a position, the changes
//! stay decidable in any order — accepting one moves the others in the prompt
//! without invalidating what they name.
//!
//! It also makes the tool able to refuse what a whole document could not. Every
//! change is placed against the prompt the tool has just read, **before anything
//! is recorded**: text the prompt does not hold, text it holds twice, and two
//! changes that overlap are each a refusal the model reads while it can still
//! fix it. That is what catches a model inventing the text it claims to be
//! changing, which a whole-document proposal could only ever show to the author
//! as a rewrite that quietly dropped their words.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::proposal_updates::{
    self, updates_or_none, UpdateArg, UPDATE_CHANGE_SETTLED, UPDATE_MALFORMED, UPDATE_NAMED_TWICE,
    UPDATE_NONE, UPDATE_NO_SUCH_CHANGE,
};
use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal, PROPOSAL_NO_CHANGE,
    PROPOSAL_RATIONALE_BLANK, RATIONALE_NO_SPOKEN_PART,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant, ThreadRef};
use crate::draft_proposals::{self, NewProposal, ProposalUpdate, RecordRefusal, UpdateRefusal};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::text_hunks::hunks::ContextSide;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (PDC contract surface)
// ---------------------------------------------------------------------------

/// PDC-FR-01.
pub const NAME: &str = "propose_draft_changes";

/// PDC-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Propose changes to the prompt of the draft under discussion, when you can see how its text would be better and want the author to decide. A draft is one prompt, so this is the one file there is to propose changes to. Reach for it instead of describing the improvement in prose, and instead of asking permission to make one. Pass a list of changes. Each one names, in `before`, the exact text it changes, copied character for character from the prompt as you read it, and gives in `after` the text that should stand in its place. To insert new text, give `after` alone and put the exact text it follows in `after_text`. To delete, give `before` alone. You do not need the whole prompt in front of you: read only the parts you change, with `offset` and `limit` on `read_draft`. The text you name must appear once in the prompt. Where it can appear more than once, put the exact text immediately before it in `lead`, or the exact text immediately after it in `trail`, up to 200 characters each; only `before` changes. A change whose text is not found, or not found once, is refused and you are told which change to correct. Say why the changes are worth making, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. They say what matters, and where the written text holds more detail they end with an offer to read it. A rationale with no tag is refused. Your proposal is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's decisions, and whatever they said about them, in front of you. The author decides each change separately, so send the changes you believe in together rather than holding some back. Offer the changes for the whole prompt one time, rather than a small improvement each time you read it. When your proposal is still open and the discussion asks for a different change, send `updates` in place of `hunks`. Each update gives `action`: `change` with the `id` of a change and its new fields, `add` with the fields of a new change, or `withdraw` with the `id` of a change you no longer offer. One call can hold several updates, and it also takes a `rationale`. It reaches only the draft you were asked about, and it changes nothing — the prompt is not rewritten unless the author accepts.";

const PATH_DESCRIPTION: &str = "The draft-relative path of the draft's prompt, which is the one file a draft holds and the only thing you can propose changes to.";

const HUNKS_DESCRIPTION: &str = "The changes, in the order they should be read. In each, `before` is the exact existing text you are changing, copied character for character from the prompt, and `after` is what should stand in its place. For an insertion give `after` and put the exact text it follows in `after_text`; for a deletion give `before` alone. The text you name must appear only once in the prompt. When it does not, give `lead`, the exact text immediately before it, or `trail`, the exact text immediately after it, or both, up to 200 characters each: the text `lead` + `before` + `trail` must then appear once, and only `before` changes. For an insertion, `lead` and `trail` stand around `after_text`. Leave it out when you pass `updates`.";

/// PDC-FR-GMWR: the parameter text of `updates`.
const UPDATES_DESCRIPTION: &str = "Changes to your proposal that is still open, in place of `hunks`. Each update gives `action`. `change` gives the `id` of a change and its new `before`, `after`, `after_text`, `lead`, and `trail`, with the rules of `hunks`; the change keeps its id and the author decides it again. `add` gives the fields of a new change and no `id`. `withdraw` gives the `id` of a change you no longer offer. A change that is accepted or withdrawn can not be updated. Updates must not cover the same text as each other or as another change still standing.";

/// PDC-FR-GPLQ: `hunks` and `updates` together.
pub const PROPOSAL_NOT_ONE_FORM: &str =
    "Pass `hunks` for a new proposal or `updates` for your open proposal, not both.";
/// PDC-FR-LGWJ: `updates` where no proposal is open.
pub const PROPOSAL_NO_OPEN: &str =
    "No proposal is open on this draft, so there is nothing to update. Send your changes as a new proposal in `hunks`.";

/// PDC-FR-TZKQ: the refusals record-time placement produces, each naming the
/// correction the model has to make.
pub const PROPOSAL_NO_HUNKS: &str =
    "Pass at least one change. An empty proposal has nothing for the author to decide.";
pub const PROPOSAL_TEXT_NOT_FOUND: &str =
    "The text you gave in `before` is not in the prompt. Copy it character for character from the prompt as you read it.";
/// PDC-FR-YSKV: the same refusal with and without `lead` and `trail`, and it
/// names them as the correction.
pub const PROPOSAL_TEXT_NOT_UNIQUE: &str =
    "The text you gave in `before` appears more than once in the prompt. Add `lead` or `trail` with the exact text around it so it names one place.";
/// PDC-FR-DMTH.
pub const PROPOSAL_LEAD_TOO_LONG: &str =
    "The `lead` you gave is longer than 200 characters. Shorten `lead` to the few words just before the text you name.";
/// PDC-FR-DMTH.
pub const PROPOSAL_TRAIL_TOO_LONG: &str =
    "The `trail` you gave is longer than 200 characters. Shorten `trail` to the few words just after the text you name.";
/// PDC-FR-DMTH.
pub const PROPOSAL_LEAD_AND_TRAIL_TOO_LONG: &str =
    "The `lead` and the `trail` you gave are each longer than 200 characters. Shorten both to the few words next to the text you name.";

/// The refusal text for the context that is too long (PDC-FR-DMTH, PPC-FR-KWQB).
fn context_too_long(side: ContextSide) -> &'static str {
    match side {
        ContextSide::Lead => PROPOSAL_LEAD_TOO_LONG,
        ContextSide::Trail => PROPOSAL_TRAIL_TOO_LONG,
        ContextSide::Both => PROPOSAL_LEAD_AND_TRAIL_TOO_LONG,
    }
}

pub const PROPOSAL_HUNKS_OVERLAP: &str =
    "Two of your changes cover the same text. Combine them into one change.";

const HUNK_DESCRIPTION: &str = "One change to the prompt. A replacement gives `before` and `after`; an insertion gives `after` and `after_text`; a deletion gives `before` alone. Never send an empty change.";

const KIND_DESCRIPTION: &str = "What this change does: `replace` puts `after` in place of `before`, `del` removes `before`, `add` inserts `after` immediately following `after_text`. The fields you send are what decide, so give the fields that match what you mean: a rewrite gives `before` and `after` together, and calling it `add` does not make it one.";

const BEFORE_DESCRIPTION: &str = "The exact existing text this change replaces or deletes, copied character for character from the prompt as you read it — including its punctuation and its capitals. It must appear only once in the prompt; where it does not, add `lead` or `trail`. Leave it out for an insertion.";

const AFTER_DESCRIPTION: &str = "The exact text that should stand in place of `before`, or the text to insert. Leave it out for a deletion.";

const AFTER_TEXT_DESCRIPTION: &str = "For an insertion only: the exact existing text the new text goes immediately after, copied character for character from the prompt. This is what says where the insertion goes, so it is required for an insertion into a prompt that has any text in it.";

const LEAD_DESCRIPTION: &str = "Optional. The exact text immediately before `before` (for an insertion, immediately before `after_text`), copied character for character, up to 200 characters. Give it when the text you change appears more than once, so that `lead` + `before` + `trail` appears once. It is not changed.";

const TRAIL_DESCRIPTION: &str = "Optional. The exact text immediately after `before` (for an insertion, immediately after `after_text`), copied character for character, up to 200 characters. Give it when the text you change appears more than once, so that `lead` + `before` + `trail` appears once. It is not changed.";

const NOTE_DESCRIPTION: &str = "What this one change is for, in a few words. Optional; the author reads the rationale for the proposal as a whole.";

/// PDC-FR-18 / TLC-FR-14: which of this tool's invalid-argument refusals fired,
/// as a stable code.
///
/// `reason()` is one term for the whole family, deliberately, so a reader can
/// filter a column on it. This tool has many refusals in that family, and a
/// record saying only `invalid_arguments` cannot say which correction the model
/// was asked to make — which is exactly what a reader looking at a call that
/// keeps failing needs. A **code** rather than the sentence, for the reason
/// `reason()` is one: a record stays constant down a column and searchable on a
/// term this file chose, and the wording the model reads is free to change
/// without a saved filter going quiet.
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    let message = match refusal {
        ToolRefusal::InvalidArguments(message) => message,
        // A refusal about one change of several carries the same code: which
        // mistake it was does not change because the call held more than one.
        ToolRefusal::InvalidArgumentsAt { message, .. }
        | ToolRefusal::InvalidUpdateAt { message, .. } => message,
        _ => return None,
    };
    Some(match *message {
        PROPOSAL_NO_HUNKS => "no_changes",
        PROPOSAL_TEXT_NOT_FOUND => "text_not_found",
        PROPOSAL_TEXT_NOT_UNIQUE => "text_not_unique",
        PROPOSAL_LEAD_TOO_LONG => "lead_too_long",
        PROPOSAL_TRAIL_TOO_LONG => "trail_too_long",
        PROPOSAL_LEAD_AND_TRAIL_TOO_LONG => "lead_and_trail_too_long",
        PROPOSAL_HUNKS_OVERLAP => "changes_overlap",
        PROPOSAL_NOT_ONE_FORM => "not_one_form",
        PROPOSAL_NO_OPEN => "no_open_proposal",
        UPDATE_NONE => "no_updates",
        UPDATE_MALFORMED => "update_malformed",
        UPDATE_NO_SUCH_CHANGE => "no_such_change",
        UPDATE_CHANGE_SETTLED => "change_already_settled",
        UPDATE_NAMED_TWICE => "change_named_twice",
        PROPOSAL_RATIONALE_BLANK => "rationale_blank",
        RATIONALE_NO_SPOKEN_PART => "no_spoken_part",
        PROPOSAL_NO_CHANGE => "no_change",
        // A refusal added without a code of its own is still followable as
        // itself rather than silently reading as one of the eight above.
        _ => "other",
    })
}

const RATIONALE_DESCRIPTION: &str = "Why the changes are worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud.";

/// CVL-FR-15: what a second proposal in one reply is told.
///
/// The loop never dispatches it, so this is the result the model reads in place
/// of one. It exists for the reason `ask_user_comment`'s twin does: a reply *can*
/// propose twice, and were the second carried out after the first refused, one
/// turn would make two contributions (AGC-FR-01).
pub const ONE_AT_A_TIME: &str = "Only one change can be proposed at a time, so this one was not recorded. Propose the single change that matters most, in one call.";

/// PDC-FR-18: how much of `path` reaches a log record.
///
/// A draft-relative path is short by construction, and the bound is what keeps a
/// model-composed value from growing a record past `LGC-logging.md`'s per-record
/// ceiling (LGC-FR-08) and costing it the tool name and the reason.
const PATH_LOG_CHARS: usize = 200;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). There is
/// deliberately no parameter naming a draft or a conversation: see the module
/// docs.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposeDraftChangesArgs {
    /// PDC-FR-05: required, and **absent or null decodes as blank** rather
    /// than refusing the whole argument set.
    ///
    /// TLC-FR-07: the parameter stays required — a blank path names no file the
    /// draft holds, so the call is refused either way. What changes is which
    /// refusal the model reads. A missing field refused by the decoder costs it
    /// the sentence naming the one file a draft holds and gives it "the
    /// arguments were not in the shape it expects", which is true of every
    /// mistake there is; refused here, it reads the refusal this tool wrote for
    /// exactly this mistake, and the call is logged with the shape it had.
    #[serde(default, deserialize_with = "text_or_none")]
    pub path: String,
    /// PDC-FR-07: required, and absent or null decodes as blank on the terms
    /// `path` does — the blank-rationale refusal says what to write, where the
    /// decoder's could only say the call was malformed.
    #[serde(default, deserialize_with = "text_or_none")]
    pub rationale: String,
    /// PDC-FR-GMWR: the updates of the open proposal, in place of `hunks`.
    #[serde(default, deserialize_with = "updates_or_none")]
    pub updates: Option<Vec<UpdateArg>>,
    /// PDC-FR-04: the changes. A model that sent `null` for the list sent no
    /// changes, which is the **no changes** refusal and not a malformed call —
    /// `#[serde(default)]` covers a field that is absent and not one that is
    /// explicitly empty.
    #[serde(default, deserialize_with = "hunks_or_none")]
    pub hunks: Vec<HunkArg>,
}

/// Text a model left out, an explicit `null` reading as blank (TLC-FR-07).
///
/// `#[serde(default)]` covers a field that is **absent** and not one that is
/// explicitly empty, and a model that sends `null` for a field it has nothing to
/// put in made the same mistake as one that left it out. Both reach the refusal
/// this tool wrote for a blank value.
fn text_or_none<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A list of changes, an explicit `null` reading as none of them (TLC-FR-07).
fn hunks_or_none<'de, D>(deserializer: D) -> Result<Vec<HunkArg>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<HunkArg>>::deserialize(deserializer)?.unwrap_or_default())
}

/// One change as the model composed it. Text only — a model cannot count
/// characters, so the geometry is derived from the prompt the tool just read
/// (PDC-FR-04).
#[derive(Debug, Clone, Deserialize)]
pub struct HunkArg {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub after_text: Option<String>,
    /// PDC-FR-WCXD: the exact text just before the change, which names its place.
    #[serde(default)]
    pub lead: Option<String>,
    /// PDC-FR-WCXD: the exact text just after the change, which names its place.
    #[serde(default)]
    pub trail: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl HunkArg {
    /// Whether this change names any text at all.
    ///
    /// A change with neither `before` nor `after` is an empty object, which is
    /// what a model sends when it cannot tell what to put in one. Counted into
    /// the log so that mistake is legible as itself rather than as a bare
    /// invalid-argument refusal.
    fn names_text(&self) -> bool {
        self.before.as_deref().is_some_and(|s| !s.trim().is_empty())
            || self.after.as_deref().is_some_and(|s| !s.trim().is_empty())
    }

    /// PDC-FR-VKMR: the kind the fields carry.
    ///
    /// The `kind` a model names is a label on the same change, so where the two
    /// disagree the fields decide. Acceptance applies `before` and `after`, and
    /// a record that kept the named kind would show the author a change the
    /// draft never takes: a rewrite sent as `add` would draw as an insertion,
    /// leave the old text in the prompt, and never mark it as removed.
    ///
    /// A change carrying neither text names nothing to do, which no kind can
    /// make sense of. It is read as a replacement so that placing it refuses on
    /// the `before` it does not hold (per `../text_hunks/hunks.rs`), rather
    /// than as an insertion, which is placed on its `after_text` alone and would
    /// record a change with no text in it at all.
    fn to_proposed(&self) -> crate::draft_proposals::hunks::ProposedHunk {
        crate::draft_proposals::hunks::ProposedHunk::from_fields(
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
///
/// PDC-FR-14: the fact of the proposal is the whole of the result. It carries
/// neither the proposal's identity nor any part of the candidate — the model has
/// nothing further to do with either, its turn having ended (CVL-FR-15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposeDraftChangesOutput {
    pub proposed: bool,
}

/// TLC-FR-06: the JSON Schema for [`ProposeDraftChangesArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": PATH_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION },
            "updates": proposal_updates::schema(UPDATES_DESCRIPTION, true),
            "hunks": {
                "type": "array",
                "description": HUNKS_DESCRIPTION,
                // The tool refuses an empty list, so the schema says so: a model
                // told the field is required and given no floor can satisfy it
                // with `[]` and be refused for it.
                "minItems": 1,
                "items": {
                    "type": "object",
                    // Each property carries its own description. A model filling
                    // an object inside an array reads these rather than the
                    // array's — without them it is choosing field names from
                    // their spelling alone, and an object it cannot fill it
                    // leaves empty.
                    "description": HUNK_DESCRIPTION,
                    "properties": hunk_properties(),
                },
            },
        },
        // PDC-FR-GPLQ: a call carries `hunks` or `updates`, so neither is
        // required on its own.
        "required": ["path", "rationale"],
    })
}

/// The properties of one hunk, which `hunks` and the `change` and `add`
/// entries of `updates` share (PDC-FR-GMWR).
fn hunk_properties() -> serde_json::Value {
    serde_json::json!({
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
        "note": { "type": "string", "description": NOTE_DESCRIPTION },
    })
}

// ---------------------------------------------------------------------------
// The conversation (PDC-FR-03, PDC-FR-06)
// ---------------------------------------------------------------------------

/// The draft a turn is about, and the conversation within it, or the refusal for
/// a turn that is about neither.
///
/// PDC-FR-06: an artifact origin is the not-retryable refusal. It is unreachable
/// while CVL-FR-08 attaches this tool to draft origins alone, and checked anyway
/// — a tool's refusals are its own contract, and a set selected elsewhere is not
/// a guarantee this tool may rest on.
fn draft_of(origin: &ConversationOrigin) -> Result<(&str, &str), ToolRefusal> {
    // A note is not a draft either: there is no draft prompt to name and
    // nothing about a note to rewrite (CVL-FR-08).
    match origin.draft_id() {
        Some(draft_id) => Ok((draft_id, origin.discussion_id())),
        None => Err(ToolRefusal::NotADraftConversation),
    }
}

/// The conversation the proposal's comment is appended into.
///
/// Resolved at call time rather than at construction because a `ThreadRef`
/// borrows, and because a fragment discussion's own file has to be located. A
/// whole-draft discussion belongs to no file of the draft; a fragment discussion
/// belongs to one.
fn target_for<'a>(
    store: &crate::fs::RootFs,
    origin: &'a ConversationOrigin,
    draft_id: &'a str,
    thread_id: &str,
    file_rel: &'a mut String,
) -> Result<ThreadRef<'a>, ToolRefusal> {
    if origin.fragment_target.is_none() {
        return Ok(ThreadRef::discussion(draft_id));
    }
    let found = comments::locate_discussion_of(store, &origin.target, thread_id)
        .and_then(|d| d.fragment_target.map(|f| f.path))
        .ok_or(ToolRefusal::ProposalNotRecorded)?;
    *file_rel = found;
    Ok(ThreadRef::draft_file(draft_id, file_rel))
}

/// `crate::draft_proposals`' refusals in this tool's terms (PDC-FR-05,
/// PDC-FR-07, PDC-FR-08, PDC-FR-09, PDC-FR-11).
fn refusal_for(refusal: RecordRefusal) -> ToolRefusal {
    match refusal {
        RecordRefusal::ProposalPending => ToolRefusal::ProposalPending,
        RecordRefusal::PathMissing => ToolRefusal::ProposalPathMissing,
        RecordRefusal::NoChange => ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE),
        RecordRefusal::NoHunks => ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS),
        // PDC-FR-TZKQ: which of the changes, as the model counted them. A call
        // carrying eight changes and refused for one of them is a call the
        // model cannot correct without being told which — it would send the
        // same eight again, and does.
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

/// PDC-FR-01: `propose_draft_changes` as a `rig` portable tool.
///
/// `proposed` is how the turn's loop learns that a proposal actually went out.
/// CVL-FR-15 turns on the difference between a call that recorded one — which
/// ends the turn — and one that refused, which ends nothing; the flag is that
/// distinction carried back to the loop, which sees only the tool's rendered
/// result otherwise. It is set exactly once, on the record succeeding.
pub struct ProposeDraftChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    proposed: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ProposeDraftChangesTool<R> {
    /// TLC-FR-19: this tool's own constructor, naming the draft it may propose
    /// to, the conversation it posts into, and the participant it proposes as.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        proposed: Arc<AtomicBool>,
    ) -> Self {
        ProposeDraftChangesTool { app, roots, origin, author, proposed, buffer: &BUFFER }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// The whole call, with its logging (TLC-FR-14, PDC-FR-18).
    fn run(&self, args: ProposeDraftChangesArgs) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
        // PDC-FR-18: the draft and the conversation, bound at construction rather
        // than composed by the model, plus `path` — this tool's one loggable
        // argument, because a record about a proposed rewrite is unfollowable
        // without saying which file was rewritten. Never the `content`, the
        // `rationale`, or any part of the file being replaced.
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "path" => super::bounded_argument(&args.path, PATH_LOG_CHARS),
            // PDC-FR-18: the **shape** of what the model composed, never its
            // text. A call that was refused is unfollowable without it: "no
            // changes" and "three changes, none of which names any text" are
            // different mistakes with different corrections, and both arrive
            // here as one invalid-argument refusal. Counts and flags only —
            // the prompt's text, the proposed text and the rationale are the
            // author's and the model's words and reach no record.
            "hunks" => args.hunks.len(),
            "hunksNamingText" => args.hunks.iter().filter(|h| h.names_text()).count(),
            "updates" => args.updates.as_ref().map_or(0, Vec::len),
            "rationaleChars" => args.rationale.trim().chars().count(),
        };
        if let Ok((draft_id, _)) = draft_of(&self.origin) {
            fields.insert("draftId".to_string(), serde_json::json!(draft_id));
        }
        // PDC-FR-18: for an update, the number of updates of each action.
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
                // PDC-FR-18: which of this tool's eight invalid-argument
                // refusals it was. Without it every one of them arrives under
                // `invalid_arguments` and a call that keeps failing cannot be
                // told apart from any other.
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".to_string(), serde_json::json!(code));
                }
                // PDC-FR-18: **which** of the changes, as the model counted
                // them. A record saying eight changes were sent and one of them
                // named text the prompt does not hold leaves a reader unable to
                // tell a model that is one change out from one that invented
                // the whole set. A count of what the model sent, and no part of
                // what it wrote.
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
        args: ProposeDraftChangesArgs,
    ) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
        // PDC-FR-16: no project, no draft to propose to, so the shared refusal
        // (TLC-FR-13). Checked per call rather than at construction for the
        // reason `ask_user_comment` checks it there: the root this tool holds was
        // bound when the turn began, and a turn can outlive the project being
        // open.
        require_open_project(&self.app)?;
        let (draft_id, thread_id) = draft_of(&self.origin)?;

        // PDC-FR-GPLQ: one form per call.
        if let Some(updates) = &args.updates {
            if !args.hunks.is_empty() {
                return Err(ToolRefusal::InvalidArguments(PROPOSAL_NOT_ONE_FORM));
            }
            return self.update(draft_id, thread_id, &args, updates);
        }

        // PDC-FR-07, before anything is resolved on disk. Three distinct
        // refusals rather than one, because each calls for a different
        // correction: pass the text, say why, and propose something that differs.
        if args.hunks.is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS));
        }
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_RATIONALE_BLANK));
        }
        // PDC-FR-05: a path that is not a draft-relative path names no file this
        // tool will act on, which is the same answer as a path naming nothing.
        if !crate::drafts::is_valid_draft_path(&args.path) {
            return Err(ToolRefusal::ProposalPathMissing);
        }

        // PDC-FR-04: the model composed text, and this is where it becomes a set
        // of changes this module can place.
        let proposed: Vec<crate::draft_proposals::hunks::ProposedHunk> =
            args.hunks.iter().map(HunkArg::to_proposed).collect();

        // PDC-FR-ZQOH: the rationale is the comment's body, so it carries a
        // spoken part.
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }

        let mut file_rel = String::new();
        let target =
            target_for(&self.roots.store, &self.origin, draft_id, thread_id, &mut file_rel)?;

        // PDC-FR-11: the candidate and the comment are one act, and neither this
        // tool nor the model performs either write — `crate::draft_proposals`
        // does, so a failure of one leaves neither behind (DCP-FR-06).
        draft_proposals::record_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            NewProposal {
                draft_id,
                path: &args.path,
                hunks: &proposed,
                rationale: &args.rationale,
                agent: &self.author,
                target,
                thread_id,
            },
        )
        .map_err(refusal_for)?;

        // CVL-FR-15: the loop reads this to tell a recorded proposal from a
        // refused one. Set only after the record and its comment have both
        // landed, so a turn is never ended awaiting a decision on nothing.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeDraftChangesOutput { proposed: true })
    }

    /// PDC-FR-GMWR: update the open proposal, in one call that appends one
    /// comment and ends the turn (PDC-FR-VUTJ).
    fn update(
        &self,
        draft_id: &str,
        thread_id: &str,
        args: &ProposeDraftChangesArgs,
        updates: &[UpdateArg],
    ) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
        // PDC-FR-GPLQ: an empty list is refused before anything else.
        if updates.is_empty() {
            return Err(ToolRefusal::InvalidArguments(UPDATE_NONE));
        }
        if args.rationale.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_RATIONALE_BLANK));
        }
        if !crate::drafts::is_valid_draft_path(&args.path) {
            return Err(ToolRefusal::ProposalPathMissing);
        }
        // PDC-FR-LGWJ: a call about the whole list, so it names no place.
        let Some(open) = draft_proposals::pending_for(&self.roots.worktree, draft_id) else {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_OPEN));
        };
        // PDC-FR-05: the path names the draft's prompt, which the open
        // proposal changes.
        if args.path.trim() != open.path {
            return Err(ToolRefusal::ProposalPathMissing);
        }
        // PDC-FR-ZQOH: an update appends a comment too.
        if crate::comments::spoken_part(&args.rationale).is_none() {
            return Err(ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
        }
        // PDC-FR-UOBH: a malformed entry is refused with its place.
        let entries = proposal_updates::to_entries(updates)?;

        let mut file_rel = String::new();
        let target =
            target_for(&self.roots.store, &self.origin, draft_id, thread_id, &mut file_rel)?;
        // PDC-FR-VUTJ: all or nothing, through the one update path.
        draft_proposals::update_proposal(
            &self.app,
            &self.roots.worktree,
            &self.roots.store,
            ProposalUpdate {
                draft_id,
                updates: &entries,
                rationale: &args.rationale,
                agent: &self.author,
                target,
                thread_id,
            },
        )
        .map_err(update_refusal_for)?;
        // CVL-FR-15: an update ends the turn as a recording does.
        self.proposed.store(true, Ordering::SeqCst);
        Ok(ProposeDraftChangesOutput { proposed: true })
    }
}

/// `crate::draft_proposals`' update refusals in this tool's terms
/// (PDC-FR-LGWJ, PDC-FR-UOBH, PDC-FR-DBFT, PDC-FR-DGUC, PDC-FR-VSMM,
/// PDC-FR-SNUK). A refusal about one update names its place.
fn update_refusal_for(refusal: UpdateRefusal) -> ToolRefusal {
    let at = |message: &'static str, at: usize, of: usize| ToolRefusal::InvalidUpdateAt {
        message,
        at,
        of,
    };
    match refusal {
        UpdateRefusal::NoOpenProposal => ToolRefusal::InvalidArguments(PROPOSAL_NO_OPEN),
        UpdateRefusal::NoUpdates => ToolRefusal::InvalidArguments(UPDATE_NONE),
        UpdateRefusal::PathMissing => ToolRefusal::ProposalPathMissing,
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

impl<R: tauri::Runtime> rig::tool::PortableTool for ProposeDraftChangesTool<R> {
    const NAME: &'static str = NAME;

    type Args = ProposeDraftChangesArgs;
    type Output = ProposeDraftChangesOutput;
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
        // PDC-FR-10: records and returns. It waits for no decision, holds no
        // timer, and opens no watcher, so it resolves before the future is ever
        // polled.
        std::future::ready(self.run(arguments))
    }
}
