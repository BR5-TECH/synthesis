//! The command implementations. Each takes its collaborators explicitly, so
//! every rule is testable without a CLI installed or a network.

use super::*;

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------
//
// Each takes its collaborators explicitly rather than reaching for managed
// state, so every one is exercisable without a Tauri runtime. The
// `#[tauri::command]` wrappers below are the only Tauri-aware code here.

/// AIC-FR-02: the registry as the UI sees it. Reads the store, stats each stored
/// path, and probes the keychain for each API record's presence; runs no binary
/// and makes no network request.
pub fn list_integrations_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
) -> Result<Vec<AgenticIntegration>, String> {
    let (records, active) = store.load_agentic_registry()?;
    Ok(integrations_from(
        &records,
        active.as_deref(),
        &*ai.probe,
        &vendor_presence(&*ai.secrets),
    ))
}

/// AIC-FR-03: locate a candidate binary. Persists nothing, executes nothing, and
/// refuses a vendor that has no binary to find.
pub fn detect_binary_impl(
    ai: &AgenticIntegrations,
    vendor: &str,
) -> Result<DetectedBinary, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    if descriptor.kind != VendorKind::Cli {
        return Err(ERR_NOT_A_CLI_INTEGRATION.into());
    }
    let path_var = std::env::var("PATH").ok();
    let home = dirs::home_dir();
    let dirs = detection_directories(path_var.as_deref(), home.as_deref());
    Ok(DetectedBinary {
        path: detect_in(&dirs, descriptor.executable_names, &*ai.probe),
    })
}

/// What a successful verification of either kind produced, before it is written.
pub(super) struct VerifiedConfig {
    binary_path: Option<String>,
    path_origin: PathOrigin,
    base_url: Option<String>,
    /// The key to write to the keychain, when one was supplied.
    api_key: Option<String>,
    masked_hint: Option<String>,
    version: Option<String>,
    models: Vec<ModelOption>,
    models_origin: ModelsOrigin,
}

/// AIC-FR-04 / FR-05 / FR-08: run the binary, confirm it is the vendor it claims
/// to be, and learn its models.
pub(super) fn verify_cli(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    path: &str,
) -> Result<VerifiedConfig, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ERR_PATH_EMPTY.into());
    }
    let binary = PathBuf::from(trimmed);

    // Distinguishing a wrong path from a permissions problem is the whole point
    // of AIC-FR-05, and neither is worth spawning a process to discover.
    if !ai.probe.exists(&binary) {
        return Err(ERR_NOT_FOUND.into());
    }
    if !ai.probe.is_executable(&binary) {
        return Err(ERR_NOT_EXECUTABLE.into());
    }

    let output = ai
        .runner
        .run(&binary, descriptor.version_args, VERIFY_TIMEOUT)
        .map_err(|e| match e {
            RunError::NotFound => ERR_NOT_FOUND.to_string(),
            RunError::NotExecutable => ERR_NOT_EXECUTABLE.to_string(),
            RunError::TimedOut => ERR_TIMED_OUT.to_string(),
            RunError::Failed(_) => ERR_EXECUTION_FAILED.to_string(),
        })?;

    if !identifies_vendor(descriptor, &binary, &output) {
        return Err(ERR_NOT_THE_EXPECTED_CLI.into());
    }

    // AIC-FR-08: the probe is enrichment. It runs after identity is settled and
    // its failure never fails a verification that otherwise succeeded — the
    // model list degrades to the bundled catalog, the verification does not.
    let probed = descriptor.model_probe_args.and_then(|args| {
        ai.runner
            .run(&binary, args, VERIFY_TIMEOUT)
            .ok()
            .filter(|out| out.success)
            .map(|out| parse_probed_models(&out))
            .filter(|models| !models.is_empty())
    });
    let (models, models_origin) = match probed {
        Some(models) => (models, ModelsOrigin::Probed),
        None => (catalog_models(descriptor), ModelsOrigin::Catalog),
    };

    // AIC-FR-07: nothing tells us where the path came from, so we ask the same
    // question detection would. A path the author typed that happens to be the
    // one detection finds *is* the detected path; the distinction only matters
    // for whether a later detection may replace it, and it may not replace one
    // it would have produced anyway.
    let detected = detect_binary_impl(ai, descriptor.vendor)?.path;
    let path_origin = if detected.as_deref() == Some(trimmed) {
        PathOrigin::Detected
    } else {
        PathOrigin::UserSupplied
    };

    Ok(VerifiedConfig {
        binary_path: Some(trimmed.to_string()),
        path_origin,
        base_url: None,
        api_key: None,
        masked_hint: None,
        version: extract_version(&output),
        models,
        models_origin,
    })
}

