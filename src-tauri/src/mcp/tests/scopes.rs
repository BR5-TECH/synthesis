//! The scopes of the servers and the switch on this computer, through the
//! operations, on a temp worktree and a temp user-global store.

use super::*;
use crate::global_settings::GlobalSettingsStore;
use crate::mcp::operations::{current_fingerprint, Context, TRUST_NOT_NEEDED};
use crate::mcp::scope::McpScope;
use crate::mcp::secrets::{McpSecrets, SecretOwner};

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

    fn context_for(&self, key: &str) -> Context<'_> {
        Context {
            root: crate::fs::RootFs::for_root(self.dir.path()),
            root_path: self.dir.path().to_path_buf(),
            key: key.to_string(),
            store: &self.store,
            manager: &self.manager,
        }
    }

    fn context(&self) -> Context<'_> {
        self.context_for("/repo")
    }

    fn file(&self) -> String {
        std::fs::read_to_string(self.dir.path().join(".synthesis/project.toml")).unwrap_or_default()
    }

    /// A team record that a pull brings: written into the file directly.
    fn pull(&self, record: &McpServer) {
        let root = crate::fs::RootFs::for_root(self.dir.path());
        crate::mcp::config::save_record(&root, record).unwrap();
    }

    fn last_owner(&self) -> Option<SecretOwner> {
        self.connector.owners.lock().unwrap().last().cloned()
    }
}

fn with_secret_env(mut server: McpServer, names: &[&str]) -> McpServer {
    if let McpTransport::Stdio(stdio) = &mut server.transport {
        stdio.secret_env = names.iter().map(|n| n.to_string()).collect();
    }
    server
}

async fn ready(f: &Fixture, id: &str) {
    wait_until(|| matches!(status_of(&f.manager, id), Some(McpServerStatus::Ready { .. }))).await;
}

// MCS-FR-ZSFL, MCS-FR-OEDO, MCS-FR-JZDF, MCS-FR-FIEE: a personal record is
// stored on this computer and never in the project file; the list holds the
// team records, then those of this project, then those of all projects, each
// with its scope; a record of all projects is listed in another project too.
#[tokio::test(flavor = "multi_thread")]
async fn personal_records_stay_on_this_computer_and_list_after_the_team() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("mine-all"), McpScope::PersonalAll).unwrap();
    c.save(stdio_record("mine"), McpScope::PersonalProject).unwrap();
    assert!(!f.file().contains("mine"), "a personal record is not in the project file");
    c.save(stdio_record("team"), McpScope::Team).unwrap();
    assert!(f.file().contains("id = \"team\""));
    assert!(!f.file().contains("mine"));
    assert_eq!(f.store.load_personal_mcp_servers("/repo").unwrap().len(), 1);
    assert_eq!(f.store.load_global_mcp_servers().unwrap().len(), 1);

    let listed = c.list().unwrap();
    let order: Vec<(&str, McpScope)> = listed.iter().map(|e| (e.id.as_str(), e.scope)).collect();
    assert_eq!(
        order,
        vec![
            ("team", McpScope::Team),
            ("mine", McpScope::PersonalProject),
            ("mine-all", McpScope::PersonalAll),
        ]
    );
    let wire = serde_json::to_value(&listed[1]).unwrap();
    assert_eq!(wire["scope"], "personal_project");
    assert_eq!(wire["enabledHere"], true);

    let other = f.context_for("/other");
    let ids: Vec<String> = other.list().unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(ids, vec!["team".to_string(), "mine-all".to_string()]);
}

