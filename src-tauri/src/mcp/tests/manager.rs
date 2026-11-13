//! The live connections (MCS-FR-RXZG to MCS-FR-QTSW, MCS-FR-ENII, MCS-FR-LTTM).

use super::*;
use crate::mcp::config::ListedRecord;
use crate::mcp::manager::{McpCallError, ServerInput};
use crate::mcp::status::MachineFacts;

fn ok_facts() -> MachineFacts {
    MachineFacts { trusted: true, secrets_present: true, signed_in: true, vault_unavailable: false }
}

fn input(record: McpServer) -> ServerInput {
    ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: record.id.clone(), record: Ok(record) },
        facts: ok_facts(),
    }
}

fn project() -> crate::mcp::manager::ProjectContext {
    crate::mcp::project_context("/repo", std::env::temp_dir())
}

async fn ready(manager: &McpManager, id: &str) {
    wait_until(|| matches!(status_of(manager, id), Some(McpServerStatus::Ready { .. }))).await;
}

// MCS-FR-UXPP, MCS-FR-BYBA, MCS-FR-RXZG, MCS-FR-NOKX, MCS-FR-KEEF: an allowed
// server connects, lists its tools, and becomes ready; the statuses are
// emitted in order and the connection is logged.
#[tokio::test(flavor = "multi_thread")]
async fn an_allowed_server_connects_and_becomes_ready() {
    let (manager, connector, _secrets, observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    assert_eq!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { tool_count: 5 }));
    let statuses: Vec<_> =
        observer.statuses.lock().unwrap().iter().map(|(_, s)| s.code()).collect();
    assert_eq!(statuses, ["connecting", "ready"]);
    assert_eq!(connector.attempt_count(), 1);
    let logs = observer.log_text();
    assert!(logs.contains("mcp connection opened"));
    assert!(logs.contains("\"transport\":\"http\""));
    assert!(!logs.contains("mcp.example.com"), "no URL is logged");
}

// MCS-FR-UXPP, MCS-FR-RXZG: a server the gate refuses gets that status and no
// connection; an invalid record is `invalid`.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_server_takes_the_gate_status_and_does_not_connect() {
    let (manager, connector, _secrets, _observer) = test_manager();
    let mut disabled = http_record("off", McpAuth::None);
    disabled.enabled = false;
    let untrusted = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: "local".into(), record: Ok(stdio_record("local")) },
        facts: MachineFacts { trusted: false, ..ok_facts() },
    };
    let invalid = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord {
            id: "bad".into(),
            record: Err(crate::mcp::config::McpConfigError::InvalidUrl),
        },
        facts: MachineFacts::default(),
    };
    manager.reconcile(project(), vec![input(disabled), untrusted, invalid], None);
    assert_eq!(status_of(&manager, "off"), Some(McpServerStatus::Disabled));
    assert_eq!(status_of(&manager, "local"), Some(McpServerStatus::Untrusted));
    assert_eq!(status_of(&manager, "bad"), Some(McpServerStatus::Invalid));
    assert_eq!(connector.attempt_count(), 0);
}

// MCS-FR-RYIK, MCS-FR-YEXD: a reconcile with the same records keeps the session; a changed
// record, and a restart, open a new one; a removed record stops.
#[tokio::test(flavor = "multi_thread")]
async fn reconcile_restarts_only_what_changed() {
    let (manager, connector, _secrets, _observer) = test_manager();
    let docs = http_record("docs", McpAuth::None);
    let other = http_record("other", McpAuth::None);
    manager.reconcile(project(), vec![input(docs.clone()), input(other.clone())], None);
    ready(&manager, "docs").await;
    ready(&manager, "other").await;
    assert_eq!(connector.attempt_count(), 2);

    manager.reconcile(project(), vec![input(docs.clone()), input(other.clone())], None);
    assert_eq!(connector.attempt_count(), 2, "nothing changed, nothing restarted");

    // A new name and a new tool filter keep the session.
    let mut renamed = docs.clone();
    renamed.name = "Docs again".to_string();
    renamed.disabled_tools = vec!["fail".to_string()];
    manager.reconcile(project(), vec![input(renamed.clone()), input(other.clone())], None);
    assert_eq!(connector.attempt_count(), 2);
    assert_eq!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { tool_count: 4 }));
    assert_eq!(manager.mcp_tools_now("/repo")[0].server_name, "Docs again");

    // A change the server sees restarts it.
    if let McpTransport::Http(http) = &mut renamed.transport {
        http.headers.insert("X-New".to_string(), "1".to_string());
    }
    manager.reconcile(project(), vec![input(renamed.clone()), input(other.clone())], None);
    wait_until(|| connector.attempt_count() == 3).await;
    ready(&manager, "docs").await;

    manager.reconcile(project(), vec![input(renamed.clone()), input(other.clone())], Some("other"));
    wait_until(|| connector.attempt_count() == 4).await;
    ready(&manager, "other").await;

    manager.reconcile(project(), vec![input(renamed)], None);
    assert_eq!(status_of(&manager, "other"), None, "a removed record is gone");
    assert!(manager.mcp_tools_now("/repo").iter().all(|t| t.server_id == "docs"));
}

