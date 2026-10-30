//! AI API integrations — `specifications/core/AAP-ai-api-integrations.md`.
//!
//! The backend home for the conversational AI endpoints Synthesis calls directly
//! over HTTP. It holds one record per supported provider — Anthropic, OpenAI,
//! OpenRouter, and a Custom OpenAI-compatible endpoint — each carrying the base
//! URL to call, an API key kept in the OS credential store, the models that
//! endpoint offers, and the model the author chose, and it records which single
//! provider is active and which provider a given project overrides to.
//!
//! Three properties shape the module:
//!
//! - **Verification is what commits a configuration** (AAP-FR-06). The fields in
//!   the UI hold candidates; nothing reaches the registry until the endpoint has
//!   answered. A failed verification persists *nothing*, so a provider that was
//!   working is never degraded by an attempt against a different URL or key.
//! - **A key lives only in the keychain** (AAP-FR-07). It is written once by a
//!   successful verification and never crosses the IPC boundary: `masked_hint`
//!   is the only key-derived text any command returns (AAP-FR-08).
//! - **Every provider here is conversational** (AAP-FR-21). No descriptor,
//!   record, or command carries a modality, so this module cannot describe a
//!   voice, embedding, or image endpoint — a feature asking for one gets a
//!   conversational integration or nothing.
//!
//! The `custom` provider is the company LLM Gateway: a host the author supplies
//! without `/v1`, and a bearer secret that is always required (AAP-FR-04,
//! AAP-FR-KRVT). Its models come from `GET <base>/v1/models` (AAP-FR-MDLQ) and
//! its calls go through the Rig OpenAI-compatible adapter (AAP-FR-ADPX).
//!
//! Both outside dependencies — the network and the credential store — are behind
//! traits (`EndpointProber`, `SecretStore`) so every rule below is unit-testable
//! without either. CI has no keychain and no outbound network, and a test that
//! needed one would either fail there or be deleted until it passed.
//!
//! Every command reports itself to `crate::logging` under the `ai` domain, and
//! under `remote` as well where it reaches an endpoint — a verification that
//! failed and a provider whose key has gone missing are the two things an author
//! asks about, and neither leaves any other trace. What the records carry is
//! bounded by AAP-FR-07 exactly as the store and the IPC boundary are: a
//! provider name, a base URL, a model id, a state, a count — never a key, and
//! never so much as the masked hint, since nothing is gained by putting even
//! four characters of a credential into a file a user attaches to a bug report.
//!
//! The base URL is the one of those that has to be *made* safe rather than
//! merely being safe: it is the author's own field, a verification reports it
//! before anything has validated it, and `normalize_base_url`'s refusal of a
//! credential smuggled into a URL comes later, inside the verification, which is
//! too late for a record already written. Every URL that reaches a record here
//! therefore goes through `ai_shared::loggable_base_url` first.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ai_openrouter::OpenRouterProber;
use crate::ai_shared::{
    mask_hint, normalize_base_url, redacted, AuthStyle, BaseUrlError, EndpointProber,
    HttpEndpointProber, KeyPresence, ModelMode, ModelOption, ModelsFormat, ModelsOrigin,
    ProbeError, ProbeRequest, SecretStore,
};
use crate::global_settings::{now_iso8601, GlobalSettingsStore};
use crate::log_fields;
use crate::logging::{self, Domain, Fields, LogBuffer, LogSink};
use crate::project::ProjectState;

// ---------------------------------------------------------------------------
// Typed errors (AAP-FR-05 / FR-11 / FR-13 / FR-17 / FR-20)
// ---------------------------------------------------------------------------
//
// The codebase's error channel is `Result<_, String>`, so a "typed" error is a
// stable string the frontend matches on. `src/types.ts` mirrors this list, and a
// silent divergence would leave the UI unable to tell an unreachable host from a
// refused key.

