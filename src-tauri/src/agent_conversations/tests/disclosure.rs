//! What a turn discloses about itself, and the guard that keeps a
//! registration from outliving what it registered.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// AGC-FR-26 / AGC-FR-23, CVL-FR-26: what a turn discloses
// ---------------------------------------------------------------------------

#[test]
fn no_part_of_a_key_appears_in_a_turn_record_an_event_or_a_progress_label() {
    // AGC-FR-26. `FixedSecrets` hands out `sk-secret-value`, which
    // is what the endpoint carries and what must appear nowhere else.
    let h = Harness::with(vec![Ok("ok".into())], Duration::from_millis(200), 4);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    let (tx, rx) = mpsc::channel();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        let _ = tx.send(event.payload().to_string());
    });

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let labels: Vec<String> = h
        .progress()
        .in_flight()
        .into_iter()
        .map(|op| op.label)
        .collect();
    // Every assertion below is **negative** — nothing carries the key — so a
    // drain that starts before the turn has finished stops checking rather than
    // failing, and the test passes without having looked at the events it is
    // about. `settle()` returns before the terminal event is published, so the
    // terminal event is waited for and the payloads are given a floor.
    wait_for_terminal(&h, &turn.id);

    let serialised = serde_json::to_string(&turn).expect("turn");
    assert!(!serialised.contains("sk-secret"));
    for label in labels {
        assert!(!label.contains("sk-secret"));
    }
    let mut payloads = 0usize;
    while let Ok(payload) = rx.recv_timeout(Duration::from_millis(200)) {
        payloads += 1;
        assert!(!payload.contains("sk-secret"), "event payload: {payload}");
    }
    assert!(
        payloads >= 2,
        "the drain saw {payloads} events, so it cannot have checked the turn's own two",
    );
    // The seam did receive it — a key exists for the duration of the call and
    // nowhere else.
    let (_, endpoint) = h.seam.requests().into_iter().next().expect("one call");
    assert_eq!(endpoint.api_key.as_deref(), Some("sk-secret-value"));
    // And even that value redacts itself when printed.
    assert!(!format!("{endpoint:?}").contains("sk-secret-value"));
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn the_delivered_comment_carries_the_answer_alone_and_nothing_records_the_request() {
    // AGC-FR-23, CVL-FR-26.
    //
    // The agent carries no instructions, so the framing is the only string in
    // the request the application wrote — which is the case CVL-FR-26 is now
    // easiest to break in, `AgentRequest` having gained a `Serialize` field
    // that production never serialises.
    let h = Harness::new(vec![Ok("Just the answer.".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "A distinctive artifact phrase.");

    // Both channels a delivery emits on: the turn's own, and the thread's
    // (CMS-FR-51), which this delivery path added.
    let (tx, rx) = mpsc::channel();
    let turn_tx = tx.clone();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        let _ = turn_tx.send(event.payload().to_string());
    });
    h.app
        .listen(crate::comments::DISCUSSION_CHANGED, move |event| {
            let _ = tx.send(event.payload().to_string());
        });

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);

    // A line of the prompt long enough that nothing else in the process could
    // hold it by coincidence.
    let prompt_sentence = COMMENT_PROMPT_TEMPLATE
        .lines()
        .max_by_key(|l| l.len())
        .expect("the prompt's longest line");

    let threads = h.threads("spec.md");
    let comments = &threads[0].comments;
    assert_eq!(comments[1].body, "Just the answer.");
    assert!(!comments[1].body.contains("A distinctive artifact phrase."));
    assert!(
        !comments[1].body.contains(prompt_sentence),
        "the delivered comment echoes the prompt back into the conversation",
    );

    let mut hits = Vec::new();
    walk(&h.root(), &mut hits);
    for path in hits {
        if let Ok(text) = std::fs::read_to_string(&path) {
            assert!(
                !text.contains(prompt_sentence),
                "{} records the assembled request",
                path.display(),
            );
            assert!(
                !text.contains(&turn.id),
                "{} records the turn",
                path.display(),
            );
        }
    }

    let mut payloads = 0;
    while let Ok(payload) = rx.recv_timeout(Duration::from_millis(200)) {
        assert!(
            !payload.contains(prompt_sentence),
            "an event payload carries the assembled request: {payload}",
        );
        payloads += 1;
    }
    // Otherwise an empty channel would satisfy the loop above.
    assert!(
        payloads >= 3,
        "expected the registration, the thread change, and the terminal state; saw {payloads}",
    );
}

// ---------------------------------------------------------------------------
// Registration guard
// ---------------------------------------------------------------------------

