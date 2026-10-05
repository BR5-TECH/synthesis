//! Codex's wire protocol, transcribed from
//! `specifications/infra/CDX-codex-cli-protocol.md` and from nothing else.
//!
//! This module is the only place in the tree that knows how Codex is invoked or
//! how its answer is read. It names no flag, output format, credential, or
//! session term belonging to any other vendor, and it reads nothing from the
//! sibling module that transcribes one — which is what makes an upgrade to
//! either protocol a change to one file (EAC-FR-11, EAC-FR-12).

use serde_json::{Map, Value};

use super::{ActivityKind, ExtractContext, Extracted, VendorActivity};
use crate::tools::agent_exec::protocol::{decode_envelope, EnvelopeInvalid};

/// The specification this module transcribes (EAC-FR-10's `protocol_spec`).
pub const SPEC: &str = "specifications/infra/CDX-codex-cli-protocol.md";

/// CDX-FR-01: the version every element here was asserted against.
pub const PINNED_CLI_VERSION: &str = "0.147.0";

/// CDX-FR-24: the variable addressing the mount this CLI reads its login from
/// and writes its session state beneath. One directory serves both, which is why
/// CDX-FR-23 makes that mount read/write.
pub const SESSION_STATE_ENV: &str = "CODEX_HOME";

/// CDX-FR-07's config-override key. This CLI has no reasoning-effort flag, so a
/// resolved effort reaches it as a configuration override or not at all.
const EFFORT_KEY: &str = "model_reasoning_effort";
/// CDX-FR-10's config-override key, used on a resumed turn because `exec resume`
/// does not accept `--sandbox`.
const SANDBOX_KEY: &str = "sandbox_mode";
const SANDBOX_VALUE: &str = "danger-full-access";

// ---------------------------------------------------------------------------
// The argument vectors (CDX-FR-02, CDX-FR-09)
// ---------------------------------------------------------------------------

/// A `-c key="value"` override. The value is quoted because the `value` portion
/// of an override is parsed as TOML, and an unquoted identifier would not parse
/// as the string it is meant to be (CDX-FR-07).
fn config_override(key: &str, value: &str) -> [String; 2] {
    ["-c".to_string(), format!("{key}=\"{value}\"")]
}

/// Whether a value can be carried inside a quoted TOML override without
/// changing what the override means.
///
/// The value lands between quotes in text the CLI parses as TOML, so a quote, a
/// backslash, or a newline in it would close the string early and let the rest
/// be read as further configuration. Nothing composes these values but this
/// application's own vendor descriptor, so this is a guard against a future
/// identifier rather than against a caller — but it is the difference between
/// "a bad effort id is refused" and "a bad effort id silently rewrites the
/// sandbox policy", and it costs one comparison.
fn is_safe_override_value(value: &str) -> bool {
    !value
        .chars()
        .any(|c| c == '"' || c == '\\' || c == '\n' || c == '\r' || c.is_control())
}

fn model_and_effort(model_id: Option<&str>, effort_id: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if let Some(model) = model_id.filter(|m| !m.is_empty()) {
        args.push("--model".into());
        args.push(model.to_string());
    }
    if let Some(effort) = effort_id.filter(|e| !e.is_empty() && is_safe_override_value(e)) {
        args.extend(config_override(EFFORT_KEY, effort));
    }
    args
}

/// CDX-FR-07: an effort identifier this CLI's override syntax can carry. Every
/// value the application offers passes; the CLI itself validates the rest.
pub fn effort_supported(effort_id: &str) -> bool {
    is_safe_override_value(effort_id)
}

/// CDX-FR-02: `exec --json --sandbox danger-full-access --skip-git-repo-check
/// [--model M] [-c model_reasoning_effort="E"] -`.
pub fn fresh_argv(model_id: Option<&str>, effort_id: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        // CDX-FR-03: the subcommand that makes the CLI non-interactive, and the
        // flag that turns stdout into an event stream rather than prose.
        "exec".into(),
        "--json".into(),
        // CDX-FR-04: the container is already the isolation boundary, and a
        // narrower sandbox would refuse the writes the turn exists to make.
        "--sandbox".into(),
        SANDBOX_VALUE.into(),
        // CDX-FR-05: without this the CLI refuses to run outside a Git
        // repository, making the outcome depend on whether the supplied
        // execution directory happened to be one.
        "--skip-git-repo-check".into(),
    ];
    args.extend(model_and_effort(model_id, effort_id));
    // CDX-FR-08: the positional prompt, which is what directs the CLI to take
    // the instructions from stdin. Always last, so the task never occupies an
    // argument vector position.
    args.push("-".into());
    args
}

