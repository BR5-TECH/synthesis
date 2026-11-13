//! The MCP tools of a turn (`specifications/tools/MCT-mcp-tool.md`).
//!
//! Each tool of a `ready` server becomes one `rig::tool::DynamicTool`. Its name,
//! description, and schema come from the server; its call goes to the server
//! through the manager, and its result comes back as text.

use std::collections::HashSet;
use std::time::Duration;

use rig::tool::{ToolErrorKind, ToolExecutionError, ToolOutput};
use rmcp::model::{CallToolResult, ContentBlock, ResourceContents};
use sha2::{Digest, Sha256};

use super::manager::{McpCallError, McpManager, McpToolSnapshot};
use crate::log_fields;
use crate::logging::{Domain, LogLevel};

/// MCT-FR-JEFK: the most MCP tools one turn is offered.
pub const MAX_TURN_TOOLS: usize = 100;
/// MCT-FR-TZMA, MCT-FR-VDFB: the longest tool name.
const MAX_NAME: usize = 64;
/// MCT-FR-VDFB: the length a long name is cut to before its hash.
const CUT_NAME: usize = 55;
/// MCT-FR-EXQO: the longest description.
const MAX_DESCRIPTION: usize = 2_000;
/// MCT-FR-ASCH: the longest result text.
pub const MAX_RESULT: usize = 50_000;
/// MCT-FR-QDEE: the longest message of an `isError` result.
const MAX_ERROR_RESULT: usize = 2_000;
/// MCT-FR-BMFR: the longest JSON-RPC error message.
const MAX_RPC_MESSAGE: usize = 500;
/// MCT-FR-NJNW: the call ends a little before the turn deadline, so `rmcp`
/// sends its cancel on a runtime that is still alive.
const DEADLINE_MARGIN: Duration = Duration::from_millis(250);
/// MCT-FR-NJNW: the deadline of a call when no turn set one, as in a test.
const DEFAULT_DEADLINE: Duration = Duration::from_secs(5 * 60);

tokio::task_local! {
    /// MCT-FR-NJNW: the time that remains under the whole-turn deadline, set by
    /// the dispatch of the call.
    pub static CALL_DEADLINE: Duration;
}

/// MCT-FR-NJNW: run one tool call with the time that remains of its turn.
pub async fn with_deadline<F: std::future::Future>(remaining: Duration, call: F) -> F::Output {
    CALL_DEADLINE.scope(remaining, call).await
}

/// MCT-FR-NJNW: the deadline of the call in progress.
pub fn call_deadline() -> Duration {
    CALL_DEADLINE.try_with(|deadline| *deadline).unwrap_or(DEFAULT_DEADLINE)
}

/// CVL-FR-FWZZ: the key input of each MCP tool name offered to a turn.
static KEY_INPUTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

fn key_inputs() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    KEY_INPUTS.get_or_init(Default::default)
}

/// CVL-FR-FWZZ: `<server name>: <tool name>` for a model-facing MCP tool name,
/// from the turns that offered it. It holds no argument value.
pub fn key_input_for(model_name: &str) -> Option<String> {
    key_inputs().lock().ok()?.get(model_name).cloned()
}

/// MCT-FR-TZMA: the prefix that marks a tool as an MCP tool.
pub const NAME_PREFIX: &str = "mcp__";

/// Cut `text` at `max` characters.
fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// MCT-FR-TZMA: `mcp__<server id>__<tool name>`, with each character of the
/// tool name outside `[A-Za-z0-9_-]` replaced by `_`.
pub fn base_name(server_id: &str, tool_name: &str) -> String {
    let tool: String = tool_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    format!("{NAME_PREFIX}{server_id}__{tool}")
}

