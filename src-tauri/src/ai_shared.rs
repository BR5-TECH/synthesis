//! Primitives shared by the two AI integration levels —
//! `specifications/core/AAP-ai-api-integrations.md` and
//! `specifications/core/AIC-agentic-integrations.md`.
//!
//! Both levels reach an HTTP endpoint, hold an API key in the OS keychain, and
//! describe models the same way. The rules that differ between them (which
//! providers exist, what verification proves, how a project resolves one) live
//! in the level's own module; only the mechanics live here.
//!
//! Two properties this module is responsible for:
//!
//! - **A key never leaves except as `mask_hint`.** The keychain is the only
//!   place a key rests (AAP-FR-07 / AIC-FR-20), and `mask_hint` is the only
//!   key-derived text either level is allowed to return.
//! - **Every failure is distinguishable.** `unreachable` and `rejected` call for
//!   different corrections by the author, so the prober never collapses them
//!   (AAP-FR-05 / AIC-FR-22).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long an endpoint may take to answer before the request is abandoned
/// (AAP-FR-05 / AIC-FR-22). Long enough for a cold serverless start, short
/// enough that an unanswering host cannot wedge the settings surface.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------------------
// Model and effort options
// ---------------------------------------------------------------------------

/// What a model declares about the reasoning it can be asked for (AAP-FR-24 /
/// AAP-FR-25).
///
/// Read from the provider, never authored here. It is what makes the reasoning
/// offered to the author *compatible with the model in front of them*: the
/// ladder is per model rather than per provider, and the levels one model
/// accepts are routinely not the levels another accepts.
///
/// `supported_efforts` is `None` for a model that reasons but declares no
/// levels of its own — the common case — which the UI renders as off/on rather
/// than as a ladder (AII-FR-43).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelReasoning {
    /// Reasoning cannot be switched off for this model, so no `off` is offered.
    pub mandatory: bool,
    /// Whether the model reasons when nothing is asked of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_enabled: Option<bool>,
    /// This model's own ladder, in the order it declares — authoritative over
    /// any level this application knows by name (AAP-FR-26).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_efforts: Option<Vec<String>>,
    /// A member of `supported_efforts` whenever both are present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

impl ModelReasoning {
    /// Whether `effort` is one this model actually offers (AAP-FR-26).
    pub fn offers_effort(&self, effort: &str) -> bool {
        self.supported_efforts
            .as_ref()
            .is_some_and(|l| l.iter().any(|e| e == effort))
    }
}

/// AAP-FR-RTMZ: the conversation route a Custom gateway model serves.
///
/// Route metadata only. A model with no value (`None` on [`ModelOption`])
/// supports both routes, and which one the adapter takes then is the adapter's
/// own business (CVL-FR-ZPGW).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelMode {
    /// Chat Completions only.
    Chat,
    /// Responses only.
    Responses,
}

impl ModelMode {
    /// A log-safe label.
    pub fn label(self) -> &'static str {
        match self {
            ModelMode::Chat => "chat",
            ModelMode::Responses => "responses",
        }
    }
}

/// One selectable model. `id` is what an invocation passes to the backend;
/// `label` is what the author reads.
///
/// `reasoning` is `None` where the model declares none at all, which is also
/// what every model of a bundled catalog carries: reasoning is something only
/// the provider can report about a model (AAP-FR-24).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ModelReasoning>,
    /// AAP-FR-35: whether this model takes **image content** in its input.
    ///
    /// True **only where the capability is declared**. A probe that reports the
    /// model's input modalities settles it; where the provider reports nothing
    /// about them, the shipped descriptor's own catalog entry settles it; and
    /// where neither declares anything, it is false.
    ///
    /// An undeclared capability is treated as absent rather than guessed at,
    /// which is the safe direction: a caller told a model takes no image sends
    /// text and metadata and the author reads a warning (per
    /// `AGC-agent-conversations.md` AGC-FR-37), while a caller told a model
    /// takes one and finding it does not would cost the turn.
    pub accepts_image_input: bool,
    /// AAP-FR-35: whether `accepts_image_input` came from a **declaration**
    /// rather than from the default.
    ///
    /// The requirement's three steps turn on the difference between "the
    /// provider said this model takes no image" and "the provider said nothing
    /// about it": the first is the answer, the second hands the question on to
    /// the shipped catalog. A single boolean cannot carry both, so this marks
    /// which of the two the flag beside it is.
    ///
    /// Persisted with the flag it qualifies, so a record reloaded from
    /// `synthesis.toml` still knows which of the two answers it holds and the
    /// three-step decision reaches the same result on the next launch as it did
    /// on the probe. A store written before this field existed loads it as
    /// `false`, which sends the question to the catalog — the same place a
    /// silent provider sends it.
    pub image_input_declared: bool,
    /// AAP-FR-RTMZ: the conversation route the model serves. `None` means both.
    /// Only models discovered from a Custom gateway carry a value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ModelMode>,
}

