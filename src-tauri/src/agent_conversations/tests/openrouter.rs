//! The OpenRouter bridge (CVL-FR-10).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// The OpenRouter bridge (CVL-FR-10)
// ---------------------------------------------------------------------------
//
// Pure translation, tested directly: the only other way in is `carry_openrouter`,
// which needs a network call, so without these the request and response mapping
// of the provider every agent in this file names is unreachable under test.

/// The text of an SDK message, which carries no `Display` of its own.
///
/// Either shape, because CVL-FR-23 puts a message that carries a cache marker
/// into parts to carry it: what these tests are about is the conversation
/// reaching the provider in order and intact, which is the same claim whichever
/// shape the text arrived in. The marking itself is asserted on its own.
fn content_text(message: &openrouter_rs::api::chat::Message) -> String {
    use openrouter_rs::api::chat::{Content, ContentPart};
    match &message.content {
        Content::Text(text) => text.clone(),
        Content::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("expected text content, got {other:?}"),
    }
}

/// CVL-FR-23: the cache markers an SDK message carries, one per content part
/// that has one. Empty for a message marked with nothing.
///
/// The **lifetime** is part of what is reported, not just the kind:
/// `CacheControlType` has a single variant, so a helper naming the kind alone
/// would read identically for a five-minute marker and a one-hour one and would
/// let CVL-FR-23's fixed lifetime be changed without a test noticing. An absent
/// TTL is the short one, which is what this SDK sends when none is named.
fn cache_markers(message: &openrouter_rs::api::chat::Message) -> Vec<String> {
    use openrouter_rs::api::chat::{Content, ContentPart};
    match &message.content {
        Content::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text {
                    cache_control: Some(control),
                    ..
                } => Some(format!(
                    "{:?}/{}",
                    control.kind,
                    control.ttl.as_deref().unwrap_or("5m"),
                )),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The marker every call is expected to carry: ephemeral, five minutes
/// (CVL-FR-23).
const FIVE_MINUTE_MARKER: &str = "Ephemeral/5m";

/// CVL-FR-39: material with no stable head, which is marked exactly as an
/// unsplit input is. Named rather than spelled `Default::default()` at each
/// call, because what a reader needs to know at a call site is that this
/// assertion is about the unsplit shape.
fn unsplit() -> openrouter_bridge::CacheHints {
    openrouter_bridge::CacheHints::default()
}

/// CVL-FR-39: material whose stable head is `head`.
fn split_at(head: &str) -> openrouter_bridge::CacheHints {
    openrouter_bridge::CacheHints {
        stable_head: Some(head.to_string()),
    }
}

/// CVL-FR-39: each content part's text, paired with whether it carries a mark.
///
/// The pairing is the point: a split that put the mark on the tail, or that left
/// an empty part behind, reads differently here from one that did neither, and
/// asserting the marks alone would let both through.
fn marked_parts(message: &openrouter_rs::api::chat::Message) -> Vec<(String, Option<String>)> {
    use openrouter_rs::api::chat::{Content, ContentPart};
    match &message.content {
        Content::Text(text) => vec![(text.clone(), None)],
        Content::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text {
                    text,
                    cache_control,
                    ..
                } => Some((
                    text.clone(),
                    // The **lifetime**, not just the presence of a mark: a
                    // helper reporting a boolean would read identically for a
                    // five-minute mark and a one-hour one, and would let
                    // CVL-FR-23's fixed lifetime be changed without a test
                    // noticing.
                    cache_control.as_ref().map(|control| {
                        format!("{:?}/{}", control.kind, control.ttl.as_deref().unwrap_or("5m"))
                    }),
                )),
                _ => None,
            })
            .collect(),
        other => panic!("expected text content, got {other:?}"),
    }
}

/// CVL-FR-39: the text of one SDK message, its parts concatenated with no
/// separator at all.
///
/// Distinct from `content_text`, which joins with a newline because the parts it
/// reads came from separate messages of the exchange. A split message's parts
/// are one text cut in two, so what proves the cut harmless is that they
/// concatenate back to exactly what arrived.
fn concatenated_text(message: &openrouter_rs::api::chat::Message) -> String {
    use openrouter_rs::api::chat::{Content, ContentPart};
    match &message.content {
        Content::Text(text) => text.clone(),
        Content::Parts(parts) => parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect(),
        other => panic!("expected text content, got {other:?}"),
    }
}

