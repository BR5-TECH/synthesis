//! The MCP tools of a turn (MCT).

use super::*;
use crate::mcp::manager::{McpCallError, McpToolSnapshot};
use crate::mcp::tool::*;
use rig::tool::ToolErrorKind;
use rmcp::model::{Resource, ResourceContents};

fn snapshot(server: &str, name: &str, description: Option<&str>) -> McpToolSnapshot {
    let mut tool = tool(name, "");
    tool.description = description.map(|d| d.to_string().into());
    McpToolSnapshot { server_id: server.to_string(), server_name: format!("Server {server}"), tool }
}

fn result(content: Vec<ContentBlock>) -> CallToolResult {
    CallToolResult::success(content)
}

// MCT-FR-TZMA: the name, with every character outside the allowed set replaced.
#[test]
fn names_carry_the_server_and_a_safe_tool_name() {
    assert_eq!(base_name("docs", "search"), "mcp__docs__search");
    assert_eq!(base_name("docs", "repo.search v2/é"), "mcp__docs__repo_search_v2__");
    assert_eq!(base_name("my-server", "a-b_c"), "mcp__my-server__a-b_c");
}

// MCT-FR-VDFB: a long name and a name that collides take the hashed form, which
// is 64 characters at most and stable.
#[test]
fn long_and_colliding_names_take_a_hash() {
    let long = "x".repeat(80);
    let tools = vec![
        snapshot("docs", "a.b", None),
        snapshot("docs", "a_b", None),
        snapshot("docs", &long, None),
    ];
    let names = model_names(&tools);
    assert_eq!(names[0], "mcp__docs__a_b");
    assert_ne!(names[1], names[0], "a collision is resolved");
    assert_eq!(names[1], hashed_name("docs", "a_b"));
    assert!(names[1].len() <= 64);
    assert_eq!(names[2], hashed_name("docs", &long));
    // The value computed apart from the code: the first 55 characters, `_`, and
    // the first 8 hexadecimal characters of SHA-256 of `docs/<name>`.
    assert_eq!(names[2], "mcp__docs__xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx_5f82046b");
    assert_eq!(names[2].len(), 64);
    assert!(names[2].starts_with(&base_name("docs", &long)[..55]));
    assert_eq!(hashed_name("docs", &long), hashed_name("docs", &long), "stable");
    assert_ne!(hashed_name("docs", "a"), hashed_name("other", "a"));
    let allowed = |n: &str| n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    assert!(names.iter().all(|n| allowed(n)));
}

// MCT-FR-EXQO: the server name in brackets, the tool name for an empty
// description, and the cut at 2,000 characters.
#[test]
fn descriptions_name_the_server() {
    assert_eq!(
        description_of(&snapshot("docs", "search", Some("Find pages."))),
        "[Server docs] Find pages."
    );
    assert_eq!(description_of(&snapshot("docs", "search", None)), "[Server docs] search");
    assert_eq!(description_of(&snapshot("docs", "search", Some("  "))), "[Server docs] search");
    let long = description_of(&snapshot("docs", "search", Some(&"d".repeat(5000))));
    assert_eq!(long.chars().count(), 2000);
}

// MCT-FR-GNUX: the schema without `$schema`, with a type where it has none.
#[test]
fn schemas_are_the_servers_with_two_repairs() {
    let mut snap = snapshot("docs", "search", None);
    let mut schema = (*snap.tool.input_schema).clone();
    schema.insert(
        "$schema".into(),
        serde_json::json!("https://json-schema.org/draft/2020-12/schema"),
    );
    schema.remove("type");
    snap.tool.input_schema = Arc::new(schema);
    let parameters = parameters_of(&snap);
    assert_eq!(parameters["type"], "object");
    assert!(parameters.get("$schema").is_none());
    assert_eq!(parameters["properties"]["text"]["type"], "string");
}

// MCT-FR-NMXZ, MCT-FR-LZUH: text blocks, embedded resources, links, and binary
// content as a line.
#[test]
fn result_blocks_become_text() {
    let link = Resource::new("file:///a.md", "a.md");
    let embedded = ContentBlock::resource(ResourceContents::text("embedded text", "file:///b.md"));
    let text = result_text(&result(vec![
        ContentBlock::text("first"),
        ContentBlock::image("aGVsbG8=", "image/png"),
        embedded,
        ContentBlock::resource_link(link),
    ]));
    assert_eq!(
        text,
        "first\n\n[The tool returned image content (image/png). It is not shown here.]\n\nembedded text\n\n[Resource: a.md file:///a.md]"
    );
}

