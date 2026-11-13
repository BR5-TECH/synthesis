//! The records, their validation, their file form, and the fingerprint.

use super::*;
use crate::mcp::config::*;
use crate::mcp::secrets::{required_slots, settable_slots, value_allowed};
use crate::mcp::status::{gate, MachineFacts};

fn stdio_of(server: &mut McpServer) -> &mut StdioTransport {
    match &mut server.transport {
        McpTransport::Stdio(stdio) => stdio,
        McpTransport::Http(_) => panic!("not stdio"),
    }
}

fn http_of(server: &mut McpServer) -> &mut HttpTransport {
    match &mut server.transport {
        McpTransport::Http(http) => http,
        McpTransport::Stdio(_) => panic!("not http"),
    }
}

// MCS-FR-LLQD: the id pattern and the name length.
#[test]
fn ids_and_names_are_validated() {
    for id in ["a", "docs", "my-server-2", "0abc", &"a".repeat(32)] {
        assert!(valid_id(id), "{id} is valid");
    }
    for id in ["", "-a", "A", "a_b", "a.b", "é", &"a".repeat(33)] {
        assert!(!valid_id(id), "{id} is not valid");
    }
    let mut server = stdio_record("docs");
    server.name = "  Docs  ".to_string();
    assert_eq!(validate(server.clone()).unwrap().name, "Docs");
    server.name = "   ".to_string();
    assert_eq!(validate(server.clone()), Err(McpConfigError::InvalidName));
    server.name = "x".repeat(81);
    assert_eq!(validate(server.clone()), Err(McpConfigError::InvalidName));
    server.name = "é".repeat(80);
    assert!(validate(server).is_ok(), "80 characters, not 80 bytes");
}

// MCS-FR-KALQ: a stdio record needs a command, and its variable names are valid
// and not both plain and secret.
#[test]
fn stdio_records_are_validated() {
    let mut server = stdio_record("docs");
    stdio_of(&mut server).command = "  ".to_string();
    assert_eq!(validate(server.clone()), Err(McpConfigError::InvalidCommand));

    let mut server = stdio_record("docs");
    stdio_of(&mut server).env.insert("1BAD".to_string(), "v".to_string());
    assert_eq!(validate(server), Err(McpConfigError::InvalidEnvName));

    let mut server = stdio_record("docs");
    stdio_of(&mut server).secret_env.push("BAD-NAME".to_string());
    assert_eq!(validate(server), Err(McpConfigError::InvalidEnvName));

    let mut server = stdio_record("docs");
    stdio_of(&mut server).env.insert("TOKEN".to_string(), "v".to_string());
    stdio_of(&mut server).secret_env.push("TOKEN".to_string());
    assert_eq!(validate(server), Err(McpConfigError::DuplicateName));

    let mut server = stdio_record("docs");
    stdio_of(&mut server).env.insert("_OK_1".to_string(), "v".to_string());
    stdio_of(&mut server).secret_env.push("API_KEY".to_string());
    assert!(validate(server).is_ok());
}

// MCS-FR-KLQS: a cwd is relative and stays inside the worktree.
#[test]
fn cwd_stays_inside_the_worktree() {
    for (cwd, ok) in [
        ("tools", true),
        ("tools/../server", true),
        ("./a/b", true),
        ("..", false),
        ("a/../../b", false),
        ("/etc", false),
    ] {
        let mut server = stdio_record("docs");
        stdio_of(&mut server).cwd = Some(cwd.to_string());
        assert_eq!(validate(server).is_ok(), ok, "{cwd}");
    }
    let mut server = stdio_record("docs");
    stdio_of(&mut server).cwd = Some("  ".to_string());
    assert_eq!(stdio_of(&mut validate(server).unwrap()).cwd, None, "a blank cwd is no cwd");
}

// MCS-FR-XDGN: https anywhere, http to a loopback host alone.
#[test]
fn urls_use_https_or_loopback_http() {
    for (url, ok) in [
        ("https://mcp.example.com/mcp", true),
        ("http://localhost:8080/mcp", true),
        ("http://127.0.0.1:9/mcp", true),
        ("http://[::1]:9/mcp", true),
        ("http://mcp.example.com/mcp", false),
        ("http://10.0.0.1/mcp", false),
        ("ftp://example.com", false),
        ("not a url", false),
    ] {
        let mut server = http_record("tracker", McpAuth::None);
        http_of(&mut server).url = url.to_string();
        assert_eq!(validate(server).is_ok(), ok, "{url}");
    }
}