#[test]
fn every_command_refuses_while_no_project_is_open() {
    // AGC-FR-NKWM and AGR-FR-02, AGR-FR-22, at the command level — the guard is on the Tauri
    // wrapper rather than on the `_impl`, so a test that called the impl would
    // prove nothing about it. The harness's `ProjectState` has no anchor, which
    // is exactly "no project is open".
    use tauri::Manager;
    let h = Harness::new(vec![]);
    let project = h.app.state::<ProjectState>();
    let turns = h.turns();
    let progress = h.progress();
    let store = h.store();

    assert_eq!(
        list_agent_turns(None, turns.clone(), project.clone()).unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        cancel_agent_turn(
            "turn-1".into(),
            h.app.handle().clone(),
            turns.clone(),
            progress.clone(),
            project.clone(),
        )
        .unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        dispatch_agent_turn(
            "arch".into(),
            ConversationOrigin::stub_artifact("t1", "a.md", true),
            "c1".into(),
            h.app.handle().clone(),
            project.clone(),
        )
        .unwrap_err(),
        ERR_NO_PROJECT_OPEN,
    );

    // AGR-FR-22: the project-scoped registry commands refuse on the same terms,
    // while the registry commands answer regardless.
    assert_eq!(
        agents::list_project_agents(store.clone(), project.clone()).unwrap_err(),
        agents::ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        agents::enrol_project_agent("a1".into(), store.clone(), project.clone()).unwrap_err(),
        agents::ERR_NO_PROJECT_OPEN,
    );
    assert_eq!(
        agents::remove_project_agent(
            "a1".into(),
            h.app.handle().clone(),
            store.clone(),
            project,
            turns,
            progress,
        )
        .unwrap_err(),
        agents::ERR_NO_PROJECT_OPEN,
    );
    assert!(agents::list_agents(store).is_ok());
}

#[test]
fn a_turn_in_flight_when_the_project_closes_terminates_cancelled() {
    // AGC-FR-23's second clause / AGC-FR-20. `cancel_all_turns` is what
    // `project::close_project` calls; nothing else exercised it.
    let h = Harness::with(vec![Ok("never".into())], Duration::from_millis(400), 4);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    assert_eq!(h.turns().in_flight(None).len(), 1);

    cancel_all_turns(&h.app.handle().clone(), &h.turns(), &h.progress());
    assert!(h.turns().in_flight(None).is_empty());
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Cancelled);
    h.settle();
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

#[test]
fn an_undecodable_anchored_draft_file_costs_its_section_and_the_turn_proceeds() {
    // AGC-FR-12's draft clause / AGC-FR-12.
    let h = Harness::new(vec![]);
    let created = crate::drafts::create_draft_at_root(&h.root(), Some("Review me"))
        .expect("draft");
    let draft_id = created.draft.id.clone();
    let thread = seed_draft_thread(&h, &draft_id, &created.file);

    // The anchored file itself does not decode as UTF-8, written past the draft
    // API — which is the only way the builder's read can fail.
    let files = h
        .root()
        .join(format!(".synthesis/drafts/{draft_id}/files"));
    std::fs::write(files.join(&created.file), [0xff, 0xfe, 0x00, 0x9f]).expect("binary");

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    assert_eq!(
        tags_of(&sections),
        vec![TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT],
        "the artifact section is absent and the rest is still assembled",
    );
    // And the turn has material to proceed on, so AGC-FR-12 does not fail it.
    assert!(section_of(&sections, TAG_CURRENT_COMMENT)
        .body
        .contains("@arch is this two specs?"));
}

#[test]
fn the_reasoning_a_call_carries_is_the_shape_each_provider_takes_it_in() {
    // CVL-FR-10's other half. `RigCompletion`'s provider match decides which
    // client carries a request; what these assert is the translation each side
    // of that match performs, which is pure and needs no network.
    use crate::ai_api::ReasoningChoice;

    let built = |choice: Option<ReasoningChoice>| {
        build_completion_request(
            &AgentRequest::default(),
            &opening_exchange(&AgentRequest::default(), false),
            &AiApiCall {
                turn_timeout_ms: None,
                provider: "openrouter".into(),
                base_url: "https://example/v1".into(),
                api_key: None,
                model_id: Some("m".into()),
                reasoning: choice,
                accepts_image_input: false,
                    model_mode: None,
            },
        )
        .additional_params
    };
    // AAP-FR-31: a null choice sends nothing at all.
    assert_eq!(built(None), None);
    assert_eq!(
        built(Some(ReasoningChoice::Off)),
        Some(serde_json::json!({ "reasoning": { "enabled": false } })),
    );
    assert_eq!(
        built(Some(ReasoningChoice::On)),
        Some(serde_json::json!({ "reasoning": { "enabled": true } })),
    );
    assert_eq!(
        built(Some(ReasoningChoice::Effort {
            effort: "high".into()
        })),
        Some(serde_json::json!({ "reasoning": { "effort": "high" } })),
    );

    // AAP-FR-26's mirror: a level neither this build nor the SDK knows by name
    // still reaches the endpoint intact rather than being coerced or dropped.
    assert_eq!(effort_of("high").as_str(), "high");
    assert_eq!(effort_of("xhigh").as_str(), "xhigh");
    assert_eq!(effort_of("something-new").as_str(), "something-new");
}