/// AIC-FR-22: reach the endpoint, confirm it answers as an agent-execution API,
/// and learn its models.
pub(super) fn verify_api(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<VerifiedConfig, String> {
    let normalized = normalize_base_url(base_url).map_err(|e| match e {
        BaseUrlError::Empty => ERR_BASE_URL_EMPTY.to_string(),
        BaseUrlError::Invalid => ERR_BASE_URL_INVALID.to_string(),
    })?;
    let key = api_key.map(str::trim).filter(|k| !k.is_empty());
    if descriptor.key_required && key.is_none() {
        // Refused before any request: a missing key is not something the network
        // can tell us, and asking it would cost a round trip to learn nothing.
        return Err(ERR_KEY_MISSING.into());
    }

    let probed = ai
        .prober
        .probe(&ProbeRequest {
            base_url: &normalized,
            api_key: key,
            auth: descriptor.auth,
            models_path: descriptor.models_path,
            models_format: crate::ai_shared::ModelsFormat::Lenient,
        })
        .map_err(agentic_probe_error)?;

    // AIC-FR-08: an endpoint that lists nothing degrades to the bundled catalog
    // rather than failing a verification that otherwise succeeded.
    let (models, models_origin) = if probed.is_empty() {
        (catalog_models(descriptor), ModelsOrigin::Catalog)
    } else {
        (probed, ModelsOrigin::Probed)
    };

    Ok(VerifiedConfig {
        binary_path: None,
        path_origin: PathOrigin::Unset,
        base_url: Some(normalized),
        api_key: key.map(str::to_string),
        masked_hint: key.map(mask_hint),
        // An agent endpoint reports no version the way a CLI banner does; the
        // verification timestamp is what the surface shows instead.
        version: None,
        models,
        models_origin,
    })
}

/// AIC-FR-26 / AIC-FR-27: settle what a CLI verification means for the vendor's
/// credential, before anything is run or written.
///
/// Returns the token to store, or `None` when there is nothing to store —
/// either because the vendor holds no credential, or because the author is
/// keeping the one already in the keychain.
///
/// The three refusals are the whole of the credential contract:
///
/// - a token supplied for a vendor that does not carry one is `wrong_config_kind`,
///   the same class of mistake as a base URL sent to a CLI;
/// - a token that is not shaped like one is `token_malformed`, decided before
///   the binary is run and before the keychain is touched;
/// - a `{ path }` payload for a vendor whose token is mandatory, with nothing
///   stored, is `token_missing` — the integration stays whatever it was rather
///   than becoming configured on a path alone.
pub(super) fn resolve_supplied_token(
    ai: &AgenticIntegrations,
    descriptor: &AgenticVendor,
    oauth_token: Option<&str>,
) -> Result<Option<String>, String> {
    match oauth_token {
        Some(raw) => {
            if !descriptor.requires_oauth_token() {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            // Trimmed before the check so a token pasted with a stray newline is
            // not rejected as malformed; an empty field still fails, because
            // `""` matches nothing the pattern accepts.
            let token = raw.trim();
            if !is_valid_oauth_token(token) {
                return Err(ERR_TOKEN_MALFORMED.into());
            }
            Ok(Some(token.to_string()))
        }
        None => {
            if descriptor.requires_oauth_token()
                && !ai
                    .secrets
                    .has(descriptor.vendor)
                    .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            {
                return Err(ERR_TOKEN_MISSING.into());
            }
            Ok(None)
        }
    }
}

/// AIC-FR-UFNB: does the payload carry a field of Claude Code's gateway shape?
fn carries_gateway_fields(config: &VerifyConfig) -> bool {
    config.gateway_base_url.is_some()
        || config.gateway_token_var.is_some()
        || config.gateway_token.is_some()
}

/// AIC-FR-25: does the payload carry a field that only Claude Code has?
fn carries_claude_only_fields(config: &VerifyConfig) -> bool {
    config.auth_mode.is_some() || carries_gateway_fields(config) || config.env_vars.is_some()
}

/// AIC-FR-06: a successful verification of either kind is what commits a
/// configuration. A failure persists nothing.
pub fn verify_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
    config: &VerifyConfig,
) -> Result<AgenticIntegration, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;

    // AIC-FR-25 / AIC-FR-26: the configuration must be the shape this vendor
    // accepts. Refusing the wrong shape rather than ignoring the surplus field
    // is what keeps a UI bug from silently verifying against a stale value — and
    // what stops a credential meant for one vendor from being handed to another.
    let mut mode = AuthMode::Subscription;
    let mut gateway: Option<GatewayPlan> = None;
    let (verified, supplied_token) = match descriptor.kind {
        VendorKind::Cli => {
            if config.base_url.is_some() || config.api_key.is_some() {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            let claude = descriptor.requires_oauth_token();
            if !claude && carries_claude_only_fields(config) {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            if claude {
                // AIC-FR-WNQR / AIC-FR-UFNB: the mode names the shape, and a
                // field of the other shape is refused rather than ignored.
                mode = match config.auth_mode.as_deref() {
                    None => AuthMode::Subscription,
                    Some(raw) => AuthMode::parse(raw).ok_or(ERR_WRONG_CONFIG_KIND)?,
                };
                match mode {
                    AuthMode::Subscription => {
                        if carries_gateway_fields(config) {
                            return Err(ERR_WRONG_CONFIG_KIND.into());
                        }
                    }
                    AuthMode::CustomGateway => {
                        if config.oauth_token.is_some() {
                            return Err(ERR_WRONG_CONFIG_KIND.into());
                        }
                        gateway = Some(plan_gateway(ai, config)?);
                    }
                }
                if let Some(entries) = config.env_vars.as_deref() {
                    validate_env_vars(entries)?;
                }
            }
            if gateway.is_some() {
                // AIC-FR-QHLN: every turn runs Claude Code inside Docker, so a
                // gateway verification runs no host binary and asks nothing of
                // the gateway. The stored path is kept as it is.
                (
                    VerifiedConfig {
                        binary_path: None,
                        path_origin: PathOrigin::Unset,
                        base_url: None,
                        api_key: None,
                        masked_hint: None,
                        version: None,
                        models: catalog_models(descriptor),
                        models_origin: ModelsOrigin::Catalog,
                    },
                    None,
                )
            } else {
                // Everything about the token is settled before the binary runs,
                // so a value that is not a token costs neither a process spawn
                // nor a keychain round trip (AIC-FR-27).
                let supplied =
                    resolve_supplied_token(ai, descriptor, config.oauth_token.as_deref())?;
                (
                    verify_cli(ai, descriptor, config.path.as_deref().unwrap_or(""))?,
                    supplied,
                )
            }
        }
        VendorKind::Api => {
            if config.path.is_some()
                || config.oauth_token.is_some()
                || carries_claude_only_fields(config)
            {
                return Err(ERR_WRONG_CONFIG_KIND.into());
            }
            (
                verify_api(
                    ai,
                    descriptor,
                    config.base_url.as_deref().unwrap_or(""),
                    config.api_key.as_deref(),
                )?,
                None,
            )
        }
    };

    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("agentic registry poisoned: {e}"))?;

    // AIC-FR-06 / AIC-FR-20: the credential reaches the keychain before the
    // registry is touched, so a keychain that refuses cannot leave behind a
    // record describing a credential that was never stored.
    //
    // Matched on the vendor rather than only on whether a credential arrived,
    // because the absences mean different things. Codex and OpenCode hold no
    // credential at all, so they must not touch the credential store even to
    // clear it — doing so would make a locked keychain fail a verification that
    // never needed one. An API-kind vendor verified without a key is being
    // reconfigured as keyless: its record will carry no hint, so any previous
    // entry has to go with it, or the old key would stay in the store with
    // nothing describing it (AIC-FR-14 / AIC-FR-20). Claude Code keeping its
    // stored token writes nothing, which is what makes a `{ path }`
    // re-verification leave the token exactly as it was.
    //
    // AIC-FR-28: this is also the atomic replacement. `set` overwrites the
    // vendor's one entry in a single step, so the token that was working is
    // still there until the new one lands and there is no observable moment
    // holding both or neither.
    match (descriptor.kind, verified.api_key.as_deref()) {
        (VendorKind::Api, Some(key)) => ai
            .secrets
            .set(vendor, key)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?,
        (VendorKind::Api, None) => ai
            .secrets
            .delete(vendor)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?,
        (VendorKind::Cli, _) => {
            if let Some(token) = supplied_token.as_deref() {
                ai.secrets
                    .set(vendor, token)
                    .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
            }
            // AIC-FR-YXAB: the gateway token has a vault entry of its own, and
            // a subscription verification never reaches it.
            if let Some(token) = gateway.as_ref().and_then(|g| g.supplied_token.as_deref()) {
                ai.secrets
                    .set(GATEWAY_SECRET_ID, token)
                    .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
            }
        }
    }

    let (mut records, active) = store.load_agentic_registry()?;
    let index = match records.iter().position(|r| r.vendor == vendor) {
        Some(i) => i,
        None => {
            records.push(AgenticRecord::empty(vendor));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        // AIC-FR-QHLN: a gateway verification keeps the stored path.
        if gateway.is_none() {
            record.binary_path = verified.binary_path;
            record.path_origin = verified.path_origin;
        }
        record.base_url = verified.base_url;
        // A new token brings a new hint; a verification that kept the stored
        // token keeps the hint describing it, because the hint describes what is
        // in the keychain rather than what this call carried.
        match supplied_token.as_deref() {
            Some(token) => record.masked_hint = Some(mask_hint(token)),
            None if descriptor.requires_oauth_token() => {}
            None => record.masked_hint = verified.masked_hint,
        }
        if descriptor.requires_oauth_token() {
            // AIC-FR-WNQR / AIC-FR-YXAB / AIC-FR-SXVA.
            record.auth_mode = mode;
            if let Some(plan) = gateway.as_ref() {
                record.gateway_base_url = Some(plan.base_url.clone());
                record.gateway_token_var = Some(plan.token_var.clone());
                if let Some(token) = plan.supplied_token.as_deref() {
                    record.gateway_masked_hint = gateway_hint(token);
                }
            }
            if let Some(entries) = config.env_vars.as_ref() {
                record.env_vars = entries.clone();
            }
        }
        record.version = verified.version;
        record.verified_at = Some(now_iso8601());
        // AIC-FR-11: a selection the refreshed list no longer offers falls back
        // rather than requesting a model this installation does not have, and
        // the clearing reaches every model selection the record holds — the
        // default and each turn kind's override alike. A selection the refreshed
        // list still offers is kept exactly as it was. The effort selections are
        // untouched — effort levels come from the descriptor and do not change
        // with the installation.
        let offered = |id: &str| verified.models.iter().any(|m| m.id == id);
        if let Some(selected) = record.selected_model.clone() {
            if !offered(&selected) {
                record.selected_model = None;
            }
        }
        // Removing rather than nulling: a kind that follows the default is
        // absent from the map, so a dropped override returns that kind to
        // whatever the default now resolves to.
        record.model_overrides.retain(|_, model| offered(model));
        record.models = verified.models;
        record.models_origin = verified.models_origin;
    }

    store.save_agentic_registry(records.clone(), active.clone())?;
    integrations_from(
        &records,
        active.as_deref(),
        &*ai.probe,
        &vendor_presence(&*ai.secrets),
    )
        .into_iter()
        .find(|i| i.vendor == vendor)
        .ok_or_else(|| ERR_UNKNOWN_VENDOR.to_string())
}

/// Shared read-modify-write for the two selection setters.
pub(super) fn update_record<F>(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
    apply: F,
) -> Result<AgenticIntegration, String>
where
    F: FnOnce(&mut AgenticRecord) -> Result<(), String>,
{
    vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("agentic registry poisoned: {e}"))?;
    let (mut records, active) = store.load_agentic_registry()?;
    let index = match records.iter().position(|r| r.vendor == vendor) {
        Some(i) => i,
        None => {
            records.push(AgenticRecord::empty(vendor));
            records.len() - 1
        }
    };
    apply(&mut records[index])?;
    store.save_agentic_registry(records.clone(), active.clone())?;
    integrations_from(
        &records,
        active.as_deref(),
        &*ai.probe,
        &vendor_presence(&*ai.secrets),
    )
        .into_iter()
        .find(|i| i.vendor == vendor)
        .ok_or_else(|| ERR_UNKNOWN_VENDOR.to_string())
}

/// AIC-FR-10: persist the chosen model, for one turn kind or for the record's
/// default.
///
/// A `None` `turn_kind` sets `selected_model`, which every kind falls back to; a
/// named one sets that kind's entry in `model_overrides`. A `None` `model_id`
/// **clears** rather than stores a null: against the default it selects the
/// backend's own, and against a kind it returns that kind to the default. An id
/// absent from the record's current list, and a kind outside the closed set, are
/// each refused, and neither rejection changes anything the record holds.
pub fn set_model_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
    turn_kind: Option<&str>,
    model_id: Option<&str>,
) -> Result<AgenticIntegration, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    if let Some(kind) = turn_kind {
        if !TURN_KINDS.contains(&kind) {
            return Err(ERR_UNKNOWN_TURN_KIND.to_string());
        }
    }
    update_record(store, ai, vendor, |record| {
        if let Some(id) = model_id {
            // The record's *current* list, which is the probed one after a
            // verification and the bundled catalog before it — matching what
            // the selector was populated from.
            let available = if record.models.is_empty() {
                catalog_models(descriptor)
            } else {
                record.models.clone()
            };
            if !available.iter().any(|m| m.id == id) {
                return Err(ERR_UNKNOWN_MODEL.to_string());
            }
        }
        match (turn_kind, model_id) {
            (None, id) => record.selected_model = id.map(str::to_string),
            (Some(kind), None) => {
                record.model_overrides.remove(kind);
            }
            (Some(kind), Some(id)) => {
                record.model_overrides.insert(kind.to_string(), id.to_string());
            }
        }
        Ok(())
    })
}

