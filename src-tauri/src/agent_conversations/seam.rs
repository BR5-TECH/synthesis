//! The request and the reply, and the seam a model is called through.
//!
//! [`CompletionSeam`] is the one place a model is reached (CVL-FR-11). The
//! types beside it are what crosses it, and the classification below turns a
//! provider's own error text into one of the typed failures.

use super::*;

/// What the module composes and the turn's first call carries. It exists only
/// for the duration of the turn: nothing here, and nothing of the exchange it
/// grows into, is persisted, echoed into the reply, or included in any event
/// payload (CVL-FR-26).
///
/// CVL-FR-01: three things and no others. Nothing else of the application's own
/// is added, and nothing of the request is assembled by concatenating a phrasing
/// this module chooses per call — a reply is only as dependable as the
/// instruction that asked for it is fixed.
///
/// `Eq` is absent because `rig`'s `ToolDefinition` carries a `serde_json::Value`
/// and derives `PartialEq` alone; comparing two requests is still what the tests
/// do, and nothing here needs the total relation.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    /// The compiled prompt, carrying the agent's instructions within it.
    pub instructions: String,
    /// Ordered: artifact, discussion history, current comment.
    pub input: Vec<InputSection>,
    /// CVL-FR-01 / CVL-FR-08: the tools of [`conversation_tools`], each
    /// definition that tool's own fixed text rather than anything composed here
    /// (TLC-FR-05). Identical for every turn, so two agents are offered
    /// byte-identical capabilities.
    #[serde(default)]
    pub tools: Vec<rig::completion::ToolDefinition>,
    /// CVL-FR-30: the provider-native entries the **provider** decides, standing
    /// in the same tool array beside the definitions above. Empty for every
    /// provider but OpenRouter, and empty for every loop but a conversation
    /// (TLC-FR-23, TLC-FR-24) — a graduation phase assembles a request of this
    /// shape and never fills this field.
    #[serde(default)]
    pub native_tools: Vec<crate::tools::ProviderNativeTool>,
    /// CVL-FR-39: how many **leading** sections of `input` are its stable head —
    /// the material whose bytes are the same on the next turn as on this one.
    ///
    /// A count of whole sections, because the head is a run of them and never a
    /// cut through one, and the tag order of CVL-FR-06 is what settles which
    /// they are. Zero says the material has no stable head, which is how an
    /// input with nothing worth marking apart reaches the wire exactly as an
    /// unsplit one does. It says nothing about what the request *presents* —
    /// the assembled request of CVL-FR-01 is the same either way.
    #[serde(default)]
    pub stable_head_sections: usize,
}

