//! One model call, repeated under the schedule of CVL-FR-17 .. CVL-FR-21.
//!
//! A recoverable failure is retried and nothing about the retry reaches a
//! surface (CVL-FR-20); an unrecoverable one ends the turn at the first
//! attempt.

use super::*;

/// CVL-FR-17 / CVL-FR-19: one **logical** `complete` invocation, made as up to
/// three physical attempts under its own deadline and its own retry budget.
///
/// The exchange is passed by reference and never rebuilt, which is the whole of
/// CVL-FR-20: a retry is the same turn making the same request, so no tool is
/// dispatched a second time, no side effect is duplicated, no comment is
/// appended, no session is re-begun, and the input is not reassembled. Nothing
/// here reaches a surface either — the turn stays `running` throughout and emits
/// no event — so a turn that answered on its third attempt and one that answered
/// on its first are indistinguishable to whoever dispatched them.
#[allow(clippy::too_many_arguments)]
pub(super) fn complete_with_retries<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    exchange: &[rig::completion::Message],
    policy: RetryPolicy,
    deadline: std::time::Instant,
    model_calls: usize,
    at_bound: bool,
) -> Result<ModelReply, TurnEnd> {
    let model_label = endpoint.model_id.as_deref().unwrap_or("<provider default>");
    for attempt in 1..=policy.max_attempts {
        check_cancelled(plan)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            log_turn_out_of_time(app, turns, plan, model_calls);
            return Err(TurnEnd::TimedOut);
        }
        // CVL-FR-17: five minutes, or the time remaining under the whole-turn
        // deadline, whichever is less — so no per-call deadline ever outlives
        // the turn it belongs to and no expiry is ambiguous about which bound it
        // was.
        let call_timeout = remaining.min(policy.call_timeout);
        let physical = plan.physical_calls.fetch_add(1, Ordering::SeqCst) + 1;
        // AGC-FR-YQMD: the turn has reached the model from here on, whatever
        // this attempt returns and whatever terminates the turn afterwards.
        turns.mark_reached_model(&plan.turn_id);

        // The external calls this module makes, so each is reported on both
        // sides: a model that is still thinking looks exactly like one that will
        // never answer until the second record lands.
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Remote],
            "calling model",
            log_fields! {
                "turnId" => &plan.turn_id,
                "agent" => &plan.agent.nickname,
                "agentId" => &plan.agent.id,
                "provider" => &endpoint.provider,
                "model" => model_label,
                "modelCall" => model_calls,
                "attempt" => attempt,
                "maxAttempts" => policy.max_attempts,
                "physicalCalls" => physical,
                "promptChars" => plan.request.instructions.chars().count(),
                "inputChars" => plan.request.input.iter().map(|s| s.body.chars().count()).sum::<usize>(),
                "exchangeMessages" => exchange.len(),
                "tools" => plan.request.tools.len(),
                // CVL-FR-30: how many provider-native entries this request
                // carries, which is what says whether the provider was offered
                // the web on this call. A count and never anything a model
                // composed (CVL-FR-26).
                "nativeTools" => plan.request.native_tools.len(),
                // CVL-FR-34: which of them, by entry type. Fixed text compiled
                // into the binary (TLC-FR-22), so naming it carries nothing of
                // the project, of the author, or of the conversation — and a
                // reader can tell a turn offered the web from one offered only
                // the search half of it.
                "nativeToolTypes" => plan.request.native_tools
                    .iter()
                    .map(|tool| tool.tool_type.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                "timeoutMs" => call_timeout.as_millis() as u64,
            },
        );
        let started = std::time::Instant::now();
        let outcome = turns
            .seam
            .complete(&plan.request, exchange, endpoint, call_timeout)
            .map(|mut reply| {
                // CVL-FR-VSSU: the markers go before anything reads the reply,
                // so a reply that held markers alone is empty (CVL-FR-POAR).
                let removed = strip_reply_citation_markers(&mut reply);
                if removed > 0 {
                    // CVL-FR-POAR: how many characters, and nothing of the text.
                    logging::log_debug(
                        app,
                        turns.buffer(),
                        &[Domain::Ai, Domain::Backend],
                        "removed citation markers from model reply",
                        log_fields! {
                            "turnId" => &plan.turn_id,
                            "agent" => &plan.agent.nickname,
                            "agentId" => &plan.agent.id,
                            "provider" => &endpoint.provider,
                            "model" => model_label,
                            "modelCall" => model_calls,
                            "attempt" => attempt,
                            "markerCharsRemoved" => removed,
                        },
                    );
                }
                reply
            });
        let failure = match outcome {
            Ok(reply) if reply_is_usable(&reply, at_bound, &plan.request.tools) => {
                // CVL-FR-28: accumulated on the usable reply alone. A failed
                // attempt was never answered and so was never billed, and one
                // whose reply the loop rejected is retried as the same logical
                // call — counting either would report a turn as having paid for
                // input it was not charged for.
                if let Some(tokens) = reply.input_tokens {
                    plan.cached_input_tokens
                        .fetch_add(tokens.cached, Ordering::SeqCst);
                    plan.uncached_input_tokens
                        .fetch_add(tokens.uncached, Ordering::SeqCst);
                    plan.tokens_reported.store(true, Ordering::SeqCst);
                }
                // CVL-FR-WQZD / AGC-FR-BVNT: the reliable usage record this
                // answered round reported leaves the loop here — once per
                // distinct record, on the usable reply alone. A failed attempt
                // was never answered and reports none; a successful retry
                // reports its own record, which is a distinct reliable record
                // of the same logical invocation rather than a second turn.
                record_refinement_usage(app, plan, model_calls, attempt as u32, &reply);
                logging::log_info(
                    app,
                    turns.buffer(),
                    &[Domain::Ai, Domain::Remote],
                    "model answered",
                    log_fields! {
                        "turnId" => &plan.turn_id,
                        "agent" => &plan.agent.nickname,
                        "provider" => &endpoint.provider,
                        "model" => model_label,
                        "modelCall" => model_calls,
                        "attempt" => attempt,
                        "durationMs" => started.elapsed().as_millis() as u64,
                        // The size of the reply and the number of tools it asked
                        // for, never the reply itself and never an argument: the
                        // prose is about to become a comment anyone can read, and
                        // an argument was composed by a model out of the
                        // conversation it is having (CVL-FR-26).
                        "replyChars" => reply.text.chars().count(),
                        "toolCalls" => reply.tool_calls.len(),
                        // CVL-FR-28 / CVL-FR-40: which upstream the provider
                        // routed this call to, and what the call presented. The
                        // two together are what says whether the caching of
                        // CVL-FR-23 is landing on a provider that reports no
                        // cached and uncached split of its own. An upstream the
                        // provider did not name is named as not reported rather
                        // than guessed at. The name of a service, never anything
                        // the model composed (CVL-FR-26).
                        "servedUpstream" => reply.served_by.as_deref().unwrap_or("not reported"),
                        "promptTokens" => reply
                            .prompt_tokens
                            .map(|tokens| tokens.to_string())
                            .unwrap_or_else(|| "not reported".to_string()),
                        // CVL-FR-31 / TLC-FR-26: how much the provider ran for
                        // itself while carrying this call. Counts alone — no
                        // query, no address, and no part of what came back
                        // (WST-FR-14, WFT-FR-14). Each call is recorded on its
                        // own beside this (CVL-FR-34).
                        "nativeToolCalls" => reply.native_calls.len(),
                        // CVL-FR-36: how many entries this call had to give up
                        // before the provider would carry it. Non-zero says the
                        // agent reached the web with less than it was offered.
                        "nativeEntriesDropped" => reply.native_entries_dropped,
                        "nativeToolCallsRequested" => reply.native_usage.and_then(|usage| usage.requested),
                        "nativeToolCallsExecuted" => reply.native_usage.and_then(|usage| usage.executed),
                        "nativeWebSearches" => reply.native_usage.and_then(|usage| usage.web_searches),
                    },
                );
                return Ok(reply);
            }
            // CVL-FR-13: neither usable prose nor a tool request. A completion
            // outcome rather than a transport one, and recoverable on exactly
            // the same terms.
            Ok(rejected) => {
                // CVL-FR-34: the provider still ran whatever it ran, and this
                // is the reply that says so. It is *recorded* and not
                // *reported*: the turn is discarding it, so no surface should
                // ever show a call from it as activity — but a turn that spent
                // three attempts on a provider that searched and then said
                // nothing would otherwise leave no evidence that a search
                // happened at all, which is the one case these records exist
                // for.
                log_native_calls(app, turns, plan, endpoint, model_calls, &rejected);
                if rejected.tool_calls.is_empty() {
                    if let Some(tool) = tool_written_as_prose(&rejected.text, &plan.request.tools) {
                        // CVL-FR-UJXD: the tool it meant, and never the text.
                        logging::log_warn(
                            app,
                            turns.buffer(),
                            &[Domain::Ai, Domain::Backend],
                            "agent wrote a tool call as prose",
                            log_fields! {
                                "turnId" => &plan.turn_id,
                                "agent" => &plan.agent.nickname,
                                "agentId" => &plan.agent.id,
                                "provider" => &endpoint.provider,
                                "model" => model_label,
                                "modelCall" => model_calls,
                                "attempt" => attempt,
                                "tool" => tool,
                            },
                        );
                    }
                }
                CallFailure::new(FAIL_EMPTY_REPLY, class::EMPTY_REPLY)
            }
            Err(failure) => failure,
        };

        // CVL-FR-19: no retry begins after the whole-turn deadline, after
        // cancellation, or after the conversation becomes locked. Each is read
        // *before* the delay is planned, so a record never promises a retry that
        // will not be made.
        let budget_left = attempt < policy.max_attempts;
        let time_left = !deadline
            .saturating_duration_since(std::time::Instant::now())
            .is_zero();
        let cancelled = plan.cancelled.load(Ordering::SeqCst);
        // AGC-FR-19: a lock is what stops a conversation taking further
        // contributions, so a turn whose thread was locked while it was failing
        // has nowhere to deliver whatever a retry produced.
        let locked = failure.repeatable()
            && budget_left
            && time_left
            && !cancelled
            && thread_is_locked(plan.roots.borrowed(), &plan.origin);
        let will_retry =
            failure.repeatable() && budget_left && time_left && !cancelled && !locked;
        let planned = if will_retry {
            backoff_delay(&policy, attempt + 1, turns.jitter.as_ref())
        } else {
            Duration::ZERO
        };

        // CVL-FR-28: one record per failed physical attempt, carrying the
        // correlation fields every record of this turn carries plus what is
        // particular to the attempt. `WARN` while a retry is still to come — the
        // application is recovering — and `ERROR` once it is not.
        logging::log(
            app,
            turns.buffer(),
            if will_retry {
                logging::LogLevel::Warn
            } else {
                logging::LogLevel::Error
            },
            // CVL-FR-28: `ai` and `backend` together, as every record a turn
            // emits carries — and `remote` besides, because an attempt at
            // reaching a provider is exactly what that domain is for. A reader
            // filtering on `backend` gets the attempts as well as the terminal
            // record that summarises them, which is the whole point of
            // correlating them by turn id.
            &[Domain::Ai, Domain::Backend, Domain::Remote],
            "model call failed",
            log_fields! {
                "turnId" => &plan.turn_id,
                "agent" => &plan.agent.nickname,
                "agentId" => &plan.agent.id,
                "provider" => &endpoint.provider,
                "model" => model_label,
                "modelCall" => model_calls,
                "attempt" => attempt,
                "maxAttempts" => policy.max_attempts,
                // CVL-FR-21: the normalized class, and — where the provider
                // answered — the status it chose. The provider's own message is
                // deliberately not carried through: an HTTP client's error
                // routinely echoes the request it made, headers included.
                "failure" => failure.failure,
                "failureClass" => failure.class,
                "status" => failure.status,
                // CVL-FR-21: the host and the cause of a refused certificate,
                // and nothing else of the request.
                "tlsHost" => failure.tls.as_ref().map(|t| t.host.as_str()),
                "tlsCause" => failure.tls.as_ref().map(|t| t.cause.as_str()),
                // CVL-FR-35: the provider's own error code and its own id for
                // the request that failed, where it answered with a structured
                // error. Neither is anything it echoed back of what was sent,
                // and the id is what takes a refusal to the provider's own
                // record of it instead of to a reproduction attempt.
                "providerCode" => failure.provider_code,
                "providerRequestId" => failure.provider_request_id.as_deref(),
                // CVL-FR-35: why the provider refused, in its own words, from a
                // structured error alone.
                "providerMessage" => failure.provider_message.as_deref(),
                "retrying" => will_retry,
                // CVL-FR-19: the delay actually chosen, so a schedule is read
                // back from the log rather than inferred.
                "plannedBackoffMs" => if will_retry { Some(planned.as_millis() as u64) } else { None },
                "durationMs" => started.elapsed().as_millis() as u64,
            },
        );

        if !will_retry {
            if cancelled {
                return Err(TurnEnd::Cancelled);
            }
            if !time_left {
                log_turn_out_of_time(app, turns, plan, model_calls);
                return Err(TurnEnd::TimedOut);
            }
            if locked {
                return Err(TurnEnd::Failed(FAIL_THREAD_LOCKED));
            }
            if let Some(tls) = failure.tls {
                return Err(TurnEnd::TlsUntrusted(tls));
            }
            return Err(TurnEnd::Failed(failure.failure));
        }

        // CVL-FR-19: the wait counts against the whole-turn deadline and is
        // cancellable where it stands.
        wait_before_retry(app, turns, plan, planned, deadline, model_calls)?;

        // CVL-FR-19: no retry *begins* after the conversation becomes locked —
        // and a lock set while the wait was running is the case the check above
        // could not have seen. Read once here rather than inside the wait: the
        // wait is sliced for cancellation, which is a flag in memory, and a lock
        // is a read of the conversation's log that has no business on that loop.
        if thread_is_locked(plan.roots.borrowed(), &plan.origin) {
            logging::log_info(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "a retry was abandoned: the conversation was locked",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "agent" => &plan.agent.nickname,
                    "discussionId" => plan.origin.discussion_id(),
                    "attempt" => attempt,
                },
            );
            return Err(TurnEnd::Failed(FAIL_THREAD_LOCKED));
        }
    }
    // Unreachable: the loop returns on every path of its last iteration, the
    // attempt budget being spent there by construction.
    Err(TurnEnd::Failed(FAIL_UNREACHABLE))
}

