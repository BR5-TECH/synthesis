//! The OpenRouter client path — `specifications/core/AAP-ai-api-integrations.md`
//! AAP-FR-23 / FR-24 / FR-25 / FR-26.
//!
//! OpenRouter is the one provider reached through an SDK rather than through the
//! generic `ureq` prober of `ai_shared`. That is not a stylistic choice: it is
//! the only provider whose model listing reports, per model, *what reasoning
//! that model can be asked for*, and the SDK is what keeps this module from
//! re-deriving that descriptor by hand from a payload whose shape is the
//! provider's to change.
//!
//! Two properties this module is responsible for:
//!
//! - **The blast radius of the dependency stops here** (AAP-FR-23). Only
//!   `openrouter` routes through it; Anthropic, OpenAI, and Custom keep the
//!   `ureq` path, so an SDK that breaks cannot cost the application its ability
//!   to reach the other three.
//! - **The async runtime is confined to one owned thread.** The SDK is async and
//!   every call path in `ai_api` is synchronous. Rather than colour the whole
//!   module async, each probe runs a current-thread runtime on a thread this
//!   function owns and joins. A dedicated thread rather than a bare `block_on`
//!   is deliberate: a Tauri command may already be executing inside a Tokio
//!   worker, and driving a nested runtime from there panics at runtime — a
//!   failure that no test without the full Tauri runtime would catch.

use openrouter_rs::api::models::Model;

use crate::ai_shared::{
    EndpointProber, ModelOption, ModelReasoning, ProbeError, ProbeRequest, PROBE_TIMEOUT,
};

/// Turn one of the SDK's effort levels into the identifier this application
/// carries (AAP-FR-26).
///
/// `Effort` has an `Other(String)` variant for levels the SDK does not know by
/// name, and `as_str` yields it verbatim — so a level neither this application
/// nor the SDK has heard of still reaches the author intact. An empty
/// identifier is dropped: it would render as a blank row nothing could select.
fn effort_id(effort: &openrouter_rs::types::Effort) -> Option<String> {
    let id = effort.as_str();
    (!id.is_empty()).then(|| id.to_string())
}

/// Map one SDK model onto a `ModelOption`, carrying its reasoning descriptor.
fn model_option(model: &Model) -> ModelOption {
    let label = if model.name.trim().is_empty() {
        model.id.clone()
    } else {
        model.name.clone()
    };
    let reasoning = model.reasoning.as_ref().map(|r| {
        let supported: Option<Vec<String>> = r
            .supported_efforts
            .as_ref()
            .map(|efforts| efforts.iter().filter_map(effort_id).collect::<Vec<_>>())
            // A ladder that mapped to nothing usable is no ladder, not an empty
            // one: an empty list would render as a selector with no levels in it.
            .filter(|l: &Vec<String>| !l.is_empty());
        let default_effort = r.default_effort.as_ref().and_then(effort_id);
        ModelReasoning {
            mandatory: r.mandatory,
            default_enabled: r.default_enabled,
            // The provider's own invariant is that `default_effort` is a member
            // of `supported_efforts`. It is enforced rather than assumed: a
            // default outside the ladder would be a choice the author can see
            // but never re-select.
            default_effort: default_effort.filter(|d| {
                supported.as_ref().is_some_and(|l| l.iter().any(|e| e == d))
            }),
            supported_efforts: supported,
        }
    });
    // AAP-FR-35: OpenRouter declares each model's input modalities in its
    // architecture block. What it declares settles the capability; a model whose
    // block carries no modality list at all leaves the question to the shipped
    // catalog, and a model neither declares anything about takes no image.
    let images = crate::ai_shared::image_input_declared(
        model.architecture.input_modalities.as_deref(),
    );
    ModelOption::new(&model.id, &label)
        .with_reasoning(reasoning)
        .with_declared_image_input(images)
}

