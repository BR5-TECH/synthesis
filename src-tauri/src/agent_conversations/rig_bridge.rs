//! The `rig`-driven half of the completion seam.
//!
//! The dependency's blast radius is this file and `rig_messages`: the request
//! assembly, the shipped [`RigCompletion`] client, and the per-provider
//! carriers that choose which of `rig`'s clients answers a call.

use super::*;

pub(super) mod rig_seam {
    //! The `rig`-driven half of the seam, kept in one place so the dependency's
    //! blast radius is exactly this module (the same containment
    //! `crate::ai_openrouter` gives the OpenRouter SDK).

    use super::*;
    use rig::completion::message::{StopReason, ToolCall};
    use rig::completion::{
        AssistantContent, CacheCost, CompletionRequest, CompletionResponse, FinishReason, Message,
        Usage,
    };
    use rig::operation::Completion;
    use rig::{DynModel, ProviderError};

    /// CVL-FR-10: message construction and the completion request are `rig`'s,
    /// whichever provider ends up carrying the call — which is what makes the
    /// shape of a turn the same everywhere.
    pub fn build_completion_request(
        request: &AgentRequest,
        exchange: &[Message],
        endpoint: &AiApiCall,
    ) -> CompletionRequest {
        // The exchange always opens with the rendered input (CVL-FR-12), so an
        // empty slice can only mean a caller assembled one wrongly; falling back
        // to the input keeps `rig`'s non-empty rule without inventing a message
        // the model has not seen.
        let history = if exchange.is_empty() {
            vec![Message::user(render_input(request))]
        } else {
            exchange.to_vec()
        };
        let mut built = CompletionRequest::from(history)
            .model::<String>(endpoint.model_id.clone())
            // CVL-FR-10 / CVL-FR-08: the origin kind's tools, reaching the
            // provider as its own tool definitions. What differs between two
            // providers is the wire format the framework produces, never which
            // tools an agent was offered.
            .tools(request.tools.clone())
            .additional_params(reasoning_params(endpoint))
            // AGC-FR-26: content telemetry would put the assembled context — and
            // in principle anything in it — on a span attribute.
            .record_content_telemetry(false)
            // CVL-FR-UALC: a finish reason the framework does not know ends a
            // reply normally. Without this, a gateway that sends a reason of its
            // own would lose every answer.
            .accept_unknown_finish_reasons(true);
        // CVL-FR-10: the compiled prompt occupies the request's instruction
        // position, which is `rig`'s system instruction, and the input occupies
        // its first user message. No provider receives the two in any other
        // arrangement and none receives them merged into one.
        if !request.instructions.is_empty() {
            built = built.preamble(request.instructions.clone());
        }
        built
    }

    /// The reasoning the agent asked for, in the shape an OpenAI-compatible
    /// endpoint takes it. `None` sends nothing at all, leaving the model to its
    /// own default (AAP-FR-31).
    ///
    /// CVL-FR-HBNW: an Anthropic call carries no `reasoning` object. The
    /// Messages API refuses an unknown top-level field, and `rig` flattens these
    /// params into the top level of the body.
    fn reasoning_params(endpoint: &AiApiCall) -> Option<serde_json::Value> {
        use ai_api::ReasoningChoice;
        if endpoint.provider == "anthropic" {
            return None;
        }
        match endpoint.reasoning.as_ref()? {
            ReasoningChoice::Off => Some(serde_json::json!({ "reasoning": { "enabled": false } })),
            ReasoningChoice::On => Some(serde_json::json!({ "reasoning": { "enabled": true } })),
            ReasoningChoice::Effort { effort } => {
                Some(serde_json::json!({ "reasoning": { "effort": effort } }))
            }
        }
    }

    /// The prose a reply carried. Reasoning blocks and images are not a
    /// contribution to a conversation, so only text counts towards it.
    pub fn text_of(choice: &[AssistantContent]) -> String {
        let mut out = String::new();
        for content in choice {
            if let AssistantContent::Text(text) = content {
                out.push_str(&text.text);
            }
        }
        out
    }

    /// The tools a reply asked for, in the order it asked (CVL-FR-12).
    pub fn tool_calls_of(choice: &[AssistantContent]) -> Vec<ToolCall> {
        choice
            .iter()
            .filter_map(|content| match content {
                AssistantContent::ToolCall(call) => Some(call.clone()),
                _ => None,
            })
            .collect()
    }

