//! The operations behind the commands, on a temp worktree and a temp
//! user-global store.

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::mcp::config::command_fingerprint;
use crate::mcp::operations::{run_test, Context, TestPlan};
use crate::mcp::secrets::McpSecrets;
use crate::mcp::status::McpTestResult;

struct Fixture {
    dir: tempfile::TempDir,
    store: GlobalSettingsStore,
    manager: McpManager,
    connector: DuplexConnector,
    secrets: Arc<MemoryMcpSecrets>,
    observer: Arc<RecordingObserver>,
    _global: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let global = tempfile::TempDir::new().unwrap();
        let store = GlobalSettingsStore::with_path(global.path().join("synthesis.toml"));
        let (manager, connector, secrets, observer) = test_manager();
        Self { dir, store, manager, connector, secrets, observer, _global: global }
    }

    fn context(&self) -> Context<'_> {
        Context {
            root: crate::fs::RootFs::for_root(self.dir.path()),
            root_path: self.dir.path().to_path_buf(),
            key: "/repo".to_string(),
            store: &self.store,
            manager: &self.manager,
        }
    }

    fn file(&self) -> String {
        std::fs::read_to_string(self.dir.path().join(".synthesis/project.toml")).unwrap_or_default()
    }
}

fn with_secret_env(mut server: McpServer, names: &[&str]) -> McpServer {
    if let McpTransport::Stdio(stdio) = &mut server.transport {
        stdio.secret_env = names.iter().map(|n| n.to_string()).collect();
    }
    server
}

// MCS-FR-XCOS, MCS-FR-EUCA, MCS-FR-PXRJ, MCS-FR-FIEE: a saved stdio record is
// trusted on this machine, waits for its secret, then connects; the list shows
// the slot's presence and never its value.
#[tokio::test(flavor = "multi_thread")]
async fn a_saved_stdio_record_is_trusted_and_waits_for_its_secret() {
    let f = Fixture::new();
    let c = f.context();
    let server = with_secret_env(stdio_record("docs"), &["DOCS_KEY"]);
    let entry = c.save(server.clone()).unwrap();
    assert_eq!(entry.status, McpServerStatus::NeedsSecret);
    assert_eq!(entry.secrets.len(), 1);
    assert!(!entry.secrets[0].present);
    let McpTransport::Stdio(stdio) = &server.transport else { unreachable!() };
    assert_eq!(
        f.store.load_mcp_trust("/repo").unwrap().get("docs"),
        Some(&command_fingerprint(stdio)),
        "the author's own save trusts the command"
    );

    let entry = c.set_secret("docs", "env:DOCS_KEY", "s3cret-value").unwrap();
    assert!(entry.secrets[0].present);
    wait_until(|| matches!(status_of(&f.manager, "docs"), Some(McpServerStatus::Ready { .. })))
        .await;
    let listed = c.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].tools.len(), 5);
    let wire = serde_json::to_string(&listed).unwrap();
    assert!(!wire.contains("s3cret-value"), "no secret value is listed");
    assert!(!f.observer.log_text().contains("s3cret-value"), "no secret value is logged");
}

// MCS-FR-HQJQ, MCS-FR-EVRS, MCS-FR-KWCN: a command changed in the file is
// untrusted until the author trusts it; trust refuses an http record.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_command_needs_trust_again() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("docs")).unwrap();
    wait_until(|| matches!(status_of(&f.manager, "docs"), Some(McpServerStatus::Ready { .. })))
        .await;

    let text = f.file().replace("command = \"npx\"", "command = \"curl\"");
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    let listed = c.list().unwrap();
    assert_eq!(listed[0].status, McpServerStatus::Untrusted);
    let attempts = f.connector.attempt_count();
    // MCS-FR-EVRS: the record the dialog showed must be the record in the file.
    let shown_before = stdio_record("docs");
    assert_eq!(c.trust("docs", &shown_before).unwrap_err(), "record_changed");
    assert_eq!(f.connector.attempt_count(), attempts, "a refused trust starts nothing");
    let shown = c.record("docs").unwrap();
    c.trust("docs", &shown).unwrap();
    wait_until(|| f.connector.attempt_count() == attempts + 1).await;
    wait_until(|| matches!(status_of(&f.manager, "docs"), Some(McpServerStatus::Ready { .. })))
        .await;

    c.save(http_record("tracker", McpAuth::None)).unwrap();
    let tracker = c.record("tracker").unwrap();
    assert_eq!(c.trust("tracker", &tracker).unwrap_err(), "no_credentials");
    assert_eq!(c.trust("nobody", &tracker).unwrap_err(), "not_found");
}