/// Map an SDK failure onto the shared probe vocabulary.
///
/// The distinction the author acts on is preserved: a refused key and an
/// unreachable host call for different corrections, so they never collapse into
/// one another (AAP-FR-05).
fn probe_error_for(e: &openrouter_rs::error::OpenRouterError) -> ProbeError {
    use openrouter_rs::error::OpenRouterError;
    match e {
        // The status is the same evidence the generic prober reasons from, so
        // the same rule decides it: only 401/403 say anything about the key.
        OpenRouterError::Api(context) => {
            crate::ai_shared::probe_failure_for_status(context.status.as_u16())
        }
        // A body that will not deserialise is an endpoint answering as something
        // other than OpenRouter — the same conclusion the generic prober draws
        // from a payload it cannot parse.
        OpenRouterError::Serialization(_) => ProbeError::NotExpectedKind,
        // A key the SDK refuses to send at all never reaches the endpoint, but
        // it is a credential problem and reads as one.
        OpenRouterError::KeyNotConfigured => ProbeError::Rejected,
        OpenRouterError::HttpRequest(inner) => {
            ProbeError::Unreachable(inner.message().to_string())
        }
        other => ProbeError::Unreachable(other.to_string()),
    }
}

/// The `openrouter` provider's prober (AAP-FR-23).
pub struct OpenRouterProber;

