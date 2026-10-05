//! The reach of the tool (GDT-FR-FZHF, GDT-FR-HCYY, GDT-FR-IFCA, GDT-FR-PXQL).

use super::*;
use crate::fs::FsError;

// GDT-FR-HCYY: the tool reads a selected document that lives outside the project,
// and the agent session's instance still refuses that path, so `read_file` gains
// no reach.
#[test]
fn an_external_document_is_read_without_widening_an_agent_session() {
    let (fixture, id) = with_document("outside the project");
    let project = fixture
        .fixture
        .app
        .state::<crate::bm25_index::Bm25Indexer>()
        .root()
        .unwrap();
    let session = crate::tools::tests::agent_session(&fixture.fixture.app, &project, "get-doc");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "outside the project");

    let state = fixture.fixture.app.state::<crate::fs::FsAccessState>();
    let agent = state.agent_session(&session).unwrap();
    let external = fixture.root.join("refs/doc.md");
    assert!(matches!(
        agent.read_bytes(&external),
        Err(FsError::EscapesAllowedRoots { .. })
    ));
    let reader = crate::tools::file_read::FileReadTool::new(fixture.fixture.handle(), session);
    let refused = block_on(reader.call(crate::tools::file_read::ReadFileArgs {
        path: external.to_string_lossy().into_owned(),
        offset: None,
        limit: None,
    }))
    .unwrap_err();
    assert!(!refused.retryable() || refused.kind() != ToolErrorKind::Other);
}

// GDT-FR-FZHF: a document that left the collection is unknown from the moment its
// last source is removed, and reads through the old id fail.
#[test]
fn a_removed_source_ends_the_reach_of_its_ids() {
    let (fixture, id) = with_document("still here");
    assert_eq!(get(&fixture, args(&id)).unwrap(), "still here");
    let fs = fixture.fixture.app.state::<crate::fs::FsAccessState>();
    fixture
        .collection()
        .remove_source(&fixture.root.join("refs").to_string_lossy(), &|_, _| Ok(()))
        .unwrap();
    fixture.collection().refresh(&fs, &fixture.fixture.handle(), &|_| {});
    assert_eq!(get(&fixture, args(&id)).unwrap_err(), ToolRefusal::DocumentNotFound);
    assert!(fixture.root.join("refs/doc.md").exists(), "the file is untouched");
}

// GDT-FR-PXQL: reading changes no file, no source, and no snapshot.
#[test]
fn the_tool_is_read_only() {
    let (fixture, id) = with_document("read me");
    let path = fixture.root.join("refs/doc.md");
    let before_time = std::fs::metadata(&path).unwrap().modified().unwrap();
    let snapshot = fixture.collection().snapshot().unwrap();
    let sources = fixture.collection().stored_sources();
    get(&fixture, args(&id)).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before_time);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "read me");
    assert_eq!(fixture.collection().snapshot().unwrap(), snapshot);
    assert_eq!(fixture.collection().stored_sources(), sources);
}

// GDT-FR-IFCA: the call resolves on one poll, so it waits on no pass and no
// refresh.
#[test]
fn the_call_never_blocks() {
    let (fixture, id) = with_document("text");
    // `block_on` panics when a tool's future is pending after one poll.
    assert!(get(&fixture, args(&id)).is_ok());
}
