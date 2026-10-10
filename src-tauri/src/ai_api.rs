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
    descriptor
        .model_catalog
        .iter()
        .find(|(id, _, _)| *id == model_id)
        .map(|(_, _, images)| *images)
}

/// AAP-FR-35: the three-step capability decision, applied to a whole model list.
///
/// The probe's own declaration wins where it made one; the shipped catalog
/// settles what the probe left unsaid; and a model neither declares anything
/// about carries `false`. Applied at the one place a model list is produced, so
/// `list_ai_api_integrations`, `resolve_ai_api_endpoint`, and the selector
/// cannot disagree about what a model takes.
fn settle_image_input(mut models: Vec<ModelOption>, descriptor: &AiApiProvider) -> Vec<ModelOption> {
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
    if record.models.is_empty() {
        catalog_models(descriptor)
    } else {
        // AAP-FR-35: a persisted record carries the probe's answer without its
        // provenance, so the shipped catalog is consulted again here for every
        // model the store did not mark image-capable. This is the one place a
        // stored list becomes the list callers read, so it is the one place the
        // three-step decision has to be made.
        settle_image_input(record.models.clone(), descriptor)
    }
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
pub fn reasoning_error(model: Option<&ModelOption>, choice: &ReasoningChoice) -> Option<&'static str> {
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
fn reasoning_survives(models: &[ModelOption], selected: Option<&str>, choice: &ReasoningChoice) -> bool {
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
        Self {
            provider: provider.to_string(),
            ..Default::default()
        }
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
#[derive(Clone, Default, PartialEq, Eq)]
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
    }
}

impl AiApiIntegrations {
    pub fn new(prober: Box<dyn EndpointProber>, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            prober,
            openrouter_prober: None,
            secrets,
            write_lock: Mutex::new(()),
        }
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
pub fn state_of(
    record: &AiApiRecord,
    key_present: bool,
    key_required: bool,
) -> IntegrationState {
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
    let verified: Vec<&AiApiRecord> = records
        .iter()
        .filter(|r| state_for(r, present) == IntegrationState::Verified)
        .collect();

    if let Some(provider) = explicit {
        return verified
            .iter()
            .find(|r| r.provider == provider)
            .map(|r| r.provider.clone());
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
                active: active.as_deref() == Some(descriptor.provider),
            }
        })
        .collect()
}