// MCS-FR-DINZ, MCS-FR-FJLR: header names, duplicates in any case, and the
// Authorization conflict of bearer and oauth2.
#[test]
fn headers_are_validated() {
    let mut server = http_record("tracker", McpAuth::None);
    http_of(&mut server).headers.insert("bad header".to_string(), "v".to_string());
    assert_eq!(validate(server), Err(McpConfigError::InvalidHeaderName));

    let mut server = http_record("tracker", McpAuth::None);
    http_of(&mut server).headers.insert("X-Team".to_string(), "v".to_string());
    http_of(&mut server).secret_headers.push("x-team".to_string());
    assert_eq!(validate(server), Err(McpConfigError::DuplicateName));

    let mut server = http_record("tracker", McpAuth::None);
    http_of(&mut server).headers.insert("X-Team".to_string(), "a\nb".to_string());
    assert_eq!(validate(server), Err(McpConfigError::InvalidHeaderValue));

    for auth in [McpAuth::Bearer, McpAuth::Oauth2] {
        let mut server = http_record("tracker", auth);
        http_of(&mut server).secret_headers.push("authorization".to_string());
        assert_eq!(validate(server), Err(McpConfigError::AuthorizationHeaderConflict));
    }
    let mut server = http_record("tracker", McpAuth::None);
    http_of(&mut server).secret_headers.push("Authorization".to_string());
    assert!(validate(server).is_ok(), "with auth none the header is the author's");
}

// MCS-FR-UAAN: the oauth2 fields, and their removal for another auth mode.
#[test]
fn oauth2_fields_are_validated_and_dropped_for_other_modes() {
    let mut server = http_record("tracker", McpAuth::Oauth2);
    http_of(&mut server).callback_port = Some(80);
    assert_eq!(validate(server), Err(McpConfigError::InvalidCallbackPort));

    let mut server = http_record("tracker", McpAuth::Oauth2);
    http_of(&mut server).client_id = Some(" id ".to_string());
    http_of(&mut server).scopes = vec![" a ".to_string(), "".to_string()];
    http_of(&mut server).callback_port = Some(8765);
    let mut valid = validate(server).unwrap();
    assert_eq!(http_of(&mut valid).client_id.as_deref(), Some("id"));
    assert_eq!(http_of(&mut valid).scopes, vec!["a".to_string()]);

    let mut server = http_record("tracker", McpAuth::Bearer);
    http_of(&mut server).client_id = Some("id".to_string());
    http_of(&mut server).callback_port = Some(8765);
    let mut valid = validate(server).unwrap();
    assert_eq!(http_of(&mut valid).client_id, None);
    assert_eq!(http_of(&mut valid).callback_port, None);
}

/// One change of a stdio transport.
type Change = Box<dyn Fn(&mut StdioTransport)>;

// MCS-FR-KWCN: the fingerprint changes with what runs, and only with that.
#[test]
fn the_fingerprint_follows_the_command() {
    let base = stdio_record("docs");
    let McpTransport::Stdio(stdio) = &base.transport else { unreachable!() };
    let print = command_fingerprint(stdio);
    assert_eq!(print.len(), 64);
    assert_eq!(print, command_fingerprint(&stdio.clone()), "stable");

    let mut renamed = base.clone();
    renamed.name = "Other".to_string();
    renamed.enabled = false;
    renamed.disabled_tools = vec!["echo".to_string()];
    let McpTransport::Stdio(same) = &renamed.transport else { unreachable!() };
    assert_eq!(command_fingerprint(same), print);

    let changes: Vec<Change> = vec![
        Box::new(|s| s.command = "uvx".to_string()),
        Box::new(|s| s.args.push("--x".to_string())),
        Box::new(|s| s.cwd = Some("tools".to_string())),
        Box::new(|s| {
            s.env.insert("A".to_string(), "1".to_string());
        }),
        Box::new(|s| s.secret_env.push("KEY".to_string())),
    ];
    for change in changes {
        let mut changed = stdio.clone();
        change(&mut changed);
        assert_ne!(command_fingerprint(&changed), print);
    }
}

