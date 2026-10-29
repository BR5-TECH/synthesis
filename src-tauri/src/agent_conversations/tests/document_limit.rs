//! The document size limit, the shapes of the `artifact` section, the context
//! figures of a turn, and the estimate before any turn (AGC-FR-GMSF …
//! AGC-FR-QCVC, CVL-FR-05, CVL-FR-OPTI, CVL-FR-AAPA, CVL-FR-HLVG).

use super::*;

/// A Custom provider whose model `m` has `window` as its detected context
/// size, an agent on it, and the Document size limit set to `percent`.
pub(super) fn window_harness(
    replies: Vec<Result<ScriptedReply, &'static str>>,
    window: Option<u64>,
    percent: i64,
) -> Harness {
    let h = Harness::scripted(replies);
    h.mount();
    h.seed_second_provider("custom");
    let mut records = h.store().load_ai_api_registry().unwrap().0;
    let custom = records.iter_mut().find(|r| r.provider == "custom").unwrap();
    custom.models =
        vec![ModelOption { detected_context_size: window, ..ModelOption::new("m", "m") }];
    h.store().save_ai_api_registry(records, Some("custom".into())).unwrap();
    h.store().save_document_limit_percent(percent).unwrap();
    h.create_plain_agent("arch");
    h
}

/// `count` numbered lines of 20 characters each, the line feed included.
pub(super) fn numbered_lines(count: usize) -> String {
    (0..count).map(|i| format!("line {i:06} of text\n")).collect()
}