// MCS-FR-EUCA, MCS-FR-HQJQ: a save that keeps the command of an untrusted
// record, such as the Enabled switch, does not trust it; a save that changes
// the command does.
#[tokio::test(flavor = "multi_thread")]
async fn a_save_that_keeps_an_untrusted_command_keeps_it_untrusted() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("docs")).unwrap();
    let text = f.file().replace("command = \"npx\"", "command = \"curl\"");
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    assert_eq!(c.list().unwrap()[0].status, McpServerStatus::Untrusted);
    let attempts = f.connector.attempt_count();

    let mut pulled = c.record("docs").unwrap();
    pulled.enabled = false;
    c.save(pulled.clone()).unwrap();
    pulled.enabled = true;
    pulled.name = "Renamed".to_string();
    let entry = c.save(pulled.clone()).unwrap();
    assert_eq!(entry.status, McpServerStatus::Untrusted);
    assert_eq!(f.connector.attempt_count(), attempts, "nothing ran");

    if let McpTransport::Stdio(stdio) = &mut pulled.transport {
        stdio.command = "uvx".to_string();
    }
    c.save(pulled).unwrap();
    wait_until(|| f.connector.attempt_count() == attempts + 1).await;
}

// MCS-FR-KXYO, MCS-FR-PPPL, MCS-FR-RISC, MCS-FR-HQJQ, MCS-FR-ERQP: an http record
// with a credential is trusted for its URL; a URL changed in the file is
// untrusted, and no test, connection, or sign-in reaches the new host.
#[tokio::test(flavor = "multi_thread")]
async fn credentials_are_bound_to_the_trusted_url() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("t", McpAuth::Bearer)).unwrap();
    c.set_secret("t", "bearer", "token").unwrap();
    wait_until(|| matches!(status_of(&f.manager, "t"), Some(McpServerStatus::Ready { .. }))).await;

    let text = f.file().replace("https://mcp.example.com/mcp", "https://other.example.net/mcp");
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    assert_eq!(c.list().unwrap()[0].status, McpServerStatus::Untrusted);
    let attempts = f.connector.attempt_count();
    let TestPlan::Refused(refused) = c.test_plan("t").unwrap() else { panic!("refused") };
    assert_eq!(serde_json::to_value(refused).unwrap()["failure"], "untrusted");
    assert_eq!(f.connector.attempt_count(), attempts);

    c.save(http_record("o", McpAuth::Oauth2)).unwrap();
    let text = f.file().replacen("https://mcp.example.com/mcp", "https://evil.example.org/mcp", 1);
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    c.list().unwrap();
    assert_eq!(c.signing_in_record("o").unwrap_err(), "untrusted");

    // An http record with no credential needs no trust.
    c.save(http_record("open", McpAuth::None)).unwrap();
    let text = f.file().replacen("https://mcp.example.com/mcp", "https://moved.example.org/mcp", 1);
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    let open = c.list().unwrap().into_iter().find(|e| e.id == "open").unwrap();
    assert_ne!(open.status, McpServerStatus::Untrusted);
}

// MCS-FR-HQJQ, MCS-FR-ERQP: a test of an invalid or a signed-out record opens
// no connection.
#[test]
fn tests_of_refused_records_open_nothing() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("o", McpAuth::Oauth2)).unwrap();
    let TestPlan::Refused(refused) = c.test_plan("o").unwrap() else { panic!("refused") };
    assert_eq!(serde_json::to_value(refused).unwrap()["failure"], "needs_sign_in");
    std::fs::write(
        f.dir.path().join(".synthesis/project.toml"),
        "[[mcpServers]]\nid = \"bad\"\nname = \"Bad\"\ntransport = \"pigeon\"\n",
    )
    .unwrap();
    let TestPlan::Refused(refused) = c.test_plan("bad").unwrap() else { panic!("refused") };
    assert_eq!(serde_json::to_value(refused).unwrap()["failure"], "invalid");
    assert_eq!(f.connector.attempt_count(), 0);
}