/// CDX-FR-09: a distinct grammar rather than the fresh vector with `resume`
/// inserted.
///
/// `exec resume` takes `[SESSION_ID] [PROMPT]` positionally in that order, and
/// its option set is a strict subset of `exec`'s — so the sandbox policy arrives
/// as a configuration override (CDX-FR-10) and none of the options this
/// subcommand does not accept is generated (CDX-FR-11).
pub fn resume_argv(
    model_id: Option<&str>,
    effort_id: Option<&str>,
    resume_session_id: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".into(),
        "resume".into(),
        "--json".into(),
        "--skip-git-repo-check".into(),
    ];
    args.extend(config_override(SANDBOX_KEY, SANDBOX_VALUE));
    args.extend(model_and_effort(model_id, effort_id));
    args.push(resume_session_id.to_string());
    args.push("-".into());
    args
}

// ---------------------------------------------------------------------------
// Extraction (CDX-FR-12 through CDX-FR-17)
// ---------------------------------------------------------------------------

const EVENT_THREAD_STARTED: &str = "thread.started";
const EVENT_TURN_FAILED: &str = "turn.failed";
const EVENT_ITEM_COMPLETED: &str = "item.completed";
const KEY_THREAD_ID: &str = "thread_id";
const ITEM_AGENT_MESSAGE: &str = "agent_message";
const KEY_ITEM: &str = "item";
const KEY_ITEM_TYPE: &str = "type";
const KEY_ITEM_TEXT: &str = "text";

pub fn extract(context: &ExtractContext<'_>) -> Result<Extracted, EnvelopeInvalid> {
    let mut session_id: Option<String> = None;
    let mut last_agent_message: Option<String> = None;
    let mut turn_failed = false;

    for line in context.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // CDX-FR-17: a line that is not valid JSON, and an event carrying a type
        // this protocol does not recognize, are ignored rather than treated as a
        // failure — the vendor documents that unknown fields and events may
        // appear. What is never ignored is the extraction position being absent.
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            continue;
        };

        match event_type {
            // CDX-FR-20: the session identifier, which the executor cannot
            // preassign because this CLI has no argument that supplies one for a
            // new session.
            EVENT_THREAD_STARTED => {
                if let Some(id) = event.get(KEY_THREAD_ID).and_then(Value::as_str) {
                    session_id = Some(id.to_string());
                }
            }
            // CDX-FR-16: a failed run, and one that yields no envelope even
            // where an earlier agent message is present in the stream.
            EVENT_TURN_FAILED => turn_failed = true,
            EVENT_ITEM_COMPLETED => {
                let Some(item) = event.get(KEY_ITEM).and_then(Value::as_object) else {
                    continue;
                };
                if item.get(KEY_ITEM_TYPE).and_then(Value::as_str) != Some(ITEM_AGENT_MESSAGE) {
                    continue;
                }
                // CDX-FR-12: the *last* such item. A turn may narrate before it
                // concludes, and the conclusion is what the envelope is.
                //
                // A final agent message carrying no usable text replaces the
                // previous one with nothing rather than being skipped over.
                // Skipping it would promote an earlier narration to "the
                // conclusion" and answer the caller with the wrong message —
                // a wrong answer, where this is a missing one.
                last_agent_message = item
                    .get(KEY_ITEM_TEXT)
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            // CDX-FR-15: a top-level `error` event is not by itself fatal — the
            // CLI emits transient reconnect notices under that type while
            // retrying a dropped stream. It is a diagnostic on the captured
            // stream rather than a signal to abandon extraction.
            _ => {}
        }
    }

    if turn_failed {
        return Err(EnvelopeInvalid::VendorRunFailed);
    }

    // CDX-FR-13 / CDX-FR-14: the envelope's shape is not enforced by the CLI,
    // because `--output-schema` takes a filesystem path and would require
    // mounting a schema file. A malformed envelope is therefore an expected
    // failure mode of this vendor rather than a rare one.
    let text = last_agent_message.ok_or(EnvelopeInvalid::NotFound)?;
    let envelope = decode_envelope(&text)?;

    Ok(Extracted {
        envelope,
        session_id,
    })
}

// ---------------------------------------------------------------------------
// Observed activity (CDX-FR-28)
// ---------------------------------------------------------------------------

/// This vendor's own event and item vocabulary, read for a reader's benefit
/// alone. Nothing here decides an outcome, and an event outside this vocabulary
/// is reported rather than dropped.
const EVENT_TURN_STARTED: &str = "turn.started";
const EVENT_TURN_COMPLETED: &str = "turn.completed";
const EVENT_ERROR: &str = "error";
const EVENT_ITEM_STARTED: &str = "item.started";
const EVENT_ITEM_UPDATED: &str = "item.updated";
const ITEM_REASONING: &str = "reasoning";
const ITEM_COMMAND_EXECUTION: &str = "command_execution";
const ITEM_FILE_CHANGE: &str = "file_change";
const ITEM_MCP_TOOL_CALL: &str = "mcp_tool_call";
const ITEM_WEB_SEARCH: &str = "web_search";
const ITEM_TODO_LIST: &str = "todo_list";
const ITEM_ERROR: &str = "error";