/// A fragment discussion on `artifact_id` over the characters `start..end`.
pub(super) fn artifact_fragment(
    h: &Harness,
    artifact_id: &str,
    text: &str,
    start: usize,
    end: usize,
) -> Discussion {
    std::fs::write(h.root().path().join(artifact_id), text).expect("artifact");
    let quote: String = text.chars().skip(start).take(end - start).collect();
    crate::comments::open_artifact_fragment_in(
        &h.root(),
        artifact_id,
        FragmentTarget::in_artifact("", start, end, &quote),
        "@arch what about this?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("thread")
}

/// A whole-target discussion of `artifact_id`.
pub(super) fn artifact_discussion(h: &Harness, artifact_id: &str, text: &str) -> Discussion {
    std::fs::write(h.root().path().join(artifact_id), text).expect("artifact");
    crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &DiscussionTarget::Artifact { artifact_id: artifact_id.into() },
        None,
        "@arch what now?".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion")
}

fn attribute(section: &InputSection, name: &str) -> Option<String> {
    section.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
}

// AGC-FR-GMSF, AGC-FR-XHHT: the limit is the whole part of window × percent /
// 100, and the bound is four characters per token, or 40,000 characters where
// no window is known.
#[test]
fn the_limit_and_the_bound_follow_the_window_and_the_percent() {
    let budget = DocumentBudget { window: Some(200_000), percent: 30 };
    assert_eq!(budget.limit_tokens(), Some(60_000));
    assert_eq!(budget.bound_chars(), 240_000);
    let odd = DocumentBudget { window: Some(999), percent: 30 };
    assert_eq!(odd.limit_tokens(), Some(299), "rounded down");
    let unknown = DocumentBudget { window: None, percent: 30 };
    assert_eq!(unknown.limit_tokens(), None);
    assert_eq!(unknown.bound_chars(), SECTION_MAX_CHARS);
    assert_eq!(DocumentBudget::unknown().bound_chars(), SECTION_MAX_CHARS);
}

// AGC-FR-GMSF: dispatch resolves the window of the agent's model on the
// provider and the stored percent.
#[test]
fn the_budget_is_resolved_from_the_registry_and_the_setting() {
    let h = window_harness(Vec::new(), Some(10_000), 7);
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    assert_eq!(budget, DocumentBudget { window: Some(10_000), percent: 7 });
    assert_eq!(DocumentBudget::resolve(&h.store(), "custom", "no-such").window, None);
}

// AGC-FR-OYVH: the size is counted on the whole text, and the estimate is
// the characters divided by four, rounded up.
#[test]
fn a_document_size_counts_lines_characters_and_the_estimate() {
    let size = DocumentSize::of("ab\ncd\ne");
    assert_eq!((size.lines, size.characters, size.estimated_tokens), (3, 7, 2));
    assert_eq!(DocumentSize::of(""), DocumentSize::default());
    assert_eq!(estimate_tokens(8), 2);
    assert_eq!(estimate_tokens(9), 3);
}

// AGC-FR-NEQL, AGC-FR-VAIY: at or below the bound the text is whole; above it
// with no fragment there is no text.
#[test]
fn a_document_fits_whole_or_not_at_all_without_a_fragment() {
    let text = numbered_lines(10);
    assert_eq!(fit(&text, None, 200), Fit::Whole, "exactly at the bound is whole");
    assert_eq!(fit(&text, Some((0, 3)), 200), Fit::Whole);
    assert_eq!(fit(&text, None, 199), Fit::MetadataOnly);
}

// AGC-FR-BORR: the passage holds the fragment's whole lines, then whole lines
// on both sides while the bound allows.
#[test]
fn the_passage_grows_whole_lines_around_the_fragment() {
    let text = numbered_lines(100);
    // The fragment sits in line 50 (0-based), characters 1005..1010.
    let Fit::Passage { start, end, first_line, last_line } = fit(&text, Some((1005, 1010)), 100)
    else {
        panic!("a passage");
    };
    assert!(first_line <= 50 && last_line >= 50, "the fragment's line is held");
    let passage = &text[start..end];
    assert!(passage.chars().count() <= 100, "within the bound");
    assert_eq!(last_line - first_line + 1, 5, "five lines of 20 characters fit in 100");
    assert!(passage.starts_with("line ") && passage.ends_with('\n'), "whole lines");
    assert!((50 - first_line).abs_diff(last_line - 50) <= 1, "both sides");

    // At the head of the document the passage grows downward only.
    let Fit::Passage { first_line, .. } = fit(&text, Some((0, 4)), 100) else { panic!() };
    assert_eq!(first_line, 0);
}

// AGC-FR-IJSE: a fragment whose own lines exceed the bound gives a passage
// that starts at its first line and ends at the bound.
#[test]
fn a_fragment_larger_than_the_bound_is_cut_at_the_bound() {
    let text = format!("head\n{}\ntail\n", "ж".repeat(500));
    let Fit::Passage { start, end, first_line, .. } = fit(&text, Some((5, 400)), 100) else {
        panic!("a passage");
    };
    assert_eq!(first_line, 1);
    assert_eq!(text[start..end].chars().count(), 100);
    assert_eq!(&text[start..end], "ж".repeat(100));
}

// AGC-FR-BORR, AGC-FR-IJSE, AGC-FR-07: a fragment discussion of a document
// above the bound carries the passage, the size attributes, and `truncated`.
#[test]
fn a_fragment_discussion_above_the_bound_carries_the_passage() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let text = numbered_lines(200);
    let thread = artifact_fragment(&h, "hld.md", &text, 2005, 2010);
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &budget,
    );
    let artifact = section_of(&built.sections, TAG_ARTIFACT);
    assert!(artifact.truncated);
    assert_eq!(attribute(artifact, "truncated").as_deref(), Some("true"));
    assert_eq!(attribute(artifact, "lines"), Some("200".into()));
    assert_eq!(attribute(artifact, "characters"), Some("4000".into()));
    assert_eq!(attribute(artifact, "estimated_tokens"), Some("1000".into()));
    assert_eq!(attribute(artifact, "path").as_deref(), Some("hld.md"));
    assert!(artifact.body.starts_with("[This is a passage of a larger document of 200 lines"));
    assert!(artifact.body.contains("line 000100 of text"), "the fragment's line is held");
    // AGC-FR-IJSE: the numbers count from 1, so the passage's first text line,
    // which holds `line i`, is line i + 1.
    let Fit::Passage { first_line, last_line, .. } =
        fit(&text, Some((2005, 2010)), budget.bound_chars())
    else {
        panic!("a passage");
    };
    assert!(artifact.body.contains(&format!("lines {} to {}.", first_line + 1, last_line + 1)));
    let (_, passage) = artifact.body.split_once("]\n\n").expect("the lead line");
    assert!(passage.starts_with(&format!("line {first_line:06} of text")));
    assert!(!artifact.body.contains("line 000000 of text"), "the head is not");
    assert_eq!(built.document, Some(DocumentSize::of(&text)));
}

