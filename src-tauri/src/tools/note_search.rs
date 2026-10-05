//! Note search tool (`NST-note-search-tool.md`).
//!
//! The tool an agent reaches for when it needs to know what the author has
//! written down for themselves about a topic. A note is where a small gap, a
//! follow-up, or a thing to be done eventually is recorded, and it is the one
//! place in the project where such an item lives until somebody picks it up.
//! This tool ranks the notes of the active worktree against the agent's own
//! description of a topic and hands each match back **whole**.
//!
//! ## Why a match is the whole note
//!
//! A note is bounded at 1 KiB of UTF-8 by notes storage itself (NTC-FR-23), so
//! returning the whole of one costs a model less than an excerpt plus the
//! second call that would fetch the rest (NST-FR-11). That is also why there is
//! no read-the-rest tool beside this one: there is no rest. The bound belongs to
//! the store rather than to this tool, which would return a longer note whole if
//! the store ever held one.
//!
//! ## Why `limit` needs no overfetch
//!
//! `search_drafts` asks the index for more chunks than it needs, because one
//! prompt occupies several of the top slots on its own. A note is **one whole
//! document** in the index rather than a set of chunks (BMI-FR-05, BMI-FR-28),
//! so the underlying call already counts notes and one call asking for exactly
//! the applied limit is the whole of what this tool does (NST-FR-08).
//!
//! ## Why the record is resolved and not carried on the hit
//!
//! A hit knows the note's id and the body the last pass captured. The scope and
//! the body are read back through `NTC-notes-storage.md`'s `note_record`
//! (NTC-FR-25) per surviving hit, so a note edited or moved since the last pass
//! is reported as it now stands (NST-FR-12). The same resolution is what drops a
//! hit whose note has since been deleted or become unreadable (NST-FR-13) — a
//! row the model could not act on is worse than no row at all.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_open_project,
    require_project_root, ToolRefusal, EMPTY_NOTE_QUERY,
};
use crate::bm25_index::{self, Bm25Indexer, IndexId};
use crate::log_fields;
use crate::logging::{Fields, LogBuffer, BUFFER};
use crate::notes::NoteScope;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (NST contract surface)
// ---------------------------------------------------------------------------

/// NST-FR-01.
pub const NAME: &str = "search_notes";

/// NST-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Find the notes most relevant to a topic, ranked by how well each note's text matches what you describe. Use this to discover what the author has written down for themselves — a gap, a follow-up, a small thing to be done eventually — because one of those items is often work the job in front of you would close. Returns the best matches first, each with the note's id, what the note is filed against, the whole of the note's text, and a score that orders this result set. A note is short, so the complete note is returned and there is nothing further to read for it. Each note appears at most once. Only notes are searched — not specifications, not drafts, not project files — and nothing that is found is read out of, changed, resolved, or deleted. Returns an empty list when nothing matches, which means this project holds no note bearing on that topic.";

const QUERY_DESCRIPTION: &str = "A plain description of the topic, the gap, or the follow-up you want the author's notes about, in your own words. For example: 'the project picker does not report a missing folder'.";

const LIMIT_DESCRIPTION: &str = "How many notes to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range.";

/// NST-FR-06. Stated in [`LIMIT_DESCRIPTION`] as TLC-FR-06 requires.
///
/// Half `search_drafts`' default and the same ceiling, and both are chosen
/// against the model's context: a match here carries a whole note rather than
/// one chunk of a longer document, and a note is bounded at 1 KiB, so twenty of
/// them is a bounded read whose worst case is known from the bound alone.
pub const DEFAULT_LIMIT: i64 = 5;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 20;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). `limit` is `i64`
/// so a model's `-5` decodes and is clamped (NST-FR-06) instead of failing to
/// deserialize before this tool ever sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteSearchArgs {
    pub query: String,
    #[serde(default, deserialize_with = "lenient_limit")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
///
/// The same forgiveness `SPS-specification-search-tool.md` and
/// `DST-draft-search-tool.md` apply, and for the same reason: losing a
/// perfectly good `query` over the shape of an optional parameter is the
/// opposite of what TLC-FR-07 asks for. Text no number can be made of leaves
/// `limit` absent, which is also what its log record then says (NST-FR-20).
fn lenient_limit<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|f| f.trunc() as i64)),
        serde_json::Value::String(text) => text
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| text.trim().parse::<f64>().ok().map(|f| f.trunc() as i64)),
        _ => None,
    })
}