// MCS-FR-ENII, MCS-FR-HBYA: the tools of ready servers, without the disabled
// ones, and none for another project.
#[tokio::test(flavor = "multi_thread")]
async fn tools_now_hold_the_ready_tools_without_the_disabled_ones() {
    let (manager, _connector, _secrets, _observer) = test_manager();
    let mut docs = http_record("docs", McpAuth::None);
    docs.disabled_tools = vec!["fail".to_string(), "not-a-tool".to_string()];
    manager.reconcile(project(), vec![input(docs)], None);
    ready(&manager, "docs").await;
    assert_eq!(status_of(&manager, "docs"), Some(McpServerStatus::Ready { tool_count: 4 }));
    let names: Vec<String> =
        manager.mcp_tools_now("/repo").iter().map(|t| t.tool.name.to_string()).collect();
    assert_eq!(names, ["echo", "bad_args", "hang", "picture"]);
    assert!(manager.mcp_tools_now("/elsewhere").is_empty());
    let (_, listed) = manager.snapshot("docs").unwrap();
    assert!(
        listed.iter().any(|t| t.name == "fail" && t.disabled),
        "the list shows a disabled tool as disabled"
    );
}

// MCS-FR-XDZK: a list-changed notification replaces the tools held.
#[tokio::test(flavor = "multi_thread")]
async fn a_list_changed_notification_relists_the_tools() {
    let (manager, connector, _secrets, _observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    connector.server.tools.lock().unwrap().push(super::tool("added", "New."));
    connector.notify_tools_changed().await;
    wait_until(|| status_of(&manager, "docs") == Some(McpServerStatus::Ready { tool_count: 6 }))
        .await;
    assert!(manager.mcp_tools_now("/repo").iter().any(|t| t.tool.name == "added"));
}

// MCS-FR-HXJF: a dropped session reconnects; after five failed attempts the
// server is failed with the last class.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_session_reconnects_then_fails_after_five_attempts() {
    let (manager, connector, _secrets, observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;

    connector.drop_sessions().await;
    wait_until(|| connector.attempt_count() == 2).await;
    ready(&manager, "docs").await;
    assert!(observer.log_text().contains("mcp connection dropped"));

    let unreachable = Plan::Fail(ConnectError::Failure(McpFailure::Unreachable));
    connector.plan(&[unreachable; 5]);
    connector.drop_sessions().await;
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Failed { .. })))
        .await;
    assert_eq!(
        status_of(&manager, "docs"),
        Some(McpServerStatus::Failed { failure: McpFailure::Unreachable })
    );
    assert_eq!(connector.attempt_count(), 7, "one open, one reconnect, five failed attempts");
    assert!(manager.mcp_tools_now("/repo").is_empty());
}

// MCS-FR-RXZG, MCS-FR-JNXV: a first attempt that fails sets its class at once,
// and a server that wants a sign-in or a secret shows that.
#[tokio::test(flavor = "multi_thread")]
async fn a_first_failure_sets_its_status_without_retries() {
    let (manager, connector, _secrets, _observer) = test_manager();
    connector.plan(&[
        Plan::Fail(ConnectError::Failure(McpFailure::Rejected)),
        Plan::Fail(ConnectError::NeedsSignIn),
        Plan::Fail(ConnectError::NeedsSecret),
    ]);
    manager.reconcile(
        project(),
        vec![
            input(http_record("a", McpAuth::Bearer)),
            input(http_record("b", McpAuth::Oauth2)),
            input(http_record("c", McpAuth::None)),
        ],
        None,
    );
    wait_until(|| {
        ["a", "b", "c"].iter().all(|id| {
            !matches!(
                status_of(&manager, id),
                Some(McpServerStatus::Connecting) | Some(McpServerStatus::Disabled)
            )
        })
    })
    .await;
    let mut statuses: Vec<_> =
        ["a", "b", "c"].iter().map(|id| status_of(&manager, id).unwrap().code()).collect();
    statuses.sort();
    assert_eq!(statuses, ["failed", "needs_secret", "needs_sign_in"]);
    assert_eq!(connector.attempt_count(), 3);
}

