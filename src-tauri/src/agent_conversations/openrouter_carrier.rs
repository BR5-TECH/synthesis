//! The OpenRouter-carried call, and the bridge into the SDK's own shapes.
//!
//! OpenRouter is reached through its own SDK rather than through `rig`,
//! because the fields a conversation needs — the reasoning ladder, the cache
//! markers, the provider-native entries — have no place in `rig`'s request.

use super::*;

/// CVL-FR-30 / CVL-FR-10: what an OpenRouter call presents, assembled from the
/// request `rig` already produced.
///
/// Separated from [`carry_openrouter`] for the reason [`anthropic_model`] is
/// separated from its own carrier: what a request *asks for* is then readable
/// without a network call, and a test pins the tool array directly rather than
/// inferring it from a response.
pub(super) fn openrouter_request(
    request: &AgentRequest,
    exchange: &[rig::completion::Message],
    endpoint: &AiApiCall,
) -> Result<openrouter_rs::api::chat::ChatCompletionRequest, CallFailure> {
    use openrouter_rs::types::ReasoningConfig;

    // CVL-FR-10: `rig` assembles the request here exactly as it does for every
    // other provider; what follows translates what it produced.
    let assembled = build_completion_request(request, exchange, endpoint);
    // CVL-FR-39: where the material under discussion stops and the conversation
    // about it begins, as text rather than as an offset — the bridge locates it
    // as a prefix of the message it is about to carry, so a head the two
    // renderers disagree about costs a mark rather than splitting in the wrong
    // place.
    let hints = openrouter_bridge::CacheHints {
        stable_head: stable_head_text(request, exchange),
    };
    let messages = openrouter_bridge::messages_out(&assembled, &hints);
    let model = endpoint.model_id.clone().unwrap_or_default();
    let mut builder = ChatCompletionRequestBuilderShim::new(model.clone(), messages);
    // CVL-FR-23: this client can be told to advance the boundary itself, so it
    // is told. What a call ends with is what the call after it presents again,
    // and a boundary the provider moves reaches the replies and the tool results
    // a round accumulated — the part this module could otherwise mark only by
    // reshaping a tool result into a form not every endpoint behind this one
    // accepts.
    builder.advancing_boundary = Some(openrouter_rs::api::chat::CacheControl::ephemeral());
    // CVL-FR-40: a cache belongs to the upstream that wrote it, so the call asks
    // for the one that served the last answered call of this model. Fallback
    // stays allowed: an upstream that has gone away costs this call its cache
    // and never the call itself.
    builder.upstream = upstream_for(&model);
    // CVL-FR-10: the same definitions every other provider is offered, in this
    // SDK's own shape.
    builder.tools = assembled
        .tools
        .iter()
        .map(openrouter_bridge::tool_out)
        .collect();
    // CVL-FR-30 / WST-FR-02 / WFT-FR-02: the entries exactly as their own
    // modules fix them, each carrying a `type` and no other field — so the
    // search and the fetch are served by OpenRouter's own default engine
    // selection (WST-FR-03, WFT-FR-03). This SDK writes both kinds into one
    // `tools` array on the wire, which is where they belong.
    builder.server_tools = request
        .native_tools
        .iter()
        .map(|tool| openrouter_rs::types::ServerTool::new(tool.tool_type.clone()))
        .collect();
    if let Some(choice) = endpoint.reasoning.as_ref() {
        builder.reasoning = Some(match choice {
            // AAP-FR-27: "reason as little as the model permits". The SDK's
            // constructors cover every shape but this one, and the struct is
            // `#[non_exhaustive]`, so it is built from `enabled()` and turned
            // off rather than by a struct expression that a new field would
            // break.
            ai_api::ReasoningChoice::Off => {
                let mut config = ReasoningConfig::enabled();
                config.enabled = Some(false);
                config
            }
            ai_api::ReasoningChoice::On => ReasoningConfig::enabled(),
            ai_api::ReasoningChoice::Effort { effort } => {
                ReasoningConfig::with_effort(effort_of(effort))
            }
        });
    }
    builder
        .build()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))
}

