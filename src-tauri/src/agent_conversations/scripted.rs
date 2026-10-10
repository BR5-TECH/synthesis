//! The scripted seam every test in this module runs against (CVL-FR-11).
//!
//! A build under test substitutes a whole completion model rather than
//! intercepting a URL, so no request leaves the machine and no token is
//! spent. Nothing here is compiled into a shipped build.

use super::*;

// ---------------------------------------------------------------------------
// The scripted seam (CVL-FR-11)
// ---------------------------------------------------------------------------

/// The host a scripted `tls_untrusted` failure names.
pub const SCRIPTED_TLS_HOST: &str = "provider.test";

/// A [`CompletionSeam`] backed by `rig`'s own `MockCompletionModel`.
///
/// Compiled into test builds only. It drives the scripted model through
/// [`build_completion_request`] and `run_model` — the same assembly and the same
/// driver the production clients use — so what it exercises is the real path
/// with the network removed, rather than a parallel one that happens to agree.
/// One scripted reply: prose, tool calls, or both (CVL-FR-12).
///
/// `text` alone ends a turn; naming tools makes this reply a round in the middle
/// of one. A script of these is how every path through the loop — a refusal fed
/// back, the bound reached, a final reply with no prose — is driven without a
/// request leaving the machine.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub struct ScriptedReply {
    pub text: String,
    /// The tools to ask for, as `(name, arguments)`.
    pub tools: Vec<(String, serde_json::Value)>,
    /// CVL-FR-31: what the provider ran for itself while carrying this call —
    /// the traffic a real OpenRouter reply reports in its reasoning blocks. A
    /// script names these to drive a search, a fetch, several of either, and a
    /// provider that reports its own tool failing, without a request leaving the
    /// machine.
    pub native: Vec<ScriptedNativeCall>,
    /// CVL-FR-28: the input-token split this reply's provider reports, or `None`
    /// for one that reports nothing — which is what `rig`'s mock model yields on
    /// its own and so what an unadorned script gets.
    pub usage: Option<InputTokens>,
    /// CVL-FR-34: the provider's own count of the tool loop it ran while
    /// carrying this call, for the records that name it. `None` stands for a
    /// provider that counts nothing.
    pub native_usage: Option<NativeToolUsage>,
    /// CVL-FR-40: the upstream a routing provider reports this call was served
    /// by, for the records that name it. `None` stands for a provider that
    /// names none, which is every provider reached directly.
    pub served_by: Option<String>,
    /// CVL-FR-28: what this call presented, as a provider that counts it says.
    pub prompt_tokens: Option<u64>,
    /// CVL-FR-EKHH: where set, each tool call gets the id of this prefix and
    /// its position, as a Responses provider gives one as its `call_id`.
    /// `None` gives each call the id `call-{position}`.
    pub call_id_prefix: Option<String>,
}

/// CVL-FR-31: one tool a scripted provider ran for itself.
///
/// `id` is the provider's own identifier where a script names one, and `None`
/// where it does not — which is the case the loop derives an id for, exactly as
/// a real reply carrying a block with no `tool_call_id` is.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub struct ScriptedNativeCall {
    pub name: String,
    pub arguments: String,
    pub result: String,
    pub id: Option<String>,
}

