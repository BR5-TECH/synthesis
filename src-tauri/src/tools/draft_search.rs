//! Draft search tool (`DST-draft-search-tool.md`).
//!
//! The tool an agent reaches for when it needs to know what this project is
//! *planning* rather than what it has already decided. It ranks the drafts of
//! the active worktree against the agent's own description of a topic and hands
//! back the best few by draft id, each carrying the passage inside it that
//! matched.
//!
//! ## Why only the live prompt
//!
//! A draft's accepted history is review provenance — the versions the author
//! settled and moved on from (`DHS-draft-history.md`) — and an agent that
//! recommended on the strength of a superseded version would be recommending
//! against the draft's own past. The exclusion is not enforced here: the
//! `drafts` index holds live prompts and nothing else (BMI-FR-04), so there is
//! no query this tool could compose that would reach a snapshot (DST-FR-04).
//!
//! ## Why the record is resolved and not carried on the hit
//!
//! A hit knows the draft's id and the text that matched, and nothing else that
//! is still true. Name and status are read from `draft.toml` per surviving hit
//! (DST-FR-12), so a draft renamed or archived since the last index pass is
//! reported as it now stands rather than as the pass found it. The same
//! resolution is what drops a hit whose draft has since been deleted or made
//! inconsistent (DST-FR-13) — every row this tool returns is one `read_draft`
//! can load.
//!
//! ## Why `limit` counts drafts and not chunks
//!
//! `BMI-bm25-indexing.md` cuts a Markdown file at every heading (BMI-FR-05), so
//! a prompt that matches well occupies several of the top slots on its own. A
//! limit spent on chunks would routinely return two drafts where the model asked
//! for ten, so this tool asks the index for more chunks than it needs and
//! deduplicates by draft id (DST-FR-08). It does that once and never escalates —
//! `TLC-tool-conventions.md` puts a retry loop out of bounds for every tool in
//! this group.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_open_project,
    require_project_root, ToolRefusal, EMPTY_DRAFT_QUERY,
};
use crate::bm25_index::{self, Bm25Indexer, IndexId};
use crate::drafts::DraftStatus;
use crate::log_fields;
use crate::logging::{Fields, LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (DST contract surface)
// ---------------------------------------------------------------------------

/// DST-FR-01.
///
/// Shares a spelling with `crate::drafts`' Tauri command of the same name
/// (DRS-FR-17), which is the Drafts panel's substring filter. The two are
/// different operations on different surfaces and neither is reachable from the
/// other; TLC-FR-02 requires a tool's name to be unique within this group, which
/// this is.
pub const NAME: &str = "search_drafts";

/// DST-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Find planned drafts relevant to a topic, ranked by how well each current live prompt matches what you describe. Use this to discover related draft work when you do not already know its draft ID. Results are best first and include the draft ID to pass unchanged to `read_draft`, its name and status, an informational draft-relative prompt path, a matching excerpt, and a score that orders this result set. Only current live prompts are searched: accepted history and other snapshots are never searched. This does not read a complete prompt, modify a draft, or make its path usable with `read_file`. An empty result means no current readable draft matched.";

const QUERY_DESCRIPTION: &str = "A plain description of the planned work or topic to find in current draft prompts. Do not supply a file path.";

const LIMIT_DESCRIPTION: &str = "How many distinct drafts to return, best match first. Defaults to 10. Values below 1 or above 20 are clamped into that range.";

/// DST-FR-06. Stated in [`LIMIT_DESCRIPTION`] as TLC-FR-06 requires.
///
/// Twice `SPS-specification-search-tool.md`'s default and the same ceiling: a
/// match there carries a whole heading section of a specification, while a
/// match here carries one chunk of a prompt an author is still writing, so ten
/// of these is a comparable read to five of those.
pub const DEFAULT_LIMIT: i64 = 10;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 20;

/// DST-FR-08: how many chunks are asked for per draft wanted.
///
/// An implementation choice, as the spec's non-functional section says — the
/// contract is only that one bounded call is made and that `limit` counts
/// drafts. Ten matches what `search_specifications` asks for and holds for the
/// same reason: a draft prompt is a Markdown document cut at its headings, so a
/// prompt that dominates the ranking occupies several slots and the overfetch is
/// what keeps a ten-draft result from collapsing to two.
const OVERFETCH_FACTOR: usize = 10;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). `limit` is `i64`
/// so a model's `-5` decodes and is clamped (DST-FR-06) instead of failing to
/// deserialize before this tool ever sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct DraftSearchArgs {
    pub query: String,
    #[serde(default, deserialize_with = "lenient_limit")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
