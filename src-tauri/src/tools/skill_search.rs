//! Skill search tool (`SST-skill-search-tool.md`).
//!
//! The tool an agent reaches for when it has been given a job and does not know
//! whether the project already provides a skill for it. It ranks every
//! invocable skill against the agent's own description of the task and hands
//! back the best few by name, description, and path, so the agent can read the
//! one it chose and follow it.
//!
//! ## Why descriptors rather than bodies
//!
//! An agent that must pick one skill out of forty cannot read forty bodies, and
//! a skill's description is the sentence it wrote to be chosen by. Ranking runs
//! against the `skills` index, whose documents are a name and a description and
//! nothing else (DSL-FR-13, SST-FR-04) — which is why the description tells the
//! model to describe its task rather than quote words it expects to find in the
//! skill.
//!
//! ## Why an empty query refuses instead of returning nothing
//!
//! An empty list is a real answer here: it means the project has no skill for
//! the task. Returning it for a query the model forgot to fill in would tell it
//! the opposite of the truth, so a blank query is a retryable refusal instead
//! (SST-FR-06).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal, log_tool_success, normalize_skill_name, require_open_project, ToolRefusal,
};
use crate::bm25_index::Bm25Indexer;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::skills::{self, Ecosystem};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (SST contract surface)
// ---------------------------------------------------------------------------

/// SST-FR-01. Shared with `DSL-dynamic-skills-loading.md`'s internal call of
/// the same name, which this tool exists to expose to a model.
pub const NAME: &str = "search_skills";

/// SST-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Find the skills most relevant to a task, ranked by how well each skill's own description matches what you are trying to do. Use this when you have a job to do and want to know whether this project already provides a skill for it — search with a plain description of the task, not with a guessed skill name. Returns the best matches first, each with the skill's name, the description it wrote about itself, the ecosystem it belongs to, and the path to its SKILL.md file; read that file to actually use the skill. Matching is over each skill's short self-description rather than its full text, so describe the task rather than quoting words you expect to find inside the skill. Only skills this project makes available to models are searched. Each name appears once: where two ecosystems provide a skill under the same name, you are shown whichever of them matched your query better. Returns an empty list when nothing matches, which means this project has no skill for that task.";

const QUERY_DESCRIPTION: &str = "A plain description of the task you are trying to accomplish, in your own words. For example: 'review a specification for internal contradictions'.";

const LIMIT_DESCRIPTION: &str = "How many skills to return, best match first. Defaults to 10. Values below 1 or above 25 are clamped into that range.";

/// SST-FR-06: what a model that sent no query is told.
pub const EMPTY_QUERY: &str = "The query must describe the task you are trying to accomplish. Call again with a short plain-language description of the job.";

/// SST-FR-05. Stated in [`LIMIT_DESCRIPTION`] as TLC-FR-06 requires, because a
/// model reads the description and does not reliably infer a bound from the
/// schema alone.
pub const DEFAULT_LIMIT: i64 = 10;
pub const MIN_LIMIT: i64 = 1;
pub const MAX_LIMIT: i64 = 25;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07) — serde's
/// default, and deliberately not `deny_unknown_fields`. `limit` is `i64` rather
/// than a `usize` so a model's `-5` decodes and is clamped (SST-FR-05) instead
/// of failing to deserialize before this tool ever sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillSearchArgs {
    pub query: String,
    #[serde(default, deserialize_with = "lenient_limit")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
///
/// A model asked for an integer routinely sends `"10"` or `10.0`, and the
/// strict decode would fail the whole call — losing a perfectly good `query`
/// over the shape of an optional parameter, which is the opposite of the
/// forgiveness TLC-FR-07 asks for. A value that is not a number in any
/// spelling, and an explicit `null`, both fall back to the documented default
/// rather than refusing.
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

/// One ranked skill (SST-FR-09).
///
/// Exactly the fields of a `SkillDescriptor` a model can act on, plus the
/// score. `folder_name` is omitted because `path` already contains it and a
/// model refers to a skill by name or by path; no part of the skill's body
/// appears here at all.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillMatch {
    pub name: String,
    pub description: String,
    pub path: String,
    pub ecosystem: Ecosystem,
    pub score: f32,
}

/// TLC-FR-08: a JSON object with the collection under a named field, so a later
/// addition extends the output rather than changing its shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillSearchOutput {
    pub skills: Vec<SkillMatch>,
}

/// TLC-FR-06: the JSON Schema for [`SkillSearchArgs`].
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

/// SST-FR-05: the documented default, clamped into the documented range.
///
/// The tool never passes zero downward, so an empty result always means nothing
/// matched rather than that nothing was asked for.
pub fn normalize_limit(limit: Option<i64>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT) as usize
}

/// SST-FR-03: answer by delegating to `DSL-dynamic-skills-loading.md`, and
/// return nothing that call did not produce.
///
/// Reads no file, enumerates no folder, holds no registry, consults no index
/// directly, and applies no ranking of its own.
pub fn search(indexer: &Bm25Indexer, args: &SkillSearchArgs) -> Result<SkillSearchOutput, ToolRefusal> {
    if args.query.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(EMPTY_QUERY));
    }
    let limit = normalize_limit(args.limit);
    let mut seen: HashSet<String> = HashSet::new();
    let skills = skills::search_skills(indexer, &args.query, limit)
        .into_iter()
        // SST-FR-12: a name appears at most once, and the delegated call
        // already ordering by descending score means the first arrival is the
        // higher-scoring one — so keeping the first *is* keeping the better
        // match. Deduplication runs over what that call returned for the
        // requested `limit` rather than over the whole registry, so a result
        // set holding a collision carries fewer matches than `limit` allowed
        // and nothing is drawn up from lower in the ranking to replace it.
        .filter(|ranked| seen.insert(normalize_skill_name(&ranked.skill.name)))
        .map(|ranked| SkillMatch {
            name: ranked.skill.name,
            description: ranked.skill.description,
            path: ranked.skill.path,
            ecosystem: ranked.skill.ecosystem,
            score: ranked.score,
        })
        .collect();
    Ok(SkillSearchOutput { skills })
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// SST-FR-01: `search_skills` as a `rig` portable tool.
///
/// Holds the app handle rather than the indexes themselves: the indexer is
/// Tauri-managed state whose lifetime is the application's, and resolving it
/// per call is also what makes a project opened after the tool was constructed
/// visible to it.
pub struct SkillSearchTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    /// Threaded rather than reached for as a global, exactly as `git.rs` does,
    /// so a test can assert on the records this tool emits without racing every
    /// other suite over the process-wide buffer.
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> SkillSearchTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name;
    /// there is no registry that would attach it on anyone's behalf.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        SkillSearchTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SkillSearchTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14). Split out so the
    /// `PortableTool` impl below is the trait plumbing and nothing else.
    fn run(&self, args: SkillSearchArgs) -> Result<SkillSearchOutput, ToolRefusal> {
        let outcome = require_open_project(&self.app).and_then(|indexer| search(&indexer, &args));
        match &outcome {
            Ok(output) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! { "matches" => output.skills.len() },
            ),
            Err(refusal) => log_tool_refusal(&self.app, self.buffer, NAME, refusal),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for SkillSearchTool<R> {
    const NAME: &'static str = NAME;

    type Args = SkillSearchArgs;
    type Output = SkillSearchOutput;
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
        // TLC-FR-16: the whole call is an in-memory lookup against the
        // published snapshot, so it resolves before the future is ever polled
        // and blocks on no background work.
        std::future::ready(self.run(arguments))
    }
}
