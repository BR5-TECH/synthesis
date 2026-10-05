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

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    PROPOSAL_NO_CHANGE, PROPOSAL_RATIONALE_BLANK,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant, ThreadRef};
use crate::draft_proposals::{self, NewProposal, RecordRefusal};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (PDC contract surface)
// ---------------------------------------------------------------------------

/// PDC-FR-01.
pub const NAME: &str = "propose_draft_changes";

/// PDC-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Propose changes to the prompt of the draft under discussion, when you can see how its text would be better and want the author to decide. A draft is one prompt, so this is the one file there is to propose changes to. Reach for it instead of describing the improvement in prose, and instead of asking permission to make one. Pass a list of changes. Each one names, in `before`, the exact text it changes, copied character for character from the prompt as you read it, and gives in `after` the text that should stand in its place. To insert new text, give `after` alone and put the exact text it follows in `after_text`. To delete, give `before` alone. Copy enough surrounding words into `before` that the text you name appears once in the prompt; if it appears more than once the call is refused and you are asked to include more. Say why the changes are worth making, in a sentence or two as you would put it to a colleague. Your proposal is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's decisions, and whatever they said about them, in front of you. The author decides each change separately, so send the changes you believe in together rather than holding some back. Offer the changes for the whole prompt one time, rather than a small improvement each time you read it. When you are answering about one change the author asked you to reconsider, name that change in `revises` and send only the one change that replaces it; the rest of what you would change waits until the author has decided the changes already standing. It reaches only the draft you were asked about, and it changes nothing — the prompt is not rewritten unless the author accepts.";

const PATH_DESCRIPTION: &str = "The draft-relative path of the draft's prompt, which is the one file a draft holds and the only thing you can propose changes to.";

const HUNKS_DESCRIPTION: &str = "The changes, in the order they should be read. In each, `before` is the exact existing text you are changing, copied character for character from the prompt, and `after` is what should stand in its place. Copy enough surrounding words into `before` that it appears only once in the prompt. For an insertion give `after` and put the exact text it follows in `after_text`; for a deletion give `before` alone.";

/// PDC-FR-TZKQ: the four refusals record-time placement produces, each naming
/// the correction the model has to make.
pub const PROPOSAL_NO_HUNKS: &str =
    "Pass at least one change. An empty proposal has nothing for the author to decide.";
pub const PROPOSAL_TEXT_NOT_FOUND: &str =
    "The text you gave in `before` is not in the prompt. Copy it character for character from the prompt as you read it.";
pub const PROPOSAL_TEXT_NOT_UNIQUE: &str =
    "The text you gave in `before` appears more than once in the prompt. Include more of the words around it so it names one place.";
pub const PROPOSAL_REVISE_ONE: &str =
    "You named a change to replace, so send only the one change that replaces it. Wait for the author to decide the changes already standing before you propose the rest.";
pub const PROPOSAL_NO_SUCH_CHANGE: &str =
    "The change you named is not one of the changes still awaiting the author. Propose a new change instead.";
pub const PROPOSAL_HUNKS_OVERLAP: &str =
    "Two of your changes cover the same text. Combine them into one change.";

const HUNK_DESCRIPTION: &str = "One change to the prompt. A replacement gives `before` and `after`; an insertion gives `after` and `after_text`; a deletion gives `before` alone. Never send an empty change.";

const KIND_DESCRIPTION: &str = "What this change does: `replace` puts `after` in place of `before`, `del` removes `before`, `add` inserts `after` immediately following `after_text`. The fields you send are what decide, so give the fields that match what you mean: a rewrite gives `before` and `after` together, and calling it `add` does not make it one.";

const BEFORE_DESCRIPTION: &str = "The exact existing text this change replaces or deletes, copied character for character from the prompt as you read it — including its punctuation and its capitals. Copy enough surrounding words that it appears only once in the prompt. Leave it out for an insertion.";

const AFTER_DESCRIPTION: &str = "The exact text that should stand in place of `before`, or the text to insert. Leave it out for a deletion.";

const AFTER_TEXT_DESCRIPTION: &str = "For an insertion only: the exact existing text the new text goes immediately after, copied character for character from the prompt. This is what says where the insertion goes, so it is required for an insertion into a prompt that has any text in it.";