///
/// The same forgiveness `SPS-specification-search-tool.md` applies, and for the
/// same reason: losing a perfectly good `query` over the shape of an optional
/// parameter is the opposite of what TLC-FR-07 asks for.
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

/// One ranked draft (DST-FR-10).
///
/// Exactly `draft_id`, `name`, `status`, `prompt_path`, `excerpt`, and `score`.
/// The `index` the underlying hit carries is constant across every match this
/// tool can return, its `path` is the record's own `prompt_path` said twice, and
/// a model can act on neither `node_id` nor `chunk_ordinal`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftSearchMatch {
    pub draft_id: String,
    pub name: String,
    pub status: DraftStatus,
    /// DST-FR-11: draft-relative and informational. Not a path `read_file`
    /// resolves to this prompt — what the model carries forward is `draft_id`.
    pub prompt_path: String,
    pub excerpt: String,
    pub score: f32,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftSearchOutput {
    pub drafts: Vec<DraftSearchMatch>,
}

/// TLC-FR-06: the JSON Schema for [`DraftSearchArgs`].
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

/// DST-FR-06: the documented default, clamped into the documented range.
///
/// Never passes zero downward, so an empty result always means nothing matched
/// rather than that nothing was asked for.
pub fn normalize_limit(limit: Option<i64>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

/// DST-FR-03, DST-FR-05: answer from `BMI-bm25-indexing.md`'s `search` against
/// the `drafts` index alone, and return no draft that call did not produce.
///
/// Enumerates no drafts folder, reads no prompt, holds no index, and applies no
/// ranking of its own — the ordering below is the one the delegated call already
/// established (DST-FR-09).
pub fn search(
    indexer: &Bm25Indexer,
    root: &crate::fs::RootFs,
    args: &DraftSearchArgs,
) -> Result<DraftSearchOutput, ToolRefusal> {
    if args.query.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(EMPTY_DRAFT_QUERY));
    }
    let limit = normalize_limit(args.limit);
    Ok(DraftSearchOutput {
        drafts: resolve_matches(
            // DST-FR-05: the `drafts` index alone. Every other index — specs,
            // skills, flows, and the rest — is unreachable through this tool
            // however well its text matches.
            bm25_index::search(
                indexer,
                &[IndexId::Drafts],
                &args.query,
                limit.saturating_mul(OVERFETCH_FACTOR),
            ),
            root,
            limit,
        ),
    })
}

/// DST-FR-07, DST-FR-12, DST-FR-13: one match per draft, resolved against the
/// draft's current record, dropped where it no longer resolves.
///
/// The delegated call orders by descending score (BMI-FR-11), so the first chunk
/// of a given draft to arrive *is* its best one and keeping the first is keeping
/// the better match (DST-FR-07). Deduplication happens **before** resolution so
/// a prompt occupying six of the top slots costs one record read rather than
/// six, and the walk stops as soon as `limit` drafts have survived — which is
/// what keeps the per-hit resolution the spec's non-functional section describes
/// bounded by `limit` rather than by how many chunks came back.
///
/// A hit whose draft has since been deleted, or whose `files/` is no longer the
/// single prompt DRS-FR-11 requires, is skipped rather than returned: a row the
/// model could not then load is worse than no row at all (DST-FR-13). The result
/// inherits the delegated ordering (DST-FR-09) and carries fewer than `limit`
/// only when the chunks asked for came from fewer distinct drafts or when a hit
/// was dropped here.
fn resolve_matches(
    hits: Vec<bm25_index::ChunkHit>,
    root: &crate::fs::RootFs,
    limit: usize,
) -> Vec<DraftSearchMatch> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut matches: Vec<DraftSearchMatch> = Vec::new();
    for hit in hits {
        if matches.len() == limit {
            break;
        }
        // BMI-FR-04 carries `draft_id` on every hit of this index; a hit without
        // one did not come from the drafts index and names no draft to resolve.
        let Some(draft_id) = hit.draft_id.clone() else {
            continue;
        };
        if !seen.insert(draft_id.clone()) {
            continue;
        }
        // DST-FR-12, DST-FR-13: the draft as it now stands, through the storage
        // module's own resolution (DRS-FR-38) rather than from a path composed
        // here. An error is a draft that is gone or inconsistent, and either way
        // this row is dropped.
        let Ok(record) = crate::drafts::draft_record(root, &draft_id) else {
            continue;
        };
        // `draft_record` succeeds only for a draft holding its one prompt
        // (DRS-FR-38), so the record's pointer is present and is the file that
        // was found; the fallback keeps this total rather than panicking.
        let Some(prompt_path) = record.prompt_path else {
            continue;
        };
        matches.push(DraftSearchMatch {
            draft_id: record.id,
            name: record.name,
            status: record.status,
            prompt_path,
            // DST-FR-14: the chunk exactly as the index supplied it. Not
            // truncated, not summarised, not re-wrapped, not annotated.
            excerpt: hit.text,
            score: hit.score,
        });
    }
    matches
}