/// CDX-FR-28: one line of the event stream, read as an activity.
pub fn activity(line: &str) -> VendorActivity {
    let Ok(Value::Object(event)) = serde_json::from_str::<Value>(line.trim()) else {
        return unrecognized(line);
    };
    let Some(event_type) = event.get("type").and_then(Value::as_str) else {
        return unrecognized(line);
    };

    match event_type {
        EVENT_THREAD_STARTED => VendorActivity {
            kind: ActivityKind::Started,
            summary: "thread started".to_string(),
        },
        EVENT_TURN_STARTED => VendorActivity {
            kind: ActivityKind::Started,
            summary: "turn started".to_string(),
        },
        EVENT_TURN_COMPLETED => VendorActivity {
            kind: ActivityKind::Finished,
            summary: format!("turn completed{}", usage_suffix(&event)),
        },
        EVENT_TURN_FAILED => VendorActivity {
            kind: ActivityKind::Error,
            summary: format!("turn failed · {}", error_message(&event)),
        },
        EVENT_ERROR => VendorActivity {
            // CDX-FR-15: a top-level error event is not by itself fatal — this
            // CLI writes transient reconnect notices under it — so it is
            // reported as what went wrong rather than as the turn ending.
            kind: ActivityKind::Error,
            summary: error_message(&event),
        },
        EVENT_ITEM_STARTED | EVENT_ITEM_UPDATED | EVENT_ITEM_COMPLETED => {
            item_activity(&event, event_type)
        }
        _ => unrecognized(line),
    }
}

/// An item event, read through the item it carries.
fn item_activity(event: &Map<String, Value>, event_type: &str) -> VendorActivity {
    let Some(item) = event.get(KEY_ITEM).and_then(Value::as_object) else {
        return VendorActivity {
            kind: ActivityKind::Unrecognized,
            summary: event_type.to_string(),
        };
    };
    let item_type = item.get(KEY_ITEM_TYPE).and_then(Value::as_str).unwrap_or("");
    // Started and updated are the same item mid-flight; the mark keeps a row
    // that will be superseded from reading as a completed one.
    let stage = if event_type == EVENT_ITEM_COMPLETED { "" } else { "…" };
    let text = |key: &str| item.get(key).and_then(Value::as_str).map(one_line);

    let (kind, body) = match item_type {
        ITEM_AGENT_MESSAGE => (
            ActivityKind::Message,
            text(KEY_ITEM_TEXT).unwrap_or_default(),
        ),
        ITEM_REASONING => (
            ActivityKind::Reasoning,
            text(KEY_ITEM_TEXT).unwrap_or_default(),
        ),
        ITEM_COMMAND_EXECUTION => {
            let command = text("command").unwrap_or_default();
            let status = item.get("status").and_then(Value::as_str).unwrap_or("");
            let exit = item.get("exit_code").and_then(Value::as_i64);
            let mut body = format!("$ {command}");
            if !status.is_empty() {
                body.push_str(&format!(" · {status}"));
            }
            if let Some(exit) = exit {
                body.push_str(&format!(" · exit {exit}"));
            }
            (ActivityKind::Command, body)
        }
        ITEM_FILE_CHANGE => {
            let changes = item
                .get("changes")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let status = item.get("status").and_then(Value::as_str).unwrap_or("");
            (
                ActivityKind::FileChange,
                format!("{changes} file changes · {status}").trim_end().to_string(),
            )
        }
        ITEM_MCP_TOOL_CALL => {
            let server = item.get("server").and_then(Value::as_str).unwrap_or("?");
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("?");
            let failed = item.get("error").is_some_and(|e| !e.is_null());
            (
                if failed { ActivityKind::Error } else { ActivityKind::ToolCall },
                format!("{server}.{tool}"),
            )
        }
        ITEM_WEB_SEARCH => (
            ActivityKind::ToolCall,
            format!("web search · {}", text("query").unwrap_or_default()),
        ),
        ITEM_TODO_LIST => {
            let items = item
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            (ActivityKind::ToolResult, format!("todo list · {items} items"))
        }
        ITEM_ERROR => (
            ActivityKind::Error,
            text("message").unwrap_or_default(),
        ),
        other => (ActivityKind::Unrecognized, other.to_string()),
    };

    VendorActivity {
        kind,
        summary: format!("{body}{stage}"),
    }
}

/// CDX's own usage accounting, where the event carries it.
fn usage_suffix(event: &Map<String, Value>) -> String {
    let Some(usage) = event.get("usage").and_then(Value::as_object) else {
        return String::new();
    };
    let field = |key: &str| {
        usage
            .get(key)
            .and_then(Value::as_i64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string())
    };
    format!(
        " · in {} · cached {} · out {}",
        field("input_tokens"),
        field("cached_input_tokens"),
        field("output_tokens")
    )
}

fn error_message(event: &Map<String, Value>) -> String {
    event
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .or_else(|| event.get("message"))
        .and_then(Value::as_str)
        .map(one_line)
        .unwrap_or_else(|| "no message".to_string())
}

fn unrecognized(line: &str) -> VendorActivity {
    VendorActivity {
        kind: ActivityKind::Unrecognized,
        summary: one_line(line),
    }
}

use super::summary_line as one_line;