// MCS-FR-LTTM, MCT-FR-OSXG: a call reaches the server's session; a hanging call
// ends at its deadline; a server that is not ready gives `not_connected`.
#[tokio::test(flavor = "multi_thread")]
async fn calls_reach_the_session_and_end_at_their_deadline() {
    let (manager, _connector, _secrets, _observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    let mut args = serde_json::Map::new();
    args.insert("text".into(), serde_json::json!("hi"));
    let result =
        manager.call_mcp_tool("docs", "echo", args, Duration::from_secs(10)).await.unwrap();
    assert_eq!(crate::mcp::tool::result_text(&result), "echo: hi");
    let error = manager
        .call_mcp_tool("docs", "bad_args", Default::default(), Duration::from_secs(10))
        .await;
    assert!(matches!(error, Err(McpCallError::Rpc { code: -32602, .. })));
    let late =
        manager.call_mcp_tool("docs", "hang", Default::default(), Duration::from_millis(50)).await;
    assert_eq!(late, Err(McpCallError::TimedOut));
    let missing =
        manager.call_mcp_tool("nobody", "echo", Default::default(), Duration::from_secs(1)).await;
    assert_eq!(missing, Err(McpCallError::NotConnected));
}

// MCS-FR-LTTM: a call made on a runtime of its own, as the turn's tool dispatch
// makes it, reaches the session that runs on the application runtime.
#[test]
fn a_call_from_another_runtime_reaches_the_session() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let (manager, _connector, _secrets, _observer) = test_manager();
    runtime.block_on(async {
        manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
        ready(&manager, "docs").await;
    });
    let call_manager = manager.clone();
    let result = std::thread::spawn(move || {
        let own = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        own.block_on(call_manager.call_mcp_tool(
            "docs",
            "echo",
            Default::default(),
            Duration::from_secs(10),
        ))
    })
    .join()
    .unwrap();
    assert_eq!(crate::mcp::tool::result_text(&result.unwrap()), "echo: ");
}

// MCS-FR-QTSW: closing the project ends every session and forgets the tools.
#[tokio::test(flavor = "multi_thread")]
async fn closing_the_project_ends_every_session() {
    let (manager, _connector, _secrets, observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    manager.close_project();
    assert!(manager.mcp_tools_now("/repo").is_empty());
    assert_eq!(status_of(&manager, "docs"), None);
    wait_until(|| observer.log_text().contains("mcp connection closed")).await;
    manager.shutdown().await;
}

// MCS-FR-RYIK: a change of project stops every server of the earlier one.
#[tokio::test(flavor = "multi_thread")]
async fn another_project_replaces_every_server() {
    let (manager, _connector, _secrets, _observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    let other = crate::mcp::project_context("/other", std::env::temp_dir());
    manager.reconcile(other, vec![input(http_record("tracker", McpAuth::None))], None);
    assert_eq!(status_of(&manager, "docs"), None);
    ready(&manager, "tracker").await;
    assert!(manager.mcp_tools_now("/repo").is_empty());
    assert!(!manager.mcp_tools_now("/other").is_empty());
}

// MCS-FR-YWVC, MCS-FR-CMWV: a sign-in sets `signing_in` and refuses a second
// one; a cancel restores the status before it; a failed one is `oauth_failed`.
#[tokio::test(flavor = "multi_thread")]
async fn sign_in_states() {
    let (manager, _connector, _secrets, _observer) = test_manager();
    let facts = MachineFacts { signed_in: false, ..ok_facts() };
    let record = http_record("t", McpAuth::Oauth2);
    let input = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord { id: "t".into(), record: Ok(record) },
        facts,
    };
    manager.reconcile(project(), vec![input.clone()], None);
    assert_eq!(status_of(&manager, "t"), Some(McpServerStatus::NeedsSignIn));

    let mut cancelled = manager.begin_sign_in("t").unwrap();
    assert_eq!(status_of(&manager, "t"), Some(McpServerStatus::SigningIn));
    assert!(manager.begin_sign_in("t").is_err(), "one sign-in at a time");
    manager.reconcile(project(), vec![input.clone()], None);
    assert_eq!(
        status_of(&manager, "t"),
        Some(McpServerStatus::SigningIn),
        "a reconcile keeps the sign-in"
    );
    manager.cancel_sign_in("t");
    assert!(*cancelled.borrow_and_update());
    assert_eq!(status_of(&manager, "t"), Some(McpServerStatus::NeedsSignIn));
    manager.finish_sign_in("t", Ok(()));
    assert_eq!(
        status_of(&manager, "t"),
        Some(McpServerStatus::NeedsSignIn),
        "a cancelled sign-in finishes nothing"
    );

    manager.begin_sign_in("t").unwrap();
    manager.finish_sign_in("t", Err(McpFailure::OauthFailed));
    assert_eq!(
        status_of(&manager, "t"),
        Some(McpServerStatus::Failed { failure: McpFailure::OauthFailed })
    );
}

// MCS-FR-HXJF: a dropped session is `connecting` at once, before the first
// wait ends.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_session_is_connecting_during_the_wait() {
    let connector = DuplexConnector::default();
    let timing = Timing {
        connect_limit: Duration::from_secs(10),
        reconnect_waits: vec![Duration::from_secs(3600); 5],
        stable_after: Duration::ZERO,
    };
    let manager =
        McpManager::new(Arc::new(MemoryMcpSecrets::default()), Arc::new(connector.clone()), timing);
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    connector.drop_sessions().await;
    wait_until(|| status_of(&manager, "docs") == Some(McpServerStatus::Connecting)).await;
    assert!(manager.mcp_tools_now("/repo").is_empty());
    manager.close_project();
}