impl ModelOption {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            reasoning: None,
            accepts_image_input: false,
            image_input_declared: false,
            mode: None,
        }
    }

    pub fn with_mode(mut self, mode: Option<ModelMode>) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_reasoning(mut self, reasoning: Option<ModelReasoning>) -> Self {
        self.reasoning = reasoning;
        self
    }

    /// AAP-FR-35: the model's image-input capability **as the provider declared
    /// it** — `None` where the provider declared nothing about it, which leaves
    /// the question to the shipped catalog.
    pub fn with_declared_image_input(mut self, declared: Option<bool>) -> Self {
        self.accepts_image_input = declared.unwrap_or(false);
        self.image_input_declared = declared.is_some();
        self
    }

    /// AAP-FR-35: settle the capability from a source that is not the provider —
    /// the shipped catalog entry for this model, or its absence.
    ///
    /// A declaration the provider made is never overwritten: the endpoint that
    /// will serve the call is the authority on what it takes, and the catalog is
    /// the fallback for what it left unsaid.
    pub fn with_catalog_image_input(mut self, catalog: Option<bool>) -> Self {
        if !self.image_input_declared {
            self.accepts_image_input = catalog.unwrap_or(false);
        }
        self
    }
}

/// AAP-FR-35: whether a provider's declared input modality list names images.
///
/// `None` means the provider declared **nothing about input modalities** — a
/// distinct answer from a declared list that omits images, and the one that
/// hands the question on to the shipped catalog. An empty list is a declaration
/// that says nothing usable, so it is read as no declaration at all.
pub fn image_input_declared(modalities: Option<&[String]>) -> Option<bool> {
    let modalities = modalities.filter(|m| !m.is_empty())?;
    Some(modalities.iter().any(|m| m.trim().eq_ignore_ascii_case("image")))
}

/// One reasoning-effort level. Declared per vendor and never probed
/// (AIC-FR-09), because effort is a property of the backend's interface rather
/// than of the installation.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct EffortOption {
    pub id: String,
    pub label: String,
}

impl EffortOption {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
        }
    }
}

/// Whether a record's model list came from the backend itself or from the
/// bundled catalog (AAP-FR-10 / AIC-FR-08).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelsOrigin {
    Probed,
    #[default]
    Catalog,
}

// ---------------------------------------------------------------------------
// The credential store
// ---------------------------------------------------------------------------

/// Why a credential-store operation failed. One variant, because the only
/// distinction a caller acts on is "no entry" (`Ok(None)` from `get`) versus
/// "the store itself would not answer".
#[derive(Debug)]
pub struct SecretUnavailable(pub String);

/// The OS credential store, narrowed to the three operations these modules
/// need. Both levels key their entries by integration id within their own
/// service namespace, so clearing one level never disturbs the other.
pub trait SecretStore: Send + Sync {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable>;
    /// `Ok(None)` means no entry exists for `id` — a normal state after a
    /// keychain wipe, which both levels render as `key_unavailable`
    /// (AAP-FR-09 / AIC-FR-15) rather than as an error.
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable>;
    /// Removing an entry that is not there succeeds; clearing is idempotent
    /// (AAP-FR-15 / AIC-FR-14).
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable>;

    /// Whether an entry exists, without handing the key back to the caller.
    ///
    /// This is what listing uses (AAP-FR-02 / AIC-FR-02): the state a record
    /// reports depends on whether its key is still readable, and expressing
    /// that as a `bool` keeps the key out of the listing path entirely rather
    /// than relying on every call site to discard it.
    fn has(&self, id: &str) -> Result<bool, SecretUnavailable> {
        Ok(self.get(id)?.is_some())
    }