// ---------------------------------------------------------------------------
// Logging (DST-FR-21)
// ---------------------------------------------------------------------------

/// DST-FR-21: how long a `query` may be before a record carries only its head.
///
/// The same bound the other loggable string arguments in this group take, and
/// for the same reason — see [`bounded_argument`].
const LOGGED_QUERY_LIMIT: usize = 512;

/// DST-FR-21: the arguments this tool's spec names as loggable, on both the
/// success record and the refusal one.
///
/// The `query` is what makes a record followable: a turn asks the same topic
/// several ways, and "returned 3 drafts" says nothing about which phrasing
/// produced which answer. It is recorded as the model composed it — untrimmed,
/// so the blank query behind an `invalid_arguments` refusal is visible as the
/// blank it was — and bounded.
///
/// The `limit` is recorded only when the model sent one and a number could be
/// made of it: its absence is itself the fact worth reading, saying that the ten
/// matches came from the default rather than from a number the model chose. The
/// value is as sent rather than as clamped, because a `1000` is a thing worth
/// seeing in a log; what it became travels beside it in `limitApplied` on the
/// success record.
///
/// **Nothing returned is recorded** — not a draft id, not a name, not a status,
/// not a prompt path, not a score, and no part of any excerpt. A draft's prompt
/// is the author's own unpublished material and an excerpt of it is a quotation
/// of that material, which nothing downstream would redact.
fn logged_arguments(args: &DraftSearchArgs) -> Fields {
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

/// DST-FR-01: `search_drafts` as a `rig` portable tool.
///
/// Holds the app handle rather than the indexes or a root, so a project opened
/// after the tool was constructed is visible to it and one closed since is not
/// (TLC-FR-15). `session` names the agent session whose `FsAccess` the record
/// reads run through (FSA-FR-29), bound here for the reason `read_file` binds
/// its own: an agent's whole filesystem reach is one instance, discarded with
/// the turn.
pub struct DraftSearchTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> DraftSearchTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>, session: impl Into<String>) -> Self {
        DraftSearchTool {
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
        DraftSearchTool {
            app,
            session: session.into(),
            buffer,
        }
    }

    /// The whole call, with its logging (TLC-FR-14, DST-FR-21).
    fn run(&self, args: DraftSearchArgs) -> Result<DraftSearchOutput, ToolRefusal> {
        // DST-FR-16: with no project open, the shared refusal rather than the
        // empty list `bm25_index::search` answers with — a model told "no draft
        // matched" would conclude the project has none and stop looking.
        let outcome = require_open_project(&self.app).and_then(|indexer| {
            let root = require_project_root(&self.app, &self.session)?;
            search(&indexer, &root, &args)
        });
        match &outcome {
            Ok(output) => {
                let mut fields = log_fields! {
                    "drafts" => output.drafts.len(),
                    // What DST-FR-06 made of the argument beside it, so a
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

impl<R: tauri::Runtime> rig::tool::PortableTool for DraftSearchTool<R> {
    const NAME: &'static str = NAME;

    type Args = DraftSearchArgs;
    type Output = DraftSearchOutput;
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
        // DST-FR-18: an in-memory lookup against the published snapshot plus the
        // record reads it survived, so it resolves before the future is polled
        // and waits on no index pass.
        std::future::ready(self.run(arguments))
    }
}