/// AAP-FR-16: resolve the open project's API integration, and say *how* — which
/// is what tells the UI what to write on the line.
pub fn resolve_project(
    records: &[AiApiRecord],
    explicit_active: Option<&str>,
    override_provider: Option<&str>,
    present: &KeyPresence,
) -> ProjectAiApiIntegration {
    let active = active_provider(records, explicit_active, present);
    let is_verified = |provider: &str| {
        records.iter().any(|r| {
            r.provider == provider
                && state_for(r, present) == IntegrationState::Verified
        })
    };

    if let Some(over) = override_provider {
        if is_verified(over) {
            return ProjectAiApiIntegration {
                provider: Some(over.to_string()),
                resolution: ProjectResolution::Overridden,
                override_provider: Some(over.to_string()),
            };
        }
        // The override is recorded but no longer usable. It is deliberately not
        // erased (AAP-FR-15): re-verifying the provider should restore the
        // author's choice rather than silently having lost it.
        return ProjectAiApiIntegration {
            provider: active,
            resolution: ProjectResolution::OverrideUnavailable,
            override_provider: Some(over.to_string()),
        };
    }

    if let Some(active) = active {
        return ProjectAiApiIntegration {
            provider: Some(active),
            resolution: ProjectResolution::Inherited,
            override_provider: None,
        };
    }

    // Nothing resolves. The distinction the UI needs is whether there is
    // anything to choose between at all: something configured means "pick one",
    // nothing configured means "go and set one up".
    let anything_configured = records
        .iter()
        .any(|r| state_for(r, present) != IntegrationState::Unconfigured);
    ProjectAiApiIntegration {
        provider: None,
        resolution: if anything_configured {
            ProjectResolution::NoneSelected
        } else {
            ProjectResolution::NoneConfigured
        },
        override_provider: None,
    }
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

/// AAP-FR-05 / FR-06 / FR-10: reach the endpoint, confirm it answers as a
/// conversational API, learn its models, and only then commit the configuration.
pub fn verify_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;

    // Both checks happen before any network work: an empty field and a typo are
    // reported instantly and are never mistaken for an unreachable host.
    let normalized = normalize_base_url(base_url).map_err(|e| match e {
        BaseUrlError::Empty => ERR_BASE_URL_EMPTY.to_string(),
        BaseUrlError::Invalid => ERR_BASE_URL_INVALID.to_string(),
    })?;
    // AAP-FR-KRVT: the gateway root carries no `/v1`, so a route is never built
    // as `/v1/v1/...`.
    if provider == "custom" && custom_gateway::has_v1_suffix(&normalized) {
        return Err(ERR_BASE_URL_INVALID.into());
    }
    // AAP-FR-32: an empty field means the key is unchanged, not withdrawn. The
    // field is never populated from the record (AII-FR-07), so treating empty
    // as "no key" would make a configured provider impossible to verify again
    // without the author holding their key a second time — which is the one
    // thing storing it was meant to avoid.
    let supplied = api_key.map(str::trim).filter(|k| !k.is_empty());
    let key = match &supplied {
        Some(k) => Some((*k).to_string()),
        None => ai
            .secrets
            .get(provider)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
            .filter(|k| !k.is_empty()),
    };
    if descriptor.key_required && key.is_none() {
        return Err(ERR_KEY_MISSING.into());
    }
    let key = key.as_deref();

    let probed = ai
        .prober_for(provider)
        .probe(&ProbeRequest {
            base_url: &normalized,
            api_key: key,
            auth: descriptor.auth,
            models_path: descriptor.models_path,
            models_format: descriptor.models_format,
        })
        .map_err(api_probe_error)?;

    // AAP-FR-10: an endpoint that lists nothing degrades to the bundled catalog
    // rather than failing a verification that otherwise succeeded.
    let (models, models_origin) = if probed.is_empty() {
        (catalog_models(descriptor), ModelsOrigin::Catalog)
    } else {
        // AAP-FR-35: what the probe declared about each model's input
        // modalities stands; what it said nothing about the shipped catalog
        // settles; and what neither declares carries `false`.
        (settle_image_input(probed, descriptor), ModelsOrigin::Probed)
    };

    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;

    // AAP-FR-06: a key the author supplied goes to the keychain first. A
    // keychain that refuses leaves the registry untouched, so a failed write
    // cannot produce a record describing a key that was never stored.
    //
    // AAP-FR-32: nothing is written when the verification re-presented the
    // stored key — the entry is already what it should be — and no verification
    // deletes one. Withdrawing a key is what `clear_ai_api_integration` is for,
    // so an author checking a working provider cannot lose their credential by
    // leaving the field alone.
    if let Some(supplied) = supplied {
        ai.secrets
            .set(provider, supplied)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    }

    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        record.base_url = Some(normalized);
        record.masked_hint = key.map(mask_hint);
        record.verified_at = Some(now_iso8601());
        // AAP-FR-12: a selection the refreshed list no longer offers falls back
        // to the provider's own default rather than requesting a model that
        // endpoint does not serve.
        if let Some(selected) = record.selected_model.clone() {
            if !models.iter().any(|m| m.id == selected) {
                record.selected_model = None;
            }
        }
        // AAP-FR-29: and the reasoning choice is re-checked against the refreshed
        // descriptors, so a level the model has stopped offering — or that fell
        // away with the model itself just above — does not survive as a depth
        // the endpoint would refuse.
        if let Some(choice) = record.selected_reasoning.clone() {
            if !reasoning_survives(&models, record.selected_model.as_deref(), &choice) {
                record.selected_reasoning = None;
            }
        }
        record.models = models;
        record.models_origin = models_origin;
    }

    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// AAP-FR-11: persist the chosen model. `None` selects the provider's own