    /// `has` for many ids, answered from **one** access of the store
    /// (`ASV-application-secret-vault.md` ASV-FR-30).
    ///
    /// Every secret in the application lives in one keyring entry, so asking
    /// per record would cost one keyring access — and, where the platform asks,
    /// one authentication prompt — for each of them. A listing asks once.
    ///
    /// The default is the honest one for a store with no batch access; the
    /// vault overrides it with a single read.
    fn presence(
        &self,
        ids: &[&str],
    ) -> Result<std::collections::HashMap<String, bool>, SecretUnavailable> {
        let mut answers = std::collections::HashMap::with_capacity(ids.len());
        for id in ids {
            answers.insert((*id).to_string(), self.has(id)?);
        }
        Ok(answers)
    }
}

/// Which ids have a stored credential, gathered once and then consulted freely.
///
/// Every rule that reads a record's state needs the same answer many times over
/// — a listing resolves the active integration, then the state of each record,
/// then each record's key state — and each of those asks would otherwise be a
/// keyring access of its own. Resolving the answer once per operation is what
/// makes a listing cost one access however many records it describes
/// (`ASV-application-secret-vault.md` ASV-FR-30).
///
/// A store that will not answer yields an empty snapshot rather than an error:
/// an unreadable credential is indistinguishable, from the author's side, from
/// one that is not there, and both mean "verify this again" (AAP-FR-20 /
/// AIC-FR-24).
#[derive(Clone, Debug, Default)]
pub struct KeyPresence(std::collections::HashMap<String, bool>);

impl KeyPresence {
    /// Ask the store about every id in one access.
    pub fn resolve(secrets: &dyn SecretStore, ids: &[&str]) -> Self {
        Self(secrets.presence(ids).unwrap_or_default())
    }

    /// A snapshot asserting exactly what it is given. For tests and for callers
    /// that already know the answer.
    pub fn from_pairs<I: IntoIterator<Item = (String, bool)>>(pairs: I) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// Whether `id` has a stored credential.
    ///
    /// An id the snapshot was not asked about reads `false` — the same answer a
    /// store that holds nothing for it would give. That is the safe direction
    /// (an integration reads `key_unavailable` and the author is told to verify
    /// it again) but it does mean a caller that forgets an id in the query gets
    /// a plausible wrong answer rather than a failure, which is why the two
    /// query builders ask about whole descriptor tables rather than about
    /// whatever the current call happens to need.
    pub fn has(&self, id: &str) -> bool {
        self.0.get(id).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod key_presence_tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Store {
        entries: HashMap<String, String>,
        broken: bool,
        /// Every `presence` query this store was handed, so a test can assert a
        /// listing asked once rather than per record.
        queries: Mutex<Vec<Vec<String>>>,
    }

    impl SecretStore for Store {
        fn set(&self, _id: &str, _secret: &str) -> Result<(), SecretUnavailable> {
            Ok(())
        }
        fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
            if self.broken {
                return Err(SecretUnavailable("locked".into()));
            }
            Ok(self.entries.get(id).cloned())
        }
        fn delete(&self, _id: &str) -> Result<(), SecretUnavailable> {
            Ok(())
        }
        fn presence(
            &self,
            ids: &[&str],
        ) -> Result<HashMap<String, bool>, SecretUnavailable> {
            self.queries
                .lock()
                .unwrap()
                .push(ids.iter().map(|s| s.to_string()).collect());
            if self.broken {
                return Err(SecretUnavailable("locked".into()));
            }
            Ok(ids
                .iter()
                .map(|id| ((*id).to_string(), self.entries.contains_key(*id)))
                .collect())
        }
    }

    #[test]
    fn resolve_asks_once_and_answers_every_id() {
        let mut store = Store::default();
        store.entries.insert("a".into(), "secret".into());

        let presence = KeyPresence::resolve(&store, &["a", "b"]);

        assert!(presence.has("a"));
        assert!(!presence.has("b"));
        assert_eq!(store.queries.lock().unwrap().len(), 1);
        assert_eq!(store.queries.lock().unwrap()[0], vec!["a", "b"]);
    }

    /// An id the snapshot was never asked about reads `false` rather than
    /// panicking or reporting the last answer.
    #[test]
    fn an_unasked_id_reads_absent() {
        let store = Store::default();
        let presence = KeyPresence::resolve(&store, &["a"]);
        assert!(!presence.has("never-asked-about"));
    }