/// CVL-FR-12: one reply from the model — what it said, and what it asked for.
///
/// Kept apart from the turn's answer because a reply carrying both prose and
/// tool calls is ordinary: a model routinely narrates what it is about to look
/// up. The loop decides which of the two ends the turn; the seam only reports
/// what came back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelReply {
    /// The prose the reply carried, which may be empty when the model only
    /// asked for tools.
    pub text: String,
    /// The tools it asked for, in the order it asked for them (CVL-FR-12).
    pub tool_calls: Vec<rig::completion::message::ToolCall>,
    /// CVL-FR-ETAH: the reply as the framework returned it — its text, its
    /// tool calls, its thinking, and the provider's own identifiers for each —
    /// which is what goes back into the exchange when the reply asks for tools.
    /// `None` where the client returned no such turn, and for a reply the
    /// framework reports as failed (CVL-FR-UALC).
    pub turn: Option<rig::completion::message::AssistantMessage>,
    /// CVL-FR-31: the tools the **provider** executed for itself while carrying
    /// this call, each with the result it produced, in the order they arrived.
    /// The loop dispatches none of them and appends none of them to the
    /// exchange: OpenRouter ran its own tool loop *inside* this call and the
    /// model had every result in front of it before it composed the reply this
    /// field belongs to. What comes back here is the report of that, which the
    /// loop reports as activity and records, and nothing more.
    pub native_calls: Vec<NativeToolCall>,
    /// CVL-FR-34: what the provider says its own tool loop did while carrying
    /// this call, as counts. `None` where the provider reported nothing.
    pub native_usage: Option<NativeToolUsage>,
    /// CVL-FR-36: how many provider-native entries this call had to give up
    /// before the provider would carry it. Zero on every call that was carried
    /// as offered, which is every call on a model that serves what it is
    /// offered.
    pub native_entries_dropped: usize,
    /// CVL-FR-28: what this call cost in input tokens, split by whether the
    /// provider served them from its cache — or `None` where the provider
    /// reported no such split.
    pub input_tokens: Option<InputTokens>,
    /// CVL-FR-40: the upstream the provider routed this call to, as the client
    /// reported it. `None` where the client reported none, which leaves the
    /// next call unpinned rather than guessing at one.
    ///
    /// The name of a service the provider chose, never anything the model
    /// composed, so it is safe on a record (CVL-FR-26).
    pub served_by: Option<String>,
    /// CVL-FR-28: what this one call presented, in input tokens as the provider
    /// counted them, or `None` where it counted nothing.
    ///
    /// Kept apart from [`ModelReply::input_tokens`], which is the cached and
    /// uncached split a turn accumulates: this is one call's whole prompt, and
    /// it is what makes a growing exchange readable round by round.
    pub prompt_tokens: Option<u64>,
    /// CVL-FR-WQZD: what this one call produced, in output tokens as the
    /// provider counted them, or `None` where it counted nothing.
    ///
    /// Reported rather than derived: no value here is ever computed from the
    /// reply's text, from a character count, from a model's limit, or from a
    /// price, and a provider that declines to report one leaves this absent so
    /// the direction reads unavailable rather than as a confident zero.
    pub output_tokens: Option<u64>,
    /// CVL-FR-28: the part of the input the provider wrote to its cache, as it
    /// counted it, or `None` where it counted nothing.
    pub cache_write_tokens: Option<u64>,
    /// CVL-FR-28: the part of the output the model spent on reasoning, as the
    /// provider counted it, or `None` where it counted nothing.
    pub reasoning_tokens: Option<u64>,
    /// CVL-FR-28: the provider's own identifier for this response.
    pub response_id: Option<String>,
    /// CVL-FR-28: the provider's own identifier for the request, which is what
    /// takes a call to the provider's own record of it.
    pub provider_request_id: Option<String>,
    /// CVL-FR-28: the model the provider says answered.
    pub response_model: Option<String>,
    /// CVL-FR-28: why the reply ended, in the framework's vocabulary or the
    /// provider's own word.
    pub finish_reason: Option<String>,
    /// CVL-FR-TQRD: what the Custom gateway repair changed in this reply before
    /// the framework read it. The loop records it, because the carrier has no
    /// access to the log.
    pub reply_repairs: super::responses_repair::ReplyRepairs,
}

/// CVL-FR-31: one tool the provider ran on its own account, and what it
/// produced.
///
/// Nothing here is composed by this application and nothing here is rewritten by
/// it: the name, the arguments, and the result are the provider's, reported as
/// they arrived (WST-FR-08, WFT-FR-08). It is kept apart from
/// [`ModelReply::tool_calls`] precisely so the loop cannot dispatch one — a call
/// the provider has already carried out, and whose result the model has already
/// read, is a record of what happened rather than work to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeToolCall {
    /// The provider's own identifier for the call, or one derived from its
    /// position where the provider named none — a result is matched to the call
    /// it answers by this and nothing else.
    pub id: String,
    /// The provider-qualified tool, e.g. `openrouter:web_search`.
    pub name: String,
    /// The arguments the model composed, as the provider reported them.
    pub arguments: String,
    /// What the provider produced, verbatim. A provider that reports a failure
    /// for its own tool reports it here, and it reaches the model as this call's
    /// result rather than as a failure of the turn (CVL-FR-32).
    pub result: String,
}

/// CVL-FR-34: the provider's own account of the tool loop it ran inside one
/// model call.
///
/// Counts and nothing else, so a record naming them carries no query, no
/// address, and no part of a page (CVL-FR-26). They are what tells a reader
/// whether a call the model asked for actually ran: a request the provider
/// counted and did not execute is the shape a refused or exhausted server tool
/// takes, and it is invisible in the call list alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeToolUsage {
    /// Server-tool calls the model asked the provider for.
    pub requested: Option<u32>,
    /// Server-tool calls the provider executed and produced a result for.
    pub executed: Option<u32>,
    /// Web searches among them, which the provider counts separately.
    pub web_searches: Option<u32>,
}