/// default; an id absent from the record's current list is refused.
pub fn set_model_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    model_id: Option<&str>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        // The record's *current* list, which is the probed one after a
        // verification and the bundled catalog before it — matching what the
        // selector was populated from.
        let available = available_models(record, descriptor);
        match model_id {
            None => record.selected_model = None,
            Some(id) => {
                if !available.iter().any(|m| m.id == id) {
                    return Err(ERR_UNKNOWN_MODEL.into());
                }
                record.selected_model = Some(id.to_string());
            }
        }
        // AAP-FR-29: the reasoning choice belongs to the model it was made
        // against. A new model that cannot honour it returns the record to that
        // model's own default rather than carrying across a depth it would
        // refuse — including the case of clearing the model entirely, where no
        // capability is known at all.
        if let Some(choice) = record.selected_reasoning.clone() {
            if !reasoning_survives(&available, record.selected_model.as_deref(), &choice) {
                record.selected_reasoning = None;
            }
        }
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// AAP-FR-27: persist the reasoning the author asked for. `None` means the
/// selected model's own default; a choice the selected model cannot honour is
/// refused rather than stored.
pub fn set_reasoning_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    choice: Option<ReasoningChoice>,
) -> Result<AiApiIntegration, String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (mut records, active) = store.load_ai_api_registry()?;
    let index = match records.iter().position(|r| r.provider == provider) {
        Some(i) => i,
        None => {
            records.push(AiApiRecord::empty(provider));
            records.len() - 1
        }
    };
    {
        let record = &mut records[index];
        match choice {
            None => record.selected_reasoning = None,
            Some(choice) => {
                let available = if record.models.is_empty() {
                    catalog_models(descriptor)
                } else {
                    record.models.clone()
                };
                // Distinguishing "no model chosen" from "this model cannot
                // reason" matters to the author: the first is fixed by picking a
                // model, the second by picking a different one.
                let Some(selected) = record.selected_model.as_deref() else {
                    return Err(ERR_NO_MODEL_SELECTED.into());
                };
                let model = available.iter().find(|m| m.id == selected);
                if model.is_none() {
                    return Err(ERR_NO_MODEL_SELECTED.into());
                }
                if let Some(err) = reasoning_error(model, &choice) {
                    return Err(err.into());
                }
                record.selected_reasoning = Some(choice);
            }
        }
    }
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    integrations_from(&records, active.as_deref(), &present)
        .into_iter()
        .find(|i| i.provider == provider)
        .ok_or_else(|| ERR_UNKNOWN_PROVIDER.to_string())
}

/// AAP-FR-13: record the user-global active provider. At most one is ever
/// recorded — activating one clears any other.
pub fn set_active_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
) -> Result<Vec<AiApiIntegration>, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;
    let (records, _) = store.load_ai_api_registry()?;
    let present = provider_presence(&*ai.secrets, &records);
    let verified = records
        .iter()
        .any(|r| r.provider == provider && state_for(r, &present) == IntegrationState::Verified);
    if !verified {
        return Err(ERR_NOT_VERIFIED.into());
    }
    store.save_ai_api_registry(records.clone(), Some(provider.to_string()))?;
    Ok(integrations_from(&records, Some(provider), &present))
}

/// AAP-FR-15: delete the provider's keychain entry and its record together.
/// Idempotent. A cleared provider that was active leaves nothing active; a
/// project override naming it is left recorded and stops resolving.
pub fn clear_integration_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
) -> Result<Vec<AiApiIntegration>, String> {
    provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let _guard = ai
        .write_lock
        .lock()
        .map_err(|e| format!("ai api registry poisoned: {e}"))?;
    ai.secrets
        .delete(provider)
        .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    let (mut records, active) = store.load_ai_api_registry()?;
    records.retain(|r| r.provider != provider);
    let active = match active {
        Some(a) if a == provider => None,
        other => other,
    };
    store.save_ai_api_registry(records.clone(), active.clone())?;
    let present = provider_presence(&*ai.secrets, &records);
    Ok(integrations_from(&records, active.as_deref(), &present))
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

/// AAP-FR-16: how the open project resolves an API integration right now.
pub fn get_project_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
) -> Result<ProjectAiApiIntegration, String> {
    let (records, active) = store.load_ai_api_registry()?;
    // An empty key is "no project open" (`ProjectState::slot_key`), whose slot is
    // the shared default one — an override read from there would belong to no
    // project in particular.
    let override_provider = if project_key.is_empty() {
        None
    } else {
        store.load_ai_api_override(project_key)?
    };
    Ok(resolve_project(
        &records,
        active.as_deref(),
        override_provider.as_deref(),
        &provider_presence(&*ai.secrets, &records),
    ))
}

