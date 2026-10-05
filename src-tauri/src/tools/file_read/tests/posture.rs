//! RFT-FR-17, RFT-FR-18, RFT-FR-19: no project open, the disk as it stands, read-only.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-17 — no project open (RFT-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn no_project_open_produces_the_shared_refusal() {
    let app = closed_project();
    let tool = FileReadTool::new(app.handle().clone(), "closed");

    let refusal = block_on(tool.call(ReadFileArgs {
        path: "src/a.ts".to_string(),
        offset: None,
        limit: None,
    }))
    .expect_err("a closed project refuses (RFT-FR-17)");

    assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(!refusal.retryable());
    assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
}

// ---------------------------------------------------------------------------
// RFT-FR-18 — the disk as it stands, not the index (RFT-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn the_tool_reads_the_disk_rather_than_the_last_pass() {
    let project = project();
    let tool = project.tool();

    // Created since the last pass: readable although no scan has surfaced it.
    write(&project.root, "fresh.md", "written after the pass\n");
    assert_eq!(
        block_on(tool.call(ReadFileArgs {
            path: "fresh.md".to_string(),
            offset: None,
            limit: None,
        }))
        .expect("readable before any pass observes it (RFT-FR-18)"),
        "written after the pass\n",
    );

    // Deleted since the last pass: refused although the tree still lists it.
    let indexed = project
        .fixture
        .app
        .state::<Bm25Indexer>()
        .snapshot()
        .paths(crate::bm25_index::IndexId::Spec);
    let _ = indexed; // the pass's view, deliberately not consulted by the tool
    std::fs::remove_file(project.root.join("src/a.ts")).unwrap();
    assert_eq!(
        project.at("src/a.ts").expect_err("gone from disk"),
        ToolRefusal::FileNotFound,
    );
}

// ---------------------------------------------------------------------------
// RFT-FR-19 — read-only (RFT-FR-19)
// ---------------------------------------------------------------------------

#[test]
fn reading_changes_nothing_including_modification_times() {
    let project = project();
    let before = crate::tools::tests::tree_snapshot(&project.root);
    let mtimes: Vec<_> = ["src/a.ts", "long.txt", "crlf.txt"]
        .iter()
        .map(|p| std::fs::metadata(project.root.join(p)).unwrap().modified().unwrap())
        .collect();

    for path in ["src/a.ts", "long.txt", "crlf.txt", "multibyte.txt"] {
        let _ = project.at(path);
        let _ = project.read(ReadFileArgs {
            path: path.to_string(),
            offset: Some(1),
            limit: Some(2),
        });
    }

    assert_eq!(
        before,
        crate::tools::tests::tree_snapshot(&project.root),
        "reading leaves the project exactly as it was (RFT-FR-19)",
    );
    for (path, was) in ["src/a.ts", "long.txt", "crlf.txt"].iter().zip(mtimes) {
        assert_eq!(
            std::fs::metadata(project.root.join(path)).unwrap().modified().unwrap(),
            was,
            "{path}: a read does not touch the modification time (RFT-FR-19)",
        );
    }
}