/// One ranked note (NST-FR-10).
///
/// Exactly `id`, `scope`, `body`, and `score`. The `index` the underlying hit
/// also carries is constant across every match this tool can return; its `path`
/// names the note's own file inside `.synthesis/` and is no path a model may
/// read or compose with; and `node_id` and `chunk_ordinal` say nothing about a
/// document that is never split.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteMatch {
    pub id: String,
    /// NST-FR-12: the note's own `NoteScope` (NTC-FR-03), carried through
    /// unchanged and interpreted nowhere here — an entity scope carries the
    /// note's `entity_id` and its last-known `entity_path`, a project scope
    /// carries neither, and a scope shape notes storage later adds reaches
    /// these results without a change to this tool.
    pub scope: NoteScope,
    /// NST-FR-11: the complete stored body, never a matching fragment of one.
    pub body: String,
    pub score: f32,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteSearchOutput {
    pub notes: Vec<NoteMatch>,
}

/// TLC-FR-06: the JSON Schema for [`NoteSearchArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": QUERY_DESCRIPTION,
            },
            "limit": {
                "type": "integer",
                "description": LIMIT_DESCRIPTION,
            },
        },
        "required": ["query"],
    })
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// NST-FR-06: the documented default, clamped into the documented range.
///
/// Never passes zero downward, so an empty result always means nothing matched
/// rather than that nothing was asked for.
pub fn normalize_limit(limit: Option<i64>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

/// NST-FR-03, NST-FR-05: answer from `BMI-bm25-indexing.md`'s `search` against
/// the `notes` index alone, and return no note that call did not produce.
///
/// It reads no note file, enumerates no folder, holds no index of its own, and
/// applies no ranking of its own — the ordering below is the one the delegated
/// call already established (NST-FR-09).
pub fn search(
    indexer: &Bm25Indexer,
    root: &crate::fs::RootFs,
    args: &NoteSearchArgs,
) -> Result<NoteSearchOutput, ToolRefusal> {
    if args.query.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(EMPTY_NOTE_QUERY));
    }
    let limit = normalize_limit(args.limit);
    Ok(NoteSearchOutput {
        notes: resolve_matches(
            // NST-FR-05: the `notes` index alone. Every other index — specs,
            // drafts, skills, flows, and the rest — is unreachable through this
            // tool however well its text matches.
            //
            // NST-FR-08: one call, asking for the applied limit and nothing
            // more. A note is one whole document (BMI-FR-05, BMI-FR-28), so
            // this call already counts notes and there is nothing to overfetch
            // for; it is never escalated, repeated, or widened.
            bm25_index::search(indexer, &[IndexId::Notes], &args.query, limit),
            root,
            limit,
        ),
    })
}

/// NST-FR-07, NST-FR-12, NST-FR-13: one match per note, resolved against the
/// note's current record, dropped where it no longer resolves.
///
/// The delegated call orders by descending score (BMI-FR-11) and produces at
/// most one hit per note, so there is no highest-scoring passage to choose
/// between; a repeated id is dropped all the same rather than returned twice
/// (NST-FR-07).
///
/// A hit whose note has since been deleted, or whose stored record can no longer
/// be read (NTC-FR-14), is skipped rather than returned (NST-FR-13). The
/// omission is silent and costs the result set that row, so a call may return
/// fewer notes than the index held documents for — which is an answer rather
/// than a failure (NST-FR-16).
fn resolve_matches(
    hits: Vec<bm25_index::ChunkHit>,
    root: &crate::fs::RootFs,
    limit: usize,
) -> Vec<NoteMatch> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut matches: Vec<NoteMatch> = Vec::new();
    for hit in hits {
        if matches.len() == limit {
            break;
        }
        // BMI-FR-28 carries `note_id` on every hit of this index; a hit without
        // one did not come from the notes index and names no note to resolve.
        let Some(note_id) = hit.note_id.clone() else {
            continue;
        };
        if !seen.insert(note_id.clone()) {
            continue;
        }
        // NST-FR-12, NST-FR-13: the note as it now stands, through the storage
        // module's own resolution (NTC-FR-25) rather than from the indexed hit.
        // An error is a note that is gone or unreadable, and either way this row
        // is dropped.
        let Ok(note) = crate::notes::note_record(root, &note_id) else {
            continue;
        };
        matches.push(NoteMatch {
            id: note.id,
            scope: note.scope,
            // NST-FR-11: the stored body whole. Not truncated, not summarised,
            // not re-wrapped, not annotated, and not the text the indexed hit
            // carried.
            body: note.body,
            score: hit.score,
        });
    }
    matches
}