// MCS-FR-MUVC, MCS-FR-EVRS, MCS-FR-UXPP: a personal stdio record connects with
// no trust record, is never untrusted, and refuses a trust step.
#[tokio::test(flavor = "multi_thread")]
async fn a_personal_record_needs_no_trust() {
    let f = Fixture::new();
    let c = f.context();
    let server = stdio_record("mine");
    c.save(server.clone(), McpScope::PersonalProject).unwrap();
    assert!(f.store.load_mcp_trust("/repo").unwrap().is_empty(), "no trust record is written");
    ready(&f, "mine").await;
    assert_eq!(c.trust("mine", &server), Err(TRUST_NOT_NEEDED.to_string()));
    // A change of its command keeps it trusted: the author typed it here.
    let mut changed = server;
    if let McpTransport::Stdio(stdio) = &mut changed.transport {
        stdio.command = "uvx".into();
    }
    let entry = c.save(changed, McpScope::PersonalProject).unwrap();
    assert_ne!(entry.status, McpServerStatus::Untrusted);
    let (owner, _) = c.signing_in_record("mine").unwrap();
    assert_eq!(owner, SecretOwner::Project("/repo".into()));
}

// MCS-FR-SBND, MCS-FR-VHBK, MCS-FR-EHBW: the secrets of each scope are apart;
// a secret of a record of all projects serves every project; a new personal
// record starts with no secret of its own scope and leaves the team's.
#[tokio::test(flavor = "multi_thread")]
async fn each_scope_has_its_own_secrets() {
    let f = Fixture::new();
    let c = f.context();
    let team = SecretOwner::Team("/repo".into());
    let project = SecretOwner::Project("/repo".into());
    f.secrets.set(&team, "kept", "env:KEY", "team-value").unwrap();
    f.secrets.set(&project, "kept", "env:KEY", "old-value").unwrap();
    c.save(with_secret_env(stdio_record("kept"), &["KEY"]), McpScope::PersonalProject).unwrap();
    assert_eq!(f.secrets.get(&project, "kept", "env:KEY").unwrap(), None);
    assert_eq!(f.secrets.get(&team, "kept", "env:KEY").unwrap().as_deref(), Some("team-value"));

    c.set_secret("kept", "env:KEY", "mine").unwrap();
    assert_eq!(f.secrets.get(&project, "kept", "env:KEY").unwrap().as_deref(), Some("mine"));
    ready(&f, "kept").await;
    assert_eq!(f.last_owner(), Some(project));

    let all = with_secret_env(stdio_record("everywhere"), &["KEY"]);
    c.save(all, McpScope::PersonalAll).unwrap();
    c.set_secret("everywhere", "env:KEY", "shared").unwrap();
    assert_eq!(
        f.secrets.get(&SecretOwner::All, "everywhere", "env:KEY").unwrap().as_deref(),
        Some("shared")
    );
    let other = f.context_for("/other");
    let entry = other.list().unwrap().into_iter().find(|e| e.id == "everywhere").unwrap();
    assert!(entry.secrets[0].present, "another project sees the one value");
}

