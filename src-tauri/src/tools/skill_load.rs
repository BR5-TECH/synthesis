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
//! A model supplies a *name*, which selects a descriptor the registry produced
//! or selects nothing (LSK-FR-03). The one path it can compose is `file`, and
//! that path is confined to the folder of the skill it named: a `..` or an
//! absolute path is refused before anything is read (LSK-FR-PZZE), a project
//! skill's file is read only through the `FsAccess` gate with its symlink
//! refusal (LSK-FR-KDVE), and a built-in skill's file is looked up in the
//! embedded list and never on disk (LSK-FR-TLXK).

use serde::Deserialize;

use super::refusals::SKILL_FILE_OUTSIDE;
use super::{
    log_tool_refusal, log_tool_success, normalize_skill_name, require_open_project, ToolRefusal,
};
use crate::bm25_index::Bm25Indexer;
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};
use crate::skills::{self, Ecosystem, SkillDescriptor, SkillFilter, SKILL_FILE};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (LSK contract surface)
// ---------------------------------------------------------------------------

/// LSK-FR-01.
pub const NAME: &str = "load_skill";

/// LSK-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Read one skill's full instructions, so you can follow them, or one file that the skill refers to. Use this once `search_skills` or `list_skills` has told you which skill you want and you are ready to carry out its procedure rather than to weigh whether it applies. Give the skill's name exactly as it was reported to you; add `ecosystem` only when you were shown two skills sharing a name and you mean a particular one. Without `file`, returns the skill's SKILL.md instructions as plain text, without the short header block carrying the name and description you have already been given. A skill's SKILL.md often names other files in its own folder, for example `references/example.md`, that hold the detail for one case; to read one, call again with the same name and `file` set to that path, and the file is returned whole as plain text. This tool reads only files inside the skill's own folder, and it does not carry out what the skill says or run any script it ships, which is yours to decide.";

const NAME_DESCRIPTION: &str = "The skill's name, exactly as `list_skills` or `search_skills` reported it. Leading and trailing spaces and differences in letter case are ignored.";

const ECOSYSTEM_DESCRIPTION: &str = "Which ecosystem's copy to load, when two skills share a name: one of `claude`, `codex`, `github`, `opencode`, or `builtin`. Leave it out unless you have been told the name is ambiguous.";

const FILE_DESCRIPTION: &str = "A file of the skill to read instead of its SKILL.md, as a path relative to the skill's own folder, exactly as the skill's instructions name it. For example: 'references/agentflow-beta.md'. Leave it out to read the skill's instructions.";

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
    /// LSK-FR-UBQE: a path relative to the skill's own folder.
    #[serde(default, deserialize_with = "lenient_file")]
    pub file: Option<String>,
}

/// TLC-FR-07 applied to `file`: a `null` and an absent key mean no file, and a
/// value of another JSON shape is read as its JSON text, which names no file
/// of any skill and so reaches the model as the file-not-found refusal.
fn lenient_file<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text),
        other => Some(other.to_string()),
    })
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
                "enum": ["claude", "codex", "github", "opencode", "builtin"],
                "description": ECOSYSTEM_DESCRIPTION,
            },
            "file": {
                "type": "string",
                "description": FILE_DESCRIPTION,
            },
        },
        "required": ["name"],
    })
}

