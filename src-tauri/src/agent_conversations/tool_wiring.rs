//! The bounds a tool loop runs under, and the tools a turn is offered.
//!
//! [`conversation_tools`] itself stands in the module root, beside the
//! contract surface, because a source-scanning test of
//! `crate::tools::propose_prompt_changes` reads it there.

use super::*;

// ---------------------------------------------------------------------------
// The tools (CVL-FR-08)
// ---------------------------------------------------------------------------

/// CVL-FR-13: how many model calls one turn may make.
///
/// The ceiling on a turn's token spend as well as on its length, since each call
/// carries an exchange larger than the last as fetched material accumulates in
/// it. Eight leaves room for a search, a few reads, and an answer — the shape of
/// an agent actually consulting the project — without letting one that keeps
/// searching become an open-ended expense.
pub(super) const MAX_MODEL_CALLS: usize = 8;

/// CVL-FR-33: the most searches a turn reports from a provider's own count
/// alone, where the provider named no call to report.
///
/// The provider's own step budget is thirty, and this is that number: a count
/// above it is a number this application has no reason to believe, and a report
/// per unit of it would be an event storm sourced from a field rather than from
/// anything that happened.
pub(super) const MAX_COUNTED_NATIVE_REPORTS: u32 = 30;

/// Erase a `PortableTool` into the form the loop can dispatch by name.
///
/// The typed `PortableTool` knows its own argument type; the loop has a name and
/// a `serde_json::Value` the model composed. This is the one place the two meet,
/// and it is deliberately generic so every tool crosses it identically rather
/// than each growing its own dispatch arm.
///
/// Arguments that do not decode are the tool's own `InvalidArgs` refusal, which
/// reaches the model as a result it can correct (CVL-FR-14) rather than as
/// anything that ends the turn.
pub(super) fn erase_tool<T>(tool: T) -> rig::tool::DynamicTool
where
    T: rig::tool::PortableTool + Send + Sync + 'static,
{
    use rig::tool::IntoToolOutput;

    let definition = rig::tool::tool_definition(&tool);
    let tool = Arc::new(tool);
    rig::tool::DynamicTool::new(
        definition.name,
        definition.description,
        definition.parameters,
        move |arguments: serde_json::Value| {
            let tool = Arc::clone(&tool);
            Box::pin(async move {
                let parsed = match serde_json::from_value::<T::Args>(arguments) {
                    Ok(parsed) => parsed,
                    Err(_) => return Err(undecodable_arguments()),
                };
                match tool.call(parsed).await {
                    Ok(output) => output.into_tool_output(),
                    Err(error) => Err(tool.map_error(error)),
                }
            })
        },
    )
}

/// TLC-FR-14 / TLC-FR-SPRR: the refusal of a call whose arguments do not
/// decode into the tool's argument type, made before the tool runs.
///
/// The message names what a model can act on and carries no Rust type name, no
/// crate name, and nothing it sent (TLC-FR-09).
pub(super) fn undecodable_arguments() -> rig::tool::ToolExecutionError {
    rig::tool::ToolExecutionError::new(
        rig::tool::ToolErrorKind::InvalidArgs,
        "The arguments for that tool were not in the shape it expects. Check the tool's parameters and call it again.",
    )
    // TLC-FR-14: what tells this refusal apart from the ones a tool authors
    // itself, which all arrive under the same kind. The dispatcher reads it to
    // log the shape the model sent, the tool having never run to log anything.
    .with_code(crate::tools::ARGUMENTS_UNDECODABLE)
    .with_retryable(true)
}

