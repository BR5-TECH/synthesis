//! Types, path rules, and typed errors of the Documents collection
//! (`DCL-documents-collection.md`).
//!
//! Everything here is pure: no filesystem, no Tauri. The serialised field names
//! are the snake_case names of the contract's "Payload shapes", so a payload
//! crosses the IPC boundary exactly as the specification writes it.

use serde::{Deserialize, Serialize};

/// DCL-FR-TYOU: what a source selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    File,
    Folder,
}

impl SourceKind {
    /// The kind as a log field.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::File => "file",
            SourceKind::Folder => "folder",
        }
    }
}

/// DCL-FR-TYOU / GSS-FR-CDYK: one stored reference, `{ kind, path }`. The path is
/// absolute and kept exactly as the user selected it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSource {
    pub kind: SourceKind,
    pub path: String,
}

/// DCL-FR-FGGU: the format of a supported document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    Pdf,
    Markdown,
    Text,
}

impl DocumentFormat {
    /// The format as a log field and as the tools report it.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "pdf",
            DocumentFormat::Markdown => "markdown",
            DocumentFormat::Text => "text",
        }
    }
}

/// The `status` of a source and of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Available,
    Unavailable,
}

/// DCL-FR-EWPO: why a source is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceReason {
    Missing,
    Unreadable,
    Link,
}

impl SourceReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceReason::Missing => "missing",
            SourceReason::Unreadable => "unreadable",
            SourceReason::Link => "link",
        }
    }
}

/// A source as a snapshot reports it (DCL contract: `DocumentSource`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSource {
    pub kind: SourceKind,
    pub path: String,
    pub status: Availability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<SourceReason>,
}

/// A document as a snapshot reports it (DCL contract: `DocumentEntry`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEntry {
    /// `doc-` and 32 hexadecimal characters (DCL-FR-QKJO).
    pub id: String,
    /// The normalised absolute path.
    pub path: String,
    /// The file name.
    pub name: String,
    pub format: DocumentFormat,
    pub status: Availability,
    /// The SHA-256 of the content, in lowercase hexadecimal. Absent while the
    /// document is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// DCL-FR-SQEP: the sources in stored order and the documents sorted by path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentsSnapshot {
    pub sources: Vec<DocumentSource>,
    pub documents: Vec<DocumentEntry>,
}

/// DCL-FR-VXXI: what `pick_document_sources` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickDocumentSourcesResult {
    pub cancelled: bool,
    pub ignored_count: usize,
    pub snapshot: DocumentsSnapshot,
}

/// DCL-FR-NCBQ: what `read_document` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentText {
    pub id: String,
    pub name: String,
    pub format: DocumentFormat,
    pub text: String,
    pub revision: String,
}

/// DCL-FR-TEUK: what `read_document_pdf` answers with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentPdf {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub bytes_base64: String,
}

/// The `mode` of `pick_document_sources` (DCL-FR-VXXI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickMode {
    Files,
    Folder,
}

/// The typed errors of the module (DCL contract surface, DCL-FR-GLUS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentsError {
    /// DCL-FR-MSHW: no project is open.
    NoProjectOpen,
    /// DCL-FR-MSHW: the stored sources could not be read.
    StoreUnavailable,
    /// DCL-FR-PHYP: a path that is not absolute.
    InvalidPath,
    /// The id is not in the collection.
    UnknownDocument,
    /// The document cannot be read.
    Unavailable,
    /// The document is not of the format the operation serves.
    WrongFormat,
    /// DCL-FR-GLUS: a PDF with no extractable text. Internal only.
    NoText,
}

impl DocumentsError {
    /// The code the frontend sees: a snake_case string, as every typed error of
    /// this backend is carried.
    pub fn code(self) -> &'static str {
        match self {
            DocumentsError::NoProjectOpen => "no_project_open",
            DocumentsError::StoreUnavailable => "store_unavailable",
            DocumentsError::InvalidPath => "invalid_path",
            DocumentsError::UnknownDocument => "unknown_document",
            DocumentsError::Unavailable => "unavailable",
            DocumentsError::WrongFormat => "wrong_format",
            DocumentsError::NoText => "no_text",
        }
    }
}

impl std::fmt::Display for DocumentsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for DocumentsError {}

impl From<DocumentsError> for String {
    fn from(error: DocumentsError) -> String {
        error.code().to_string()
    }
}

// ---------------------------------------------------------------------------
// Paths (DCL-FR-QKJO, DCL-FR-FGGU)
// ---------------------------------------------------------------------------

/// Whether `path` is absolute in either path syntax: a leading separator, or a
/// drive letter followed by a separator. Judged on the text alone, so the answer
/// does not depend on the platform the application runs on.
pub fn is_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if matches!(bytes.first(), Some(b'/') | Some(b'\\')) {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

/// DCL-FR-QKJO: normalise a path without consulting the disk.
///
/// `\` becomes `/`, `.` segments and repeated separators vanish, `..` removes
/// the segment before it (and never climbs above the root), and a trailing `/`
/// is dropped. A drive letter stays at the front of the result.
pub fn normalise_path(path: &str) -> String {
    let text = path.replace('\\', "/");
    let mut prefix = String::new();
    let mut rest = text.as_str();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        prefix.push_str(&text[..2]);
        rest = &text[2..];
    }
    let rooted = rest.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if matches!(segments.last(), Some(last) if *last != "..") {
                    segments.pop();
                } else if !rooted {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    if rooted {
        format!("{prefix}/{joined}")
    } else if prefix.is_empty() {
        joined
    } else {
        format!("{prefix}{joined}")
    }
}

/// DCL-FR-QKJO: `doc-` and the first 32 lowercase hexadecimal characters of the
/// SHA-256 of the normalised path.
pub fn document_id(normalised_path: &str) -> String {
    let digest = crate::fs::sha256_bytes(normalised_path.as_bytes());
    format!("doc-{}", &digest[..32])
}

/// The last segment of a normalised path.
pub fn file_name_of(normalised_path: &str) -> &str {
    normalised_path.rsplit('/').next().unwrap_or(normalised_path)
}

/// DCL-FR-FGGU: the format a file name's extension names, ignoring case, or
/// `None` for a file the collection does not support.
pub fn format_of(file_name: &str) -> Option<DocumentFormat> {
    let (stem, extension) = file_name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    match extension.to_ascii_lowercase().as_str() {
        "pdf" => Some(DocumentFormat::Pdf),
        "md" | "markdown" => Some(DocumentFormat::Markdown),
        "txt" => Some(DocumentFormat::Text),
        _ => None,
    }
}
