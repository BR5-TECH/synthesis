//! Holding a conversation with an agent. This module carries two specifications:
//! `specifications/core/AGC-agent-conversations.md`, which is how a conversation
//! reaches an agent — the commands, the turn's lifecycle, the per-kind context
//! builders, and the delivery of what comes back — and
//! `specifications/ai/CVL-conversation-loop.md`, which is how the conversation is
//! actually held: the prompt, the tools, the rounds, the retries, and the logging.
//!
//! When an author addresses one of the project's enrolled personas
//! (`crate::agents`) from a conversational surface, this module takes that
//! message, assembles the context a participant in that surface would have,
//! calls the agent's model through the endpoint `crate::ai_api` resolves for it,
//! and delivers the answer back into the surface the question came from — as an
//! ordinary contribution by an agent participant rather than as a notification
//! about one.
//!
//! Three properties shape the code below:
//!
//! - **A turn is never refused for load** (AGC-FR-03). Any number may be in
//!   flight in one conversation, for one agent, and across the application. A
//!   concurrency bound *delays* the call; it never declines the dispatch, on the
//!   principle that a participant asked a second question is answered rather
//!   than silenced.
//! - **Context assembly is a builder per origin kind** (AGC-FR-05). What a
//!   conversation carries differs by the surface it happens in, so adding a
//!   surface adds an origin kind and a builder and changes no command signature.
//! - **One seam reaches the network** (CVL-FR-11). [`CompletionSeam`] is the
//!   only place a model is called, and it is driven through `rig`'s own
//!   completion-model abstraction — so a build under test substitutes a scripted
//!   model wholesale rather than intercepting a URL, and the whole suite runs
//!   with no request leaving the machine and no token spent.
//!
//! A turn is otherwise invisible: it runs on a thread of its own, it answers
//! into a surface the author may not be looking at, and everything interesting
//! about it — which provider carried it, how long the model took, why it
//! produced nothing — is gone the moment it terminates. So each stage reports
//! itself to `crate::logging` under the `ai` domain (`remote` as well wherever
//! the network is involved), which is what the Logs panel reads.
//!
//! **Nothing a participant wrote is logged, and no key ever is** (AGC-FR-26).
//! The records carry the *shape* of the conversation — section tags, character
//! counts, a truncation flag, a duration, a typed failure — and never a body, a
//! prompt, a reply, or a credential: the logging facility redacts nothing
//! (LGC-FR-16), and an exported record travels as far as the bug report it is
//! attached to.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::agents::{self, Agent};
use crate::ai_api::{self, reasoning_label, AiApiCall, AiApiIntegrations};
use crate::comments::{self, Discussion, DiscussionTarget, Participant};

/// The two roots a turn reads (AGC-FR-06, `CMS-comments-storage.md` CMS-FR-01).
///
/// `worktree` is the active worktree, which holds the material under discussion
/// — an artifact, a draft's prompt, a note. `store` is the repository machine
/// store, which holds the conversation itself and its attachments
/// (`RMS-repository-machine-storage.md` RMS-FR-MVDU). They are carried as one
/// value so no builder can read a conversation from a worktree, or material
/// from a store, by picking the wrong parameter.
#[derive(Clone, Copy)]
pub struct Roots<'a> {
    pub worktree: &'a crate::fs::RootFs,
    pub store: &'a crate::fs::RootFs,
}

/// [`Roots`] owned, for a value that outlives the call that built it — a turn's
/// plan, and a tool bound to a conversation for the whole of a turn.
#[derive(Clone)]
pub struct OwnedRoots {
    pub worktree: crate::fs::RootFs,
    pub store: crate::fs::RootFs,
}

impl OwnedRoots {
    pub fn borrowed(&self) -> Roots<'_> {
        Roots {
            worktree: &self.worktree,
            store: &self.store,
        }
    }
}

