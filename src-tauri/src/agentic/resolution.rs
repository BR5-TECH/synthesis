//! The pure rules: what state a record stands in, which vendor is active, and
//! what a project resolves to (`AIC-agentic-integration-config.md`).

use super::*;

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// AIC-FR-15: a record's state, derived from the filesystem or the keychain
/// rather than stored.
///
/// The `key_present` argument is a `bool` rather than a store handle so the rule
/// stays pure and so no caller of this function is ever handed a key.
pub fn state_of(
    record: &AgenticRecord,
    descriptor: &AgenticVendor,
    probe: &dyn FileProbe,
    key_present: bool,
) -> IntegrationState {
    match descriptor.kind {
        VendorKind::Cli => match record.binary_path.as_deref() {
            None | Some("") => IntegrationState::Unconfigured,
            Some(path) => {
                if !probe.exists(Path::new(path)) {
                    // AIC-FR-15: `missing` wins where both degradations hold,
                    // because a path that no longer exists is the first thing
                    // the author has to correct — re-supplying a token against a
                    // binary that is not there would not fix anything.
                    IntegrationState::Missing
                } else if descriptor.requires_oauth_token() && !key_present {
                    IntegrationState::KeyUnavailable
                } else {
                    IntegrationState::Verified
                }
            }
        },
        VendorKind::Api => match record.base_url.as_deref() {
            None | Some("") => IntegrationState::Unconfigured,
            // A record that was verified with a key must still have one.
            Some(_) if record.masked_hint.is_some() => {
                if key_present {
                    IntegrationState::Verified
                } else {
                    IntegrationState::KeyUnavailable
                }
            }
            // No hint at all. For a vendor whose key is optional that is a
            // deployment configured without one, and nothing is missing. For a
            // vendor that requires one it is a record that cannot work — a
            // half-written or hand-edited store — and calling it `verified`
            // would let it be activated and then hand `resolve_agentic_invocation`
            // an endpoint with no credential, turning a configuration problem
            // into a 401 at the moment of use. Better a row that says re-verify.
            Some(_) => {
                if descriptor.key_required {
                    IntegrationState::KeyUnavailable
                } else {
                    IntegrationState::Verified
                }
            }
        },
    }
}

/// The credential-presence answer each record needs, gathered once per listing.
///
/// A keychain that will not answer downgrades the records rather than failing
/// the listing (AIC-FR-24): an unreadable entry is indistinguishable, from the
/// author's side, from one that is not there, and both mean "verify this again".
///
/// The two credential-holding shapes are probed slightly differently. An
/// API-kind record is probed only once it carries a masked hint, because a
/// deployment verified with no key at all has nothing to look for. Claude Code
/// is probed on the strength of the descriptor alone: its token is mandatory
/// (AIC-FR-26), so a stored configuration with no hint recorded is a
/// half-written store rather than a keyless setup, and asking the keychain is
/// what tells the two apart.
pub(super) fn key_presence(
    present: &KeyPresence,
    record: &AgenticRecord,
    descriptor: &AgenticVendor,
) -> bool {
    let worth_probing = match descriptor.kind {
        VendorKind::Api => record.masked_hint.is_some(),
        VendorKind::Cli => descriptor.requires_oauth_token(),
    };
    if !worth_probing {
        return false;
    }
    present.has(&record.vendor)
}

/// AIC-FR-02: which vendors hold a credential, asked of the vault **once**.
///
/// Only the vendors that hold one are asked about: Codex authenticates through
/// its own login directory and OpenCode through a mechanism this module knows
/// nothing about, so neither has a path in the vault to ask after (AIC-FR-20,
/// `ASV-application-secret-vault.md` ASV-FR-32). Asking about the rest together
/// costs the same single vault access as asking about one (ASV-FR-30).
pub fn vendor_presence(secrets: &dyn SecretStore) -> KeyPresence {
    let ids: Vec<&str> = VENDORS
        .iter()
        .filter(|d| d.holds_credential())
        .map(|d| d.vendor)
        .collect();
    KeyPresence::resolve(secrets, &ids)
}

// A record naming a vendor this build does not know needs no entry here: every
// rule that reads a credential's presence reaches it through
// `vendor_descriptor`, which answers `None` for such a record and stops before
// the question is asked.

