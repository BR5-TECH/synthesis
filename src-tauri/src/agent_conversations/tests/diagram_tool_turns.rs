//! The turns of the tool `propose_diagram_changes`: which origins are offered
//! it, which turns are not, that a call of it ends the turn, and what the
//! prompts tell the agent about it (`PDM-propose-diagram-changes-tool.md`
//! PDM-FR-ZXQE, PDM-FR-JLYU, PDM-FR-TCDX; `CVL-conversation-loop.md` CVL-FR-08,
//! CVL-FR-15, CVL-FR-HNGB; `AGC-agent-conversations.md` AGC-FR-DIZM).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::agent_groups::*;
use super::*;

const DIAGRAM_TOOL: &str = crate::tools::propose_diagram_changes::NAME;

const PROMPT_PATH: &str = "prompts/flow.md";
const DRAFT_PATH: &str = "artifact-window.md";
/// A file with one Mermaid diagram.
const FENCED: &str =
    "# Flow\n\nHow it runs.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd of file.\n";
const SOURCE: &str = "flowchart TD\n  A --> B";
const NEW_SOURCE: &str = "flowchart TD\n  A --> C";

/// The call of the diagram tool that changes the diagram of `FENCED`.
fn diagram_call(path: &str, after: &str) -> serde_json::Value {
    serde_json::json!({
        "path": path,
        "rationale": said("The arrow should reach C."),
        "diagrams": [{ "before": SOURCE, "after": after }],
    })
}

/// A prompt artifact that holds `FENCED`, with a discussion that addresses
/// `arch`, typed by a file-scope assignment so its type is `prompt`.
fn seed_prompt_with_diagram(h: &Harness) -> Discussion {
    let root = h.root();
    std::fs::create_dir_all(root.path().join("prompts")).expect("dirs");
    std::fs::write(root.path().join(PROMPT_PATH), FENCED).expect("prompt");
    std::fs::create_dir_all(root.path().join(".synthesis")).expect("dirs");
    std::fs::write(
        root.path().join(".synthesis/library.toml"),
        format!("[assignments.\"{PROMPT_PATH}\"]\ntype = \"prompt\"\nscope = \"file\"\n"),
    )
    .expect("assignment");
    crate::comments::open_discussion_in(
        &root,
        &root,
        &crate::comments::DiscussionTarget::Artifact { artifact_id: PROMPT_PATH.into() },
        None,
        "@arch what would you change?".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion")
}

/// A draft whose prompt holds `FENCED`, with a discussion that addresses `arch`.
fn seed_draft_with_diagram(h: &Harness) -> (String, Discussion) {
    let (draft_id, discussion) = seed_discussion(h, &["@arch what would you change?"]);
    crate::drafts::save_draft_file_impl(&h.root(), &draft_id, DRAFT_PATH, FENCED).expect("write");
    (draft_id, discussion)
}

fn names_of(request: &AgentRequest) -> Vec<String> {
    request.tools.iter().map(|t| t.name.to_string()).collect()
}

fn count_of(request: &AgentRequest) -> usize {
    names_of(request).iter().filter(|name| name.as_str() == DIAGRAM_TOOL).count()
}

// CVL-FR-15, CVL-FR-HNGB: a call of the diagram tool ends the turn, and the
// advisor turn does not carry it.
#[test]
fn the_diagram_tool_ends_the_turn_and_advisors_lack_it() {
    assert!(crate::agent_conversations::turn_body::ends_turn(DIAGRAM_TOOL));
    assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS.contains(&DIAGRAM_TOOL));
    // The tools that only gather end nothing.
    assert!(!crate::agent_conversations::turn_body::ends_turn("read_file"));
}

// CVL-FR-08, PDM-FR-ZXQE: a draft origin and an artifact origin, discussion and
// comment, are each offered the diagram tool once. A note origin is not.
#[test]
fn the_diagram_tool_is_offered_to_draft_and_artifact_origins_alone() {
    let h = Harness::new(vec![Ok("Answered.".into()), Ok("Answered.".into())]);
    h.mount();
    h.create_agent("arch", "");

    let (draft_id, discussion) = seed_discussion(&h, &["@arch thoughts?"]);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&discussion), &discussion.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let comment = seed_draft_thread(&h, &draft_id, DRAFT_PATH);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&comment), &comment.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let requests: Vec<AgentRequest> = h.seam.requests().into_iter().map(|(r, _)| r).collect();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(count_of(request), 1, "a draft origin: {:?}", names_of(request));
    }

    let artifact = Harness::new(vec![Ok("Answered.".into())]);
    artifact.mount();
    artifact.create_agent("arch", "");
    let thread = artifact.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = artifact
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&artifact, &turn.id);
    let requests = artifact.seam.requests();
    assert_eq!(count_of(&requests[0].0), 1, "an artifact origin");

    // A note origin has no file to propose to, so the tool is not attached.
    let note = Harness::new(vec![Ok("Answered.".into())]);
    note.mount();
    note.create_agent("arch", "");
    let (_, origin) =
        seed_note_discussion(&note, crate::notes::NoteScope::Project, "A gap.", "@arch thoughts?");
    let discussion = resolve_origin(Roots::same(&note.root()), &origin).expect("discussion");
    let turn =
        note.dispatch("arch", origin, &discussion.comments[0].id).expect("dispatch");
    wait_for_terminal(&note, &turn.id);
    let requests = note.seam.requests();
    assert_eq!(count_of(&requests[0].0), 0, "a note origin: {:?}", names_of(&requests[0].0));
}

