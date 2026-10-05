//! Read graduation file tool (`RGF-read-graduation-file-tool.md`).
//!
//! The tool that lets the graduation loop read the specification it is about to
//! approve. The manifest tells it which paths changed and what the application
//! made of them, but whether a specification actually answers the prompt it was
//! written from is a question only its text can settle, and a loop deciding that
//! from a list of paths would be approving filenames.
//!
//! ## Why this is not `read_file`
//!
//! `RFT-read-file-tool.md` reads the **active worktree** (RFT-FR-03), which
//! during a graduation is very often a different branch, a different worktree,
//! or a different project entirely. A loop given it would read a
//! plausible-looking wrong tree and never know. So this tool's constructor binds
//! a `crate::fs::FsAccess` whose *single* allowed root is the run's execution
//! directory (RGF-FR-05), and the two tools are never attached to one agent.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

use super::{
    log_tool_refusal_with, log_tool_success, ToolRefusal, GRADUATION_PATH_BLANK,
};
use crate::fs::{EntryKind, FsAccess, FsError};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

// ---------------------------------------------------------------------------
// The contract surface (RGF contract surface)
// ---------------------------------------------------------------------------

/// RGF-FR-01.
pub const NAME: &str = "read_graduation_file";

/// RGF-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Read a file from the working copy this graduation is being written in, and get its text back. Use it on the paths the change set names, to see whether what was written actually answers the prompt, and on any other file in the project to check that it is consistent with what is already there. Give the path relative to the project root, exactly as the change set reports it. Returns the file's full text by default; pass `offset` and `limit` to read a range of lines instead, which is what you want for a file too large to be worth reading whole. This working copy already holds every change made so far, so what you read is what would be published. This tool reads one file and returns it as written — it does not list a folder, does not search, does not summarise what it returns, and can neither reach outside this working copy nor change anything anywhere.";

const PATH_DESCRIPTION: &str = "The file to read, as a path relative to the project root. For example: 'specifications/ui/ABC-thing.md'. A path outside this working copy cannot be read.";

const OFFSET_DESCRIPTION: &str = "The first line to return, counting from 0. Defaults to 0, the start of the file. Use it with `limit` to read part of a large file; an offset past the end of the file returns nothing.";

const LIMIT_DESCRIPTION: &str = "How many lines to return, starting at `offset`. Defaults to the rest of the file. A value below 1 is treated as 1, and a value reaching past the end returns the lines that exist.";

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07), and the two
/// optional integers are `i64` so a model's `-5` decodes and is clamped
/// (RGF-FR-10) rather than failing to deserialize before this tool sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadGraduationFileArgs {
    pub path: String,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub offset: Option<i64>,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits, and
/// fall back to the documented default for anything else rather than losing a
/// perfectly good `path` over the shape of an optional parameter.
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

/// TLC-FR-06: the JSON Schema for [`ReadGraduationFileArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": PATH_DESCRIPTION },
            "offset": { "type": "integer", "description": OFFSET_DESCRIPTION },
            "limit": { "type": "integer", "description": LIMIT_DESCRIPTION },
        },
        "required": ["path"],
    })
}

// ---------------------------------------------------------------------------
// Resolution (RGF-FR-04 … RGF-FR-07)
// ---------------------------------------------------------------------------

/// RGF-FR-06: the model's path, resolved against the execution directory and
/// refused when it lands outside.
///
/// Resolution is lexical — `.` dropped and `..` collapsed against the segment
/// before it, without consulting the disk — so containment is judged by where
/// the path ends rather than by where it passed, and a path that climbs out of
/// the root and back into it is read.
pub fn resolve(root: &std::path::Path, path: &str) -> Result<PathBuf, ToolRefusal> {
    if path.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(GRADUATION_PATH_BLANK));
    }
    crate::fs::resolve_under(root, path.trim())
        .map_err(|_| ToolRefusal::GraduationPathOutside)
}