/// AIC-FR-10 / AIC-FR-19: the model a record resolves for one turn kind.
///
/// The kind's own override where it holds one, and `selected_model` otherwise —
/// the same shape `effort_for` reads, and read from a map of its own so that
/// neither selection can move the other.
pub(super) fn model_for(record: &AgenticRecord, turn_kind: Option<&str>) -> Option<String> {
    turn_kind
        .and_then(|kind| record.model_overrides.get(kind).cloned())
        .or_else(|| record.selected_model.clone())
}

/// AIC-FR-10 / AIC-FR-19: the reasoning effort a record resolves for one turn
/// kind.
///
/// The kind's own override where it holds one, and `selected_effort` otherwise.
/// A caller that names no kind gets the default, which is what every record
/// written before kinds were distinguished resolves for every kind.
pub(super) fn effort_for(record: &AgenticRecord, turn_kind: Option<&str>) -> Option<String> {
    turn_kind
        .and_then(|kind| record.effort_overrides.get(kind).cloned())
        .or_else(|| record.selected_effort.clone())
}

/// AIC-FR-10: persist the chosen reasoning effort, for one turn kind or for the
/// record's default.
///
/// A `None` `turn_kind` sets `selected_effort`, which every kind falls back to;
/// a named one sets that kind's override. A `None` `effort_id` **clears** rather
/// than stores a null: against the default it selects the backend's own, and
/// against a kind it returns that kind to the default. An id the vendor does not
/// declare, and a kind outside the closed set, are each refused.
pub fn set_effort_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
    turn_kind: Option<&str>,
    effort_id: Option<&str>,
) -> Result<AgenticIntegration, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    if let Some(kind) = turn_kind {
        if !TURN_KINDS.contains(&kind) {
            return Err(ERR_UNKNOWN_TURN_KIND.to_string());
        }
    }
    update_record(store, ai, vendor, |record| {
        if let Some(id) = effort_id {
            if !descriptor.reasoning_efforts.iter().any(|(e, _)| *e == id) {
                return Err(ERR_UNKNOWN_EFFORT.to_string());
            }
        }
        match (turn_kind, effort_id) {
            (None, id) => record.selected_effort = id.map(str::to_string),
            (Some(kind), None) => {
                record.effort_overrides.remove(kind);
            }
            (Some(kind), Some(id)) => {
                record.effort_overrides.insert(kind.to_string(), id.to_string());
            }
        }
        Ok(())
    })
}

