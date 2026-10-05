//! Search documents tool (`SDT-search-documents-tool.md`).
//!
//! The tool an agent reaches for when it needs to know which of the user's
//! reference documents bear on a topic. A reference document is a PDF, Markdown,
//! or text file the user selected for the project, and it may live outside the
//! project checkout. The tool ranks the `documents` index against the agent's
//! own description of a topic and returns each match with a stable document id
//! and enough identifying text to choose between matches.
//!
//! ## Why a match carries an id and no path
//!
//! A model that held a path could compose another one. The id is resolved
//! through the Documents collection at the moment of the call, so the only way
//! to read a selected document is `get_document` with an id the collection
//! holds (SDT-FR-ZWJW, SDT-FR-ONOF). The index holds no path either.
//!
//! ## Why the call asks for more chunks than documents
//!
//! A document holds several chunks in the index, and one document can fill many
//! of the top slots. `limit` counts documents, so the one call asks for a
//! multiple of it and the walk keeps the best chunk of each document
//! (SDT-FR-EMCC). The call is never repeated or widened (SDT-FR-BTOJ).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    EMPTY_DOCUMENT_QUERY,
};
use crate::bm25_index::{self, Bm25Indexer, IndexId};
use crate::documents::{Availability, DocumentsCollection};
use crate::log_fields;
use crate::logging::{Fields, LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (SDT contract surface)
// ---------------------------------------------------------------------------

/// SDT-FR-LNNL.
pub const NAME: &str = "search_documents";

/// SDT-FR-JMSA: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Search for Documents: find the reference documents the user selected for this project that are most relevant to a topic, ranked by how well each document's text matches what you describe. Use this to discover what the user's PDF, Markdown, and text references say about a subject, wherever those files live on the machine. Returns the best matches first, each with the document's id, its name, the folder it sits in, its format, a short passage that matched, and a score that orders this result set. Each document appears at most once. Pass the id to Get Document to read the document. Only the user's selected documents are searched — not specifications, not drafts, not notes, not project files. Nothing that is found is changed. Returns an empty list when nothing matches, which means no selected document bears on that topic.";

const QUERY_DESCRIPTION: &str = "A plain description of the topic you want reference documents about, in your own words. For example: 'how the vendor API rate limits requests'.";

const LIMIT_DESCRIPTION: &str = "How many documents to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range.";

/// SDT-FR-BTOJ. Stated in [`LIMIT_DESCRIPTION`] as TLC-FR-06 requires.
pub const DEFAULT_LIMIT: i64 = 5;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 20;

/// SDT-FR-EMCC: how many chunks are asked for per document wanted, so one
/// document that fills the top slots does not collapse the result set.
const OVERFETCH_FACTOR: usize = 10;

/// SDT-FR-PJKE: the most characters a snippet carries, ellipsis included.
pub const SNIPPET_LIMIT: usize = 600;

/// How long a `query` may be before a record carries only its head
/// (SDT-FR-KRWA).
const LOGGED_QUERY_LIMIT: usize = 512;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). `limit` is `i64`
/// so a model's `-5` decodes and is clamped (SDT-FR-BTOJ) instead of failing to
/// deserialize before this tool sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchDocumentsArgs {
    pub query: String,
    #[serde(default, deserialize_with = "lenient_limit")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
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

/// One ranked document (SDT-FR-ZWJW).
///
/// Exactly `id`, `name`, `folder`, `format`, `snippet`, and `score`. It carries
/// no filesystem path, no source path, and no chunk ordinal.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentMatch {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub format: String,
    pub snippet: String,
    pub score: f32,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchDocumentsOutput {
    pub documents: Vec<DocumentMatch>,
}

/// TLC-FR-06: the JSON Schema for [`SearchDocumentsArgs`].
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