const NOTE_DESCRIPTION: &str = "What this one change is for, in a few words. Optional; the author reads the rationale for the proposal as a whole.";

/// PDC-FR-18 / TLC-FR-14: which of this tool's invalid-argument refusals fired,
/// as a stable code.
///
/// `reason()` is one term for the whole family, deliberately, so a reader can
/// filter a column on it. This tool has eight refusals in that family, and a
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
        ToolRefusal::InvalidArgumentsAt { message, .. } => message,
        _ => return None,
    };
    Some(match *message {
        PROPOSAL_NO_HUNKS => "no_changes",
        PROPOSAL_TEXT_NOT_FOUND => "text_not_found",
        PROPOSAL_TEXT_NOT_UNIQUE => "text_not_unique",
        PROPOSAL_HUNKS_OVERLAP => "changes_overlap",
        PROPOSAL_REVISE_ONE => "revise_one",
        PROPOSAL_NO_SUCH_CHANGE => "no_such_change",
        PROPOSAL_RATIONALE_BLANK => "rationale_blank",
        PROPOSAL_NO_CHANGE => "no_change",
        // A refusal added without a code of its own is still followable as
        // itself rather than silently reading as one of the eight above.
        _ => "other",
    })
}

const REVISES_DESCRIPTION: &str = "The id of a change you were asked about, when you are replacing it with a better one. A call that names a change carries exactly one change — the one that replaces it. Leave it out when you are proposing something new.";