/// AIC-FR-12: record the user-global active integration, across both kinds. At
/// most one is ever recorded — activating one clears any other.
pub fn set_active_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
) -> Result<Vec<AgenticIntegration>, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("agentic registry poisoned: {e}"))?;
    let (records, _) = store.load_agentic_registry()?;
    let present = vendor_presence(&*ai.secrets);
    let verified = records.iter().any(|r| {
        r.vendor == vendor
            && state_of(r, descriptor, &*ai.probe, key_presence(&present, r, descriptor))
                == IntegrationState::Verified
    });
    if !verified {
        return Err(ERR_NOT_VERIFIED.into());
    }
    store.save_agentic_registry(records.clone(), Some(vendor.to_string()))?;
    Ok(integrations_from(&records, Some(vendor), &*ai.probe, &present))
}

/// AIC-FR-14: return a vendor to `unconfigured`, deleting the keychain entry of
/// every vendor that holds one — an API-kind vendor's key or both of Claude
/// Code's tokens — as part of the same operation. Idempotent. A cleared vendor that was
/// active leaves nothing active; a project override naming it is left recorded
/// and stops resolving.
pub fn clear_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    vendor: &str,
) -> Result<Vec<AgenticIntegration>, String> {
    let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("agentic registry poisoned: {e}"))?;
    // The delete comes first, for the same reason the write does in
    // verification: a keychain that refuses must leave the registry untouched
    // rather than dropping the record and stranding the credential (AIC-FR-24).
    // `delete` succeeds on an entry that is not there, which is what makes
    // clearing idempotent.
    if descriptor.holds_credential() {
        ai.secrets
            .delete(vendor)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    }
    // AIC-FR-14: Claude Code's gateway token goes with its OAuth token.
    if descriptor.requires_oauth_token() {
        ai.secrets
            .delete(GATEWAY_SECRET_ID)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    }
    let (mut records, active) = store.load_agentic_registry()?;
    records.retain(|r| r.vendor != vendor);
    let active = match active {
        Some(a) if a == vendor => None,
        other => other,
    };
    store.save_agentic_registry(records.clone(), active.clone())?;
    Ok(integrations_from(
        &records,
        active.as_deref(),
        &*ai.probe,
        &vendor_presence(&*ai.secrets),
    ))
}

