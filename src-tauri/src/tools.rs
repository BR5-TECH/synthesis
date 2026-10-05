//! Tool conventions (`TLC-tool-conventions.md`).
//!
//! The rules every tool in this group follows, so a tool can be handed to an
//! agent by a caller that knows nothing about it beyond its constructor. A tool
//! is a capability the application lends to a model: the model decides when to
//! reach for it, calls it with arguments it composed itself, and reads the
//! result as text in its own context. That is what makes a tool's description,
//! its parameter schema, and its refusal messages contract rather than
//! documentation — they are the entire interface a model has (TLC-FR-03).
//!
//! A tool the application implements is one implementation of
//! [`rig::tool::PortableTool`] and nothing else (TLC-FR-01): no
//! `#[tauri::command]`, no Tauri event, and nothing the frontend can
//! `invoke()`.
//!
//! ## The provider-native class
//!
//! A **provider-native** tool is model-facing too, but the application
//! implements none of it (TLC-FR-21). It is one definition entry —
//! [`ProviderNativeTool`] — naming a capability the provider itself executes,
//! so the provider owns the name the model sees, the arguments it composes, the
//! work, and the result, and this application owns only the decision to offer
//! it. Reaching the web is the capability that takes this shape:
//! [`web_search`] and [`web_fetch`] are the two tools of this class, and both
//! are OpenRouter's (TLC-FR-23). Nothing about either is settable or stored
//! anywhere (TLC-FR-22), the application makes no request of its own to carry
//! one out (TLC-FR-25), and only a conversational turn attaches one
//! (TLC-FR-24).
//!
//! ## Why there is no registry
//!
//! A caller names the tools it attaches (TLC-FR-19). This module publishes no
//! `builtin_tools()` and no other call returning the whole set, so adding a tool
//! here cannot silently widen what an existing agent can do — a capability
//! reaches a model only where someone wrote its name down.
//!
//! ## Why the refusal type is shared
//!
//! TLC-FR-13 requires the no-open-project refusal to read identically whichever
//! tool produced it. One type spanning the group is what holds that in place;
//! two types with two copies of the sentence would drift on the first edit.

use tauri::Manager;

use crate::log_fields;
use crate::logging::{self, Domain, Fields, LogBuffer, LogSink};

pub mod agent_exec;
pub mod ask_discussion_questions;
pub mod ask_user_comment;
pub mod document_get;
pub mod document_search;
pub mod draft_read;
pub mod draft_search;
pub mod escalate_to_user;
pub mod file_read;
pub mod note_search;
pub mod read_graduation_file;
pub mod propose_draft_changes;
pub mod propose_prompt_changes;
pub mod refusals;
pub mod skill_list;
pub mod skill_load;
pub mod skill_search;
pub mod spec_search;
pub mod web_fetch;
pub mod web_search;

#[cfg(test)]
pub(crate) mod tests;

/// The shared refusal vocabulary (TLC-FR-09, TLC-FR-10, TLC-FR-11).
///
/// Re-exported rather than referenced through its module, so a tool reaches
/// `crate::tools::NO_PROJECT_OPEN` and `crate::tools::ToolRefusal` exactly as it
/// did before the split.
pub use refusals::*;

// ---------------------------------------------------------------------------
// The provider-native entry (TLC-FR-21)
// ---------------------------------------------------------------------------

/// TLC-FR-21: one definition entry, which is the whole of the application's side
/// of a provider-native tool.
///
/// There is no `NAME`, no `description()`, no `parameters()`, no argument type,
/// no output shape, no error type, and no `call()`, because nothing here is ever
/// called. The entry rides in the `tools` array of a request beside the
/// definitions of the portable tools that request offers, and it declares its
/// `type` and no other field — so the provider applies whatever default it
/// applies to an entry carrying nothing else, and no author, project, or setting
/// can vary it (TLC-FR-22).
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProviderNativeTool {
    /// The provider-qualified capability, e.g. `openrouter:web_search`.
    #[serde(rename = "type")]
    pub tool_type: String,
}

