//! Skill load tool (`LSK-load-skill-tool.md`).
//!
//! The tool an agent reaches for once it has decided which skill to follow.
//! `SST-skill-search-tool.md` and `SLT-skill-list-tool.md` answer *which* skill,
//! with a name, a sentence, and a path; this one answers what comes next,
//! returning that skill's instructions in full so the agent can carry them out.
//!
//! ## Why the result is text rather than an object
//!
//! `rig` carries a tool result as one or more content blocks, and its blanket
//! `IntoToolOutput` sends a `String` to a text block and a struct to a JSON
//! block. A skill's instructions are a document, so text is the block that fits:
//! the same Markdown inside a JSON string field would reach the model with every
//! line break escaped (LSK-FR-12, TLC-FR-08).
//!
//! ## Why it cannot be steered at another file
//!
//! The only path this tool ever reads is one the registry produced (LSK-FR-03).
//! A model supplies a *name*, which selects a descriptor or selects nothing;
//! there is no argument through which a path reaches the filesystem, so path
//! traversal is not defended against here so much as unrepresentable.

use serde::Deserialize;

use super::{
    log_tool_refusal, log_tool_success, normalize_skill_name, require_open_project, ToolRefusal,
};
use crate::bm25_index::Bm25Indexer;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::skills::{self, Ecosystem, SkillDescriptor};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (LSK contract surface)
// ---------------------------------------------------------------------------

/// LSK-FR-01.
pub const NAME: &str = "load_skill";

/// LSK-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Read one skill's full instructions, so you can follow them. Use this once `search_skills` or `list_skills` has told you which skill you want and you are ready to carry out its procedure rather than to weigh whether it applies. Give the skill's name exactly as it was reported to you; add `ecosystem` only when you were shown two skills sharing a name and you mean a particular one. Returns the skill's instructions as plain text, without the short header block carrying the name and description you have already been given. This tool reads one skill and nothing else — it opens no other file in the project, and it does not carry out what the skill says, which is yours to do.";

const NAME_DESCRIPTION: &str = "The skill's name, exactly as `list_skills` or `search_skills` reported it. Leading and trailing spaces and differences in letter case are ignored.";

const ECOSYSTEM_DESCRIPTION: &str = "Which ecosystem's copy to load, when two skills share a name: one of `claude`, `codex`, `github`, or `opencode`. Leave it out unless you have been told the name is ambiguous.";

/// LSK-FR-06: what a model that named no skill is told.
pub const BLANK_NAME: &str = "The name must name a skill. Call `list_skills` to see which names this project offers, then call again with one of them.";

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07) — serde's
/// default, and deliberately not `deny_unknown_fields`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadSkillArgs {
    pub name: String,
    #[serde(default, deserialize_with = "lenient_ecosystem")]
    pub ecosystem: Option<String>,
}

/// TLC-FR-07 applied to an enumerated parameter.
///
/// A `null` and an absent key both mean "any ecosystem". A value of some other
/// JSON shape is neither: the model asked to be narrowed, and it named nothing
/// this project has. Decoding it to an empty string carries that through to
/// [`parse_ecosystem`], which matches none of the four, so it lands on the
/// unknown-skill refusal along with every other way of naming a skill that is
/// not there (LSK-FR-07) rather than failing the whole decode and losing the
/// perfectly good `name` beside it.
fn lenient_ecosystem<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text),
        _ => Some(String::new()),
    })
}

/// TLC-FR-06: the JSON Schema for [`LoadSkillArgs`].
///
/// The four values are declared as an `enum` as well as named in the
/// description: the schema is what a provider can constrain a model against,
/// and the description is what the model reads.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": NAME_DESCRIPTION,
            },
            "ecosystem": {
                "type": "string",
                "enum": ["claude", "codex", "github", "opencode"],
                "description": ECOSYSTEM_DESCRIPTION,
            },
        },
        "required": ["name"],
    })
}