/// CVL-FR-21 / CVL-FR-35: one OpenRouter failure, classified from what the SDK
/// actually reports rather than from the text of its `Display`.
///
/// This client answers a refused request with a *structured* error — a status,
/// the provider's own error code, and the provider's own request id — and
/// reducing that to a string only to scan the string for a three-digit number
/// throws away the two fields a reader needs to take a refusal to the provider,
/// and can find a "status" in text that never carried one. The classification is
/// the same one the generic path reaches (`ai_shared::probe_failure_for_status`
/// decides what a status means), so a log line, a retry, and a reported failure
/// still cannot disagree.
///
/// Nothing of the provider's message crosses this boundary (CVL-FR-21): a code
/// is a number of the provider's own vocabulary and a request id is opaque.
pub(super) fn openrouter_failure(error: &openrouter_rs::error::OpenRouterError) -> CallFailure {
    use openrouter_rs::error::OpenRouterError;
    match error {
        OpenRouterError::Api(context) => {
            let status = context.status.as_u16();
            // CVL-FR-18: the client knows which statuses repeating could help,
            // and a refusal it calls unrepeatable is not made three times. A 400
            // is the request's own shape, and the same request will be refused
            // in the same way for as long as it is sent.
            let failure = if matches!(status, 401 | 403) {
                FAIL_REJECTED
            } else {
                FAIL_UNREACHABLE
            };
            let class = if matches!(status, 401 | 403) {
                class::AUTH
            } else {
                class::HTTP_STATUS
            };
            CallFailure {
                failure,
                class,
                status: Some(status),
                provider_code: context.api_code,
                provider_request_id: context.request_id.clone(),
                // CVL-FR-35: bounded, because a provider is free to answer with
                // as much text as it likes and a log line is not the place to
                // find out how much that is.
                provider_message: Some(truncated(&context.message, PROVIDER_MESSAGE_LIMIT)),
            }
        }
        // A transport failure carries no structure at all, so the text-scanning
        // classification of CVL-FR-21 is what is left — and it is applied to a
        // string that never reaches a record.
        other => classify_provider_error(&other.to_string()),
    }
}

/// CVL-FR-10: the OpenRouter call, carried by `openrouter-rs`. The request was
/// assembled by `rig` exactly as every other provider's was — the client is the
/// only thing that differs.
pub(super) async fn carry_openrouter(
    built: openrouter_rs::api::chat::ChatCompletionRequest,
    endpoint: AiApiCall,
) -> Result<ModelReply, CallFailure> {
    let client = openrouter_rs::OpenRouterClient::builder()
        .base_url(endpoint.base_url.clone())
        .api_key(endpoint.api_key.clone().unwrap_or_default())
        .build()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))?;

    // CVL-FR-34: how many messages this call presented, which is what makes an
    // id derived from a call's position unique across the whole turn rather than
    // only within the reply that carried it. The exchange only grows
    // (CVL-FR-12), so no two rounds of one turn share this number — and the id
    // is what ties a record to the provider's own trace, so two calls sharing
    // one would make two records nobody can tell apart.
    let round = built.messages().len();
    let response = client
        .send_chat_completion(&built)
        .await
        .map_err(|e| openrouter_failure(&e))?;
    Ok(reply_from_openrouter(
        &response,
        endpoint.model_id.as_deref().unwrap_or_default(),
        round,
    ))
}