/// CVL-FR-30: the provider-native entries a turn is offered, decided by the
/// **provider** carrying the call and by nothing else.
///
/// Both or neither: `web_search` and `web_fetch` are two tools the model sees
/// separately (WST-FR-06, WFT-FR-06), so an agent holding an address reaches for
/// the page rather than searching for what it can already name — and no turn is
/// offered one of them alone. A provider this application has no native contract
/// for gets nothing, and nothing local stands in for them there (TLC-FR-23): the
/// capability is simply absent from that provider's conversations until its own
/// entries are defined and mapped here.
///
/// Named here for the reason [`conversation_tools`] names its own set: there is
/// no registry (TLC-FR-19), so a capability reaches a model only where a caller
/// wrote it down. Only a conversation calls this — a graduation phase assembles
/// its own request and fills the field with nothing (TLC-FR-24).
///
/// Nothing else varies the set. No origin kind, no agent definition, no project,
/// no selected model, and no setting of any kind reaches it (TLC-FR-22), so the
/// entries are byte-identical on every request that carries them.
pub(super) fn conversation_native_tools(provider: &str) -> Vec<crate::tools::ProviderNativeTool> {
    if provider == crate::tools::web_search::PROVIDER {
        // CVL-FR-30: the search alone. `web_fetch` is **withheld** — the entry
        // exists and is byte-identical to what it always was, and nothing here
        // is offered to the model (per `../tools/WFT-web-fetch-tool.md`
        // WFT-FR-16).
        //
        // OpenRouter refuses a request carrying it on every OpenAI model, on
        // every engine, and the narrowing of CVL-FR-36 recovers such a turn at
        // the price of one refused call per model per process. Withholding it
        // spends nothing at all, which is the right trade while the models an
        // author actually uses are the models that refuse it. Restoring it is
        // this list — nothing else in the application distinguishes the two
        // entries.
        return vec![crate::tools::web_search::entry()];
    }
    Vec::new()
}

/// A thin stand-in for `openrouter-rs`' derived builder, whose generated setters
/// take a different shape per field. Keeping the assembly here means one place
/// to look when the SDK's builder changes.
pub(super) struct ChatCompletionRequestBuilderShim {
    pub(super) model: String,
    pub(super) messages: Vec<openrouter_rs::api::chat::Message>,
    pub(super) reasoning: Option<openrouter_rs::types::ReasoningConfig>,
    pub(super) tools: Vec<openrouter_rs::types::Tool>,
    /// CVL-FR-30: the provider-native entries, which this SDK serializes into
    /// the same `tools` array as the function definitions above.
    pub(super) server_tools: Vec<openrouter_rs::types::ServerTool>,
    /// CVL-FR-23: the boundary this client advances for itself, so what a call
    /// ends with is held for the call after it.
    pub(super) advancing_boundary: Option<openrouter_rs::api::chat::CacheControl>,
    /// CVL-FR-40: the upstream to ask for, or `None` before anything is known
    /// of this model.
    pub(super) upstream: Option<String>,
}

impl ChatCompletionRequestBuilderShim {
    pub(super) fn new(model: String, messages: Vec<openrouter_rs::api::chat::Message>) -> Self {
        Self {
            model,
            messages,
            reasoning: None,
            tools: Vec::new(),
            server_tools: Vec::new(),
            advancing_boundary: None,
            upstream: None,
        }
    }

    pub(super) fn build(self) -> Result<openrouter_rs::api::chat::ChatCompletionRequest, String> {
        let mut builder = openrouter_rs::api::chat::ChatCompletionRequest::builder();
        builder.model(self.model).messages(self.messages);
        if let Some(reasoning) = self.reasoning {
            builder.reasoning(reasoning);
        }
        // The SDK's setter takes one tool at a time; the field itself is
        // `#[builder(setter(custom))]` and not reachable as a whole.
        for tool in self.tools {
            builder.tool(tool);
        }
        // CVL-FR-30: the same one-at-a-time shape, and the SDK writes both kinds
        // into one `tools` array on the wire — which is where the spec puts them.
        for tool in self.server_tools {
            builder.server_tool(tool);
        }
        // CVL-FR-23: the boundary the client advances for itself, at the request
        // level rather than on any one message — which is what lets it hold a
        // block this module never has to reshape.
        if let Some(boundary) = self.advancing_boundary {
            builder.cache_control(boundary);
        }
        // CVL-FR-40: ask for the upstream that served the last answered call,
        // and keep fallback allowed — an upstream that has gone away costs this
        // call its cache and never the call itself.
        if let Some(upstream) = self.upstream {
            let mut preferences = openrouter_rs::types::ProviderPreferences::default();
            preferences.order = Some(vec![upstream]);
            preferences.allow_fallbacks = Some(true);
            builder.provider(preferences);
        }
        builder.build().map_err(|e| e.to_string())
    }
}