impl ProviderNativeTool {
    /// The entry for one capability. Constructed from a constant its own module
    /// owns rather than from anything composed per call (TLC-FR-22).
    pub fn new(tool_type: &str) -> Self {
        Self {
            tool_type: tool_type.to_string(),
        }
    }
}


// ---------------------------------------------------------------------------
// Shared naming (SST-FR-12, SLT-FR-13, LSK-FR-05)
// ---------------------------------------------------------------------------

/// A skill name reduced to what the three tools in this group compare on.
///
/// One function so the three agree. `SLT-skill-list-tool.md` and
/// `SST-skill-search-tool.md` collapse duplicates on it and
/// `LSK-load-skill-tool.md` resolves against it; were the list to treat
/// `Review` and `review` as two skills while the load treated them as one, the
/// list would show a model two entries that `load_skill` then refuses as
/// ambiguous, and its description's promise that each name appears once would
/// be false.
pub fn normalize_skill_name(name: &str) -> String {
    name.trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Logging (TLC-FR-14)
// ---------------------------------------------------------------------------

/// The domains every record in this group carries.
///
/// `ai` and `backend` together: a tool call is work the backend performs
/// because a model asked it to, and a reader filtering on either should see it.
const TOOL_DOMAINS: [Domain; 2] = [Domain::Ai, Domain::Backend];

/// TLC-FR-14: an argument a tool's own spec names as loggable, bounded.
///
/// A tool's results are project material and are never loggable; an argument is
/// loggable only where the tool's own specification names it and takes
/// responsibility for it — `read_file`'s `path` (RFT-FR-20),
/// `search_specifications`'s `query` (SPS-FR-19), `propose_draft_changes`'s
/// `path` (PDC-FR-18), `propose_prompt_changes`'s `path` (PPC-FR-19),
/// `search_drafts`'s `query` (DST-FR-21), and `read_draft`'s
/// `draft_id` (RDT-FR-17) are the arguments named today. Every one of them is a
/// string a model composed, which is why they are bounded here:
/// `LGC-logging.md`'s per-record ceiling (LGC-FR-08) replaces a whole record's
/// fields when one of them makes it oversized, so an unbounded 20 KB argument
/// would cost the record the tool name and the reason — precisely what a reader
/// following a turn cannot do without. The bound counts characters rather than
/// bytes so it never lands inside one.
pub fn bounded_argument(value: &str, max_chars: usize) -> String {
    match value.char_indices().nth(max_chars) {
        Some((byte, _)) => format!("{}…", &value[..byte]),
        None => value.to_string(),
    }
}

/// TLC-FR-07 / TLC-FR-14: the reason a call refused before the tool ever saw it,
/// because the arguments did not decode into the tool's argument type.
///
/// A stable code rather than a sentence, for the reason [`ToolRefusal::reason`]
/// is one: a reader filters a column on it. It is told apart from the eight
/// refusals a tool authors itself because the correction differs — every one of
/// those is a value the model chose wrongly, and this is a **shape** the model
/// composed wrongly, which no tool ever gets the chance to describe.
pub const ARGUMENTS_UNDECODABLE: &str = "arguments_undecodable";

/// How much of an argument shape reaches a record, and how deep it is read.
const SHAPE_CHARS: usize = 300;
const SHAPE_DEPTH: usize = 3;
const SHAPE_KEYS: usize = 12;

/// TLC-FR-14: the **shape** of the arguments a model composed — the fields it
/// sent and the kind of value each holds — and never a value.
///
/// A call refused for arguments that do not decode is otherwise unfollowable.
/// The tool never runs, so it logs nothing of its own; the loop can say only
/// that the shape was wrong, which leaves the one question a reader has — wrong
/// *how* — unanswered, and the same call fails the same way for as long as
/// nobody can see what the model sent.
///
/// Nothing downstream redacts anything, so this carries no value of any kind: a
/// string is `string` whatever it holds, and a number is `number`. What it does
/// carry is field **names**, and a name reaches a record only where `schema` —
/// the tool's own parameter schema, which is fixed text compiled into the
/// binary (TLC-FR-05) — declares it. Every name in a record is therefore a name
/// this application wrote, and a key the model composed is `?`.
///
/// The rule is the schema rather than the spelling of the key, because the
/// values this application must never write down are spelled exactly as a field
/// name is: a commit, an identifier, a login, and a path are all letters and
/// digits within any length a field name keeps, so a rule that reads the key
/// admits every one of them. A key nothing declares is worth exactly one mark —
/// it says the model invented a field — and repeating it buys a reader nothing
/// that the mark does not already tell them.
///
/// An object wider than [`SHAPE_KEYS`] is read in the order a JSON object holds
/// its fields, which is by name, so a very wide argument set is reported by its
/// first fields by name rather than by the order the model wrote them.
pub fn argument_shape(value: &serde_json::Value, schema: &serde_json::Value) -> String {
    let mut declared = std::collections::BTreeSet::new();
    collect_declared(schema, &mut declared);
    let mut out = String::new();
    write_shape(value, SHAPE_DEPTH, &declared, &mut out);
    bounded_argument(&out, SHAPE_CHARS)
}

/// Every property name a JSON Schema declares, at any depth.
///
/// Read from the schema itself rather than from a list beside it, so a tool that
/// gains a parameter gains it here as well and a reader is never told a field the
/// tool does accept was invented.
fn collect_declared(schema: &serde_json::Value, into: &mut std::collections::BTreeSet<String>) {
    match schema {
        serde_json::Value::Object(fields) => {
            if let Some(serde_json::Value::Object(properties)) = fields.get("properties") {
                for (name, held) in properties {
                    into.insert(name.clone());
                    collect_declared(held, into);
                }
            }
            for key in ["items", "additionalProperties"] {
                if let Some(held) = fields.get(key) {
                    collect_declared(held, into);
                }
            }
            for key in ["oneOf", "anyOf", "allOf"] {
                if let Some(serde_json::Value::Array(branches)) = fields.get(key) {
                    for branch in branches {
                        collect_declared(branch, into);
                    }
                }
            }
        }
        serde_json::Value::Array(branches) => {
            for branch in branches {
                collect_declared(branch, into);
            }
        }
        _ => {}
    }
}

fn write_shape(
    value: &serde_json::Value,
    depth: usize,
    declared: &std::collections::BTreeSet<String>,
    out: &mut String,
) {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(_) => out.push_str("bool"),
        serde_json::Value::Number(_) => out.push_str("number"),
        serde_json::Value::String(_) => out.push_str("string"),
        serde_json::Value::Array(items) => {
            // The length and the shape of the first entry: a list the model
            // filled wrongly is wrong in the same way all the way down, and one
            // entry is what names the mistake.
            out.push('[');
            match items.first() {
                Some(_) if depth == 0 => out.push('…'),
                Some(first) => write_shape(first, depth - 1, declared, out),
                None => {}
            }
            out.push_str(&format!("]({})", items.len()));
        }
        serde_json::Value::Object(fields) => {
            if depth == 0 {
                out.push_str("{…}");
                return;
            }
            out.push('{');
            for (index, (key, held)) in fields.iter().take(SHAPE_KEYS).enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(if declared.contains(key) { key } else { "?" });
                out.push(':');
                write_shape(held, depth - 1, declared, out);
            }
            if fields.len() > SHAPE_KEYS {
                out.push_str(",…");
            }
            out.push('}');
        }
    }
}