/// Verify was asked to check an empty base URL (AAP-FR-05).
pub const ERR_BASE_URL_EMPTY: &str = "base_url_empty";
/// The base URL is not a well-formed absolute URL (AAP-FR-05).
pub const ERR_BASE_URL_INVALID: &str = "base_url_invalid";
/// No key was supplied where the provider requires one (AAP-FR-04 / FR-05).
pub const ERR_KEY_MISSING: &str = "key_missing";
/// The host could not be reached at all (AAP-FR-05).
pub const ERR_UNREACHABLE: &str = "unreachable";
/// The endpoint answered and refused the credentials (AAP-FR-05).
pub const ERR_REJECTED: &str = "rejected";
/// The URL answered, but not as a conversational API (AAP-FR-05).
pub const ERR_NOT_AN_AI_ENDPOINT: &str = "not_an_ai_endpoint";
/// It did not answer within the bounded timeout (AAP-FR-05).
pub const ERR_TIMED_OUT: &str = "timed_out";
/// The OS credential store would not answer (AAP-FR-20).
pub const ERR_KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
/// A model id absent from the record's current list (AAP-FR-11).
pub const ERR_UNKNOWN_MODEL: &str = "unknown_model";
/// A reasoning choice was made while the provider is on its own model default,
/// so no model's capability is known yet (AAP-FR-27).
pub const ERR_NO_MODEL_SELECTED: &str = "no_model_selected";
/// The selected model declares no reasoning at all (AAP-FR-27).
pub const ERR_REASONING_UNSUPPORTED: &str = "reasoning_unsupported";
/// An effort absent from the selected model's own ladder (AAP-FR-26 / FR-27).
pub const ERR_UNKNOWN_EFFORT: &str = "unknown_effort";
/// Reasoning was switched off for a model that cannot stop reasoning
/// (AAP-FR-27).
pub const ERR_REASONING_MANDATORY: &str = "reasoning_mandatory";
/// Activation, or an override, naming a provider that is not verified
/// (AAP-FR-13 / AAP-FR-17).
pub const ERR_NOT_VERIFIED: &str = "not_verified";
/// No provider carries the requested id.
pub const ERR_UNKNOWN_PROVIDER: &str = "unknown_provider";
/// AAP-FR-FGNK: a turn timeout outside 30,000 to 3,600,000 milliseconds.
pub const ERR_TURN_TIMEOUT_OUT_OF_RANGE: &str = "turn_timeout_out_of_range";
/// An override can only be set while a project is open.
pub const ERR_NO_PROJECT: &str = "no project is open";
/// `resolve_ai_api_call` was asked for an integration when none is configured at
/// all (AAP-FR-19).
pub const ERR_NONE_CONFIGURED: &str = "none_configured";
/// `resolve_ai_api_call` was asked when providers exist but none is chosen
/// (AAP-FR-19).
pub const ERR_NONE_SELECTED: &str = "none_selected";
/// A selection was validated against a provider that has never been configured
/// at all — no base URL, nothing (AAP-FR-33). Distinct from `not_verified`,
/// which is a provider that *is* configured but has not passed a check: the
/// two call for different corrections, and a caller storing a selection of its
/// own (`crate::agents`, AGR-FR-16) reports them as different rows.
pub const ERR_PROVIDER_UNCONFIGURED: &str = "provider_unconfigured";
/// The provider's keychain entry is gone, so an endpoint cannot be handed out
/// with the key it needs (AAP-FR-34). Distinct from `keychain_unavailable`,
/// which is the store declining to answer at all.
pub const ERR_KEY_UNAVAILABLE: &str = "key_unavailable";
/// AAP-FR-ITBL: the value is not a whole number from 1 to `custom_limits::TOKEN_LIMIT_MAX`.
pub const ERR_TOKEN_LIMIT_INVALID: &str = "token_limit_invalid";
/// AAP-FR-ITBL: the provider is not Custom, so it holds no max output tokens.
pub const ERR_LIMIT_UNSUPPORTED: &str = "limit_unsupported";

/// AAP-FR-36: the service name of the **earlier** one-entry-per-provider
/// keyring layout.
///
/// Nothing writes to it any more — every provider key lives in the application
/// secret vault (`ASV-application-secret-vault.md` ASV-FR-01) — but it is what
/// a migration candidate names, so a machine that stored keys under the old
/// layout has them adopted into the one entry on the next launch.
pub const LEGACY_KEYCHAIN_SERVICE: &str = "com.synthesis.ai-api-provider";

/// AAP-FR-07: the vault namespace this module owns
/// (`ASV-application-secret-vault.md` ASV-FR-03). A provider's key is addressed
/// at `["ai_api", "providers", <provider_id>]` and nowhere else.
pub const VAULT_NAMESPACE: &[&str] = &["ai_api", "providers"];

// ---------------------------------------------------------------------------
// Provider descriptors (AAP-FR-01)
// ---------------------------------------------------------------------------

/// Everything the application knows about a provider before the author touches
/// anything (AAP-FR-01). Application data, not author input: no command here
/// edits any of it.
#[derive(Clone, Debug)]
pub struct AiApiProvider {
    pub provider: &'static str,
    pub display_name: &'static str,
    /// The endpoint prefilled in the UI (AAP-FR-03). `None` for `custom`, whose
    /// whole purpose is a URL only the author knows.
    pub default_base_url: Option<&'static str>,
    /// How this provider expects its key to be presented.
    pub auth: AuthStyle,
    /// Appended to the base URL to reach the model listing.
    pub models_path: &'static str,
    /// How strictly the listing is read: `custom` alone must return a valid
    /// gateway model list (AAP-FR-MDLQ).
    pub models_format: ModelsFormat,
    /// Whether a key is required (AAP-FR-04). `true` for every provider.
    pub key_required: bool,
    /// The bundled fallback list (AAP-FR-10). May be empty.
    ///
    /// Each entry is the model's id, its label, and what this application's own
    /// shipped descriptor declares about its **image input** (AAP-FR-35). The
    /// third field settles the capability where the provider's probe reported
    /// nothing about input modalities; a model neither declares anything about
    /// carries `false`, an undeclared capability being treated as absent rather
    /// than guessed at.
    pub model_catalog: &'static [(&'static str, &'static str, bool)],
}