/// AAP-FR-17 / FR-18: record (or clear) the open project's override. A provider
/// that is not currently verified is refused, so an override can only ever name
/// an integration the author has already made work.
pub fn set_project_impl(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
    provider: Option<&str>,
) -> Result<ProjectAiApiIntegration, String> {
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT.into());
    }
    match provider {
        None => store.clear_ai_api_override(project_key)?,
        Some(provider) => {
            provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
            let (records, _) = store.load_ai_api_registry()?;
            let present = provider_presence(&*ai.secrets, &records);
            let verified = records.iter().any(|r| {
                r.provider == provider && state_for(r, &present) == IntegrationState::Verified
            });
            if !verified {
                return Err(ERR_NOT_VERIFIED.into());
            }
            store.save_ai_api_override(project_key, provider)?;
        }
    }
    get_project_impl(store, ai, project_key)
}

/// AAP-FR-19: one of the two read paths for a key and an endpoint in the
/// application, and the one selected by what the *project* resolves to.
///
/// Deliberately **not** a `#[tauri::command]`: it is not registered in
/// `generate_handler!`, so no frontend `invoke` can reach it. Its one consumer
/// is the graduation loop (`crate::graduation::driver`, GRL-FR-EKXT), which is
/// therefore the only thing the model and the reasoning an integration stores
/// ever govern (AAP-FR-31). An agent's turn takes the other path,
/// `resolve_ai_api_endpoint`, naming its own provider and carrying its own
/// model and reasoning (AGC-FR-14) — so a stored selection never reaches an
/// agent and an agent's selection never reaches a graduation run.
///
/// It refuses with the same typed distinction `get_project_ai_api_integration`
/// reports rather than returning a key when the project resolves no integration.
pub fn resolve_ai_api_call(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    project_key: &str,
) -> Result<AiApiCall, String> {
    let resolved = get_project_impl(store, ai, project_key)?;
    let provider = match (resolved.resolution, resolved.provider.as_deref()) {
        (ProjectResolution::NoneConfigured, _) => return Err(ERR_NONE_CONFIGURED.into()),
        (ProjectResolution::NoneSelected, _) => return Err(ERR_NONE_SELECTED.into()),
        (_, Some(provider)) => provider.to_string(),
        // `override_unavailable` with nothing active: the project names a
        // provider that no longer works and there is no fallback.
        (_, None) => return Err(ERR_NONE_SELECTED.into()),
    };
    let (records, _) = store.load_ai_api_registry()?;
    let record = records
        .iter()
        .find(|r| r.provider == provider)
        .ok_or(ERR_NONE_SELECTED)?;
    let base_url = record
        .base_url
        .clone()
        .filter(|u| !u.is_empty())
        .ok_or(ERR_NONE_SELECTED)?;
    // The one place a key is read back out, and only for a caller that is about
    // to present it to the endpoint it belongs to.
    let api_key = if record.masked_hint.is_some() {
        ai.secrets
            .get(&provider)
            .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?
    } else {
        None
    };
    let provider_name = provider.clone();
    Ok(AiApiCall {
        provider,
        base_url,
        api_key,
        model_id: record.selected_model.clone(),
        // AAP-FR-31: the one place the stored choice becomes the reasoning a
        // request carries. `None` sends nothing at all.
        reasoning: record.selected_reasoning.clone(),
        // AAP-FR-35: the flag of the model this project resolved to. A record
        // that names no model resolves to the provider's own default, whose
        // capability nothing here declares — so it is `false`, on the same terms
        // every undeclared capability is.
        accepts_image_input: provider_descriptor(&provider_name)
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| Some(m.id.as_str()) == record.selected_model.as_deref())
            .is_some_and(|m| m.accepts_image_input),
        // AAP-FR-RTMZ: the route metadata of the same model.
        model_mode: provider_descriptor(&provider_name)
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| Some(m.id.as_str()) == record.selected_model.as_deref())
            .and_then(|m| m.mode),
        // AAP-FR-TXNM: no read path but `resolve_ai_api_endpoint` returns it.
        turn_timeout_ms: None,
    })
}