/// TLC-FR-14: one `INFO` record naming the tool and what it returned.
///
/// `extra` carries the shape of the answer — a count — and never a value drawn
/// from it: a tool's results are project material, and nothing downstream
/// redacts anything, so a record reaches the Logs panel, the clipboard, and any
/// exported file verbatim. An argument reaches `extra` only where the tool's own
/// specification names it as loggable, through [`bounded_argument`].
pub fn log_tool_success<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    mut extra: Fields,
) {
    extra.insert("tool".to_string(), serde_json::json!(tool));
    logging::log_info(sink, buffer, &TOOL_DOMAINS, "tool call completed", extra);
}

/// TLC-FR-14: one `WARN` record naming the tool and why it refused.
///
/// The reason is [`ToolRefusal::reason`]'s stable code rather than the message,
/// so the record neither varies with the sentence nor risks carrying anything
/// the model sent.
pub fn log_tool_refusal<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    refusal: &ToolRefusal,
) {
    log_tool_refusal_with(sink, buffer, tool, refusal, Fields::new());
}

/// The same record, carrying the fields a tool's own spec asks it to name.
///
/// The default is [`log_tool_refusal`]'s three fields and nothing else; a tool
/// reaches for this one only where its spec says a refusal is not followable
/// without something more — `read_file`'s path (RFT-FR-20) being the case this
/// exists for. The three fields of the record proper are written first and
/// `extra` fills in around them, so a tool naming one of them cannot displace
/// what the convention says that field carries.
pub fn log_tool_refusal_with<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    refusal: &ToolRefusal,
    extra: Fields,
) {
    let mut fields = log_fields! {
        "tool" => tool,
        "reason" => refusal.reason(),
        "retryable" => refusal.retryable(),
    };
    for (key, value) in extra {
        fields.entry(key).or_insert(value);
    }
    logging::log_warn(sink, buffer, &TOOL_DOMAINS, "tool call refused", fields);
}

