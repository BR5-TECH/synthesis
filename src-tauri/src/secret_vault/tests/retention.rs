//! Retention, disclosure, reach, and the budget.

use super::*;

// ---------------------------------------------------------------------------
// ASV-FR-27 … ASV-FR-33 — retention, disclosure, reach, and the budget
// ---------------------------------------------------------------------------

/// ASV-FR-27: nothing is retained between operations — a second read of the
/// same path reads the keyring again.
#[test]
fn asv_ts_22_nothing_is_cached_between_operations() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    let reads_before = keyring.count(&Call::Read);
    vault.read_secret(&p(&["github", "tokens", "a"])).unwrap();
    assert_eq!(keyring.count(&Call::Read) - reads_before, 1);
    vault.read_secret(&p(&["github", "tokens", "a"])).unwrap();
    assert_eq!(keyring.count(&Call::Read) - reads_before, 2);

    // And the answer follows the entry rather than a retained copy.
    keyring.set_entry(r#"{"version":1}"#);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        None
    );
}

/// ASV-FR-31, ASV-FR-28 (renderings): no `Debug` rendering the module produces carries a
/// secret, a path, or an id.
#[test]
fn asv_ts_23_debug_renderings_disclose_nothing() {
    let mutation = Mutation::Set {
        path: p(&["github", "tokens", "token-id-42"]),
        secret: "ghp_supersecret".into(),
    };
    let rendered = format!("{mutation:?}");
    assert!(!rendered.contains("ghp_supersecret"));
    assert!(!rendered.contains("token-id-42"));
    assert!(!rendered.contains("github"));

    let removal = Mutation::Remove {
        path: p(&["ai_api", "providers", "openai"]),
    };
    let rendered = format!("{removal:?}");
    assert!(!rendered.contains("openai"));

    let candidate = candidate(
        GITHUB_LEGACY,
        "token-id-42",
        &["github", "tokens", "token-id-42"],
    );
    let rendered = format!("{candidate:?}");
    assert!(!rendered.contains("token-id-42"));

    // Every error renders as its own stable code and nothing more.
    for error in [
        VaultError::Unavailable,
        VaultError::WriteFailed,
        VaultError::VerifyFailed,
        VaultError::Malformed,
        VaultError::UnsupportedVersion,
    ] {
        assert_eq!(error.to_string(), error.code());
        assert!(error.code().starts_with("vault_"));
    }
}