/// The spelling of an ecosystem a model may send, or `None` for anything that
/// names none of the five (LSK-FR-07).
pub fn parse_ecosystem(value: &str) -> Option<Ecosystem> {
    let value = value.trim();
    [
        Ecosystem::Claude,
        Ecosystem::Codex,
        Ecosystem::Github,
        Ecosystem::Opencode,
        Ecosystem::Builtin,
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
    let by_name: Vec<&SkillDescriptor> =
        skills.iter().filter(|s| normalize_skill_name(&s.name) == wanted).collect();

    // LSK-FR-07: the filter narrows, and a value naming none of the four
    // narrows to nothing rather than being refused as malformed.
    let candidates: Vec<&SkillDescriptor> = match args.ecosystem.as_deref() {
        None => by_name,
        Some(requested) => match parse_ecosystem(requested) {
            Some(ecosystem) => by_name.into_iter().filter(|s| s.ecosystem == ecosystem).collect(),
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
    let text = root.read_text(&abs).map_err(|_| ToolRefusal::SkillUnreadable)?;
    Ok(strip_frontmatter(&text).to_string())
}

// ---------------------------------------------------------------------------
// The `file` argument (LSK-FR-UBQE through LSK-FR-ULLF)
// ---------------------------------------------------------------------------

/// LSK-FR-UBQE / LSK-FR-PZZE: the segments of the requested file, `None` when
/// no file was asked for, or the refusal for a path outside the skill.
///
/// Checked before any read, so a path that escapes the skill's folder costs no
/// filesystem access at all.
pub fn requested_file(args: &LoadSkillArgs) -> Result<Option<Vec<String>>, ToolRefusal> {
    let Some(file) = args.file.as_deref().filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    let segments = skills::builtin::relative_segments(file)
        .ok_or(ToolRefusal::InvalidArguments(SKILL_FILE_OUTSIDE))?;
    // A path of nothing but `.` names the skill's folder itself, which is not
    // a file (LSK-FR-ULLF).
    if segments.is_empty() {
        return Err(ToolRefusal::SkillFileNotFound);
    }
    Ok(Some(segments.into_iter().map(str::to_string).collect()))
}

/// LSK-FR-KDVE / LSK-FR-ULLF: read one file of a project skill.
///
/// The path is the skill's folder joined with segments that hold no `..`, and
/// it goes through the `FsAccess` gate, so FSA-FR-10 and FSA-FR-17 bind it: a
/// symbolic link at the file or at any folder on the way is refused, and
/// nothing is read through it. Every failure is the one retryable refusal,
/// because the model's remedy is the same for each: read the `SKILL.md` and
/// name a file it names.
pub fn read_project_file(
    root: &crate::fs::RootFs,
    skill: &SkillDescriptor,
    segments: &[String],
) -> Result<String, ToolRefusal> {
    let folder = skill.path.strip_suffix(SKILL_FILE).unwrap_or(&skill.path).trim_end_matches('/');
    let rel = format!("{folder}/{}", segments.join("/"));
    let abs = crate::fs::resolve_under(root.path(), &rel)
        .map_err(|_| ToolRefusal::InvalidArguments(SKILL_FILE_OUTSIDE))?;
    let info = root.file_info(&abs).map_err(|_| ToolRefusal::SkillFileNotFound)?;
    if info.kind != crate::fs::EntryKind::File {
        return Err(ToolRefusal::SkillFileNotFound);
    }
    root.read_text(&abs).map_err(|_| ToolRefusal::SkillFileNotFound)
}

/// LSK-FR-SGVX / LSK-FR-TLXK: a built-in skill's instructions or one of its
/// files, from the embedded list alone.
pub fn read_builtin(
    skill: &SkillDescriptor,
    segments: Option<&[String]>,
) -> Result<String, ToolRefusal> {
    match segments {
        None => skills::builtin_skill_file(&skill.folder_name, SKILL_FILE)
            .map(|text| strip_frontmatter(text).to_string())
            // The registry admitted this skill from the same embedded list, so
            // its `SKILL.md` is there; this is the shape of the answer if not.
            .ok_or(ToolRefusal::SkillUnreadable),
        Some(segments) => skills::builtin_skill_file(&skill.folder_name, &segments.join("/"))
            .map(str::to_string)
            .ok_or(ToolRefusal::SkillFileNotFound),
    }
}

/// The whole call against a mounted project.
///
/// LSK-FR-BSJS: names resolve only among the skills `filter` admits, so a
/// project skill the agent's persona does not select is the unknown-skill
/// refusal, with or without `file`.
pub fn load(
    indexer: &Bm25Indexer,
    args: &LoadSkillArgs,
    filter: &SkillFilter,
) -> Result<String, ToolRefusal> {
    let skill = resolve(&skills::list_skills(indexer, filter), args)?;
    let segments = requested_file(args)?;
    if skill.ecosystem == Ecosystem::Builtin {
        return read_builtin(&skill, segments.as_deref());
    }
    // Taken after the resolve so a project closed mid-call refuses rather than
    // reading through a root the application has already let go of. The caller
    // has already seen a mounted root, so this is a race guard rather than a
    // path a test can drive: it fires only if a close lands between the two
    // reads of the schedule.
    let root = indexer.root_fs().ok_or(ToolRefusal::NoProjectOpen)?;
    match segments {
        None => read_instructions(&root, &skill),
        Some(segments) => read_project_file(&root, &skill, &segments),
    }
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
    /// LSK-FR-BSJS: the skills the turn's agent may use.
    filter: SkillFilter,
}

impl<R: tauri::Runtime> SkillLoadTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name;
    /// there is no registry that would attach it on anyone's behalf. The
    /// filter is the turn's agent's (CVL-FR-VZZR).
    pub fn new(app: tauri::AppHandle<R>, filter: SkillFilter) -> Self {
        SkillLoadTool { app, buffer: &BUFFER, filter }
    }

    /// The same tool, reporting into `buffer`, admitting every project skill
    /// and no built-in skill until [`Self::with_filter`] says otherwise.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        SkillLoadTool { app, buffer, filter: SkillFilter::all_project() }
    }

    /// The same tool with another filter.
    #[cfg(test)]
    pub fn with_filter(mut self, filter: SkillFilter) -> Self {
        self.filter = filter;
        self
    }

    /// The whole call, with its logging (TLC-FR-14).
    fn run(&self, args: LoadSkillArgs) -> Result<String, ToolRefusal> {
        let with_file = args.file.as_deref().is_some_and(|f| !f.trim().is_empty());
        let outcome =
            require_open_project(&self.app).and_then(|indexer| load(&indexer, &args, &self.filter));
        match &outcome {
            // LSK-FR-19: the size of the answer, whether a file was asked for,
            // and nothing drawn from either. The requested name and file were
            // composed by a model out of the conversation it is having, and the
            // text is project material, so neither the name, the file, the
            // resolved path, the ecosystem, nor any part of the text belongs in
            // a record.
            Ok(text) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! { "bytes" => text.len(), "file" => with_file },
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