/// The spelling of an ecosystem a model may send, or `None` for anything that
/// names none of the four (LSK-FR-07).
pub fn parse_ecosystem(value: &str) -> Option<Ecosystem> {
    let value = value.trim();
    [
        Ecosystem::Claude,
        Ecosystem::Codex,
        Ecosystem::Github,
        Ecosystem::Opencode,
    ]
    .into_iter()
    .find(|e| e.as_str().eq_ignore_ascii_case(value))
}

// ---------------------------------------------------------------------------
// Resolution (LSK-FR-05 through LSK-FR-10)
// ---------------------------------------------------------------------------

/// Pick the one descriptor a model's arguments name.
///
/// Pure over a registry slice, so every resolution rule is testable without a
/// filesystem or a Tauri runtime behind it.
pub fn resolve(
    skills: &[SkillDescriptor],
    args: &LoadSkillArgs,
) -> Result<SkillDescriptor, ToolRefusal> {
    // LSK-FR-06: guessing which skill was meant would answer a question the
    // model did not ask.
    let wanted = normalize_skill_name(&args.name);
    if wanted.is_empty() {
        return Err(ToolRefusal::InvalidArguments(BLANK_NAME));
    }

    // LSK-FR-05: matched on the normalization the whole group shares, so a
    // model retyping a name it was shown reaches the skill it meant.
    let by_name: Vec<&SkillDescriptor> = skills
        .iter()
        .filter(|s| normalize_skill_name(&s.name) == wanted)
        .collect();

    // LSK-FR-07: the filter narrows, and a value naming none of the four
    // narrows to nothing rather than being refused as malformed.
    let candidates: Vec<&SkillDescriptor> = match args.ecosystem.as_deref() {
        None => by_name,
        Some(requested) => match parse_ecosystem(requested) {
            Some(ecosystem) => by_name
                .into_iter()
                .filter(|s| s.ecosystem == ecosystem)
                .collect(),
            None => Vec::new(),
        },
    };

    match candidates.as_slice() {
        // LSK-FR-08: the ordinary outcome.
        [only] => Ok((*only).clone()),
        // LSK-FR-09: nothing by that name, or nothing by it in the ecosystem
        // asked for, or only skills the registry excludes — one answer for all
        // three, so the refusal never reveals that a skill exists but is
        // withheld.
        [] => Err(ToolRefusal::SkillNotFound),
        // LSK-FR-10: never pick one. The list shows a model whichever copy
        // sorts first and the search shows it whichever matched better, so a
        // silent choice here could hand back instructions it never saw while it
        // believed it was following the ones it did.
        many => {
            let mut ecosystems: Vec<Ecosystem> = many.iter().map(|s| s.ecosystem).collect();
            ecosystems.sort();
            ecosystems.dedup();
            if ecosystems.len() > 1 {
                Err(ToolRefusal::SkillAmbiguous(ecosystems))
            } else {
                // Every candidate already sits in the one ecosystem an
                // argument could name, so telling the model to narrow by
                // ecosystem would send it back for a call that refuses
                // identically — with `retryable` promising otherwise. Two
                // skill folders in one family declaring the same name is a
                // project that made its own name ambiguous, and no argument
                // reaches past it (TLC-FR-11).
                Err(ToolRefusal::SkillNameNotUnique)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The body (LSK-FR-13)
// ---------------------------------------------------------------------------

const DELIMITER: &str = "---";

/// The instructions below a `SKILL.md`'s frontmatter, byte-for-byte.
///
/// The delimiters are located rather than the YAML re-parsed and re-emitted, so
/// whatever the header contained the text beneath it is returned exactly as its
/// author wrote it — trailing newline, inner `---`, and all.
///
/// The rules track `skills::parse_frontmatter` deliberately: a leading BOM is
/// tolerated, because an editor on Windows emits one routinely and DSL admitted
/// the skill in spite of it; and an *unterminated* block is not frontmatter at
/// all, so the file is returned whole rather than swallowed entirely by a
/// Markdown horizontal rule on line one.
pub fn strip_frontmatter(text: &str) -> &str {
    // `trim_start_matches` rather than `strip_prefix`, matching
    // `skills::parse_frontmatter` exactly: it strips *every* leading BOM, and a
    // file carrying two is admitted to the registry by that reader. Stripping
    // only one here would leave the second in front of the `---`, the header
    // would not be recognised, and the model would be handed the frontmatter it
    // was promised would be removed.
    let body = text.trim_start_matches('\u{feff}');
    let mut lines = body.split_inclusive('\n');
    match lines.next() {
        Some(first) if first.trim_end() == DELIMITER => {
            let mut offset = first.len();
            for line in lines {
                let closes = !line.starts_with([' ', '\t']) && line.trim_end() == DELIMITER;
                offset += line.len();
                if closes {
                    return &body[offset..];
                }
            }
            // No closing delimiter: not a header, so nothing is a header.
            text
        }
        _ => text,
    }
}

/// Read the resolved skill's instructions.
///
/// The read goes through the handle the indexer mounted, so FSA-FR-10's
/// path-escape rejection and FSA-FR-17's symlink refusal bind it exactly as
/// they bind the pass that enumerated the file (LSK-FR-15). No ceiling is
/// applied: a model that named one skill asked for that skill's procedure, and
/// half a procedure reads as a whole one (LSK-FR-14).
pub fn read_instructions(
    root: &crate::fs::RootFs,
    skill: &SkillDescriptor,
) -> Result<String, ToolRefusal> {
    let abs = crate::fs::resolve_under(root.path(), &skill.path)
        .map_err(|_| ToolRefusal::SkillUnreadable)?;
    let text = root
        .read_text(&abs)
        .map_err(|_| ToolRefusal::SkillUnreadable)?;
    Ok(strip_frontmatter(&text).to_string())
}

/// The whole call against a mounted project.
pub fn load(indexer: &Bm25Indexer, args: &LoadSkillArgs) -> Result<String, ToolRefusal> {
    let skill = resolve(&skills::list_skills(indexer), args)?;
    // Taken after the resolve so a project closed mid-call refuses rather than
    // reading through a root the application has already let go of. The caller
    // has already seen a mounted root, so this is a race guard rather than a
    // path a test can drive: it fires only if a close lands between the two
    // reads of the schedule.
    let root = indexer.root_fs().ok_or(ToolRefusal::NoProjectOpen)?;
    read_instructions(&root, &skill)
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// LSK-FR-01: `load_skill` as a `rig` portable tool.
pub struct SkillLoadTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    /// Threaded rather than reached for as a global, so a test can assert on
    /// the records this tool emits without racing the process-wide buffer.
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> SkillLoadTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name;
    /// there is no registry that would attach it on anyone's behalf.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        SkillLoadTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SkillLoadTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14).
    fn run(&self, args: LoadSkillArgs) -> Result<String, ToolRefusal> {
        let outcome = require_open_project(&self.app).and_then(|indexer| load(&indexer, &args));
        match &outcome {
            // LSK-FR-19: the size of the answer and nothing drawn from it. The
            // requested name was composed by a model out of the conversation it
            // is having, and the instructions are project material, so neither
            // the name, the resolved path, the ecosystem, nor any part of the
            // text belongs in a record.
            Ok(text) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! { "bytes" => text.len() },
            ),
            Err(refusal) => log_tool_refusal(&self.app, self.buffer, NAME, refusal),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for SkillLoadTool<R> {
    const NAME: &'static str = NAME;

    type Args = LoadSkillArgs;
    /// LSK-FR-12: a `String`, which `rig` carries as one text block. No
    /// wrapping object, no named field, and nothing this tool wrote itself.
    type Output = String;
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
        // TLC-FR-16: a registry lookup and one bounded read, resolved before
        // the future is ever polled and waiting on no pass in flight.
        std::future::ready(self.run(arguments))
    }
}