// MCS-FR-PQHI, MCS-FR-EBJL, MCS-FR-UOIO: a team record that a pull brings with
// the id of a personal record is the active record; the personal record is
// `duplicate_id` and never connects; a command by id acts on the team record,
// and the remove of the personal scope removes the personal record alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_pulled_team_record_wins_over_a_personal_record_of_its_id() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("docs"), McpScope::PersonalProject).unwrap();
    ready(&f, "docs").await;
    let mut pulled = http_record("docs", McpAuth::None);
    pulled.name = "Team docs".into();
    f.pull(&pulled);

    let listed = c.list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].scope, McpScope::Team);
    assert_eq!(listed[0].record.as_ref().map(|r| r.name.as_str()), Some("Team docs"));
    assert_eq!(listed[1].scope, McpScope::PersonalProject);
    assert_eq!(listed[1].status, McpServerStatus::Invalid);
    assert_eq!(listed[1].invalid, Some(crate::mcp::config::McpConfigError::DuplicateId));
    assert!(!listed[1].enabled_here);
    // The old session can still read as ready, so wait for the new owner.
    wait_until(|| f.last_owner() == Some(SecretOwner::Team("/repo".into()))).await;
    wait_until(|| !f.manager.mcp_tools_now("/repo").is_empty()).await;
    let tools = f.manager.mcp_tools_now("/repo");
    assert!(
        tools.iter().all(|t| t.server_name == "Team docs"),
        "the shadowed record offers no tool"
    );

    // MCS-FR-EBJL: a command by id goes to the team record and its secrets.
    let team = SecretOwner::Team("/repo".into());
    let project = SecretOwner::Project("/repo".into());
    f.secrets.set(&project, "docs", "env:KEY", "mine").unwrap();
    let entry = c.set_enabled("docs", false).unwrap();
    assert_eq!(entry.scope, McpScope::Team);
    assert_eq!(f.store.load_mcp_switches("/repo").unwrap().get("docs"), Some(&false));
    // MCS-FR-HWNC: the record that its scope holds can still be saved.
    let mut renamed = pulled.clone();
    renamed.name = "Team docs 2".into();
    let entry = c.save(renamed, McpScope::Team).unwrap();
    assert_eq!(entry.record.map(|r| r.name), Some("Team docs 2".to_string()));
    let mut bearer = pulled.clone();
    if let McpTransport::Http(http) = &mut bearer.transport {
        http.auth = McpAuth::Bearer;
    }
    c.save(bearer, McpScope::Team).unwrap();
    c.set_secret("docs", "bearer", "team-token").unwrap();
    assert_eq!(f.secrets.get(&team, "docs", "bearer").unwrap().as_deref(), Some("team-token"));
    assert_eq!(f.secrets.get(&project, "docs", "bearer").unwrap(), None);

    // MCS-FR-UOIO: the remove of the personal record removes its secrets and
    // leaves the team record, its trust, its switch, and its secrets.
    c.remove("docs", McpScope::PersonalProject).unwrap();
    assert!(f.store.load_personal_mcp_servers("/repo").unwrap().is_empty());
    assert_eq!(f.secrets.get(&project, "docs", "env:KEY").unwrap(), None);
    assert!(f.file().contains("id = \"docs\""), "the team record stays");
    assert_eq!(f.store.load_mcp_switches("/repo").unwrap().get("docs"), Some(&false));
    assert!(f.store.load_mcp_trust("/repo").unwrap().contains_key("docs"));
    assert_eq!(f.secrets.get(&team, "docs", "bearer").unwrap().as_deref(), Some("team-token"));
    assert_eq!(c.list().unwrap().len(), 1);
}

// MCS-FR-PQHI, MCS-FR-UOIO: when the team record of an id goes, the personal
// record of that id is active and connects with its own secrets.
#[tokio::test(flavor = "multi_thread")]
async fn a_personal_record_is_active_again_when_the_team_record_goes() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("docs"), McpScope::PersonalProject).unwrap();
    f.pull(&http_record("docs", McpAuth::None));
    assert_eq!(c.list().unwrap()[1].status, McpServerStatus::Invalid);
    c.remove("docs", McpScope::Team).unwrap();
    let listed = c.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].scope, McpScope::PersonalProject);
    assert!(listed[0].enabled_here);
    wait_until(|| f.last_owner() == Some(SecretOwner::Project("/repo".into()))).await;
    ready(&f, "docs").await;
    // MCS-FR-UOIO: a removed server stops and offers no tool.
    c.remove("docs", McpScope::PersonalProject).unwrap();
    assert_eq!(status_of(&f.manager, "docs"), None);
    assert!(f.manager.mcp_tools_now("/repo").is_empty());
}

// MCS-FR-PQHI: a personal record of this project wins over a record of all
// projects with its id, which another project added.
#[tokio::test(flavor = "multi_thread")]
async fn a_record_of_this_project_wins_over_one_of_all_projects() {
    let f = Fixture::new();
    f.context_for("/other").save(stdio_record("notes"), McpScope::PersonalAll).unwrap();
    f.store
        .save_personal_mcp_servers(
            "/repo",
            vec![crate::mcp::config::to_toml(&http_record("notes", McpAuth::None))],
        )
        .unwrap();
    let listed = f.context().list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].scope, McpScope::PersonalProject);
    assert_ne!(listed[0].status, McpServerStatus::Invalid);
    assert_eq!(listed[1].scope, McpScope::PersonalAll);
    assert_eq!(listed[1].status, McpServerStatus::Invalid);
    assert_eq!(listed[1].invalid, Some(crate::mcp::config::McpConfigError::DuplicateId));
    assert!(!listed[1].enabled_here);
}

