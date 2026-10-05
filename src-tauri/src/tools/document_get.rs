//! Get document tool (`GDT-get-document-tool.md`).
//!
//! The tool an agent reaches for once it knows which reference document it wants
//! and needs the text of that document. It takes the stable id that
//! `search_documents` returned and returns the text of the document: all of it
//! by default, or a byte range or a line range.
//!
//! ## Why there is no path argument
//!
//! The tool resolves the id through the Documents collection and reads through
//! the collection's own read-only instance of the filesystem helper
//! (GDT-FR-FZHF, GDT-FR-HCYY). A string that looks like a path is an id the
//! collection does not hold, so it is the unknown-document refusal. No other
//! tool gains reach to a selected path.
//!
//! ## Why a range is cut from the whole text
//!
//! A range is cut from what one read returned, so paging buys context and not
//! I/O (GDT non-functional). For a PDF the text is the extracted text from the
//! session cache, and the PDF bytes are never returned (GDT-FR-HREW).

use serde::Deserialize;
use tauri::Manager;

use super::file_read::{normalize_range, slice_lines};
use super::{
    bounded_argument, log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal,
    DOCUMENT_BOTH_RANGE_FORMS, DOCUMENT_ID_BLANK,
};
use crate::documents::{DocumentsCollection, DocumentsError};
use crate::fs::FsAccessState;
use crate::log_fields;
use crate::logging::{Fields, LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (GDT contract surface)
// ---------------------------------------------------------------------------

/// GDT-FR-LITM.
pub const NAME: &str = "get_document";

/// GDT-FR-GNCR: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Get Document: read the text of one reference document the user selected for this project. Use this when a search for documents named a document and you need what it says. Give the document id exactly as the search reported it. Returns the whole text by default. For a PDF the text is the text extracted from the PDF, because the PDF itself cannot be returned. To read part of a long document, pass a byte range with `byte_offset` and `byte_length`, or a line range with `line_offset` and `line_limit`. Do not pass both kinds of range in one call. This tool reads one document and returns it as it is written. It does not search, does not summarise, cannot read any file that is not a selected document, and cannot change anything.";

const ID_DESCRIPTION: &str = "The id of the document to read, exactly as a search for documents reported it. For example: 'doc-0123456789abcdef0123456789abcdef'. A path cannot be used here.";

const BYTE_OFFSET_DESCRIPTION: &str = "The first byte to return, counting from 0 in the UTF-8 text of the document. Use it with `byte_length` to read part of a long document. Do not combine it with a line range.";

const BYTE_LENGTH_DESCRIPTION: &str = "How many bytes to return, starting at `byte_offset`. Defaults to the rest of the text. A range that cuts through a character is widened to include the whole character.";

const LINE_OFFSET_DESCRIPTION: &str = "The first line to return, counting from 0. Use it with `line_limit` to read part of a long document. Do not combine it with a byte range.";

const LINE_LIMIT_DESCRIPTION: &str = "How many lines to return, starting at `line_offset`. Defaults to the rest of the text. A value below 1 is treated as 1.";

/// GDT-FR-LDHA: how long an `id` may be before a record carries only its head.
const LOGGED_ID_LIMIT: usize = 512;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). The four range
/// arguments are `i64` so a model's `-5` decodes and is clamped (GDT-FR-QTAA,
/// GDT-FR-RQZS) rather than failing to deserialize before this tool sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct GetDocumentArgs {
    pub id: String,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub byte_offset: Option<i64>,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub byte_length: Option<i64>,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub line_offset: Option<i64>,
    #[serde(default, deserialize_with = "lenient_integer")]
    pub line_limit: Option<i64>,
}

/// TLC-FR-07: accept every spelling of a number a model plausibly emits. A value
/// that is not a number in any spelling, and an explicit `null`, fall back to
/// the documented default.
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

/// TLC-FR-06: the JSON Schema for [`GetDocumentArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": ID_DESCRIPTION },
            "byte_offset": { "type": "integer", "description": BYTE_OFFSET_DESCRIPTION },
            "byte_length": { "type": "integer", "description": BYTE_LENGTH_DESCRIPTION },
            "line_offset": { "type": "integer", "description": LINE_OFFSET_DESCRIPTION },
            "line_limit": { "type": "integer", "description": LINE_LIMIT_DESCRIPTION },
        },
        "required": ["id"],
    })
}

// ---------------------------------------------------------------------------
// Ranges (GDT-FR-NKKK … GDT-FR-RQZS)
// ---------------------------------------------------------------------------

/// Which range form a call used (GDT-FR-VZXF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeForm {
    None,
    Bytes,
    Lines,
}

impl RangeForm {
    fn as_str(self) -> &'static str {
        match self {
            RangeForm::None => "none",
            RangeForm::Bytes => "bytes",
            RangeForm::Lines => "lines",
        }
    }
}

/// GDT-FR-MVHY: the range form a call asks for, or the retryable refusal when it
/// asks for both. It looks at the arguments alone, so it runs before the id is
/// resolved or any text is read.
pub fn range_form(args: &GetDocumentArgs) -> Result<RangeForm, ToolRefusal> {
    let bytes = args.byte_offset.is_some() || args.byte_length.is_some();
    let lines = args.line_offset.is_some() || args.line_limit.is_some();
    match (bytes, lines) {
        (true, true) => Err(ToolRefusal::InvalidArguments(DOCUMENT_BOTH_RANGE_FORMS)),
        (true, false) => Ok(RangeForm::Bytes),
        (false, true) => Ok(RangeForm::Lines),
        (false, false) => Ok(RangeForm::None),
    }
}