#[test]
fn a_failure_message_is_classified_by_what_it_calls_for() {
    // AGC-FR-15: the distinctions exist because they call for different
    // corrections — a host to bring up, a credential to replace, a wait to sit
    // out.
    assert_eq!(classify_provider_error("connection refused").failure, FAIL_UNREACHABLE);
    assert_eq!(classify_provider_error("request timed out").failure, FAIL_TIMED_OUT);
    assert_eq!(classify_provider_error("HTTP 401 Unauthorized").failure, FAIL_REJECTED);
    assert_eq!(classify_provider_error("invalid api key").failure, FAIL_REJECTED);
}

#[test]
fn a_long_discussion_history_is_bounded_like_any_other_section() {
    // AGC-FR-11 applies to every section, not only the artifact's.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Short source.");
    // A conversation past the bound.
    for i in 0..40 {
        crate::comments::add_comment_in(
            &h.root(),
            "spec.md",
            &thread.id,
            "x".repeat(2_000),
            vec![],
            Vec::new(),
            &human("ada"),
            &format!("2026-01-01T00:00:{:02}Z", i + 1))
        .expect("comment");
    }
    let refreshed = h.threads("spec.md");
    let last = refreshed[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &last,
    );
    let conversation = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert!(conversation.truncated);
    assert!(conversation.body.contains("partial"));
    assert!(conversation.body.chars().count() < 41_000);
}

#[test]
fn the_conversation_commands_are_in_scope() {
    let _ = dispatch_agent_turn::<tauri::test::MockRuntime>;
    let _ = cancel_agent_turn::<tauri::test::MockRuntime>;
    let _ = list_agent_turns;
}

/// AGC-FR-05, AGC-FR-06, AGC-FR-07 / CVL-FR-01, AGC-FR-12: the `artifact_discussion` builder carries the whole
/// source of the file the discussion is about — its own document, not a rendering
/// of it — and nothing of any other file.
#[test]
fn an_artifact_discussion_carries_the_files_whole_source() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "Argue about structure.");

    // A Flow, so the assertion that the builder reads bytes rather than a
    // rendering is a real one: what reaches the model is the graph's JSON.
    let flow_source = r#"{"version":1,"name":"Onboarding review","nodes":[],"edges":[]}"#;
    std::fs::write(h.root().join("review.flow"), flow_source).expect("flow");
    // A second file that must not leak into the section.
    std::fs::write(h.root().join("other.md"), "NOT THIS ONE").expect("other");

    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: "review.flow".into(),
        },
        None,
        "@arch can the review step split?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let seen = h.seam.requests();
    assert_eq!(seen.len(), 1);
    let (request, _) = &seen[0];

    // AGC-FR-06: the same three sections in the same order, and no fourth.
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(
        tags,
        vec![TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]
    );

    let artifact = &request.input[0];
    assert_eq!(artifact.body, flow_source, "the file's own bytes");
    assert!(!artifact.body.contains("NOT THIS ONE"));
    assert!(
        artifact
            .attributes
            .iter()
            .any(|(k, v)| k == "path" && v == "review.flow"),
        "named by its project-relative path"
    );
    // A discussion is pinned to nothing, so no anchor appears anywhere.
    assert!(!request.input[1].body.contains("40..61"));
    assert!(
        request.input[1]
            .attributes
            .iter()
            .all(|(k, _)| k != "anchor")
    );
}