/// AIC-FR-32: one migration candidate per vendor that holds a credential.
///
/// No candidate is supplied for Codex or OpenCode: neither holds a credential
/// here — Codex authenticates through a login directory its own CLI wrote,
/// which is outside the vault entirely (`ASV-application-secret-vault.md`
/// ASV-FR-32), and OpenCode through a mechanism this module knows nothing
/// about. For every vendor that does hold one a candidate is supplied whether
/// or not the vault already holds that credential, because which credentials
/// are adopted and which legacy entries are deleted is the vault's decision
/// (ASV-FR-21, ASV-FR-25).
pub fn migration_candidates(records: &[AgenticRecord]) -> Vec<crate::secret_vault::Candidate> {
    records
        .iter()
        .filter(|record| vendor_descriptor(&record.vendor).is_some_and(|d| d.holds_credential()))
        .map(|record| crate::secret_vault::Candidate {
            legacy_service: LEGACY_KEYCHAIN_SERVICE.to_string(),
            legacy_account: record.vendor.clone(),
            path: crate::secret_vault::path(&["agentic", "vendors", &record.vendor]),
        })
        .collect()
}

/// AIC-FR-16: how the open project resolves an integration right now.
pub fn get_project_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    project_key: &str,
) -> Result<ProjectAgenticIntegration, String> {
    let (records, active) = store.load_agentic_registry()?;
    // An empty key is "no project open" (`ProjectState::slot_key`), whose slot is
    // the shared default one — an override read from there would belong to no
    // project in particular.
    let override_vendor = if project_key.is_empty() {
        None
    } else {
        store.load_agentic_override(project_key)?
    };
    Ok(resolve_project(
        &records,
        active.as_deref(),
        override_vendor.as_deref(),
        &*ai.probe,
        &vendor_presence(&*ai.secrets),
    ))
}