/// AAP-FR-33: whether a caller's own `(provider, model, reasoning)` is one this
/// module could serve.
///
/// Deliberately **not** a `#[tauri::command]`, on the same terms as the two
/// resolvers: it is the rule an agent's stored selection is checked against
/// (`crate::agents`, AGR-FR-06 / AGR-FR-07), not something a frontend decides.
///
/// Reads the registry and nothing else. In particular it never asks the
/// keychain whether the key is still there — `state_of` is handed `true` for
/// key presence — because a locked keychain must not stop an author *editing*
/// an agent, only calling one. The keychain is consulted exactly once, in
/// `resolve_ai_api_endpoint` below, at the moment a call is actually made.
#[allow(dead_code)]
pub fn validate_ai_api_selection(
    store: &GlobalSettingsStore,
    provider: &str,
    model_id: &str,
    reasoning: Option<&ReasoningChoice>,
) -> Result<(), String> {
    let descriptor = provider_descriptor(provider).ok_or(ERR_UNKNOWN_PROVIDER)?;
    let (records, _) = store.load_ai_api_registry()?;
    let record = records
        .iter()
        .find(|r| r.provider == provider)
        .cloned()
        .unwrap_or_else(|| AiApiRecord::empty(provider));

    // Registry-only reading of AAP-FR-09 and AAP-FR-13. `key_present: true` is
    // the assumption that makes this offline: a record that carries a masked
    // hint was verified with a key, and whether that key is *readable right now*
    // is a question for the call, not for the edit.
    match state_assuming_key(&record, descriptor.key_required) {
        IntegrationState::Unconfigured => return Err(ERR_PROVIDER_UNCONFIGURED.into()),
        IntegrationState::KeyUnavailable => return Err(ERR_NOT_VERIFIED.into()),
        IntegrationState::Verified => {}
    }

    let models = available_models(&record, descriptor);
    let model = models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or(ERR_UNKNOWN_MODEL)?;
    // AAP-FR-27: `None` is the model's own default and is always serviceable.
    if let Some(choice) = reasoning {
        if let Some(err) = reasoning_error(Some(model), choice) {
            return Err(err.into());
        }
    }
    Ok(())
}