impl EndpointProber for OpenRouterProber {
    fn probe(&self, request: &ProbeRequest<'_>) -> Result<Vec<ModelOption>, ProbeError> {
        // The SDK takes the key as a plain `&str`. An absent key probes as an
        // empty one, which OpenRouter answers with a 401 — surfacing as
        // `rejected`, which is what it is.
        let base_url = request.base_url.to_string();
        let api_key = request.api_key.unwrap_or_default().to_string();

        // The key is moved into a thread this function joins before returning,
        // so it lives no longer than the probe itself (AAP-FR-07).
        std::thread::scope(|scope| {
            let handle = scope.spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| ProbeError::Unreachable(e.to_string()))?;
                // AAP-FR-WMCX: the SDK's own free functions build a client with
                // the default trust setup, so the client is built here from the
                // shared roots and handed over.
                let (http_client, tls_record) = crate::tls::openrouter_http_client()
                    .map_err(|_| ProbeError::Unreachable("the HTTP client could not be built".into()))?;
                let client = openrouter_rs::OpenRouterClient::builder()
                    .base_url(base_url.clone())
                    .api_key(api_key.clone())
                    .http_client(http_client)
                    .build()
                    .map_err(|e| ProbeError::Unreachable(e.to_string()))?;
                runtime.block_on(async {
                    // The SDK applies no deadline of its own, so the same bound
                    // every other probe honours is imposed here. Without it an
                    // endpoint that never answers would hold the settings
                    // surface's Verify action open indefinitely.
                    match tokio::time::timeout(PROBE_TIMEOUT, client.models().list()).await {
                        Err(_elapsed) => Err(ProbeError::TimedOut),
                        // AAP-FR-HZTB: the SDK flattens a TLS error into text,
                        // so the refused certificate is read from the shared
                        // verifier's record.
                        Ok(Err(e)) => Err(match (&e, tls_record.take()) {
                            // A structured answer had a good handshake.
                            (openrouter_rs::error::OpenRouterError::Api(_), _) | (_, None) => {
                                probe_error_for(&e)
                            }
                            (_, Some(failure)) => ProbeError::TlsUntrusted(failure),
                        }),
                        Ok(Ok(models)) => Ok(models.iter().map(model_option).collect()),
                    }
                })
            });
            match handle.join() {
                Ok(result) => result,
                Err(_) => Err(ProbeError::Unreachable(
                    "the OpenRouter client panicked".to_string(),
                )),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrouter_rs::types::Effort;

    // The SDK's model type is `#[non_exhaustive]` and cannot be built with
    // struct literal syntax from outside its crate, so these fixtures go
    // through deserialisation — which has the side benefit of pinning the
    // *wire* shape this module depends on rather than an in-memory one.
    fn model_from(json: &str) -> Model {
        serde_json::from_str(json).expect("fixture parses as a model")
    }

    /// The minimum an OpenRouter model entry carries, so a fixture only has to
    /// state the fields under test.
    fn model_json(id: &str, name: &str, reasoning: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"{name}","created":0.0,"description":"",
                 "architecture":{{}},"top_provider":{{"is_moderated":false}},
                 "pricing":{{"prompt":"0","completion":"0"}},
                 "supported_parameters":["reasoning"]{reasoning}}}"#
        )
    }

    #[test]
    fn a_models_ladder_is_carried_through_in_the_order_the_provider_declares() {
        // AAP-FR-24 / FR-26: the ladder is the model's, in its own order.
        let model = model_from(&model_json(
            "anthropic/claude-opus-5",
            "Claude Opus 5",
            r#","reasoning":{"mandatory":false,"default_enabled":true,
                "supported_efforts":["max","xhigh","high","medium","low"],
                "default_effort":"high"}"#,
        ));
        let option = model_option(&model);
        assert_eq!(option.id, "anthropic/claude-opus-5");
        assert_eq!(option.label, "Claude Opus 5");
        let r = option.reasoning.expect("declares reasoning");
        assert!(!r.mandatory);
        assert_eq!(
            r.supported_efforts.as_deref(),
            Some(["max", "xhigh", "high", "medium", "low"].map(String::from).as_slice())
        );
        assert_eq!(r.default_effort.as_deref(), Some("high"));
        assert_eq!(r.default_enabled, Some(true));
    }

    #[test]
    fn a_model_that_reasons_without_a_ladder_carries_a_descriptor_and_no_efforts() {
        // The common case (135 of the catalogue): reasoning, but no levels —
        // which the UI renders as off/on rather than as a ladder (AII-FR-43).
        let model = model_from(&model_json(
            "deepseek/deepseek-r1",
            "DeepSeek R1",
            r#","reasoning":{"mandatory":true}"#,
        ));
        let r = model_option(&model).reasoning.expect("declares reasoning");
        assert!(r.mandatory, "this model cannot be asked to stop reasoning");
        assert_eq!(r.supported_efforts, None);
        assert_eq!(r.default_effort, None);
    }

    #[test]
    fn a_model_declaring_no_reasoning_carries_none() {
        // AAP-FR-24: `None`, not an empty descriptor — the UI renders no
        // reasoning control at all for it (AII-FR-41).
        let model = model_from(&model_json("kwaipilot/kat-coder", "KAT Coder", ""));
        assert_eq!(model_option(&model).reasoning, None);
    }

    #[test]
    fn a_level_the_sdk_does_not_know_by_name_still_reaches_the_author() {
        // AAP-FR-26: the model's ladder is authoritative. The SDK models an
        // unknown level as `Effort::Other`, and it must survive the crossing
        // rather than being dropped as unrecognised.
        assert_eq!(
            effort_id(&Effort::Other("ludicrous".into())).as_deref(),
            Some("ludicrous")
        );
        assert_eq!(effort_id(&Effort::High).as_deref(), Some("high"));

        let model = model_from(&model_json(
            "acme/model",
            "Acme",
            r#","reasoning":{"mandatory":false,
                "supported_efforts":["ludicrous","high"],
                "default_effort":"ludicrous"}"#,
        ));
        let r = model_option(&model).reasoning.expect("declares reasoning");
        assert!(r.offers_effort("ludicrous"));
        assert_eq!(r.default_effort.as_deref(), Some("ludicrous"));
    }

    #[test]
    fn a_default_effort_outside_the_ladder_is_dropped_rather_than_offered() {
        // A default the selector could never re-select is worse than none: the
        // author would see a level, change it, and be unable to get back to it.
        let model = model_from(&model_json(
            "acme/model",
            "Acme",
            r#","reasoning":{"mandatory":false,
                "supported_efforts":["high","low"],"default_effort":"medium"}"#,
        ));
        let r = model_option(&model).reasoning.expect("declares reasoning");
        assert_eq!(r.default_effort, None);
        assert!(!r.offers_effort("medium"));
    }

    #[test]
    fn an_empty_ladder_reads_as_no_ladder_at_all() {
        let model = model_from(&model_json(
            "acme/model",
            "Acme",
            r#","reasoning":{"mandatory":false,"supported_efforts":[]}"#,
        ));
        let r = model_option(&model).reasoning.expect("declares reasoning");
        assert_eq!(
            r.supported_efforts, None,
            "an empty ladder is off/on, not a selector with nothing in it"
        );
    }

    #[test]
    fn a_model_with_no_name_falls_back_to_its_id_for_a_label() {
        let model = model_from(&model_json("acme/model", "", ""));
        assert_eq!(model_option(&model).label, "acme/model");
    }

    #[test]
    fn an_auth_failure_stays_distinguishable_from_an_unreachable_host() {
        // AAP-FR-05: the two call for different corrections by the author, so
        // the SDK's error taxonomy must not collapse them.
        use openrouter_rs::error::{ApiErrorContext, ApiErrorKind, OpenRouterError};

        let api = |code: u16| {
            OpenRouterError::Api(Box::new(ApiErrorContext {
                status: http::StatusCode::from_u16(code).unwrap(),
                api_code: None,
                message: "no".into(),
                request_id: None,
                metadata: None,
                kind: ApiErrorKind::Generic,
            }))
        };

        assert_eq!(probe_error_for(&api(401)), ProbeError::Rejected);
        assert_eq!(probe_error_for(&api(403)), ProbeError::Rejected);
        // A URL that answers but not as this API: the endpoint is wrong, the
        // key is not in question.
        assert_eq!(probe_error_for(&api(404)), ProbeError::NotExpectedKind);
        // The right endpoint, unwell — reporting a bad key would send the
        // author to fix the wrong thing.
        assert!(matches!(probe_error_for(&api(503)), ProbeError::Unreachable(_)));

        assert!(matches!(
            probe_error_for(&OpenRouterError::HttpRequest(
                openrouter_rs::error::HttpRequestError::new("dns failure")
            )),
            ProbeError::Unreachable(_)
        ));
        assert_eq!(
            probe_error_for(&OpenRouterError::Serialization(
                serde_json::from_str::<serde_json::Value>("<html>").unwrap_err()
            )),
            ProbeError::NotExpectedKind
        );
    }
}