// MCT-FR-KUIJ: structured content where no block is, and the empty result.
#[test]
fn structured_and_empty_results() {
    let mut structured = result(Vec::new());
    structured.structured_content = Some(serde_json::json!({ "count": 2 }));
    assert_eq!(result_text(&structured), "{\"count\":2}");
    assert_eq!(result_text(&result(Vec::new())), "The tool returned no content.");
}

// MCT-FR-ASCH: a long result is cut at 50,000 characters with a note.
#[test]
fn long_results_are_cut() {
    let short = "a".repeat(MAX_RESULT);
    assert_eq!(bounded_result(short.clone()), short);
    let cut = bounded_result("é".repeat(MAX_RESULT + 10));
    assert!(cut.starts_with(&"é".repeat(MAX_RESULT)));
    assert!(cut.ends_with("\n[The result was cut at 50,000 characters.]"));
}

// MCT-FR-QDEE, MCT-FR-BMFR, MCT-FR-BZVS, MCT-FR-QWPU: each failure is a refusal
// of its kind, with its retryable flag.
#[test]
fn failures_become_refusals() {
    let error =
        outcome_of("Docs", Ok(CallToolResult::error(vec![ContentBlock::text("x".repeat(3000))])))
            .unwrap_err();
    assert_eq!(error.kind(), ToolErrorKind::Other);
    assert_eq!(error.retryable(), Some(true));
    assert_eq!(error.message().chars().count(), 2000);

    let invalid =
        outcome_of("Docs", Err(McpCallError::Rpc { code: -32602, message: "m".repeat(900) }))
            .unwrap_err();
    assert_eq!(invalid.kind(), ToolErrorKind::InvalidArgs);
    assert_eq!(invalid.retryable(), Some(true));
    assert_eq!(invalid.message().chars().count(), 500);
    let other = outcome_of("Docs", Err(McpCallError::Rpc { code: -32603, message: "boom".into() }))
        .unwrap_err();
    assert_eq!(other.kind(), ToolErrorKind::Other);
    assert_eq!(other.retryable(), Some(false));
    assert_eq!(other.message(), "boom");

    let offline = outcome_of("Docs", Err(McpCallError::NotConnected)).unwrap_err();
    assert_eq!(
        offline.message(),
        "The Docs server is not connected now. Answer without its tools."
    );
    assert_eq!(offline.retryable(), Some(false));
    let late = outcome_of("Docs", Err(McpCallError::TimedOut)).unwrap_err();
    assert_eq!(late.kind(), ToolErrorKind::Timeout);
    assert_eq!(late.message(), "The Docs server did not answer in time.");
    assert_eq!(late.retryable(), Some(false));

    let ok = outcome_of("Docs", Ok(result(vec![ContentBlock::text("fine")]))).unwrap();
    assert_eq!(ok.as_text(), Some("fine"));
}

// MCT-FR-NJNW: a call reads the deadline its dispatch set, and a default
// without one.
#[tokio::test]
async fn the_deadline_comes_from_the_dispatch() {
    let inside = with_deadline(Duration::from_secs(7), async { call_deadline() }).await;
    assert_eq!(inside, Duration::from_secs(7));
    assert_eq!(call_deadline(), Duration::from_secs(300));
}