// MCS-FR-RYIK: reconciles from several threads at once start a server one time.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_reconciles_start_a_server_once() {
    let (manager, connector, _secrets, _observer) = test_manager();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let manager = manager.clone();
            std::thread::spawn(move || {
                manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None)
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    ready(&manager, "docs").await;
    assert_eq!(connector.attempt_count(), 1);
}

// MCS-FR-HBOD: a later record with the same id does not change the status of
// the first one.
#[tokio::test(flavor = "multi_thread")]
async fn a_duplicate_id_leaves_the_first_record_alone() {
    let (manager, _connector, _secrets, observer) = test_manager();
    let duplicate = ServerInput {
        owner: "/repo".into(),
        listed: ListedRecord {
            id: "docs".into(),
            record: Err(crate::mcp::config::McpConfigError::DuplicateId),
        },
        facts: MachineFacts::default(),
    };
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None)), duplicate], None);
    ready(&manager, "docs").await;
    assert!(observer.statuses.lock().unwrap().iter().all(|(_, s)| *s != McpServerStatus::Invalid));
}

// MCS-FR-BYBA: an attempt that does not complete within the limit is
// `timed_out`, and the failure is logged with its class.
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_over_the_limit_times_out() {
    let connector = DuplexConnector::default();
    connector.plan(&[Plan::Hang]);
    let timing = Timing {
        connect_limit: Duration::from_millis(50),
        reconnect_waits: vec![Duration::from_millis(5); 5],
        stable_after: Duration::ZERO,
    };
    let manager =
        McpManager::new(Arc::new(MemoryMcpSecrets::default()), Arc::new(connector.clone()), timing);
    let observer = Arc::new(RecordingObserver::default());
    manager.install_observer(observer.clone());
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Failed { .. })))
        .await;
    assert_eq!(
        status_of(&manager, "docs"),
        Some(McpServerStatus::Failed { failure: McpFailure::TimedOut })
    );
    let logs = observer.logs.lock().unwrap();
    assert!(logs.iter().any(|(l, m, f)| *l == LogLevel::Warn
        && m == "mcp connection failed"
        && f["failure"] == "timed_out"));
}

// MCS-FR-LTTM: a call whose deadline passes is cancelled at the server.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_call_is_cancelled_at_the_server() {
    let (manager, connector, _secrets, _observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    let late =
        manager.call_mcp_tool("docs", "hang", Default::default(), Duration::from_millis(50)).await;
    assert_eq!(late, Err(McpCallError::TimedOut));
    let flag = Arc::clone(&connector.server.hang_cancelled);
    wait_until(|| flag.load(std::sync::atomic::Ordering::SeqCst)).await;
}

