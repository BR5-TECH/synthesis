//! Read file tool (`RFT-read-file-tool.md`).
//!
//! The tool an agent reaches for once it knows which file it wants and needs
//! what is actually written in it. It takes a project-relative path and returns
//! that file's text — the whole of it by default, or a range of lines when the
//! agent asks for one.
//!
//! ## Why two containment gates, neither redundant
//!
//! Reads run on the agent session's own `FsAccess` (FSA-FR-29, RFT-FR-05),
//! whose allowlist holds the worktree and that session's temp directory and
//! *not* `app_data_dir()` — so no path a model composes can reach the
//! recent-projects list or anything else the user-global store holds, because
//! the instance has no root there to land in. What the instance does hold beyond the project is that
//! session's scratch directory, and [`resolve`] is what keeps this tool out of
//! it: the project-root bound is a second, narrower gate rather than a copy of
//! the first (RFT-FR-04).
//!
//! ## Why the whole file is read even for a line range
//!
//! A range is cut from what one `read_text` returned rather than by seeking, so
//! a paged call costs what an unpaged one costs and paging buys context rather
//! than I/O. That is the spec's non-functional posture, and it is also what
//! makes RFT-FR-11 cheap to honour: the terminators are still attached to the
//! lines when the cut is made.

use std::path::PathBuf;

use serde::Deserialize;

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal, PATH_BLANK,
};
use tauri::Manager;

use crate::fs::{EntryKind, FsAccess, FsError};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (RFT contract surface)
// ---------------------------------------------------------------------------

/// RFT-FR-01.
pub const NAME: &str = "read_file";

/// RFT-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Read a file from this project and get its text back. Use this when you already know which file you want — because a search named it, or because another file referred to it — and you need what is actually written in it. Give the path relative to the project root, exactly as it was reported to you. Returns the file's full text by default; pass `offset` and `limit` to read a range of lines instead, which is what you want for a file too large to be worth reading whole. This tool reads one file and returns it as written — it does not search for a file, does not summarise or interpret what it returns, and can neither reach outside this project nor change anything anywhere.";

const PATH_DESCRIPTION: &str = "The file to read, as a path relative to the project root. For example: 'src/main.ts'. A path outside the project cannot be read.";

const OFFSET_DESCRIPTION: &str = "The first line to return, counting from 0. Defaults to 0, the start of the file. Use it with `limit` to read part of a large file; an offset past the end of the file returns nothing.";

const LIMIT_DESCRIPTION: &str = "How many lines to return, starting at `offset`. Defaults to the rest of the file. A value below 1 is treated as 1, and a value reaching past the end returns the lines that exist.";

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). `offset` and
/// `limit` are `i64` so a model's `-5` decodes and is clamped (RFT-FR-07,
/// RFT-FR-08) rather than failing to deserialize before this tool sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub offset: Option<i64>,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits.
///
/// A value that is not a number in any spelling, and an explicit `null`, both
/// fall back to the documented default rather than refusing — losing a
/// perfectly good `path` over the shape of an optional parameter is what
/// TLC-FR-07 exists to prevent.
fn lenient_integer<'de, D: serde::Deserializer<'de>>(
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

/// TLC-FR-06: the JSON Schema for [`ReadFileArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": PATH_DESCRIPTION,
            },
            "offset": {
                "type": "integer",
                "description": OFFSET_DESCRIPTION,
            },
            "limit": {
                "type": "integer",
                "description": LIMIT_DESCRIPTION,
            },
        },
        "required": ["path"],
    })
}

// ---------------------------------------------------------------------------
// Resolution (RFT-FR-03, RFT-FR-04)
// ---------------------------------------------------------------------------

/// RFT-FR-03, RFT-FR-04: the model's path, resolved against the project root
/// and refused when it lands outside.
///
/// Resolution is `crate::fs::resolve_under`'s — lexical, `.` dropped and `..`
/// collapsed against the segment before it, without consulting the disk — so
/// containment is judged by where the path ends rather than by where it passed
/// and a path that climbs out of the root and back into it is read.
pub fn resolve(root: &std::path::Path, path: &str) -> Result<PathBuf, ToolRefusal> {
    if path.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(PATH_BLANK));
    }
    // Trimmed, because a model that padded a path meant the path (TLC-FR-07).
    crate::fs::resolve_under(root, path.trim()).map_err(|_| ToolRefusal::PathOutsideProject)
}