impl NativeToolUsage {
    /// `None` where the provider reported no count at all, so a record says
    /// "not reported" rather than a confident zero.
    pub(crate) fn reported(
        requested: Option<u32>,
        executed: Option<u32>,
        web_searches: Option<u32>,
    ) -> Option<Self> {
        (requested.is_some() || executed.is_some() || web_searches.is_some()).then_some(Self {
            requested,
            executed,
            web_searches,
        })
    }
}

/// CVL-FR-28: one call's input tokens, split by whether the provider had them
/// cached already (CVL-FR-23).
///
/// The split is what says whether caching is landing: a loop whose second and
/// later calls report almost all of their input as cached is one whose repeated
/// prefix is being served from the cache, and one that reports none is paying
/// full price every round. Counts only — nothing here is drawn from the
/// exchange, so a record naming them carries no material (CVL-FR-26).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputTokens {
    /// Input tokens the provider served from its cache.
    pub cached: u64,
    /// Input tokens the provider read afresh and billed at full rate.
    pub uncached: u64,
}

// ---------------------------------------------------------------------------
// The completion seam (CVL-FR-11)
// ---------------------------------------------------------------------------

/// The single seam through which a model is reached, and the only place in this
/// module that touches the network.
///
/// Object-safe by design, and synchronous: `rig`'s own model is async and is
/// driven on a thread each call owns, under the provider-call deadline — this
/// is the façade over it. The production implementation
/// ([`RigCompletion`]) assembles a `rig::completion::CompletionRequest` and
/// drives whichever provider client carries it; a build under test substitutes
/// [`ScriptedCompletion`], which drives `rig`'s own `MockCompletionModel`
/// through the *same* assembly. Every failure of AGC-FR-15 is therefore
/// reachable without a request leaving the machine.
/// Every round of the loop goes through it (CVL-FR-12), carrying the exchange as
/// it stands at that moment. The tools reach no network of their own, so a
/// turn's whole outbound footprint is the calls made here.
pub trait CompletionSeam: Send + Sync {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure>;
}

/// Escape a value for an attribute the rendered input quotes with `"`.
///
/// A path is not a controlled string — a filename may legitimately carry a quote
/// or an ampersand — and one that closed its own attribute early would put a
/// participant's text where a tag's structure belongs. `&` first, so the escape
/// of a quote is not itself re-escaped.
pub(super) fn escape_attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}

/// AGC-FR-06: the input, as the tagged text the model reads.
///
/// The sections in the order the builder produced them, each inside the tag it
/// names, so an instruction about how to treat a passage under discussion can
/// name the section holding it (CVL-FR-06).
pub fn render_input(request: &AgentRequest) -> String {
    render_sections(&request.input)
}

/// The same, over the sections themselves.
///
/// Named apart from [`render_input`] so a **leading run** of the input can be
/// rendered through this one renderer rather than through a second that could
/// drift from it (CVL-FR-39), and so doing that costs no copy of the material.
pub fn render_sections(sections: &[InputSection]) -> String {
    let mut out = String::new();
    for section in sections {
        out.push('<');
        out.push_str(&section.tag);
        for (name, value) in &section.attributes {
            out.push_str(&format!(" {name}=\"{}\"", escape_attribute(value)));
        }
        out.push_str(">\n");
        out.push_str(&section.body);
        if !section.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("</{}>\n\n", section.tag));
    }
    out.truncate(out.trim_end().len());
    out
}