// AGC-FR-IJSE: a fragment on the first line gives lines 1 to 20, and one on
// the last line gives lines 181 to 200 (twenty lines of 20 characters fit in
// a bound of 400).
#[test]
fn the_passage_names_its_lines_from_one_at_both_ends() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let text = numbered_lines(200);
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    for (path, start, end, expected, first_text) in [
        ("head.md", 0, 4, "lines 1 to 20.", "line 000000 of text"),
        ("tail.md", 3985, 3990, "lines 181 to 200.", "line 000180 of text"),
    ] {
        let thread = artifact_fragment(&h, path, &text, start, end);
        let built = build_input_within(
            Roots::same(&h.root()),
            &ConversationOrigin::of(&thread),
            &thread.comments[0].id,
            &budget,
        );
        let artifact = section_of(&built.sections, TAG_ARTIFACT);
        assert!(
            artifact.body.contains(expected),
            "{path}: {}",
            artifact.body.lines().next().unwrap()
        );
        let (_, passage) = artifact.body.split_once("]\n\n").expect("the lead line");
        assert!(passage.starts_with(first_text), "{path}");
    }
}

// AGC-FR-VAIY, AGC-FR-07: a whole-target discussion of a document above the
// bound carries its size and no text.
#[test]
fn a_whole_target_discussion_above_the_bound_carries_metadata_only() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let text = numbered_lines(200);
    let thread = artifact_discussion(&h, "hld.md", &text);
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &budget,
    );
    let artifact = section_of(&built.sections, TAG_ARTIFACT);
    assert!(artifact.truncated);
    assert!(artifact.parts.is_empty(), "no image");
    assert!(!artifact.body.contains("of text"), "no text of the document");
    assert!(artifact.body.contains("200 lines and 4000 characters"));
    assert_eq!(artifact.body.lines().count(), 1, "one line");
    assert_eq!(attribute(artifact, "path").as_deref(), Some("hld.md"));
    assert_eq!(attribute(artifact, "truncated").as_deref(), Some("true"));
    assert_eq!(attribute(artifact, "lines").as_deref(), Some("200"));
    assert_eq!(attribute(artifact, "characters").as_deref(), Some("4000"));
    assert_eq!(attribute(artifact, "estimated_tokens").as_deref(), Some("1000"));

    // The type stands beside the size, as it does for a whole document.
    std::fs::create_dir_all(h.root().path().join(".claude/skills/big")).unwrap();
    let skill = artifact_discussion(&h, ".claude/skills/big/SKILL.md", &text);
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&skill),
        &skill.comments[0].id,
        &budget,
    );
    let typed = section_of(&built.sections, TAG_ARTIFACT);
    assert!(typed.truncated);
    assert_eq!(attribute(typed, "type").as_deref(), Some("skill"));
    assert_eq!(attribute(typed, "path").as_deref(), Some(".claude/skills/big/SKILL.md"));

    // AGC-FR-NEQL: the same document under a larger window goes in whole,
    // with no size attribute.
    let whole = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &DocumentBudget { window: Some(10_000_000), percent: 30 },
    );
    let artifact = section_of(&whole.sections, TAG_ARTIFACT);
    assert!(!artifact.truncated);
    assert_eq!(artifact.body, text);
    assert_eq!(attribute(artifact, "lines"), None);
}