/// CVL-FR-23: the wire shape of one message, so a test can assert that marking
/// put exactly the intended messages into parts and left every other one alone.
///
/// The distinction matters because a tool result whose content is an array
/// rather than a string is a shape not every endpoint behind OpenRouter accepts,
/// and that risk is the whole reason the marking stops where it does.
fn content_shape(message: &openrouter_rs::api::chat::Message) -> &'static str {
    use openrouter_rs::api::chat::Content;
    match &message.content {
        Content::Text(_) => "text",
        Content::Parts(_) => "parts",
        _ => "other",
    }
}


#[test]
fn the_bridge_carries_a_tool_definition_into_the_sdks_shape() {
    let definition = rig::completion::ToolDefinition {
        name: "search_specifications".into(),
        description: "Find the specifications most relevant to a topic.".into(),
        parameters: serde_json::json!({ "type": "object", "properties": {} }),
    };
    let carried = openrouter_bridge::tool_out(&definition);
    assert_eq!(carried.tool_type, "function");
    assert_eq!(carried.function.name, "search_specifications");
    assert_eq!(
        carried.function.description,
        "Find the specifications most relevant to a topic.",
    );
    assert_eq!(carried.function.parameters, definition.parameters);
}

#[test]
fn the_bridge_carries_a_whole_exchange_in_order_with_its_ids_intact() {
    // The shape a turn's second round actually has: preamble, the input, the
    // reply that asked for two tools, and a result for each.
    let request = rig::completion::CompletionRequest {
        model: Some("m".into()),
        preamble: Some("PREAMBLE".into()),
        chat_history: rig::OneOrMany::many(vec![
            rig::completion::Message::user("INPUT"),
            rig::completion::Message::Assistant {
                id: None,
                content: rig::OneOrMany::many(vec![
                    rig::completion::message::AssistantContent::text("Looking."),
                    rig::completion::message::AssistantContent::ToolCall(rig_tool_call(
                        "call-0",
                        "list_skills",
                        serde_json::json!({}),
                    )),
                    rig::completion::message::AssistantContent::ToolCall(rig_tool_call(
                        "call-1",
                        "read_file",
                        serde_json::json!({ "path": "a.md" }),
                    )),
                ])
                .expect("content"),
            },
            rig::completion::Message::tool_result("call-0", "SKILLS"),
            rig::completion::Message::tool_result("call-1", "FILE"),
        ])
        .expect("history"),
        documents: Vec::new(),
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    };

    let carried = openrouter_bridge::messages_out(&request, &unsplit());
    use openrouter_rs::types::Role;
    assert_eq!(carried.len(), 5, "system, user, assistant, and two results");
    assert!(matches!(carried[0].role, Role::System));
    assert!(matches!(carried[1].role, Role::User));
    assert!(matches!(carried[2].role, Role::Assistant));
    assert!(matches!(carried[3].role, Role::Tool));
    assert!(matches!(carried[4].role, Role::Tool));

    // CVL-FR-10: the preamble occupies the instruction position and nothing else
    // is merged into it.
    assert_eq!(content_text(&carried[0]), "PREAMBLE");
    assert_eq!(content_text(&carried[1]), "INPUT");

    // The calls go back out as the provider sent them: a result is matched to
    // its call by id and by nothing else.
    let calls = carried[2].tool_calls.as_ref().expect("the reply's calls");
    assert_eq!(
        calls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec!["call-0", "call-1"],
    );
    assert_eq!(calls[1].function.name, "read_file");
    assert_eq!(carried[3].tool_call_id.as_deref(), Some("call-0"));
    assert_eq!(carried[4].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(content_text(&carried[3]), "SKILLS");
    assert_eq!(content_text(&carried[4]), "FILE");
}

/// The exchange a turn's Nth round actually presents: preamble, the input, then
/// `rounds` reply/result pairs on top of it. CVL-FR-12's growth, in the shape the
/// bridge is asked to carry.
fn exchange_after_rounds(rounds: usize) -> rig::completion::CompletionRequest {
    exchange_after_rounds_with_input(rounds, "INPUT")
}

/// The same, over an input whose text a test chooses — so a split can be
/// asserted against a head and a tail that are told apart by reading them.
fn exchange_after_rounds_with_input(
    rounds: usize,
    input: &str,
) -> rig::completion::CompletionRequest {
    let mut history = vec![rig::completion::Message::user(input)];
    for round in 0..rounds {
        let id = format!("call-{round}");
        history.push(rig::completion::Message::Assistant {
            id: None,
            content: rig::OneOrMany::one(rig::completion::message::AssistantContent::ToolCall(
                rig_tool_call(&id, "list_skills", serde_json::json!({})),
            )),
        });
        history.push(rig::completion::Message::tool_result(
            id,
            format!("RESULT-{round}"),
        ));
    }
    rig::completion::CompletionRequest {
        model: Some("m".into()),
        preamble: Some("PREAMBLE".into()),
        chat_history: rig::OneOrMany::many(history).expect("a non-empty exchange"),
        documents: Vec::new(),
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    }
}

#[test]
fn the_bridge_marks_the_prompt_and_the_input_for_caching_on_every_round() {
    // CVL-FR-20, CVL-FR-01, CVL-FR-10 / CVL-FR-23. What a call presents that the next call presents
    // again unchanged is everything up to and including the input, the exchange
    // growing only at its end (CVL-FR-12) — so that is what carries the mark,
    // and it carries it on every round rather than only on the first.
    for rounds in 0..3 {
        let carried = openrouter_bridge::messages_out(&exchange_after_rounds(rounds), &unsplit());
        assert_eq!(
            cache_markers(&carried[0]),
            vec![FIVE_MINUTE_MARKER.to_string()],
            "round {rounds}: the compiled prompt is marked, for five minutes",
        );
        assert_eq!(
            cache_markers(&carried[1]),
            vec![FIVE_MINUTE_MARKER.to_string()],
            "round {rounds}: the input is marked, for five minutes",
        );
        // Nothing after the input is marked *and* nothing after it is even put
        // into parts: a tool result whose content is an array rather than a
        // string is a shape not every endpoint behind this one accepts, and
        // asserting only the absence of a marker would let that happen.
        let shapes: Vec<&str> = carried.iter().map(content_shape).collect();
        let expected: Vec<&str> = std::iter::repeat("parts")
            .take(2)
            .chain(std::iter::repeat("text").take(rounds * 2))
            .collect();
        assert_eq!(
            shapes, expected,
            "round {rounds}: only the prompt and the input travel as parts",
        );
        for (index, message) in carried.iter().enumerate().skip(2) {
            assert!(
                cache_markers(message).is_empty(),
                "round {rounds}: message {index} ({:?}) is marked and should not be",
                message.role,
            );
        }
        // CVL-FR-23: the marking adds no message and alters none of the text —
        // the same conversation reaches the provider either way.
        assert_eq!(carried.len(), 2 + rounds * 2);
        assert_eq!(content_text(&carried[0]), "PREAMBLE");
        assert_eq!(content_text(&carried[1]), "INPUT");
    }
}

#[test]
fn the_bridge_marks_the_stable_head_apart_from_the_conversation_that_grew() {
    // CVL-FR-23 / CVL-FR-39. The material under discussion is what the next turn
    // of a conversation presents again unchanged, so the mark stands at its end
    // rather than at the end of a message the discussion grows. The split is a
    // cut in the text and nothing else: the parts concatenate to exactly what
    // arrived, and the mark stands on the head alone.
    for rounds in 0..3 {
        let request = exchange_after_rounds_with_input(rounds, "MATERIAL\n\nDISCUSSION");
        let carried = openrouter_bridge::messages_out(&request, &split_at("MATERIAL"));
        assert_eq!(
            cache_markers(&carried[0]),
            vec![FIVE_MINUTE_MARKER.to_string()],
            "round {rounds}: the compiled prompt is marked, for five minutes",
        );
        assert_eq!(
            marked_parts(&carried[1]),
            vec![
                ("MATERIAL".to_string(), Some(FIVE_MINUTE_MARKER.to_string())),
                ("\n\nDISCUSSION".to_string(), None),
            ],
            "round {rounds}: the head carries the input's only mark",
        );
        assert_eq!(
            concatenated_text(&carried[1]),
            "MATERIAL\n\nDISCUSSION",
            "round {rounds}: the split adds no separator and removes none",
        );
        // What grows is not marked here at all, and is not put into parts
        // either: the request asks the client to advance a boundary of its own,
        // and reshaping a tool result is the risk that avoids.
        let shapes: Vec<&str> = carried.iter().map(content_shape).collect();
        let expected: Vec<&str> = std::iter::repeat("parts")
            .take(2)
            .chain(std::iter::repeat("text").take(rounds * 2))
            .collect();
        assert_eq!(
            shapes, expected,
            "round {rounds}: only the prompt and the input travel as parts",
        );
        for (index, message) in carried.iter().enumerate().skip(2) {
            assert!(
                cache_markers(message).is_empty(),
                "round {rounds}: message {index} ({:?}) is marked and should not be",
                message.role,
            );
        }
    }
}

#[test]
fn a_head_the_message_does_not_open_with_marks_the_message_whole() {
    // CVL-FR-01, CVL-FR-37 / CVL-FR-39. Three ways a split does not apply, each falling
    // back to the mark an unsplit input already carries rather than cutting the
    // message in the wrong place or leaving an empty part behind.
    let request = exchange_after_rounds_with_input(1, "MATERIAL\n\nDISCUSSION");
    let whole = vec![(
        "MATERIAL\n\nDISCUSSION".to_string(),
        Some(FIVE_MINUTE_MARKER.to_string()),
    )];

    for (head, why) in [
        ("SOMETHING ELSE", "a head the message does not open with"),
        (
            "MATERIAL\n\nDISCUSSION",
            "a head that is the whole message, whose boundary is where the mark already stands",
        ),
        ("", "no head at all"),
    ] {
        let carried = openrouter_bridge::messages_out(&request, &split_at(head));
        assert_eq!(marked_parts(&carried[1]), whole, "{why}");
        assert!(
            marked_parts(&carried[1]).iter().all(|(text, _)| !text.is_empty()),
            "{why}: no empty content part is produced",
        );
    }
}

#[test]
fn the_stable_head_is_a_real_prefix_of_the_message_it_marks() {
    // CVL-FR-01 / CVL-FR-39, CVL-FR-37. The head is rendered through whichever
    // renderer produced the message it must be a prefix of, so the two cannot
    // disagree — asserted against both, because an input carrying an image is
    // built by the other one.
    for with_images in [false, true] {
        let mut material = InputSection {
            tag: TAG_ARTIFACT.into(),
            attributes: vec![("path".into(), "spec.md".into())],
            body: "The material.".into(),
            truncated: false,
            parts: Vec::new(),
        };
        if with_images {
            material.parts = vec![
                InputPart::Text("Before.\n".into()),
                InputPart::Image(InputImage {
                    data: "AAAA".into(),
                    media_type: "image/png".into(),
                    context: "![a](a.png)".into(),
                    metadata: "image/png a.png".into(),
                }),
                InputPart::Text("After.".into()),
            ];
        }
        let request = AgentRequest {
            instructions: "PROMPT".into(),
            input: vec![
                material,
                InputSection {
                    tag: TAG_DISCUSSION_HISTORY.into(),
                    attributes: Vec::new(),
                    body: "What was said.".into(),
                    truncated: false,
                    parts: Vec::new(),
                },
                InputSection {
                    tag: TAG_CURRENT_COMMENT.into(),
                    attributes: Vec::new(),
                    body: "The question.".into(),
                    truncated: false,
                    parts: Vec::new(),
                },
            ],
            tools: Vec::new(),
            native_tools: Vec::new(),
            stable_head_sections: 1,
        };
        let exchange = opening_exchange(&request, with_images);
        let head = stable_head_text(&request, &exchange).expect("the material is a head");
        let whole = openrouter_bridge::user_message_text(&exchange[0]);
        assert!(
            whole.starts_with(&head),
            "with_images={with_images}: the head must be a real prefix\nhead: {head:?}\nwhole: {whole:?}",
        );
        assert!(
            whole.len() > head.len(),
            "with_images={with_images}: the discussion follows the material",
        );
        assert!(
            head.contains("The material.") || head.contains("Before."),
            "with_images={with_images}: the head carries the material itself",
        );
        assert!(
            !head.contains("What was said.") && !head.contains("The question."),
            "with_images={with_images}: the head stops before what the conversation grows",
        );
    }
}

#[test]
fn the_head_stops_at_the_first_section_a_conversation_grows() {
    // CVL-FR-23 / CVL-FR-39, CVL-FR-06. Read from the tags rather than from a
    // fixed count, because a section the material did not supply is never pushed
    // and a count would then name the wrong boundary.
    let section = |tag: &str| InputSection {
        tag: tag.into(),
        attributes: Vec::new(),
        body: "x".into(),
        truncated: false,
        parts: Vec::new(),
    };
    assert_eq!(
        stable_head_of(&[
            section(TAG_ARTIFACT),
            section(TAG_DISCUSSION_HISTORY),
            section(TAG_CURRENT_COMMENT),
        ]),
        1,
    );
    assert_eq!(
        stable_head_of(&[
            section(TAG_NOTE_CONTEXT),
            section(TAG_DISCUSSION_HISTORY),
            section(TAG_CURRENT_COMMENT),
        ]),
        1,
    );
    // The material carried nothing to show, so the input opens with what the
    // conversation grows and there is no head at all.
    assert_eq!(
        stable_head_of(&[section(TAG_DISCUSSION_HISTORY), section(TAG_CURRENT_COMMENT)]),
        0,
    );
}

#[test]
fn a_retry_presents_the_same_exchange_marked_the_same_way() {
    // CVL-FR-01, CVL-FR-10 / CVL-FR-20, CVL-FR-23. Driven through the real retry path
    // rather than by calling the bridge twice on one value: a pure function of an
    // immutable input cannot disagree with itself, so that would hold with the
    // marking deleted. What has to be true is that the *turn* presented the same
    // bytes on the attempt that failed and on the one that succeeded.
    let h = Harness::scripted(vec![
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("Settled.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 2, "the failed attempt and the retry");
    let requests = h.seam.requests();

    let carried: Vec<_> = exchanges
        .iter()
        .zip(requests.iter())
        .map(|(exchange, (request, endpoint))| {
            openrouter_bridge::messages_out(
                &build_completion_request(request, exchange, endpoint),
                &unsplit(),
            )
        })
        .collect();
    assert_eq!(
        serde_json::to_value(&carried[0]).expect("serialisable"),
        serde_json::to_value(&carried[1]).expect("serialisable"),
        "a retry presents the same messages and the same marks",
    );
    // And the marks are actually there, so this cannot pass by both attempts
    // carrying none of them.
    assert_eq!(
        cache_markers(&carried[0][0]),
        vec![FIVE_MINUTE_MARKER.to_string()],
    );
    assert_eq!(
        cache_markers(&carried[0][1]),
        vec![FIVE_MINUTE_MARKER.to_string()],
    );
}

#[test]
fn the_anthropic_carrier_asks_for_caching_at_the_short_lifetime() {
    // CVL-FR-20, CVL-FR-01, CVL-FR-10 / CVL-FR-23: the other half of "a provider that caches of its own
    // accord is marked with nothing; one that caches only what it is told is
    // told". Asserted on the model the carrier builds rather than on a response,
    // so what a request asks for is pinned without a network call — and so the
    // five-minute lifetime cannot be lengthened without a test noticing.
    let client = rig::providers::anthropic::Client::builder()
        .api_key("not-a-real-key")
        .build()
        .expect("a client needs no network to build");
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "anthropic".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some("m".into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    };
    let model = anthropic_model(&client, &endpoint);
    assert!(
        model.automatic_caching,
        "the provider is told to cache what each call presents",
    );
    assert!(
        model.automatic_caching_ttl.is_none(),
        "the lifetime is the short one, which is what an unset TTL asks for",
    );
}

#[test]
fn an_exchange_with_no_preamble_marks_the_input_rather_than_the_wrong_message() {
    // CVL-FR-23: the marks are found by role rather than by index, so an absent
    // preamble costs the prompt its mark rather than moving it onto whatever
    // happens to sit first.
    let mut request = exchange_after_rounds(1);
    request.preamble = None;
    let carried = openrouter_bridge::messages_out(&request, &unsplit());
    assert!(matches!(
        carried[0].role,
        openrouter_rs::types::Role::User
    ));
    assert_eq!(content_text(&carried[0]), "INPUT");
    assert_eq!(
        cache_markers(&carried[0]),
        vec![FIVE_MINUTE_MARKER.to_string()],
    );
    for message in carried.iter().skip(1) {
        assert!(cache_markers(message).is_empty());
    }
}

#[test]
fn the_bridge_round_trips_a_tool_calls_arguments_through_the_sdks_json_string() {
    let arguments = serde_json::json!({ "path": "a.md", "offset": 3, "limit": 10 });
    let out = openrouter_bridge::tool_call_out(&rig_tool_call("call-0", "read_file", arguments.clone()));
    assert_eq!(out.id, "call-0");
    assert_eq!(out.function.name, "read_file");

    let back = openrouter_bridge::tool_call_in(&out);
    assert_eq!(back.id, "call-0");
    assert_eq!(back.function.name, "read_file");
    assert_eq!(
        back.function.arguments, arguments,
        "arguments survive the trip through the SDK's JSON string",
    );
}

#[test]
fn arguments_the_provider_sent_unparseably_become_null_rather_than_ending_the_turn() {
    // The tool the call names decides what to do with arguments it cannot use,
    // and that refusal reaches the model as a result it can correct (CVL-FR-14)
    // — a better answer than ending the conversation in the bridge.
    let mangled = openrouter_rs::types::ToolCall::new("call-0", "read_file", "{not json");
    let back = openrouter_bridge::tool_call_in(&mangled);
    assert_eq!(back.function.name, "read_file");
    assert_eq!(back.function.arguments, serde_json::Value::Null);
}

#[test]
fn an_assistant_reply_with_calls_and_no_prose_still_carries_its_calls() {
    // The ordinary middle of a loop: a model that asked for a tool and said
    // nothing. Dropping the message would strand the results that follow it.
    let request = rig::completion::CompletionRequest {
        model: Some("m".into()),
        preamble: None,
        chat_history: rig::OneOrMany::one(rig::completion::Message::Assistant {
            id: None,
            content: rig::OneOrMany::one(rig::completion::message::AssistantContent::ToolCall(
                rig_tool_call("call-0", "list_skills", serde_json::json!({})),
            )),
        }),
        documents: Vec::new(),
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    };
    let carried = openrouter_bridge::messages_out(&request, &unsplit());
    assert_eq!(carried.len(), 1, "no preamble, so the reply alone");
    assert_eq!(
        carried[0]
            .tool_calls
            .as_ref()
            .expect("the calls survived")
            .len(),
        1,
    );
}


#[test]
fn a_panic_inside_the_loop_still_ends_the_turn_and_releases_its_session() {
    // CVL-FR-24 and PRG-FR-09. The loop widened what can panic — arguments
    // decoded from what a model composed, three SDK paths — and an unguarded
    // panic would leave the turn `running` for the rest of the session, its
    // progress operation unresolved, and its filesystem session leaked.
    let h = Harness::with_patient_seam(Box::new(PanickingCompletion));
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert!(terminal.ended_at.is_some(), "the turn reached a terminal state");
    assert_eq!(
        h.turns().in_flight(None).len(),
        0,
        "a panicked turn does not stay running forever",
    );
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "a panicked turn appends nothing",
    );
    wait_for_sessions(&h, 0);

    // The record is where the truth lives: the failure vocabulary has no word
    // for an internal defect, so a reader has to be able to tell this from a
    // provider that was down.
    let panicked = find(&records_where("turnId", &terminal.id), "agent turn panicked");
    assert_eq!(panicked.level, LogLevel::Error);
    assert_eq!(require(&panicked, "agent"), "arch");
}

#[test]
fn nothing_a_tool_returned_is_written_anywhere_or_carried_in_an_event() {
    // AGC-FR-23 / CVL-FR-26. The sibling test covers a turn that called no tool;
    // this is the clause the loop added — no tool call, no tool result, and no
    // intermediate message the model composed reaches disk or an event payload.
    const FETCHED: &str = "SENTINELPHRASEONLYATOOLCOULDHAVERETURNED";

    let h = Harness::scripted(vec![
        Ok(
            ScriptedReply::calls("read_file", serde_json::json!({ "path": "fetched.md" }))
                .with_text("Let me look."),
        ),
        Ok(ScriptedReply::answer("The answer alone.")),
    ]);
    std::fs::write(h.root().path().join("fetched.md"), FETCHED).expect("a file to fetch");
    h.mount();
    h.create_agent("arch", "");

    let (tx, rx) = mpsc::channel::<String>();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        let _ = tx.send(event.payload().to_string());
    });

    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    // It really was fetched — otherwise every sweep below passes for nothing.
    assert!(
        tool_results(&h.seam.exchanges()[1])
            .iter()
            .any(|text| text.contains(FETCHED)),
        "the sentinel reached the model as a tool result",
    );

    // …and nowhere else. Every file under the project, including the
    // conversation's own log.
    for (path, bytes) in tree_snapshot(h.root().path()) {
        let text = String::from_utf8_lossy(&bytes);
        if path == "fetched.md" {
            continue;
        }
        assert!(
            !text.contains(FETCHED),
            "a tool's result was written to {path}",
        );
        assert!(
            !text.contains("Let me look."),
            "an intermediate message the model composed was written to {path}",
        );
    }

    // …nor in any event payload.
    for payload in rx.try_iter() {
        assert!(!payload.contains(FETCHED), "an event carried a tool result");
        assert!(
            !payload.contains("Let me look."),
            "an event carried an intermediate message",
        );
    }

    // The comment is the final answer alone.
    assert_eq!(h.threads("spec.md")[0].comments[1].body, "The answer alone.");
}

#[test]
fn turns_in_flight_at_once_hold_sessions_of_their_own() {
    // RFT-FR-05 (middle clause) / CVL-FR-24, per FSA-FR-29. Sessions are keyed
    // per turn, so two conversations running at once cannot reach each other's
    // scratch space.
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let h = Harness::with_patient_seam(Box::new(ToolThenGated { gate: gate.clone() }));
    h.mount();
    h.create_agent("arch", "");

    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let dispatch = |h: &Harness| {
        h.dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch")
    };
    let first = dispatch(&h);
    let second = dispatch(&h);

    // Both turns are past their first call, so both are genuinely in flight at
    // the same moment: the record lands before the second call is made, and the
    // gate is shut, so neither turn can have got past it. The session count is
    // what says two sessions stand at once — `open_agent_session` is the turn
    // thread's first act, so the registry cannot express "past the first call"
    // and the record is the only thing that can.
    wait_for_record(&first.id, "agent tool call completed");
    wait_for_record(&second.id, "agent tool call completed");
    wait_for_sessions(&h, 2);
    // The two sessions, not the two ids: distinct turn ids come free from the
    // process-wide counter and would hold however sessions were keyed. What is
    // being claimed is that each turn holds a filesystem instance of its own.
    let (a_fs, b_fs) = {
        let state = h.app.state::<crate::fs::FsAccessState>();
        (
            state.agent_session(&first.id).expect("the first turn holds a session"),
            state.agent_session(&second.id).expect("the second turn holds a session"),
        )
    };
    assert!(
        !std::sync::Arc::ptr_eq(&a_fs, &b_fs),
        "two turns, two filesystem sessions",
    );

    {
        let (lock, cvar) = &*gate;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
    }
    // The state, not merely the arrival. A turn that blew its whole-turn
    // deadline also terminates, also emits a terminal event, and also releases
    // its session — so discarding this would pass on two turns that stalled and
    // never answered, which is exactly what a starved harness produces.
    assert_eq!(
        wait_for_terminal(&h, &first.id).state,
        AgentTurnState::Delivered,
    );
    assert_eq!(
        wait_for_terminal(&h, &second.id).state,
        AgentTurnState::Delivered,
    );
    wait_for_sessions(&h, 0);
}

#[test]
fn neither_session_can_reach_the_other_s_temp_directory() {
    // RFT-FR-05 (the isolation clause) / CVL-FR-24, per FSA-FR-29 and FSA-FR-20.
    //
    // Distinct session *ids* do not establish this — a shared temp root handed
    // out under per-turn ids would satisfy the id assertion beside this one and
    // still let one turn read another's scratch work. What isolation means is
    // that the path resolves for the session that owns it and escapes the roots
    // of the one that does not, so that is what is asserted.
    //
    // The containment itself belongs to the filesystem layer and is asserted
    // there (`ts33_an_agent_session_reaches_the_project_and_its_own_scratch_and_nothing_else`).
    // What is new here is the wiring above it: two concurrently dispatched
    // *turns* are keyed to two sessions, which is the clause CVL-FR-24 adds.
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let h = Harness::with_patient_seam(Box::new(ToolThenGated { gate: gate.clone() }));
    h.mount();
    h.create_agent("arch", "");

    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let dispatch = |h: &Harness| {
        h.dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch")
    };
    let first = dispatch(&h);
    let second = dispatch(&h);

    // Waited for on the session registry alone, and NOT on a log record.
    //
    // The registry is the honest precondition: `run_turn` calls
    // `open_agent_session` as its first act, and that builds the `FsAccess` —
    // session temp directory included — before inserting it, so a count of two
    // already means both instances every assertion below reads are fully
    // constructed. Waiting on the tool-call record instead gated on something
    // strictly later than necessary, and on a second mechanism (the log ring)
    // that has nothing to do with what is being asserted. The gate is still shut
    // here, so once the count reaches two neither turn can leave before the
    // assertions run.
    //
    // This shortens the budget from ~10s to ~6s, which is safe only because the
    // session registers strictly earlier than the record did. Both bounds are
    // iteration counts rather than deadlines, so they stretch with the machine.
    wait_for_sessions(&h, 2);

    let fs_state = h.app.state::<crate::fs::FsAccessState>();
    let a = fs_state.agent_session(&first.id).expect("the first turn holds a session");
    let b = fs_state.agent_session(&second.id).expect("the second turn holds a session");

    let a_temp = a.session_temp_dir().expect("a session has a temp directory").to_path_buf();
    let b_temp = b.session_temp_dir().expect("a session has a temp directory").to_path_buf();
    assert_ne!(a_temp, b_temp, "two sessions, two temp directories");

    // The owner can write its own scratch space…
    a.write_text_atomic(a_temp.join("scratch.txt"), "the first turn's working note")
        .expect("a session writes into its own temp directory");

    // …and the session beside it cannot reach that path at all. Not "reads an
    // empty result" — the path escapes every root `b` holds.
    let err = b.read_text(a_temp.join("scratch.txt")).unwrap_err();
    assert!(
        matches!(err, crate::fs::FsError::EscapesAllowedRoots { .. }),
        "one session reached another's temp directory: {err:?}",
    );
    // Symmetrical, so the refusal is a property of the roots rather than of
    // which turn happened to be dispatched first.
    let err = a.read_text(b_temp.join("anything.txt")).unwrap_err();
    assert!(
        matches!(err, crate::fs::FsError::EscapesAllowedRoots { .. }),
        "the refusal is not symmetrical: {err:?}",
    );

    // CVL-FR-24: each session's temp directory goes with the turn that held it.
    // Both are released together here, because one gate holds both turns, so what
    // this pins is that a directory is discarded with its own session rather than
    // outliving it — that they are two directories and not one shared root is
    // what the assertions above establish. The stronger form, one session closing
    // while the other keeps its directory, is asserted without threads in
    // `ts33_closing_one_session_leaves_the_others_alone`.
    //
    // The instances are dropped first: a temp directory lives as long as the
    // `FsAccess` holding it, so a test still holding one would observe the
    // directory surviving a close it had itself prevented.
    drop(a);
    drop(b);
    {
        let (lock, cvar) = &*gate;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
    }
    wait_for_terminal(&h, &first.id);
    wait_for_terminal(&h, &second.id);
    wait_for_sessions(&h, 0);
    // Polled, not read once: the registry dropping its reference is not the
    // same event as the directory going away. A temp directory lives as long as
    // the `FsAccess` that owns it, and `FsAccessState::replace` says so — the
    // instance drops when the LAST `Arc` does, which may be a worker thread on
    // its way out rather than the registry. Asserting the instant the count hits
    // zero races that thread and fails on a loaded machine, while the claim
    // being made — that the directory goes with its turn rather than outliving
    // it — is about the end state, not about which microsecond it arrives in.
    let gone = |p: &std::path::Path| {
        for _ in 0..600 {
            if !p.exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    };
    assert!(gone(&a_temp), "the first temp directory went with its turn");
    assert!(gone(&b_temp), "the second temp directory went with its turn");
}
