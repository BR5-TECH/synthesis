//! DST-FR-15 … DST-FR-18: refusals and empty results.

use super::*;

// ---------------------------------------------------------------------------
// DST-FR-15 … DST-FR-18: refusals and empty results
// ---------------------------------------------------------------------------

/// DST-FR-15: an empty or blank query is a retryable `InvalidArgs` refusal, not
/// an empty list — the two say opposite things to a model.
#[test]
fn blank_query_refuses_rather_than_returning_nothing() {
    let fixture = DraftFixture::new();
    fixture.draft("present", "# Plan\n\nteardown\n");
    fixture.reindex();

    for query in ["", "   ", "\n\t"] {
        let error = block_on(fixture.tool().call(DraftSearchArgs {
            query: query.into(),
            limit: None,
        }))
        .expect_err("a blank query refuses");
        assert_eq!(error, ToolRefusal::InvalidArguments(EMPTY_DRAFT_QUERY));
        let execution = error.to_execution_error();
        assert_eq!(execution.kind(), rig::tool::ToolErrorKind::InvalidArgs);
        assert_eq!(execution.retryable(), Some(true));
    }
}

/// DST-FR-16: with no project open the shared refusal, not the empty list the
/// index would answer with.
#[test]
fn no_open_project_refuses() {
    let app = crate::tools::tests::closed_project();
    let tool = DraftSearchTool::new(app.handle().clone(), "closed");
    let error = block_on(tool.call(DraftSearchArgs {
        query: "teardown".into(),
        limit: None,
    }))
    .expect_err("a closed project refuses");
    assert_eq!(error, ToolRefusal::NoProjectOpen);
    let execution = error.to_execution_error();
    assert_eq!(execution.kind(), rig::tool::ToolErrorKind::NotFound);
    assert_eq!(execution.retryable(), Some(false), "no argument opens a project");
}

/// DST-FR-17: nothing matching, and a worktree holding no draft at all, are
/// each a success carrying an empty list.
#[test]
fn no_match_is_an_empty_success() {
    let fixture = DraftFixture::new();
    fixture.draft("present", "# Plan\n\nteardown\n");
    fixture.reindex();
    assert!(fixture.search("nonexistentterm", None).is_empty());

    let bare = DraftFixture::new();
    assert!(
        bare.search("teardown", None).is_empty(),
        "a worktree with no draft succeeds rather than refusing"
    );
}

/// DST-FR-18: a call made while a build is still in flight answers from what has
/// been indexed so far rather than blocking.
///
/// [`block_on`] panics on a `Pending`, so every call in this file already
/// asserts the non-blocking half; this one covers the un-indexed project.
#[test]
fn never_blocks_on_an_index_pass() {
    let fixture = DraftFixture::new();
    fixture.draft("unindexed", "# Plan\n\nteardown\n");
    // No reindex at all: the drafts index has never seen this prompt.
    assert!(
        fixture.search("teardown", None).is_empty(),
        "answers from the index as it stands"
    );
    fixture.reindex();
    assert_eq!(fixture.search("teardown", None).len(), 1);
}

/// DST-FR-19: a rewritten prompt reaches the tool on the next pass, in both
/// directions, and a deleted draft leaves.
#[test]
fn follows_the_index_in_every_direction() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("evolving", "# Plan\n\nteardown sequence\n");
    fixture.reindex();
    assert_eq!(fixture.search("teardown", None).len(), 1);

    let root = fixture.root();
    crate::drafts::save_draft_file_impl(&root, &id, "evolving.md", "# Plan\n\nsarcophagus only\n")
        .expect("the prompt writes");
    fixture.reindex();
    assert!(
        fixture.search("teardown", None).is_empty(),
        "the superseded text stops matching"
    );
    let m = fixture.search("sarcophagus", None).remove(0);
    assert!(m.excerpt.contains("sarcophagus only"));
    assert!(
        !m.excerpt.contains("teardown"),
        "no text of the older version survives in the excerpt"
    );

    crate::drafts::delete_draft_impl(&root, &root, &id).expect("deletes");
    fixture.reindex();
    assert!(fixture.search("sarcophagus", None).is_empty());
}