// ---------------------------------------------------------------------------
// Shared resolution (TLC-FR-13)
// ---------------------------------------------------------------------------

/// The mounted indexes, or the shared refusal when no project is open.
///
/// A project is open exactly when the indexer has a content root mounted: a
/// close and a worktree change both clear it (BMI-FR-25, DSL-FR-22). The
/// distinction matters because `DSL-dynamic-skills-loading.md` answers an empty
/// list in that circumstance (DSL-FR-15) and a model told "no skills" would
/// conclude the project offers none and stop looking (SST-FR-07, SLT-FR-11).
pub fn require_open_project<'a, R: tauri::Runtime>(
    app: &'a tauri::AppHandle<R>,
) -> Result<tauri::State<'a, crate::bm25_index::Bm25Indexer>, ToolRefusal> {
    // `try_state` rather than `state`: a mock app in another module's test
    // manages only the stores that test needs, and a panic here would be a far
    // worse answer than the refusal this returns anyway.
    let indexer = app
        .try_state::<crate::bm25_index::Bm25Indexer>()
        .ok_or(ToolRefusal::NoProjectOpen)?;
    if indexer.root().is_none() {
        return Err(ToolRefusal::NoProjectOpen);
    }
    Ok(indexer)
}

/// The open project's active worktree, governed by the agent session's own
/// `FsAccess`, or the shared refusal (TLC-FR-13).
///
/// The root is the mounted content root [`require_open_project`] answers from,
/// which is what `read_file` resolves its own paths against (RFT-FR-03): one
/// notion of "the project is open" across the group, so no tool here can answer
/// for a root another would refuse. A close and a worktree change both clear it
/// (BMI-FR-25, WTC-FR-08).
///
/// The instance behind it is that agent session's (FSA-FR-29) and never a fresh
/// one: production shares the instance built at project open (FSA-FR-21), and a
/// tool minting its own allowlist would be a tool opting out of the gate. It is
/// resolved per call rather than held, for the reason `read_file` resolves its
/// own per call — a tool holding an `Arc` would keep read reach into a checkout
/// the application has stopped showing. That is also what makes the two draft
/// tools stateless in the sense TLC-FR-15 means: nothing about a project is
/// captured at construction.
pub fn require_project_root<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &str,
) -> Result<crate::fs::RootFs, ToolRefusal> {
    let root = require_open_project(app)?
        .root()
        .ok_or(ToolRefusal::NoProjectOpen)?;
    // `try_state`, not `state`: a mock app in another module's test manages only
    // what that test needs, and a panic here would be a far worse answer than
    // the refusal this returns anyway.
    let access = app
        .try_state::<crate::fs::FsAccessState>()
        .ok_or(ToolRefusal::NoProjectOpen)?
        .agent_session(session)
        .ok_or(ToolRefusal::NoProjectOpen)?;
    Ok(crate::fs::RootFs::new(root, access))
}
