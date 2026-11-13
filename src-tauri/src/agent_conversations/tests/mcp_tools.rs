//! The MCP tools in a conversational turn (CVL-FR-TJZI, CVL-FR-FWZZ,
//! MCT-FR-GPCO, MCT-FR-YSVS).
//!
//! The project's MCP manager connects to an in-process server, and each test
//! observes the request and the exchange the model seam was given.

use super::*;
use crate::mcp::config::{ListedRecord, McpAuth};
use crate::mcp::manager::ServerInput;
use crate::mcp::status::{MachineFacts, McpServerStatus};

/// Connect one ready MCP server, `docs`, for the open project of `h`.
fn connect_docs(h: &Harness) {
    let (manager, _connector, _secrets, _observer) = crate::mcp::tests::test_manager();
    h.app.manage(manager.clone());
    let key = h.app.state::<crate::project::ProjectState>().slot_key();
    let facts = MachineFacts {
        trusted: true,
        secrets_present: true,
        signed_in: true,
        vault_unavailable: false,
    };
    let record = crate::mcp::tests::http_record("docs", McpAuth::None);
    let input = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: "docs".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(
        crate::mcp::project_context(&key, h.root.path().to_path_buf()),
        vec![input],
        None,
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !matches!(manager.snapshot("docs"), Some((McpServerStatus::Ready { .. }, _))) {
        assert!(std::time::Instant::now() < deadline, "the server never became ready");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

// CVL-FR-TJZI, MCT-FR-GPCO: a turn of a project with a ready server is offered
// its tools after every tool of the application.
#[test]
fn a_turn_is_offered_the_tools_of_a_ready_server_last() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.mount();
    connect_docs(&h);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let tools: Vec<String> =
        h.seam.requests()[0].0.tools.iter().map(|t| t.name.as_str().to_string()).collect();
    let first_mcp = tools.iter().position(|t| t.starts_with("mcp__")).expect("an MCP tool");
    assert!(
        tools[first_mcp..].iter().all(|t| t.starts_with("mcp__")),
        "MCP tools come last: {tools:?}"
    );
    assert!(tools.contains(&"mcp__docs__echo".to_string()));
    for name in EXPECTED_TOOLS {
        assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
    }
}

// CVL-FR-TJZI, MCT-FR-YSVS, CVL-FR-FWZZ: a call of an MCP tool goes to the
// server, its result goes into the exchange, the loop continues, and the turn
// reports the server and the tool as the key input.
#[test]
fn a_call_reaches_the_server_and_the_loop_continues() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("mcp__docs__echo", serde_json::json!({ "text": "hello" }))),
        Ok(ScriptedReply::answer("The server said hello.")),
    ]);
    h.mount();
    connect_docs(&h);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results, vec!["echo: hello".to_string()]);
    let call = terminal.tool_calls.iter().find(|c| c.tool == "mcp__docs__echo").expect("the call");
    assert_eq!(call.state, ToolCallState::Done);
    assert_eq!(call.key_input.as_deref(), Some("Server docs: echo"));
}

// CVL-FR-TJZI: a project with no ready server is offered no MCP tool.
#[test]
fn a_project_without_servers_gets_no_mcp_tool() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.mount();
    h.create_agent("arch", "");
    run_one(&h, "arch");
    let tools = &h.seam.requests()[0].0.tools;
    assert!(tools.iter().all(|t| !t.name.as_str().starts_with("mcp__")));
}

// CVL-FR-TJZI: a project whose server is not ready is offered no MCP tool.
#[test]
fn a_server_that_is_not_ready_offers_no_tool() {
    let h = Harness::new(vec![Ok("ok".into())]);
    h.mount();
    let (manager, _connector, _secrets, _observer) = crate::mcp::tests::test_manager();
    h.app.manage(manager.clone());
    let key = h.app.state::<crate::project::ProjectState>().slot_key();
    let facts = MachineFacts {
        trusted: true,
        secrets_present: false,
        signed_in: true,
        vault_unavailable: false,
    };
    let record = crate::mcp::tests::http_record("docs", McpAuth::Bearer);
    let input = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: "docs".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(
        crate::mcp::project_context(&key, h.root.path().to_path_buf()),
        vec![input],
        None,
    );
    assert_eq!(manager.snapshot("docs").map(|(s, _)| s), Some(McpServerStatus::NeedsSecret));
    h.create_agent("arch", "");
    run_one(&h, "arch");
    let tools = &h.seam.requests()[0].0.tools;
    assert!(tools.iter().all(|t| !t.name.as_str().starts_with("mcp__")));
}
