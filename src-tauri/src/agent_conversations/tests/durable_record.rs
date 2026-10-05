//! The durable record a turn leaves behind.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// The durable record a turn leaves (AGC-FR-ZPKW, AGC-FR-YQMD, AGC-FR-BVNT,
// AGC-FR-LHRC)
// ---------------------------------------------------------------------------

/// The draft's statistics, after the asynchronous writer has caught up.
///
/// Production never waits for it — that is the whole of
/// `DSS-draft-statistics-storage.md` DSS-FR-TUMX — so a test that wants to read
/// what was written is the one thing that does.
fn refinement_statistics(h: &Harness, draft_id: &str) -> crate::statistics::DraftStatistics {
    crate::statistics::wait_for_writer();
    crate::statistics::read_statistics_impl(&h.root(), draft_id).expect("statistics")
}

/// A draft with a discussion on it, ready to be addressed.
///
/// The harness manages a `ProjectState` with no root — its tools resolve their
/// own — so the root is mounted here, which is what the statistics recorder
/// resolves the draft's log against.
fn seeded_draft(h: &Harness) -> (String, crate::comments::Discussion) {
    h.app
        .state::<ProjectState>()
        .set_root(h.root.path().to_path_buf());
    let created =
        crate::drafts::create_draft_at_root(&h.root(), Some("Onboarding sequence")).expect("draft");
    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Draft {
            draft_id: created.draft.id.clone(),
        },
        None,
        "@arch where should graduation live?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    (created.draft.id, discussion)
}

#[test]
fn agc_ts_kdqp_a_draft_turn_leaves_one_lifecycle_line_and_nothing_of_its_material() {
    // AGC-FR-26 / AGC-FR-YQMD, AGC-FR-LHRC, AGC-FR-ZPKW.
    let h = Harness::new(vec![Ok("Graduation belongs beside the draft.".into())]);
    h.create_agent("arch", "Argue about structure.");
    let (draft_id, discussion) = seeded_draft(&h);

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let stats = refinement_statistics(&h, &draft_id);
    // One model-reaching logical turn is one interaction, and its own execution
    // interval is the whole of the Refinement time it contributes.
    assert_eq!(stats.totals.ai_interactions.value, Some(1));
    assert_eq!(
        stats.totals.agent_time_ms.refinement.availability,
        crate::statistics::Availability::Available,
    );

    // AGC-FR-LHRC: no request, context, exchange, tool payload, comment body, or
    // part of the answer is copied into the statistics event.
    let path = crate::statistics::log_path(&h.root(), &draft_id).expect("path");
    let text = h.root().read_text(&path).expect("log");
    for forbidden in [
        "Graduation belongs",
        "Argue about structure",
        "where should graduation live",
        "Onboarding sequence",
    ] {
        assert!(
            !text.contains(forbidden),
            "a refinement statistics line must not carry {forbidden:?}",
        );
    }
}

#[test]
fn agc_ts_zfab_a_non_draft_turn_leaves_no_statistics_line_anywhere() {
    // AGC-FR-LHRC: an artifact turn has no draft to record
    // against, so no statistics log gains a line because of it.
    let h = Harness::new(vec![Ok("The second paragraph buries it.".into())]);
    h.create_agent("arch", "Argue about structure.");
    let (draft_id, _) = seeded_draft(&h);
    let artifact = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&artifact),
        &artifact.comments[0].id,
    )
    .expect("dispatch");
    h.settle();
    crate::statistics::wait_for_writer();

    // The draft's own log was never created, and its totals never moved.
    let stats = crate::statistics::read_statistics_impl(&h.root(), &draft_id).expect("statistics");
    assert_eq!(stats.boundary_at, None);
    assert_eq!(
        stats.totals.ai_interactions,
        crate::statistics::Measure::unavailable(),
    );
    assert!(!h
        .root()
        .path()
        .join(crate::statistics::STATISTICS_REL)
        .join(format!("{draft_id}.jsonl"))
        .exists());
}
