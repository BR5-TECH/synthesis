//! `request_document_export` — ask the author to export a document
//! (`RDX-request-document-export-tool.md`).
//!
//! The agent can not choose where a file goes on the author's machine, so the
//! tool exports nothing. It records one export request and posts it into the
//! turn's conversation through `crate::document_export::requests`
//! (RDX-FR-TMYF). The author exports from the card, and the outcome comes back
//! as a comment.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant};
use crate::document_export::{self as dxp, requests};
use crate::log_fields;
use crate::logging::{LogBuffer, LogSink, BUFFER};

#[cfg(test)]
mod tests;

/// RDX-FR-UBTA.
pub const NAME: &str = "request_document_export";

/// RDX-FR-ORZH: the fixed description of the contract surface.
pub const DESCRIPTION: &str = "Ask the author to export a document of the project to a PDF or a Word (.docx) file. Use it when the author asks you to export, convert, or download a document as PDF or DOCX. You can not choose where the file goes, so this tool does not export: it posts an export request into the conversation, and the author opens it, chooses the destination, and confirms. Give the document in `document`: its project-relative path, for example `docs/guide.md`, or its id from `search_documents` or `get_document`, for example `doc-…`. Only a Markdown document of the project that carries the document type can be exported. Give `format` `pdf` or `docx` when the author said which one; leave it out when they did not. Tell the author in `message`, in a sentence, what you ask them to export. Put one `<spoken>…</spoken>` tag in `message`: one short sentence that a voice can read aloud. A message with no tag is refused. Your turn ends when the request is posted, and the outcome comes back to you in the conversation.";

const DOCUMENT_DESCRIPTION: &str = "The document to export: its project-relative path, for example `docs/guide.md`, or its Documents id, for example `doc-…`.";
const FORMAT_DESCRIPTION: &str =
    "`pdf` or `docx`, when the author said which. Leave it out when they did not.";
const MESSAGE_DESCRIPTION: &str = "What you ask the author to export, in a sentence. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud.";

// RDX contract surface, "Refusals".
pub const BLANK_DOCUMENT: &str =
    "Give the document to export in `document`: its project-relative path or its Documents id.";
pub const DOCUMENT_NOT_FOUND: &str = "No document of the project has this path or id. Find it with `search_documents` and give its id or its path.";
pub const NOT_A_DOC_ARTIFACT: &str = "This file is not a document of the Documents collection, so it can not be exported. Only a Markdown file with the document type can be exported.";
pub const NOT_MARKDOWN: &str = "This document is not a Markdown file, so it can not be exported.";
pub const OUTSIDE_PROJECT: &str = "This document is not in the project, so it can not be exported.";
pub const BAD_FORMAT: &str =
    "`format` is `pdf` or `docx`. Leave it out when the author did not say which.";
pub const BLANK_MESSAGE: &str = "Tell the author in `message` what you ask them to export.";
pub const NO_SPOKEN_PART: &str = "The message has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `message`.";
pub const CONVERSATION_RESOLVED: &str = "This conversation is resolved and takes no further comments, so nothing was requested. Answer with what you already have; asking again will not succeed.";
pub const NOT_RECORDED: &str = "The export request could not be recorded.";

/// CVL-FR-15: what a second request in one reply is told.
pub const ONE_AT_A_TIME: &str = "Only one export request can be posted in a reply, so this one was not recorded. Ask for the next export after the author answered this one.";

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07). There is no destination (RDX-FR-EDQY).
#[derive(Debug, Clone, Deserialize)]
pub struct RequestDocumentExportArgs {
    #[serde(default)]
    pub document: String,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub message: String,
}

/// RDX-FR-PZGN: the data object of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestDocumentExportOutput {
    pub requested: bool,
}

/// TLC-FR-06: the JSON Schema of the contract surface.
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "document": { "type": "string", "description": DOCUMENT_DESCRIPTION },
            "format": {
                "type": "string",
                "enum": ["pdf", "docx"],
                "description": FORMAT_DESCRIPTION,
            },
            "message": { "type": "string", "description": MESSAGE_DESCRIPTION },
        },
        "required": ["document", "message"],
    })
}

/// How the tool resolves `document` (DXP-FR-PKDB). A function value, so a test
/// can stand in for the project's artifact types.
pub type Resolver<R> = fn(&tauri::AppHandle<R>, &str) -> Result<String, &'static str>;

/// RDX-FR-DVQL: the typed errors of DXP-FR-PKDB in this tool's terms.
fn refusal_for_resolve(error: &str) -> ToolRefusal {
    match error {
        dxp::ERR_NO_PROJECT_OPEN => ToolRefusal::NoProjectOpen,
        dxp::ERR_NOT_FOUND => ToolRefusal::NotFoundWith(DOCUMENT_NOT_FOUND),
        dxp::ERR_NOT_DOC_ARTIFACT => ToolRefusal::InvalidArguments(NOT_A_DOC_ARTIFACT),
        dxp::ERR_NOT_MARKDOWN => ToolRefusal::InvalidArguments(NOT_MARKDOWN),
        dxp::ERR_OUTSIDE_PROJECT => ToolRefusal::InvalidArguments(OUTSIDE_PROJECT),
        _ => ToolRefusal::NotFoundWith(DOCUMENT_NOT_FOUND),
    }
}

