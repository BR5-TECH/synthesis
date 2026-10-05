//! Specification search tool (`SPS-specification-search-tool.md`).
//!
//! The tool an agent reaches for when it needs to know what this project has
//! already decided about a topic. It ranks the project's specifications against
//! the agent's own description of that topic and hands back the best few by
//! path, each carrying the passage inside it that matched.
//!
//! ## Why an excerpt rather than a path alone
//!
//! A project's specifications are its accumulated decisions, and an agent asked
//! to change behaviour three of them already constrain cannot find those three
//! by guessing filenames. The excerpt is what lets it tell from the result
//! whether a file is worth reading whole, so a search that named five files
//! costs one call rather than six (SPS-FR-12).
//!
//! ## Why `limit` counts files and not passages
//!
//! `BMI-bm25-indexing.md` cuts a Markdown file at every heading (BMI-FR-05), so
//! a specification that matches well occupies several of the top slots on its
//! own. A limit spent on passages would routinely return two files where the
//! model asked for five, so this tool asks the index for more passages than it
//! needs and deduplicates by path (SPS-FR-08). It does that once and never
//! escalates — `TLC-tool-conventions.md` puts a retry loop out of bounds for
//! every tool in this group.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    EMPTY_SPEC_QUERY,
};
use crate::bm25_index::{self, Bm25Indexer, IndexId};
use crate::log_fields;
use crate::logging::{Fields, LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (SPS contract surface)
// ---------------------------------------------------------------------------

/// SPS-FR-01.
pub const NAME: &str = "search_specifications";

/// SPS-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Find the specifications most relevant to a topic, ranked by how well each one's text matches what you describe. Use this when you need to know what this project has already decided about something — search with a plain description of the topic or the behaviour, not with a guessed filename. Returns the best matches first, each with the specification's path, the passage inside it that matched, and a score that orders the results. That passage is one section of the file rather than the whole of it, so read the file at that path when you need the rest. Each specification appears at most once, however many of its sections matched your query. Only specifications are searched — not source code, not skills, not notes — and nothing that is found is changed. Returns an empty list when nothing matches, which means this project has no specification bearing on that topic.";

const QUERY_DESCRIPTION: &str = "A plain description of the topic or behaviour you need this project's decisions about, in your own words. For example: 'how the file tree decides which files are artifacts'.";

const LIMIT_DESCRIPTION: &str = "How many specifications to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range.";

/// SPS-FR-06. Stated in [`LIMIT_DESCRIPTION`] as TLC-FR-06 requires.
///
/// Deliberately lower than `SST-skill-search-tool.md`'s: a skill match is a name
/// and a sentence, while a match here carries a whole heading section, so the
/// payload per result is an order of magnitude larger.
pub const DEFAULT_LIMIT: i64 = 5;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 20;

/// SPS-FR-08: how many passages are asked for per specification wanted.
///
/// An implementation choice, as the spec's non-functional section says — the
/// contract is only that one bounded call is made and that `limit` counts
/// specifications. Ten is chosen against what a spec in this corpus actually
/// looks like: the largest hold on the order of thirty heading sections, and
/// asking for ten passages per wanted file fills a five-file result even when
/// two files dominate the ranking.
const OVERFETCH_FACTOR: usize = 10;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). `limit` is `i64`
/// so a model's `-5` decodes and is clamped (SPS-FR-06) instead of failing to
/// deserialize before this tool ever sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct SpecificationSearchArgs {
    pub query: String,
    #[serde(default, deserialize_with = "lenient_limit")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
///
/// The same forgiveness `SST-skill-search-tool.md` applies, and for the same
/// reason: losing a perfectly good `query` over the shape of an optional
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

/// One ranked specification (SPS-FR-10).
///
/// Exactly `path`, `excerpt`, and `score`. The `index` the underlying hit
/// carries is constant across every match this tool can return, and a model
/// refers to a specification by path and can act on neither `node_id` nor
/// `chunk_ordinal`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpecificationMatch {
    pub path: String,
    pub excerpt: String,
    pub score: f32,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpecificationSearchOutput {
    pub specifications: Vec<SpecificationMatch>,
}

/// TLC-FR-06: the JSON Schema for [`SpecificationSearchArgs`].
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
// The call, as a pure function of the mounted indexes
// ---------------------------------------------------------------------------