/// MCT-FR-VDFB: the name cut to 55 characters, then `_` and 8 hexadecimal
/// characters of the SHA-256 of `<server id>/<tool name>`.
pub fn hashed_name(server_id: &str, tool_name: &str) -> String {
    let base = base_name(server_id, tool_name);
    let digest = Sha256::digest(format!("{server_id}/{tool_name}").as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{}_{hex}", cut(&base, CUT_NAME))
}

/// MCT-FR-TZMA, MCT-FR-VDFB: the model-facing names of the tools, in order. A
/// name that is too long, or that an earlier tool has, takes its hashed form.
pub fn model_names(tools: &[McpToolSnapshot]) -> Vec<String> {
    let mut taken = HashSet::new();
    tools
        .iter()
        .map(|snapshot| {
            let base = base_name(&snapshot.server_id, &snapshot.tool.name);
            let name = if base.chars().count() > MAX_NAME || taken.contains(&base) {
                hashed_name(&snapshot.server_id, &snapshot.tool.name)
            } else {
                base
            };
            taken.insert(name.clone());
            name
        })
        .collect()
}

/// MCT-FR-EXQO: `[<server name>] ` and the description, or the tool name.
pub fn description_of(snapshot: &McpToolSnapshot) -> String {
    let text = snapshot
        .tool
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(snapshot.tool.name.as_ref());
    cut(&format!("[{}] {text}", snapshot.server_name), MAX_DESCRIPTION)
}

/// MCT-FR-GNUX: the schema of the server, with no top-level `$schema` and with
/// `"type": "object"` where it has no type.
pub fn parameters_of(snapshot: &McpToolSnapshot) -> serde_json::Value {
    let mut schema = (*snapshot.tool.input_schema).clone();
    schema.remove("$schema");
    schema.entry("type").or_insert_with(|| serde_json::json!("object"));
    serde_json::Value::Object(schema)
}

/// MCT-FR-NMXZ, MCT-FR-LZUH, MCT-FR-KUIJ: the text of a result, not cut.
pub fn result_text(result: &CallToolResult) -> String {
    let parts: Vec<String> = result
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(text) => text.text.clone(),
            ContentBlock::Image(image) => {
                format!(
                    "[The tool returned image content ({}). It is not shown here.]",
                    image.mime_type
                )
            }
            ContentBlock::Audio(audio) => {
                format!(
                    "[The tool returned audio content ({}). It is not shown here.]",
                    audio.mime_type
                )
            }
            ContentBlock::Resource(embedded) => match &embedded.resource {
                ResourceContents::TextResourceContents { text, .. } => text.clone(),
                ResourceContents::BlobResourceContents { mime_type, .. } => format!(
                    "[The tool returned binary content ({}). It is not shown here.]",
                    mime_type.as_deref().unwrap_or("application/octet-stream")
                ),
                _ => "[The tool returned content of a kind this application does not show.]"
                    .to_string(),
            },
            ContentBlock::ResourceLink(link) => format!("[Resource: {} {}]", link.name, link.uri),
            _ => {
                "[The tool returned content of a kind this application does not show.]".to_string()
            }
        })
        .collect();
    if !parts.is_empty() {
        return parts.join("\n\n");
    }
    match &result.structured_content {
        Some(value) => value.to_string(),
        None => "The tool returned no content.".to_string(),
    }
}

/// MCT-FR-ASCH: cut a result at 50,000 characters, with a note.
pub fn bounded_result(text: String) -> String {
    if text.chars().count() <= MAX_RESULT {
        return text;
    }
    format!("{}\n[The result was cut at 50,000 characters.]", cut(&text, MAX_RESULT))
}

/// The output or the refusal of one call (MCT-FR-QDEE to MCT-FR-QWPU).
pub fn outcome_of(
    server_name: &str,
    result: Result<CallToolResult, McpCallError>,
) -> Result<ToolOutput, ToolExecutionError> {
    match result {
        Ok(result) if result.is_error == Some(true) => Err(ToolExecutionError::new(
            ToolErrorKind::Other,
            cut(&result_text(&result), MAX_ERROR_RESULT),
        )
        .with_retryable(true)),
        Ok(result) => Ok(ToolOutput::text(bounded_result(result_text(&result)))),
        Err(McpCallError::NotConnected) => Err(ToolExecutionError::new(
            ToolErrorKind::Other,
            format!("The {server_name} server is not connected now. Answer without its tools."),
        )
        .with_retryable(false)),
        Err(McpCallError::TimedOut) => Err(ToolExecutionError::new(
            ToolErrorKind::Timeout,
            format!("The {server_name} server did not answer in time."),
        )
        .with_retryable(false)),
        Err(McpCallError::Rpc { code, message }) => {
            let kind =
                if code == -32602 { ToolErrorKind::InvalidArgs } else { ToolErrorKind::Other };
            Err(ToolExecutionError::new(kind, cut(&message, MAX_RPC_MESSAGE))
                .with_retryable(code == -32602))
        }
        Err(McpCallError::ProtocolError) => Err(ToolExecutionError::new(
            ToolErrorKind::Other,
            format!("The {server_name} server did not answer as an MCP server."),
        )
        .with_retryable(false)),
    }
}