// MCS-FR-IXHD, MCS-FR-WBDV: set and clear refuse a slot the record does not
// declare, a value with a line break, and an unknown server; clear of an empty
// slot is no error.
#[test]
fn secret_operations_refuse_what_the_record_does_not_allow() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("tracker", McpAuth::Bearer)).unwrap();
    assert_eq!(c.set_secret("tracker", "env:X", "v").unwrap_err(), "unknown_slot");
    assert_eq!(
        c.set_secret("tracker", "oauth", "v").unwrap_err(),
        "unknown_slot",
        "the oauth slot is not settable"
    );
    assert_eq!(c.set_secret("tracker", "bearer", "a\nb").unwrap_err(), "invalid_secret");
    assert_eq!(c.set_secret("tracker", "bearer", "").unwrap_err(), "invalid_secret");
    assert_eq!(c.set_secret("nobody", "bearer", "v").unwrap_err(), "not_found");
    assert!(c.clear_secret("tracker", "bearer").is_ok());
    c.set_secret("tracker", "bearer", "token").unwrap();
    assert_eq!(f.secrets.get("/repo", "tracker", "bearer").unwrap().as_deref(), Some("token"));
    let entry = c.clear_secret("tracker", "bearer").unwrap();
    assert!(!entry.secrets[0].present);
    assert_eq!(entry.status, McpServerStatus::NeedsSecret);
    f.secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(c.set_secret("tracker", "bearer", "token").unwrap_err(), "keychain_unavailable");
}

// MCS-FR-OCHR: a save removes the values of the slots the record no longer
// declares, and the OAuth state when the record leaves oauth2.
#[test]
fn a_save_removes_the_secrets_of_dropped_slots() {
    let f = Fixture::new();
    let c = f.context();
    c.save(with_secret_env(stdio_record("docs"), &["A", "B"])).unwrap();
    c.set_secret("docs", "env:A", "a").unwrap();
    c.set_secret("docs", "env:B", "b").unwrap();
    c.save(with_secret_env(stdio_record("docs"), &["A"])).unwrap();
    assert_eq!(f.secrets.get("/repo", "docs", "env:A").unwrap().as_deref(), Some("a"));
    assert_eq!(f.secrets.get("/repo", "docs", "env:B").unwrap(), None);

    c.save(http_record("t", McpAuth::Oauth2)).unwrap();
    f.secrets.set("/repo", "t", "oauth", "{}").unwrap();
    c.save(http_record("t", McpAuth::Bearer)).unwrap();
    assert_eq!(f.secrets.get("/repo", "t", "oauth").unwrap(), None);
}

// MCS-FR-UOIO: remove removes the record, every secret, and the trust record.
#[tokio::test(flavor = "multi_thread")]
async fn remove_takes_the_record_its_secrets_and_its_trust() {
    let f = Fixture::new();
    let c = f.context();
    c.save(with_secret_env(stdio_record("docs"), &["A"])).unwrap();
    c.set_secret("docs", "env:A", "a").unwrap();
    f.secrets.set("/repo", "docs", "oauth", "{}").unwrap();
    c.remove("docs").unwrap();
    assert!(c.list().unwrap().is_empty());
    assert_eq!(f.secrets.get("/repo", "docs", "env:A").unwrap(), None);
    assert_eq!(f.secrets.get("/repo", "docs", "oauth").unwrap(), None);
    assert!(f.store.load_mcp_trust("/repo").unwrap().is_empty());
    assert_eq!(c.remove("docs").unwrap_err(), "not_found");
    assert_eq!(status_of(&f.manager, "docs"), None);
}

// MCS-FR-XCOS: a refused save changes nothing and gives the first code.
#[test]
fn a_refused_save_changes_nothing() {
    let f = Fixture::new();
    let c = f.context();
    let mut bad = http_record("t", McpAuth::None);
    if let McpTransport::Http(http) = &mut bad.transport {
        http.url = "http://example.com".to_string();
    }
    assert_eq!(c.save(bad).unwrap_err(), "invalid_url");
    assert!(f.file().is_empty());
}

