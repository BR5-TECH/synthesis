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

fn no_diagram(
    _: convert::model::DiagramKind,
    _: &str,
) -> Result<Vec<u8>, convert::diagrams::DiagramFailure> {
    Err(convert::diagrams::DiagramFailure::Invalid)
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

// DXP-FR-PKDB: the `doc-…` id that a real Documents collection gives a DOC
// artifact of the project resolves to its project-relative path.
#[test]
fn a_documents_id_of_the_collection_resolves_to_its_project_path() {
    use crate::documents::{normalise_path, SourceKind, StoredSource, TypeSources};
    use tauri::Manager;
    let f = crate::tools::tests::document_fixture::DocFixture::new();
    let path = crate::changes::canonicalize_lenient(f.fixture._dir.path());
    let root = RootFs::for_root(&path);
    root.write_text_atomic("docs/guide.md", "# Guide\n").unwrap();
    scanning::assign(&root, "docs", ArtifactType::Doc, Scope::Folder).unwrap();
    let base = normalise_path(&path.to_string_lossy());
    let docs = normalise_path(&path.join("docs").to_string_lossy());
    let read_typed = move || TypeSources {
        sources: vec![StoredSource { kind: SourceKind::Folder, path: docs.clone() }],
        worktree_root: Some(base.clone()),
        is_doc: Box::new(|_| true),
    };
    let fs = f.fixture.app.state::<crate::fs::FsAccessState>();
    f.collection().refresh_with(&fs, &f.fixture.handle(), &read_typed, &|_| {});
    let snapshot = f.collection().snapshot().expect("a snapshot");
    let id = crate::documents::document_id(&normalise_path(
        &path.join("docs").join("guide.md").to_string_lossy(),
    ));
    assert!(id.starts_with("doc-") && id.len() == 36, "{id}");
    let entry = snapshot.documents.iter().find(|d| d.id == id).expect("the collection holds it");
    assert_eq!(entry.project_path.as_deref(), Some("docs/guide.md"));
    assert_eq!(resolve_identifier(&root, &id, Some(&snapshot)), Ok("docs/guide.md".into()));
}

/// A PNG of `width` × `height` pixels.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::new(width, height);
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

// DXP-FR-JYHX: a local image of the project, relative to the file, reaches
// both formats. A remote image and one outside the project show their alt text.
#[test]
fn a_local_image_reaches_both_formats() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    p.root.write_bytes_atomic("docs/img/shot.png", &png(30, 20)).unwrap();
    p.root
        .write_text_atomic(
            "docs/pictures.md",
            "# Pictures\n\n![a shot](img/shot.png)\n\n![remote one](https://example.test/x.png)\n\n![outside one](../../x.png)\n",
        )
        .unwrap();

    let docx = p.destination("pictures.docx");
    let (outcome, _) = run(&BUFFER, Some(&p.root), &p.job("docs/pictures.md", docx.clone()));
    outcome.expect("exported");
    let bytes = std::fs::read(&docx).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let media: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .filter(|name| name.starts_with("word/media/") && !name.ends_with('/'))
        .collect();
    assert_eq!(media.len(), 1, "only the local image is embedded: {media:?}");
    let xml = super::convert::tests::document_xml(&std::fs::read(&docx).unwrap());
    assert!(xml.contains("remote one"), "a remote image shows its alt text");
    assert!(xml.contains("outside one"), "an image outside the project shows its alt text");

    let mut job = p.job("docs/pictures.md", p.destination("pictures.pdf"));
    job.format = Format::Pdf;
    let (outcome, _) = run(&BUFFER, Some(&p.root), &job);
    outcome.expect("exported");
    let pdf = std::fs::read(p.destination("pictures.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(
        pdf.windows(b"/Subtype /Image".len()).any(|w| w == b"/Subtype /Image")
            || pdf.windows(b"/Subtype/Image".len()).any(|w| w == b"/Subtype/Image"),
        "the PDF holds an image object"
    );
    let text = pdf_extract::extract_text_from_mem(&pdf).unwrap_or_default();
    assert!(text.contains("remote one"), "{text}");
}

// DXP-FR-LNCQ: a source that exists and does not read is `read_failed`, not
// `not_found`, and the export writes nothing.
#[test]
fn a_source_that_does_not_read_is_read_failed() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    p.root.write_bytes_atomic("docs/broken.md", &[0xff, 0xfe, 0x00, 0xc3]).unwrap();
    let destination = p.destination("broken.docx");
    let (outcome, _) = run(&BUFFER, Some(&p.root), &p.job("docs/broken.md", destination.clone()));
    assert_eq!(outcome, Err(ERR_READ_FAILED));
    assert!(!std::path::Path::new(&destination).exists());
}