    /// Drive any `rig` completion model through one round of the loop.
    ///
    /// Erased rather than monomorphised at each call site so the production
    /// clients and the scripted model of CVL-FR-11 travel exactly the same code
    /// path — a test that took a different path would be testing a different
    /// thing.
    ///
    /// An empty reply is *not* rejected here: a reply carrying tool calls and no
    /// prose is the ordinary middle of a loop, and whether emptiness ends the
    /// turn as `empty_reply` is the loop's decision (CVL-FR-13), not this
    /// function's.
    pub async fn run_model(
        model: impl Into<DynModel<Completion>>,
        request: CompletionRequest,
    ) -> Result<ModelReply, CallFailure> {
        let model = model.into();
        match model.call(request).await {
            Ok(response) => Ok(reply_of(&response)),
            Err(e) => Err(classify_completion_error(&e)),
        }
    }

    /// One reply, read out of what the framework returned.
    fn reply_of(response: &CompletionResponse) -> ModelReply {
        // CVL-FR-UALC: a reply the framework reports as failed — a refusal,
        // filtered content, or a failure the provider reported — supplies
        // neither usable prose nor a tool request, whatever it holds. It
        // reaches the loop as a reply with neither, which is `empty_reply`, so
        // none of its tools is dispatched and none of it is appended.
        let failed = matches!(response.stop(), StopReason::Error(_)) && !unstated_finish(response);
        let (text, tool_calls, turn) = if failed {
            (String::new(), Vec::new(), None)
        } else {
            (
                text_of(&response.choice),
                tool_calls_of(&response.choice),
                // CVL-FR-ETAH: the reply as the framework returned it, so the
                // next round presents its thinking and its identifiers back to
                // the provider unchanged.
                (!response.choice.is_empty())
                    .then(|| response.head().with_content(response.choice.clone())),
            )
        };
        let usage = &response.usage;
        ModelReply {
            text,
            tool_calls,
            turn,
            // CVL-FR-40: these providers are reached directly, so no upstream
            // stands between the call and the model that served it.
            served_by: None,
            // CVL-FR-28 / CVL-FR-WQZD: what this one call presented, as the
            // provider counted it — every prompt token, cache reads and cache
            // writes included. Absent where the provider reported none.
            prompt_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_write_tokens: usage.cache_creation_input_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            // CVL-FR-31: a provider-native tool is OpenRouter's alone
            // (TLC-FR-23), and `rig`'s own response shape carries no report of
            // one — the client that does fills this in for itself.
            native_calls: Vec::new(),
            native_usage: None,
            native_entries_dropped: 0,
            input_tokens: input_tokens_of(usage),
            // CVL-FR-28: the provider's own identifiers for the response and the
            // request, the model it says answered, and why the reply ended.
            response_id: response.origin.response_id.clone(),
            provider_request_id: response.provider_request_id.clone(),
            response_model: response.origin.response_model.clone(),
            finish_reason: response.finish_reason().as_ref().map(finish_reason_name),
            // The carrier sets this where it repaired the reply.
            reply_repairs: Default::default(),
        }
    }

    /// The text the framework gives a reply whose wire stated no finish reason.
    pub const NO_FINISH_REASON: &str = "the provider ended the reply without a finish reason";

    /// CVL-FR-UALC: whether the only failure the framework reports for a reply
    /// is that the wire stated no finish reason. That is no refusal, no
    /// filtered content, and no failure the provider reported: a gateway that
    /// leaves the field out keeps its answer.
    fn unstated_finish(response: &CompletionResponse) -> bool {
        response.finish_reason().is_none()
            && response.aborted.is_none()
            && response.error.as_deref() == Some(NO_FINISH_REASON)
    }

    /// CVL-FR-28: the reason a reply ended, in the framework's own vocabulary,
    /// or the provider's own word for one outside it.
    fn finish_reason_name(reason: &FinishReason) -> String {
        match reason {
            FinishReason::Stop => "stop".to_string(),
            FinishReason::Length => "length".to_string(),
            FinishReason::ToolCalls => "tool_calls".to_string(),
            FinishReason::ContentFilter => "content_filter".to_string(),
            FinishReason::Other(other) => other.clone(),
            _ => "other".to_string(),
        }
    }

