//! The vendor descriptors this module knows (`AIC-agentic-integration-config.md` AIC-FR-01).

use super::*;

// ---------------------------------------------------------------------------
// Vendor descriptors (AIC-FR-01)
// ---------------------------------------------------------------------------

/// Which kind of backend a vendor is, and therefore which configuration fields
/// its records carry (AIC-FR-25).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VendorKind {
    #[default]
    Cli,
    Api,
}

/// Everything the application knows about a vendor before the author touches
/// anything (AIC-FR-01). Application data, not author input: no command here
/// edits any of it.
#[derive(Clone, Debug)]
pub struct AgenticVendor {
    pub vendor: &'static str,
    pub kind: VendorKind,
    pub display_name: &'static str,
    /// CLI kind: what detection looks for on `PATH` and in conventional
    /// locations. Empty for API-kind vendors, which have no binary.
    pub executable_names: &'static [&'static str],
    /// CLI kind: passed to the binary to make it say what it is (AIC-FR-04).
    pub version_args: &'static [&'static str],
    /// CLI kind: lowercase substrings that identify the vendor in a version
    /// banner (AIC-FR-05).
    pub identity_markers: &'static [&'static str],
    /// CLI kind: how to ask this installation which models it offers, when the
    /// vendor has such a subcommand at all (AIC-FR-08). `None` means the probe
    /// is unsupported and the bundled catalog is used.
    pub model_probe_args: Option<&'static [&'static str]>,
    /// API kind: the endpoint prefilled in the UI (AIC-FR-23). `None` for
    /// `custom_agent_api`, whose whole purpose is a URL only the author knows.
    pub default_base_url: Option<&'static str>,
    /// API kind: how this vendor expects its key to be presented.
    pub auth: AuthStyle,
    /// API kind: appended to the base URL to reach the model listing.
    pub models_path: &'static str,
    /// Whether this vendor requires a credential of its own (AIC-FR-25).
    ///
    /// The flag follows the *credential*, not the kind. `false` for an API-kind
    /// vendor lets a self-hosted deployment that authenticates nobody verify
    /// with the field empty; `true` on a CLI-kind vendor means it holds an OAuth
    /// token, which today is Claude Code and only Claude Code.
    pub key_required: bool,
    /// The bundled fallback list (AIC-FR-08). May be empty.
    pub model_catalog: &'static [(&'static str, &'static str)],
    /// Fixed per vendor (AIC-FR-09). May be empty, which is what renders no
    /// effort selector at all (`../ui/AII-ai-integrations.md` AII-FR-24).
    pub reasoning_efforts: &'static [(&'static str, &'static str)],
    /// Whether the application can execute this vendor in a pinned container
    /// (AIC-FR-30). CLI-kind alone is not enough: OpenCode is a CLI and is
    /// still false here, because no execution protocol is pinned for it
    /// (`../tools/EAC-execute-agent-cli.md` EAC-FR-04).
    pub container_executable: bool,
    /// CLI kind: where this vendor's own login state lives, for a vendor that
    /// authenticates through a directory rather than through a credential this
    /// module holds (AIC-FR-30). `None` for every vendor but Codex.
    pub config_mount: Option<CliConfigMount>,
}

/// Where a CLI's own login directory is, and where its pinned container
/// expects to find it. Application data: the author supplies neither, and no
/// caller may substitute either (AIC-FR-30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliConfigMount {
    /// Resolved against the machine user's home directory.
    pub host_home_relative: &'static str,
    /// The absolute path inside the container. Must match the home directory of
    /// the non-root user the vendor image declares
    /// (`../infra/AVI-agent-vendor-images.md` AVI-FR-03).
    pub container_target: &'static str,
}

/// AIC-FR-10: the closed set of turn kinds an override may be held against.
///
/// The same four identifiers `crate::tools::agent_exec::TurnKind` names, which
/// is where a turn's kind is decided; this module only holds a selection against
/// one.
/// AIC-FR-10: the closed set an override may be held against.
///
/// AIC-FR-QVWX: an override stored under a kind outside this set is dropped on
/// read, exactly as AIC-FR-11 drops a model the refreshed list no longer
/// offers, so a record written by an older build falls back rather than
/// resolving a kind this build cannot dispatch.
pub(super) const TURN_KINDS: &[&str] = &["work", "review", "semantic_rebase"];

/// AIC-FR-QVWX: the overrides whose turn kind this build still offers.
pub(super) fn offered_kinds_only(
    stored: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    stored
        .iter()
        .filter(|(kind, _)| TURN_KINDS.contains(&kind.as_str()))
        .map(|(kind, value)| (kind.clone(), value.clone()))
        .collect()
}

pub(super) const EFFORTS_LOW_MED_HIGH: &[(&str, &str)] =
    &[("low", "Low"), ("medium", "Medium"), ("high", "High")];

/// AIC-FR-09: the five levels Claude Code's pinned CLI accepts, as
/// `../infra/CCP-claude-code-cli-protocol.md` CCP-FR-05 states them.
///
/// Declaring fewer would put a level the executor can generate and the CLI would
/// honour beyond the author's reach — a setting nobody can choose and nothing
/// reports as missing.
pub(super) const EFFORTS_CLAUDE_CODE: &[(&str, &str)] = &[
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "Extra high"),
    ("max", "Max"),
];