    /// A store that will not answer downgrades **every** id together, which is
    /// what the consolidated vault's all-or-nothing read actually does: there is
    /// one entry, so either it was read or it was not.
    #[test]
    fn a_store_that_refuses_downgrades_every_id() {
        let mut store = Store::default();
        store.entries.insert("a".into(), "secret".into());
        store.entries.insert("b".into(), "secret".into());
        store.broken = true;

        let presence = KeyPresence::resolve(&store, &["a", "b"]);

        assert!(!presence.has("a"));
        assert!(!presence.has("b"));
    }

    #[test]
    fn from_pairs_asserts_exactly_what_it_is_given() {
        let presence =
            KeyPresence::from_pairs([("a".to_string(), true), ("b".to_string(), false)]);
        assert!(presence.has("a"));
        assert!(!presence.has("b"));
        assert!(!presence.has("c"));
    }

    /// The default `presence` implementation is all-or-nothing too: one id the
    /// store will not answer for fails the whole query rather than returning a
    /// map with a plausible `false` in it.
    #[test]
    fn the_default_presence_implementation_is_all_or_nothing() {
        struct Defaulted(bool);
        impl SecretStore for Defaulted {
            fn set(&self, _id: &str, _secret: &str) -> Result<(), SecretUnavailable> {
                Ok(())
            }
            fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
                if self.0 && id == "b" {
                    return Err(SecretUnavailable("locked".into()));
                }
                Ok(Some("secret".into()))
            }
            fn delete(&self, _id: &str) -> Result<(), SecretUnavailable> {
                Ok(())
            }
        }

        let answers = Defaulted(false).presence(&["a", "b"]).unwrap();
        assert_eq!(answers.get("a"), Some(&true));
        assert_eq!(answers.get("b"), Some(&true));

        assert!(Defaulted(true).presence(&["a", "b"]).is_err());
    }
}

/// What a cleartext key formats as in any `Debug` output.
///
/// The types that carry a key out of these modules — the invocation and call
/// shapes handed to a caller about to present one — deliberately hand-roll
/// `Debug` around this rather than deriving it. A derived `Debug` puts the key
/// one stray `{:?}`, one `unwrap` panic, or one `assert_eq!` failure away from a
/// log line, and AAP-FR-07 / AIC-FR-20 say no log line may ever contain one.
pub fn redacted(key: &Option<String>) -> &'static str {
    match key {
        Some(_) => "<redacted>",
        None => "None",
    }
}

/// The last four characters of a key — the only key-derived text either level
/// ever returns (AAP-FR-08 / AIC-FR-20).
///
/// A key shorter than four characters yields the whole thing, which is not a
/// disclosure worth guarding: a secret that short is not a secret, and padding
/// it would misrepresent its length.
pub fn mask_hint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let start = chars.len().saturating_sub(4);
    chars[start..].iter().collect()
}

// ---------------------------------------------------------------------------
// Base URLs
// ---------------------------------------------------------------------------

/// Why a base URL was refused, before any request is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseUrlError {
    Empty,
    Invalid,
}