#[cfg(test)]
impl ScriptedReply {
    /// A reply that answers and asks for nothing.
    pub fn answer(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tools: Vec::new(),
            native: Vec::new(),
            usage: None,
            native_usage: None,
            served_by: None,
            prompt_tokens: None,
            call_id_prefix: None,
        }
    }

    /// A reply that asks for one tool and says nothing.
    pub fn calls(name: &str, arguments: serde_json::Value) -> Self {
        Self {
            text: String::new(),
            tools: vec![(name.to_string(), arguments)],
            native: Vec::new(),
            usage: None,
            native_usage: None,
            served_by: None,
            prompt_tokens: None,
            call_id_prefix: None,
        }
    }

    /// CVL-FR-31: a reply reporting one tool the provider ran for itself, and
    /// the result it produced. Such a reply carries no prose, which is the
    /// shape of a provider that searched and then said nothing — an empty reply
    /// (CVL-FR-13), the provider having given the model the result inside this
    /// same call. The ordinary shape is prose *and* this report together, which
    /// [`ScriptedReply::answer`] and `and_native` compose.
    pub fn native(name: &str, arguments: &str, result: &str) -> Self {
        Self::default().and_native(name, arguments, result)
    }

    /// Report a further provider-run tool in the same reply (CVL-FR-31).
    pub fn and_native(self, name: &str, arguments: &str, result: &str) -> Self {
        self.and_native_call(ScriptedNativeCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
            result: result.to_string(),
            id: None,
        })
    }

    /// The same, for a provider that names the call's id itself — which is what
    /// OpenRouter does, and what a result is matched to its call by.
    pub fn and_native_with_id(self, id: &str, name: &str, arguments: &str, result: &str) -> Self {
        self.and_native_call(ScriptedNativeCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
            result: result.to_string(),
            id: Some(id.to_string()),
        })
    }

    fn and_native_call(mut self, call: ScriptedNativeCall) -> Self {
        self.native.push(call);
        self
    }

    /// CVL-FR-28: a reply whose provider reports its input-token split, for the
    /// records that name what caching came to.
    pub fn reporting(mut self, cached: u64, uncached: u64) -> Self {
        self.usage = Some(InputTokens { cached, uncached });
        self
    }

    /// CVL-FR-40 / CVL-FR-28: a reply whose provider reports which upstream
    /// carried it and what it presented, for the records that name them.
    pub fn routed_by(mut self, upstream: &str, prompt_tokens: u64) -> Self {
        self.served_by = Some(upstream.to_string());
        self.prompt_tokens = Some(prompt_tokens);
        self
    }

    /// CVL-FR-34: a reply whose provider counts its own tool loop, for the
    /// records that name those counts.
    pub fn counting(
        mut self,
        requested: Option<u32>,
        executed: Option<u32>,
        web_searches: Option<u32>,
    ) -> Self {
        self.native_usage = NativeToolUsage::reported(requested, executed, web_searches);
        self
    }

    /// A reply that says something *and* asks for a tool — the case CVL-FR-13
    /// turns on at the bound.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Ask for a further tool in the same reply (CVL-FR-12).
    pub fn and_calls(mut self, name: &str, arguments: serde_json::Value) -> Self {
        self.tools.push((name.to_string(), arguments));
        self
    }

    /// CVL-FR-EKHH: give each tool call of this reply the id the Responses
    /// route gives as its `call_id`. The call at position `n` gets the id
    /// `{prefix}{n}`.
    pub fn with_call_ids(mut self, prefix: &str) -> Self {
        self.call_id_prefix = Some(prefix.to_string());
        self
    }
}

#[cfg(test)]
pub struct ScriptedCompletion {
    pub(super) replies: Mutex<std::collections::VecDeque<Result<ScriptedReply, &'static str>>>,
    pub(super) seen: Mutex<Vec<(AgentRequest, AiApiCall)>>,
    /// Every exchange the seam was given, in call order — which is how a test
    /// sees that round two carried round one's tool result (CVL-FR-12).
    pub(super) exchanges: Mutex<Vec<Vec<rig::completion::Message>>>,
    /// How long `complete` pretends the model takes, so a cancellation or a
    /// concurrency bound has a window to be observed in.
    pub(super) delay: Duration,
    /// How many calls have ever been inside `complete` at once — the only place
    /// AGC-FR-25's bound can be measured without racing it.
    pub(super) concurrency: Mutex<(usize, usize)>,
}

#[cfg(test)]
impl ScriptedCompletion {
    /// The common case: a script of plain text answers, each ending its turn.
    pub fn new(replies: Vec<Result<String, &'static str>>) -> Self {
        Self::scripted(
            replies
                .into_iter()
                .map(|reply| reply.map(ScriptedReply::answer))
                .collect(),
        )
    }