/// The four supported providers, in the stable order `list_ai_api_integrations`
/// returns them (AAP-FR-02).
///
/// The catalogs are the *bundled fallback* — what a probe replaces when an
/// endpoint enumerates its own models (AAP-FR-10). They are the one part of this
/// module that tracks a moving external target, so they are grouped here rather
/// than scattered. `openrouter` and `custom` bundle nothing: which models exist
/// depends entirely on the account or the deployment, so the endpoint is the
/// only thing that can answer.
pub const PROVIDERS: &[AiApiProvider] = &[
    AiApiProvider {
        provider: "openrouter",
        display_name: "OpenRouter",
        default_base_url: Some("https://openrouter.ai/api/v1"),
        auth: AuthStyle::Bearer,
        models_path: "/models",
        models_format: ModelsFormat::Lenient,
        key_required: true,
        model_catalog: &[],
    },
    AiApiProvider {
        provider: "anthropic",
        display_name: "Anthropic",
        default_base_url: Some("https://api.anthropic.com/v1"),
        auth: AuthStyle::AnthropicApiKey,
        models_path: "/models",
        models_format: ModelsFormat::Lenient,
        key_required: true,
        model_catalog: &[
            ("claude-opus-5-5", "Claude Opus 5.5", true),
            ("claude-sonnet-5-5", "Claude Sonnet 5.5", true),
            ("claude-haiku-5-5", "Claude Haiku 5.5", true),
            // AAP-FR-35: the earlier generation stays, because a probe still
            // lists it and this catalog is the only source of its image input.
            ("claude-opus-5", "Claude Opus 5", true),
            ("claude-sonnet-5", "Claude Sonnet 5", true),
            ("claude-haiku-4-5", "Claude Haiku 4.5", true),
        ],
    },
    AiApiProvider {
        provider: "openai",
        display_name: "OpenAI",
        default_base_url: Some("https://api.openai.com/v1"),
        auth: AuthStyle::Bearer,
        models_path: "/models",
        models_format: ModelsFormat::Lenient,
        key_required: true,
        model_catalog: &[("gpt-5.1", "GPT-5.1", true), ("gpt-5", "GPT-5", true)],
    },
    AiApiProvider {
        provider: "custom",
        display_name: "Custom",
        // AAP-FR-03: no default at all — this provider exists precisely for a
        // gateway host only the author knows.
        default_base_url: None,
        auth: AuthStyle::Bearer,
        // AAP-FR-KRVT / AAP-FR-MDLQ: the gateway root has no `/v1`, so the
        // listing route carries it.
        models_path: "/v1/models",
        models_format: ModelsFormat::Gateway,
        // AAP-FR-04: the gateway secret is required.
        key_required: true,
        model_catalog: &[],
    },
];

/// Look up a provider descriptor by id.
pub fn provider_descriptor(provider: &str) -> Option<&'static AiApiProvider> {
    PROVIDERS.iter().find(|p| p.provider == provider)
}

fn catalog_models(descriptor: &AiApiProvider) -> Vec<ModelOption> {
    descriptor
        .model_catalog
        .iter()
        .map(|(id, label, images)| {
            // A catalog entry is a declaration of this application's own, so it
            // stands as one: nothing further consults the catalog for a model
            // the catalog itself produced.
            ModelOption::new(id, label).with_declared_image_input(Some(*images))
        })
        .collect()
}

/// AAP-FR-35: what the shipped descriptor declares about one model's image
/// input, or `None` where its catalog does not name that model at all.
///
/// The second of the requirement's three steps: it settles the capability where
/// the provider's probe reported nothing about input modalities, and it says
/// nothing about a model it has never heard of — which leaves `false`, an
/// undeclared capability being absent rather than guessed at.
fn catalog_image_input(descriptor: &AiApiProvider, model_id: &str) -> Option<bool> {
    descriptor.model_catalog.iter().find(|(id, _, _)| *id == model_id).map(|(_, _, images)| *images)
}

/// AAP-FR-35: the three-step capability decision, applied to a whole model list.
///
/// The probe's own declaration wins where it made one; the shipped catalog
/// settles what the probe left unsaid; and a model neither declares anything
/// about carries `false`. Applied at the one place a model list is produced, so
/// `list_ai_api_integrations`, `resolve_ai_api_endpoint`, and the selector
/// cannot disagree about what a model takes.
fn settle_image_input(
    mut models: Vec<ModelOption>,
    descriptor: &AiApiProvider,
) -> Vec<ModelOption> {
    for model in &mut models {
        let catalog = catalog_image_input(descriptor, &model.id);
        *model = std::mem::take(model).with_catalog_image_input(catalog);
    }
    models
}