/// CVL-FR-03, CVL-FR-04, CVL-FR-08: each discussion kind has an instruction of its own, and neither is
/// the one a remark pinned to a passage is answered under.
#[test]
fn each_discussion_kind_is_answered_under_its_own_prompt() {
    let instructions = "Argue about structure.";
    let draft = compile_prompt(OriginKind::DraftDiscussion, instructions, "");
    let artifact = compile_prompt(OriginKind::ArtifactDiscussion, instructions, "");
    let comment = compile_prompt(OriginKind::ArtifactComment, instructions, "");
    assert_ne!(draft, artifact, "the two discussions differ");
    assert_ne!(artifact, comment);
    assert_ne!(draft, comment);
    // Each is the whole of its own template with the agent's instructions in it,
    // rather than one template with a sentence appended to it.
    assert_eq!(
        draft,
        DISCUSS_DRAFT_PROMPT_TEMPLATE
            .replacen(AGENT_INSTRUCTIONS_PLACEHOLDER, instructions, 1)
            .replacen(AGENT_TITLE_PLACEHOLDER, AGENT_TITLE_UNDEFINED, 1),
    );
    assert_eq!(
        artifact,
        DISCUSS_ARTIFACT_PROMPT_TEMPLATE
            .replacen(AGENT_INSTRUCTIONS_PLACEHOLDER, instructions, 1)
            .replacen(AGENT_TITLE_PLACEHOLDER, AGENT_TITLE_UNDEFINED, 1),
    );
    // The two comment kinds still share theirs — the split is between the
    // discussions and nowhere else.
    assert_eq!(comment, compile_prompt(OriginKind::DraftComment, instructions, ""));
    for compiled in [&draft, &artifact, &comment] {
        assert!(compiled.contains(instructions));
    }
    // CVL-FR-03 / CVL-FR-08: all three tell the agent it may offer a rewrite,
    // the artifact origins and the draft origins each having a proposal tool of
    // their own — and none names which store the rewrite would be recorded in,
    // that being the tool's affair rather than the prompt's. Asserted on the
    // section each prompt commits to rather than on a stray word: the tool's own
    // name is what CVL-FR-05 forbids it to carry, and a bare substring would be
    // satisfied by the word turning up in an unrelated sentence.
    assert!(
        DISCUSS_DRAFT_PROMPT_TEMPLATE
            .to_lowercase()
            .contains("## making proposals"),
        "the draft prompt does not tell the agent it may offer a rewrite",
    );
    for (name, template) in [
        ("comment.md", COMMENT_PROMPT_TEMPLATE),
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
    ] {
        assert!(
            template.to_lowercase().contains("## offering a rewrite"),
            "{name} does not tell the agent it may offer a rewrite",
        );
        for store in [".synthesis", "proposals/", "draft's own folder"] {
            assert!(
                !template.to_lowercase().contains(store),
                "{name} names the store a rewrite would be recorded in",
            );
        }
    }
}

/// AGC-FR-12: a file that cannot be read costs the agent that section and not the
/// turn — the history and the current comment are still assembled.
#[test]
fn an_unreadable_file_leaves_the_rest_of_an_artifact_discussion_intact() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "Argue about structure.");
    std::fs::write(h.root().join("spec.md"), "# Spec").expect("spec");
    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: "spec.md".into(),
        },
        None,
        "@arch thoughts?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    // Gone after the thread was opened, which is exactly the case AGC-FR-12 is
    // about: the log still reads, the material does not.
    std::fs::remove_file(h.root().join("spec.md")).expect("remove");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let seen = h.seam.requests();
    assert_eq!(seen.len(), 1);
    let tags: Vec<&str> = seen[0].0.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, vec![TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]);
}

/// CMS-FR-54 / AGC-FR-02: a locked artifact discussion takes no further
/// contribution, and the refusal is knowable before any model is called.
#[test]
fn a_locked_artifact_discussion_refuses_its_turn() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "Argue about structure.");
    std::fs::write(h.root().join("spec.md"), "# Spec").expect("spec");
    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: "spec.md".into(),
        },
        None,
        "@arch thoughts?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    crate::comments::set_lock_to(
        &h.root(),
        crate::comments::ThreadRef::artifact_discussion("spec.md"),
        &discussion.id,
        true,
        &human("ada"),
        "2026-01-01T00:01:00Z",
    )
    .expect("lock");

    let err = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&discussion),
            &discussion.comments[0].id,
        )
        .expect_err("a locked conversation takes no contribution");
    assert!(err.contains("discussion_locked"), "got {err}");
    h.settle();
    assert!(h.seam.requests().is_empty(), "no model was called");
}