/// SPS-FR-06: the documented default, clamped into the documented range.
///
/// Never passes zero downward, so an empty result always means nothing matched
/// rather than that nothing was asked for.
pub fn normalize_limit(limit: Option<i64>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

/// SPS-FR-03, SPS-FR-05: answer from `BMI-bm25-indexing.md`'s `search` against
/// the `spec` index alone, and return nothing that call did not produce.
///
/// Reads no file, enumerates no folder, holds no index, and applies no ranking
/// of its own — the ordering below is the one the delegated call already
/// established (SPS-FR-09).
pub fn search(
    indexer: &Bm25Indexer,
    args: &SpecificationSearchArgs,
) -> Result<SpecificationSearchOutput, ToolRefusal> {
    if args.query.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(EMPTY_SPEC_QUERY));
    }
    let limit = normalize_limit(args.limit);
    Ok(SpecificationSearchOutput {
        specifications: dedupe_by_path(
            // SPS-FR-05: the `spec` index alone. Every other index — skills,
            // drafts, flows, and the rest — is unreachable through this tool
            // however well its text matches.
            bm25_index::search(
                indexer,
                &[IndexId::Spec],
                &args.query,
                limit.saturating_mul(OVERFETCH_FACTOR),
            ),
            limit,
        ),
    })
}

/// SPS-FR-07: one match per specification, the highest-scoring passage standing
/// for the file.
///
/// The delegated call orders by descending score (BMI-FR-11), so the first
/// passage of a given path to arrive *is* its best one and keeping the first is
/// keeping the better match. The result inherits that order (SPS-FR-09), and
/// carries fewer than `limit` only when the passages asked for came from fewer
/// distinct files (SPS-FR-08).
fn dedupe_by_path(hits: Vec<bm25_index::ChunkHit>, limit: usize) -> Vec<SpecificationMatch> {
    let mut seen: HashSet<String> = HashSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(hit.path.clone()))
        .take(limit)
        .map(|hit| SpecificationMatch {
            path: hit.path,
            // SPS-FR-12: the passage exactly as the index supplied it. Not
            // truncated, not summarised, not re-wrapped, not annotated.
            excerpt: hit.text,
            score: hit.score,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Logging (SPS-FR-19)
// ---------------------------------------------------------------------------

/// SPS-FR-19: how long a `query` may be before a record carries only its head.
///
/// The same bound `read_file`'s `path` takes (RFT-FR-20), and for the same
/// reason — see [`bounded_argument`]. A query the model wrote as a plain
/// description of a topic is nowhere near it, so the bound costs a real call
/// nothing and only clips the pathological one.
const LOGGED_QUERY_LIMIT: usize = 512;

/// SPS-FR-19: the arguments this tool's spec names as loggable, on both the
/// success record and the refusal one.
///
/// The `query` is what makes a record followable: a turn asks the same topic
/// several ways, and "returned 3 matches" says nothing about which phrasing
/// produced which answer. It is recorded as the model composed it — untrimmed,
/// so the blank query behind an `invalid_arguments` refusal is visible as the
/// blank it was — and bounded (SPS-FR-19).
///
/// The `limit` is recorded only when the model sent one: its absence is itself
/// the fact worth reading, saying that the five matches came from the default
/// rather than from a number the model chose. The value is as sent rather than
/// as clamped, because a `1000` is a thing worth seeing in a log; what it became
/// travels beside it in `limitApplied` on the success record.
fn logged_arguments(args: &SpecificationSearchArgs) -> Fields {
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

/// SPS-FR-01: `search_specifications` as a `rig` portable tool.
///
/// Holds the app handle rather than the indexes themselves, so a project opened
/// after the tool was constructed is visible to it (TLC-FR-15).
pub struct SpecSearchTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> SpecSearchTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        SpecSearchTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SpecSearchTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14, SPS-FR-19).
    fn run(
        &self,
        args: SpecificationSearchArgs,
    ) -> Result<SpecificationSearchOutput, ToolRefusal> {
        let outcome = require_open_project(&self.app).and_then(|indexer| search(&indexer, &args));
        match &outcome {
            // SPS-FR-19: what was asked and what came back. The arguments are
            // the two this tool's spec names as loggable (per TLC-FR-14); no
            // returned path, excerpt, or score is recorded, those being the
            // project's own material.
            Ok(output) => {
                let mut fields = log_fields! {
                    "matches" => output.specifications.len(),
                    // What SPS-FR-06 made of the argument beside it, so a
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

impl<R: tauri::Runtime> rig::tool::PortableTool for SpecSearchTool<R> {
    const NAME: &'static str = NAME;

    type Args = SpecificationSearchArgs;
    type Output = SpecificationSearchOutput;
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
        // SPS-FR-16: an in-memory lookup against the published snapshot, so it
        // resolves before the future is polled and waits on no index pass.
        std::future::ready(self.run(arguments))
    }
}