/// AIC-FR-13: which vendor is active, by either route.
///
/// An explicit activation always wins. The implicit resolution applies only to a
/// *set of one* — an author with a single backend is never asked to choose from
/// a set of one — and only while nothing has been activated explicitly. An
/// explicit choice that has stopped verifying resolves to nothing rather than
/// silently handing the role to another vendor: the author chose that one, and
/// quietly running a different agent is worse than running none.
pub fn active_vendor(
    records: &[AgenticRecord],
    explicit: Option<&str>,
    probe: &dyn FileProbe,
    present: &KeyPresence,
) -> Option<String> {
    let verified: Vec<&AgenticRecord> = records
        .iter()
        .filter(|r| {
            vendor_descriptor(&r.vendor).is_some_and(|d| {
                state_of(r, d, probe, key_presence(present, r, d))
                    == IntegrationState::Verified
            })
        })
        .collect();

    if let Some(vendor) = explicit {
        return verified
            .iter()
            .find(|r| r.vendor == vendor)
            .map(|r| r.vendor.clone());
    }
    match verified.as_slice() {
        [only] => Some(only.vendor.clone()),
        _ => None,
    }
}

/// AIC-FR-02: one outbound record per supported vendor, in a stable order,
/// configured or not — so the UI never has to reason about an absent record.
pub fn integrations_from(
    records: &[AgenticRecord],
    explicit_active: Option<&str>,
    probe: &dyn FileProbe,
    present: &KeyPresence,
) -> Vec<AgenticIntegration> {
    let active = active_vendor(records, explicit_active, probe, present);
    VENDORS
        .iter()
        .map(|descriptor| {
            let stored = records
                .iter()
                .find(|r| r.vendor == descriptor.vendor)
                .cloned()
                .unwrap_or_else(|| AgenticRecord::empty(descriptor.vendor));
            let has_key = key_presence(present, &stored, descriptor);
            let state = state_of(&stored, descriptor, probe, has_key);
            // AIC-FR-25: the credential fields follow the credential, not the
            // kind. Codex and OpenCode hold nothing, so they never report a
            // credential as set or as missing.
            let key_state = if !descriptor.holds_credential() {
                KeyState::Unset
            } else if descriptor.requires_oauth_token() {
                // Claude Code's token is mandatory, so "no entry" reads as
                // unavailable once a path is stored and as unset before that —
                // an unconfigured tab is not a broken one.
                let configured = stored.binary_path.as_deref().is_some_and(|p| !p.is_empty());
                match (has_key, configured) {
                    (true, _) => KeyState::Set,
                    (false, true) => KeyState::Unavailable,
                    (false, false) => KeyState::Unset,
                }
            } else {
                match (stored.masked_hint.is_some(), has_key) {
                    (false, _) => KeyState::Unset,
                    (true, true) => KeyState::Set,
                    (true, false) => KeyState::Unavailable,
                }
            };
            AgenticIntegration {
                vendor: descriptor.vendor.to_string(),
                kind: descriptor.kind,
                display_name: descriptor.display_name.to_string(),
                // AIC-FR-25: each kind's fields, and only its own.
                binary_path: match descriptor.kind {
                    VendorKind::Cli => stored.binary_path.clone(),
                    VendorKind::Api => None,
                },
                path_origin: match descriptor.kind {
                    VendorKind::Cli => stored.path_origin,
                    VendorKind::Api => PathOrigin::Unset,
                },
                base_url: match descriptor.kind {
                    VendorKind::Cli => None,
                    VendorKind::Api => stored.base_url.clone(),
                },
                key_state,
                // AIC-FR-25: a vendor that holds nothing reports no hint, even
                // if a hand-edited store put one there.
                masked_hint: if descriptor.holds_credential() {
                    stored.masked_hint.clone()
                } else {
                    None
                },
                key_required: descriptor.key_required,
                state,
                version: stored.version.clone(),
                verified_at: stored.verified_at.clone(),
                // An unconfigured vendor still offers its bundled catalog, so
                // the selectors render before anything is verified.
                models: if stored.models.is_empty() {
                    catalog_models(descriptor)
                } else {
                    stored.models.clone()
                },
                models_origin: if stored.models.is_empty() {
                    ModelsOrigin::Catalog
                } else {
                    stored.models_origin
                },
                selected_model: stored.selected_model.clone(),
                // AIC-FR-10: only the kinds that use a model of their own.
                // AIC-FR-QVWX: an override held against a kind this build does
                // not offer is dropped on read, so a record an older build
                // wrote falls back rather than naming a kind nothing can
                // dispatch. Nothing is written until the next selection.
                model_overrides: offered_kinds_only(&stored.model_overrides),
                // AIC-FR-09: always the descriptor's, never the store's — the
                // levels are a property of the backend's interface.
                reasoning_efforts: declared_efforts(descriptor),
                selected_effort: stored.selected_effort.clone(),
                // AIC-FR-10 / AIC-FR-QVWX: as the model overrides above.
                effort_overrides: offered_kinds_only(&stored.effort_overrides),
                active: active.as_deref() == Some(descriptor.vendor),
            }
        })
        .collect()
}