/// A record's *current* model list: the probed one after a verification, the
/// bundled catalog before it. The single definition of "the models this
/// provider offers", so `set_model_impl`, the outbound integration, and
/// `validate_ai_api_selection` (AAP-FR-33) cannot disagree about whether a
/// model exists — a disagreement would let a selection be stored against a
/// model the selector never showed.
fn available_models(record: &AiApiRecord, descriptor: &AiApiProvider) -> Vec<ModelOption> {
    let models = if record.models.is_empty() {
        catalog_models(descriptor)
    } else {
        // AAP-FR-35: a persisted record carries the probe's answer without its
        // provenance, so the shipped catalog is consulted again here for every
        // model the store did not mark image-capable. This is the one place a
        // stored list becomes the list callers read, so it is the one place the
        // three-step decision has to be made.
        settle_image_input(record.models.clone(), descriptor)
    };
    // AAP-FR-WSIC: every Custom model offers the fixed reasoning set. It is
    // given here, where callers read the list, so a list stored before the set
    // existed offers it too.
    if descriptor.provider == custom_limits::CUSTOM {
        return custom_limits::with_custom_reasoning(models);
    }
    models
}

// ---------------------------------------------------------------------------
// Wire and persisted types
// ---------------------------------------------------------------------------

/// Whether a record's key is readable (AAP-FR-09).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Set,
    #[default]
    Unset,
    Unavailable,
}

/// A record's state (AAP-FR-09). Derived at read time, never persisted: a
/// keychain wiped while the app was closed must read as such on the next launch
/// without anything having written that fact down.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationState {
    #[default]
    Unconfigured,
    Verified,
    KeyUnavailable,
}

/// What the author asked for, stored per provider beside the chosen model
/// (AAP-FR-28).
///
/// Three shapes rather than a bare string, because "reason at `high`" and
/// "reason at whatever depth you like" are different requests and a model that
/// offers no ladder can only express the second.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReasoningChoice {
    /// Reason as little as the model permits.
    Off,
    /// Reason, at the model's own depth.
    On,
    /// Reason at a named level from the model's own ladder.
    Effort { effort: String },
}

/// A reasoning choice as one short log field.
///
/// Capability data and the depth the author asked for — nothing derived from a
/// key and nothing a participant wrote — so it is safe to record (AAP
/// non-functional requirements), and it is what explains a call whose depth
/// surprised the author. Lives here beside the shape rather than at each emit
/// site, so `crate::agent_conversations` spells a choice the same way this
/// module does.
pub fn reasoning_label(choice: Option<&ReasoningChoice>) -> String {
    match choice {
        // AAP-FR-31: a null choice sends nothing at all, leaving the model to
        // its own default — which is not the same as asking it not to reason.
        None => "default".to_string(),
        Some(ReasoningChoice::Off) => "off".to_string(),
        Some(ReasoningChoice::On) => "on".to_string(),
        Some(ReasoningChoice::Effort { effort }) => format!("effort:{effort}"),
    }
}

/// AAP-FR-27: whether a model can honour a choice, which is the whole point of
/// carrying the descriptor to the UI in the first place.
///
/// Pure, and the single place the rule lives: `set_ai_api_reasoning` refuses on
/// it, and AAP-FR-29 re-runs it after a model change or a re-verification to
/// decide whether a stored choice survives. Two callers with two copies of this
/// rule would eventually disagree, and the disagreement would be a choice
/// stored that the model then ignores.
pub fn reasoning_error(
    model: Option<&ModelOption>,
    choice: &ReasoningChoice,
) -> Option<&'static str> {
    // No model selected means no capability is known: the provider is on its own
    // default, and which model that resolves to is the provider's business.
    let model = model?;
    let Some(reasoning) = model.reasoning.as_ref() else {
        return Some(ERR_REASONING_UNSUPPORTED);
    };
    match choice {
        ReasoningChoice::Off if reasoning.mandatory => Some(ERR_REASONING_MANDATORY),
        ReasoningChoice::Effort { effort } if !reasoning.offers_effort(effort) => {
            Some(ERR_UNKNOWN_EFFORT)
        }
        _ => None,
    }
}

/// Whether a stored choice still stands against the model now selected
/// (AAP-FR-29).
///
/// AAP-FR-WSIC: a Custom choice is checked against the fixed set, with or
/// without a selected model, because the set is the same for every model.
fn reasoning_survives(
    provider: &str,
    models: &[ModelOption],
    selected: Option<&str>,
    choice: &ReasoningChoice,
) -> bool {
    if provider == custom_limits::CUSTOM {
        return custom_limits::custom_reasoning_error(choice).is_none();
    }
    let model = selected.and_then(|id| models.iter().find(|m| m.id == id));
    model.is_some() && reasoning_error(model, choice).is_none()
}