/// AGC-FR-16: an agent's answer lands in the DISCUSSION log, not in the anchored
/// log beside it — the two are separate conversations about the same file.
#[test]
fn an_answer_lands_in_the_discussion_log_beside_the_anchored_one() {
    let h = Harness::new(vec![Ok("It reads well.".into())]);
    h.create_agent("arch", "Argue about structure.");
    let anchored = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let discussion = crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: "spec.md".into(),
        },
        None,
        "@arch thoughts?".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let folded = crate::comments::fold_artifact_discussion(&h.root(), "spec.md");
    assert_eq!(folded.len(), 1);
    assert_eq!(folded[0].comments.len(), 2, "the answer joined the discussion");
    assert_eq!(folded[0].comments[1].body, "It reads well.");

    // The anchored thread on the same file gained nothing.
    let still = crate::comments::list_fragment_discussions_in(&h.root(), "spec.md");
    assert_eq!(still.len(), 1);
    assert_eq!(still[0].id, anchored.id);
    assert_eq!(still[0].comments.len(), 1);
}


/// CTA-FR-HEYB: the fixed wording the *surface* holds for each of these tools.
///
/// Never sent, never received, and named here only so a test can assert it
/// appears nowhere a tool could have put it (TLC-FR-27).
const SURFACE_ACTIVITY_WORDING: [&str; 13] = [
    "Searching related specifications",
    "Reading a project file",
    "Searching drafts",
    "Reading a draft",
    "Searching notes",
    "Searching available skills",
    "Listing available skills",
    "Loading skill instructions",
    "Asking a clarifying question",
    "Preparing draft changes",
    "Preparing prompt changes",
    "Searching the web",
    "Fetching from the web",
];




#[test]
fn no_conversation_tool_declares_carries_or_returns_an_activity_status() {
    // CTA-FR-VSIM / TLC-FR-27: a call in flight is reported to the author as one
    // short activity status, and the wording belongs to the **surface** rather
    // than to the tool. Two assertions, and the structural one is what makes
    // this falsifiable: a definition that grew a status-shaped field of any
    // wording fails it, not merely one that copied the surface's own text.
    fn inspect(tools: &[rig::completion::ToolDefinition], carried: &str) {
        assert!(!tools.is_empty(), "a turn is lent its tools");
        for tool in tools {
            let definition = serde_json::to_value(tool).expect("serialise");
            // No field a surface could read a status out of — under any of the
            // three names one would plausibly take, at the top level or in the
            // parameters.
            for name in ["status", "activity", "label"] {
                assert!(
                    definition.get(name).is_none(),
                    "`{}` declares a `{name}` of its own",
                    tool.name,
                );
                assert!(
                    tool.parameters
                        .get("properties")
                        .and_then(|p| p.get(name))
                        .is_none(),
                    "`{}` declares a `{name}` parameter",
                    tool.name,
                );
            }
            // …and none of the surface's own wording, which is what a tool
            // that tried to *carry* its status would contain.
            let text = definition.to_string();
            for wording in SURFACE_ACTIVITY_WORDING {
                assert!(
                    !text.contains(wording),
                    "`{}` carries the surface's wording for {wording}",
                    tool.name,
                );
            }
        }
        // Nor does anything a tool **returned**: the exchange that went back to
        // the provider carries this turn's tool results.
        for wording in SURFACE_ACTIVITY_WORDING {
            assert!(
                !carried.contains(wording),
                "a tool returned the surface's wording for {wording}",
            );
        }
        for name in ["\"status\"", "\"activity\""] {
            assert!(!carried.contains(name), "a tool result carries {name}");
        }
    }

    // Both proposal tools, so the ninth of each origin kind is inspected too —
    // `propose_draft_changes` on a draft origin and `propose_prompt_changes` on
    // an artifact one, the two the CTA-FR-HEYB vocabulary names beside the nine.
    let artifact = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    artifact.mount();
    artifact.create_agent("arch", "");
    let (terminal, _) = run_one(&artifact, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let (request, _) = artifact.seam.requests().remove(0);
    assert!(
        request.tools.iter().any(|t| t.name == ARTIFACT_ONLY_TOOL),
        "the artifact origin's ninth is among them",
    );
    inspect(
        &request.tools,
        &format!("{:?}", artifact.seam.exchanges()[1]),
    );

    let draft = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    draft.mount();
    draft.create_agent("arch", "");
    let (_draft_id, discussion) = seed_discussion(&draft, &["@arch thoughts?"]);
    let turn = draft
        .dispatch(
            "arch",
            ConversationOrigin::of(&discussion),
            &discussion.comments[0].id,
        )
        .expect("dispatch");
    wait_for_terminal(&draft, &turn.id);
    let (request, _) = draft.seam.requests().remove(0);
    assert!(
        request.tools.iter().any(|t| t.name == DRAFT_ONLY_TOOL),
        "the draft origin's ninth is among them",
    );
    inspect(&request.tools, &format!("{:?}", draft.seam.exchanges()[1]));
}