/// GDT-FR-QTAA, GDT-FR-YXCH, GDT-FR-IUFV: the half-open byte range
/// `[offset, offset + length)` of `text`, widened outward to whole UTF-8
/// characters.
///
/// `offset` defaults to 0 and a negative value is clamped to 0. `length`
/// defaults to the rest of the text, and a value below 1 is clamped to 1. A range
/// that reaches past the end returns the bytes that exist, and an offset at or
/// beyond the end is empty text. When the start falls inside a character it moves
/// back to that character's first byte, and when the end falls inside one it
/// moves forward past that character's last byte.
pub fn slice_bytes(text: &str, offset: Option<i64>, length: Option<i64>) -> &str {
    let total = text.len();
    let start = offset.unwrap_or(0).max(0) as u64;
    if start >= total as u64 {
        return "";
    }
    let start = start as usize;
    let end = match length {
        None => total,
        Some(length) => {
            let length = length.max(1) as u64;
            (start as u64).saturating_add(length).min(total as u64) as usize
        }
    };
    let mut from = start;
    while !text.is_char_boundary(from) {
        from -= 1;
    }
    let mut to = end;
    while !text.is_char_boundary(to) {
        to += 1;
    }
    &text[from..to]
}

/// GDT-FR-NKKK, GDT-FR-QTAA, GDT-FR-RQZS: the text a call asked for. With no
/// range it is the whole text, whatever its size.
pub fn cut(text: &str, args: &GetDocumentArgs, form: RangeForm) -> String {
    match form {
        RangeForm::None => text.to_string(),
        RangeForm::Bytes => slice_bytes(text, args.byte_offset, args.byte_length).to_string(),
        RangeForm::Lines => {
            let (offset, limit) = normalize_range(args.line_offset, args.line_limit);
            slice_lines(text, offset, limit).to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// The call
// ---------------------------------------------------------------------------

/// GDT-FR-FZHF, GDT-FR-IPJG, GDT-FR-IFCA: refuse a bad argument, resolve the id
/// through the collection, read the text through its read-only instance, and cut
/// the range from it.
///
/// A blank id and both range forms are refused before the id is resolved or any
/// text is read (GDT-FR-MVHY). The text is the current content of the document
/// and nothing this tool composed (GDT-FR-CHWC).
pub fn read(
    collection: &DocumentsCollection,
    fs: &FsAccessState,
    log: &dyn crate::documents::log::DocLog,
    args: &GetDocumentArgs,
) -> Result<(String, RangeForm), ToolRefusal> {
    if args.id.trim().is_empty() {
        return Err(ToolRefusal::InvalidArguments(DOCUMENT_ID_BLANK));
    }
    let form = range_form(args)?;
    let id = args.id.trim();
    collection.resolve_document(id).map_err(refusal_for)?;
    let text = collection.document_text(fs, log, id).map_err(refusal_for)?;
    Ok((cut(&text, args, form), form))
}

/// Map a collection error onto the refusal a model reads.
fn refusal_for(error: DocumentsError) -> ToolRefusal {
    match error {
        DocumentsError::NoProjectOpen | DocumentsError::StoreUnavailable => {
            ToolRefusal::NoProjectOpen
        }
        DocumentsError::UnknownDocument | DocumentsError::InvalidPath => {
            ToolRefusal::DocumentNotFound
        }
        DocumentsError::Unavailable | DocumentsError::WrongFormat => {
            ToolRefusal::DocumentUnavailable
        }
        DocumentsError::NoText => ToolRefusal::DocumentNoText,
    }
}

// ---------------------------------------------------------------------------
// Logging (GDT-FR-VZXF, GDT-FR-LDHA)
// ---------------------------------------------------------------------------

/// GDT-FR-LDHA: the `id` as the model composed it — untrimmed — and bounded.
/// No record carries a range value, a document name or path, or any part of the
/// text.
fn logged_arguments(args: &GetDocumentArgs) -> Fields {
    log_fields! { "id" => bounded_argument(&args.id, LOGGED_ID_LIMIT) }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// GDT-FR-LITM: `get_document` as a `rig` portable tool.
///
/// Holds the app handle rather than the collection, so a project opened after
/// the tool was constructed is visible to it and one closed since is not
/// (TLC-FR-15). It reads through the collection's documents instance and never
/// through an agent session's instance (GDT-FR-HCYY).
pub struct GetDocumentTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> GetDocumentTool<R> {
    /// TLC-FR-19: this tool's own constructor. A caller attaches it by name.
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        GetDocumentTool {
            app,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(app: tauri::AppHandle<R>, buffer: &'static LogBuffer) -> Self {
        GetDocumentTool { app, buffer }
    }

    /// The whole call, with its logging (TLC-FR-14, GDT-FR-VZXF).
    fn run(&self, args: GetDocumentArgs) -> Result<String, ToolRefusal> {
        let outcome = require_open_project(&self.app).and_then(|_| {
            let collection = self
                .app
                .try_state::<DocumentsCollection>()
                .filter(|collection| collection.is_open())
                .ok_or(ToolRefusal::NoProjectOpen)?;
            let fs = self
                .app
                .try_state::<FsAccessState>()
                .ok_or(ToolRefusal::NoProjectOpen)?;
            read(&collection, &fs, &self.app, &args)
        });
        match &outcome {
            Ok((text, form)) => {
                let mut fields = log_fields! {
                    "range" => form.as_str(),
                    "bytes" => text.len(),
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
        outcome.map(|(text, _)| text)
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for GetDocumentTool<R> {
    const NAME: &'static str = NAME;

    type Args = GetDocumentArgs;
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
        // GDT-FR-IFCA: one read of the document text, or one cache lookup for a
        // PDF, so it resolves before the future is polled and waits on no pass.
        std::future::ready(self.run(arguments))
    }
}