/// CVL-FR-10 / CVL-FR-40: what one OpenRouter answer comes to.
///
/// Separated from [`carry_openrouter`] so the whole of it — the reply, the
/// provider's own traffic, and the routing this call is remembered by — is
/// reachable from a test without a request leaving the machine, on the same
/// principle that separates [`anthropic_model`] from its own carrier.
pub(super) fn reply_from_openrouter(
    response: &openrouter_rs::types::completion::CompletionsResponse,
    model: &str,
    round: usize,
) -> ModelReply {
    let text: String = response
        .choices
        .iter()
        .filter_map(|c| c.content())
        .collect();
    // Emptiness is the loop's business here as it is on every other provider
    // (CVL-FR-13): a reply carrying tool calls and no prose is the ordinary
    // middle of a turn.
    // CVL-FR-31: what the provider ran for itself while carrying this call.
    // Reported rather than requested, and finished rather than pending: the
    // provider executed each of these inside this call and gave the model the
    // result before the reply was composed. The loop dispatches none of it and
    // replays none of it — it reports it and records it (CVL-FR-33, CVL-FR-34).
    let native_calls = openrouter_bridge::native_calls_in(
        response
            .choices
            .iter()
            .filter_map(|choice| choice.reasoning_details())
            .flatten(),
        round,
    );
    // CVL-FR-34: the provider's own count of what its tool loop did, which is
    // the only evidence that a call the model asked for was refused or dropped
    // rather than carried out — a requested total above an executed one.
    //
    // Read through the SDK's `ResponseUsage`, which carries the block under
    // `server_tool_use_details` — confirmed against a live chat completion,
    // which returns exactly that key. (The `server_tool_use` spelling in
    // OpenRouter's server-tools guide belongs to its Messages API, whose `usage`
    // object counts `input_tokens` and `output_tokens` rather than the
    // `prompt_tokens` this one carries.)
    //
    // These counts are the *only* report of a search on a provider that serves
    // one natively: such a reply carries `reasoning.summary` and
    // `reasoning.encrypted` blocks, none of them naming a tool, and its
    // citations ride in `annotations` instead — so `native_calls_in` finds
    // nothing and this is what says a search happened at all.
    let native_usage = response.usage.as_ref().and_then(|usage| {
        usage
            .server_tool_use_details
            .as_ref()
            .and_then(|details| {
                NativeToolUsage::reported(
                    details.tool_calls_requested,
                    details.tool_calls_executed,
                    details.web_search_requests,
                )
            })
    });
    // CVL-FR-40: what the client says of the call it carried, remembered for the
    // next call of this model. Written from every answered call rather than from
    // the first alone, so a call that fell back re-pins to wherever it landed.
    let served_by = response
        .provider
        .as_deref()
        .filter(|upstream| !upstream.is_empty())
        .map(str::to_string);
    if let Some(upstream) = served_by.as_deref() {
        remember_upstream(model, upstream);
    }
    ModelReply {
        native_calls,
        native_usage,
        native_entries_dropped: 0,
        served_by,
        // CVL-FR-28: what this one call presented, as this client counted it.
        // A count of zero is read as not counted, on the same terms
        // `input_tokens_of` reads an all-zero report: no call presents nothing,
        // every one of them carrying at least the compiled prompt.
        prompt_tokens: response
            .usage
            .as_ref()
            .map(|usage| usage.prompt_tokens as u64)
            .filter(|tokens| *tokens > 0),
        // CVL-FR-WQZD: the output half of the same report, read on the same
        // terms — a count of zero is read as not counted rather than as a claim
        // the model produced nothing.
        output_tokens: response
            .usage
            .as_ref()
            .map(|usage| usage.completion_tokens as u64)
            .filter(|tokens| *tokens > 0),
        // CVL-FR-28: this client's usage report carries a prompt-token total and
        // no cached/uncached split, so this provider reports neither and
        // contributes nothing to either total. A total reported as wholly
        // uncached would be a confident claim that caching had not landed, which
        // is the one thing the record must not say when it does not know.
        input_tokens: None,
        text,
        tool_calls: response
            .choices
            .iter()
            .filter_map(|choice| choice.tool_calls())
            .flatten()
            .map(openrouter_bridge::tool_call_in)
            .collect(),
    }
}

/// The OpenRouter SDK's shapes, on both sides of a call.
///
/// `rig` assembled the request (CVL-FR-10), so everything here is a translation
/// of what it produced rather than a second assembly: the tools, the messages,
/// and the tool calls that come back. Kept together so the round trip is
/// readable in one place — a mismatch between what goes out and what comes back
/// is exactly the bug this module would otherwise hide.
pub(super) mod openrouter_bridge {
    use openrouter_rs::api::chat::Message as OrMessage;
    use openrouter_rs::types::Role;
    use rig::completion::message::{AssistantContent, ToolCall, ToolFunction, UserContent};
    use rig::completion::{Message, ToolDefinition};

    /// One tool definition, in the SDK's shape (CVL-FR-10).
    pub fn tool_out(definition: &ToolDefinition) -> openrouter_rs::types::Tool {
        openrouter_rs::types::Tool::new(
            &definition.name,
            &definition.description,
            definition.parameters.clone(),
        )
    }