/// The five supported vendors, in the stable order `list_agentic_integrations`
/// returns them (AIC-FR-02): the three CLIs first, then the two API agents.
///
/// The catalogs and effort levels below are the *bundled fallback* — the thing a
/// probe replaces when a backend can enumerate its own models (AIC-FR-08). They
/// are the one part of this module that tracks a moving external target, so they
/// are grouped here rather than scattered.
pub const VENDORS: &[AgenticVendor] = &[
    AgenticVendor {
        vendor: "claude_code",
        kind: VendorKind::Cli,
        display_name: "Claude Code",
        executable_names: &["claude"],
        version_args: &["--version"],
        identity_markers: &["claude"],
        // Claude Code has no model-enumeration subcommand, so the catalog is
        // authoritative for it rather than a fallback.
        model_probe_args: None,
        default_base_url: None,
        auth: AuthStyle::Bearer,
        models_path: "",
        // AIC-FR-26: the one CLI vendor that holds a credential. It is expected
        // to run in an isolated container with no session of its own, so the
        // OAuth token is what makes the integration usable at all.
        key_required: true,
        model_catalog: &[("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")],
        reasoning_efforts: EFFORTS_CLAUDE_CODE,
        container_executable: true,
        config_mount: None,
    },
    AgenticVendor {
        vendor: "codex",
        kind: VendorKind::Cli,
        display_name: "Codex",
        executable_names: &["codex"],
        version_args: &["--version"],
        identity_markers: &["codex"],
        model_probe_args: None,
        default_base_url: None,
        auth: AuthStyle::Bearer,
        models_path: "",
        key_required: false,
        model_catalog: &[
            ("gpt-5.1-codex", "GPT-5.1 Codex"),
            ("gpt-5.1", "GPT-5.1"),
            ("gpt-5", "GPT-5"),
        ],
        reasoning_efforts: EFFORTS_LOW_MED_HIGH,
        container_executable: true,
        config_mount: Some(CliConfigMount {
            host_home_relative: ".codex",
            container_target: "/home/agent/.codex",
        }),
    },
    AgenticVendor {
        vendor: "opencode",
        kind: VendorKind::Cli,
        display_name: "OpenCode",
        executable_names: &["opencode"],
        version_args: &["--version"],
        identity_markers: &["opencode"],
        // OpenCode is provider-agnostic: which models exist depends entirely on
        // the author's configured providers, so the installation is the only
        // thing that can answer and there is nothing sensible to bundle.
        model_probe_args: Some(&["models"]),
        default_base_url: None,
        auth: AuthStyle::Bearer,
        models_path: "",
        key_required: false,
        model_catalog: &[],
        reasoning_efforts: &[],
        container_executable: false,
        config_mount: None,
    },
    AgenticVendor {
        vendor: "claude_agent_api",
        kind: VendorKind::Api,
        display_name: "Claude Agent API",
        executable_names: &[],
        version_args: &[],
        identity_markers: &[],
        model_probe_args: None,
        default_base_url: Some("https://api.anthropic.com/v1"),
        auth: AuthStyle::AnthropicApiKey,
        models_path: "/models",
        key_required: true,
        model_catalog: &[
            ("claude-opus-5", "Claude Opus 5"),
            ("claude-sonnet-5", "Claude Sonnet 5"),
        ],
        reasoning_efforts: EFFORTS_LOW_MED_HIGH,
        container_executable: false,
        config_mount: None,
    },
    AgenticVendor {
        vendor: "custom_agent_api",
        kind: VendorKind::Api,
        display_name: "Custom agent API",
        executable_names: &[],
        version_args: &[],
        identity_markers: &[],
        model_probe_args: None,
        // AIC-FR-23: no default, because this vendor exists precisely so that a
        // self-hosted, proxied, or otherwise relocated deployment of the same
        // protocol can be pointed at by URL.
        default_base_url: None,
        auth: AuthStyle::AnthropicApiKey,
        models_path: "/models",
        // A deployment on the author's own machine may authenticate nobody.
        key_required: false,
        model_catalog: &[],
        reasoning_efforts: EFFORTS_LOW_MED_HIGH,
        container_executable: false,
        config_mount: None,
    },
];

impl AgenticVendor {
    /// AIC-FR-26: does this vendor's configuration payload carry an OAuth token?
    ///
    /// Derived rather than declared separately, because "a CLI-kind vendor that
    /// requires a credential" is exactly what an OAuth-token vendor is — a
    /// second flag saying the same thing could drift out of step with the first.
    /// Claude Code is the only one: it is expected to run in an isolated
    /// container carrying no session of its own, while Codex and OpenCode each
    /// authenticate themselves through a mechanism this module neither performs
    /// nor observes (AIC-FR-20).
    pub fn requires_oauth_token(&self) -> bool {
        self.kind == VendorKind::Cli && self.key_required
    }

    /// Whether this vendor holds a credential in the keychain at all, of either
    /// sort (AIC-FR-20). What decides whether `key_state` and `masked_hint` mean
    /// anything for its records (AIC-FR-25).
    pub fn holds_credential(&self) -> bool {
        self.kind == VendorKind::Api || self.requires_oauth_token()
    }
}

/// Look up a vendor descriptor by id.
pub fn vendor_descriptor(vendor: &str) -> Option<&'static AgenticVendor> {
    VENDORS.iter().find(|v| v.vendor == vendor)
}

pub(super) fn catalog_models(descriptor: &AgenticVendor) -> Vec<ModelOption> {
    descriptor
        .model_catalog
        .iter()
        .map(|(id, label)| ModelOption::new(id, label))
        .collect()
}

pub(super) fn declared_efforts(descriptor: &AgenticVendor) -> Vec<EffortOption> {
    descriptor
        .reasoning_efforts
        .iter()
        .map(|(id, label)| EffortOption::new(id, label))
        .collect()
}