/// ASV-FR-31, ASV-FR-28 (log records): a session in which secrets were read, written,
/// removed, and migrated, and in which every failure of ASV-FR-31 was provoked
/// in turn, produces no log record carrying a secret, a substring of one, a
/// secret path, an id, or a quarantined value.
///
/// This is the requirement a passing suite is least likely to catch on its own
/// — the production sink is absent under test, so every emit site would
/// otherwise execute zero times — which is why `Vault` records what it emitted.
#[test]
fn asv_ts_23_no_log_record_carries_a_secret_a_path_or_an_id() {
    // Every string that must never appear, and the one place each comes from.
    const SECRET: &str = "ghp_supersecretvalue";
    const LEGACY_SECRET: &str = "sk-legacy-key-value";
    const TOKEN_ID: &str = "token-id-42";
    const PROVIDER_ID: &str = "openai-provider-id";
    const GARBAGE: &str = "undecodable-prior-value";

    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(AI_API_LEGACY, PROVIDER_ID, LEGACY_SECRET);
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(
            AI_API_LEGACY,
            PROVIDER_ID,
            &["ai_api", "providers", PROVIDER_ID],
        )]
    }));

    // A whole session: migrate, write, read, presence, remove.
    let token_path = p(&["github", "tokens", TOKEN_ID]);
    set(&vault, &["github", "tokens", TOKEN_ID], SECRET).unwrap();
    vault.read_secret(&token_path).unwrap();
    vault.secret_presence(&[token_path.clone()]).unwrap();
    vault
        .apply_secret_mutations(&[Mutation::Remove {
            path: token_path.clone(),
        }])
        .unwrap();

    // And every typed failure in turn.
    keyring.state().refuse_read = true;
    let _ = vault.read_secret(&token_path);
    keyring.state().refuse_read = false;

    keyring.state().refuse_write = true;
    let _ = set(&vault, &["github", "tokens", TOKEN_ID], SECRET);
    keyring.state().refuse_write = false;

    keyring.state().mangle_next_writes = 1;
    let _ = set(&vault, &["github", "tokens", TOKEN_ID], SECRET);

    keyring.set_entry(GARBAGE);
    let _ = vault.read_secret(&token_path);
    // The quarantine recovery, so the quarantined value passes through a write.
    set(&vault, &["github", "tokens", TOKEN_ID], SECRET).unwrap();

    keyring.set_entry(r#"{"version":99}"#);
    let _ = vault.read_secret(&token_path);
    let _ = set(&vault, &["github", "tokens", TOKEN_ID], SECRET);

    // The two emit sites the session above does not reach on its own: a
    // migration with nothing to adopt, and a candidate whose path is blocked.
    // The claim below is about *every* record the module produces, so every
    // site has to have run.
    {
        let (other, other_keyring) = unmigrated_vault();
        // Nothing to adopt: the candidate has no legacy entry at all.
        other
            .migrate_legacy_secrets(&[candidate(
                AI_API_LEGACY,
                "absent-provider",
                &["ai_api", "providers", "absent-provider"],
            )])
            .unwrap();
        // Now put a secret at the path the next candidate's path runs *through*,
        // so that one cannot be created.
        other_keyring.set_legacy(AI_API_LEGACY, PROVIDER_ID, LEGACY_SECRET);
        other
            .migrate_legacy_secrets(&[candidate(
                AI_API_LEGACY,
                PROVIDER_ID,
                &["ai_api", "providers", PROVIDER_ID],
            )])
            .unwrap();
        other_keyring.set_legacy(GITHUB_LEGACY, TOKEN_ID, SECRET);
        other
            .migrate_legacy_secrets(&[candidate(
                GITHUB_LEGACY,
                TOKEN_ID,
                // Blocked: the leaf below is a secret, not an object.
                &["ai_api", "providers", PROVIDER_ID, "deeper"],
            )])
            .unwrap();
        for record in other.emitted() {
            vault.emitted.lock().unwrap().push(record);
        }
    }

    let records = vault.emitted();
    // Every message the module can emit, so a site added later without a look
    // at what it carries turns this red rather than sliding through.
    let messages: std::collections::HashSet<&str> =
        records.iter().map(|(m, _)| m.as_str()).collect();
    for site in [
        "secret vault operation failed",
        "secret vault mutated",
        "secret vault migrated legacy secrets",
        "secret vault migration found nothing to adopt",
        "secret vault could not adopt a legacy secret",
        "secret vault write could not be verified",
    ] {
        assert!(messages.contains(site), "the session never reached {site:?}");
    }
    assert_eq!(
        messages.len(),
        include_str!("../../secret_vault.rs")
            .matches("            \"secret vault")
            .count()
            .max(6),
        "every emit site in the module is covered by this sweep"
    );
    let forbidden = [
        SECRET,
        LEGACY_SECRET,
        TOKEN_ID,
        PROVIDER_ID,
        GARBAGE,
        // The path segments themselves: ASV-FR-28 forbids a path as much as a
        // secret, and these are the words a leaking field would carry.
        "github",
        "tokens",
        "ai_api",
        "providers",
    ];
    for (message, fields) in &records {
        let rendered = format!("{message} {}", serde_json::json!(fields));
        for needle in forbidden {
            assert!(
                !rendered.contains(needle),
                "a log record disclosed {needle:?}: {rendered}"
            );
        }
    }
}

/// ASV-FR-29: no operation returns the whole object, a namespace of it, or the
/// quarantine field — `read_secret` answers for the one path it was given.
#[test]
fn asv_ts_24_no_operation_returns_more_than_one_path() {
    let (vault, keyring) = vault();
    keyring.set_entry("garbage");
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    set(&vault, &["github", "tokens", "b"], "B").unwrap();

    // A path naming a namespace rather than a leaf answers nothing.
    assert_eq!(vault.read_secret(&p(&["github"])).unwrap(), None);
    assert_eq!(vault.read_secret(&p(&["github", "tokens"])).unwrap(), None);
    // And a presence question answers booleans, never values.
    let presence = vault
        .secret_presence(&[p(&["github", "tokens", "a"]), p(&["github", "tokens", "z"])])
        .unwrap();
    assert_eq!(presence.get(&p(&["github", "tokens", "a"])), Some(&true));
    assert_eq!(presence.get(&p(&["github", "tokens", "z"])), Some(&false));
}

/// ASV-FR-30: presence for twenty paths across three namespaces costs exactly
/// one keyring read and returns twenty booleans.
#[test]
fn asv_ts_25_presence_is_one_read_for_twenty_paths() {
    let (vault, keyring) = vault();
    let mut paths = Vec::new();
    for i in 0..20 {
        let id = format!("id{i}");
        let namespace: &[&str] = match i % 3 {
            0 => &["github", "tokens"],
            1 => &["ai_api", "providers"],
            _ => &["agentic", "vendors"],
        };
        let mut segments = namespace.to_vec();
        segments.push(&id);
        set(&vault, &segments, &format!("secret-{i}")).unwrap();
        paths.push(path(&segments));
    }

    let reads_before = keyring.count(&Call::Read);
    let answers = vault.secret_presence(&paths).unwrap();

    assert_eq!(keyring.count(&Call::Read) - reads_before, 1);
    assert_eq!(answers.len(), 20);
    assert!(answers.values().all(|v| *v));
}