    /// CVL-FR-21: classify a framework error by its kind before any text of it
    /// is read.
    ///
    /// A body the framework could not decode is `decode`: the provider answered,
    /// so it is not `unreachable`. A request the framework refused to build, or
    /// an option the model cannot honour, is `request`: nothing was sent, so it
    /// is not `unreachable` either. A credential the provider refused is
    /// `rejected`. A response the provider returned with an error status is a
    /// structured error (CVL-FR-35). Only an error whose kind does not settle
    /// its class goes to the text classification. No text of the error goes
    /// into the failure.
    pub fn classify_completion_error(error: &ProviderError) -> CallFailure {
        match error {
            ProviderError::Json(_) | ProviderError::Response(_) => {
                CallFailure::new(FAIL_INVALID_RESPONSE, class::DECODE)
            }
            // CVL-FR-21: the framework refused to build the request, so nothing
            // reached the provider. Its text can carry part of the request, so
            // it is not read.
            ProviderError::Request(_) | ProviderError::UnsupportedOption(_) => {
                CallFailure::new(FAIL_INVALID_REQUEST, class::REQUEST)
            }
            // CVL-FR-21: a reply that ended before the provider ended it is the
            // connection reset in flight.
            ProviderError::Truncated => CallFailure::unreachable(class::RESET),
            ProviderError::InvalidAuthentication(response) => {
                structured_failure(response, FAIL_REJECTED, class::AUTH)
            }
            ProviderError::ProviderResponse(response) => match response.status {
                Some(status) if matches!(status.as_u16(), 401 | 403) => {
                    structured_failure(response, FAIL_REJECTED, class::AUTH)
                }
                Some(status) if !status.is_success() => {
                    structured_failure(response, FAIL_UNREACHABLE, class::HTTP_STATUS)
                }
                // An error envelope the provider returned without a failure
                // status: a structured refusal all the same, so its code, its
                // request id, and its message are carried, and no status is
                // named that did not fail.
                _ => CallFailure {
                    status: None,
                    ..structured_failure(response, FAIL_UNREACHABLE, class::TRANSPORT_OTHER)
                },
            },
            other => classify_provider_error(&other.to_string()),
        }
    }

    /// CVL-FR-35: a structured provider error, recorded in the provider's own
    /// terms — its status, its own error code as text, its own identifier for
    /// the request, and its own message, bounded.
    ///
    /// The message is read from the parsed body's `error.message` alone, never
    /// from the rendered error, which names the route the request took, and it
    /// is cut by character on the terms the OpenRouter carrier cuts its own.
    fn structured_failure(
        response: &rig::ProviderResponseError,
        failure: &'static str,
        class: &'static str,
    ) -> CallFailure {
        let message = provider_message_of(&response.body)
            .map(|message| truncated(&message, PROVIDER_MESSAGE_LIMIT));
        CallFailure {
            failure,
            class,
            status: response.status.map(|status| status.as_u16()),
            provider_code: response.machine_code(),
            provider_request_id: response.provider_request_id.clone(),
            provider_message: message,
            tls: None,
            reply_repairs: Default::default(),
        }
    }

    /// The provider's own message in an error body: `error.message` of the
    /// envelope Anthropic and OpenAI both use, or a top-level `message`.
    pub(in super::super) fn provider_message_of(body: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(body).ok()?;
        value
            .pointer("/error/message")
            .or_else(|| value.get("message"))
            .and_then(|message| message.as_str())
            .filter(|message| !message.is_empty())
            .map(str::to_string)
    }

    /// CVL-FR-28 / CVL-FR-WQZD: one call's input tokens as the provider
    /// reported them, split into what it read from its cache and what it billed
    /// at the full rate, or `None` where it reported no input total.
    ///
    /// The input total counts every prompt token, cache reads and cache writes
    /// included, on every provider. The uncached part is that total less the
    /// cache reads and the cache writes.
    pub fn input_tokens_of(usage: &Usage) -> Option<InputTokens> {
        usage.input_tokens?;
        let cost = CacheCost::from_usage(usage);
        Some(InputTokens {
            cached: cost.cache_reads,
            uncached: cost.uncached_input,
        })
    }
}