// MCS-FR-OEDO, MCS-FR-XCOS, MCS-FR-JZDF: a personal list keeps the order the
// author added its records in, a save replaces in place, and no personal
// write touches the project file.
#[tokio::test(flavor = "multi_thread")]
async fn personal_writes_keep_their_order_and_leave_the_project_file() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("team"), McpScope::Team).unwrap();
    let file = f.file();
    for scope in [McpScope::PersonalProject, McpScope::PersonalAll] {
        c.save(stdio_record("b"), scope).unwrap();
        c.save(stdio_record("a"), scope).unwrap();
        let mut again = stdio_record("b");
        again.name = "B again".into();
        c.save(again, scope).unwrap();
        let ids: Vec<(String, String)> = c
            .list()
            .unwrap()
            .into_iter()
            .filter(|e| e.scope == scope)
            .map(|e| (e.id, e.record.unwrap().name))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("b".to_string(), "B again".to_string()),
                ("a".to_string(), "Server a".to_string())
            ]
        );
        c.set_enabled("a", false).unwrap();
        c.remove("a", scope).unwrap();
        c.remove("b", scope).unwrap();
    }
    assert_eq!(f.file(), file, "the project file is not written");
    // MCS-FR-OEDO: a personal record is validated as a team record is.
    let mut bad = stdio_record("Bad!");
    bad.id = "Bad!".into();
    assert_eq!(c.save(bad, McpScope::PersonalAll).unwrap_err(), "invalid_id");
    assert!(f.store.load_global_mcp_servers().unwrap().is_empty());
}

// MCS-FR-HBOD, GSS-FR-RPBB: a personal table that does not decode is listed as
// invalid and stays in the store until a valid record with its id replaces it.
#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_personal_record_stays_until_it_is_replaced() {
    let f = Fixture::new();
    let c = f.context();
    let mut future = toml::Table::new();
    future.insert("id".into(), toml::Value::String("later".into()));
    future.insert("transport".into(), toml::Value::String("future".into()));
    future.insert("name".into(), toml::Value::String("Later".into()));
    f.store.save_global_mcp_servers(vec![future.clone()]).unwrap();
    let listed = c.list().unwrap();
    assert_eq!(listed[0].scope, McpScope::PersonalAll);
    assert_eq!(listed[0].status, McpServerStatus::Invalid);
    assert_eq!(listed[0].invalid, Some(crate::mcp::config::McpConfigError::InvalidTransport));
    c.save(stdio_record("other"), McpScope::PersonalAll).unwrap();
    assert_eq!(f.store.load_global_mcp_servers().unwrap()[0], future);
    c.save(stdio_record("later"), McpScope::PersonalAll).unwrap();
    let tables = f.store.load_global_mcp_servers().unwrap();
    assert_eq!(tables[0].get("transport").and_then(toml::Value::as_str), Some("stdio"));
    assert_eq!(tables.len(), 2);
}

// MCS-FR-ZHKZ: a team server that is off on this computer is `disabled`,
// also when it has no trust record and a missing secret.
#[tokio::test(flavor = "multi_thread")]
async fn an_off_team_server_asks_for_nothing() {
    let f = Fixture::new();
    let c = f.context();
    f.pull(&with_secret_env(stdio_record("pulled"), &["KEY"]));
    assert_eq!(c.list().unwrap()[0].status, McpServerStatus::Untrusted);
    let entry = c.set_enabled("pulled", false).unwrap();
    assert_eq!(entry.status, McpServerStatus::Disabled);
    assert_eq!(f.connector.attempt_count(), 0);
}

/// Secrets that fail the test when anything reads them.
struct NoReads;