/// CVL-FR-19: sleep out a planned backoff, abandoning it the moment the turn is
/// cancelled or the whole-turn deadline passes.
///
/// Sliced rather than slept in one go so a cancellation lands promptly: the wait
/// is short, but an author who pressed Cancel should not watch a card think for
/// a further second.
fn wait_before_retry<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    planned: Duration,
    deadline: std::time::Instant,
    model_calls: usize,
) -> Result<(), TurnEnd> {
    const SLICE: Duration = Duration::from_millis(20);
    let until = std::time::Instant::now() + planned;
    loop {
        check_cancelled(plan)?;
        let now = std::time::Instant::now();
        if now >= deadline {
            log_turn_out_of_time(app, turns, plan, model_calls);
            return Err(TurnEnd::TimedOut);
        }
        if now >= until {
            return Ok(());
        }
        std::thread::sleep(SLICE.min(until - now).min(deadline - now));
    }
}

/// The model's own reply, back in the exchange so the next call sees what it
/// said and what it asked for (CVL-FR-12).
///
/// A provider matches a result to the call it answers by the call's id, so the
/// calls have to go back exactly as they arrived; dropping them and sending only
/// the results would leave every result unattached.
pub(super) fn assistant_message(reply: &ModelReply) -> rig::completion::Message {
    use rig::completion::message::AssistantContent;
    let mut parts = Vec::new();
    if !reply.text.is_empty() {
        parts.push(AssistantContent::text(&reply.text));
    }
    for call in &reply.tool_calls {
        parts.push(AssistantContent::ToolCall(call.clone()));
    }
    rig::completion::Message::Assistant {
        id: None,
        // Only called where the reply asked for at least one tool, so `parts` is
        // never empty; the fallback keeps `rig`'s non-empty invariant without a
        // panic if that ever stops being true.
        content: rig::OneOrMany::many(parts)
            .unwrap_or_else(|_| rig::OneOrMany::one(AssistantContent::text(&reply.text))),
    }
}