impl<'a> Roots<'a> {
    /// The pair a test builds over one temporary directory, which serves as
    /// both the worktree and the store.
    #[cfg(test)]
    pub fn same(root: &'a crate::fs::RootFs) -> Roots<'a> {
        Roots {
            worktree: root,
            store: root,
        }
    }

    pub fn owned(&self) -> OwnedRoots {
        OwnedRoots {
            worktree: self.worktree.clone(),
            store: self.store.clone(),
        }
    }
}
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, LogBuffer};
use crate::notes::now_rfc3339;
use crate::progress::{self, OperationId, OperationState, ProgressRegistry};
use crate::project::ProjectState;
use crate::scanning;

// ---------------------------------------------------------------------------
// The module's parts
// ---------------------------------------------------------------------------
//
// One module, in files. Each part carries `use super::*`, so it reads the
// imports above and every other part's items exactly as one file did, and each
// part's items are re-exported here — a caller outside names
// `crate::agent_conversations::X` and nothing else changes.
//
// Three things stay in this file because a test reads this file as text and
// would otherwise guard nothing: the four prompt templates and their
// `include_str!` (`tests/request_shape.rs`), the three commands that test names
// (`dispatch_agent_turn`, `cancel_agent_turn`, `list_agent_turns`), and
// [`conversation_tools`], whose exclusive attachment of the two proposal tools
// is read by `crate::tools::propose_prompt_changes`.

mod builders;
mod cancellation;
mod citation_markers;
mod delivery;
mod dispatch;
mod failures;
mod native_calls;
mod openrouter_carrier;
mod prose_tool_calls;
mod provider_memory;
mod registry;
mod responses_repair;
mod reporting;
mod retries;
mod rig_bridge;
mod rig_messages;
mod seam;
mod sections;
mod tool_dispatch;
mod tool_wiring;
mod turn_body;
mod wire;

pub use builders::*;
pub use cancellation::*;
pub(crate) use citation_markers::strip_citation_markers;
use citation_markers::strip_reply_citation_markers;
use delivery::*;
pub use dispatch::*;
pub use failures::*;
use native_calls::*;
use openrouter_carrier::*;
use prose_tool_calls::*;
// The memories are read and written in this module only, so the re-export is
// private. The two seams a test clears them through are reached the same way.
use provider_memory::*;
pub use registry::*;
use reporting::*;
use retries::*;
pub use rig_bridge::*;
pub use rig_messages::*;
pub use seam::*;
use sections::*;
use tool_dispatch::*;
use tool_wiring::*;
use turn_body::*;
pub use wire::*;

/// The scripted seam, which no shipped build holds (CVL-FR-11).
#[cfg(test)]
mod scripted;
#[cfg(test)]
pub use scripted::*;

// ---------------------------------------------------------------------------
// The prompts (CVL-FR-02 .. CVL-FR-04)
// ---------------------------------------------------------------------------
//
// The four templates are compiled in from `resources/prompts/` and named
// nowhere else, which `tests/request_shape.rs` asserts against this file's own
// text. Moving one to a part would leave that assertion reading a file the
// template is not in, so they stay here.

/// CVL-FR-02: the prompt a remark pinned to a passage is answered under, compiled
/// into the binary at build time.
///
/// Not read from disk on any turn, so a turn is unaffected by what the installed
/// application's directories hold and an author can neither edit nor replace it.
/// A build either carries both prompts or does not build at all — a missing file
/// is a compile error rather than a turn that reaches a model with no instruction.
///
/// It is application data on the same footing as a provider descriptor
/// (AAP-FR-01): nothing in the contract surface accepts it, returns it, or
/// overrides it.
pub const COMMENT_PROMPT_TEMPLATE: &str = include_str!("../../resources/prompts/comment.md");