// MCS-FR-ETIS, MCS-FR-ERQP: a test opens its own connection and lists the
// tools, also for a disabled server, and changes nothing; a test of a server
// the gate refuses opens nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_test_connects_on_its_own_and_changes_nothing() {
    let f = Fixture::new();
    let c = f.context();
    let mut off = http_record("off", McpAuth::None);
    off.enabled = false;
    c.save(off).unwrap();
    let before = f.connector.attempt_count();
    let result = run_test(&f.manager, c.test_plan("off").unwrap()).await;
    let McpTestResult::Passed { ok, tools } = result else { panic!("the test passes") };
    assert!(ok);
    assert_eq!(tools.len(), 5);
    assert_eq!(f.connector.attempt_count(), before + 1);
    assert_eq!(
        status_of(&f.manager, "off"),
        Some(McpServerStatus::Disabled),
        "the status is unchanged"
    );

    c.save(http_record("bearer", McpAuth::Bearer)).unwrap();
    let TestPlan::Refused(refused) = c.test_plan("bearer").unwrap() else { panic!("refused") };
    assert_eq!(serde_json::to_value(refused).unwrap()["failure"], "needs_secret");

    f.connector.plan(&[Plan::Fail(ConnectError::Failure(McpFailure::Unreachable))]);
    c.set_secret("bearer", "bearer", "t").unwrap();
    wait_until(|| {
        status_of(&f.manager, "bearer")
            == Some(McpServerStatus::Failed { failure: McpFailure::Unreachable })
    })
    .await;
    f.connector.plan(&[Plan::Fail(ConnectError::Failure(McpFailure::Rejected))]);
    let failed = run_test(&f.manager, c.test_plan("bearer").unwrap()).await;
    let json = serde_json::to_value(failed).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["failure"], "rejected");
    assert_eq!(json["stderrTail"], serde_json::json!([]));
    assert_eq!(c.test_plan("nobody").err().as_deref(), Some("not_found"));
}

// MCS-FR-KCIH: reconnect rereads the record and opens a new connection.
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_opens_a_new_connection() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("docs", McpAuth::None)).unwrap();
    wait_until(|| matches!(status_of(&f.manager, "docs"), Some(McpServerStatus::Ready { .. })))
        .await;
    let attempts = f.connector.attempt_count();
    c.reconnect("docs").unwrap();
    wait_until(|| f.connector.attempt_count() == attempts + 1).await;
    assert_eq!(c.reconnect("nobody").unwrap_err(), "not_found");
}

// MCS-FR-QRIB, MCS-FR-MGCJ: sign-out drops the tokens, keeps the registered
// client, and sets `needs_sign_in`; it refuses a server that is not oauth2.
#[test]
fn sign_out_drops_the_tokens_and_keeps_the_client() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("t", McpAuth::Oauth2)).unwrap();
    let slot = serde_json::json!({
        "registration": { "clientId": "registered", "redirectUri": "http://127.0.0.1:5000/callback" },
        "credentials": {
            "client_id": "registered",
            "token_response": { "access_token": "at", "token_type": "bearer" }
        }
    });
    f.secrets.set("/repo", "t", "oauth", &slot.to_string()).unwrap();
    let entry = c.sign_out("t").unwrap();
    assert_eq!(entry.status, McpServerStatus::NeedsSignIn);
    let stored = crate::mcp::oauth::read_slot(f.secrets.as_ref(), "/repo", "t").unwrap();
    assert!(stored.credentials.is_none());
    assert_eq!(stored.registration.unwrap().client_id, "registered");
    c.save(http_record("b", McpAuth::Bearer)).unwrap();
    assert_eq!(c.sign_out("b").unwrap_err(), "not_oauth2");
}

// MCS-FR-OCHR: the OAuth state of a server does not follow its record to a new
// URL.
#[test]
fn a_new_url_drops_the_oauth_state() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("t", McpAuth::Oauth2)).unwrap();
    f.secrets.set("/repo", "t", "oauth", "{}").unwrap();
    let mut same = http_record("t", McpAuth::Oauth2);
    same.name = "Renamed".to_string();
    c.save(same).unwrap();
    assert!(f.secrets.get("/repo", "t", "oauth").unwrap().is_some(), "a rename keeps the sign-in");
    let mut moved = http_record("t", McpAuth::Oauth2);
    if let McpTransport::Http(http) = &mut moved.transport {
        http.url = "https://other.example.com/mcp".to_string();
    }
    c.save(moved).unwrap();
    assert_eq!(f.secrets.get("/repo", "t", "oauth").unwrap(), None);
}

// MCS-FR-UOIO: a remove stops the session also when the vault fails.
#[tokio::test(flavor = "multi_thread")]
async fn a_remove_stops_the_session_when_the_vault_fails() {
    let f = Fixture::new();
    let c = f.context();
    c.save(http_record("docs", McpAuth::None)).unwrap();
    wait_until(|| matches!(status_of(&f.manager, "docs"), Some(McpServerStatus::Ready { .. })))
        .await;
    f.secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(c.remove("docs").unwrap_err(), "keychain_unavailable");
    assert_eq!(status_of(&f.manager, "docs"), None);
    assert!(f.manager.mcp_tools_now("/repo").is_empty());
}