/// Map a `rig` completion error onto this module's failure vocabulary
/// (AGC-FR-15). The distinctions matter because they call for different
/// corrections: a host to bring up, a credential to replace, a wait to sit out.
/// CVL-FR-21: every failed attempt is reduced to a normalized class **before**
/// it is logged or reported, and the provider's own message goes no further than
/// this function. What comes back is a class, a failure value, and at most a
/// status number — never the payload that produced them, because an HTTP
/// client's error routinely echoes the request it made, headers included.
pub(super) fn classify_provider_error(message: &str) -> CallFailure {
    let lower = message.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        return CallFailure::new(FAIL_TIMED_OUT, class::PROVIDER_TIMEOUT);
    }
    // The credentials the provider refused. Read before the status scan below so
    // a 401 is `rejected` rather than an anonymous `http_status`: the two call
    // for entirely different corrections, and only one of them is worth
    // retrying — neither, as it happens, but for opposite reasons.
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid api key")
        || lower.contains("authentication")
    {
        return CallFailure::new(FAIL_REJECTED, class::AUTH);
    }
    // A name that would not resolve, and a connection that would not open: the
    // two ends of "the request never reached a server", told apart because one
    // is a DNS or a typo in a base URL and the other is a host that is down or a
    // port nothing is listening on.
    if lower.contains("dns")
        || lower.contains("name resolution")
        || lower.contains("failed to lookup")
        || lower.contains("nodename nor servname")
        || lower.contains("name or service not known")
    {
        return CallFailure::unreachable(class::DNS);
    }
    if lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("connection closed")
        || lower.contains("incomplete message")
    {
        return CallFailure::unreachable(class::RESET);
    }
    if lower.contains("connection refused")
        || lower.contains("connect")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
    {
        return CallFailure::unreachable(class::CONNECT);
    }
    // A response the provider actually returned. The status alone is carried
    // through: it is a number the provider chose, not anything it echoed of what
    // was sent to it.
    if let Some(status) = http_status_in(&lower) {
        return CallFailure {
            failure: FAIL_UNREACHABLE,
            class: class::HTTP_STATUS,
            status: Some(status),
            ..CallFailure::unreachable(class::HTTP_STATUS)
        };
    }
    // CVL-FR-21: matching none of them is normalized all the same, and the
    // redacted category is the whole of what survives.
    CallFailure::unreachable(class::TRANSPORT_OTHER)
}

/// AAP-FR-HZTB / CVL-FR-21: a call whose client's verifier refused a
/// certificate failed for that reason.
///
/// `rig` and `openrouter-rs` flatten a TLS error into text, so the cause is read
/// from the record of the client the call used. The record is the evidence, and
/// it is read ahead of the text classification, which can mistake a host name or
/// a port for a status. A provider that answered with a status had a good
/// handshake, so a failure that carries a status is left as it is.
pub(super) fn with_recorded_tls(failure: CallFailure, record: &crate::tls::TlsRecord) -> CallFailure {
    if failure.status.is_some() {
        return failure;
    }
    match record.take() {
        Some(tls) => CallFailure::tls_untrusted(tls),
        None => failure,
    }
}

/// CVL-FR-35: how much of a provider's own message a record carries.
///
/// Long enough for the sentence a provider leads with, which is the part that
/// names the field or the item it refused; short enough that a provider
/// answering with a page of text cannot fill the buffer of `logging`.
pub const PROVIDER_MESSAGE_LIMIT: usize = 400;

/// The first `limit` characters of `text`, marked where anything was dropped.
///
/// Characters rather than bytes, so a message in any script is cut where it
/// reads rather than in the middle of one.
pub(super) fn truncated(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().nth(limit).is_some() {
        out.push('…');
    }
    out
}

/// The HTTP status a provider's message names, where it names one.
///
/// A scan for a bare three-digit 4xx/5xx rather than a parse of the message's
/// shape, because every client spells this differently and the number is the
/// only part worth keeping.
fn http_status_in(lower: &str) -> Option<u16> {
    let bytes = lower.as_bytes();
    for (i, window) in bytes.windows(3).enumerate() {
        if !window.iter().all(|b| b.is_ascii_digit()) {
            continue;
        }
        // A digit on either side means this is part of a longer number.
        if i > 0 && bytes[i - 1].is_ascii_digit() {
            continue;
        }
        if bytes.get(i + 3).is_some_and(|b| b.is_ascii_digit()) {
            continue;
        }
        let status: u16 = std::str::from_utf8(window).ok()?.parse().ok()?;
        if (400..=599).contains(&status) {
            return Some(status);
        }
    }
    None
}