/// CVL-FR-02 / CVL-FR-03: the prompt a discussion of a **file already in the
/// project** is answered under, on the same terms.
///
/// A template of its own rather than a branch inside the first, because what is
/// asked of the agent differs in kind: the material is a whole document and the
/// question is what to do with it, rather than what is wrong with a line.
pub const DISCUSS_ARTIFACT_PROMPT_TEMPLATE: &str =
    include_str!("../../resources/prompts/discuss-artifact.md");

/// CVL-FR-02 / CVL-FR-03: the prompt a discussion of a **draft** is answered
/// under.
///
/// A third template rather than a shared one with the artifact discussion, for
/// the reason that one is not shared with `comment.md`: a draft is unpublished
/// working material on its way to being handed to an agent that will act on it,
/// so what is asked here is what the prompt it holds will *cause* — and this is
/// the only origin kind that can offer the author a rewrite of the material
/// (CVL-FR-08), which an instruction has to tell the agent it may do.
pub const DISCUSS_DRAFT_PROMPT_TEMPLATE: &str =
    include_str!("../../resources/prompts/discuss-draft.md");

/// CVL-FR-02 / CVL-FR-03: the prompt a discussion of a **note** is answered
/// under.
///
/// A template of its own because the material is a different kind of thing: a
/// few lines the author wrote to themselves about a gap, an issue, or a
/// follow-up, rather than a document. What is asked is what to make of that
/// observation — and the instruction has to say that the scope the note is filed
/// against is where it sits rather than what it is about, which is the one
/// mistake an agent handed a note would otherwise make.
pub const DISCUSS_NOTE_PROMPT_TEMPLATE: &str =
    include_str!("../../resources/prompts/discuss-note.md");

/// CVL-FR-03: a turn's prompt is selected by its origin kind and by nothing else.
///
/// The selection is the extension point beside the builder of AGC-FR-05: a
/// conversational surface added later brings an origin kind, a builder, and
/// whichever of the prompts already compiled in answers its shape — or a new one
/// of its own — and changes no command signature and no payload shape.
pub fn prompt_template(kind: OriginKind) -> &'static str {
    match kind {
        // Both are a remark pinned to a passage, and are answered under one
        // instruction whichever file the passage is in.
        OriginKind::ArtifactComment | OriginKind::DraftComment => COMMENT_PROMPT_TEMPLATE,
        // A whole file the project already holds: what is asked is what to do
        // with the document as it stands.
        OriginKind::ArtifactDiscussion => DISCUSS_ARTIFACT_PROMPT_TEMPLATE,
        // A whole draft: unpublished material the author is still shaping, and
        // the one origin kind an agent can propose a rewrite of.
        OriginKind::DraftDiscussion => DISCUSS_DRAFT_PROMPT_TEMPLATE,
        // A note: the material is the author's own observation rather than any
        // file, and the one origin kind whose first section is not `artifact`.
        OriginKind::NoteDiscussion => DISCUSS_NOTE_PROMPT_TEMPLATE,
    }
}

/// CVL-FR-29: what a template **says**, which is the file with its authoring
/// comments removed and the blank space they left closed up.
///
/// A comment in one of the four files is a note to whoever maintains it and is
/// never part of the instruction a turn carries. The removal happens here,
/// before [`compile_prompt`] substitutes anything, so a comment can never
/// contribute a placeholder, a substituted value, or a word of what a turn is
/// told — a `{{ agent-instructions }}` written inside a comment is gone before
/// the substitution pass ever sees it.
///
/// A template carrying no comment is returned unchanged, which is every one of
/// the four today; the rule binds whichever of them grows one.
pub fn prompt_instruction(kind: OriginKind) -> String {
    crate::prompts::instruction_text(prompt_template(kind))
}

/// CVL-FR-04: the placeholder the agent's own instructions are substituted
/// into.
pub const AGENT_INSTRUCTIONS_PLACEHOLDER: &str = "{{ agent-instructions }}";

/// CVL-FR-04: the placeholder the agent's title is substituted into.
pub const AGENT_TITLE_PLACEHOLDER: &str = "{{ agent-title }}";