/// Normalise an author-supplied base URL, or say why it cannot be used
/// (AAP-FR-05 / AIC-FR-22).
///
/// Pure and checked before any network work, so an empty field and a typo are
/// reported instantly and are never mistaken for an unreachable host. Trailing
/// slashes are stripped so `…/v1` and `…/v1/` produce the same stored value and
/// the same request path.
pub fn normalize_base_url(raw: &str) -> Result<String, BaseUrlError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BaseUrlError::Empty);
    }
    let scheme_end = match trimmed.find("://") {
        Some(i) => i,
        None => return Err(BaseUrlError::Invalid),
    };
    let scheme = &trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(BaseUrlError::Invalid);
    }
    // A scheme with no authority (`https://`, `https:///v1`) is not a URL any
    // request could be made against.
    let rest = &trimmed[scheme_end + 3..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains(char::is_whitespace) {
        return Err(BaseUrlError::Invalid);
    }
    // Userinfo (`https://user:pass@host`, `https://sk-abc123@host`) is refused
    // outright rather than passed through. A base URL is persisted verbatim to
    // `synthesis.toml`, and AAP-FR-07 / AIC-FR-20 say a key is never written
    // into a URL — so an author who pastes a credential into the URL field must
    // be stopped here, not quietly have it written to a file the specs promise
    // can be attached to a bug report. No AI endpoint authenticates this way;
    // the key belongs in the key field, where the keychain can hold it.
    if authority.contains('@') {
        return Err(BaseUrlError::Invalid);
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

/// A base URL as it may appear in a log record.
///
/// A different question from [`normalize_base_url`]'s, and it has to be asked
/// separately for two reasons. That one refuses a URL carrying userinfo — but it
/// runs *inside* the verification, after the emit site that reports what is
/// being attempted, so the refusal does nothing for a record already written.
/// And a URL it accepts may still carry a query string, which is where more than
/// one gateway puts its key.
///
/// So this keeps the part a reader needs to recognise an endpoint — scheme,
/// host, port, path — and drops the parts that exist to carry credentials and
/// carry nothing else worth reading: userinfo, query, fragment. Anything it
/// cannot read as a URL at all reduces to `<invalid>` rather than being passed
/// through, because a string that is not a URL is a string nobody has reasoned
/// about (AAP-FR-07 / AIC-FR-20).
///
/// The path is kept, and a path *can* hold a key (`…/v1/sk-live-999/chat`). That
/// is a deliberate trade rather than an oversight: the same path is already
/// persisted verbatim to `synthesis.toml`, and an endpoint a reader cannot
/// recognise explains nothing, which is the whole reason to log one.
pub fn loggable_base_url(raw: &str) -> String {
    const INVALID: &str = "<invalid>";
    let trimmed = raw.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return INVALID.to_string();
    };
    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return INVALID.to_string();
    }
    let rest = &trimmed[scheme_end + 3..];
    // Everything up to the first delimiter is the authority; what follows, up to
    // a query or a fragment, is the path.
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    // `user:pass@host` and `sk-abc123@host` alike: the host is what identifies
    // the endpoint, and whatever preceded the `@` is exactly what must not be
    // recorded.
    let host = authority.rsplit('@').next().unwrap_or("");
    if host.is_empty() || host.contains(char::is_whitespace) {
        return INVALID.to_string();
    }
    let after_authority = &rest[authority_end..];
    let path_end = after_authority
        .find(['?', '#'])
        .unwrap_or(after_authority.len());
    let path = after_authority[..path_end].trim_end_matches('/');
    format!("{scheme}://{host}{path}")
}

// ---------------------------------------------------------------------------
// The endpoint prober
// ---------------------------------------------------------------------------

/// How a provider expects its key to be presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthStyle {
    /// `Authorization: Bearer <key>` — OpenAI, OpenRouter, and every
    /// OpenAI-compatible deployment.
    Bearer,
    /// `x-api-key: <key>` plus an API version header — Anthropic.
    AnthropicApiKey,
}

/// What a probe is asked to do. Grouped into a struct because the two levels
/// build it from different descriptors and passing five positional arguments
/// would invite transposing the key and the URL.
#[derive(Clone, Debug)]
pub struct ProbeRequest<'a> {
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub auth: AuthStyle,
    /// Appended to the base URL to reach the model listing, e.g. `/models`.
    pub models_path: &'a str,
    /// How strictly the response is read (AAP-FR-MDLQ).
    pub models_format: ModelsFormat,
}

/// The shape a model listing must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelsFormat {
    /// The tolerant reading every named provider uses: entries without a usable
    /// `id` are skipped.
    Lenient,
    /// The Custom gateway's standard model list (AAP-FR-MDLQ): the whole
    /// response must be valid, or the verification fails.
    Gateway,
}

/// Why reaching an endpoint did not produce a model list.
///
/// The variants are the distinctions the author acts on, and the reason the
/// prober never collapses them: a host that never answered needs a different
/// correction from one that refused the key, and both differ from a URL that
/// answers with something other than the expected API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeError {
    /// The host could not be reached at all.
    Unreachable(String),
    /// The endpoint answered and refused the credentials.
    Rejected,
    /// The endpoint answered, but not as the expected kind of API.
    NotExpectedKind,
    /// It did not answer within `PROBE_TIMEOUT`.
    TimedOut,
    /// The TLS check refused its certificate (AAP-FR-HZTB).
    TlsUntrusted(crate::tls::TlsFailure),
}