const RATIONALE_DESCRIPTION: &str = "Why the changes are worth making, in a sentence or two as you would put it to a colleague. The author reads this as your comment.";

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
    /// PDC-FR-GMWR: the id of a change being replaced, when the agent is
    /// answering about one it was asked to reconsider.
    #[serde(default)]
    pub revises: Option<String>,
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
    /// the `before` it does not hold (per `../draft_proposals/hunks.rs`), rather
    /// than as an insertion, which is placed on its `after_text` alone and would
    /// record a change with no text in it at all.
    fn to_proposed(&self) -> crate::draft_proposals::hunks::ProposedHunk {
        use crate::draft_proposals::anchors::HunkKind;
        let has_before = self.before.as_deref().is_some_and(|s| !s.is_empty());
        let has_after = self.after.as_deref().is_some_and(|s| !s.is_empty());
        let kind = match (has_before, has_after) {
            (true, true) => HunkKind::Replace,
            (true, false) => HunkKind::Del,
            (false, true) => HunkKind::Add,
            (false, false) => HunkKind::Replace,
        };
        crate::draft_proposals::hunks::ProposedHunk {
            kind,
            before: self.before.clone(),
            after: self.after.clone(),
            after_text: self.after_text.clone(),
            note: self.note.clone(),
        }
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
            "revises": { "type": "string", "description": REVISES_DESCRIPTION },
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
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["replace", "del", "add"],
                            "description": KIND_DESCRIPTION,
                        },
                        "before": { "type": "string", "description": BEFORE_DESCRIPTION },
                        "after": { "type": "string", "description": AFTER_DESCRIPTION },
                        "after_text": {
                            "type": "string",
                            "description": AFTER_TEXT_DESCRIPTION,
                        },
                        "note": { "type": "string", "description": NOTE_DESCRIPTION },
                    },
                },
            },
        },
        "required": ["path", "rationale", "hunks"],
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
        RecordRefusal::HunkAnchorLost { at, of } => ToolRefusal::InvalidArgumentsAt {
            message: PROPOSAL_TEXT_NOT_FOUND,
            at,
            of,
        },
        RecordRefusal::HunkAmbiguous { at, of } => ToolRefusal::InvalidArgumentsAt {
            message: PROPOSAL_TEXT_NOT_UNIQUE,
            at,
            of,
        },
        RecordRefusal::HunkOverlap { at, of } => ToolRefusal::InvalidArgumentsAt {
            message: PROPOSAL_HUNKS_OVERLAP,
            at,
            of,
        },
        RecordRefusal::ThreadLocked => ToolRefusal::ProposalConversationLocked,
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
        ProposeDraftChangesTool {
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

    /// The whole call, with its logging (TLC-FR-14, PDC-FR-18).
    fn run(
        &self,
        args: ProposeDraftChangesArgs,
    ) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
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
            "revising" => args.revises.is_some(),
            "rationaleChars" => args.rationale.trim().chars().count(),
        };
        if let Ok((draft_id, _)) = draft_of(&self.origin) {
            fields.insert("draftId".to_string(), serde_json::json!(draft_id));
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
        args: ProposeDraftChangesArgs,
    ) -> Result<ProposeDraftChangesOutput, ToolRefusal> {
        // PDC-FR-16: no project, no draft to propose to, so the shared refusal
        // (TLC-FR-13). Checked per call rather than at construction for the
        // reason `ask_user_comment` checks it there: the root this tool holds was
        // bound when the turn began, and a turn can outlive the project being
        // open.
        require_open_project(&self.app)?;
        let (draft_id, thread_id) = draft_of(&self.origin)?;

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

        // PDC-FR-GMWR: `revises` is read for what it can mean on the draft as it
        // stands. A call naming a change the author has still to decide replaces
        // that one in place, which is how an agent answers about a change held
        // for discussion.
        //
        // A field naming nothing the draft holds is a field the model filled by
        // mistake, and where the draft carries no standing proposal there is no
        // change it could have meant. Ignored there rather than refused
        // (TLC-FR-07): a call that names a change takes this path and never
        // reaches the recording below, so refusing would leave such a model
        // unable to propose anything at all — every call it made answered by a
        // refusal about a change that is not there.
        use crate::draft_proposals::RevisionTarget;
        let named = args
            .revises
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let target = named.map(|id| {
            (
                id,
                crate::draft_proposals::revision_target(&self.roots.worktree, draft_id, id),
            )
        });
        if let Some((_, RevisionTarget::NoSuchChange)) = target {
            return Err(ToolRefusal::InvalidArguments(PROPOSAL_NO_SUCH_CHANGE));
        }
        if let Some((revises, RevisionTarget::Revisable)) = target {
            // The one-change rule is checked here rather than before the name is
            // resolved, so a model that named a change wrongly is told **that**
            // — the correction it has to make — instead of being asked to send
            // one change against a name that would refuse whatever it sent.
            if proposed.len() != 1 {
                return Err(ToolRefusal::InvalidArguments(PROPOSAL_REVISE_ONE));
            }
            let outcome = crate::draft_proposals::revise_hunk_impl(
                &self.roots.worktree,
                &draft_id,
                revises,
                &proposed[0],
            );
            return match outcome {
                Ok(proposal) => {
                    crate::draft_proposals::announce(&self.app, self.buffer, &proposal);
                    self.proposed.store(true, Ordering::SeqCst);
                    Ok(ProposeDraftChangesOutput { proposed: true })
                }
                // PDC-FR-TZKQ: a revision is placed against the prompt exactly
                // as a recording is, so it fails in the same ways and must say
                // so in the same words. A replacement naming text the prompt
                // does not hold is that mistake and not a name that resolved to
                // nothing — told apart because the corrections are opposites:
                // copy the text again, against name a change that is still
                // there.
                //
                // Matched on the arguments this call can get wrong, and never as
                // a catch-all: a write that failed is nothing the model composed
                // (TLC-FR-10), and told it had named a change wrongly it would
                // go and compose a whole new proposal instead of calling again.
                // The recording path answers a failed write with the same
                // refusal, this being the same failure.
                Err(error) => Err(match error.as_str() {
                    crate::draft_proposals::ERR_ANCHOR_LOST => {
                        ToolRefusal::InvalidArguments(PROPOSAL_TEXT_NOT_FOUND)
                    }
                    crate::draft_proposals::ERR_HUNK_AMBIGUOUS => {
                        ToolRefusal::InvalidArguments(PROPOSAL_TEXT_NOT_UNIQUE)
                    }
                    crate::draft_proposals::ERR_HUNK_NOT_FOUND
                    | crate::draft_proposals::ERR_HUNK_ALREADY_DECIDED
                    | crate::draft_proposals::ERR_PROPOSAL_NOT_FOUND => {
                        ToolRefusal::InvalidArguments(PROPOSAL_NO_SUCH_CHANGE)
                    }
                    _ => ToolRefusal::ProposalNotRecorded,
                }),
            };
        }

        let mut file_rel = String::new();
        let target = target_for(
            &self.roots.store,
            &self.origin,
            draft_id,
            thread_id,
            &mut file_rel,
        )?;

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