/// The `FsError` vocabulary in this tool's terms (RFT-FR-13 … RFT-FR-16).
///
/// A residual I/O failure — a file present but unreadable, say — maps to
/// [`ToolRefusal::FileNotFound`]. The spec enumerates seven refusals and this is
/// not among them; of the seven it is the only one whose advice ("check the
/// path, or search again") is right for a model that cannot get at the file,
/// where the not-text refusal would assert something false about its contents.
fn refusal_for(error: FsError) -> ToolRefusal {
    match error {
        FsError::Utf8 { .. } => ToolRefusal::FileNotText,
        FsError::SymlinkRefused { .. } | FsError::SymlinkCycle { .. } => {
            ToolRefusal::PathThroughLink
        }
        FsError::EscapesAllowedRoots { .. }
        | FsError::PathEscape { .. }
        | FsError::RelativePath { .. } => ToolRefusal::PathOutsideProject,
        FsError::NotADirectory { .. } => ToolRefusal::PathIsFolder,
        _ => ToolRefusal::FileNotFound,
    }
}

// ---------------------------------------------------------------------------
// Paging (RFT-FR-06 … RFT-FR-11)
// ---------------------------------------------------------------------------

/// RFT-FR-07, RFT-FR-08: the documented defaults, clamped as documented.
///
/// Returns the 0-based first line and the count, `None` meaning "to the end".
pub fn normalize_range(offset: Option<i64>, limit: Option<i64>) -> (usize, Option<usize>) {
    let start = offset.unwrap_or(0).max(0) as usize;
    let count = limit.map(|value| value.max(1) as usize);
    (start, count)
}

/// RFT-FR-06, RFT-FR-09, RFT-FR-10, RFT-FR-11: the requested span of `text`.
///
/// `split_inclusive` is what makes RFT-FR-11 hold without any work: each line
/// keeps the terminator the file gave it, so a `\r\n` file reads back with its
/// `\r\n`, a final line without a terminator gets none, and the concatenation
/// of a range is precisely the span of the file those lines occupy. With both
/// bounds absent this is the whole text and no allocation of a slice happens at
/// all (RFT-FR-06).
pub fn slice_lines(text: &str, offset: usize, limit: Option<usize>) -> &str {
    if offset == 0 && limit.is_none() {
        return text;
    }
    let mut lines = text.split_inclusive('\n');
    // Byte offset of the first wanted line. Consuming rather than indexing
    // keeps this correct for multi-byte text: a line's length is its bytes.
    let mut start = 0usize;
    for _ in 0..offset {
        match lines.next() {
            Some(line) => start += line.len(),
            // RFT-FR-09: the file has no line at that position, which is a
            // success carrying empty text rather than a refusal.
            None => return "",
        }
    }
    let Some(count) = limit else {
        return &text[start..];
    };
    let mut end = start;
    for _ in 0..count {
        match lines.next() {
            Some(line) => end += line.len(),
            // RFT-FR-08: a limit reaching past the end returns the lines that
            // exist rather than refusing.
            None => break,
        }
    }
    &text[start..end]
}

// ---------------------------------------------------------------------------
// Logging (RFT-FR-20)
// ---------------------------------------------------------------------------

/// RFT-FR-20: how long a `path` may be before a record carries only its head.
const LOGGED_PATH_LIMIT: usize = 512;

