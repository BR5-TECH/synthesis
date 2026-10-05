//! Skill list tool (`SLT-skill-list-tool.md`).
//!
//! The tool an agent reaches for when it wants to know everything the project
//! makes available to it rather than the best match for one task: every
//! invocable skill, each with the description it wrote about itself and the
//! path to read it at.
//!
//! ## Why it is complete and uncapped
//!
//! The description promises that a name absent from the list names nothing
//! available to the agent, and a silently shortened list would make that
//! statement false (SLT-FR-07). The promise is affordable because a descriptor
//! is a name, a sentence or two, a path, and an ecosystem — which is also why
//! `SST-skill-search-tool.md` bounds its own result and this one does not.
//!
//! ## Why a name appears once
//!
//! Two ecosystems providing a `review` skill are, to a model surveying the set,
//! two entries that told it the same thing about themselves and no basis to
//! choose between them (SLT-FR-13). The first in path order stands for the name
//! here; nothing is lost by it, because `LSK-load-skill-tool.md` still loads the
//! other by name and ecosystem.

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
// The contract surface (SLT contract surface)
// ---------------------------------------------------------------------------

/// SLT-FR-01. Shared with `DSL-dynamic-skills-loading.md`'s internal call of
/// the same name, which this tool exists to expose to a model.
pub const NAME: &str = "list_skills";

/// SLT-FR-02: the fixed text the model reads, compiled into the binary.
///
/// It names `search_skills` outright so a model holding a specific task is
/// pointed at the ranked call rather than reading the whole set to find one
/// skill.
pub const DESCRIPTION: &str = "List every skill this project makes available to you, each with the description it wrote about itself. Use this to survey what the project can do, or when a search for a specific task came back with nothing useful and you want to see the whole set before concluding no skill applies. Each entry gives the skill's name, its description, the ecosystem it belongs to, and the path to its SKILL.md file; read that file to actually use the skill. Every skill name available to you appears exactly once, so a name that is not in this list names nothing you can use. This tool takes no parameters. When you already know what task you need a skill for, prefer `search_skills`, which ranks the same set against that task.";

/// The arguments: none (SLT-FR-03).
///
/// An empty struct rather than a unit struct so the tool decodes from `{}`, and
/// deliberately not `deny_unknown_fields` so a model that invented a plausible
/// filter is still answered (SLT-FR-04) rather than spending a turn on a
/// refusal that would teach it nothing the description does not already say.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillListArgs {}

/// One skill (SLT-FR-09). No score — this tool ranks nothing — and no part of
/// the skill's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: String,
    pub ecosystem: Ecosystem,
}

/// TLC-FR-08: a JSON object with the collection under a named field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillListOutput {
    pub skills: Vec<SkillEntry>,
}

/// TLC-FR-06 / SLT-FR-03: an object schema declaring no properties and
/// requiring none, which is what tells a model to call with `{}` rather than to
/// invent an argument.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "required": [],
    })
}

// ---------------------------------------------------------------------------
// The call, as a pure function of the mounted indexes
// ---------------------------------------------------------------------------

/// SLT-FR-05: answer by delegating to `DSL-dynamic-skills-loading.md`, and
/// return nothing that call did not produce.
///
/// The ascending-path order is that call's (DSL-FR-15) and is preserved here,
/// so two calls against an unchanged project agree and a model can refer back
/// to what it saw (SLT-FR-08).
///
/// SLT-FR-13: a name appears at most once. Keeping the *first* occurrence in an
/// already path-sorted list is the whole rule — which prefers `.claude` over
/// `.codex`, `.codex` over `.github`, and `.github` over `.opencode`, those
/// folder names sorting that way. Entries are removed from the order without
/// reordering what remains (SLT-FR-08).
pub fn list(indexer: &Bm25Indexer) -> SkillListOutput {
    let mut seen: HashSet<String> = HashSet::new();
    let skills = skills::list_skills(indexer)
        .into_iter()
        .filter(|skill| seen.insert(normalize_skill_name(&skill.name)))
        .map(|skill| SkillEntry {
            name: skill.name,
            description: skill.description,
            path: skill.path,
            ecosystem: skill.ecosystem,
        })
        .collect();
    SkillListOutput { skills }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// SLT-FR-01: `list_skills` as a `rig` portable tool.
pub struct SkillListTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    /// Threaded rather than reached for as a global, so a test can assert on
    /// the records this tool emits without racing the process-wide buffer.
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> SkillListTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name;
    /// there is no registry that would attach it on anyone's behalf.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        SkillListTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SkillListTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14).
    fn run(&self) -> Result<SkillListOutput, ToolRefusal> {
        let outcome = require_open_project(&self.app).map(|indexer| list(&indexer));
        match &outcome {
            Ok(output) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! { "skills" => output.skills.len() },
            ),
            Err(refusal) => log_tool_refusal(&self.app, self.buffer, NAME, refusal),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for SkillListTool<R> {
    const NAME: &'static str = NAME;

    type Args = SkillListArgs;
    type Output = SkillListOutput;
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
        _arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        // TLC-FR-16: an in-memory read of the published registry, resolved
        // before the future is ever polled.
        std::future::ready(self.run())
    }
}