impl McpSecrets for NoReads {
    fn get(
        &self,
        _: &SecretOwner,
        _: &str,
        _: &str,
    ) -> Result<Option<String>, crate::mcp::secrets::SecretsUnavailable> {
        panic!("a secret was read")
    }
    fn set(
        &self,
        _: &SecretOwner,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(), crate::mcp::secrets::SecretsUnavailable> {
        panic!("a secret was written")
    }
    fn remove(
        &self,
        _: &SecretOwner,
        _: &str,
        _: &str,
    ) -> Result<(), crate::mcp::secrets::SecretsUnavailable> {
        panic!("a secret was removed")
    }
    fn remove_server(
        &self,
        _: &SecretOwner,
        _: &str,
    ) -> Result<(), crate::mcp::secrets::SecretsUnavailable> {
        panic!("a server was removed")
    }
    fn presence(
        &self,
        _: &SecretOwner,
        _: &str,
        _: &[String],
    ) -> Result<std::collections::HashMap<String, bool>, crate::mcp::secrets::SecretsUnavailable>
    {
        panic!("a presence was read")
    }
}

// MCS-FR-ZHKZ: nothing of the vault is read for a server that is off.
#[test]
fn the_facts_of_an_off_server_read_no_secret() {
    let mut record = http_record("o", McpAuth::Oauth2);
    record.enabled = false;
    let facts = crate::mcp::facts_for(&record, SecretOwner::All, &Default::default(), &NoReads);
    assert_eq!(facts, crate::mcp::status::MachineFacts::default());
}

// MCS-FR-HWNC, MCS-FR-LLQD: an id is in one scope alone, so a save of an id of
// another scope is `duplicate_id` and writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_save_never_moves_an_id_to_another_scope() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("team"), McpScope::Team).unwrap();
    c.save(stdio_record("mine"), McpScope::PersonalAll).unwrap();
    assert_eq!(
        c.save(stdio_record("team"), McpScope::PersonalProject).unwrap_err(),
        "duplicate_id"
    );
    assert_eq!(c.save(stdio_record("mine"), McpScope::Team).unwrap_err(), "duplicate_id");
    assert!(f.store.load_personal_mcp_servers("/repo").unwrap().is_empty());
    assert!(!f.file().contains("mine"));
}

// MCS-FR-VKKF, MCS-FR-KKDO, MCS-FR-MKDH, MCS-FR-ZHKZ, MCS-FR-RYIK: the switch of
// a team server is stored on this computer and not in the file; off, the
// server is `disabled` and its secrets are not read; a value equal to the
// team's removes the switch; a team record that is off by default follows the
// team until the switch turns it on.
#[tokio::test(flavor = "multi_thread")]
async fn the_switch_of_a_team_server_is_on_this_computer() {
    let f = Fixture::new();
    let c = f.context();
    c.save(with_secret_env(stdio_record("docs"), &["KEY"]), McpScope::Team).unwrap();
    c.set_secret("docs", "env:KEY", "value").unwrap();
    ready(&f, "docs").await;
    let file = f.file();

    let entry = c.set_enabled("docs", false).unwrap();
    assert_eq!(entry.status, McpServerStatus::Disabled);
    assert!(!entry.enabled_here);
    assert_eq!(f.file(), file, "the project file is not written");
    assert_eq!(f.store.load_mcp_switches("/repo").unwrap().get("docs"), Some(&false));
    // MCS-FR-ZHKZ: an off server is `disabled` also when the vault fails, as
    // nothing of the vault is read for it.
    f.secrets.unavailable.store(true, std::sync::atomic::Ordering::SeqCst);
    let attempts = f.connector.attempt_count();
    let listed = c.list().unwrap();
    assert_eq!(listed[0].status, McpServerStatus::Disabled);
    assert_eq!(f.connector.attempt_count(), attempts, "an off server does not connect");
    f.secrets.unavailable.store(false, std::sync::atomic::Ordering::SeqCst);

    let entry = c.set_enabled("docs", true).unwrap();
    assert!(entry.enabled_here);
    assert!(f.store.load_mcp_switches("/repo").unwrap().is_empty(), "it follows the team again");
    ready(&f, "docs").await;

    let mut off = http_record("off", McpAuth::None);
    off.enabled = false;
    f.pull(&off);
    let listed = c.list().unwrap();
    assert_eq!(listed[1].status, McpServerStatus::Disabled, "a pulled record follows the team");
    c.set_enabled("off", true).unwrap();
    assert_eq!(f.store.load_mcp_switches("/repo").unwrap().get("off"), Some(&true));
    ready(&f, "off").await;
    assert!(f.file().contains("enabled = false"), "the team default stays");
}