/// Reaching an endpoint, narrowed to the one question both levels ask of it:
/// "does this answer as the API I expect, and what models does it offer?"
pub trait EndpointProber: Send + Sync {
    fn probe(&self, request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError>;
}

/// Which HTTP statuses mean "the endpoint refused these credentials" and which
/// mean something else.
///
/// Pure, because the distinction the whole verification rests on must be
/// testable without a network. Only 401 and 403 are evidence *about the key*;
/// a 404 or a 500 says something about the URL or the service, and calling
/// either `Rejected` would tell the author their good key is bad. A 5xx is
/// `Unreachable` rather than `NotExpectedKind` for the same reason — the
/// endpoint may well be the right one, just unwell.
pub fn probe_failure_for_status(code: u16) -> ProbeError {
    match code {
        401 | 403 => ProbeError::Rejected,
        500..=599 => ProbeError::Unreachable(format!("the endpoint answered {code}")),
        _ => ProbeError::NotExpectedKind,
    }
}

/// Read a model list out of a `/models` payload.
///
/// `None` means the body is not the shape either API family produces, which the
/// caller reports as `NotExpectedKind` — that is what distinguishes a real
/// endpoint from a web server that happens to answer 200 at that path.
/// `Some(vec![])` is a well-formed response that lists nothing, which is a
/// successful verification whose model list degrades to the bundled catalog
/// (AAP-FR-10 / AIC-FR-08).
///
/// Both the OpenAI and Anthropic families answer with `{"data": [{"id": …}]}`,
/// so one parser serves both. Anthropic additionally carries `display_name`,
/// which is preferred for the label when present.
pub fn parse_models_payload(body: &str) -> Option<Vec<ModelOption>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let entries = value.get("data")?.as_array()?;
    let mut models = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let label = entry
            .get("display_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        if models.iter().any(|m: &ModelOption| m.id == id) {
            continue;
        }
        // AAP-FR-35: the model's own input modalities where the payload
        // declares them, under either of the two spellings the OpenAI-shaped
        // `/models` responses use, and `None` where it declares nothing. A
        // provider that says nothing is not a provider saying "no": the shipped
        // catalog is asked next, and a model neither declares anything about
        // carries `false`.
        let declared = entry
            .get("input_modalities")
            .or_else(|| entry.get("architecture").and_then(|a| a.get("input_modalities")))
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            });
        models.push(
            ModelOption::new(id, label)
                .with_declared_image_input(image_input_declared(declared.as_deref())),
        );
    }
    Some(models)
}

/// AAP-FR-MDLQ / AAP-FR-RTMZ: read a Custom gateway's model list, strictly.
///
/// Valid only if `object` is `list`, `data` is an array, and every entry has a
/// string `id` and `object` equal to `model`. `mode` is optional and, when
/// present, is `"chat"`, `"responses"`, or `null`. Every other field is
/// ignored: nothing about reasoning, images, tools, or streaming is inferred.
/// `None` means the body is not valid, and no part of it is used.
pub fn parse_gateway_models_payload(body: &str) -> Option<Vec<ModelOption>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("object")?.as_str()? != "list" {
        return None;
    }
    let entries = value.get("data")?.as_array()?;
    let mut models: Vec<ModelOption> = Vec::new();
    for entry in entries {
        let id = entry.get("id")?.as_str()?;
        if entry.get("object")?.as_str()? != "model" {
            return None;
        }
        let mode = match entry.get("mode") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(m)) if m == "chat" => Some(ModelMode::Chat),
            Some(serde_json::Value::String(m)) if m == "responses" => Some(ModelMode::Responses),
            Some(_) => return None,
        };
        if id.is_empty() || models.iter().any(|m| m.id == id) {
            continue;
        }
        models.push(ModelOption::new(id, id).with_mode(mode));
    }
    Some(models)
}

/// Production prober: one `GET <base_url><models_path>` under a bounded global
/// timeout.
pub struct HttpEndpointProber;

