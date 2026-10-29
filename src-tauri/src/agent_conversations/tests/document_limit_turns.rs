//! The context figures and the document limit as a whole turn sees them: the
//! split's routing, the tool's binding at dispatch, compaction, usage, and the
//! estimate's edge cases (AGC-FR-JFBZ, AGC-FR-OBUA, AGC-FR-QCVC, CVL-FR-OPTI,
//! CVL-FR-AAPA, CVL-FR-HLVG, MDT-FR-EEFA).
//!
//! One part of `../tests/mod.rs`; the helpers are `document_limit.rs`'s.

use super::document_limit::{
    artifact_discussion, artifact_fragment, numbered_lines, window_harness,
};
use super::*;

fn agent_id_of(h: &Harness, nickname: &str) -> String {
    agents::list_project_agents_impl(&h.store(), PROJECT_KEY, &h.root())
        .unwrap()
        .into_iter()
        .find(|p| p.agent.nickname == nickname)
        .unwrap()
        .agent
        .id
}

// CVL-FR-AAPA: each part of the split is the estimate of its own section —
// the `artifact` body is the document, the addressing comment is the current
// comment — and the instructions count the tool definitions.
#[test]
fn the_split_routes_each_section_to_its_own_part() {
    let h = window_harness(vec![Ok(ScriptedReply::answer("Done."))], Some(100_000), 30);
    let text = numbered_lines(40);
    let thread = artifact_discussion(&h, "hld.md", &text);
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    let terminal = wait_for_terminal(&h, &turn.id);
    let split = terminal.context.expect("figures").split;

    let (request, _) = &h.seam.requests()[0];
    let tokens = |chars: usize| (chars as u64).div_ceil(4);
    let body = |tag: &str| section_of(&request.input, tag).body.chars().count();
    assert_eq!(split.document, tokens(body(TAG_ARTIFACT)));
    assert_eq!(split.current_comment, tokens(body(TAG_CURRENT_COMMENT)));
    assert_eq!(split.history, tokens(body(TAG_DISCUSSION_HISTORY)));
    let definitions: usize =
        request.tools.iter().map(|t| serde_json::to_string(t).unwrap().chars().count()).sum();
    assert!(definitions > 0);
    assert_eq!(split.instructions, tokens(request.instructions.chars().count() + definitions));
    assert_eq!(split.tool_results, 0, "a single call holds no tool result");
}

// MDT-FR-EEFA, MDT-FR-CTKN, TLC-FR-15: the tool a turn holds is bound to that
// turn's window and to the stored percent, which no argument can change.
#[test]
fn the_measure_tool_holds_the_turns_window_and_percent() {
    let h = window_harness(
        vec![
            Ok(ScriptedReply::calls("measure_document", serde_json::json!({ "path": "hld.md" }))),
            Ok(ScriptedReply::answer("Measured.")),
        ],
        Some(100_000),
        7,
    );
    let text = numbered_lines(40);
    let thread = artifact_discussion(&h, "hld.md", &text);
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    let exchanges = h.seam.exchanges();
    let results = tool_results(exchanges.last().expect("a second call"));
    let measured: serde_json::Value = serde_json::from_str(&results[0]).expect("data");
    assert_eq!(measured["limit_tokens"], 7_000, "100,000 × 7 / 100");
    assert_eq!(measured["lines"], 40);
    assert_eq!(measured["characters"], 800);
    assert_eq!(measured["estimated_tokens"], 200);
    assert_eq!(measured["above_limit"], false);
}

// CVL-FR-OPTI: a compaction call reports no context figures; the two answered
// calls of the turn report one each.
#[test]
fn a_compaction_call_reports_no_context() {
    // The first call carries a passage of 400 characters and stays under the
    // compaction budget of this small window (CVL-FR-EBJG). The whole file it
    // reads puts the second call over it, so the loop compacts before it.
    let h = window_harness(
        vec![
            Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "hld.md" }))),
            Ok(ScriptedReply::answer("The earlier rounds read hld.md.")),
            Ok(ScriptedReply::answer("Done.")),
        ],
        Some(10_000),
        1,
    );
    // A fragment discussion opens its history with the fragment, which is the
    // older history compaction folds (CVL-FR-LCXM).
    let thread = artifact_fragment(&h, "hld.md", &numbered_lines(1_000), 105, 110);
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 3, "two answered calls and one compaction call");
    let reports = h.context_events.lock().unwrap().iter().filter(|t| t.id == turn.id).count();
    assert_eq!(reports, 2, "the compaction call reports nothing");
}

// CVL-FR-HLVG, AGC-FR-NUKP: the context reports add no usage record, so a
// draft's statistics count each call's reported input once.
#[test]
fn the_context_figures_add_no_usage_record() {
    let h = window_harness(
        vec![
            Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "none.md" }))
                .routed_by("upstream", 111)),
            Ok(ScriptedReply::answer("Done.").routed_by("upstream", 222)),
        ],
        Some(100_000),
        30,
    );
    h.app.state::<ProjectState>().set_root(h.root.path().to_path_buf());
    let created = crate::drafts::create_draft_at_root(&h.root(), Some("usage")).expect("draft");
    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
        None,
        "@arch what now?".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion");
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&discussion), &discussion.comments[0].id)
        .unwrap();
    assert_eq!(wait_for_terminal(&h, &turn.id).state, AgentTurnState::Delivered);
    let reports = h.context_events.lock().unwrap().iter().filter(|t| t.id == turn.id).count();
    assert_eq!(reports, 2);
    crate::statistics::wait_for_writer();
    let stats =
        crate::statistics::read_statistics_impl(&h.root(), &created.draft.id).expect("statistics");
    assert_eq!(stats.totals.conversation_tokens.input.value, Some(333), "each call once");
}

// AGC-FR-JFBZ: the estimate makes no model call.
#[test]
fn the_estimate_reaches_no_model() {
    let h = window_harness(Vec::new(), Some(10_000), 30);
    let thread = artifact_discussion(&h, "hld.md", &numbered_lines(10));
    let agent = agent_id_of(&h, "arch");
    let context = estimate_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        thread.target.clone(),
        Some(&thread.id),
        Some(&agent),
    );
    assert_eq!(context.window, Some(10_000));
    assert_eq!(h.seam.call_count(), 0);
    assert!(h.seam.requests().is_empty());
}

// AGC-FR-OBUA: an agent the project resolves no provider for gives no window,
// and so no limit and no warning.
#[test]
fn the_estimate_of_an_agent_with_no_provider_has_no_window() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let thread = artifact_discussion(&h, "hld.md", &numbered_lines(200));
    let agent = agent_id_of(&h, "arch");
    h.store().save_ai_api_registry(Vec::new(), None).unwrap();
    let context = estimate_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        thread.target.clone(),
        Some(&thread.id),
        Some(&agent),
    );
    assert_eq!(context.window, None);
    assert_eq!(context.limit_tokens, None);
    assert!(!context.above_limit);
    assert!(context.document.is_some(), "the document is still measured");
}

// AGC-FR-QCVC: a file that does not decode as UTF-8 gives no document and no
// warning, and the estimate still answers.
#[test]
fn the_estimate_of_a_file_that_is_not_utf8_has_no_document() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    std::fs::write(h.root().path().join("bad.md"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let context = estimate_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        DiscussionTarget::Artifact { artifact_id: "bad.md".into() },
        None,
        None,
    );
    assert_eq!(context.document, None);
    assert_eq!(context.split.document, 0);
    assert!(!context.above_limit);
}