// MCT-FR-RDUX: the definitions of a turn stay as built when the server's tools
// change; a call of a tool the server removed gets the server's answer.
#[tokio::test(flavor = "multi_thread")]
async fn the_definitions_of_a_turn_do_not_change() {
    let (manager, connector, _secrets, _observer) = test_manager();
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    ready(&manager, "docs").await;
    let tools = crate::mcp::tool::conversation_tools(&manager, "/repo", |_, _, _| {});
    let before: Vec<_> = tools.iter().map(|t| t.definition()).collect();
    connector.server.tools.lock().unwrap().retain(|t| t.name != "echo");
    connector.notify_tools_changed().await;
    wait_until(|| status_of(&manager, "docs") == Some(McpServerStatus::Ready { tool_count: 4 }))
        .await;
    let after: Vec<_> = tools.iter().map(|t| t.definition()).collect();
    assert_eq!(before, after);
    assert_eq!(
        crate::mcp::tool::key_input_for("mcp__docs__echo").as_deref(),
        Some("Server docs: echo")
    );
    let echo = tools.iter().find(|t| t.name().as_str() == "mcp__docs__echo").unwrap();
    assert!(
        echo.execute(serde_json::json!({})).await.is_ok(),
        "the test server still answers the name"
    );
}

// MCS-FR-EMPU: sessions that drop within the stable time count as failed
// attempts, so a server that opens and drops at once ends failed.
#[tokio::test(flavor = "multi_thread")]
async fn a_flapping_server_ends_failed() {
    let connector = DuplexConnector::default();
    let timing = Timing {
        connect_limit: Duration::from_secs(10),
        reconnect_waits: vec![Duration::from_millis(5); 5],
        stable_after: Duration::from_secs(3600),
    };
    let manager =
        McpManager::new(Arc::new(MemoryMcpSecrets::default()), Arc::new(connector.clone()), timing);
    manager.reconcile(project(), vec![input(http_record("docs", McpAuth::None))], None);
    for drop_number in 1..=5 {
        wait_until(|| connector.attempt_count() == drop_number).await;
        ready(&manager, "docs").await;
        wait_until(|| connector.sessions.lock().unwrap().len() == 1).await;
        connector.drop_sessions().await;
    }
    wait_until(|| matches!(status_of(&manager, "docs"), Some(McpServerStatus::Failed { .. })))
        .await;
    assert_eq!(
        status_of(&manager, "docs"),
        Some(McpServerStatus::Failed { failure: McpFailure::Unreachable })
    );
    assert_eq!(connector.attempt_count(), 5, "no session after the fifth drop");
}

/// A server that answers the handshake, then lists a tool whose schema is a
/// string, as no MCP server may.
struct NonObjectSchemaConnector;

#[async_trait::async_trait]
impl Connector for NonObjectSchemaConnector {
    async fn connect(
        &self,
        request: crate::mcp::connect::ConnectRequest,
    ) -> Result<crate::mcp::connect::McpSession, ConnectError> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let (read, mut write) = tokio::io::split(server_io);
            let mut lines = tokio::io::BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let message: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                let result = match message["method"].as_str() {
                    Some("initialize") => serde_json::json!({
                        "protocolVersion": message["params"]["protocolVersion"],
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "raw", "version": "1" },
                    }),
                    Some("tools/list") => serde_json::json!({
                        "tools": [{ "name": "odd", "inputSchema": "not an object" }],
                    }),
                    _ => continue,
                };
                let reply =
                    serde_json::json!({ "jsonrpc": "2.0", "id": message["id"], "result": result });
                let _ = write.write_all(format!("{reply}\n").as_bytes()).await;
            }
        });
        rmcp::serve_client(request.client, client_io)
            .await
            .map_err(|_| ConnectError::Failure(McpFailure::ProtocolError))
    }
}

// MCT-FR-GNUX: a tool list in which a schema is not an object does not
// decode, so the server is failed with `protocol_error` and offers no tool.
#[tokio::test(flavor = "multi_thread")]
async fn a_non_object_schema_fails_the_server() {
    let manager = McpManager::new(
        Arc::new(MemoryMcpSecrets::default()),
        Arc::new(NonObjectSchemaConnector),
        fast_timing(),
    );
    manager.reconcile(project(), vec![input(http_record("raw", McpAuth::None))], None);
    wait_until(|| matches!(status_of(&manager, "raw"), Some(McpServerStatus::Failed { .. }))).await;
    assert_eq!(
        status_of(&manager, "raw"),
        Some(McpServerStatus::Failed { failure: McpFailure::ProtocolError })
    );
    assert!(manager.mcp_tools_now("/repo").is_empty());
}