// MCS-FR-MKDH, MCS-FR-VKKF: the switch of a personal server is its `enabled`.
#[tokio::test(flavor = "multi_thread")]
async fn the_switch_of_a_personal_server_saves_its_record() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("mine"), McpScope::PersonalAll).unwrap();
    let entry = c.set_enabled("mine", false).unwrap();
    assert_eq!(entry.status, McpServerStatus::Disabled);
    assert!(f.store.load_mcp_switches("/repo").unwrap().is_empty());
    let tables = f.store.load_global_mcp_servers().unwrap();
    assert_eq!(tables[0].get("enabled").and_then(toml::Value::as_bool), Some(false));
    assert_eq!(c.set_enabled("missing", true).unwrap_err(), "not_found");
}

// MCS-FR-UOIO: removing a team record removes its trust record and its
// switch; removing a record of all projects removes it and its secrets for
// every project.
#[tokio::test(flavor = "multi_thread")]
async fn a_remove_clears_what_its_scope_holds() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("team"), McpScope::Team).unwrap();
    c.set_enabled("team", false).unwrap();
    assert!(!f.store.load_mcp_trust("/repo").unwrap().is_empty());
    assert_eq!(f.store.load_mcp_switches("/repo").unwrap().get("team"), Some(&false));
    c.remove("team", McpScope::Team).unwrap();
    assert!(f.store.load_mcp_trust("/repo").unwrap().is_empty());
    assert!(f.store.load_mcp_switches("/repo").unwrap().is_empty());

    c.save(with_secret_env(stdio_record("all"), &["KEY"]), McpScope::PersonalAll).unwrap();
    c.set_secret("all", "env:KEY", "v").unwrap();
    assert_eq!(c.remove("all", McpScope::PersonalProject).unwrap_err(), "not_found");
    c.remove("all", McpScope::PersonalAll).unwrap();
    assert!(f.store.load_global_mcp_servers().unwrap().is_empty());
    assert_eq!(f.secrets.get(&SecretOwner::All, "all", "env:KEY").unwrap(), None);
    assert!(f.context_for("/other").list().unwrap().is_empty());
}

// MCS-FR-ENII, MCT-FR-JEFK, MCS-FR-ZCPK: the tools of the turn come from the
// servers of every scope, team first; the fingerprint a sign-in compares is
// the one of the active record of every scope.
#[tokio::test(flavor = "multi_thread")]
async fn tools_come_from_every_scope_in_list_order() {
    let f = Fixture::new();
    let c = f.context();
    c.save(stdio_record("zz-team"), McpScope::Team).unwrap();
    c.save(stdio_record("aa-all"), McpScope::PersonalAll).unwrap();
    c.save(stdio_record("mm-project"), McpScope::PersonalProject).unwrap();
    for id in ["zz-team", "aa-all", "mm-project"] {
        ready(&f, id).await;
    }
    let tools = f.manager.mcp_tools_now("/repo");
    let mut servers: Vec<&str> = tools.iter().map(|t| t.server_id.as_str()).collect();
    servers.dedup();
    assert_eq!(servers, vec!["zz-team", "mm-project", "aa-all"]);

    let root = crate::fs::RootFs::for_root(f.dir.path());
    let http = http_record("remote", McpAuth::Oauth2);
    c.save(http.clone(), McpScope::PersonalProject).unwrap();
    assert_eq!(
        current_fingerprint(&root, &f.store, "/repo", "remote"),
        Some(crate::mcp::config::trust_fingerprint(&http))
    );
    assert_eq!(current_fingerprint(&root, &f.store, "/repo", "missing"), None);
}

// MCS-FR-MABN: a log record of a connection names the scope of the server.
#[tokio::test(flavor = "multi_thread")]
async fn connection_logs_name_the_scope() {
    let f = Fixture::new();
    f.context().save(stdio_record("mine"), McpScope::PersonalProject).unwrap();
    ready(&f, "mine").await;
    wait_until(|| f.observer.log_text().contains("\"scope\":\"personal_project\"")).await;
}