/// AAP-FR-34: the endpoint and key of a provider the **caller** names, carrying
/// the **caller's** model and reasoning.
///
/// The difference from `resolve_ai_api_call` is the whole point of it existing:
/// that one asks "what does the open project resolve to, and what did the author
/// choose for it"; this one asks "reach *this* provider with *this* model". The
/// two consumers are settled and do not overlap (AAP-FR-19): this path serves an
/// agent's turn alone (`crate::agent_conversations`, AGC-FR-14), which runs each
/// conversation against the configuration its own agent named — its provider,
/// its model, its reasoning, all chosen in the Agents section rather than in the
/// AI API one — and no project override and no stored selection redirects any of
/// them.
///
/// Deliberately **not** a `#[tauri::command]`: it returns a cleartext key.
pub fn resolve_ai_api_endpoint(
    store: &GlobalSettingsStore,
    ai: &AiApiIntegrations,
    provider: &str,
    model_id: &str,
    reasoning: Option<ReasoningChoice>,
) -> Result<AiApiCall, String> {
    // Validated first, so an unserviceable combination is refused *before* a key
    // is read rather than after.
    validate_ai_api_selection(store, provider, model_id, reasoning.as_ref())?;
    let (records, _) = store.load_ai_api_registry()?;
    let record = records
        .iter()
        .find(|r| r.provider == provider)
        .ok_or(ERR_PROVIDER_UNCONFIGURED)?;
    let base_url = record
        .base_url
        .clone()
        .filter(|u| !u.is_empty())
        .ok_or(ERR_PROVIDER_UNCONFIGURED)?;
    let descriptor = provider_descriptor(provider);
    let api_key = if record.masked_hint.is_some() {
        match ai.secrets.get(provider) {
            // The store answered and the entry is gone. A distinct outcome from
            // the store refusing to answer: one is a credential to re-enter, the
            // other is a keychain to unlock.
            Ok(None) => return Err(ERR_KEY_UNAVAILABLE.into()),
            Ok(some) => some,
            Err(_) => return Err(ERR_KEYCHAIN_UNAVAILABLE.into()),
        }
    } else {
        None
    };
    Ok(AiApiCall {
        provider: provider.to_string(),
        base_url,
        api_key,
        // The caller's, never the record's. Substituting the provider's stored
        // selection here would make every agent on a provider answer as one.
        model_id: Some(model_id.to_string()),
        reasoning,
        // AAP-FR-35: the flag of the model that was resolved. `validate_ai_api_selection`
        // above neither read nor reported it — an agent is never refused for the
        // pictures a conversation might later carry — so it is read here, from
        // the same model list the validation resolved against.
        accepts_image_input: descriptor
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| m.id == model_id)
            .is_some_and(|m| m.accepts_image_input),
        // AAP-FR-RTMZ: the route metadata of the resolved model.
        model_mode: descriptor
            .map(|d| available_models(record, d))
            .unwrap_or_default()
            .iter()
            .find(|m| m.id == model_id)
            .and_then(|m| m.mode),
        // AAP-FR-TXNM: this read path alone returns it.
        turn_timeout_ms: record.turn_timeout_ms,
    })
}

// ---------------------------------------------------------------------------
// Logging (LGC-logging.md)
// ---------------------------------------------------------------------------
//
// The emit sites sit at the command layer rather than inside the `*_impl`
// functions above, which are pure over their collaborators and are what the
// tests drive. Every command here is a thing the author did, so one record per
// command is the whole account of how a configuration came to be what it is —
// and none of the impls has to carry a sink to produce it.

/// AAP-FR-05: a verification is about to reach an endpoint.
///
/// Takes the key so the record can report whether one was *supplied* — the
/// difference between AAP-FR-32's "re-present what is stored" and a fresh
/// credential, which is the first thing to check when a verification that used
/// to work stops. The key itself goes no further than this boolean; the record
/// is built here rather than at the call site precisely so that is a property of
/// one function a reader can check rather than of every caller.
fn log_verify_attempt<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    provider: &str,
    base_url: &str,
    api_key: Option<&str>,
) {
    logging::log_info(
        sink,
        buffer,
        &[Domain::Ai, Domain::Remote],
        "verifying ai api integration",
        log_fields! {
            "provider" => provider,
            // The author's raw field, and it has been validated by nothing at
            // this point: `normalize_base_url`'s refusal of a credential
            // smuggled into a URL happens inside the verification below, which
            // is too late for a record already written. `loggable_base_url` is
            // what makes this field safe — userinfo, query, and fragment gone,
            // the endpoint still recognisable.
            "baseUrl" => crate::ai_shared::loggable_base_url(base_url),
            "keySupplied" => api_key.is_some_and(|k| !k.trim().is_empty()),
        },
    );
}