// AGC-FR-XHHT: with a known window the limit replaces the 40,000-character
// bound for the `artifact` section, and the other sections keep it.
#[test]
fn a_large_window_lifts_the_artifact_bound_alone() {
    let h = window_harness(Vec::new(), Some(1_000_000), 30);
    let text = "x".repeat(SECTION_MAX_CHARS * 2);
    let thread = artifact_discussion(&h, "big.md", &text);
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &budget,
    );
    let artifact = section_of(&built.sections, TAG_ARTIFACT);
    assert!(!artifact.truncated);
    assert_eq!(artifact.body.chars().count(), SECTION_MAX_CHARS * 2);

    // The comment that addressed the agent is longer than 40,000 characters,
    // and its section is cut there under the same large window.
    let long = format!("@arch {}", "y".repeat(SECTION_MAX_CHARS + 500));
    let talk = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &DiscussionTarget::Artifact { artifact_id: "big.md".into() },
        None,
        long,
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion");
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&talk),
        &talk.comments[0].id,
        &budget,
    );
    assert!(!section_of(&built.sections, TAG_ARTIFACT).truncated);
    let comment = section_of(&built.sections, TAG_CURRENT_COMMENT);
    assert!(comment.truncated, "the other sections keep the 40,000-character bound");
    assert!(comment.body.chars().count() < SECTION_MAX_CHARS + 500);
}

// AGC-FR-JCPO, AGC-FR-VAIY: a draft prompt's image is carried in a passage
// only where its reference lies inside it.
#[test]
fn only_the_images_inside_the_passage_are_carried() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let created = crate::drafts::create_draft_at_root(&h.root(), Some("big")).expect("draft");
    let draft_id = created.draft.id.clone();
    let store = |name: &str| {
        crate::draft_assets::store_image_impl(
            &h.root(),
            &draft_id,
            "image/png",
            Some(&format!("{name}.png")),
            &super::images::encoded(&super::images::png_bytes(name)),
            &|_: &str| {},
        )
        .expect("stored")
        .reference
    };
    let (near, far) = (store("near"), store("far"));
    let body = format!("![near]({near})\n{}![far]({far})\n", numbered_lines(200));
    crate::drafts::save_draft_file_impl(&h.root(), &draft_id, &created.file, &body).unwrap();
    let thread = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &DiscussionTarget::Draft { draft_id: draft_id.clone() },
        Some(FragmentTarget::in_draft(&draft_id, &created.file, 0, 5, "![nea")),
        "@arch this one".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("thread");
    let budget = DocumentBudget::resolve(&h.store(), "custom", "m");
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &budget,
    );
    let artifact = section_of(&built.sections, TAG_ARTIFACT);
    let images: Vec<&InputImage> = artifact
        .parts
        .iter()
        .filter_map(|p| match p {
            InputPart::Image(image) => Some(image),
            InputPart::Text(_) => None,
        })
        .collect();
    assert_eq!(images.len(), 1, "the image inside the passage alone");
    assert_eq!(images[0].data, super::images::encoded(&super::images::png_bytes("near")));
    assert!(!artifact.body.contains(&far), "the image outside contributes nothing");

    // The whole draft above the bound: metadata only, no image at all.
    let (_, discussion) = seed_discussion(&h, &["@arch what now?"]);
    let draft_of = match &discussion.target {
        DiscussionTarget::Draft { draft_id } => draft_id.clone(),
        _ => unreachable!(),
    };
    let record = crate::drafts::read_draft_record(&h.root(), &draft_of).unwrap();
    crate::drafts::save_draft_file_impl(
        &h.root(),
        &draft_of,
        record.prompt_path.as_deref().unwrap(),
        &body,
    )
    .unwrap();
    let built = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
        &budget,
    );
    let artifact = section_of(&built.sections, TAG_ARTIFACT);
    assert!(artifact.parts.is_empty());
    assert!(!artifact.body.contains("near") && !artifact.body.contains("far"));
    assert_eq!(attribute(artifact, "path"), record.prompt_path.clone());
    assert_eq!(attribute(artifact, "truncated").as_deref(), Some("true"));
}

