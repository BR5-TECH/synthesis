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
    use rig::completion::message::ToolCall;
    use rig::completion::{AssistantContent, CompletionModel, CompletionRequest, Message};
    use rig::OneOrMany;

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
        // to the input keeps `rig`'s non-empty invariant without inventing a
        // message the model has not seen.
        let history = OneOrMany::many(exchange.iter().cloned())
            .unwrap_or_else(|_| OneOrMany::one(Message::user(render_input(request))));
        CompletionRequest {
            model: endpoint.model_id.clone(),
            // CVL-FR-10: the compiled prompt occupies the request's instruction
            // position, which is `rig`'s preamble, and the input occupies its
            // first user message. No provider receives the two in any other
            // arrangement and none receives them merged into one.
            preamble: Some(request.instructions.clone()).filter(|s| !s.is_empty()),
            chat_history: history,
            documents: Vec::new(),
            // CVL-FR-10 / CVL-FR-08: the origin kind's tools, reaching the provider as
            // its own tool definitions. What differs between two providers is the
            // wire format the framework produces, never which tools an agent was
            // offered.
            tools: request.tools.clone(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: reasoning_params(endpoint),
            output_schema: None,
            // AGC-FR-26: content telemetry would put the assembled context — and
            // in principle anything in it — on a span attribute.
            record_telemetry_content: false,
        }
    }

    /// The reasoning the agent asked for, in the shape an OpenAI-compatible
    /// endpoint takes it. `None` sends nothing at all, leaving the model to its
    /// own default (AAP-FR-31).
    fn reasoning_params(endpoint: &AiApiCall) -> Option<serde_json::Value> {
        use ai_api::ReasoningChoice;
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
    pub fn text_of(choice: &OneOrMany<AssistantContent>) -> String {
        let mut out = String::new();
        for content in choice.iter() {
            if let AssistantContent::Text(text) = content {
                out.push_str(&text.text);
            }
        }
        out
    }

    /// The tools a reply asked for, in the order it asked (CVL-FR-12).
    pub fn tool_calls_of(choice: &OneOrMany<AssistantContent>) -> Vec<ToolCall> {
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
    /// Generic rather than monomorphised at each call site so the production
    /// clients and the scripted model of CVL-FR-11 travel exactly the same code
    /// path — a test that took a different path would be testing a different
    /// thing.
    ///
    /// An empty reply is *not* rejected here: a reply carrying tool calls and no
    /// prose is the ordinary middle of a loop, and whether emptiness ends the
    /// turn as `empty_reply` is the loop's decision (CVL-FR-13), not this
    /// function's.
    pub async fn run_model<M: CompletionModel>(
        model: M,
        request: CompletionRequest,
    ) -> Result<ModelReply, CallFailure> {
        match model.completion(request).await {
            Ok(response) => Ok(ModelReply {
                text: text_of(&response.choice),
                tool_calls: tool_calls_of(&response.choice),
                // CVL-FR-40: these providers are reached directly, so no
                // upstream stands between the call and the model that served it.
                served_by: None,
                // CVL-FR-28: what this one call presented, as the provider
                // counted it. Zero is read as not counted, on the same terms
                // `input_tokens_of` reads an all-zero report.
                prompt_tokens: (response.usage.input_tokens > 0)
                    .then_some(response.usage.input_tokens),
                // CVL-FR-WQZD: the output half of the same report, read on
                // exactly the terms the input half is — zero means not counted,
                // `rig` reducing a missing usage report to a zero-valued
                // `Usage` rather than to an absent one.
                output_tokens: (response.usage.output_tokens > 0)
                    .then_some(response.usage.output_tokens),
                // CVL-FR-31: a provider-native tool is OpenRouter's alone
                // (TLC-FR-23), and `rig`'s own response shape carries no report
                // of one — the client that does fills this in for itself.
                native_calls: Vec::new(),
                native_usage: None,
                native_entries_dropped: 0,
                input_tokens: input_tokens_of(&response.usage),
            }),
            Err(e) => Err(classify_provider_error(&e.to_string())),
        }
    }

    /// CVL-FR-28: one call's input tokens as `rig` reports them, or `None` where
    /// the provider reported nothing at all.
    ///
    /// `rig` reduces a missing usage report to a zero-valued `Usage` rather than
    /// to an absent one, so an all-zero report is read here as *not reported*.
    /// A call that genuinely consumed no input tokens does not exist — every call
    /// carries at least the compiled prompt — so the ambiguity costs nothing and
    /// the alternative would record a confident zero for every provider that
    /// declines to say (CVL-FR-28).
    pub fn input_tokens_of(usage: &rig::completion::Usage) -> Option<InputTokens> {
        if usage.input_tokens == 0 && usage.cached_input_tokens == 0 {
            return None;
        }
        Some(InputTokens {
            cached: usage.cached_input_tokens,
            uncached: usage.input_tokens,
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
    let client = rig::providers::anthropic::Client::builder()
        .api_key(endpoint.api_key.clone().unwrap_or_default())
        .base_url(&endpoint.base_url)
        .build()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))?;
    let model = anthropic_model(&client, &endpoint);
    rig_seam::run_model(model, request).await
}

/// CVL-FR-23: the Anthropic completion model a turn's calls are carried by, with
/// caching asked for.
///
/// This provider caches only what it is told to cache, so it is told on every
/// call. Automatic caching rather than a breakpoint placed here, because the
/// breakpoint has to *move*: the exchange only grows (CVL-FR-12), so what the
/// next call presents again is the whole of what this one presented, and the
/// provider advancing its own breakpoint as the conversation grows is exactly
/// that rule expressed on its side.
///
/// The lifetime is the short one — five minutes, which is what this client asks
/// for when no TTL is named, and which spans a whole loop by a wide margin under
/// the bound of CVL-FR-16. Named as a lifetime left unset rather than as one
/// chosen, because the client offers exactly two and the long one is the other.
///
/// Separated from [`carry_anthropic`] so what is asked for is readable without a
/// network call: the flags below are what a request carries, and a test asserts
/// them directly rather than inferring them from a response.
pub(super) fn anthropic_model(
    client: &rig::providers::anthropic::Client,
    endpoint: &AiApiCall,
) -> rig::providers::anthropic::completion::CompletionModel {
    use rig::client::CompletionClient;
    client
        .completion_model(endpoint.model_id.clone().unwrap_or_default())
        .with_automatic_caching()
}

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
        };
    }
    OpenAiAdapterConfig {
        base_url: ai_api::custom_gateway::adapter_base_url(&endpoint.base_url),
        route: match endpoint.model_mode {
            Some(crate::ai_shared::ModelMode::Chat) => OpenAiRoute::ChatCompletions,
            Some(crate::ai_shared::ModelMode::Responses) => OpenAiRoute::Responses,
            None => OpenAiRoute::Unrestricted,
        },
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
    use rig::client::CompletionClient;
    let config = openai_adapter_config(&endpoint);
    let client = rig::providers::openai::Client::builder()
        .api_key(endpoint.api_key.clone().unwrap_or_default())
        .base_url(&config.base_url)
        .build()
        .map_err(|_| CallFailure::unreachable(class::CONNECT))?;
    let model_id = endpoint.model_id.clone().unwrap_or_default();
    match config.route {
        // The client's own route is Responses, which is also the only route a
        // `responses` model permits, so one arm serves both.
        OpenAiRoute::Unrestricted | OpenAiRoute::Responses => {
            rig_seam::run_model(client.completion_model(model_id), request).await
        }
        OpenAiRoute::ChatCompletions => {
            rig_seam::run_model(client.completions_api().completion_model(model_id), request).await
        }
    }
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