/// What a verification turned out to be. `ERROR` on failure: the author asked
/// for something and got nothing, and every one of AAP-FR-05's typed refusals
/// names a different correction.
fn log_verify_outcome<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    provider: &str,
    result: &Result<AiApiIntegration, String>,
    duration_ms: u64,
) {
    match result {
        Ok(integration) => logging::log_info(
            sink,
            buffer,
            &[Domain::Ai, Domain::Remote],
            "ai api integration verified",
            log_fields! {
                "provider" => provider,
                // Normalised by now, so it carries no userinfo — but a stored
                // URL may still hold a query string, and one gateway or another
                // puts a key there.
                "baseUrl" => integration.base_url.as_deref()
                    .map(crate::ai_shared::loggable_base_url)
                    .unwrap_or_else(|| "none".to_string()),
                "state" => integration.state,
                "keyState" => integration.key_state,
                "models" => integration.models.len(),
                // AAP-FR-10: `catalog` means the probe told us nothing and the
                // list is the bundled one, which is why a model the author
                // expected may be missing from the selector.
                "modelsOrigin" => integration.models_origin,
                "selectedModel" => integration.selected_model.as_deref().unwrap_or("<provider default>"),
                "durationMs" => duration_ms,
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai, Domain::Remote],
            "ai api verification failed",
            log_fields! {
                "provider" => provider,
                // One of this module's typed errors (AAP-FR-05). No provider
                // response body and no client error is carried through: both
                // routinely echo the request that produced them, headers and
                // all.
                "error" => error,
                "durationMs" => duration_ms,
            },
        ),
    }
}

/// AAP-FR-02: how the registry reads right now.
///
/// `DEBUG`, because this runs whenever the settings level renders and says
/// nothing new most times it does. It earns its slot on the one occasion it
/// matters: a provider that was working reads `key_unavailable` because the
/// keychain no longer answers for it (AAP-FR-09), which is invisible everywhere
/// else until an agent fails to answer and nobody can say why.
fn log_listing<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    result: &Result<Vec<AiApiIntegration>, String>,
) {
    match result {
        Ok(integrations) => logging::log_debug(
            sink,
            buffer,
            &[Domain::Ai],
            "ai api integrations listed",
            log_fields! {
                "total" => integrations.len(),
                "verified" => integrations.iter().filter(|i| i.state == IntegrationState::Verified).count(),
                "keyUnavailable" => integrations.iter().filter(|i| i.state == IntegrationState::KeyUnavailable).count(),
                "active" => integrations.iter().find(|i| i.active).map(|i| i.provider.as_str()).unwrap_or("none"),
            },
        ),
        Err(error) => logging::log_error(
            sink,
            buffer,
            &[Domain::Ai],
            "ai api integrations could not be listed",
            log_fields! { "error" => error },
        ),
    }
}

/// AAP-FR-16: which provider the open project actually calls, and whether an
/// override it still names is being ignored.
///
/// `DEBUG` on the same terms as the listing: read on every render, and the one
/// thing worth knowing from it — `override_unavailable`, where the author
/// believes they are calling one provider and the application is calling another
/// — is otherwise reported nowhere but in the settings panel they are not
/// looking at.
fn log_project_resolution<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    resolved: &ProjectAiApiIntegration,
) {
    logging::log_debug(
        sink,
        buffer,
        &[Domain::Ai],
        "project ai api integration resolved",
        log_fields! {
            "provider" => resolved.provider.as_deref().unwrap_or("none"),
            "resolution" => resolved.resolution,
            "override" => resolved.override_provider.as_deref().unwrap_or("none"),
        },
    );
}