// MCS-FR-EKPA, MCS-FR-XCOS, MCS-FR-JZDF, MCS-FR-UOIO, MCS-FR-FIEE: save adds at
// the end or replaces in place, remove removes, and every other key survives.
#[test]
fn records_round_trip_in_project_toml_and_keep_other_keys() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    std::fs::write(
        dir.path().join(".synthesis/project.toml"),
        "lineEndings = \"crlf\"\n[flowTracker]\nepicType = \"Epic\"\n",
    )
    .unwrap();

    let mut docs = stdio_record("docs");
    stdio_of(&mut docs).secret_env.push("DOCS_KEY".to_string());
    stdio_of(&mut docs).cwd = Some("tools".to_string());
    docs.disabled_tools = vec!["reindex".to_string()];
    let mut tracker = http_record("tracker", McpAuth::Oauth2);
    http_of(&mut tracker).scopes = vec!["issues:read".to_string()];
    http_of(&mut tracker).headers.insert("X-Team".to_string(), "core".to_string());
    save_record(&root, &docs).unwrap();
    save_record(&root, &tracker).unwrap();

    let listed = list_records(&root).unwrap();
    assert_eq!(listed.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["docs", "tracker"]);
    assert_eq!(listed[0].record.as_ref().unwrap(), &docs);
    assert_eq!(listed[1].record.as_ref().unwrap(), &tracker);

    let mut renamed = docs.clone();
    renamed.name = "Docs index".to_string();
    save_record(&root, &renamed).unwrap();
    let listed = list_records(&root).unwrap();
    assert_eq!(listed[0].record.as_ref().unwrap().name, "Docs index", "replaced in place");

    let text = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(text.contains("[[mcpServers]]"));
    assert!(text.contains("transport = \"stdio\""));
    assert!(text.contains("secretEnv = [\"DOCS_KEY\"]"));
    assert!(text.contains("lineEndings = \"crlf\""));
    assert!(text.contains("epicType = \"Epic\""));

    assert!(remove_record(&root, "docs").unwrap());
    assert!(!remove_record(&root, "docs").unwrap(), "a second remove finds nothing");
    assert!(remove_record(&root, "tracker").unwrap());
    let text = std::fs::read_to_string(dir.path().join(".synthesis/project.toml")).unwrap();
    assert!(!text.contains("mcpServers"));
    assert!(text.contains("epicType = \"Epic\""));
}

// MCS-FR-HBOD: a record that fails validation, and a later duplicate id, list
// as invalid with their codes and stay in the file.
#[test]
fn invalid_records_are_listed_with_their_codes() {
    let text = r#"
[[mcpServers]]
id = "ok"
name = "OK"
transport = "http"
url = "https://example.com/mcp"

[[mcpServers]]
id = "ok"
name = "Again"
transport = "http"
url = "https://example.com/mcp"

[[mcpServers]]
id = "Bad"
name = "Bad id"
transport = "stdio"
command = "x"

[[mcpServers]]
id = "plain"
name = "Plain http"
transport = "http"
url = "http://example.com/mcp"

[[mcpServers]]
id = "what"
name = "Unknown"
transport = "pigeon"

[[mcpServers]]
id = "auth"
name = "Bad auth"
transport = "http"
url = "https://example.com/mcp"
auth = "magic"
"#;
    let public: toml::Table = toml::from_str(text).unwrap();
    let listed = records_of(&public);
    let codes: Vec<_> = listed.iter().map(|l| l.record.as_ref().err().copied()).collect();
    assert_eq!(
        codes,
        vec![
            None,
            Some(McpConfigError::DuplicateId),
            Some(McpConfigError::InvalidId),
            Some(McpConfigError::InvalidUrl),
            Some(McpConfigError::InvalidTransport),
            Some(McpConfigError::InvalidAuth),
        ]
    );
    let first = listed[0].record.as_ref().unwrap();
    assert!(first.enabled, "an absent enabled reads as true");
    assert_eq!(first.auth(), McpAuth::None, "an absent auth reads as none");
}

// MCS-FR-QBFA, MCS-FR-PXRJ: the settable and the required slots of a record.
#[test]
fn slots_follow_the_record() {
    let mut docs = stdio_record("docs");
    stdio_of(&mut docs).secret_env = vec!["A".to_string(), "B".to_string()];
    assert_eq!(settable_slots(&docs), ["env:A", "env:B"]);

    let mut tracker = http_record("tracker", McpAuth::Bearer);
    http_of(&mut tracker).secret_headers = vec!["X-Key".to_string()];
    assert_eq!(settable_slots(&tracker), ["header:X-Key", "bearer"]);

    let mut oauth = http_record("tracker", McpAuth::Oauth2);
    assert!(settable_slots(&oauth).is_empty());
    http_of(&mut oauth).client_id = Some("id".to_string());
    assert_eq!(settable_slots(&oauth), ["oauth_client_secret"]);
    assert!(required_slots(&oauth).is_empty(), "the client secret is optional");
}