/// AIC-FR-05: does this binary identify itself as the vendor being verified?
///
/// Verification confirms identity, not merely existence — otherwise pointing
/// Claude Code at `/bin/ls` would "work" until the first real invocation.
///
/// Two ways to pass, because the CLIs disagree about what a version banner is:
///
/// 1. The output names the vendor (`claude 2.1.4`, `codex-cli 0.4.0`). This is
///    the strong signal and holds however the binary was renamed.
/// 2. The output is a *bare* version (`0.4.12`) and the file is named as one of
///    the vendor's executables. A CLI that prints nothing but its version is
///    common, and rejecting it would make the feature unusable for that vendor;
///    requiring the filename keeps `/bin/ls --version` — which prints a banner
///    naming coreutils — from passing as anything.
pub fn identifies_vendor(descriptor: &AgenticVendor, path: &Path, output: &CliOutput) -> bool {
    let text = output.combined().to_lowercase();
    if descriptor
        .identity_markers
        .iter()
        .any(|marker| text.contains(marker))
    {
        return true;
    }
    let named_as_vendor = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|stem| {
            let stem = stem.to_lowercase();
            descriptor
                .executable_names
                .iter()
                .any(|name| stem == *name || stem.starts_with(&format!("{name}-")))
        })
        .unwrap_or(false);
    named_as_vendor && is_bare_version_banner(&text)
}

/// Is this output nothing but a version number (plus incidental whitespace or a
/// leading `v`)? Used only as the second half of the filename fallback above.
pub(super) fn is_bare_version_banner(text: &str) -> bool {
    let condensed: Vec<&str> = text.split_whitespace().collect();
    match condensed.as_slice() {
        [single] => {
            let trimmed = single.trim_start_matches('v');
            !trimmed.is_empty()
                && trimmed
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
                && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// Pull a version out of a banner. Prefers the first dotted-numeric token
/// (`2.1.4` out of `claude 2.1.4 (build 9)`); falls back to the first non-empty
/// line so a CLI with an unusual banner still records *something* rather than
/// reading as unverified.
pub fn extract_version(output: &CliOutput) -> Option<String> {
    let combined = output.combined();
    for token in combined.split_whitespace() {
        let candidate = token
            .trim_start_matches('v')
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        let mut chars = candidate.chars();
        let starts_numeric = chars.next().is_some_and(|c| c.is_ascii_digit());
        if starts_numeric && candidate.contains('.') {
            return Some(candidate.to_string());
        }
    }
    combined
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect())
}

/// AIC-FR-08: read a model list out of a CLI probe's output.
///
/// Deliberately tolerant. The probe is a best-effort enrichment whose failure
/// mode is already handled — an empty result means the bundled catalog is used —
/// so a parser that guesses wrong costs nothing, while one that panics or
/// rejects a whole list over a stray line costs the author real information.
/// Anything that looks like prose (spaces, punctuation a model id would not
/// carry) is skipped.
pub fn parse_probed_models(output: &CliOutput) -> Vec<ModelOption> {
    let mut seen: Vec<ModelOption> = Vec::new();
    for line in output.stdout.lines() {
        let candidate = line.trim().trim_start_matches(['-', '*', '•']).trim();
        if candidate.is_empty() || candidate.contains(char::is_whitespace) {
            continue;
        }
        // A model id is a slug: alphanumerics plus the few separators vendors
        // use. Anything else is a table border, a heading, or prose.
        let plausible = candidate.len() <= 100
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '/' | ':'))
            && candidate.chars().any(|c| c.is_ascii_alphanumeric());
        if !plausible || seen.iter().any(|m| m.id == candidate) {
            continue;
        }
        seen.push(ModelOption::new(candidate, candidate));
    }
    seen
}