    /// A script that can ask for tools (CVL-FR-12).
    pub fn scripted(replies: Vec<Result<ScriptedReply, &'static str>>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
            exchanges: Mutex::new(Vec::new()),
            delay: Duration::ZERO,
            concurrency: Mutex::new((0, 0)),
        }
    }

    /// Every exchange the seam was handed, in call order.
    pub fn exchanges(&self) -> Vec<Vec<rig::completion::Message>> {
        self.exchanges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How many times the seam was called — the loop's rounds (CVL-FR-13).
    pub fn call_count(&self) -> usize {
        self.exchanges.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// The greatest number of calls that were ever in flight at once.
    pub fn peak_concurrency(&self) -> usize {
        self.concurrency.lock().unwrap_or_else(|e| e.into_inner()).1
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn requests(&self) -> Vec<(AgentRequest, AiApiCall)> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// How many requests the seam has been given, without cloning any of them.
    ///
    /// For the poll loops, which ask only whether a call has started yet.
    /// [`requests`](Self::requests) deep-clones every `AgentRequest` it answers
    /// with — each carrying the compiled instructions, the input sections, and
    /// every tool's schema — and clones them while it holds the mutex
    /// `complete` takes on entry. A loop reading that every few milliseconds
    /// delays the call it is waiting for, which is the shape of defect
    /// `wait_for_record` was fixed for.
    pub fn requests_len(&self) -> usize {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// So a test can hold on to the seam it injected and read back what it was
/// given, without the registry having to expose the box it owns.
#[cfg(test)]
impl<T: CompletionSeam + ?Sized> CompletionSeam for Arc<T> {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        (**self).complete(request, exchange, endpoint, timeout)
    }
}

/// The literal pieces of a template, in order, with both placeholders of
/// CVL-FR-04 taken out — the parts a compiled prompt reproduces byte-for-byte
/// however the two substituted values happen to read.
///
/// Mirrors [`substitute_once`]'s scan, a placeholder occurring twice being
/// literal text on its second appearance in both.
#[cfg(test)]
fn template_segments(template: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut rest = template;
    let mut pending = vec![AGENT_TITLE_PLACEHOLDER, AGENT_INSTRUCTIONS_PLACEHOLDER];
    while !pending.is_empty() {
        let Some((at, index)) = pending
            .iter()
            .enumerate()
            .filter_map(|(i, needle)| rest.find(needle).map(|at| (at, i)))
            .min()
        else {
            break;
        };
        let needle = pending.remove(index);
        segments.push(&rest[..at]);
        rest = &rest[at + needle.len()..];
    }
    segments.push(rest);
    segments
}

/// Whether `compiled` is `template` with its placeholders filled — the check the
/// scripted seam makes without knowing which agent the turn was for, so it holds
/// whatever title and instructions were substituted.
///
/// The first segment must lead and the last must trail, so nothing can be
/// prepended or appended to a compiled prompt; the middles must occur in order,
/// so the template's own body cannot be reordered or dropped.
#[cfg(test)]
pub(super) fn carries_template(compiled: &str, template: &str) -> bool {
    let segments = template_segments(template);
    let (first, rest) = segments.split_first().expect("at least one segment");
    let (last, middles) = rest.split_last().expect("at least two segments");
    if !compiled.starts_with(first) || !compiled.ends_with(last) {
        return false;
    }
    // Between the prefix and the suffix, in order. Anything not a segment is a
    // substituted value, which this check deliberately says nothing about.
    let mut cursor = &compiled[first.len()..compiled.len() - last.len()];
    for segment in middles {
        match cursor.find(segment) {
            Some(at) => cursor = &cursor[at + segment.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
impl CompletionSeam for ScriptedCompletion {
    fn complete(
        &self,
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
        endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        // CVL-FR-01 / CVL-FR-02 / CVL-FR-04: asserted here rather than in one test, so
        // every dispatch this module exercises is a witness to it.
        // `AgentRequest` derives `Default` and its fields are public, so a
        // second construction site added later could reach a model with raw
        // instructions, an uncompiled template, or nothing at all; this is what
        // stops that landing green.
        //
        // The whole template must be present around whatever was substituted
        // into it — checking only that the instructions are non-empty would pass
        // against a request carrying the agent's instructions bare.
        //
        // CVL-FR-03: any of the compiled prompts satisfies this; which of them a
        // turn should have carried is the origin kind's business and is asserted
        // where the origin is known.
        let carries_a_compiled_prompt = [
            COMMENT_PROMPT_TEMPLATE,
            DISCUSS_ARTIFACT_PROMPT_TEMPLATE,
            DISCUSS_DRAFT_PROMPT_TEMPLATE,
            DISCUSS_NOTE_PROMPT_TEMPLATE,
        ]
        .iter()
        .any(|template| carries_template(&request.instructions, template));
        assert!(
            carries_a_compiled_prompt,
            "every request reaching a model carries the compiled prompt",
        );
        // CVL-FR-08: every request reaching a model offers the eight, in order, so
        // a turn that lost them on some path cannot land green.
        //
        // The seventh is the origin kind's business and is asserted where the
        // origin is known, exactly as the choice of prompt above is: this seam
        // sees a request and not the conversation it was assembled for, and a
        // request carries nothing that distinguishes a draft turn from an
        // artifact one. What *is* checkable here is that the set is either the
        // nine or the nine followed by one proposal tool and never anything
        // else — which is what stops a tool being added to a turn without
        // anybody deciding it should be.
        let offered: Vec<&str> = request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        // CVL-FR-08: the eleven every origin kind gets, in the order
        // `conversation_tools` names them.
        let common = vec![
            crate::tools::spec_search::NAME,
            crate::tools::file_read::NAME,
            crate::tools::draft_search::NAME,
            crate::tools::draft_read::NAME,
            crate::tools::note_search::NAME,
            crate::tools::document_search::NAME,
            crate::tools::document_get::NAME,
            crate::tools::skill_search::NAME,
            crate::tools::skill_list::NAME,
            crate::tools::skill_load::NAME,
            crate::tools::ask_user_comment::NAME,
        ];
        // CVL-FR-08: a discussion origin gets `ask_discussion_questions` beside
        // the nine, and an anchored comment origin gets none of it. Which of the
        // two a turn is remains the origin kind's business and is asserted where
        // the origin is known; what is checkable here is that it never appears
        // anywhere but in this position.
        let mut with_questions = common.clone();
        with_questions.push(crate::tools::ask_discussion_questions::NAME);
        // CVL-FR-08: a draft origin gets one proposal tool and an artifact origin
        // the other, and the two are never offered together.
        let permitted: Vec<Vec<&str>> = [common.clone(), with_questions.clone()]
            .into_iter()
            .flat_map(|base| {
                let mut with_draft = base.clone();
                with_draft.push(crate::tools::propose_draft_changes::NAME);
                let mut with_prompt = base.clone();
                with_prompt.push(crate::tools::propose_prompt_changes::NAME);
                [base, with_draft, with_prompt]
            })
            .collect();
        assert!(
            permitted.iter().any(|shape| *shape == offered),
            "every request offers the eleven, then `ask_discussion_questions` on a discussion, then at most one proposal tool, and nothing else: {offered:?}",
        );
        // CVL-FR-30: asserted on every dispatch the suite makes rather than in
        // one test, so a request that lost the entries on some path, gained them
        // on a provider that does not serve them, or was offered one of the two
        // alone cannot land green.
        let native: Vec<&str> = request
            .native_tools
            .iter()
            .map(|tool| tool.tool_type.as_str())
            .collect();
        // CVL-FR-30: the search alone on OpenRouter, and nothing anywhere else.
        // `web_fetch` is withheld (WFT-FR-16), so a request that carried it
        // would be one nobody decided to send.
        let expected: Vec<&str> = if endpoint.provider == crate::tools::web_search::PROVIDER {
            vec![crate::tools::web_search::ENTRY_TYPE]
        } else {
            Vec::new()
        };
        assert_eq!(
            native, expected,
            "a conversational request carries both provider-native entries on OpenRouter and neither anywhere else",
        );
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((request.clone(), endpoint.clone()));
        self.exchanges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(exchange.to_vec());
        {
            let mut guard = self.concurrency.lock().unwrap_or_else(|e| e.into_inner());
            guard.0 += 1;
            guard.1 = guard.1.max(guard.0);
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        {
            let mut guard = self.concurrency.lock().unwrap_or_else(|e| e.into_inner());
            guard.0 -= 1;
        }
        let scripted = self
            .replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or(Err(FAIL_UNREACHABLE));
        match scripted {
            // The success path goes through rig's mock model and rig's own
            // request assembly, so the seam a test exercises is the seam
            // production uses.
            Ok(reply) => {
                let built = build_completion_request(request, exchange, endpoint);
                // CVL-FR-28: `rig`'s mock reports no usage, so a script that
                // names one stands in for a provider that does. Applied to what
                // `run_model` returned rather than replacing it, so the rest of
                // the reply still travels the production extraction.
                let scripted_usage = reply.usage;
                let scripted_native_usage = reply.native_usage;
                // CVL-FR-40 / CVL-FR-28: what a routing provider reports of the
                // call it carried, which `rig`'s mock has no shape for.
                let scripted_served_by = reply.served_by.clone();
                let scripted_prompt_tokens = reply.prompt_tokens;
                // CVL-FR-31: the provider's own traffic, in the shape
                // `carry_openrouter` extracts it into. Applied to what
                // `run_model` returned rather than replacing it, so the rest of
                // the reply still travels the production extraction.
                // The id a provider names, or one derived from the round and
                // the call's position — which is what the production extraction
                // does, and which keeps ids unique across a growing exchange.
                let round = exchange.len();
                let scripted_native: Vec<NativeToolCall> = reply
                    .native
                    .iter()
                    .enumerate()
                    .map(|(index, call)| NativeToolCall {
                        id: call
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("provider-tool-{round}-{index}")),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                        result: call.result.clone(),
                    })
                    .collect();
                let model = if reply.tools.is_empty() {
                    rig::test_utils::MockCompletionModel::text(reply.text)
                } else {
                    // The mock returns the assistant content verbatim, so the
                    // tool calls travel through `run_model`'s own extraction
                    // rather than around it.
                    let mut parts = Vec::new();
                    if !reply.text.is_empty() {
                        parts.push(rig::completion::message::AssistantContent::text(&reply.text));
                    }
                    for (index, (name, arguments)) in reply.tools.iter().enumerate() {
                        // CVL-FR-EKHH: a call has one id. A script that names
                        // a prefix gives the call at position `n` the id
                        // `{prefix}{n}`, as the Responses route gives its
                        // `call_id`.
                        let id = match &reply.call_id_prefix {
                            Some(prefix) => format!("{prefix}{index}"),
                            None => format!("call-{index}"),
                        };
                        let call = rig::completion::message::ToolCall::from_wire(
                            id,
                            rig::completion::message::ToolFunction::new(
                                rig::completion::message::ToolName::new(name.clone())
                                    .expect("a scripted tool has a name"),
                                arguments.clone(),
                            ),
                        );
                        parts.push(rig::completion::message::AssistantContent::ToolCall(call));
                    }
                    rig::test_utils::MockCompletionModel::from_turns([
                        rig::test_utils::MockTurn::from_contents(parts),
                    ])
                };
                block_on_with_timeout(
                    async move { rig_seam::run_model(model, built).await },
                    Duration::from_secs(5),
                )
                .map(|mut reply| {
                    if let Some(usage) = scripted_usage {
                        reply.input_tokens = Some(usage);
                    }
                    reply.native_calls = scripted_native;
                    reply.native_usage = scripted_native_usage;
                    if scripted_served_by.is_some() {
                        reply.served_by = scripted_served_by;
                    }
                    if scripted_prompt_tokens.is_some() {
                        reply.prompt_tokens = scripted_prompt_tokens;
                    }
                    reply
                })
            }
            // A script names the failure it wants; the class follows from the
            // value, which is what keeps every existing script unchanged while
            // the seam carries CVL-FR-21's finer classification in production.
            // AGC-FR-RWPT: a scripted TLS refusal names a fixed host and cause.
            Err(FAIL_TLS_UNTRUSTED) => Err(CallFailure::tls_untrusted(
                crate::tls::TlsFailure::new(SCRIPTED_TLS_HOST, crate::tls::TlsCause::UnknownIssuer),
            )),
            Err(failure) => Err(CallFailure::from(failure)),
        }
    }
}