/// CVL-FR-04: what an **empty** title substitutes.
///
/// The one value in this module that fills a placeholder with something rather
/// than with nothing, and deliberately: each template builds a sentence around
/// the title, and an empty value there leaves a model reading a sentence with a
/// hole in it. It belongs to this substitution alone — no surface renders it
/// (CTA-FR-KFUF), and the registry never stores it.
pub const AGENT_TITLE_UNDEFINED: &str = "Not defined";

/// CVL-FR-04: the prompt a turn works under, with the agent's title and its own
/// instructions composed into it.
///
/// Both substitutions are literal and performed **in one pass over the
/// template**, which is what keeps either value from reaching the other's
/// placeholder. The pass is built by splitting the template on the two
/// placeholders and joining the pieces around the supplied values, so a
/// substituted value is never itself scanned: an agent whose title reads
/// `{{ agent-instructions }}`, or whose instructions read `{{ agent-title }}`,
/// gets that text back as the literal it was. A `replacen` chain would not hold
/// this — the second call would scan what the first had just substituted, and an
/// agent could reach the rest of the template by writing the other placeholder
/// into a field it controls.
///
/// Instructions that are empty or hold nothing but whitespace are no
/// instructions for the purpose of the call, and their placeholder is replaced
/// with nothing. That is an ordinary state rather than an unconfigured one
/// (AGR-FR-08): an agent carrying none still answers under the whole of the rest
/// of the prompt. A field cleared by selecting its text and pressing space
/// leaves a stray behind, and appending one to the prompt is not a persona.
///
/// An empty **title** is the one value that substitutes something rather than
/// nothing — [`AGENT_TITLE_UNDEFINED`], for the reason stated there. The title
/// arrives already trimmed from the registry (AGR-FR-23); trimming again here
/// costs nothing and keeps a caller that hand-built an `Agent` from producing a
/// prompt the stored path never would.
///
/// What the registry stores is untouched by any of this — a decision about what
/// to *send*, not about what to keep.
pub fn compile_prompt(origin_kind: OriginKind, instructions: &str, title: &str) -> String {
    compile_from_template(prompt_template(origin_kind), instructions, title)
}

/// The composition itself, over whichever template it is handed.
///
/// Split from [`compile_prompt`] so the **order** of the two steps is
/// exercisable: CVL-FR-29 is a claim about stripping happening before
/// substituting, and a test that reaches only `compile_prompt(kind, …)` can
/// only ever see templates that carry no comment, which is every one of the
/// four today. Handed a template that does, this is the path that has to get
/// the order right.
pub(crate) fn compile_from_template(template: &str, instructions: &str, title: &str) -> String {
    let instructions = if instructions.trim().is_empty() {
        ""
    } else {
        instructions
    };
    let title = title.trim();
    let title = if title.is_empty() {
        AGENT_TITLE_UNDEFINED
    } else {
        title
    };
    substitute_once(
        // CVL-FR-29: the template's instruction text rather than the file, and
        // taken **before** the substitution rather than after it, so a comment
        // can never contribute a placeholder or a substituted value.
        &crate::prompts::instruction_text(template),
        &[
            (AGENT_TITLE_PLACEHOLDER, title),
            (AGENT_INSTRUCTIONS_PLACEHOLDER, instructions),
        ],
    )
}

