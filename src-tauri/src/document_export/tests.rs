//! Tests for the source checks and the run of an export
//! (`DXP-document-export.md`).

use std::sync::Mutex;

use tempfile::TempDir;

use super::commands::{run_export, ExportJob, ExportProgress};
use super::convert::Format;
use super::source::{check_doc_artifact, resolve_identifier};
use super::*;
use crate::fs::RootFs;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::scanning::{self, ArtifactType, Scope};

/// A worktree with `docs/guide.md` typed `doc` and `notes/plain.md` untyped.
struct Project {
    _dir: TempDir,
    root: RootFs,
    out: TempDir,
}

impl Project {
    fn new() -> Project {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = RootFs::for_root(&path);
        root.write_text_atomic("docs/guide.md", "---\ntitle: Guide\n---\n# Guide\n\nBody text.\n")
            .unwrap();
        root.write_text_atomic("docs/data.txt", "plain\n").unwrap();
        root.write_text_atomic("notes/plain.md", "# Plain\n").unwrap();
        scanning::assign(&root, "docs", ArtifactType::Doc, Scope::Folder).unwrap();
        let out = TempDir::new().expect("out dir");
        Project { _dir: dir, root, out }
    }

    fn job(&self, path: &str, destination: String) -> ExportJob {
        ExportJob {
            path: path.into(),
            format: Format::Docx,
            include_frontmatter: false,
            destination,
            export_id: "e-1".into(),
        }
    }

    fn destination(&self, name: &str) -> String {
        self.out.path().join(name).to_string_lossy().into_owned()
    }
}

fn no_diagram(_: &str) -> Option<Vec<u8>> {
    None
}

fn run(
    buffer: &'static LogBuffer,
    root: Option<&RootFs>,
    job: &ExportJob,
) -> (Result<commands::ExportResult, &'static str>, Vec<ExportProgress>) {
    let app = tauri::test::mock_app();
    let seen = Mutex::new(Vec::new());
    let emit = |p: ExportProgress| seen.lock().unwrap().push(p);
    let outcome = run_export(app.handle(), buffer, root, job, &no_diagram, &emit);
    (outcome, seen.into_inner().unwrap())
}

fn backend_records(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    let filter = LogFilter {
        min_level: LogLevel::Debug,
        domains: vec![Domain::Backend],
        ..LogFilter::default()
    };
    buffer.query(&filter, None, 1000).unwrap().records
}

// DXP-FR-LNCQ: a DOC artifact passes, and each other file is its own typed
// error.
#[test]
fn the_source_must_be_a_doc_artifact_in_markdown_inside_the_project() {
    let p = Project::new();
    assert_eq!(check_doc_artifact(&p.root, "docs/guide.md"), Ok("docs/guide.md".into()));
    assert_eq!(check_doc_artifact(&p.root, "./docs/../docs/guide.md"), Ok("docs/guide.md".into()));
    assert_eq!(check_doc_artifact(&p.root, "docs/missing.md"), Err(ERR_NOT_FOUND));
    assert_eq!(check_doc_artifact(&p.root, "docs/data.txt"), Err(ERR_NOT_MARKDOWN));
    assert_eq!(check_doc_artifact(&p.root, "notes/plain.md"), Err(ERR_NOT_DOC_ARTIFACT));
    assert_eq!(check_doc_artifact(&p.root, "../elsewhere.md"), Err(ERR_OUTSIDE_PROJECT));
    assert_eq!(check_doc_artifact(&p.root, "/etc/passwd.md"), Err(ERR_OUTSIDE_PROJECT));
}

// DXP-FR-PKDB: an id the Documents collection does not hold is not found, and
// a path is checked as DXP-FR-LNCQ states.
#[test]
fn an_identifier_is_a_path_or_a_known_documents_id() {
    let p = Project::new();
    let unknown = format!("doc-{}", "a".repeat(32));
    assert_eq!(resolve_identifier(&p.root, &unknown, None), Err(ERR_NOT_FOUND));
    assert_eq!(resolve_identifier(&p.root, " docs/guide.md ", None), Ok("docs/guide.md".into()));
    assert_eq!(resolve_identifier(&p.root, "notes/plain.md", None), Err(ERR_NOT_DOC_ARTIFACT));
}

// DXP-FR-IEQA, DXP-FR-XRAI, DXP-FR-LSYA, DXP-FR-NGUG: an export reads the file
// on disk, writes one file at the destination, reports the stages in order,
// and logs a start and an end that carry no path and no content.
#[test]
fn an_export_writes_the_destination_and_logs_no_path() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let destination = p.destination("guide.docx");
    let (outcome, progress) =
        run(&BUFFER, Some(&p.root), &p.job("docs/guide.md", destination.clone()));
    let result = outcome.expect("exported");
    assert_eq!(result.destination, destination);
    assert!(result.warnings.is_empty());
    let bytes = std::fs::read(&destination).expect("the file is written");
    assert!(bytes.starts_with(b"PK"), "a DOCX is a zip archive");
    let stages: Vec<&str> = progress.iter().map(|e| e.stage).collect();
    let mut order = stages.clone();
    order.dedup();
    assert_eq!(order, vec!["reading", "diagrams", "converting", "writing"]);
    assert!(progress.iter().all(|e| e.export_id == "e-1"));

    let records = backend_records(&BUFFER);
    let messages: Vec<&str> = records.iter().map(|r| r.message.as_str()).collect();
    assert!(messages.contains(&"document export started"));
    assert!(messages.contains(&"document export finished"));
    let text = serde_json::to_string(&records).unwrap();
    assert!(!text.contains("guide"), "no file name or path: {text}");
    assert!(!text.contains("Body text"), "no content");
}

// DXP-FR-XRAI: a destination that is not absolute is refused, and nothing is
// written.
#[test]
fn a_relative_destination_is_refused() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let (outcome, _) =
        run(&BUFFER, Some(&p.root), &p.job("docs/guide.md", "out/guide.docx".into()));
    assert_eq!(outcome, Err(ERR_INVALID_DESTINATION));
    assert!(!p.root.path().join("out").exists());
}

// DXP-FR-LHEV, DXP-FR-NGUG: with no project open the export fails with
// `no_project_open`, writes nothing, and logs one error with the typed code.
#[test]
fn no_open_project_is_refused_and_logged() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let destination = p.destination("guide.docx");
    let (outcome, progress) = run(&BUFFER, None, &p.job("docs/guide.md", destination.clone()));
    assert_eq!(outcome, Err(ERR_NO_PROJECT_OPEN));
    assert!(progress.is_empty());
    assert!(!std::path::Path::new(&destination).exists());
    let records = backend_records(&BUFFER);
    let failed = records.iter().find(|r| r.message == "document export failed").expect("logged");
    assert_eq!(failed.level, LogLevel::Error);
    assert!(serde_json::to_string(failed).unwrap().contains(ERR_NO_PROJECT_OPEN));
}

// DXP-FR-LNCQ, DXP-FR-LHEV: an export of a file that is not a DOC artifact
// fails with its typed error and writes nothing.
#[test]
fn an_export_of_another_file_writes_nothing() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let destination = p.destination("plain.docx");
    let (outcome, _) = run(&BUFFER, Some(&p.root), &p.job("notes/plain.md", destination.clone()));
    assert_eq!(outcome, Err(ERR_NOT_DOC_ARTIFACT));
    assert!(!std::path::Path::new(&destination).exists());
}