/// The directories detection searches (AIC-FR-03): everything on `PATH`, then
/// the platform's conventional install locations.
///
/// Taking `path_var` and `home` as arguments rather than reading the environment
/// keeps the search order testable on a machine that has any of these
/// directories for real.
pub fn detection_directories(path_var: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(raw) = path_var {
        let separator = if cfg!(windows) { ';' } else { ':' };
        dirs.extend(
            raw.split(separator)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    // Conventional locations a GUI application will not have inherited on its
    // `PATH`: a macOS app launched from Finder gets a minimal environment, so a
    // Homebrew or npm-global install is invisible without these.
    let conventional: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];
    dirs.extend(conventional.iter().map(PathBuf::from));
    if let Some(home) = home {
        for suffix in [".local/bin", ".bun/bin", ".npm-global/bin", ".volta/bin"] {
            dirs.push(home.join(suffix));
        }
    }
    dirs.dedup();
    dirs
}

/// AIC-FR-03: the first executable among `names` found in `dirs`, or `None`.
/// Persists nothing and executes nothing.
pub fn detect_in(dirs: &[PathBuf], names: &[&str], probe: &dyn FileProbe) -> Option<String> {
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if probe.exists(&candidate) && probe.is_executable(&candidate) {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// AIC-FR-16: resolve the open project's integration, and say *how* — which is
/// what tells the UI what to write on the line.
pub fn resolve_project(
    records: &[AgenticRecord],
    explicit_active: Option<&str>,
    override_vendor: Option<&str>,
    probe: &dyn FileProbe,
    present: &KeyPresence,
) -> ProjectAgenticIntegration {
    let active = active_vendor(records, explicit_active, probe, present);
    let is_verified = |vendor: &str| {
        records.iter().any(|r| {
            r.vendor == vendor
                && vendor_descriptor(vendor).is_some_and(|d| {
                    state_of(r, d, probe, key_presence(present, r, d))
                        == IntegrationState::Verified
                })
        })
    };

    if let Some(over) = override_vendor {
        if is_verified(over) {
            return ProjectAgenticIntegration {
                vendor: Some(over.to_string()),
                resolution: ProjectResolution::Overridden,
                override_vendor: Some(over.to_string()),
            };
        }
        // The override is recorded but no longer usable. It is deliberately not
        // erased (AIC-FR-14): re-verifying the integration should restore the
        // author's choice rather than silently having lost it.
        return ProjectAgenticIntegration {
            vendor: active,
            resolution: ProjectResolution::OverrideUnavailable,
            override_vendor: Some(over.to_string()),
        };
    }

    if let Some(active) = active {
        return ProjectAgenticIntegration {
            vendor: Some(active),
            resolution: ProjectResolution::Inherited,
            override_vendor: None,
        };
    }

    // Nothing resolves. The distinction the UI needs is whether there is
    // anything to choose between at all: something configured means "pick one",
    // nothing configured means "go and set one up".
    let anything_configured = records.iter().any(|r| {
        vendor_descriptor(&r.vendor).is_some_and(|d| {
            state_of(r, d, probe, key_presence(present, r, d)) != IntegrationState::Unconfigured
        })
    });
    ProjectAgenticIntegration {
        vendor: None,
        resolution: if anything_configured {
            ProjectResolution::NoneSelected
        } else {
            ProjectResolution::NoneConfigured
        },
        override_vendor: None,
    }
}

/// Map a probe failure onto this module's typed error vocabulary (AIC-FR-22).
pub(super) fn agentic_probe_error(e: ProbeError) -> String {
    match e {
        ProbeError::Unreachable(_) => ERR_UNREACHABLE.to_string(),
        ProbeError::Rejected => ERR_REJECTED.to_string(),
        ProbeError::NotExpectedKind => ERR_NOT_AN_AGENT_ENDPOINT.to_string(),
        ProbeError::TimedOut => ERR_TIMED_OUT.to_string(),
    }
}