// DXP-FR-LHEV, DXP-FR-NGUG: a format that is not `pdf` or `docx` is
// `invalid_format`, and the command converts and writes nothing.
#[test]
fn an_unknown_format_is_invalid_format() {
    let p = Project::new();
    let app = tauri::test::mock_app();
    let destination = p.destination("guide.html");
    let outcome = tauri::async_runtime::block_on(commands::export_document(
        app.handle().clone(),
        "docs/guide.md".into(),
        "html".into(),
        false,
        destination.clone(),
        "e-1".into(),
        None,
    ));
    assert_eq!(outcome.unwrap_err(), ERR_INVALID_FORMAT);
    assert!(!std::path::Path::new(&destination).exists());
    assert_eq!(Format::parse("PDF"), Some(Format::Pdf));
    assert_eq!(Format::parse("Docx"), Some(Format::Docx));
}

// DXP-FR-TQCQ: the CALM sources of the saved file, each one time and in
// order, with one DEBUG record that holds the count and no source text.
#[test]
fn the_calm_sources_of_a_doc_artifact_are_listed() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let text = "# Guide\n\n```calm\n{\"secret-node\": 1}\n```\n\n- step\n\n  ```calm\n  {\"other\": 2}\n  ```\n\n```calm\n{\"secret-node\": 1}\n```\n";
    p.root.write_text_atomic("docs/arch.md", text).unwrap();
    let app = tauri::test::mock_app();
    let sources =
        commands::run_list_calm_sources(app.handle(), &BUFFER, Some(&p.root), "docs/arch.md");
    assert_eq!(sources, Ok(vec!["{\"secret-node\": 1}".to_string(), "{\"other\": 2}".to_string()]));
    let records = backend_records(&BUFFER);
    let listed: Vec<_> = records
        .iter()
        .filter(|r| r.message == "the CALM sources of a document export were listed")
        .collect();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].level, LogLevel::Debug);
    assert_eq!(listed[0].fields.get("sources"), Some(&serde_json::json!(2)));
    let all =
        serde_json::to_string(&records.iter().map(|r| &r.fields).collect::<Vec<_>>()).unwrap();
    assert!(!all.contains("secret-node") && !all.contains("arch.md"));
}

// DXP-FR-TQCQ: the list call has the checks and the typed errors of the
// export, and logs each failure with its typed error alone.
#[test]
fn the_calm_sources_need_a_doc_artifact_and_a_project() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let p = Project::new();
    let app = tauri::test::mock_app();
    let list = |root: Option<&RootFs>, path: &str| {
        commands::run_list_calm_sources(app.handle(), &BUFFER, root, path)
    };
    assert_eq!(list(None, "docs/guide.md"), Err(ERR_NO_PROJECT_OPEN));
    assert_eq!(list(Some(&p.root), "notes/plain.md"), Err(ERR_NOT_DOC_ARTIFACT));
    assert_eq!(list(Some(&p.root), "docs/data.txt"), Err(ERR_NOT_MARKDOWN));
    assert_eq!(list(Some(&p.root), "docs/missing.md"), Err(ERR_NOT_FOUND));
    assert_eq!(list(Some(&p.root), "../outside.md"), Err(ERR_OUTSIDE_PROJECT));
    assert_eq!(list(Some(&p.root), "docs/guide.md"), Ok(vec![]));
    p.root.write_bytes_atomic("docs/broken.md", &[0xff, 0xfe, 0x00, 0xc3]).unwrap();
    assert_eq!(list(Some(&p.root), "docs/broken.md"), Err(ERR_READ_FAILED));
    let warnings: Vec<_> = backend_records(&BUFFER)
        .into_iter()
        .filter(|r| r.message == "the CALM sources of a document export were not listed")
        .collect();
    assert_eq!(warnings.len(), 6);
    assert!(warnings.iter().all(|r| r.level == LogLevel::Warn && r.fields.len() == 1));
}