impl EndpointProber for HttpEndpointProber {
    fn probe(&self, request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError> {
        let config = crate::tls::ureq_config().timeout_global(Some(PROBE_TIMEOUT));
        let agent: ureq::Agent = config.build().into();

        let url = format!("{}{}", request.base_url, request.models_path);
        let mut call = agent.get(&url).header("User-Agent", "synthesis");
        if let Some(key) = request.api_key.filter(|k| !k.is_empty()) {
            call = match request.auth {
                AuthStyle::Bearer => call.header("Authorization", &format!("Bearer {key}")),
                AuthStyle::AnthropicApiKey => call
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01"),
            };
        }

        let mut response = match call.call() {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(code)) => return Err(probe_failure_for_status(code)),
            Err(ureq::Error::Timeout(_)) => return Err(ProbeError::TimedOut),
            Err(e) => {
                return Err(match crate::tls::ureq_failure(&e, &url) {
                    Some(failure) => ProbeError::TlsUntrusted(failure),
                    None => ProbeError::Unreachable(e.to_string()),
                })
            }
        };

        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| ProbeError::Unreachable(e.to_string()))?;
        match request.models_format {
            ModelsFormat::Lenient => parse_models_payload(&body),
            ModelsFormat::Gateway => parse_gateway_models_payload(&body),
        }
        .ok_or(ProbeError::NotExpectedKind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    // -- mask_hint -------------------------------------------------------

    #[test]
    fn mask_hint_is_the_last_four_characters_and_nothing_more() {
        assert_eq!(mask_hint("sk-proj-abcdefgh3f9a"), "3f9a");
        // A short key yields itself: padding would misrepresent its length, and
        // there is nothing to protect.
        assert_eq!(mask_hint("ab"), "ab");
        assert_eq!(mask_hint(""), "");
        // Multi-byte characters count as characters, not bytes — slicing by
        // byte offset here would panic.
        assert_eq!(mask_hint("key-日本語テスト"), "語テスト");
    }

    // -- normalize_base_url ----------------------------------------------

    #[test]
    fn an_empty_or_malformed_base_url_is_refused_without_a_request() {
        assert_eq!(normalize_base_url(""), Err(BaseUrlError::Empty));
        assert_eq!(normalize_base_url("   "), Err(BaseUrlError::Empty));
        assert_eq!(normalize_base_url("api.openai.com/v1"), Err(BaseUrlError::Invalid));
        assert_eq!(normalize_base_url("ftp://example.com"), Err(BaseUrlError::Invalid));
        // A scheme with no authority is not something a request can be made to.
        assert_eq!(normalize_base_url("https://"), Err(BaseUrlError::Invalid));
        assert_eq!(normalize_base_url("https:///v1"), Err(BaseUrlError::Invalid));
        assert_eq!(
            normalize_base_url("https://exa mple.com"),
            Err(BaseUrlError::Invalid)
        );
    }

    #[test]
    fn a_credential_smuggled_into_the_url_is_refused_rather_than_stored() {
        // AAP-FR-07 / AIC-FR-20: a key is never written into a URL. A base URL
        // is persisted verbatim, so accepting userinfo here would put a pasted
        // key straight into `synthesis.toml` — which the specs promise can be
        // attached to a bug report — and no assertion about the `api_key`
        // argument could ever catch it.
        assert_eq!(
            normalize_base_url("https://sk-abc123@api.openai.com/v1"),
            Err(BaseUrlError::Invalid)
        );
        assert_eq!(
            normalize_base_url("https://user:pass@host/v1"),
            Err(BaseUrlError::Invalid)
        );
        // Only the *authority* is policed: an `@` later in the path is ordinary.
        assert_eq!(
            normalize_base_url("https://host/v1/@scope").unwrap(),
            "https://host/v1/@scope"
        );
    }

    #[test]
    fn a_usable_base_url_is_normalised_so_one_path_is_produced() {
        assert_eq!(
            normalize_base_url("  https://api.openai.com/v1/  ").unwrap(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1").unwrap(),
            "https://api.openai.com/v1"
        );
        // A locally-running model is an ordinary base URL: plain http and a
        // port are both fine (AAP-FR-03).
        assert_eq!(
            normalize_base_url("http://localhost:11434/v1").unwrap(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            normalize_base_url("HTTPS://Example.com/v1").unwrap(),
            "HTTPS://Example.com/v1"
        );
    }

    // -- loggable_base_url -----------------------------------------------

    #[test]
    fn a_logged_base_url_keeps_the_endpoint_and_drops_what_can_carry_a_key() {
        // The two things `normalize_base_url` cannot do for a log record: it
        // runs after the emit site that reports an attempt, and it accepts a
        // query string.
        assert_eq!(
            loggable_base_url("https://sk-abc123@api.openai.com/v1"),
            "https://api.openai.com/v1",
        );
        assert_eq!(
            loggable_base_url("https://user:pass@gateway.internal:8443/v1/"),
            "https://gateway.internal:8443/v1",
        );
        assert_eq!(
            loggable_base_url("https://gateway.example/v1?api-key=sk-live-999"),
            "https://gateway.example/v1",
        );
        assert_eq!(
            loggable_base_url("https://gateway.example/v1#token=sk-live-999"),
            "https://gateway.example/v1",
        );
        // The authority is read to the *last* `@`, so no credential-shaped
        // segment before the host survives however many there are.
        assert_eq!(
            loggable_base_url("https://user@sk-abc123@api.openai.com/v1"),
            "https://api.openai.com/v1",
        );
        // Percent-encoding inside the userinfo changes nothing: the literal `@`
        // is still what delimits it, and everything before it goes.
        assert_eq!(
            loggable_base_url("https://sk%2Dabc123@api.openai.com/v1"),
            "https://api.openai.com/v1",
        );
        // An ordinary URL survives intact, which is the whole point of logging
        // it: an endpoint a reader cannot recognise explains nothing.
        assert_eq!(
            loggable_base_url("  http://localhost:11434/v1  "),
            "http://localhost:11434/v1",
        );
        // An IPv6 literal is a host like any other — the brackets and the port
        // must survive the authority split.
        assert_eq!(
            loggable_base_url("http://[::1]:8080/v1"),
            "http://[::1]:8080/v1",
        );
        // An `@` in the *path* is ordinary (a scoped package, say). The
        // authority ends at the first `/`, so it is never mistaken for userinfo
        // — the same distinction `normalize_base_url` draws.
        assert_eq!(
            loggable_base_url("https://registry.example/v1/@scope/pkg"),
            "https://registry.example/v1/@scope/pkg",
        );
        // Anything that is not a URL is not passed through: a string nobody has
        // reasoned about is a string that may hold anything.
        for junk in ["", "   ", "api.openai.com/v1", "ftp://host/v1", "https://", "sk-abc123"] {
            assert_eq!(loggable_base_url(junk), "<invalid>", "{junk:?}");
        }
    }

    // -- probe_failure_for_status ----------------------------------------

    #[test]
    fn only_an_auth_status_is_evidence_about_the_key() {
        assert_eq!(probe_failure_for_status(401), ProbeError::Rejected);
        assert_eq!(probe_failure_for_status(403), ProbeError::Rejected);
        // A URL that answers but not as the API: the endpoint is wrong, the key
        // is not in question.
        assert_eq!(probe_failure_for_status(404), ProbeError::NotExpectedKind);
        assert_eq!(probe_failure_for_status(418), ProbeError::NotExpectedKind);
        // The right endpoint, unwell — telling the author their key is bad
        // would send them to fix the wrong thing.
        assert!(matches!(
            probe_failure_for_status(503),
            ProbeError::Unreachable(_)
        ));
    }

    // -- parse_models_payload --------------------------------------------

    #[test]
    fn an_openai_shaped_payload_yields_its_models_in_order() {
        let models = parse_models_payload(
            r#"{"object":"list","data":[{"id":"gpt-5","object":"model"},{"id":"gpt-4o"}]}"#,
        )
        .unwrap();
        assert_eq!(
            models,
            vec![ModelOption::new("gpt-5", "gpt-5"), ModelOption::new("gpt-4o", "gpt-4o")]
        );
    }

    #[test]
    fn an_anthropic_payload_prefers_the_display_name_for_the_label() {
        let models = parse_models_payload(
            r#"{"data":[{"id":"claude-opus-5","display_name":"Claude Opus 5"}]}"#,
        )
        .unwrap();
        assert_eq!(models, vec![ModelOption::new("claude-opus-5", "Claude Opus 5")]);
    }

    #[test]
    fn a_well_formed_payload_listing_nothing_is_success_not_a_failure() {
        // AAP-FR-10: an empty list degrades the model list to the bundled
        // catalog; it must not read as "this is not an AI endpoint".
        assert_eq!(parse_models_payload(r#"{"data":[]}"#), Some(vec![]));
    }

    #[test]
    fn a_body_that_is_not_a_model_listing_is_rejected_as_the_wrong_kind() {
        assert_eq!(parse_models_payload("<html>hello</html>"), None);
        assert_eq!(parse_models_payload(r#"{"error":"nope"}"#), None);
        assert_eq!(parse_models_payload(r#"{"data":"not-an-array"}"#), None);
    }

    #[test]
    fn malformed_entries_are_skipped_rather_than_failing_the_whole_list() {
        // The probe is best-effort enrichment: one odd entry must not cost the
        // author the rest of the list.
        let models = parse_models_payload(
            r#"{"data":[{"id":"a"},{"no-id":true},{"id":""},{"id":"a"},{"id":"b"}]}"#,
        )
        .unwrap();
        assert_eq!(
            models,
            vec![ModelOption::new("a", "a"), ModelOption::new("b", "b")]
        );
    }
}