/// One left-to-right pass over `template`, replacing the **first** occurrence of
/// each placeholder in `pairs` with its value and copying everything else
/// through untouched.
///
/// The pass is what makes CVL-FR-04's non-recursion hold for more than one
/// placeholder at a time: the scan only ever advances past text it has already
/// emitted, so a value carrying another placeholder is emitted and left behind
/// rather than reconsidered. A placeholder absent from the template simply never
/// matches, which is how a template that names only one of the two still
/// compiles.
fn substitute_once(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    // Which placeholders are still eligible: each substitutes at most once, so a
    // template naming one twice keeps its second occurrence as literal text.
    let mut pending: Vec<(&str, &str)> = pairs.to_vec();
    while !pending.is_empty() {
        // The earliest pending placeholder still ahead of us. Earliest rather
        // than first-in-`pairs`, so the result does not depend on the order the
        // caller happened to list them in.
        let Some((at, index)) = pending
            .iter()
            .enumerate()
            .filter_map(|(i, (needle, _))| rest.find(needle).map(|at| (at, i)))
            .min()
        else {
            break;
        };
        let (needle, value) = pending.remove(index);
        out.push_str(&rest[..at]);
        out.push_str(value);
        rest = &rest[at + needle.len()..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// The tools a turn is offered (CVL-FR-08)
// ---------------------------------------------------------------------------
//
// The rest of the wiring is in `tool_wiring`. This one function stays because
// `crate::tools::propose_prompt_changes` reads this file's text to assert that
// the two proposal tools are attached exclusively (CVL-FR-08 / PPC-FR-21).

/// CVL-FR-08: the tools attached to a turn, decided by its **origin kind** and by
/// nothing else.
///
/// Named here rather than fetched from a registry, because `TLC-tool-conventions.md`
/// publishes none (TLC-FR-19): a capability reaches a model only where a caller
/// wrote its name down, so a tool added to that group cannot silently widen what
/// a conversation can do.
///
/// Eleven go to every turn. A turn on a **discussion** origin gets
/// `ask_discussion_questions` beside them, because a discussion is where
/// material is decided rather than corrected and an agent reading a whole
/// document finds several open points where one reading a passage finds one
/// (ADQ-FR-LFDX). A turn on a **draft** origin gets `propose_draft_changes`,
/// because there is unpublished working material for it to offer a rewrite of —
/// which a file already in the project is not. Nothing else varies the set: no
/// agent, project, or model widens or narrows what the origin kind decides, so
/// two turns of one kind were offered byte-identical definitions of byte-identical
/// tools.
///
/// `search_drafts` and `read_draft` go everywhere for the reason
/// `search_specifications` and `read_file` do: reading what the project has
/// *planned* is the same kind of move as reading what it has decided, and an
/// agent that could reach the drafts only while discussing one would give a
/// worse answer about a published file than about an unpublished one. Reading a
/// draft and rewriting one are separate reaches — `read_draft` loads any draft's
/// current prompt from any origin and changes nothing (RDT-FR-16), while
/// `propose_draft_changes` offers a rewrite of the one draft its constructor
/// bound it to.
///
/// `search_notes` goes everywhere on the same principle: a note is where a gap
/// or a small thing still to be done is written down, so work of any kind is
/// answered better by an agent that can tell whether the project already holds a
/// note that work would close (NST-FR-04). A `note_discussion` turn is no
/// exception and is no special case either — searching the notes and rewriting a
/// prompt are unrelated reaches, so that turn is attached the eleven and neither
/// proposal tool, there being nothing about a note to rewrite.
///
/// `search_documents` and `get_document` go everywhere on the same principle: a
/// reference document the user selected is material any work may rest on, so an
/// agent of any kind can find one and read it by id (SDT-FR-LNNL, GDT-FR-LITM).
///
/// `session` is the agent session the turn runs inside (CVL-FR-24). The four
/// tools that touch the filesystem take it — `read_file`, `search_drafts`,
/// `read_draft`, and `search_notes` — so an agent's whole reach is that
/// session's one `FsAccess` instance (FSA-FR-29), and binding it here is what
/// makes RFT-FR-05 a property of the tools rather than a convention.
///
/// The turn-ending tools take rather more: `ask_user_comment` posts into *this*
/// conversation as *this* agent, and both are bound here rather than named in an
/// argument (AUC-FR-03) — the model chooses what to ask and never where the
/// question lands. `ask_discussion_questions` records against *this* discussion
/// as *this* agent on the same terms (ADQ-FR-HTGC). `asked`, `asked_questions`
/// and `proposed` come back the other way, so the loop can tell a contribution
/// that went out from one that was refused (CVL-FR-15).
fn conversation_tools<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    author: &Participant,
    asked: &Arc<AtomicBool>,
    asked_questions: &Arc<AtomicBool>,
    proposed: &Arc<AtomicBool>,
) -> Vec<rig::tool::DynamicTool> {
    let mut tools = vec![
        erase_tool(crate::tools::spec_search::SpecSearchTool::new(app.clone())),
        erase_tool(crate::tools::file_read::FileReadTool::new(
            app.clone(),
            session,
        )),
        erase_tool(crate::tools::draft_search::DraftSearchTool::new(
            app.clone(),
            session,
        )),
        erase_tool(crate::tools::draft_read::DraftReadTool::new(
            app.clone(),
            session,
        )),
        erase_tool(crate::tools::note_search::NoteSearchTool::new(
            app.clone(),
            session,
        )),
        // CVL-FR-08: the two that reach the user's reference documents. Neither
        // takes the agent session: they read through the Documents collection's
        // own read-only instance and never through the session's (GDT-FR-HCYY).
        erase_tool(crate::tools::document_search::SearchDocumentsTool::new(
            app.clone(),
        )),
        erase_tool(crate::tools::document_get::GetDocumentTool::new(
            app.clone(),
        )),
        erase_tool(crate::tools::skill_search::SkillSearchTool::new(app.clone())),
        erase_tool(crate::tools::skill_list::SkillListTool::new(app.clone())),
        erase_tool(crate::tools::skill_load::SkillLoadTool::new(app.clone())),
        erase_tool(crate::tools::ask_user_comment::AskUserCommentTool::new(
            app.clone(),
            roots.owned(),
            origin.clone(),
            author.clone(),
            Arc::clone(asked),
        )),
    ];
    // CVL-FR-08: a discussion origin is attached `ask_discussion_questions`
    // beside the nine, and an anchored comment origin is attached none — a
    // passage under review being one remark rather than a set of open points
    // (ADQ-FR-LFDX). Bound to this discussion by its constructor for the reason
    // `ask_user_comment` is (ADQ-FR-HTGC): the model chooses what to ask and
    // never where the questions land.
    if origin.is_discussion() {
        tools.push(erase_tool(
            crate::tools::ask_discussion_questions::AskDiscussionQuestionsTool::new(
                app.clone(),
                roots.owned(),
                origin.clone(),
                author.clone(),
                Arc::clone(asked_questions),
            ),
        ));
    }
    // CVL-FR-08: the proposal tool, and which one it is depends on the origin kind
    // alone. A draft origin is about a draft's own prompt; an artifact origin is
    // about a file of the project, whose rewrite is offered only where that
    // file's resolved type is `prompt` — a condition the tool itself checks
    // (PPC-FR-06), the origin kind saying which **store** is in play rather than
    // what the file turned out to be. The two are never attached together, and a
    // note origin is attached neither: there is nothing about a note to rewrite.
    //
    // Each is bound to this conversation by its constructor for the reason
    // `ask_user_comment` is (PDC-FR-03, PPC-FR-03): the model chooses what to
    // propose and never what to propose it into.
    if origin.is_draft() {
        tools.push(erase_tool(
            crate::tools::propose_draft_changes::ProposeDraftChangesTool::new(
                app.clone(),
                roots.owned(),
                origin.clone(),
                author.clone(),
                Arc::clone(proposed),
            ),
        ));
    } else if origin.is_artifact() {
        tools.push(erase_tool(
            crate::tools::propose_prompt_changes::ProposePromptChangesTool::new(
                app.clone(),
                roots.owned(),
                origin.clone(),
                author.clone(),
                Arc::clone(proposed),
            ),
        ));
    }
    tools
}

// ---------------------------------------------------------------------------
// Tauri commands (AGC contract surface)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn dispatch_agent_turn<R: tauri::Runtime>(
    nickname: String,
    origin: ConversationOrigin,
    trigger_comment_id: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<AgentTurn, String> {
    let key = project
        .anchor()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project
        .require_root()
        .map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    // RMS-FR-HAJC: the store's own refusal, passed through rather than rewritten
    // as "no project open" — a project *is* open, and the two are different
    // things for the surface to say.
    let store = project.require_store()?;
    dispatch_impl(
        &app,
        Roots {
            worktree: &root,
            store: &store,
        },
        &key,
        &nickname,
        origin,
        &trigger_comment_id,
    )
}

#[tauri::command]
pub fn cancel_agent_turn<R: tauri::Runtime>(
    turn_id: String,
    app: tauri::AppHandle<R>,
    turns: State<'_, TurnRegistry>,
    progress_registry: State<'_, ProgressRegistry>,
    project: State<'_, ProjectState>,
) -> Result<AgentTurn, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    cancel_impl(&app, &turns, &progress_registry, &turn_id)
}

#[tauri::command]
pub fn list_agent_turns(
    origin: Option<ConversationOrigin>,
    turns: State<'_, TurnRegistry>,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentTurn>, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    Ok(turns.in_flight(origin.as_ref()))
}

/// AGC-FR-31: the recovery registry — at most one turn per conversation, each
/// `failed`, each carrying `retry_permitted`, most recently failed first.
///
/// What a surface remounted after a card was closed reads to recover the offer
/// to retry it would otherwise have lost with the event it was not listening
/// for. A turn returned here is not outstanding, so `list_agent_turns` returns
/// it under neither form.
#[tauri::command]
pub fn list_recoverable_agent_turn_failures(
    origin: Option<ConversationOrigin>,
    turns: State<'_, TurnRegistry>,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentTurn>, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    Ok(turns.recoverable_failures(origin.as_ref()))
}

/// AGC-FR-39: the notice registry — at most one turn per conversation, each
/// having ended with `images_omitted` true, most recent first.
///
/// What a surface mounted after a turn ended reads to say that the pictures were
/// not sent, exactly as `list_recoverable_agent_turn_failures` is what it reads
/// to recover an offer to retry (per `../ui/CMT-comments.md` CTA-FR-XSGX). What it
/// answers is the **current state** of each conversation rather than a history
/// of it: an entry is replaced by a later turn that also omitted images and
/// retired by a later turn that carried its images successfully or carried none
/// at all.
#[tauri::command]
pub fn list_agent_turn_image_notices(
    origin: Option<ConversationOrigin>,
    turns: State<'_, TurnRegistry>,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentTurn>, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    Ok(turns.image_notices(origin.as_ref()))
}

/// AGC-FR-32: register a **new** turn for the failed one's agent, origin, and
/// trigger comment.
///
/// Refuses any turn that is not its conversation's current entry — one that
/// never failed, one whose failure was not recoverable, one already retried, and
/// one displaced by a later failure alike — and a refusal registers nothing and
/// removes nothing. Nothing of the failed turn carries over: the new turn
/// assembles its input afresh (AGC-FR-13), begins its own session, and resolves
/// its own endpoint and key, so the failed turn is neither mutated nor
/// resurrected and keeps its own id and terminal state.
#[tauri::command]
pub fn retry_agent_turn<R: tauri::Runtime>(
    turn_id: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<AgentTurn, String> {
    let key = project
        .anchor()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project
        .require_root()
        .map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    // RMS-FR-HAJC: the store's own refusal, passed through rather than rewritten
    // as "no project open" — a project *is* open, and the two are different
    // things for the surface to say.
    let store = project.require_store()?;
    retry_impl(
        &app,
        Roots {
            worktree: &root,
            store: &store,
        },
        &key,
        &turn_id,
    )
}

/// AGC-FR-32's whole behaviour, reachable without the Tauri runtime.
pub fn retry_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    turn_id: &str,
) -> Result<AgentTurn, String> {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();

    // The entry is taken under one lock, so two retries issued together leave
    // exactly one new turn: the loser finds nothing to take and is refused,
    // exactly as a retry of a displaced turn is.
    let Some((failed, entry_project_key)) = turns.take_recoverable(turn_id) else {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent turn retry refused",
            log_fields! {
                "turnId" => turn_id,
                "reason" => ERR_TURN_NOT_FOUND,
            },
        );
        return Err(ERR_TURN_NOT_FOUND.to_string());
    };

    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "retrying a failed agent turn",
        log_fields! {
            "failedTurnId" => &failed.id,
            "agent" => &failed.nickname,
            "agentId" => &failed.agent_id,
            "failure" => failed.failure.as_deref().unwrap_or("none"),
            "originKind" => failed.origin.kind().as_str(),
            "discussionId" => failed.origin.discussion_id(),
        },
    );

    // Dropped before dispatching: `dispatch_impl` reads the registry off the app
    // handle itself and would otherwise contend with a `State` still borrowed
    // here for the whole of this call.
    drop(turns);

    dispatch_impl(
        app,
        roots,
        project_key,
        &failed.nickname,
        failed.origin.clone(),
        &failed.trigger_comment_id,
    )
    .inspect_err(|reason| {
        // AGC-FR-32 / CTA-FR-UUXA: a dispatch refused synchronously leaves the
        // author exactly where they were — the entry goes back, so the surface
        // restores its failed contribution and the offer is there to take again.
        // Without this a locked thread would silently swallow the only route
        // back to a turn that never happened.
        let turns = app.state::<TurnRegistry>();
        turns.remember_recoverable(failed.clone(), entry_project_key.clone());
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent turn retry was refused; the offer stands",
            log_fields! {
                "failedTurnId" => &failed.id,
                "agent" => &failed.nickname,
                "reason" => reason.as_str(),
            },
        );
    })
}