/// The persisted half of a provider — what `synthesis.toml` carries
/// (`GSS-global-settings-storage.md` GSS-FR-27).
///
/// `#[serde(default)]` matters: a store written before a field existed must
/// still load rather than sending the whole file through GSS-FR-13's
/// repair-to-defaults, which would wipe recents and every registry.
///
/// Field order is load-bearing for TOML: every scalar is declared before
/// `models`, which serialises as an array of tables.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AiApiRecord {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The last four characters of the key, which is the only key-derived text
    /// that reaches this store (AAP-FR-07). `None` for an endpoint configured
    /// with no key at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// `None` means the provider's own default, not "unconfigured" (AAP-FR-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_model: Option<String>,
    pub models_origin: ModelsOrigin,
    /// AAP-FR-FGNK: how long one conversation turn on this provider may run, in
    /// milliseconds. `None` leaves the bound to the project's execution timeout
    /// or to the default (`CVL-conversation-loop.md` CVL-FR-16). A scalar, so it
    /// is declared before the table below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_timeout_ms: Option<u64>,
    /// AAP-FR-AYKU: the Custom provider's max output tokens. `None` leaves the
    /// loop's default (`CVL-conversation-loop.md` CVL-FR-FHFM).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// AAP-FR-AYKU: the provider's max context size, for every provider. It
    /// wins over a detected context size (AAP-FR-RANS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_size: Option<u64>,
    /// `None` means the selected model's own default (AAP-FR-27). Serialises as
    /// a table, so it sits after every scalar and before the array-of-tables
    /// below — TOML assigns a bare `key = value` to the most recently opened
    /// table, so a scalar declared after this one would land inside it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_reasoning: Option<ReasoningChoice>,
    pub models: Vec<ModelOption>,
}

impl AiApiRecord {
    fn empty(provider: &str) -> Self {
        Self { provider: provider.to_string(), ..Default::default() }
    }
}

/// The single representation of a provider that crosses the IPC boundary
/// (AAP-FR-08). Carries no key material beyond `masked_hint`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AiApiIntegration {
    pub provider: String,
    pub display_name: String,
    pub base_url: Option<String>,
    pub key_state: KeyState,
    pub masked_hint: Option<String>,
    pub key_required: bool,
    pub state: IntegrationState,
    pub verified_at: Option<String>,
    pub models: Vec<ModelOption>,
    pub models_origin: ModelsOrigin,
    pub selected_model: Option<String>,
    /// `None` means the selected model's own default (AAP-FR-27).
    pub selected_reasoning: Option<ReasoningChoice>,
    /// AAP-FR-FGNK: `None` means the project's execution timeout, or the
    /// default.
    pub turn_timeout_ms: Option<u64>,
    /// AAP-FR-AYKU: Custom only. `None` means the loop's default limit.
    pub max_output_tokens: Option<u64>,
    /// AAP-FR-AYKU: every provider. `None` means no override.
    pub max_context_size: Option<u64>,
    pub active: bool,
}

/// A provider's identity, configuration state, and model catalogue — the part
/// of `AiApiIntegration` that owes nothing to the OS keychain.
///
/// `AiApiIntegration` exists to describe a *credential*: its `key_state`,
/// `masked_hint`, and `state` are all answers to "can this key be read right
/// now", so building one probes the keychain per record (AAP non-functional
/// requirements). That is the right shape for the AI API settings level, which
/// is where an author manages keys.
///
/// It is the wrong shape for every surface that only wants to *name* a model or
/// know which providers can carry an agent. Those are the agent surfaces, and
/// `AGR-FR-16` and `AGR-FR-16` are explicit that they read no key — the same
/// rule `availability_of` follows by validating with the key assumed present.
/// Serving them from `AiApiIntegration` would make listing personas prompt for
/// the keychain, which is both a lie about what is being read and, on a machine
/// whose keychain asks before answering, a prompt to see a list of names.
///
/// So this carries `state` computed exactly as `availability_of` computes it —
/// `key_present = true` — which is what keeps the frontend's derivation of an
/// agent's availability identical to the backend's rather than merely similar.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AiApiCatalog {
    pub provider: String,
    pub display_name: String,
    /// Computed with the key assumed present, so it never reports a *readable*
    /// key's absence — a locked or wiped keychain reads `Verified` here.
    ///
    /// Not narrowed to two variants, though: `state_of`'s last arm returns
    /// `KeyUnavailable` for a record that names an endpoint but has never held
    /// a key for a provider that requires one — a half-written or hand-edited
    /// store, which no key could rescue and which `key_present` therefore does
    /// not speak to. It survives here deliberately, because both sides map it
    /// the same way: `availability_of` turns it into `provider_unverified`, and
    /// so does the frontend's derivation. Collapsing it to `Unconfigured` would
    /// be the one thing that breaks that agreement.
    pub state: IntegrationState,
    pub models: Vec<ModelOption>,
}