/// RDX-FR-POVJ: the format in lowercase, or the bad-format refusal.
fn format_of(format: Option<&str>) -> Result<Option<&'static str>, ToolRefusal> {
    let Some(format) = format else {
        return Ok(None);
    };
    match format.trim().to_ascii_lowercase().as_str() {
        "pdf" => Ok(Some("pdf")),
        "docx" => Ok(Some("docx")),
        _ => Err(ToolRefusal::InvalidArguments(BAD_FORMAT)),
    }
}

/// The stable code of a refusal, for the log (RDX-FR-IEQA).
fn refusal_code(refusal: &ToolRefusal) -> Option<&'static str> {
    let ToolRefusal::InvalidArguments(message) = refusal else {
        return None;
    };
    Some(match *message {
        BLANK_DOCUMENT => "blank_document",
        NOT_A_DOC_ARTIFACT => "not_doc_artifact",
        NOT_MARKDOWN => "not_markdown",
        OUTSIDE_PROJECT => "outside_project",
        BAD_FORMAT => "bad_format",
        BLANK_MESSAGE => "blank_message",
        NO_SPOKEN_PART => "no_spoken_part",
        _ => "other",
    })
}

/// RDX-FR-UBTA: `request_document_export` as a `rig` portable tool.
/// `requested` is how the loop learns that a request went out and the turn
/// ends (CVL-FR-15).
pub struct RequestDocumentExportTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    requested: Arc<AtomicBool>,
    resolve: Resolver<R>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> RequestDocumentExportTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// RDX-FR-EDQY: bound to the turn's conversation and agent.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        requested: Arc<AtomicBool>,
    ) -> Self {
        RequestDocumentExportTool {
            app,
            roots,
            origin,
            author,
            requested,
            resolve: dxp::source::resolve_export_document::<R>,
            buffer: &BUFFER,
        }
    }

    /// The same tool, resolving through `resolve`.
    #[cfg(test)]
    pub fn with_resolver(mut self, resolve: Resolver<R>) -> Self {
        self.resolve = resolve;
        self
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// RDX-FR-IEQA: the call with its logging. The fields are ids and kinds,
    /// and never the document, the path, or the message.
    fn run(
        &self,
        args: RequestDocumentExportArgs,
    ) -> Result<RequestDocumentExportOutput, ToolRefusal> {
        let mut fields = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
        };
        let outcome = self.request(&args);
        match &outcome {
            Ok(_) => {
                let format = format_of(args.format.as_deref()).ok().flatten().unwrap_or("any");
                fields.insert("format".into(), serde_json::json!(format));
                log_tool_success(&self.app, self.buffer, NAME, fields)
            }
            Err(refusal) => {
                if let Some(code) = refusal_code(refusal) {
                    fields.insert("refusal".into(), serde_json::json!(code));
                }
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, fields)
            }
        }
        outcome
    }

    fn request(
        &self,
        args: &RequestDocumentExportArgs,
    ) -> Result<RequestDocumentExportOutput, ToolRefusal> {
        require_open_project(&self.app)?;
        // RDX-FR-DVQL.
        let document = args.document.trim();
        if document.is_empty() {
            return Err(ToolRefusal::InvalidArguments(BLANK_DOCUMENT));
        }
        // RDX-FR-POVJ.
        let format = format_of(args.format.as_deref())?;
        // RDX-FR-AROQ.
        if args.message.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(BLANK_MESSAGE));
        }
        if comments::spoken_part(&args.message).is_none() {
            return Err(ToolRefusal::InvalidArguments(NO_SPOKEN_PART));
        }
        // RDX-FR-RRJQ: any DOC artifact of the project.
        let path = (self.resolve)(&self.app, document).map_err(refusal_for_resolve)?;
        // RDX-FR-TMYF: the loop has already removed the citation markers from
        // every string argument (CVL-FR-VSSU).
        let input = requests::ExportRequestInput {
            path: &path,
            format,
            message: &args.message,
            agent: &self.author,
            origin: &self.origin,
        };
        requests::record_with(&self.app, self.buffer, &self.roots.store, input).map_err(
            |error| {
                // RDX-FR-NOBI / RDX-FR-IDWS.
                if error == requests::ERR_DISCUSSION_RESOLVED {
                    ToolRefusal::Denied {
                        message: CONVERSATION_RESOLVED,
                        code: "conversation_resolved",
                        retryable: false,
                    }
                } else {
                    ToolRefusal::NotRecordedWith(NOT_RECORDED)
                }
            },
        )?;
        // CVL-FR-15: set only after the record and its comment both landed.
        self.requested.store(true, Ordering::SeqCst);
        Ok(RequestDocumentExportOutput { requested: true })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for RequestDocumentExportTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = RequestDocumentExportArgs;
    type Output = RequestDocumentExportOutput;
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
        // RDX-FR-PZGN: it records and returns; nothing waits.
        std::future::ready(self.run(arguments))
    }
}