/// The error vocabulary of ASV-FR-31, pinned.
///
/// Deliberately **not** the whole of ASV-FR-31: this asserts only that the five
/// codes exist and are distinct, which is what lets a failure be told apart in a
/// log. That the owning modules collapse all five into one
/// `keychain_unavailable` across the IPC boundary is asserted where it happens —
/// `github_tokens`, `ai_api`, and `agentic` each drive all five through a
/// command.
#[test]
fn the_five_error_codes_are_the_specified_vocabulary() {
    let codes: Vec<&str> = [
        VaultError::Unavailable,
        VaultError::WriteFailed,
        VaultError::VerifyFailed,
        VaultError::Malformed,
        VaultError::UnsupportedVersion,
    ]
    .iter()
    .map(|e| e.code())
    .collect();
    let unique: std::collections::HashSet<&&str> = codes.iter().collect();
    assert_eq!(unique.len(), 5);
    assert_eq!(
        codes,
        vec![
            "vault_unavailable",
            "vault_write_failed",
            "vault_verify_failed",
            "vault_malformed",
            "vault_unsupported_version",
        ]
    );
}

/// ASV-FR-32: nothing in this module reaches a vendor's own credential storage.
///
/// The claim rests on two facts, and both are asserted rather than argued. The
/// `VaultBackend` seam has no filesystem operation at all, so a Codex login
/// directory cannot be opened from here whatever the module does; and the set of
/// legacy entries migration touches is **exactly** the set of candidates it was
/// given, so a vendor that supplies none is never so much as named.
#[test]
fn asv_ts_27_migration_touches_exactly_the_candidates_it_was_given() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(AGENTIC_LEGACY, "claude_code", "sk-ant-oat01-x");
    // Planted, and named by no candidate: Codex authenticates through its own
    // login directory (AIC-FR-20), which this module neither reads nor moves.
    keyring.set_legacy(AGENTIC_LEGACY, "codex", "not ours to touch");
    keyring.set_legacy(AGENTIC_LEGACY, "opencode", "nor this");

    vault
        .migrate_legacy_secrets(&[candidate(
            AGENTIC_LEGACY,
            "claude_code",
            &["agentic", "vendors", "claude_code"],
        )])
        .unwrap();
    set(&vault, &["ai_api", "providers", "openai"], "K").unwrap();
    vault
        .read_secret(&p(&["agentic", "vendors", "claude_code"]))
        .unwrap();

    // Exactly the one candidate, and nothing else, was ever named to the
    // keyring — which is what makes the planted entries a control rather than
    // decoration.
    assert_eq!(
        keyring.legacy_entries_touched(),
        vec![(AGENTIC_LEGACY.to_string(), "claude_code".to_string())]
    );
    assert_eq!(
        keyring.legacy(AGENTIC_LEGACY, "codex").as_deref(),
        Some("not ours to touch")
    );
    assert_eq!(
        keyring.legacy(AGENTIC_LEGACY, "opencode").as_deref(),
        Some("nor this")
    );
    let object = keyring.object();
    let vendors = object
        .get("agentic")
        .and_then(|v| v.get("vendors"))
        .and_then(Value::as_object)
        .expect("the agentic.vendors namespace exists");
    assert!(!vendors.contains_key("codex"));
    assert!(!vendors.contains_key("opencode"));
}

/// ASV-FR-33: every operation in the contract surface returns its documented
/// shape, the whole-object write and read-back both happened, and none returns
/// a secret at a path the caller did not name.
#[test]
fn asv_ts_28_every_operation_answers_in_its_documented_shape() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "legacy", "L");

    let outcome: MigrationOutcome = vault
        .migrate_legacy_secrets(&[candidate(
            GITHUB_LEGACY,
            "legacy",
            &["github", "tokens", "legacy"],
        )])
        .unwrap();
    assert_eq!(outcome.adopted, 1);

    let applied: () = vault
        .apply_secret_mutations(&[Mutation::Set {
            path: p(&["github", "tokens", "a"]),
            secret: "A".into(),
        }])
        .unwrap();
    assert_eq!(applied, ());

    let read: Option<String> = vault.read_secret(&p(&["github", "tokens", "a"])).unwrap();
    assert_eq!(read, Some("A".into()));
    // Not the secret at a path the caller did not name.
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "b"])).unwrap(),
        None
    );

    let presence: HashMap<SecretPath, bool> = vault
        .secret_presence(&[p(&["github", "tokens", "a"])])
        .unwrap();
    assert_eq!(presence.get(&p(&["github", "tokens", "a"])), Some(&true));

    // The whole-object write and its verification both happened.
    assert!(keyring.count(&Call::Write) >= 1);
    let calls = keyring.calls();
    let last_write = calls.iter().rposition(|c| *c == Call::Write).unwrap();
    assert!(calls[last_write + 1..].contains(&Call::Read));
}