// CVL-FR-15, PDM-FR-JLYU: a call of the diagram tool on a draft that records the
// proposal ends the turn on that call, awaiting the author, with one comment
// that carries the proposal and no file rewritten.
#[test]
fn a_diagram_proposal_on_a_draft_ends_the_turn() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(DIAGRAM_TOOL, diagram_call(DRAFT_PATH, NEW_SOURCE))),
        // Never reached: the turn ends on the proposal.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (draft_id, thread) = seed_draft_with_diagram(&h);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(turn.failure, None);
    assert_eq!(h.seam.requests().len(), 1, "no further model call once the proposal was recorded");

    let thread = crate::comments::fold_discussion(&h.root(), &draft_id)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments.as_slice(),
        [crate::comments::Attachment::Proposal { .. }]
    ));
    let proposals =
        crate::draft_proposals::list_proposals_impl(&h.root(), &draft_id).expect("list");
    assert_eq!(proposals.len(), 1);
    assert!(proposals[0].ledger.iter().all(|row| row.diagram));
    assert_eq!(
        crate::drafts::load_draft_file_impl(&h.root(), &draft_id, DRAFT_PATH)
            .expect("file")
            .body,
        FENCED,
        "a turn writes no file of the draft",
    );
}

// CVL-FR-15, PDM-FR-JLYU: the same on an artifact origin, and a refusal ends
// nothing. The source that does not parse is refused with the parse error, the
// loop goes on, and the corrected call ends the turn.
#[test]
fn a_refused_diagram_call_ends_nothing_and_the_corrected_call_ends_the_turn() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(DIAGRAM_TOOL, diagram_call(PROMPT_PATH, "this is not a diagram"))),
        Ok(ScriptedReply::calls(DIAGRAM_TOOL, diagram_call(PROMPT_PATH, NEW_SOURCE))),
        // Never reached: the turn ends on the proposal.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let thread = seed_prompt_with_diagram(&h);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 2, "the refusal left the loop running");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(results.iter().any(|r| r.contains("not valid Mermaid")), "{results:?}");

    let proposals = crate::prompt_proposals::list_proposals_impl(&h.root(), PROMPT_PATH);
    assert_eq!(proposals.len(), 1, "the refused call recorded nothing");
    assert!(proposals[0].ledger.iter().all(|row| row.diagram));
    let thread = crate::comments::fold_artifact_discussion(&h.root(), PROMPT_PATH)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments.as_slice(),
        [crate::comments::Attachment::PromptProposal { .. }]
    ));
    assert_eq!(
        std::fs::read_to_string(h.root().path().join(PROMPT_PATH)).expect("prompt"),
        FENCED,
        "a turn writes no file of the project",
    );
}

// CVL-FR-15: where the first proposal call of a reply refused, the second call
// is not dispatched and gets the one at a time refusal, and the loop goes on.
#[test]
fn a_second_call_after_a_refused_one_gets_the_one_at_a_time_refusal() {
    let reply =
        ScriptedReply::calls(DIAGRAM_TOOL, diagram_call(PROMPT_PATH, "this is not a diagram"))
            .and_calls(DIAGRAM_TOOL, diagram_call(PROMPT_PATH, NEW_SOURCE));
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("I could not propose."))]);
    h.mount();
    h.create_agent("arch", "");
    let thread = seed_prompt_with_diagram(&h);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.requests().len(), 2, "the loop went on after the refusal");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(
        results.iter().any(|r| r.contains(crate::tools::propose_diagram_changes::ONE_AT_A_TIME)),
        "{results:?}"
    );
    assert!(crate::prompt_proposals::list_proposals_impl(&h.root(), PROMPT_PATH).is_empty());
}

// CVL-FR-HNGB: an advisor turn is not offered the diagram tool, and the Meta
// turn that asked it is.
#[test]
fn an_advisor_turn_is_not_offered_the_diagram_tool() {
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            Ok(text_reply(said("Important points:\n- a point\nKey questions:\n- a question")))
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is it safe?", text_reply(said("Meta answer."))))
        }
    });
    let (h, thread) = group_harness(seam.clone());
    switch(&h, true, 60);
    dispatch_group(&h, &["arch"], &thread).expect("dispatch");
    h.settle();

    let requests = seam.requests();
    let advisors: Vec<&AgentRequest> =
        requests.iter().filter(|r| r.instructions.contains(ADVISOR_MARK)).collect();
    let metas: Vec<&AgentRequest> =
        requests.iter().filter(|r| !r.instructions.contains(ADVISOR_MARK)).collect();
    assert!(!advisors.is_empty(), "the advisors ran");
    assert!(!metas.is_empty(), "the Meta turn ran");
    for request in advisors {
        assert_eq!(count_of(request), 0, "an advisor: {:?}", names_of(request));
        assert!(!request.tools.is_empty(), "and still has its gathering tools");
    }
    for request in metas {
        assert_eq!(count_of(request), 1, "the Meta turn: {:?}", names_of(request));
    }
}