/// AGC-FR-31: a later **human** comment in this conversation retires its offer
/// to retry.
///
/// Called from the human write path alone (CMS-FR-11's acting participant), so
/// an agent's own answer retires nothing: an agent answering elsewhere in the
/// thread does not mean the author has stopped wanting the answer that failed.
pub fn retire_recoverable_for_human_comment<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    thread_id: &str,
) {
    use tauri::Manager;
    let Some(turns) = app.try_state::<TurnRegistry>() else {
        return;
    };
    let Some(turn) = turns.retire_recoverable(thread_id) else {
        return;
    };
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "an offer to retry was retired by a later comment",
        log_fields! {
            "turnId" => &turn.id,
            "agent" => &turn.nickname,
            "discussionId" => thread_id,
        },
    );
}

/// AGC-FR-TQLC: retire the turns a discussion has outstanding, because its
/// question set has been answered.
///
/// A submission retires or discards the asking turn **whether or not that agent
/// is among the agents the submission dispatches to**. The ordinary retirement
/// is per agent at dispatch (`dispatch.rs`), which would leave an agent that
/// asked and was not dispatched to awaiting a reply for good — the answers reach
/// the agents as fresh later turns and never by resuming the turn that asked
/// (ADQ-FR-ZXAF).
///
/// Called from the submission alone, on its success path.
pub fn retire_awaiting_for_thread<R: tauri::Runtime>(app: &tauri::AppHandle<R>, thread_id: &str) {
    use tauri::Manager;
    let Some(turns) = app.try_state::<TurnRegistry>() else {
        return;
    };
    let retired = turns.retire_awaiting_on_thread(thread_id);
    if retired == 0 {
        return;
    }
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "turns awaiting a question set were retired by its answers",
        log_fields! { "discussionId" => thread_id, "turns" => retired as u64 },
    );
}

#[cfg(test)]
mod tests;
