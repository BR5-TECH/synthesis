//! The tool loop: how a call the model asks for is dispatched, and what
//! the loop does with what it answers.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// The tool loop (CVL-FR-08 … CVL-FR-28)
// ---------------------------------------------------------------------------


#[test]
fn each_origin_kind_is_offered_the_nine_and_at_most_one_proposal_tool() {
    // CVL-FR-30 / CVL-FR-08. Two agents across the four origin kinds: the set is
    // decided by the origin kind and by nothing else, and each tool's definition
    // is byte-identical everywhere it appears.
    let mut draft_seen: Vec<Vec<rig::completion::ToolDefinition>> = Vec::new();
    for nickname in ["arch", "sec"] {
        let h = Harness::new(vec![Ok("Answered.".into()), Ok("Answered.".into())]);
        h.mount();
        h.create_agent(nickname, "");

        // A draft comment and a draft discussion, so both draft origin kinds.
        let (draft_id, discussion) = seed_discussion(&h, &[&format!("@{nickname} thoughts?")]);
        let turn = h
            .dispatch(
                nickname,
                ConversationOrigin::of(&discussion),
                &discussion.comments[0].id,
            )
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);

        let anchored = seed_draft_thread(&h, &draft_id, "artifact-window.md");
        let turn = h
            .dispatch(
                nickname,
                ConversationOrigin::of(&anchored),
                &anchored.comments[0].id,
            )
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);

        for (request, _) in h.seam.requests() {
            draft_seen.push(request.tools.clone());
        }
    }

    assert_eq!(draft_seen.len(), 4, "two agents x two draft origin kinds");
    // CVL-FR-08: the two draft origin kinds no longer get the same set. A
    // discussion is attached `ask_discussion_questions` and an anchored comment
    // is not, a passage under review being one remark rather than a set of open
    // points (ADQ-FR-LFDX). The dispatch order above is discussion then comment,
    // per agent.
    let discussion_expected = expected_tools(true, Some(DRAFT_ONLY_TOOL));
    let comment_expected = expected_tools(false, Some(DRAFT_ONLY_TOOL));
    for (index, definitions) in draft_seen.iter().enumerate() {
        let names: Vec<&str> = definitions.iter().map(|tool| tool.name.as_str()).collect();
        let (expected, what) = if index % 2 == 0 {
            (&discussion_expected, "a draft discussion")
        } else {
            (&comment_expected, "a draft comment")
        };
        assert_eq!(&names, expected, "{what} is offered the wrong set");
    }
    // And within one origin kind the set does not vary by agent.
    assert_eq!(draft_seen[0], draft_seen[2], "a draft discussion varies by agent");
    assert_eq!(draft_seen[1], draft_seen[3], "a draft comment varies by agent");

    every_artifact_turn_is_offered_the_nine_and_the_prompt_proposal_tool(&draft_seen[0]);
}

/// CVL-FR-08: the names one origin kind is offered, in the order
/// `conversation_tools` pushes them — the nine, then the discussion tool where
/// the origin is a discussion, then the proposal tool its store decides.
fn expected_tools(is_discussion: bool, proposal: Option<&'static str>) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = EXPECTED_TOOLS.to_vec();
    if is_discussion {
        names.push(DISCUSSION_ONLY_TOOL);
    }
    names.extend(proposal);
    names
}

