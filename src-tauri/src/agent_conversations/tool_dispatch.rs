//! One tool call, dispatched and answered (CVL-FR-12).

use super::*;

/// Run one tool the model asked for, and return the text of what it produced.
///
/// Every outcome is a *result*, never a turn failure (CVL-FR-14): a refusal
/// reaches the model carrying the tool's own message so it can correct itself on
/// a further call inside this same turn, and a tool the model invented by name
/// is told so on the same terms. A turn fails only when a model call fails.
pub(super) fn dispatch_tool<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    call: &rig::completion::message::ToolCall,
    remaining: Duration,
) -> String {
    let name = call.function.name.to_string();
    let Some(tool) = plan
        .tools
        .iter()
        .find(|tool| tool.name().as_str() == name)
        .cloned()
    else {
        // Not a defect in the project — the model named something that is not on
        // offer, which the definitions it was given already contradict.
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent asked for a tool that is not attached",
            log_fields! { "turnId" => &plan.turn_id, "tool" => &name },
        );
        return "No tool by that name is available to you. Use one of the tools you were given, or answer without it.".to_string();
    };

    let started = std::time::Instant::now();
    let sent = arguments_as_sent(&call.function);
    let outcome = if call.function.invalid_arguments.is_some() {
        // TLC-FR-SPRR: arguments the model sent as anything but an object are
        // refused before the tool runs, also when the framework recovered some
        // fields of a cut-off object — a tool that ran on them would act on
        // half of what the model meant to send.
        Ok(Err(undecodable_arguments()))
    } else {
        let arguments = call.function.arguments_value();
        // The tools are synchronous underneath (each resolves before its
        // future is polled), so this neither blocks on I/O nor holds the
        // deadline open; it is the same owned-thread pattern the model calls
        // use, for the same reason — a nested runtime inside a Tokio worker
        // panics at runtime.
        block_on_with_timeout(async move { Ok(tool.execute(arguments).await) }, remaining)
    };

    match outcome {
        Ok(Ok(output)) => {
            logging::log_debug(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "agent tool call completed",
                // The shape of the answer and never a value drawn from it: a
                // tool's result is the project's own material (CVL-FR-28). The
                // tool logs its own call on its own terms besides (TLC-FR-14).
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "tool" => &name,
                    "resultChars" => output.render().chars().count(),
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            output.render()
        }
        Ok(Err(error)) => {
            // TLC-FR-14: a call whose arguments never decoded is refused
            // **before the tool runs**, so the tool logs nothing of its own and
            // this record is the only one there is. Without the shape the model
            // sent, it says a call was refused for its arguments and nothing at
            // all about which of them was wrong, which is the one question a
            // reader has. Value kinds, and only the field names the tool's own
            // schema declares — never a value (`crate::tools::argument_shape`).
            if error.code() == Some(crate::tools::ARGUMENTS_UNDECODABLE) {
                // The schema is read back from the tool rather than carried
                // here, so a tool that gains a parameter needs nothing of this
                // module. Found again rather than held across the dispatch,
                // because the shape is wanted on one route out of five.
                let schema = plan
                    .tools
                    .iter()
                    .find(|tool| tool.name().as_str() == name)
                    .map(|tool| tool.definition().parameters)
                    .unwrap_or(serde_json::Value::Null);
                logging::log_warn(
                    app,
                    turns.buffer(),
                    &[Domain::Ai, Domain::Backend],
                    "tool call refused",
                    log_fields! {
                        // The turn, as every other record of this module
                        // carries it: a refusal that cannot be tied to a turn
                        // cannot be read beside the calls either side of it,
                        // which is the whole of what makes one followable.
                        "turnId" => &plan.turn_id,
                        "tool" => &name,
                        "reason" => crate::tools::ARGUMENTS_UNDECODABLE,
                        // Read from the error rather than asserted here, so the
                        // record cannot say a further call may succeed after
                        // the boundary stops saying so.
                        "retryable" => error.retryable().unwrap_or(false),
                        "argShape" => crate::tools::argument_shape(&sent, &schema),
                    },
                );
            }
            logging::log_debug(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "agent tool call refused",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "tool" => &name,
                    // The kind and the flag, not the sentence: the message is
                    // authored by the tool and the pair is what a reader filters
                    // on.
                    "kind" => error.kind().to_string(),
                    "retryable" => error.retryable().unwrap_or(false),
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            // The tool's own message, which is written to be read by a model
            // (TLC-FR-09), plus whether a further call could succeed — the
            // `retryable` flag is otherwise invisible to a model reading a
            // result as text (TLC-FR-11).
            if error.retryable() == Some(false) {
                format!(
                    "{} This will not succeed on a further call; do not retry it.",
                    error.message()
                )
            } else {
                error.message().to_string()
            }
        }
        // Told apart, because they call for different reactions from whoever
        // reads the log: one is a slow tool, the other is a broken one. Reported
        // to the model as the same kind of thing — something it cannot fix by
        // calling differently — but never as a timeout that did not happen.
        Err(failure) if failure.failure == FAIL_TIMED_OUT => {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "agent tool call ran out of time",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "tool" => &name,
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            "That tool did not finish in time. Answer with what you already have.".to_string()
        }
        Err(_) => {
            // A tool that panicked, or a runtime that would not start. `ERROR`
            // rather than `WARN`: nothing here is an expected outcome, and a
            // reader told a tool "timed out" would go looking for a slow disk
            // instead of for the defect this actually is.
            logging::log_error(
                app,
                turns.buffer(),
                &[Domain::Ai, Domain::Backend],
                "agent tool call failed",
                log_fields! {
                    "turnId" => &plan.turn_id,
                    "tool" => &name,
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            );
            "That tool could not be run. Answer with what you already have, or continue without it."
                .to_string()
        }
    }
}

/// TLC-FR-SPRR: the arguments of a call as the model sent them.
///
/// The framework keeps a call's arguments as an object and holds the text the
/// model sent beside it when that text was not one. The shape of a refused call
/// is read from what the model sent, so text that is JSON reads as its own
/// kind and text that is not JSON reads as `string`.
pub(super) fn arguments_as_sent(function: &rig::completion::message::ToolFunction) -> serde_json::Value {
    match &function.invalid_arguments {
        Some(text) => serde_json::from_str(text)
            .unwrap_or_else(|_| serde_json::Value::String(String::new())),
        None => function.arguments_value(),
    }
}

pub(super) fn check_cancelled(plan: &TurnPlan) -> Result<(), TurnEnd> {
    if plan.cancelled.load(Ordering::SeqCst) {
        Err(TurnEnd::Cancelled)
    } else {
        Ok(())
    }
}
