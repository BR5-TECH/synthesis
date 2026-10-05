//! SPS-FR-13, SPS-FR-14, SPS-FR-15: a blank query, no project open, and nothing matched.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-13 — a blank query refuses (SPS-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn a_blank_query_refuses_rather_than_answering_nothing_matched() {
    let fixture = spec_project();
    let indexer = fixture.app.state::<Bm25Indexer>();

    for text in ["", "   ", "\t\n "] {
        let refusal = search(
            &indexer,
            &SpecificationSearchArgs {
                query: text.to_string(),
                limit: None,
            },
        )
        .expect_err("a blank query is a refusal (SPS-FR-13)");

        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        assert_eq!(refusal.to_string(), crate::tools::EMPTY_SPEC_QUERY);
    }
}

// ---------------------------------------------------------------------------
// SPS-FR-14 — no project open (SPS-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn no_project_open_refuses_rather_than_returning_an_empty_list() {
    let app = closed_project();
    let tool = SpecSearchTool::new(app.handle().clone());

    let refusal = block_on(tool.call(SpecificationSearchArgs {
        query: "teardown".to_string(),
        limit: None,
    }))
    .expect_err("a closed project refuses (SPS-FR-14)");

    assert_eq!(refusal, crate::tools::ToolRefusal::NoProjectOpen);
    assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
    assert!(!refusal.retryable());
    assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
}

// ---------------------------------------------------------------------------
// SPS-FR-15 — nothing matched is a success (SPS-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn nothing_matched_and_no_specifications_are_both_successes() {
    let fixture = spec_project();
    assert!(
        query(&fixture, "zzzzunmatchablezzzz", None)
            .specifications
            .is_empty(),
        "a query matching nothing is an empty list (SPS-FR-15)",
    );

    let empty = mounted(TempDir::new().unwrap());
    assert!(
        query(&empty, "teardown", None).specifications.is_empty(),
        "a project holding no specification is an empty list (SPS-FR-15)",
    );
}