// AGC-FR-NJXW, AGC-FR-BCHZ, AGC-FR-NUKP, CVL-FR-OPTI, CVL-FR-AAPA, CVL-FR-JBTX: a turn
// holds the figures of its latest answered call and keeps them when it ends.
#[test]
fn a_turn_holds_the_context_figures_of_its_latest_call() {
    // The window keeps the tool definitions below the compaction budget (CVL-FR-EBJG).
    let h = window_harness(
        vec![
            Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "hld.md" }))
                .routed_by("upstream", 111)),
            Ok(ScriptedReply::answer("Done.").routed_by("upstream", 222)),
        ],
        Some(100_000),
        1,
    );
    let text = numbered_lines(300);
    let thread = artifact_fragment(&h, "hld.md", &text, 2005, 2010);
    let turn = h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let context = terminal.context.expect("AGC-FR-NUKP: the terminal turn keeps its figures");
    assert_eq!(context.window, Some(100_000));
    assert_eq!(context.input_tokens, Some(222), "the latest call's reported input");
    assert_eq!(context.limit_tokens, Some(1_000));
    assert_eq!(context.document, Some(DocumentSize::of(&text)));
    assert!(context.above_limit, "1500 estimated tokens against a limit of 1000");
    let split = context.split;
    assert!(split.instructions > 0 && split.document > 0);
    assert!(split.history > 0 && split.current_comment > 0);
    assert!(split.tool_results > 0, "the read_file result of the first round");

    // CVL-FR-OPTI: one report per answered call.
    let reports: Vec<AgentTurn> =
        h.context_events.lock().unwrap().iter().filter(|t| t.id == turn.id).cloned().collect();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].context.as_ref().unwrap().input_tokens, Some(111));
    assert_eq!(reports[0].context.as_ref().unwrap().split.tool_results, 0);
}

// CVL-FR-OPTI: a failed attempt reports nothing.
#[test]
fn a_failed_attempt_reports_no_context() {
    let h = window_harness(
        vec![Err(FAIL_UNREACHABLE), Ok(ScriptedReply::answer("Done."))],
        Some(10_000),
        30,
    );
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let reports = h.context_events.lock().unwrap().iter().filter(|t| t.id == terminal.id).count();
    assert_eq!(reports, 1, "the answered attempt alone");
    let context = terminal.context.unwrap();
    assert_eq!(context.input_tokens, None, "a call that reported no usage");
    assert!(!context.above_limit);
}

// AGC-FR-BCHZ, AGC-FR-NJXW: a note turn carries a null document and is never
// above the limit; with no window there is no limit.
#[test]
fn a_note_turn_has_no_document_and_no_limit_without_a_window() {
    let h = window_harness(vec![Ok(ScriptedReply::answer("Noted."))], None, 30);
    let (_, origin) =
        seed_note_discussion(&h, crate::notes::NoteScope::Project, "A gap.", "@arch thoughts?");
    let discussion = resolve_origin(Roots::same(&h.root()), &origin).unwrap();
    let turn = h.dispatch("arch", origin, &discussion.comments[0].id).unwrap();
    let terminal = wait_for_terminal(&h, &turn.id);
    let context = terminal.context.expect("figures for a note turn too");
    assert_eq!(context.document, None);
    assert_eq!(context.window, None);
    assert_eq!(context.limit_tokens, None);
    assert!(!context.above_limit);
    assert!(context.split.document > 0, "the note_context section");
}

// CVL-FR-HLVG, AGC-FR-NUKP: the figures are numbers alone and are written
// nowhere.
#[test]
fn the_context_figures_are_numbers_and_never_reach_disk() {
    let h = window_harness(vec![Ok(ScriptedReply::answer("Done."))], Some(10_000), 30);
    let (terminal, _) = run_one(&h, "arch");
    let json = serde_json::to_value(terminal.context.unwrap()).unwrap();
    fn leaves_are_numbers(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => map.values().all(leaves_are_numbers),
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) | serde_json::Value::Null => {
                true
            }
            _ => false,
        }
    }
    assert!(leaves_are_numbers(&json), "no text in the figures: {json}");
    for (path, bytes) in crate::tools::tests::tree_snapshot(h.root().path()) {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("toolResults") && !text.contains("tool_results"), "{path}");
    }
}