/// SDT-FR-BTOJ: the documented default, clamped into the documented range.
///
/// Never passes zero downward, so an empty result always means nothing matched
/// rather than that nothing was asked for.
pub fn normalize_limit(limit: Option<i64>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

/// SDT-FR-PJKE: the chunk cut at a character boundary to at most
/// [`SNIPPET_LIMIT`] characters, an ellipsis counted among them when it was cut.
pub fn snippet_of(text: &str) -> String {
    if text.chars().count() <= SNIPPET_LIMIT {
        return text.to_string();
    }
    let kept: String = text.chars().take(SNIPPET_LIMIT - 1).collect();
    format!("{kept}…")
}

/// SDT-FR-ONOF: the name of the folder that holds a document, from its
/// normalised path.
fn folder_of(path: &str) -> String {
    let mut parts = path.rsplit('/');
    parts.next();
    parts.next().unwrap_or_default().to_string()
}

/// SDT-FR-BKAW, SDT-FR-BTOJ: one call to `BMI-bm25-indexing.md`'s `search`
/// against the `documents` index alone, and no document that call did not
/// produce. The tool holds no index and applies no ranking of its own.
pub fn search(
    indexer: &Bm25Indexer,
    collection: &DocumentsCollection,
    args: &SearchDocumentsArgs,
) -> Result<SearchDocumentsOutput, ToolRefusal> {
    if args.query.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(EMPTY_DOCUMENT_QUERY));
    }
    let limit = normalize_limit(args.limit);
    let hits = bm25_index::search(
        indexer,
        &[IndexId::Documents],
        &args.query,
        limit.saturating_mul(OVERFETCH_FACTOR),
    );
    Ok(SearchDocumentsOutput {
        documents: resolve_matches(hits, collection, limit),
    })
}

/// SDT-FR-EMCC, SDT-FR-ONOF: one match per document, resolved against the
/// collection as it stands, dropped where it no longer resolves.
///
/// The delegated call orders by descending score (BMI-FR-11), so the first chunk
/// of a document to arrive is its best one and keeping the first keeps the
/// better match. A hit for a document that left the collection, or became
/// unavailable, is omitted without an error.
fn resolve_matches(
    hits: Vec<bm25_index::ChunkHit>,
    collection: &DocumentsCollection,
    limit: usize,
) -> Vec<DocumentMatch> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut matches = Vec::new();
    for hit in hits {
        if matches.len() == limit {
            break;
        }
        let Some(id) = hit.document_id.clone() else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let Ok(entry) = collection.resolve_document(&id) else {
            continue;
        };
        if entry.status != Availability::Available {
            continue;
        }
        matches.push(DocumentMatch {
            folder: folder_of(&entry.path),
            id: entry.id,
            name: entry.name,
            format: entry.format.as_str().to_string(),
            snippet: snippet_of(&hit.text),
            score: hit.score,
        });
    }
    matches
}

// ---------------------------------------------------------------------------
// Logging (SDT-FR-QSKI, SDT-FR-KRWA)
// ---------------------------------------------------------------------------

/// SDT-FR-QSKI, SDT-FR-KRWA: the arguments this tool's spec names as loggable,
/// on both the success record and the refusal one.
///
/// The `query` is recorded as the model composed it — untrimmed — and bounded.
/// The `limit` is recorded as sent, and only when a number could be made of it.
/// Nothing returned is recorded: no id, name, folder, snippet, or score, because
/// a document is the user's own material.
fn logged_arguments(args: &SearchDocumentsArgs) -> Fields {
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

/// SDT-FR-LNNL: `search_documents` as a `rig` portable tool.
///
/// Holds the app handle rather than the index or the collection, so a project
/// opened after the tool was constructed is visible to it and one closed since
/// is not (TLC-FR-15).
pub struct SearchDocumentsTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> SearchDocumentsTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        SearchDocumentsTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SearchDocumentsTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14, SDT-FR-QSKI).
    fn run(&self, args: SearchDocumentsArgs) -> Result<SearchDocumentsOutput, ToolRefusal> {
        // SDT-FR-GGYO: with no project open, the shared refusal rather than the
        // empty list the index answers with.
        let outcome = require_open_project(&self.app).and_then(|indexer| {
            let collection = self
                .app
                .try_state::<DocumentsCollection>()
                .filter(|collection| collection.is_open())
                .ok_or(ToolRefusal::NoProjectOpen)?;
            search(&indexer, &collection, &args)
        });
        match &outcome {
            Ok(output) => {
                let mut fields = log_fields! {
                    "documents" => output.documents.len(),
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

impl<R: tauri::Runtime> rig::tool::PortableTool for SearchDocumentsTool<R> {
    const NAME: &'static str = NAME;

    type Args = SearchDocumentsArgs;
    type Output = SearchDocumentsOutput;
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
        // SDT-FR-AMNB: one in-memory lookup and one collection lookup per hit,
        // so it resolves before the future is polled and waits on no pass.
        std::future::ready(self.run(arguments))
    }
}