/// The artifact half of CVL-FR-30, CVL-FR-08, taking the draft turn's definitions so the
/// nine shared by both can be compared byte-for-byte across the two sets.
fn every_artifact_turn_is_offered_the_nine_and_the_prompt_proposal_tool(
    from_a_draft_turn: &[rig::completion::ToolDefinition],
) {
    let mut seen: Vec<Vec<rig::completion::ToolDefinition>> = Vec::new();

    for nickname in ["arch", "sec"] {
        let h = Harness::new(vec![Ok("Answered.".into()), Ok("Answered.".into())]);
        h.mount();
        h.create_agent(nickname, "");

        // An artifact comment and an artifact discussion, so two origin kinds
        // and both compiled prompts are covered.
        let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
        let turn = h
            .dispatch(
                nickname,
                ConversationOrigin::of(&thread),
                &thread.comments[0].id,
            )
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);

        let discussion = crate::comments::open_discussion_in(
            &h.root(),
            &h.root(),
            &crate::comments::DiscussionTarget::Artifact {
                artifact_id: "spec.md".into(),
            },
            None,
            format!("@{nickname} thoughts?"),
            Vec::new(),
            &human("ada"),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        let turn = h
            .dispatch(
                nickname,
                ConversationOrigin::of(&discussion),
                &discussion.comments[0].id,
            )
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);

        for (request, _) in h.seam.requests() {
            seen.push(request.tools.clone());
        }
    }

    assert_eq!(seen.len(), 4, "two agents x two artifact origin kinds");
    // CVL-FR-08: the dispatch order above is comment then discussion, per agent,
    // and only the discussion is attached `ask_discussion_questions`.
    let comment_expected = expected_tools(false, Some(ARTIFACT_ONLY_TOOL));
    let discussion_expected = expected_tools(true, Some(ARTIFACT_ONLY_TOOL));
    for (index, definitions) in seen.iter().enumerate() {
        let names: Vec<&str> = definitions.iter().map(|tool| tool.name.as_str()).collect();
        let (expected, what) = if index % 2 == 0 {
            (&comment_expected, "an artifact comment")
        } else {
            (&discussion_expected, "an artifact discussion")
        };
        assert_eq!(&names, expected, "{what} is offered the wrong set");
        // CVL-FR-30, CVL-FR-08: no turn of any kind carries both proposal tools.
        assert!(
            !names.contains(&DRAFT_ONLY_TOOL),
            "the two proposal tools are never attached together: {names:?}",
        );
        // CVL-FR-08: a tool's definition is the same wherever it appears, so the
        // nine an artifact turn gets are byte-identical to the nine inside the
        // ten a draft turn gets.
        assert_eq!(
            &definitions[..EXPECTED_TOOLS.len()],
            &from_a_draft_turn[..EXPECTED_TOOLS.len()],
            "the shared nine are byte-identical across origin kinds",
        );
        for tool in definitions {
            assert!(!tool.description.is_empty(), "a tool carries its description");
            assert!(tool.parameters.is_object(), "a schema is an object");
        }
    }
    assert_eq!(seen[0], seen[2], "an artifact comment varies by agent");
    assert_eq!(seen[1], seen[3], "an artifact discussion varies by agent");

    // CVL-FR-08: an agent's own definition carries nothing that names a tool,
    // which is what makes the set fixed rather than merely uniform today.
    const AGENTS_SOURCE: &str = include_str!("../../agents.rs");
    let draft = AGENTS_SOURCE
        .split_once("pub struct AgentDraft")
        .expect("the stored draft")
        .1
        .split_once('}')
        .expect("its body")
        .0;
    assert!(
        !draft.contains("tool"),
        "an agent's definition names no tool: {draft}",
    );
}

#[test]
fn a_turn_loops_until_the_model_answers_and_each_call_carries_the_whole_exchange() {
    // CVL-FR-12, AGC-FR-16.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "search_specifications",
            serde_json::json!({ "query": "teardown" }),
        )),
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(ScriptedReply::answer("The editor spec settles this.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 3, "two tool rounds and one answer");

    let exchanges = h.seam.exchanges();
    // CVL-FR-12: the exchange opens with the input alone and grows by one entry
    // per reply and one per tool result.
    assert_eq!(exchanges[0].len(), 1, "the first call carries the input alone");
    assert_eq!(exchanges[1].len(), 3, "input, the reply, and the tool result");
    assert_eq!(exchanges[2].len(), 5, "and again for the second round");

    // Each round's history is a prefix of the next: nothing was dropped or
    // rewritten between calls.
    assert_eq!(&exchanges[1][..1], &exchanges[0][..]);
    assert_eq!(&exchanges[2][..3], &exchanges[1][..]);

    // The tools genuinely ran: a result came back for each call.
    let results = exchanges[2]
        .iter()
        .filter(|message| matches!(
            message,
            rig::completion::Message::User { content }
                if content.iter().any(|part| matches!(
                    part,
                    rig::completion::message::UserContent::ToolResult(_)
                ))
        ))
        .count();
    assert_eq!(results, 2, "one result per tool the model asked for");

    // AGC-FR-16 / CVL-FR-26: one comment, carrying the final answer alone.
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[1].body, "The editor spec settles this.");
}