// MCS-FR-SBND, MCS-FR-UOIO, ASV-FR-FFKB: the vault store keeps each slot at
// its path, and removes the whole subtree of one server.
#[test]
fn the_vault_store_uses_the_project_mcp_namespace() {
    use crate::secret_vault::test_support::FakeKeyring;
    let vault = Arc::new(crate::secret_vault::Vault::new(Box::new(FakeKeyring::default())));
    vault.mark_migrated();
    let secrets = crate::mcp::secrets::VaultMcpSecrets::new(Arc::clone(&vault));
    secrets.set("/repo", "docs", "env:A", "a").unwrap();
    secrets.set("/repo", "docs", "bearer", "b").unwrap();
    secrets.set("/other", "docs", "env:A", "kept").unwrap();
    let path = |p: &str, slot: &str| {
        vec!["projects".to_string(), p.to_string(), "mcp".into(), "docs".into(), slot.to_string()]
    };
    assert_eq!(vault.read_secret(&path("/repo", "env:A")).unwrap().as_deref(), Some("a"));
    let presence = secrets.presence("/repo", "docs", &["env:A".into(), "env:B".into()]).unwrap();
    assert_eq!(presence.get("env:A"), Some(&true));
    assert_eq!(presence.get("env:B"), Some(&false));
    secrets.remove_server("/repo", "docs").unwrap();
    assert_eq!(secrets.get("/repo", "docs", "env:A").unwrap(), None);
    assert_eq!(secrets.get("/repo", "docs", "bearer").unwrap(), None);
    assert_eq!(secrets.get("/other", "docs", "env:A").unwrap().as_deref(), Some("kept"));
}

// MCS-FR-LXLB: trusting a new destination of an http record removes the
// credentials stored for the old one, so the server needs them again.
#[tokio::test(flavor = "multi_thread")]
async fn trusting_a_new_destination_removes_old_credentials() {
    let f = Fixture::new();
    let c = f.context();
    let mut record = http_record("t", McpAuth::Bearer);
    if let McpTransport::Http(http) = &mut record.transport {
        http.secret_headers.push("X-Key".to_string());
    }
    c.save(record).unwrap();
    c.set_secret("t", "bearer", "token-for-a").unwrap();
    c.set_secret("t", "header:X-Key", "key-for-a").unwrap();
    f.secrets.set("/repo", "t", "oauth", "{}").unwrap();
    let text = f.file().replace("https://mcp.example.com/mcp", "https://other.example.net/mcp");
    std::fs::write(f.dir.path().join(".synthesis/project.toml"), text).unwrap();
    assert_eq!(c.list().unwrap()[0].status, McpServerStatus::Untrusted);
    let shown = c.record("t").unwrap();
    let entry = c.trust("t", &shown).unwrap();
    for slot in ["bearer", "header:X-Key", "oauth"] {
        assert_eq!(f.secrets.get("/repo", "t", slot).unwrap(), None, "{slot}");
    }
    assert_eq!(entry.status, McpServerStatus::NeedsSecret);
}

// MCS-FR-EHBW: a record added under an id the file does not hold starts with
// no secret, also when the vault still holds the secrets of an earlier one.
#[test]
fn a_new_record_under_an_old_id_starts_with_no_secret() {
    let f = Fixture::new();
    let c = f.context();
    f.secrets.set("/repo", "t", "bearer", "left-behind").unwrap();
    f.secrets.set("/repo", "t", "oauth", "{}").unwrap();
    let entry = c.save(http_record("t", McpAuth::Bearer)).unwrap();
    assert_eq!(f.secrets.get("/repo", "t", "bearer").unwrap(), None);
    assert_eq!(f.secrets.get("/repo", "t", "oauth").unwrap(), None);
    assert!(!entry.secrets[0].present);
    // A save of a record the file holds keeps its secrets.
    c.set_secret("t", "bearer", "kept").unwrap();
    let mut renamed = http_record("t", McpAuth::Bearer);
    renamed.name = "Renamed".to_string();
    c.save(renamed).unwrap();
    assert_eq!(f.secrets.get("/repo", "t", "bearer").unwrap().as_deref(), Some("kept"));
}