// MCS-FR-IXHD: a header value and a bearer token hold no line break.
#[test]
fn secret_values_are_checked() {
    assert!(value_allowed("env:A", "line one\nline two"));
    assert!(!value_allowed("env:A", ""));
    assert!(!value_allowed("bearer", "abc\n"));
    assert!(!value_allowed("header:X-Key", "a\rb"));
    assert!(value_allowed("bearer", "abc"));
}

// MCS-FR-UXPP, MCS-FR-HQJQ, MCS-FR-PXRJ, MCS-FR-MGCJ, MCS-FR-RISC: the gate.
#[test]
fn the_gate_decides_the_status_before_a_connection() {
    let all = MachineFacts {
        trusted: true,
        secrets_present: true,
        signed_in: true,
        vault_unavailable: false,
    };
    let mut disabled = stdio_record("docs");
    disabled.enabled = false;
    assert_eq!(gate(&disabled, &all), Err(McpServerStatus::Disabled));
    let untrusted = MachineFacts { trusted: false, ..all.clone() };
    assert_eq!(gate(&stdio_record("docs"), &untrusted), Err(McpServerStatus::Untrusted));
    assert_eq!(gate(&http_record("t", McpAuth::None), &untrusted), Ok(()), "MCS-FR-RISC");
    assert_eq!(
        gate(&http_record("t", McpAuth::Bearer), &untrusted),
        Err(McpServerStatus::Untrusted)
    );
    assert_eq!(
        gate(&http_record("t", McpAuth::Oauth2), &untrusted),
        Err(McpServerStatus::Untrusted)
    );
    let missing = MachineFacts { secrets_present: false, ..all.clone() };
    assert_eq!(gate(&stdio_record("docs"), &missing), Err(McpServerStatus::NeedsSecret));
    let signed_out = MachineFacts { signed_in: false, ..all.clone() };
    assert_eq!(
        gate(&http_record("t", McpAuth::Oauth2), &signed_out),
        Err(McpServerStatus::NeedsSignIn)
    );
    assert_eq!(gate(&http_record("t", McpAuth::Bearer), &signed_out), Ok(()));
    let vault = MachineFacts { vault_unavailable: true, ..all.clone() };
    assert_eq!(
        gate(&http_record("t", McpAuth::None), &vault),
        Err(McpServerStatus::Failed { failure: McpFailure::KeychainUnavailable })
    );
    assert_eq!(gate(&stdio_record("docs"), &all), Ok(()));
}

// MCS contract surface: the wire shape the frontend reads.
#[test]
fn the_wire_shape_is_camel_case_with_a_transport_kind() {
    let mut server = http_record("tracker", McpAuth::Oauth2);
    http_of(&mut server).callback_port = Some(8765);
    let json = serde_json::to_value(&server).unwrap();
    assert_eq!(json["transport"]["kind"], "http");
    assert_eq!(json["transport"]["auth"], "oauth2");
    assert_eq!(json["transport"]["callbackPort"], 8765);
    assert_eq!(json["disabledTools"], serde_json::json!([]));
    let back: McpServer = serde_json::from_value(json).unwrap();
    assert_eq!(back, server);

    let ready = serde_json::to_value(McpServerStatus::Ready { tool_count: 3 }).unwrap();
    assert_eq!(ready, serde_json::json!({ "state": "ready", "toolCount": 3 }));
    let failed =
        serde_json::to_value(McpServerStatus::Failed { failure: McpFailure::TlsUntrusted })
            .unwrap();
    assert_eq!(failed, serde_json::json!({ "state": "failed", "failure": "tls_untrusted" }));
    let sign_in = serde_json::to_value(McpServerStatus::NeedsSignIn).unwrap();
    assert_eq!(sign_in, serde_json::json!({ "state": "needs_sign_in" }));
}

// MCS-FR-IQOF: an `enabled` that is not a boolean reads as false, so a server
// the author meant to turn off does not start.
#[test]
fn an_enabled_that_is_not_a_boolean_is_false() {
    let public: toml::Table = toml::from_str(
        "[[mcpServers]]\nid = \"a\"\nname = \"A\"\nenabled = \"false\"\ntransport = \"http\"\nurl = \"https://e.com/mcp\"\n",
    )
    .unwrap();
    assert!(!records_of(&public)[0].record.as_ref().unwrap().enabled);
}