/// AIC-FR-17 / FR-18: record (or clear) the open project's override. A vendor
/// that is not currently verified is refused, so an override can only ever name
/// an integration the author has already made work.
pub fn set_project_impl(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    project_key: &str,
    vendor: Option<&str>,
) -> Result<ProjectAgenticIntegration, String> {
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT.into());
    }
    match vendor {
        None => store.clear_agentic_override(project_key)?,
        Some(vendor) => {
            let descriptor = vendor_descriptor(vendor).ok_or(ERR_UNKNOWN_VENDOR)?;
            let (records, _) = store.load_agentic_registry()?;
            let present = vendor_presence(&*ai.secrets);
            let verified = records.iter().any(|r| {
                r.vendor == vendor
                    && state_of(r, descriptor, &*ai.probe, key_presence(&present, r, descriptor))
                        == IntegrationState::Verified
            });
            if !verified {
                return Err(ERR_NOT_VERIFIED.into());
            }
            store.save_agentic_override(project_key, vendor)?;
        }
    }
    get_project_impl(store, ai, project_key)
}

/// AIC-FR-19: the single read path for an actual invocation.
///
/// Deliberately **not** a `#[tauri::command]`: it is not registered in
/// `generate_handler!`, so no frontend `invoke` can reach it. The features that
/// drive an agent call this; nothing calls it today.
///
/// It refuses with the same typed distinction `get_project_agentic_integration`
/// reports rather than returning an invocation when the project resolves no
/// integration — so a caller can route "nothing configured" to Global settings
/// and "nothing chosen" to the choice itself.
#[allow(dead_code)]
pub fn resolve_agentic_invocation(
    store: &GlobalSettingsStore,
    ai: &AgenticIntegrations,
    project_key: &str,
    turn_kind: Option<&str>,
) -> Result<AgenticInvocation, String> {
    let resolved = get_project_impl(store, ai, project_key)?;
    let vendor = match (resolved.resolution, resolved.vendor.as_deref()) {
        (ProjectResolution::NoneConfigured, _) => return Err(ERR_NONE_CONFIGURED.into()),
        (ProjectResolution::NoneSelected, _) => return Err(ERR_NONE_SELECTED.into()),
        (_, Some(vendor)) => vendor.to_string(),
        // `override_unavailable` with nothing active: the project names an
        // integration that no longer works and there is no fallback.
        (_, None) => return Err(ERR_NONE_SELECTED.into()),
    };
    let descriptor = vendor_descriptor(&vendor).ok_or(ERR_NONE_SELECTED)?;
    let (records, _) = store.load_agentic_registry()?;
    let record = records
        .iter()
        .find(|r| r.vendor == vendor)
        .ok_or(ERR_NONE_SELECTED)?;

    match descriptor.kind {
        VendorKind::Cli => {
            // AIC-FR-30 / AIC-FR-QHLN: the executor never runs the stored path,
            // so Claude Code in gateway mode resolves with none.
            let gateway_mode =
                descriptor.requires_oauth_token() && record.auth_mode == AuthMode::CustomGateway;
            let binary_path = if gateway_mode {
                String::new()
            } else {
                record
                    .binary_path
                    .clone()
                    .filter(|p| !p.is_empty())
                    .ok_or(ERR_NONE_SELECTED)?
            };
            // AIC-FR-29: a CLI invocation is a path, a model, and an effort. No
            // token is read here and none is carried out — Claude Code's stored
            // token is stored and nothing more, and transporting it to a
            // process, a container, or an environment is outside what this
            // module does.
            Ok(AgenticInvocation::Cli {
                vendor,
                binary_path,
                model_id: model_for(record, turn_kind),
                effort_id: effort_for(record, turn_kind),
            })
        }
        VendorKind::Api => {
            let base_url = record
                .base_url
                .clone()
                .filter(|u| !u.is_empty())
                .ok_or(ERR_NONE_SELECTED)?;
            // The one place a key is read back out, and only for a caller that
            // is about to present it to the endpoint it belongs to.
            let api_key = if record.masked_hint.is_some() {
                ai.secrets
                    .get(&vendor)
                    .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            } else {
                None
            };
            Ok(AgenticInvocation::Api {
                vendor,
                base_url,
                api_key,
                model_id: model_for(record, turn_kind),
                effort_id: effort_for(record, turn_kind),
            })
        }
    }
}