/// A change to the registry: `INFO` when it took, `WARN` with the typed refusal
/// when it did not.
///
/// `WARN` rather than `ERROR` because nothing broke — a rejected model id or an
/// unverified provider is the module declining to store something it could not
/// serve (AAP-FR-11, AAP-FR-13, AAP-FR-27), and the record exists so an author
/// wondering why their choice did not stick can see that it was refused.
fn log_config_outcome<S: LogSink + Clone + Send + 'static, T>(
    sink: &S,
    buffer: &'static LogBuffer,
    message: &str,
    result: &Result<T, String>,
    mut fields: Fields,
) {
    match result {
        Ok(_) => logging::log_info(sink, buffer, &[Domain::Ai], message, fields),
        Err(error) => {
            fields.insert("error".to_string(), serde_json::json!(error));
            logging::log_warn(sink, buffer, &[Domain::Ai], message, fields);
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_ai_api_integrations(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = list_integrations_impl(&store, &ai);
    log_listing(&app, &logging::BUFFER, &result);
    result
}

#[tauri::command]
pub fn list_ai_api_catalogs(
    store: State<'_, GlobalSettingsStore>,
) -> Result<Vec<AiApiCatalog>, String> {
    list_catalogs_impl(&store)
}

#[tauri::command]
pub fn verify_ai_api_integration(
    provider: String,
    base_url: String,
    api_key: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    log_verify_attempt(&app, &logging::BUFFER, &provider, &base_url, api_key.as_deref());
    let started = std::time::Instant::now();
    let result = verify_integration_impl(&store, &ai, &provider, &base_url, api_key.as_deref());
    // The duration is what separates `timed_out` from a slow endpoint that
    // answered, and it is the only measurement of the one network call this
    // module makes.
    log_verify_outcome(
        &app,
        &logging::BUFFER,
        &provider,
        &result,
        started.elapsed().as_millis() as u64,
    );
    result
}

#[tauri::command]
pub fn set_ai_api_model(
    provider: String,
    model_id: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let result = set_model_impl(&store, &ai, &provider, model_id.as_deref());
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api model selected",
        &result,
        log_fields! {
            "provider" => &provider,
            "model" => model_id.as_deref().unwrap_or("<provider default>"),
            // AAP-FR-29: a model change can clear the reasoning choice under the
            // author, which is otherwise a silent change to what every call
            // carries.
            "reasoning" => result.as_ref().ok()
                .map(|i| reasoning_label(i.selected_reasoning.as_ref()))
                .unwrap_or_else(|| "unchanged".to_string()),
        },
    );
    result
}

#[tauri::command]
pub fn set_ai_api_reasoning(
    provider: String,
    choice: Option<ReasoningChoice>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<AiApiIntegration, String> {
    let requested = reasoning_label(choice.as_ref());
    let result = set_reasoning_impl(&store, &ai, &provider, choice);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api reasoning selected",
        &result,
        log_fields! { "provider" => &provider, "reasoning" => requested },
    );
    result
}

#[tauri::command]
pub fn set_active_ai_api_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = set_active_impl(&store, &ai, &provider);
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "active ai api integration set",
        &result,
        log_fields! { "provider" => &provider },
    );
    result
}

#[tauri::command]
pub fn clear_ai_api_integration(
    provider: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
) -> Result<Vec<AiApiIntegration>, String> {
    let result = clear_integration_impl(&store, &ai, &provider);
    // AAP-FR-15: the keychain entry is gone and every agent naming this provider
    // stops resolving. Worth a line of its own — an agent that answered
    // yesterday and refuses today is otherwise unexplained.
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "ai api integration cleared",
        &result,
        log_fields! { "provider" => &provider },
    );
    result
}

#[tauri::command]
pub fn get_project_ai_api_integration(
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAiApiIntegration, String> {
    let result = get_project_impl(&store, &ai, &project.slot_key());
    if let Ok(resolved) = &result {
        log_project_resolution(&app, &logging::BUFFER, resolved);
    }
    result
}

#[tauri::command]
pub fn set_project_ai_api_integration(
    provider: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    ai: State<'_, AiApiIntegrations>,
    project: State<'_, ProjectState>,
) -> Result<ProjectAiApiIntegration, String> {
    let result = set_project_impl(&store, &ai, &project.slot_key(), provider.as_deref());
    log_config_outcome(
        &app,
        &logging::BUFFER,
        "project ai api override set",
        &result,
        log_fields! {
            "provider" => provider.as_deref().unwrap_or("<inherit>"),
            "resolution" => result.as_ref().ok().map(|r| r.resolution),
        },
    );
    result
}

mod agent_provider;
pub(crate) mod custom_gateway;
mod probe_error;
mod turn_timeout;
use probe_error::api_probe_error;
pub use agent_provider::*;
pub use turn_timeout::*;

#[cfg(test)]
mod tests;