pub use rig_seam::build_completion_request;
/// Exported so that a scripted seam belonging to another loop — the graduation
/// judgement of `../ai/GRL-graduation-loop.md` (GRL-FR-UVJP) — drives its replies
/// through the same assembly and the same extraction the production clients
/// use, rather than constructing a [`ModelReply`] beside them.
pub use rig_seam::run_model;

/// Run one future to completion on a thread this call owns, under a deadline.
///
/// A dedicated thread rather than a bare `block_on` for the reason
/// `crate::ai_openrouter` documents: a Tauri command may already be executing
/// inside a Tokio worker, and driving a nested runtime from there panics at
/// runtime — a failure no test without the full Tauri runtime would catch.
pub(super) fn block_on_with_timeout<F, T>(future: F, timeout: Duration) -> Result<T, CallFailure>
where
    F: std::future::Future<Output = Result<T, CallFailure>> + Send + 'static,
    T: Send + 'static,
{
    let handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return Err(CallFailure::unreachable(class::TRANSPORT_OTHER)),
        };
        // CVL-FR-16: the request is abandoned rather than waited on, so an
        // unanswering provider cannot leave a conversation waiting indefinitely.
        runtime.block_on(async move {
            match tokio::time::timeout(timeout, future).await {
                Ok(result) => result,
                Err(_) => Err(CallFailure::new(FAIL_TIMED_OUT, class::PROVIDER_TIMEOUT)),
            }
        })
    });
    handle.join().unwrap_or(Err(CallFailure::unreachable(class::TRANSPORT_OTHER)))
}

/// The production seam: `rig` composes every request, and the provider decides
/// only which client carries it (CVL-FR-10).
pub struct RigCompletion;

impl CompletionSeam for RigCompletion {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        match endpoint.provider.as_str() {
            // AAP-FR-23 / CVL-FR-10: OpenRouter alone is reached through its own
            // SDK, because it is the provider whose interface this application
            // already depends on that client for.
            //
            // CVL-FR-30: it is also the one provider with native entries to
            // carry, and `rig`'s own `CompletionRequest` has no shape for a tool
            // it does not define — so this arm assembles from the `AgentRequest`
            // itself rather than from what `rig` produced, and the entries reach
            // the wire in the same `tools` array the function definitions do.
            "openrouter" => carry_openrouter_narrowing(request, exchange, endpoint, timeout),
            "anthropic" => {
                let built = build_completion_request(request, exchange, endpoint);
                let endpoint = endpoint.clone();
                block_on_with_timeout(
                    async move { carry_anthropic(built, endpoint).await },
                    timeout,
                )
            }
            // OpenAI, and a custom endpoint, which is an OpenAI-compatible one by
            // construction (AAP-FR-01).
            _ => {
                let built = build_completion_request(request, exchange, endpoint);
                let endpoint = endpoint.clone();
                block_on_with_timeout(
                    async move { carry_openai(built, endpoint).await },
                    timeout,
                )
            }
        }
    }
}

async fn carry_anthropic(
    request: rig::completion::CompletionRequest,
    endpoint: AiApiCall,
) -> Result<ModelReply, CallFailure> {
    // AAP-FR-WMCX: the client is given the shared HTTP client, so it keeps no
    // trust setup of its own.
    let (http_client, tls_record) = crate::tls::rig_http_client()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))?;
    let client = rig::providers::anthropic::AnthropicConfig::new(
        endpoint.api_key.clone().unwrap_or_default(),
    )
    .with_base_url(&endpoint.base_url)
    .connect(rig_reqwest::ReqwestClient::from(http_client));
    let model = anthropic_model(&client, &endpoint);
    rig_seam::run_model(model, anthropic_request(request))
        .await
        .map_err(|failure| with_recorded_tls(failure, &tls_record))
}

