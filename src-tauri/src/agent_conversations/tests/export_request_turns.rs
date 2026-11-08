//! A turn that requests a document export
//! (`RDX-request-document-export-tool.md`, `CVL-conversation-loop.md` CVL-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::asking_author::{folded_discussion, run_draft_turn};
use super::*;

fn requests_export(format: &str) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::request_document_export::NAME,
        serde_json::json!({
            "document": "docs/guide.md",
            "format": format,
            "message": said("Please export the guide."),
        }),
    )
}

/// Open the harness root as the project, with `docs/guide.md` typed `doc`.
fn open_project_with_a_doc(h: &Harness) {
    let root = h.root();
    root.write_text_atomic("docs/guide.md", "# Guide\n\nBody.\n").unwrap();
    crate::scanning::assign(
        &root,
        "docs",
        crate::scanning::ArtifactType::Doc,
        crate::scanning::Scope::Folder,
    )
    .unwrap();
    h.app.state::<ProjectState>().set_root(h.root.path().to_path_buf());
}

/// The export requests recorded under the harness root.
fn recorded_requests(h: &Harness) -> usize {
    std::fs::read_dir(h.root.path().join(crate::document_export::requests::REQUESTS_DIR))
        .map(|d| d.count())
        .unwrap_or(0)
}

// RDX-FR-PZGN / RDX-FR-DMLK / CVL-FR-15: a recorded export request ends the
// turn on the call, and a second call of the tool in the same reply records
// nothing.
#[test]
fn a_recorded_export_request_ends_the_turn_and_a_second_is_not_carried_out() {
    let reply = requests_export("pdf")
        .and_calls(
            crate::tools::request_document_export::NAME,
            serde_json::json!({
                "document": "docs/guide.md",
                "format": "docx",
                "message": said("And as Word."),
            }),
        )
        .with_text("I would also export the notes");
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    open_project_with_a_doc(&h);
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(thread.comments.len(), 2, "one contribution");
    match &thread.comments[1].attachments[0] {
        crate::comments::Attachment::ExportRequest { format, path, .. } => {
            assert_eq!(format.as_deref(), Some("pdf"));
            assert_eq!(path, "docs/guide.md");
        }
        other => panic!("an export request: {other:?}"),
    }
    assert!(!thread.comments.iter().any(|c| c.body.contains("also export")));
    assert_eq!(recorded_requests(&h), 1);
}

// RDX-FR-DMLK / CVL-FR-15: where the first call of the tool refused, the
// second call in the reply is not dispatched and gets the one at a time
// refusal, and the loop goes on.
#[test]
fn a_second_call_after_a_refused_one_gets_the_one_at_a_time_refusal() {
    let reply = requests_export("html").and_calls(
        crate::tools::request_document_export::NAME,
        serde_json::json!({
            "document": "docs/guide.md",
            "format": "pdf",
            "message": said("Please export the guide."),
        }),
    );
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("I could not."))]);
    h.mount();
    open_project_with_a_doc(&h);
    h.create_agent("arch", "");
    let (_turn, _draft_id, _thread) = run_draft_turn(&h, "arch");

    assert_eq!(h.seam.requests().len(), 2, "the loop went on after the refusal");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(
        results.iter().any(|r| r.contains(crate::tools::request_document_export::ONE_AT_A_TIME)),
        "{results:?}"
    );
    assert_eq!(recorded_requests(&h), 0, "nothing was recorded");
}