    /// CVL-FR-23: mark one message so the provider caches everything up to and
    /// including it.
    ///
    /// A marker rides on a *content part* rather than on a message, so a message
    /// carrying plain text becomes a one-part message to carry one. Anything
    /// already in parts has its last text part marked, and a message with no
    /// text part at all is left alone — there is nothing there to hang a marker
    /// on, and an unmarked message is a missed cache rather than a broken call.
    /// CVL-FR-39: where the input stops being stable, as the text of its head.
    ///
    /// Empty where the material has no head worth marking apart, which is how an
    /// input reaches the wire exactly as an unsplit one does.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct CacheHints {
        pub stable_head: Option<String>,
    }

    /// The text of one message, its parts in order and joined the way
    /// [`messages_out`] joins them.
    ///
    /// Shared with `stable_head_text` so the head and the message it must be a
    /// prefix of are produced by one rule rather than by two that could drift.
    pub fn user_message_text(message: &Message) -> String {
        let Message::User { content } = message else {
            return String::new();
        };
        content
            .iter()
            .filter_map(|part| match part {
                UserContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// CVL-FR-39: mark a message at the end of `head`, splitting its text there.
    ///
    /// The two parts concatenate to exactly the text that arrived — no separator
    /// is added and none is removed — so the model reads the message it always
    /// read and only the marking differs. `false` where `head` is not a prefix
    /// of the message's text, or where it is the whole of it: the caller then
    /// marks the message whole, which is the mark that would have stood at that
    /// boundary anyway.
    fn mark_at_head(message: &mut OrMessage, head: &str) -> bool {
        use openrouter_rs::api::chat::{CacheControl, Content, ContentPart};
        if head.is_empty() {
            return false;
        }
        // Only a message that reached here as plain text is split. A message
        // already in parts arrived that way from something other than this
        // marking, and cutting a part list is a shape decision that belongs
        // where the parts were made.
        let Content::Text(text) = &message.content else {
            return false;
        };
        let Some(tail) = text.strip_prefix(head) else {
            return false;
        };
        if tail.is_empty() {
            return false;
        }
        message.content = Content::Parts(vec![
            ContentPart::text_with_cache_control(head.to_string(), CacheControl::ephemeral()),
            ContentPart::text(tail.to_string()),
        ]);
        true
    }

    fn mark_for_caching(message: &mut OrMessage) {
        use openrouter_rs::api::chat::{CacheControl, Content, ContentPart};
        match &mut message.content {
            Content::Text(text) => {
                message.content = Content::Parts(vec![ContentPart::text_with_cache_control(
                    text.clone(),
                    // The short lifetime, which is this SDK's default when no
                    // TTL is named and which spans a whole loop by a wide margin
                    // under the bound of CVL-FR-16.
                    CacheControl::ephemeral(),
                )]);
            }
            Content::Parts(parts) => {
                if let Some(ContentPart::Text { cache_control, .. }) = parts
                    .iter_mut()
                    .rev()
                    .find(|part| matches!(part, ContentPart::Text { .. }))
                {
                    *cache_control = Some(CacheControl::ephemeral());
                }
            }
            // A content shape this build has never heard of, the SDK's enum
            // being open. There is nothing here to hang a marker on, and a
            // missed cache is the right cost for that — the alternative is a
            // call this module declines to make over a shape it did not
            // recognise.
            _ => {}
        }
    }

    /// The exchange, in the SDK's shape.
    ///
    /// The preamble leads as a system message, then every message of the
    /// exchange in order — the input, each reply, and each tool result — so the
    /// model sees the same conversation here that it sees on every other
    /// provider.
    ///
    /// CVL-FR-23: two marks are placed here, and both name a **fixed** place.
    /// The compiled prompt is one, and the end of the input's stable head is the
    /// other (CVL-FR-39) — the material under discussion, which the next turn of
    /// the conversation presents again unchanged while the discussion after it
    /// grows. The tool definitions the request carries ahead of the prompt are
    /// fixed too (CVL-FR-07), so what those two marks hold spans turns rather
    /// than only rounds.
    ///
    /// What grows is not marked here at all. The request asks the client to
    /// advance a boundary of its own (CVL-FR-23), which reaches the replies and
    /// the tool results a round accumulated without this module reshaping a tool
    /// result into an array of parts — a shape not every endpoint behind this one
    /// accepts. A reply carrying tool calls and no prose has no text part to hang
    /// a mark on, and needs none for the same reason.
    ///
    /// The marks are derived from the exchange rather than chosen per attempt, so
    /// a retry of a failed call is marked exactly as that call was (CVL-FR-20).
    pub fn messages_out(
        request: &rig::completion::CompletionRequest,
        hints: &CacheHints,
    ) -> Vec<OrMessage> {
        let mut out = Vec::new();
        if let Some(preamble) = request.preamble.as_ref().filter(|p| !p.is_empty()) {
            out.push(OrMessage::new(Role::System, preamble.as_str()));
        }
        for message in request.chat_history.iter() {
            match message {
                Message::User { content } => {
                    // A tool result is its own message with its own id, so it is
                    // emitted separately rather than folded into the user text
                    // beside it — a provider matches a result to the call it
                    // answers by that id and nothing else.
                    //
                    // Text is flushed *before* each result rather than gathered
                    // to the end, so a message carrying both emits them in the
                    // order it holds them. Nothing builds such a message today,
                    // and a reordering that only appears once something does is
                    // the kind that is found in production rather than here.
                    let mut text: Vec<String> = Vec::new();
                    let flush = |text: &mut Vec<String>, out: &mut Vec<OrMessage>| {
                        if !text.is_empty() {
                            out.push(OrMessage::new(Role::User, text.join("\n").as_str()));
                            text.clear();
                        }
                    };
                    for part in content.iter() {
                        match part {
                            UserContent::Text(t) => text.push(t.text.clone()),
                            UserContent::ToolResult(result) => {
                                flush(&mut text, &mut out);
                                out.push(OrMessage::tool_response(
                                    &result.id,
                                    render_tool_result(result),
                                ));
                            }
                            _ => {}
                        }
                    }
                    flush(&mut text, &mut out);
                }
                Message::Assistant { content, .. } => {
                    let mut text = String::new();
                    let mut calls = Vec::new();
                    for part in content.iter() {
                        match part {
                            AssistantContent::Text(t) => text.push_str(&t.text),
                            AssistantContent::ToolCall(call) => calls.push(tool_call_out(call)),
                            _ => {}
                        }
                    }
                    if calls.is_empty() {
                        out.push(OrMessage::new(Role::Assistant, text.as_str()));
                    } else {
                        out.push(OrMessage::assistant_with_tool_calls(text.as_str(), calls));
                    }
                }
                Message::System { content } => {
                    out.push(OrMessage::new(Role::System, content.as_str()));
                }
            }
        }
        // CVL-FR-23: the compiled prompt, then the input — the first user
        // message, the exchange opening with it and growing only after it
        // (CVL-FR-12). Found by role rather than by index, so a preamble that is
        // absent costs the input its mark rather than moving it onto the wrong
        // message.
        if let Some(system) = out.iter_mut().find(|m| matches!(m.role, Role::System)) {
            mark_for_caching(system);
        }
        if let Some(input) = out.iter_mut().find(|m| matches!(m.role, Role::User)) {
            // CVL-FR-39: the head where the material has one, the whole message
            // where it has none. A head the message does not open with, and a
            // head that is the whole of it, both fall back here rather than
            // splitting — an unsplit input is marked exactly as it always was.
            let split = hints
                .stable_head
                .as_deref()
                .is_some_and(|head| mark_at_head(input, head));
            if !split {
                mark_for_caching(input);
            }
        }
        out
    }

    /// A tool result's text. Every tool in this group returns text (TLC-FR-08),
    /// so anything else is not something this application put there.
    pub(in crate::agent_conversations) fn render_tool_result(result: &rig::completion::message::ToolResult) -> String {
        use rig::completion::message::ToolResultContent;
        result
            .content
            .iter()
            .filter_map(|part| match part {
                ToolResultContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// CVL-FR-31: one tool the provider ran for itself, as this client reports
    /// it.
    ///
    /// OpenRouter carries a server tool's call and the result it produced in the
    /// reply's reasoning blocks rather than in its tool calls — which is exactly
    /// right, since there is nothing here to dispatch: the call is a transcript
    /// of work the provider already did inside this call. A block naming no tool
    /// is the model's own reasoning and is not one of these; `round` supplies an
    /// id only where the provider named none, so every reported call still
    /// carries one a record can be correlated by (CVL-FR-34).
    ///
    /// Nothing is reshaped on the way through: the name, the arguments, and the
    /// result are carried as the provider produced them (WST-FR-08, WFT-FR-08).
    pub fn native_calls_in<'a>(
        details: impl Iterator<Item = &'a openrouter_rs::types::ReasoningDetail>,
        round: usize,
    ) -> Vec<super::NativeToolCall> {
        let mut out: Vec<super::NativeToolCall> = Vec::new();
        for detail in details {
            let Some(name) = detail.tool_name.clone() else {
                continue;
            };
            // A provider free to report a call and its result as two blocks
            // sharing one id is folded back into the one call it was, rather
            // than reaching the model as a call with no result followed by a
            // result with no arguments. Only a block naming the *same tool* is
            // folded: two tools reported under one id are two calls, and the
            // second keeps an id of its own rather than disappearing into the
            // first. A block naming no id is its own call — there is nothing to
            // fold it into.
            if let Some(id) = detail.tool_call_id.as_ref() {
                if let Some(existing) = out
                    .iter_mut()
                    .find(|call| &call.id == id && call.name == name)
                {
                    if existing.arguments.is_empty() {
                        existing.arguments = detail.arguments.clone().unwrap_or_default();
                    }
                    if existing.result.is_empty() {
                        existing.result = detail.result.clone().unwrap_or_default();
                    }
                    continue;
                }
            }
            let taken = |id: &str, out: &[super::NativeToolCall]| {
                out.iter().any(|call| call.id == id)
            };
            let id = match detail.tool_call_id.clone() {
                // A provider that names an id this reply has already used for
                // another tool does not get to collide with it.
                Some(id) if !taken(&id, &out) => id,
                _ => {
                    let mut candidate = format!("provider-tool-{round}-{}", out.len());
                    let mut nudge = out.len();
                    while taken(&candidate, &out) {
                        nudge += 1;
                        candidate = format!("provider-tool-{round}-{nudge}");
                    }
                    candidate
                }
            };
            out.push(super::NativeToolCall {
                id,
                name,
                arguments: detail.arguments.clone().unwrap_or_default(),
                result: detail.result.clone().unwrap_or_default(),
            });
        }
        out
    }

    /// A call the model asked for, on its way back out in a later request.
    ///
    /// Arguments travel as the JSON string this SDK's wire format uses, which is
    /// what the provider sent in the first place.
    pub(in crate::agent_conversations) fn tool_call_out(call: &ToolCall) -> openrouter_rs::types::ToolCall {
        openrouter_rs::types::ToolCall::new(
            call.id.clone(),
            call.function.name.clone(),
            call.function.arguments.to_string(),
        )
    }

    /// A call the model just asked for, in `rig`'s shape so the loop handles
    /// every provider's identically (CVL-FR-10).
    ///
    /// Arguments arrive as a JSON *string* and are parsed back into a value. A
    /// string that does not parse becomes `null` rather than failing the turn:
    /// the tool it names decides what to do with arguments it cannot use, and
    /// that refusal reaches the model as a result it can correct (CVL-FR-14),
    /// which is a better answer than ending the conversation here.
    pub fn tool_call_in(call: &openrouter_rs::types::ToolCall) -> ToolCall {
        ToolCall::new(
            call.id.clone(),
            ToolFunction {
                name: call.function.name.clone(),
                arguments: serde_json::from_str(&call.function.arguments)
                    .unwrap_or(serde_json::Value::Null),
            },
        )
    }
}

/// One of this application's effort identifiers as the OpenRouter SDK's own
/// level.
///
/// `Other` for anything the SDK does not know by name, which is the mirror of
/// how `crate::ai_openrouter` reads a level *out* of it (AAP-FR-26): a ladder
/// this build has never heard of still reaches the endpoint intact rather than
/// being dropped or coerced to a neighbouring level.
pub(super) fn effort_of(effort: &str) -> openrouter_rs::types::Effort {
    use openrouter_rs::types::Effort;
    match effort {
        "xhigh" => Effort::Xhigh,
        "max" => Effort::Max,
        "high" => Effort::High,
        "medium" => Effort::Medium,
        "low" => Effort::Low,
        "minimal" => Effort::Minimal,
        "none" => Effort::None,
        other => Effort::Other(other.to_string()),
    }
}