/// A live check of the whole client path, against the real endpoint.
///
/// `#[ignore]` because CI has no outbound network and AAP's non-functional
/// requirements say the suite must run without one. Run it deliberately with
/// `cargo test --lib -- --ignored openrouter_live`. It needs no key: the model
/// listing is public, which is also why it can prove the descriptor mapping
/// without touching the author's credential.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn openrouter_live_probe_reports_per_model_reasoning() {
        let models = OpenRouterProber
            .probe(&ProbeRequest {
                base_url: "https://openrouter.ai/api/v1",
                api_key: None,
                auth: crate::ai_shared::AuthStyle::Bearer,
                models_path: "/models",
                models_format: crate::ai_shared::ModelsFormat::Lenient,
                report_status: false,
            })
            .expect("the public model listing answers");

        assert!(models.len() > 100, "got {} models", models.len());

        let with_ladder = models
            .iter()
            .filter(|m| {
                m.reasoning
                    .as_ref()
                    .is_some_and(|r| r.supported_efforts.is_some())
            })
            .count();
        let with_any = models.iter().filter(|m| m.reasoning.is_some()).count();
        let mandatory = models
            .iter()
            .filter(|m| m.reasoning.as_ref().is_some_and(|r| r.mandatory))
            .count();
        let labelled = models.iter().filter(|m| m.label != m.id).count();

        println!(
            "models={} with_reasoning={} with_ladder={} mandatory={} labelled={}",
            models.len(),
            with_any,
            with_ladder,
            mandatory,
            labelled
        );
        if let Some(m) = models
            .iter()
            .find(|m| m.reasoning.as_ref().is_some_and(|r| r.supported_efforts.is_some()))
        {
            println!("  sample: {} -> {:?}", m.id, m.reasoning);
        }

        assert!(with_ladder > 0, "no model reported an effort ladder");
        assert!(mandatory > 0, "no model reported mandatory reasoning");
        assert!(
            labelled > 0,
            "labels came back equal to ids — the SDK's `name` was not used"
        );
    }
}