// ---------------------------------------------------------------------------
// Logging (NST-FR-20)
// ---------------------------------------------------------------------------

/// NST-FR-20: how long a `query` may be before a record carries only its head.
///
/// The same bound the other loggable string arguments in this group take, and
/// for the same reason — see [`bounded_argument`]: no query a model composes may
/// push a record past `LGC-logging.md`'s per-record ceiling (LGC-FR-08) and cost
/// it the tool and the reason it exists to carry.
const LOGGED_QUERY_LIMIT: usize = 512;

/// NST-FR-20: the arguments this tool's spec names as loggable, on both the
/// success record and the refusal one.
///
/// The `query` is what makes a record followable: a turn asks the same topic
/// several ways, and "returned 3 notes" says nothing about which phrasing
/// produced which answer. It is recorded as the model composed it — untrimmed,
/// so the blank query behind an `invalid_arguments` refusal is visible as the
/// blank it was — and bounded.
///
/// The `limit` is recorded only when the model sent one and a number could be
/// made of it: its absence is itself the fact worth reading, saying that the
/// five matches came from the default rather than from a number the model chose.
/// The value is as sent rather than as clamped, because a `1000` is a thing
/// worth seeing in a log; what it became travels beside it in `limitApplied` on
/// the success record.
///
/// **Nothing returned is recorded** — not a note id, not a scope, not an entity
/// path, not a score, and no part of any body. A note is what an author wrote to
/// themselves about their own project, and a body is that text verbatim, which
/// nothing downstream would redact.
fn logged_arguments(args: &NoteSearchArgs) -> Fields {
    let mut fields = log_fields! {
        "query" => bounded_argument(&args.query, LOGGED_QUERY_LIMIT),
    };
    if let Some(limit) = args.limit {
        fields.insert("limit".to_string(), serde_json::json!(limit));
    }
    fields
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// NST-FR-01: `search_notes` as a `rig` portable tool.
///
/// Holds the app handle rather than the indexes or a root, so a project opened
/// after the tool was constructed is visible to it and one closed since is not
/// (TLC-FR-15). `session` names the agent session whose `FsAccess` the record
/// reads run through (FSA-FR-29), bound here for the reason the draft tools bind
/// their own: an agent's whole filesystem reach is one instance, discarded with
/// the turn.
pub struct NoteSearchTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> NoteSearchTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>, session: impl Into<String>) -> Self {
        NoteSearchTool {
            app,
            session: session.into(),
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        session: impl Into<String>,
        buffer: &'static LogBuffer,
    ) -> Self {
        NoteSearchTool {
            app,
            session: session.into(),
            buffer,
        }
    }

    /// The whole call, with its logging (TLC-FR-14, NST-FR-20).
    fn run(&self, args: NoteSearchArgs) -> Result<NoteSearchOutput, ToolRefusal> {
        // NST-FR-15: with no project open, the shared refusal rather than the
        // empty list `bm25_index::search` answers with — a model told "no note
        // matched" would conclude the project has none and stop looking.
        let outcome = require_open_project(&self.app).and_then(|indexer| {
            let root = require_project_root(&self.app, &self.session)?;
            search(&indexer, &root, &args)
        });
        match &outcome {
            Ok(output) => {
                let mut fields = log_fields! {
                    "notes" => output.notes.len(),
                    // What NST-FR-06 made of the argument beside it, so a
                    // clamped limit is readable as the number that produced the
                    // result rather than as a contradiction of the count.
                    "limitApplied" => normalize_limit(args.limit),
                };
                fields.extend(logged_arguments(&args));
                log_tool_success(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => log_tool_refusal_with(
                &self.app,
                self.buffer,
                NAME,
                refusal,
                logged_arguments(&args),
            ),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for NoteSearchTool<R> {
    const NAME: &'static str = NAME;

    type Args = NoteSearchArgs;
    type Output = NoteSearchOutput;
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
        // NST-FR-17: an in-memory lookup against the published snapshot plus the
        // record reads it survived, so it resolves before the future is polled
        // and waits on no index pass.
        std::future::ready(self.run(arguments))
    }
}