// MCT-FR-GPCO, MCT-FR-JEFK, MCT-FR-OSXG, MCT-FR-YSVS, MCT-FR-RIWQ,
// MCT-FR-WVNU, CVL-FR-CJXC, TLC-FR-WOBX: a ready server's tools become
// definitions from the server, past the cap they are dropped with one warning,
// and a call goes to the server with no confirmation and logs no argument.
#[tokio::test(flavor = "multi_thread")]
async fn ready_tools_become_dynamic_tools_that_call_the_server() {
    let (manager, connector, _secrets, _observer) = test_manager();
    {
        let mut tools = connector.server.tools.lock().unwrap();
        tools.extend((0..110).map(|i| super::tool(&format!("t{i}"), "extra")));
    }
    let project = crate::mcp::project_context("/repo", std::env::temp_dir());
    let mut record = http_record("docs", McpAuth::None);
    record.disabled_tools = vec!["fail".to_string()];
    manager.reconcile(
        project,
        vec![crate::mcp::manager::ServerInput {
            owner: "/repo".into(),
            listed: crate::mcp::config::ListedRecord { id: "docs".into(), record: Ok(record) },
            facts: crate::mcp::status::MachineFacts {
                trusted: true,
                secrets_present: true,
                ..Default::default()
            },
        }],
        None,
    );
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { .. }))).await;

    let logs = Arc::new(Mutex::new(Vec::<(LogLevel, String, Fields)>::new()));
    let sink = Arc::clone(&logs);
    let tools = conversation_tools(&manager, "/repo", move |level, message, fields| {
        sink.lock().unwrap().push((level, message.to_string(), fields));
    });
    assert_eq!(tools.len(), MAX_TURN_TOOLS);
    let names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
    assert_eq!(names[0], "mcp__docs__echo");
    assert!(!names.contains(&"mcp__docs__fail".to_string()), "a disabled tool is not offered");
    let warnings: Vec<_> =
        logs.lock().unwrap().iter().filter(|(l, _, _)| *l == LogLevel::Warn).cloned().collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].2["lost"], "docs:14");

    let echo = tools.iter().find(|t| t.name().as_str() == "mcp__docs__echo").unwrap();
    assert_eq!(echo.definition().description, "[Server docs] Return the text.");
    let output = echo.execute(serde_json::json!({ "text": "a private value" })).await.unwrap();
    assert_eq!(output.as_text(), Some("echo: a private value"));
    let refused = echo.execute(serde_json::json!("not an object")).await.unwrap_err();
    assert_eq!(refused.code(), Some(crate::tools::ARGUMENTS_UNDECODABLE));

    let text = format!("{:?}", logs.lock().unwrap());
    let records: Vec<_> = logs
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, m, _)| m == "tool call completed")
        .cloned()
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].2["server"], "docs");
    assert_eq!(records[0].2["outcome"], "done");
    assert!(!format!("{text:?}{records:?}").contains("a private value"), "no argument is logged");
    assert!(
        conversation_tools(&manager, "/other", |_, _, _| {}).is_empty(),
        "another project gets none"
    );
}

// MCT-FR-CFDC: a secret of the server reaches no name, description, schema, or
// refusal message of its tools.
#[tokio::test(flavor = "multi_thread")]
async fn no_secret_reaches_a_tool_definition_or_refusal() {
    let (manager, _connector, secrets, _observer) = test_manager();
    use crate::mcp::secrets::McpSecrets;
    secrets.set(&"/repo".into(), "docs", "bearer", "tok-SECRET-1").unwrap();
    secrets.set(&"/repo".into(), "docs", "header:X-Key", "key-SECRET-2").unwrap();
    let mut record = http_record("docs", McpAuth::Bearer);
    if let McpTransport::Http(http) = &mut record.transport {
        http.secret_headers.push("X-Key".to_string());
    }
    let project = crate::mcp::project_context("/repo", std::env::temp_dir());
    manager.reconcile(
        project,
        vec![crate::mcp::manager::ServerInput {
            owner: "/repo".into(),
            listed: crate::mcp::config::ListedRecord { id: "docs".into(), record: Ok(record) },
            facts: crate::mcp::status::MachineFacts {
                trusted: true,
                secrets_present: true,
                ..Default::default()
            },
        }],
        None,
    );
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { .. }))).await;
    let tools = conversation_tools(&manager, "/repo", |_, _, _| {});
    let definitions =
        serde_json::to_string(&tools.iter().map(|t| t.definition()).collect::<Vec<_>>()).unwrap();
    manager.close_project();
    let refused = tools[0].execute(serde_json::json!({})).await.unwrap_err();
    for text in [definitions, refused.message().to_string()] {
        assert!(!text.contains("SECRET"), "{text}");
    }
}

// MCT-FR-NJNW, MCT-FR-QWPU: a call reads the deadline its dispatch set, and a
// server that does not answer by then gives the timeout refusal.
#[tokio::test(flavor = "multi_thread")]
async fn a_call_ends_at_the_deadline_of_its_dispatch() {
    let (manager, _connector, _secrets, _observer) = test_manager();
    let project = crate::mcp::project_context("/repo", std::env::temp_dir());
    manager.reconcile(
        project,
        vec![crate::mcp::manager::ServerInput {
            owner: "/repo".into(),
            listed: crate::mcp::config::ListedRecord {
                id: "docs".into(),
                record: Ok(http_record("docs", McpAuth::None)),
            },
            facts: crate::mcp::status::MachineFacts {
                trusted: true,
                secrets_present: true,
                ..Default::default()
            },
        }],
        None,
    );
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { .. }))).await;
    let tools = conversation_tools(&manager, "/repo", |_, _, _| {});
    let hang = tools.iter().find(|t| t.name().as_str() == "mcp__docs__hang").unwrap();
    let refused = with_deadline(Duration::from_millis(400), hang.execute(serde_json::json!({})))
        .await
        .unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::Timeout);
    assert_eq!(refused.message(), "The Server docs server did not answer in time.");
}