#[test]
fn several_tools_asked_for_at_once_are_all_dispatched_before_the_next_call() {
    // CVL-FR-12.
    let h = Harness::scripted(vec![
        Ok(
            ScriptedReply::calls("search_specifications", serde_json::json!({ "query": "a" }))
                .and_calls("list_skills", serde_json::json!({}))
                .and_calls("search_skills", serde_json::json!({ "query": "b" })),
        ),
        Ok(ScriptedReply::answer("Three at once.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2, "one round of tools, then the answer");

    let final_exchange = &h.seam.exchanges()[1];
    // Input, the reply carrying three calls, and three results.
    assert_eq!(final_exchange.len(), 5);
    let results: Vec<String> = final_exchange
        .iter()
        .filter_map(|message| match message {
            rig::completion::Message::User { content } => Some(content),
            _ => None,
        })
        .flat_map(|content| content.iter())
        .filter_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => Some(result.id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 3, "all three ran before the next call");
    assert_eq!(
        results,
        vec!["call-0", "call-1", "call-2"],
        "the results are in the order the reply asked for them",
    );
}

#[test]
fn at_the_bound_the_turn_delivers_the_prose_the_last_reply_carried() {
    // AGC-FR-18, CVL-FR-19 / CVL-FR-13. A model that never stops asking for tools.
    let script: Vec<Result<ScriptedReply, &'static str>> = (0..MAX_MODEL_CALLS)
        .map(|i| {
            let reply =
                ScriptedReply::calls("list_skills", serde_json::json!({}));
            Ok(if i == MAX_MODEL_CALLS - 1 {
                reply.with_text("The editor spec settles this.")
            } else {
                reply
            })
        })
        .collect();
    let h = Harness::scripted(script);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(
        h.seam.call_count(),
        MAX_MODEL_CALLS,
        "the loop stopped at the bound rather than running on",
    );
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[1].body, "The editor spec settles this.");

    // CVL-FR-13: the tool the final reply asked for was never dispatched.
    //
    // Counted from the records `dispatch_tool` emits, because the exchange
    // cannot answer this: the seam records what it was *given*, on entry, so the
    // last recorded exchange predates the final reply and could not hold that
    // reply's result whether or not it was dispatched. Asserting on it would be
    // an arithmetic identity rather than an observation of behaviour.
    let dispatched = records_where("turnId", &terminal.id)
        .iter()
        .filter(|record| record.message == "agent tool call completed")
        .count();
    assert_eq!(
        dispatched,
        MAX_MODEL_CALLS - 1,
        "every round below the bound dispatched, and the reply at the bound did not",
    );
}

#[test]
fn a_final_reply_with_no_prose_at_all_leaves_nothing_to_deliver() {
    // CVL-FR-13 (second half) / CVL-FR-13, AGC-FR-18, CVL-FR-19.
    //
    // The reply the bound falls on carries a tool call and no prose. At the
    // bound that call is never dispatched, so the reply supplies nothing the
    // turn can use — which is the `empty_reply` outcome, recoverable like any
    // other. Repeating it repeats that same *logical* invocation as a further
    // physical attempt rather than granting the loop another round, so the
    // script carries the two retries beyond the bound.
    let script: Vec<Result<ScriptedReply, &'static str>> = (0..MAX_MODEL_CALLS + MAX_ATTEMPTS - 1)
        .map(|_| Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))))
        .collect();
    let h = Harness::scripted(script);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    // AGC-FR-31: an empty reply is recoverable, so the author is offered it again.
    assert!(terminal.retry_permitted);
    assert_eq!(
        h.seam.call_count(),
        MAX_MODEL_CALLS + MAX_ATTEMPTS - 1,
        "the bound's own invocation was attempted three times",
    );
    // CVL-FR-13: and the logical bound stands regardless — the retries advanced
    // the physical count alone rather than granting the loop further rounds.
    assert_eq!(
        records_where("turnId", &terminal.id)
            .iter()
            .filter(|record| record.message == "agent tool call completed")
            .count(),
        MAX_MODEL_CALLS - 1,
        "no round beyond the bound dispatched a tool",
    );
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "the thread gained no line",
    );
    assert_eq!(h.agent_sessions(), 0, "a failed turn's session ended with it");
}

#[test]
fn the_model_call_bound_is_the_value_the_spec_argues_for() {
    // CVL-FR-13. Both bound tests build their script from the constant, so a
    // change to it would keep them green; this is what makes changing the number
    // a deliberate act rather than a silent one.
    assert_eq!(MAX_MODEL_CALLS, 8);
}

#[test]
fn a_tool_refusal_is_fed_back_and_the_turn_still_delivers() {
    // CVL-FR-14. The first path does not exist; the second does.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "does/not/exist.md" }),
        )),
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(ScriptedReply::answer("Found it on the second try.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    // CVL-FR-14: a tool's refusal is not a turn's failure.
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(terminal.failure.is_none());
    assert_eq!(h.seam.call_count(), 3, "the loop continued past the refusal");

    // The refusal reached the model as that call's own result, carrying the
    // tool's own message.
    let second = &h.seam.exchanges()[1];
    let refusal = second
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .find_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => Some(result.clone()),
            _ => None,
        })
        .expect("the refusal came back as a tool result");
    let text = match refusal.content.first() {
        rig::completion::message::ToolResultContent::Text(text) => text.text.clone(),
        _ => panic!("a tool result is text"),
    };
    assert_eq!(
        text,
        crate::tools::FILE_NOT_FOUND,
        "the model reads the tool's own sentence",
    );

    assert_eq!(h.threads("spec.md")[0].comments[1].body, "Found it on the second try.");
}