/// The `FsError` vocabulary in this tool's terms (RGF-FR-13).
fn refusal_for(error: FsError) -> ToolRefusal {
    match error {
        FsError::Utf8 { .. } => ToolRefusal::FileNotText,
        FsError::SymlinkRefused { .. } | FsError::SymlinkCycle { .. } => {
            ToolRefusal::PathThroughLink
        }
        FsError::EscapesAllowedRoots { .. }
        | FsError::PathEscape { .. }
        | FsError::RelativePath { .. } => ToolRefusal::GraduationPathOutside,
        FsError::NotADirectory { .. } => ToolRefusal::PathIsFolder,
        _ => ToolRefusal::GraduationFileNotFound,
    }
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// RGF-FR-07 … RGF-FR-12: the read, performed through the bound instance.
pub fn read(
    access: &FsAccess,
    root: &std::path::Path,
    args: &ReadGraduationFileArgs,
) -> Result<String, ToolRefusal> {
    // RGF-FR-14: a working copy that is gone is said plainly to be over, rather
    // than reported as a missing file for every path in turn.
    if !root.is_dir() {
        return Err(ToolRefusal::GraduationWorkingCopyUnavailable);
    }
    let absolute = resolve(root, &args.path)?;

    // A folder is the one kind `read_text` cannot tell apart usefully; a link
    // needs no arm, the helper refusing one before opening anything
    // (FSA-FR-17).
    if access.file_info(&absolute).map_err(refusal_for)?.kind == EntryKind::Dir {
        return Err(ToolRefusal::PathIsFolder);
    }
    let text = access.read_text(&absolute).map_err(refusal_for)?;
    // RGF-FR-09 … RGF-FR-12: the same paging the read-file tool performs, and
    // for the same reason — each line keeps the terminator the file gave it, so
    // a range is precisely the span of the file those lines occupy and nothing
    // this tool composed is added to it.
    let (offset, limit) =
        crate::tools::file_read::normalize_range(args.offset, args.limit);
    Ok(crate::tools::file_read::slice_lines(&text, offset, limit).to_string())
}

// ---------------------------------------------------------------------------
// Logging (RGF-FR-16)
// ---------------------------------------------------------------------------

/// RGF-FR-16: how long a `path` may be before a record carries only its head.
const LOGGED_PATH_LIMIT: usize = 512;

fn logged_path(path: &str) -> String {
    super::bounded_argument(path, LOGGED_PATH_LIMIT)
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// RGF-FR-01: `read_graduation_file` as a `rig` portable tool.
///
/// Constructed with the run it is judging and an instance whose single allowed
/// root is that run's execution directory (RGF-FR-05). The run is fixed before
/// the model composes an argument, so there is no run identifier among its
/// parameters and no way for a model to read out of a run it is not judging.
pub struct ReadGraduationFileTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    run_id: String,
    root: PathBuf,
    access: Arc<FsAccess>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> ReadGraduationFileTool<R> {
    /// TLC-FR-19: this tool's own constructor.
    ///
    /// Fails where the execution directory cannot be governed at all, because a
    /// tool with no instance is a tool that would have to reach the disk
    /// unguarded — the one thing FSA-FR-19 exists to make impossible.
    pub fn new(
        app: tauri::AppHandle<R>,
        run_id: impl Into<String>,
        execution_directory: PathBuf,
    ) -> Result<Self, String> {
        let access = crate::fs::FsAccess::builder()
            .allow_root(&execution_directory)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(ReadGraduationFileTool {
            app,
            run_id: run_id.into(),
            root: execution_directory,
            access: Arc::new(access),
            buffer: &BUFFER,
        })
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(
        app: tauri::AppHandle<R>,
        run_id: impl Into<String>,
        execution_directory: PathBuf,
        buffer: &'static LogBuffer,
    ) -> Result<Self, String> {
        let mut tool = ReadGraduationFileTool::new(app, run_id, execution_directory)?;
        tool.buffer = buffer;
        Ok(tool)
    }

    /// The instance this tool holds — its whole reach (RGF-FR-05).
    #[cfg(test)]
    pub fn allowed_roots(&self) -> Vec<PathBuf> {
        self.access.roots().to_vec()
    }

    fn run(&self, args: ReadGraduationFileArgs) -> Result<String, ToolRefusal> {
        let outcome = read(&self.access, &self.root, &args);
        match &outcome {
            // RGF-FR-16: the tool, the run, the path, and the outcome. The path
            // is the model's own argument, bounded, because a record about a
            // file read is unfollowable without saying which run and which
            // file. No part of the file's contents.
            Ok(text) => log_tool_success(
                &self.app,
                self.buffer,
                NAME,
                log_fields! {
                    "run_id" => self.run_id.clone(),
                    "path" => logged_path(&args.path),
                    "bytes" => text.len(),
                },
            ),
            Err(refusal) => log_tool_refusal_with(
                &self.app,
                self.buffer,
                NAME,
                refusal,
                log_fields! {
                    "run_id" => self.run_id.clone(),
                    "path" => logged_path(&args.path),
                },
            ),
        }
        outcome
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for ReadGraduationFileTool<R> {
    const NAME: &'static str = NAME;

    type Args = ReadGraduationFileArgs;
    /// RGF-FR-12: a `String` is carried by `rig` as one text block, so the model
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
        // RGF non-functional: answers from the execution directory as it stands
        // and waits on no scan, no index, and no watcher, so it resolves before
        // the future is ever polled.
        std::future::ready(self.run(arguments))
    }
}