/// How the open project resolved its API integration (AAP-FR-16) — the value
/// that tells the UI what to say.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectResolution {
    Overridden,
    Inherited,
    OverrideUnavailable,
    NoneSelected,
    #[default]
    NoneConfigured,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectAiApiIntegration {
    /// The provider actually in effect, or `None` when nothing resolves.
    pub provider: Option<String>,
    pub resolution: ProjectResolution,
    /// The recorded override, whether or not it still resolves.
    pub override_provider: Option<String>,
}

/// What an actual call needs (AAP-FR-19). Returned only by
/// `resolve_ai_api_call`, which is not reachable from the frontend.
///
/// `Debug` is hand-rolled below rather than derived, because this shape carries
/// a cleartext key (AAP-FR-07).
#[derive(Clone, PartialEq, Eq)]
pub struct AiApiCall {
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model_id: Option<String>,
    /// AAP-FR-31: `None` sends nothing, leaving the model to its own default.
    pub reasoning: Option<ReasoningChoice>,
    /// AAP-FR-35: whether the **exact endpoint that will serve this call** takes
    /// image content.
    ///
    /// The flag of the model that was resolved rather than of the provider in
    /// general — two models of one provider differ in it freely — so the caller
    /// about to build a request learns what the model in front of it takes
    /// (per `AGC-agent-conversations.md` AGC-FR-36). False where nothing
    /// declared it, an undeclared capability being absent rather than guessed
    /// at.
    pub accepts_image_input: bool,
    /// AAP-FR-RTMZ: the route metadata of the resolved model. `None` means both
    /// routes; only a Custom gateway model carries a value.
    pub model_mode: Option<ModelMode>,
    /// AAP-FR-TXNM: the provider's stored turn timeout in milliseconds, which
    /// bounds a conversation turn on it (`CVL-conversation-loop.md`
    /// CVL-FR-16). Filled by `resolve_ai_api_endpoint` alone; `None` where the
    /// provider stores none.
    pub turn_timeout_ms: Option<u64>,
    /// AAP-FR-HFQZ: the effective output limit. `None` leaves the loop's
    /// default (`CVL-conversation-loop.md` CVL-FR-FHFM).
    pub max_output_tokens: Option<u64>,
    /// AAP-FR-RANS: the effective context size, which is the conversation
    /// loop's token budget (CVL-FR-LCXM). Never absent, and never sent to the
    /// host.
    pub context_size: u64,
}

impl std::fmt::Debug for AiApiCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiApiCall")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("api_key", &redacted(&self.api_key))
            .field("model_id", &self.model_id)
            .field("reasoning", &self.reasoning)
            .field("accepts_image_input", &self.accepts_image_input)
            .field("model_mode", &self.model_mode)
            .field("turn_timeout_ms", &self.turn_timeout_ms)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("context_size", &self.context_size)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

/// The module's collaborators, as Tauri-managed state. The registry itself is
/// not here — it lives in the user-global store (GSS-FR-27), which
/// `GlobalSettingsStore` owns.
pub struct AiApiIntegrations {
    prober: Box<dyn EndpointProber>,
    /// AAP-FR-QRAX: how a Custom verification asks a host for each model's
    /// detected values. `NoDetection` unless set, so a test sends no request.
    detector: Box<dyn detection::ModelDetector>,
    /// AAP-FR-23: `openrouter` alone is reached through its SDK, because it is
    /// the only provider whose listing reports per-model reasoning. `None`
    /// routes it through `prober` like any other, which is what tests do.
    openrouter_prober: Option<Box<dyn EndpointProber>>,
    secrets: Box<dyn SecretStore>,
    /// Serialises read-modify-write cycles over the registry, so two concurrent
    /// `invoke`s cannot interleave and silently discard one another's record.
    write_lock: Mutex<()>,
}

impl Default for AiApiIntegrations {
    fn default() -> Self {
        Self::new(
            Box::new(HttpEndpointProber),
            Box::new(crate::secret_vault::VaultSecrets::new(
                crate::secret_vault::global(),
                VAULT_NAMESPACE,
            )),
        )
        .with_openrouter_prober(Box::new(OpenRouterProber))
        .with_detector(Box::new(detection::HttpModelDetector::new()))
    }
}