// AGC-FR-JFBZ, AGC-FR-OBUA: the estimate before any turn — the agent's window,
// the split, an empty current comment, and no tool results; nothing is
// registered, emitted, or recorded.
#[test]
fn the_estimate_assembles_the_input_dry() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let agent_id = agents::list_project_agents_impl(&h.store(), PROJECT_KEY, &h.root())
        .unwrap()
        .into_iter()
        .find(|p| p.agent.nickname == "arch")
        .unwrap()
        .agent
        .id;
    let text = numbered_lines(200);
    let thread = artifact_discussion(&h, "hld.md", &text);
    let target = thread.target.clone();
    let handle = h.app.handle().clone();
    let events_before = h.events.lock().unwrap().len();

    let with = estimate_impl(
        &handle,
        Roots::same(&h.root()),
        PROJECT_KEY,
        target.clone(),
        Some(&thread.id),
        Some(&agent_id),
    );
    assert_eq!(with.window, Some(10_000));
    assert_eq!(with.limit_tokens, Some(100));
    assert!(with.above_limit);
    assert_eq!(with.document, Some(DocumentSize::of(&text)));
    assert_eq!(with.split.current_comment, 0);
    assert_eq!(with.split.tool_results, 0);
    assert!(with.split.history > 0 && with.split.instructions > 0 && with.split.document > 0);
    assert_eq!(with.estimated_tokens, with.split.total());

    // AGC-FR-OBUA: no discussion gives an empty history; no agent gives no
    // window and so no limit.
    let bare =
        estimate_impl(&handle, Roots::same(&h.root()), PROJECT_KEY, target.clone(), None, None);
    assert_eq!(bare.split.history, 0);
    assert_eq!(bare.window, None);
    assert_eq!(bare.limit_tokens, None);
    assert!(!bare.above_limit);
    let unknown = estimate_impl(
        &handle,
        Roots::same(&h.root()),
        PROJECT_KEY,
        target,
        Some("no-such"),
        Some("no-such-agent"),
    );
    assert_eq!((unknown.window, unknown.split.history), (None, 0));

    // AGC-FR-JFBZ: nothing registered, nothing emitted.
    assert!(h.turns().in_flight(None).is_empty());
    assert_eq!(h.events.lock().unwrap().len(), events_before);
}

// AGC-FR-QCVC: a document that cannot be read gives no document and no
// warning, and the estimate still answers.
#[test]
fn the_estimate_of_an_unreadable_document_has_no_document() {
    let h = window_harness(Vec::new(), Some(10_000), 1);
    let target = DiscussionTarget::Artifact { artifact_id: "missing.md".into() };
    let context = estimate_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        target,
        None,
        None,
    );
    assert_eq!(context.document, None);
    assert_eq!(context.split.document, 0);
    assert!(!context.above_limit);
}

// AGC-FR-QCVC: the command refuses with `no_project_open` when no project is
// open.
#[test]
fn the_estimate_command_needs_an_open_project() {
    let h = Harness::scripted(Vec::new());
    let result = estimate_discussion_context(
        DiscussionTarget::Artifact { artifact_id: "a.md".into() },
        None,
        None,
        h.app.handle().clone(),
        h.app.state::<ProjectState>(),
    );
    assert_eq!(result, Err(ERR_NO_PROJECT_OPEN.to_string()));
}

// CVL-FR-05: the prompts whose material is `artifact` say that the section
// can hold a passage or only the document's size, and that the rest is
// material the agent may gather.
#[test]
fn the_prompts_say_the_material_can_be_a_passage_or_its_size() {
    for kind in
        [OriginKind::ArtifactComment, OriginKind::ArtifactDiscussion, OriginKind::DraftDiscussion]
    {
        let prompt = compile_prompt(kind, "", "").to_lowercase();
        assert!(prompt.contains("too large to send whole"), "{kind:?}");
        assert!(prompt.contains("material you may gather"), "{kind:?}");
    }
    let comment = compile_prompt(OriginKind::ArtifactComment, "", "").to_lowercase();
    assert!(comment.contains("passage around the fragment"));
    for kind in [OriginKind::ArtifactDiscussion, OriginKind::DraftDiscussion] {
        assert!(compile_prompt(kind, "", "").to_lowercase().contains("only the size"));
    }
}