/// The outcome of a call, for its log record (MCT-FR-RIWQ).
fn outcome_code(result: &Result<CallToolResult, McpCallError>) -> &'static str {
    match result {
        Ok(result) if result.is_error == Some(true) => "tool_error",
        Ok(_) => "done",
        Err(McpCallError::NotConnected) => "not_connected",
        Err(McpCallError::TimedOut) => "timed_out",
        Err(McpCallError::Rpc { .. }) => "rpc_error",
        Err(McpCallError::ProtocolError) => "protocol_error",
    }
}

/// MCT-FR-GPCO, MCT-FR-JEFK: the MCP tools of one turn. `log` takes one record
/// for each server that lost tools to the cap, and for each tool skipped.
pub fn conversation_tools(
    manager: &McpManager,
    project_key: &str,
    log: impl Fn(LogLevel, &str, crate::logging::Fields) + Clone + Send + Sync + 'static,
) -> Vec<rig::tool::DynamicTool> {
    let mut snapshots = manager.mcp_tools_now(project_key);
    if snapshots.len() > MAX_TURN_TOOLS {
        let mut lost: Vec<(String, usize)> = Vec::new();
        for dropped in &snapshots[MAX_TURN_TOOLS..] {
            match lost.iter_mut().find(|(id, _)| *id == dropped.server_id) {
                Some((_, count)) => *count += 1,
                None => lost.push((dropped.server_id.clone(), 1)),
            }
        }
        snapshots.truncate(MAX_TURN_TOOLS);
        let servers: Vec<String> = lost.iter().map(|(id, count)| format!("{id}:{count}")).collect();
        log(
            LogLevel::Warn,
            "mcp tools over the turn limit",
            log_fields! { "lost" => servers.join(","), "limit" => MAX_TURN_TOOLS },
        );
    }
    let names = model_names(&snapshots);
    snapshots
        .into_iter()
        .zip(names)
        .filter_map(|(snapshot, name)| {
            // A name of MCT-FR-TZMA holds only the characters a name allows.
            let tool_name = rig::completion::message::ToolName::new(name.clone()).ok()?;
            let description = description_of(&snapshot);
            let parameters = parameters_of(&snapshot);
            if let Ok(mut inputs) = key_inputs().lock() {
                inputs.insert(
                    name.clone(),
                    format!("{}: {}", snapshot.server_name, snapshot.tool.name),
                );
            }
            let manager = manager.clone();
            let log = log.clone();
            let model_name = name.clone();
            Some(rig::tool::DynamicTool::new(
                tool_name,
                description,
                parameters,
                move |arguments: serde_json::Value| {
                    let manager = manager.clone();
                    let log = log.clone();
                    let snapshot = snapshot.clone();
                    let model_name = model_name.clone();
                    Box::pin(async move {
                        let arguments =
                            match arguments {
                                serde_json::Value::Object(map) => map,
                                serde_json::Value::Null => serde_json::Map::new(),
                                _ => return Err(
                                    crate::agent_conversations::tool_wiring::undecodable_arguments(
                                    ),
                                ),
                            };
                        let started = std::time::Instant::now();
                        let deadline = call_deadline().saturating_sub(DEADLINE_MARGIN);
                        let result = manager
                            .call_mcp_tool(
                                &snapshot.server_id,
                                &snapshot.tool.name,
                                arguments,
                                deadline,
                            )
                            .await;
                        let outcome = outcome_code(&result);
                        let level = if outcome == "done" { LogLevel::Info } else { LogLevel::Warn };
                        let message = if outcome == "done" {
                            "tool call completed"
                        } else {
                            "tool call refused"
                        };
                        log(
                            level,
                            message,
                            log_fields! {
                                "tool" => &model_name,
                                "server" => &snapshot.server_id,
                                "outcome" => outcome,
                                "durationMs" => started.elapsed().as_millis() as u64,
                            },
                        );
                        outcome_of(&snapshot.server_name, result)
                    })
                },
            ))
        })
        .collect()
}

/// The domains of a tool call record (TLC-FR-14).
pub const TOOL_DOMAINS: [Domain; 2] = [Domain::Ai, Domain::Backend];