// AGC-FR-DIZM: an addressed agent of a synthesis group is not offered the
// diagram tool, because its offer would not be held for the group. The
// synthesis turn keeps it.
#[test]
fn an_addressed_agent_of_a_synthesis_group_lacks_the_diagram_tool_and_the_synthesis_has_it() {
    let seam = RoutedSeam::new(|r| {
        if r.instructions.contains(CLASSIFIER_MARK) {
            Ok("sec".into())
        } else if r.instructions.contains(SYNTHESIS_MARK) {
            Ok(said("The synthesis."))
        } else {
            Ok(said(format!("reply of {}", persona_of(r))))
        }
    });
    let (h, thread) = group_harness(Arc::clone(&seam));
    switch(&h, true, 60);
    let group = dispatch_group(&h, &["arch", "sec"], &thread).expect("dispatch");
    h.join_turns();
    assert_eq!(group.mode, "synthesis");

    let requests = seam.requests();
    let participants: Vec<&AgentRequest> = requests
        .iter()
        .filter(|r| {
            !r.instructions.contains(SYNTHESIS_MARK)
                && !r.instructions.contains(CLASSIFIER_MARK)
                && persona_of(r) != "NONE"
        })
        .collect();
    assert_eq!(participants.len(), 2, "the two addressed agents ran");
    for request in participants {
        assert_eq!(count_of(request), 0, "{}: {:?}", persona_of(request), names_of(request));
        // The tool of the origin that the group holds is still offered.
        assert!(names_of(request).iter().any(|n| n == "propose_prompt_changes"));
    }
    let synthesis = requests
        .iter()
        .find(|r| r.instructions.contains(SYNTHESIS_MARK))
        .expect("a synthesis turn ran");
    assert_eq!(count_of(synthesis), 1, "the synthesis turn: {:?}", names_of(synthesis));
}

// PDM-FR-TCDX: the three prompts of a draft target and of an artifact target
// tell the agent to use the diagram tool, and that a diagram change never goes
// in a text change and never in the same call as text changes. The tool is named
// in that subsection alone.
#[test]
fn the_document_prompts_name_the_diagram_tool_and_keep_diagrams_out_of_text_changes() {
    const HEADING: &str = "### Changes to a Mermaid diagram";
    for (name, template, text_tool) in [
        ("discuss-draft.md", DISCUSS_DRAFT_PROMPT_TEMPLATE, Some(DRAFT_ONLY_TOOL)),
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE, Some(ARTIFACT_ONLY_TOOL)),
        // A comment is on a draft or on an artifact, so the prompt names no one
        // text tool.
        ("comment.md", COMMENT_PROMPT_TEMPLATE, None),
    ] {
        let start = template.find(HEADING).unwrap_or_else(|| panic!("{name} has no {HEADING}"));
        let rest = &template[start + HEADING.len()..];
        let end = rest.find("\n#").unwrap_or(rest.len());
        let section = &rest[..end];
        let lower = section.to_lowercase();

        assert!(section.contains(&format!("`{DIAGRAM_TOOL}`")), "{name} names the tool");
        assert!(lower.contains("never goes in a text change"), "{name}: not in a text change");
        assert!(
            lower.contains("never in the same call as text changes"),
            "{name}: not in the same call as text changes",
        );
        assert!(lower.contains("valid mermaid"), "{name}: the new source must be valid");
        match text_tool {
            Some(tool) => assert!(section.contains(&format!("`{tool}`")), "{name} names {tool}"),
            None => {
                for tool in [DRAFT_ONLY_TOOL, ARTIFACT_ONLY_TOOL] {
                    assert!(!section.contains(tool), "{name} names no one text tool");
                }
                assert!(lower.contains("their own call"), "{name}: text changes go apart");
            }
        }

        // The tools are named in this subsection and nowhere else of the prompt.
        let elsewhere = format!("{}{}", &template[..start], &rest[end..]);
        for tool in [DIAGRAM_TOOL, DRAFT_ONLY_TOOL, ARTIFACT_ONLY_TOOL] {
            assert!(!elsewhere.contains(tool), "{name} names {tool} outside the subsection");
        }
    }
    // The prompt of a note has no file to propose to, so it names none of them.
    for tool in [DIAGRAM_TOOL, DRAFT_ONLY_TOOL, ARTIFACT_ONLY_TOOL] {
        assert!(!DISCUSS_NOTE_PROMPT_TEMPLATE.contains(tool), "discuss-note.md names {tool}");
    }
}