/// CVL-FR-23: the request an Anthropic call carries, with caching asked for.
///
/// This provider caches only what it is told to cache, so it is told on every
/// call. The request's own retention rather than a breakpoint placed here,
/// because the breakpoint has to *move*: the exchange only grows (CVL-FR-12),
/// so what the next call presents again is the whole of what this one
/// presented, and the provider advancing its own breakpoint as the conversation
/// grows is exactly that rule expressed on its side.
///
/// The lifetime is the short one — five minutes, which spans a whole loop by a
/// wide margin under the bound of CVL-FR-16. Asked for on Anthropic alone: a
/// retention option on another provider's request switches on the framework's
/// refusal of options a model cannot honour, and those providers cache a
/// repeated prefix of their own accord.
pub(super) fn anthropic_request(
    request: rig::completion::CompletionRequest,
) -> rig::completion::CompletionRequest {
    request.cache(rig::completion::CacheRetention::Short)
}

/// The Anthropic completion model a turn's calls are carried by.
///
/// CVL-FR-KXTQ: the output-token limit is set here on every model. `rig` knows
/// a default only for the model ids in its own catalog, and without a limit it
/// refuses the call before it sends a request.
///
/// CVL-FR-HBNW: nothing here chooses thinking. The framework applies its own
/// default for the model, which is adaptive thinking where its catalog marks
/// the model for it.
///
/// Separated from [`carry_anthropic`] so what is asked for is readable without a
/// network call: a test asserts the model directly rather than inferring it
/// from a response.
pub(super) fn anthropic_model(
    client: &rig::providers::anthropic::Anthropic,
    endpoint: &AiApiCall,
) -> rig::Model<rig::providers::anthropic::Messages> {
    let mut model = client.completion(endpoint.model_id.clone().unwrap_or_default());
    model.wire = model.wire.with_default_max_tokens(ANTHROPIC_MAX_OUTPUT_TOKENS);
    model
}

/// CVL-FR-KXTQ: the output-token limit of every Anthropic call. All current
/// Anthropic models accept it.
pub(super) const ANTHROPIC_MAX_OUTPUT_TOKENS: u64 = 32_000;

/// Which conversation route the OpenAI-compatible adapter takes (CVL-FR-ZPGW).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpenAiRoute {
    /// No restriction: the adapter selects the route as it does for any
    /// unrestricted model, with the fallback behaviour it already has. This
    /// layer adds no preference.
    Unrestricted,
    /// Chat Completions only.
    ChatCompletions,
    /// Responses only.
    Responses,
}

/// What the OpenAI-compatible adapter is configured with for one call
/// (AAP-FR-ADPX), apart from the secret, which is handed to the client builder
/// alone and kept out of this value so it cannot be formatted by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OpenAiAdapterConfig {
    pub base_url: String,
    pub route: OpenAiRoute,
    /// CVL-FR-TQRD: whether a Responses reply gets the tolerances of the Custom
    /// gateway (`text.format` and `output_text` parts without text). Set for
    /// the Custom gateway alone.
    pub repair_replies: bool,
}

/// AAP-FR-ADPX / CVL-FR-ZPGW: the adapter configuration an endpoint calls for.
///
/// The Custom gateway's root carries no `/v1`, which the framework's route paths
/// hang from, so it is added here. The model's `mode` restricts the route; a
/// missing mode leaves the adapter unrestricted. Every other provider reaches
/// the adapter exactly as before.
pub(super) fn openai_adapter_config(endpoint: &AiApiCall) -> OpenAiAdapterConfig {
    if endpoint.provider != "custom" {
        return OpenAiAdapterConfig {
            base_url: endpoint.base_url.clone(),
            route: OpenAiRoute::Unrestricted,
            repair_replies: false,
        };
    }
    OpenAiAdapterConfig {
        base_url: ai_api::custom_gateway::adapter_base_url(&endpoint.base_url),
        route: match endpoint.model_mode {
            Some(crate::ai_shared::ModelMode::Chat) => OpenAiRoute::ChatCompletions,
            Some(crate::ai_shared::ModelMode::Responses) => OpenAiRoute::Responses,
            None => OpenAiRoute::Unrestricted,
        },
        repair_replies: true,
    }
}