/// RFT-FR-20: the `path` as the model composed it, bounded.
///
/// Verbatim otherwise — untrimmed and unresolved — because the point of the
/// field is to say which file the model asked for, and both the trimmed and the
/// resolved form answer a slightly different question. The bound is
/// [`super::bounded_argument`]'s, shared with the other tool whose spec names an
/// argument as loggable, and the reason for it is that function's.
fn logged_path(path: &str) -> String {
    super::bounded_argument(path, LOGGED_PATH_LIMIT)
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// RFT-FR-05: the read, performed through the agent session's `FsAccess`.
///
/// `file_info` runs first because it is the only way to tell RFT-FR-14's folder
/// from RFT-FR-13's absent path and RFT-FR-16's link from either — it describes
/// the entry itself and resolves nothing (FSA-FR-23), so a symbolic link reports
/// as a link rather than as whatever it points at. It stats and reads nothing,
/// which leaves the spec's "one whole-file read and no more" intact.
pub fn read(
    access: &FsAccess,
    root: &std::path::Path,
    args: &ReadFileArgs,
) -> Result<String, ToolRefusal> {
    let absolute = resolve(root, &args.path)?;

    // A folder is the one kind `read_text` cannot tell apart usefully: it fails
    // with an I/O error indistinguishable from any other, and RFT-FR-14 wants
    // the model told which mistake it made. A link needs no arm here — the
    // helper refuses one before opening anything (FSA-FR-17), which is
    // RFT-FR-16 already served by the gate RFT-FR-05 says owns it.
    if access.file_info(&absolute).map_err(refusal_for)?.kind == EntryKind::Dir {
        return Err(ToolRefusal::PathIsFolder);
    }

    let text = access.read_text(&absolute).map_err(refusal_for)?;
    let (offset, limit) = normalize_range(args.offset, args.limit);
    Ok(slice_lines(&text, offset, limit).to_string())
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// RFT-FR-01: `read_file` as a `rig` portable tool.
///
/// Constructed with the agent session's own `FsAccess` (FSA-FR-29): the caller
/// attaching tools to an agent is the one that knows which session it is
/// running, and binding the instance here is what makes RFT-FR-05 a property of
/// the tool rather than a convention its callers are trusted to follow. The app
/// handle supplies the project root and the log sink, so a project opened after
/// the tool was constructed is visible to it.
pub struct FileReadTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    session: String,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> FileReadTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name,
    /// naming the agent session whose reach this tool is to have.
    pub fn new(app: tauri::AppHandle<R>, session: impl Into<String>) -> Self {
        FileReadTool {
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
        FileReadTool {
            app,
            session: session.into(),
            buffer,
        }
    }

    /// This session's instance, as it stands right now (FSA-FR-29).
    ///
    /// Resolved per call rather than held, for the reason the other tools
    /// resolve the indexer per call: an instance is discarded when the project
    /// closes or the active worktree changes, and a tool holding its own `Arc`
    /// would keep read-write reach into a checkout the application has stopped
    /// showing. Looking it up each time is what makes "no session keeps reading
    /// a retired worktree" true of this tool rather than merely intended.
    fn access(&self) -> Result<std::sync::Arc<FsAccess>, ToolRefusal> {
        // `try_state`, not `state`: a mock app in another module's test manages
        // only what that test needs, and a panic here would be a far worse
        // answer than the refusal this returns anyway.
        let state = self
            .app
            .try_state::<crate::fs::FsAccessState>()
            .ok_or(ToolRefusal::NoProjectOpen)?;
        state
            .agent_session(&self.session)
            .ok_or(ToolRefusal::NoProjectOpen)
    }

    /// The whole call, with its logging (TLC-FR-14, RFT-FR-20).
    fn run(&self, args: ReadFileArgs) -> Result<String, ToolRefusal> {
        // RFT-FR-17: no project, no root to resolve against, so the shared
        // refusal rather than an attempt against nothing.
        let outcome = require_open_project(&self.app).and_then(|indexer| {
            let root = indexer.root().ok_or(ToolRefusal::NoProjectOpen)?;
            let access = self.access()?;
            read(&access, &root, &args)
        });
        match &outcome {
            // RFT-FR-20: the path asked for and the byte count. The path is the
            // model's own argument, recorded as it composed it and bounded by
            // [`logged_path`], because a record saying only that a read happened
            // does not say which file it was about. Not the offset, not the
            // limit, and no part of the file, which is the project's own
            // material.
            Ok(text) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! { "path" => logged_path(&args.path), "bytes" => text.len() },
            ),
            // The refusal record wants the path for the same reason and rather
            // more: which path was refused is the whole of what a model needs to
            // correct, and a refused path is one no read ever reached.
            Err(refusal) => log_tool_refusal_with(
                &self.app,
                self.buffer,
                NAME,
                refusal,
                log_fields! { "path" => logged_path(&args.path) },
            ),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for FileReadTool<R> {
    const NAME: &'static str = NAME;

    type Args = ReadFileArgs;
    /// RFT-FR-10: a `String` is carried by `rig` as one text block, so the model
    /// receives the file's characters unwrapped rather than a JSON object with
    /// the document escaped into a string field (TLC-FR-08).
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
        // RFT-FR-18: answers from the filesystem as it stands, consulting no
        // index, no scan, and no watcher, so it resolves before the future is
        // ever polled.
        std::future::ready(self.run(arguments))
    }
}
