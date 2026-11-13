//! The stdio transport against a real process: a small MCP server written in
//! Python, run through the production connector.

use std::path::{Path, PathBuf};

use super::*;
use crate::mcp::connect::{child_path, resolve_command, RealConnector};
use crate::mcp::manager::ServerInput;
use crate::mcp::operations::{run_test, TestPlan};
use crate::mcp::secrets::McpSecrets;
use crate::mcp::status::{MachineFacts, McpTestResult};
use crate::openspec::cli::{CliEnvironment, CliProbe};

/// MCP over standard input and output, one JSON message on each line. The
/// tool `env` returns a variable, the working directory, and `PATH`. With
/// `spawn`, the server starts a child process and writes its PID to a file.
const SERVER: &str = r#"
import json, os, subprocess, sys
if len(sys.argv) > 1:
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)"])
    open(sys.argv[1], "w").write(str(child.pid))
sys.stderr.write("server started\n")
sys.stderr.flush()
for line in sys.stdin:
    msg = json.loads(line)
    method, ident = msg.get("method"), msg.get("id")
    if method == "initialize":
        result = {"protocolVersion": msg["params"]["protocolVersion"], "capabilities": {"tools": {}},
                  "serverInfo": {"name": "py", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "env", "description": "Read a variable.", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        name = msg["params"].get("arguments", {}).get("name", "")
        text = os.environ.get(name, "") + "|" + os.getcwd() + "|" + os.environ.get("PATH", "")
        result = {"content": [{"type": "text", "text": text}]}
    elif ident is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": ident, "error": {"code": -32601, "message": "no"}}), flush=True)
        continue
    else:
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": ident, "result": result}), flush=True)
"#;

fn python() -> String {
    if cfg!(windows) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

fn manager_with(secrets: Arc<MemoryMcpSecrets>) -> (McpManager, Arc<RecordingObserver>) {
    let manager = McpManager::new(secrets, Arc::new(RealConnector), fast_timing());
    let observer = Arc::new(RecordingObserver::default());
    manager.install_observer(observer.clone());
    (manager, observer)
}

fn record(id: &str, args: Vec<String>) -> McpServer {
    McpServer {
        id: id.to_string(),
        name: format!("Python {id}"),
        enabled: true,
        disabled_tools: Vec::new(),
        transport: McpTransport::Stdio(StdioTransport {
            command: python(),
            args,
            ..Default::default()
        }),
    }
}

fn write_server(dir: &Path) -> PathBuf {
    let path = dir.join("server.py");
    std::fs::write(&path, SERVER).unwrap();
    path
}

fn input(record: McpServer) -> ServerInput {
    let facts = MachineFacts { trusted: true, secrets_present: true, ..Default::default() };
    ServerInput {
        owner: "/repo".into(),
        listed: crate::mcp::config::ListedRecord { id: record.id.clone(), record: Ok(record) },
        facts,
    }
}

// MCS-FR-APCF, MCS-FR-KAYL, MCS-FR-KLQS, MCS-FR-ROVQ, MCT-FR-OSXG: a bare
// command resolves; the process runs in its working directory with the plain
// and the secret variables and the folder of the executable first on `PATH`.
#[tokio::test(flavor = "multi_thread")]
async fn a_stdio_server_runs_with_its_environment() {
    let dir = tempfile::TempDir::new().unwrap();
    let script = write_server(dir.path());
    std::fs::create_dir_all(dir.path().join("tools")).unwrap();
    let mut server = record("py", vec![script.display().to_string()]);
    if let McpTransport::Stdio(stdio) = &mut server.transport {
        stdio.cwd = Some("tools".to_string());
        stdio.env.insert("PLAIN".to_string(), "plain-value".to_string());
        stdio.secret_env.push("SECRET".to_string());
    }
    let secrets = Arc::new(MemoryMcpSecrets::default());
    secrets.set(&"/repo".into(), "py", "env:SECRET", "secret-value").unwrap();
    let (manager, observer) = manager_with(secrets);
    manager.reconcile(
        crate::mcp::project_context("/repo", dir.path().to_path_buf()),
        vec![input(server)],
        None,
    );
    wait_until(|| {
        matches!(
            status_of(&manager, "py"),
            Some(McpServerStatus::Ready { .. }) | Some(McpServerStatus::Failed { .. })
        )
    })
    .await;
    assert_eq!(status_of(&manager, "py"), Some(McpServerStatus::Ready { tool_count: 1 }));

    let mut args = serde_json::Map::new();
    args.insert("name".into(), serde_json::json!("SECRET"));
    let result = manager.call_mcp_tool("py", "env", args, Duration::from_secs(20)).await.unwrap();
    let text = crate::mcp::tool::result_text(&result);
    let parts: Vec<&str> = text.split('|').collect();
    assert_eq!(parts[0], "secret-value");
    let cwd = std::fs::canonicalize(parts[1]).unwrap();
    assert_eq!(cwd, std::fs::canonicalize(dir.path().join("tools")).unwrap());
    let mut args = serde_json::Map::new();
    args.insert("name".into(), serde_json::json!("PLAIN"));
    let result = manager.call_mcp_tool("py", "env", args, Duration::from_secs(20)).await.unwrap();
    assert!(crate::mcp::tool::result_text(&result).starts_with("plain-value|"));
    let first_path = parts[2].split(if cfg!(windows) { ';' } else { ':' }).next().unwrap();
    let python_dir = resolve_command(
        &python(),
        dir.path(),
        &CliEnvironment::from_process(),
        &crate::openspec::cli::RealCliProbe,
    )
    .unwrap()
    .parent()
    .unwrap()
    .to_path_buf();
    assert_eq!(Path::new(first_path), python_dir);
    assert!(manager.stderr_tail("py").lines().iter().any(|l| l == "server started"));
    assert!(!observer.log_text().contains("secret-value"), "no variable value is logged");
    assert!(!observer.log_text().contains("server started"), "no standard error line is logged");
    manager.shutdown().await;
}

// MCS-FR-APCF, MCS-FR-JNXV: a command that does not resolve is
// `command_not_found`, and a process that exits is `process_exited` with its
// last lines of standard error in the test result (MCS-FR-ERQP).
#[tokio::test(flavor = "multi_thread")]
async fn failures_of_a_process_are_classified() {
    let dir = tempfile::TempDir::new().unwrap();
    let (manager, _observer) = manager_with(Arc::new(MemoryMcpSecrets::default()));
    let mut missing = record("missing", Vec::new());
    if let McpTransport::Stdio(stdio) = &mut missing.transport {
        stdio.command = "synthesis-no-such-command-4711".to_string();
    }
    let plan = TestPlan::Run {
        owner: "/repo".into(),
        root: dir.path().to_path_buf(),
        record: Box::new(missing),
    };
    let McpTestResult::Refused { failure, .. } = run_test(&manager, plan).await else {
        panic!("refused")
    };
    assert_eq!(failure, "command_not_found");

    let lines: Vec<String> = (0..25).map(|i| format!("sys.stderr.write('line {i}\\n')")).collect();
    let crash = record(
        "crash",
        vec!["-c".into(), format!("import sys\n{}\nsys.exit(3)", lines.join("\n"))],
    );
    let plan = TestPlan::Run {
        owner: "/repo".into(),
        root: dir.path().to_path_buf(),
        record: Box::new(crash),
    };
    let McpTestResult::Refused { failure, stderr_tail, .. } = run_test(&manager, plan).await else {
        panic!("refused")
    };
    assert_eq!(failure, "process_exited");
    assert_eq!(stderr_tail.len(), 20, "the last 20 lines");
    assert_eq!(stderr_tail.last().map(String::as_str), Some("line 24"));
    assert_eq!(stderr_tail.first().map(String::as_str), Some("line 5"));
}

// MCS-FR-BLBG, MCS-FR-QTSW: ending the session stops the server and the
// process it started.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn ending_a_session_stops_the_whole_process_group() {
    let dir = tempfile::TempDir::new().unwrap();
    let script = write_server(dir.path());
    let pid_file = dir.path().join("child.pid");
    let server = record("py", vec![script.display().to_string(), pid_file.display().to_string()]);
    let (manager, _observer) = manager_with(Arc::new(MemoryMcpSecrets::default()));
    manager.reconcile(
        crate::mcp::project_context("/repo", dir.path().to_path_buf()),
        vec![input(server)],
        None,
    );
    wait_until(|| matches!(status_of(&manager, "py"), Some(McpServerStatus::Ready { .. }))).await;
    let pid: String = std::fs::read_to_string(&pid_file).unwrap();
    let alive = |pid: &str| {
        std::process::Command::new("kill").args(["-0", pid]).status().is_ok_and(|s| s.success())
    };
    assert!(alive(&pid), "the child runs while the session is open");
    manager.shutdown().await;
    wait_until(|| !alive(&pid)).await;
}

/// A machine with fixed executables, for the resolution rules.
struct FakeProbe(Vec<PathBuf>);

impl CliProbe for FakeProbe {
    fn is_executable(&self, path: &Path) -> bool {
        self.0.iter().any(|p| p == path)
    }
    fn modified(&self, _path: &Path) -> Option<std::time::SystemTime> {
        None
    }
    fn subfolders(&self, _dir: &Path) -> Vec<String> {
        Vec::new()
    }
}

// MCS-FR-APCF, MCS-FR-KAYL: a bare name is searched in the directories in
// order; a path resolves against the working directory; PATH starts with the
// folder of the executable.
#[test]
fn commands_resolve_as_the_openspec_cli_does() {
    if cfg!(windows) {
        return;
    }
    let env = CliEnvironment { path_var: Some("/first:/second".to_string()), home: None };
    let probe = FakeProbe(vec![PathBuf::from("/second/npx"), PathBuf::from("/work/bin/run")]);
    assert_eq!(
        resolve_command("npx", Path::new("/work"), &env, &probe),
        Some(PathBuf::from("/second/npx"))
    );
    assert_eq!(
        resolve_command("bin/run", Path::new("/work"), &env, &probe),
        Some(PathBuf::from("/work/bin/run"))
    );
    assert_eq!(resolve_command("missing", Path::new("/work"), &env, &probe), None);
    let path = child_path(Path::new("/second/npx"), &env, &probe);
    assert!(path.to_string_lossy().starts_with("/second:/first"));
}