/// CVL-FR-23: nothing here marks anything for caching, and that is the whole of
/// this provider's implementation of it. An OpenAI-compatible endpoint caches a
/// repeated prefix of its own accord and offers no way to be told to, so it is
/// relied on to do it — the exchange growing only at its end (CVL-FR-12) is what
/// makes the prefix it matches on the whole of the previous call.
///
/// Carries `openai` and `custom` alike (CVL-FR-ZPGW): the secret is presented as
/// a bearer token by the client.
async fn carry_openai(
    request: rig::completion::CompletionRequest,
    endpoint: AiApiCall,
) -> Result<ModelReply, CallFailure> {
    let config = openai_adapter_config(&endpoint);
    // AAP-FR-WMCX: the client is given the shared HTTP client, so it keeps no
    // trust setup of its own.
    let (http_client, tls_record) = crate::tls::rig_http_client()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))?;
    // CVL-FR-TQRD: the Custom gateway's replies are repaired before `rig` reads
    // them. Every other provider's client passes its replies unchanged.
    let http_client = super::responses_repair::ResponsesRepair::new(
        rig_reqwest::ReqwestClient::from(http_client),
        config.repair_replies,
    );
    let repair = http_client.clone();
    let client = rig::providers::openai::OpenAIConfig::new(
        endpoint.api_key.clone().unwrap_or_default(),
    )
    .with_base_url(&config.base_url)
    .connect(http_client);
    let model_id = endpoint.model_id.clone().unwrap_or_default();
    let result = match config.route {
        // The client's own route is Responses, which is also the only route a
        // `responses` model permits, so one arm serves both.
        OpenAiRoute::Unrestricted | OpenAiRoute::Responses => {
            rig_seam::run_model(client.responses(model_id), request).await
        }
        OpenAiRoute::ChatCompletions => rig_seam::run_model(client.chat(model_id), request).await,
    };
    result
        .map(|mut reply| {
            reply.reply_repairs = repair.repairs();
            reply
        })
        .map_err(|mut failure| {
            // A repaired reply the framework still refused is recorded too.
            failure.reply_repairs = repair.repairs();
            with_recorded_tls(failure, &tls_record)
        })
}

/// CVL-FR-36: make the call, and narrow the provider-native entries it carries
/// until the provider stops refusing them.
///
/// OpenRouter refuses a whole request with a `400` when it cannot serve one of
/// the server tools that request offers, and it refuses it *before* the model
/// sees anything — so the graceful path of CVL-FR-32, where a provider's own
/// tool failure is material the model reads, never opens. One entry the model
/// never used is enough to cost a turn everything.
///
/// Which entries a model can be offered is not knowable in advance: the model
/// listing declares nothing about server tools, and two models whose declared
/// parameters are identical differ in what they will serve. So it is learned
/// from the refusal, and remembered for the model it was learned from, and the
/// narrowing keeps the entries whose capability the model has not refused.
///
/// The order is deliberate. `web_fetch` is dropped first because a provider that
/// serves neither will not serve it, and a provider that serves one serves
/// search: search alone leaves an agent able to reach the web, while fetch alone
/// leaves it able to read only pages whose address it already has.
fn carry_openrouter_narrowing(
    request: &AgentRequest,
    exchange: &[rig::completion::Message],
    endpoint: &AiApiCall,
    timeout: Duration,
) -> Result<ModelReply, CallFailure> {
    let model = endpoint.model_id.clone().unwrap_or_default();
    let mut narrowed = request.clone();
    // What this model is already known to refuse, so a turn pays for the lesson
    // once rather than on every call.
    let mut dropped = native_entries_refused_by(&model).min(narrowed.native_tools.len());

    loop {
        narrowed.native_tools = request.native_tools
            [..request.native_tools.len() - dropped]
            .to_vec();
        let built = openrouter_request(&narrowed, exchange, endpoint)?;
        let carried = {
            let endpoint = endpoint.clone();
            block_on_with_timeout(
                async move { carry_openrouter(built, endpoint).await },
                timeout,
            )
        };
        let refusal = match &carried {
            Err(failure) if refuses_a_server_tool(failure) => failure,
            _ => {
                return carried.map(|mut reply| {
                    reply.native_entries_dropped = dropped;
                    reply
                })
            }
        };
        // Nothing left to give up, so the refusal is the answer. A 400 that
        // outlives every entry was never about them.
        if narrowed.native_tools.is_empty() {
            let _ = refusal;
            return carried;
        }
        dropped += 1;
        remember_native_entries_refused(&model, dropped);
    }
}