#[test]
fn a_tool_the_model_invented_is_reported_to_it_rather_than_ending_the_turn() {
    // CVL-FR-14. A name that is not among the five it was offered.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("delete_everything", serde_json::json!({}))),
        Ok(ScriptedReply::answer("Never mind.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let result = h.seam.exchanges()[1]
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .find_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => match result.content.first() {
                rig::completion::message::ToolResultContent::Text(text) => Some(text.text.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("a result came back");
    assert!(
        result.contains("No tool by that name"),
        "the model is told the name is not on offer: {result}",
    );
}

#[test]
fn a_turn_that_called_every_tool_changes_nothing_in_the_project() {
    // CVL-FR-09.
    let h = Harness::scripted(vec![
        Ok(
            ScriptedReply::calls("search_specifications", serde_json::json!({ "query": "teardown" }))
                .and_calls("read_file", serde_json::json!({ "path": "spec.md" }))
                .and_calls(
                    "search_skills",
                    serde_json::json!({ "query": "author specifications" }),
                )
                .and_calls("list_skills", serde_json::json!({}))
                .and_calls("load_skill", serde_json::json!({ "name": "analyst" }))
                .and_calls("search_notes", serde_json::json!({ "query": "teardown" })),
        ),
        Ok(ScriptedReply::answer("Read everything.")),
    ]);
    // Seeded so all six tools have something real to answer from: without a
    // skill, a specification, and a note in the tree, `load_skill`,
    // `search_specifications`, and `search_notes` refuse or answer empty, and a
    // turn where every tool refused early would satisfy the snapshot assertion
    // without ever reading anything.
    crate::tools::tests::write_skill(
        h.root().path(),
        ".claude/skills",
        "analyst",
        "name: analyst\ndescription: Author specifications for this project.",
        "Follow the analyst procedure.",
    );
    std::fs::create_dir_all(h.root().path().join("specifications")).expect("specs dir");
    std::fs::write(
        h.root().path().join("specifications/teardown.md"),
        "# Teardown\n\nHow a worktree is torn down.\n",
    )
    .expect("a specification");
    // NST-FR-19: the note this turn will find. It is written before the
    // snapshot, so the assertion below is that searching left it — and its
    // `updated_at` — exactly as it was.
    let note = crate::notes::create_note_in(
        &crate::fs::RootFs::for_root(h.root().path()),
        crate::notes::NoteScope::Project,
        "the teardown leaves a stale overlay behind".to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("a note");
    h.mount();
    h.create_agent("arch", "");

    // Seeded before the snapshot: the artifact and its thread are the test's own
    // setup, and counting them as changes would make this pass for the wrong
    // reason.
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let before = tree_snapshot(h.root().path());

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    // Every one of the six ran and answered: CVL-FR-09 is about tools that read,
    // and six refusals would prove nothing about what a reading turn changes.
    // In the order the reply asked for them.
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results.len(), 6, "all six tools ran");
    assert!(
        results[0].contains("specifications/teardown.md"),
        "search_specifications found the seeded specification: {}",
        results[0],
    );
    assert_eq!(
        results[1], "Some artifact source here.",
        "read_file returned the artifact",
    );
    assert!(
        results[2].contains("analyst"),
        "search_skills found the seeded skill: {}",
        results[2],
    );
    assert!(
        results[3].contains("analyst"),
        "list_skills listed it: {}",
        results[3],
    );
    assert!(
        results[4].contains("Follow the analyst procedure."),
        "load_skill returned the skill's body: {}",
        results[4],
    );
    // NST-FR-01: the only place the whole path is exercised — the model's own
    // JSON decoded into the tool's arguments, the call made, and the output
    // rendered back into the content block the model reads. Every other test of
    // this tool constructs its arguments by hand.
    assert!(
        results[5].contains("the teardown leaves a stale overlay behind"),
        "search_notes returned the seeded note whole: {}",
        results[5],
    );
    assert!(
        results[5].contains(&note.id),
        "and named it, so the model could act on it: {}",
        results[5],
    );

    // The comment the turn appended is the only change; every other file is
    // byte-identical, and nothing was created or removed.
    let after = tree_snapshot(h.root().path());
    let changed: Vec<_> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(bytes))
        .map(|(path, _)| path.clone())
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "a turn changed something other than the conversation it answered: {changed:?}",
    );
    assert!(
        changed[0].contains("comments"),
        "the one change is the conversation's own log: {changed:?}",
    );
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "a turn created and removed no file",
    );
    // NST-FR-19: and the note the turn found was not resolved, marked, or
    // reported as read — `updated_at` is what a write of any kind would have
    // moved, and the tree comparison above already covers its body and scope.
    let after_note = crate::notes::note_record(
        &crate::fs::RootFs::for_root(h.root().path()),
        &note.id,
    )
    .expect("the note still resolves");
    assert_eq!(after_note, note, "searching a note is not activity on it");
}