impl AiApiIntegrations {
    pub fn new(prober: Box<dyn EndpointProber>, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            prober,
            detector: Box::new(detection::NoDetection),
            openrouter_prober: None,
            secrets,
            write_lock: Mutex::new(()),
        }
    }

    pub fn with_detector(mut self, detector: Box<dyn detection::ModelDetector>) -> Self {
        self.detector = detector;
        self
    }

    pub fn with_openrouter_prober(mut self, prober: Box<dyn EndpointProber>) -> Self {
        self.openrouter_prober = Some(prober);
        self
    }

    /// AAP-FR-23: which client reaches this provider. The dependency's blast
    /// radius is exactly this match — every other provider keeps the generic
    /// path, so an SDK that breaks cannot cost the application the other three.
    fn prober_for(&self, provider: &str) -> &dyn EndpointProber {
        match (provider, self.openrouter_prober.as_ref()) {
            ("openrouter", Some(prober)) => &**prober,
            _ => &*self.prober,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// AAP-FR-09: a record's state, derived from the keychain rather than stored.
///
/// `key_present` is a `bool` rather than a store handle so the rule stays pure
/// and so no caller of this function is ever handed a key.
pub fn state_of(record: &AiApiRecord, key_present: bool, key_required: bool) -> IntegrationState {
    match record.base_url.as_deref() {
        None | Some("") => IntegrationState::Unconfigured,
        // A record verified with a key must still have one.
        Some(_) if record.masked_hint.is_some() => {
            if key_present {
                IntegrationState::Verified
            } else {
                IntegrationState::KeyUnavailable
            }
        }
        // No hint at all. Every provider requires a key (AAP-FR-04), so this is
        // a record that cannot work — a half-written or
        // hand-edited store — and calling it `verified` would let it be
        // activated and then hand `resolve_ai_api_call` an endpoint with no
        // credential, turning a configuration problem into a 401 at the moment
        // of use. Better a row that says re-verify.
        Some(_) => {
            if key_required {
                IntegrationState::KeyUnavailable
            } else {
                IntegrationState::Verified
            }
        }
    }
}

/// `state_of` for a stored record, looking its requirement up from the shipped
/// descriptor. A record naming a provider the application no longer supports
/// resolves as key-required, which is the conservative answer: it cannot be
/// activated and does not resolve.
fn state_for(record: &AiApiRecord, present: &KeyPresence) -> IntegrationState {
    let key_required = provider_descriptor(&record.provider).is_none_or(|d| d.key_required);
    state_of(record, key_presence(present, record), key_required)
}

/// The key-presence answer each record needs.
///
/// A keychain that will not answer downgrades the records rather than failing
/// the listing (AAP-FR-20): an unreadable entry is indistinguishable, from the
/// author's side, from one that is not there, and both mean "verify this again".
fn key_presence(present: &KeyPresence, record: &AiApiRecord) -> bool {
    if record.masked_hint.is_none() {
        return false;
    }
    present.has(&record.provider)
}

/// AAP-FR-02: which providers have a key, asked of the vault **once**.
///
/// Every supported provider is asked about rather than only the stored ones, so
/// the answer covers the empty records `integrations_from` synthesises; and
/// every stored record is asked about too, so a record naming a provider this
/// build no longer supports is answered from the vault rather than assumed
/// keyless. It costs the same single vault access either way
/// (`ASV-application-secret-vault.md` ASV-FR-30).
pub fn provider_presence(secrets: &dyn SecretStore, records: &[AiApiRecord]) -> KeyPresence {
    let mut ids: Vec<&str> = PROVIDERS.iter().map(|d| d.provider).collect();
    for record in records {
        if !ids.contains(&record.provider.as_str()) {
            ids.push(record.provider.as_str());
        }
    }
    KeyPresence::resolve(secrets, &ids)
}

/// AAP-FR-14: which provider is active, by either route.
///
/// An explicit activation always wins. The implicit resolution applies only to a
/// *set of one* — an author with a single key is never asked to choose from a
/// set of one — and only while nothing has been activated explicitly. An
/// explicit choice that has stopped verifying resolves to nothing rather than
/// silently handing the role to another provider: the author chose that one, and
/// quietly calling a different endpoint (and spending against a different
/// account) is worse than calling none.
pub fn active_provider(
    records: &[AiApiRecord],
    explicit: Option<&str>,
    present: &KeyPresence,
) -> Option<String> {
    let verified: Vec<&AiApiRecord> =
        records.iter().filter(|r| state_for(r, present) == IntegrationState::Verified).collect();

    if let Some(provider) = explicit {
        return verified.iter().find(|r| r.provider == provider).map(|r| r.provider.clone());
    }
    match verified.as_slice() {
        [only] => Some(only.provider.clone()),
        _ => None,
    }
}

/// AAP-FR-02: one outbound record per supported provider, in a stable order,
/// configured or not — so the UI never has to reason about an absent record.
pub fn integrations_from(
    records: &[AiApiRecord],
    explicit_active: Option<&str>,
    present: &KeyPresence,
) -> Vec<AiApiIntegration> {
    let active = active_provider(records, explicit_active, present);
    PROVIDERS
        .iter()
        .map(|descriptor| {
            let stored = records
                .iter()
                .find(|r| r.provider == descriptor.provider)
                .cloned()
                .unwrap_or_else(|| AiApiRecord::empty(descriptor.provider));
            let has_key = key_presence(present, &stored);
            AiApiIntegration {
                provider: descriptor.provider.to_string(),
                display_name: descriptor.display_name.to_string(),
                base_url: stored.base_url.clone(),
                key_state: match (stored.masked_hint.is_some(), has_key) {
                    (false, _) => KeyState::Unset,
                    (true, true) => KeyState::Set,
                    (true, false) => KeyState::Unavailable,
                },
                masked_hint: stored.masked_hint.clone(),
                key_required: descriptor.key_required,
                state: state_of(&stored, has_key, descriptor.key_required),
                verified_at: stored.verified_at.clone(),
                // An unconfigured provider still offers its bundled catalog, so
                // the selector renders before anything is verified.
                models: available_models(&stored, descriptor),
                models_origin: if stored.models.is_empty() {
                    ModelsOrigin::Catalog
                } else {
                    stored.models_origin
                },
                selected_model: stored.selected_model.clone(),
                selected_reasoning: stored.selected_reasoning.clone(),
                turn_timeout_ms: stored.turn_timeout_ms,
                max_output_tokens: stored.max_output_tokens,
                max_context_size: stored.max_context_size,
                active: active.as_deref() == Some(descriptor.provider),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

/// AAP-FR-02: the registry as the UI sees it. Reads the store and probes the
/// keychain for each record's presence; makes no network request.
pub fn list_integrations_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
) -> Result<Vec<AiApiIntegration>, String> {
    let (records, active) = store.load_ai_api_registry()?;
    let present = provider_presence(&*ai.secrets, &records);
    Ok(integrations_from(&records, active.as_deref(), &present))
}

/// Every provider's catalogue, read from the registry alone — no network
/// request and no keychain access, so it answers offline and with the keychain
/// locked (AGR-FR-16).
pub fn list_catalogs_impl(store: &GlobalSettingsStore) -> Result<Vec<AiApiCatalog>, String> {
    let (records, _active) = store.load_ai_api_registry()?;
    Ok(catalogs_from(&records))
}

/// AAP-FR-09 and AAP-FR-13 read together with the key assumed present — the
/// single definition of "could this record carry work, key aside".
///
/// `state_of` alone is not that definition. It answers AAP-FR-09 from the shape
/// of the record, and a record can have that shape without ever having passed a
/// check: `verified_at` is what AAP-FR-13 adds, and a base URL alone is not
/// verification. Reading only `state_of` would call a hand-edited store verified
/// and hand an agent an endpoint that has never answered.
///
/// Shared by `validate_ai_api_selection` and by the catalogue so the two cannot
/// drift. They were separately written once, and the copy without this check
/// let one surface call an agent ready while another called its provider
/// unverified — the same agent, two verdicts.
///
/// `KeyUnavailable` is the carrier for "configured but not usable", as it
/// already is in `state_of`'s last arm; every caller maps it to `not_verified`.
pub fn state_assuming_key(record: &AiApiRecord, key_required: bool) -> IntegrationState {
    match state_of(record, true, key_required) {
        IntegrationState::Verified if record.verified_at.is_none() => {
            IntegrationState::KeyUnavailable
        }
        other => other,
    }
}

pub fn catalogs_from(records: &[AiApiRecord]) -> Vec<AiApiCatalog> {
    PROVIDERS
        .iter()
        .map(|descriptor| {
            let stored = records
                .iter()
                .find(|r| r.provider == descriptor.provider)
                .cloned()
                .unwrap_or_else(|| AiApiRecord::empty(descriptor.provider));
            AiApiCatalog {
                provider: descriptor.provider.to_string(),
                display_name: descriptor.display_name.to_string(),
                // The key is assumed present, through the same function
                // `validate_ai_api_selection` uses (AAP-FR-33). A provider whose
                // key has gone missing is still one an agent may name; that it
                // cannot be *called* right now is the dispatch's answer to give,
                // not the roster's.
                state: state_assuming_key(&stored, descriptor.key_required),
                models: available_models(&stored, descriptor),
            }
        })
        .collect()
}

/// AAP-FR-36: one migration candidate per configured provider.
///
/// A candidate is supplied for **every** stored record, whether or not the
/// vault already holds that provider's key: which keys are adopted and which
/// legacy entries are deleted is the vault's decision
/// (`ASV-application-secret-vault.md` ASV-FR-21, ASV-FR-25), and this module
/// reads no legacy entry itself.
pub fn migration_candidates(records: &[AiApiRecord]) -> Vec<crate::secret_vault::Candidate> {
    records
        .iter()
        .map(|record| crate::secret_vault::Candidate {
            legacy_service: LEGACY_KEYCHAIN_SERVICE.to_string(),
            legacy_account: record.provider.clone(),
            path: crate::secret_vault::path(&["ai_api", "providers", &record.provider]),
        })
        .collect()
}

mod agent_provider;
mod commands;
pub(crate) mod custom_gateway;
pub(crate) mod custom_limits;
pub(crate) mod detection;
mod probe_error;
mod project_override;
mod read_paths;
mod selections;
mod turn_timeout;
mod verify;
pub use agent_provider::*;
pub use commands::*;
pub use custom_limits::*;
use probe_error::api_probe_error;
pub use project_override::*;
pub use read_paths::*;
pub use selections::*;
pub use turn_timeout::*;
pub use verify::*;

#[cfg(test)]
mod tests;
