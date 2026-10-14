//! Detection of an Ollama host's per-model values (AAP-FR-QRAX, AAP-FR-JQIJ,
//! AAP-FR-KJSL).
//!
//! After a valid Custom model list, verification asks `POST <base>/api/show`
//! for every model. A host that is not Ollama answers with an error, and that
//! only means "no detected values": detection never fails a verification and
//! never changes the model list.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::ai_shared::{ModelOption, PROBE_TIMEOUT};

/// AAP-FR-QRAX: the most detection requests in flight at one time.
pub const MAX_CONCURRENT_DETECTIONS: usize = 4;

/// The route detection asks, under the Custom base URL.
pub const SHOW_PATH: &str = "/api/show";

/// What one detection response gives a model (AAP-FR-JQIJ).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detected {
    pub context_size: Option<u64>,
    pub capabilities: Option<Vec<String>>,
}

/// The seam detection reaches a host through, so every rule here is testable
/// without a network.
pub trait ModelDetector: Send + Sync {
    /// The values the host reports for one model, or `None` on any failure
    /// (AAP-FR-KJSL). The secret goes into the request and nowhere else.
    fn show(&self, base_url: &str, api_key: Option<&str>, model_id: &str) -> Option<Detected>;
}

/// A detector that detects nothing. The default of a module built for tests,
/// so that no test sends a request by accident.
pub struct NoDetection;

impl ModelDetector for NoDetection {
    fn show(&self, _: &str, _: Option<&str>, _: &str) -> Option<Detected> {
        None
    }
}

/// AAP-FR-JQIJ: read the detected values out of an `/api/show` body.
///
/// The context size is the positive whole number at
/// `model_info.<architecture>.context_length`, where the architecture is
/// `model_info["general.architecture"]`. The capabilities are the strings of
/// the top-level `capabilities` array. A value that is not in that form is
/// `None`, and a body that is not a JSON object is `None` as a whole.
pub fn parse_show_payload(body: &str) -> Option<Detected> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let info = object.get("model_info").and_then(|i| i.as_object());
    let context_size = info.and_then(|info| {
        let architecture = info.get("general.architecture")?.as_str()?;
        info.get(&format!("{architecture}.context_length"))?.as_u64().filter(|n| *n > 0)
    });
    let capabilities = object.get("capabilities").and_then(|c| c.as_array()).and_then(|list| {
        list.iter().map(|c| c.as_str().map(str::to_string)).collect::<Option<Vec<_>>>()
    });
    Some(Detected { context_size, capabilities })
}

/// Production detector: one `POST <base>/api/show` under the bounded timeout of
/// AAP-FR-05, with the same bearer secret the model list was asked with.
pub struct HttpModelDetector;

impl ModelDetector for HttpModelDetector {
    fn show(&self, base_url: &str, api_key: Option<&str>, model_id: &str) -> Option<Detected> {
        let config = crate::tls::ureq_config().timeout_global(Some(PROBE_TIMEOUT));
        let agent: ureq::Agent = config.build().into();
        let url = format!("{}{}", base_url.trim_end_matches('/'), SHOW_PATH);
        let mut call = agent
            .post(&url)
            .header("User-Agent", "synthesis")
            .header("Content-Type", "application/json");
        if let Some(key) = api_key.filter(|k| !k.is_empty()) {
            call = call.header("Authorization", &format!("Bearer {key}"));
        }
        // AAP-FR-KJSL: every failure is "no detected values". The error itself
        // is dropped here, so it cannot carry the request into a log.
        let mut response = call.send(serde_json::json!({ "model": model_id }).to_string()).ok()?;
        let body = response.body_mut().read_to_string().ok()?;
        parse_show_payload(&body)
    }
}

/// AAP-FR-QRAX / AAP-FR-KJSL: ask the host for every model, with at most
/// `MAX_CONCURRENT_DETECTIONS` requests in flight, and replace the detected
/// values of each model. The list keeps its models and its order.
pub fn detect_all(
    detector: &dyn ModelDetector,
    base_url: &str,
    api_key: Option<&str>,
    models: Vec<ModelOption>,
) -> Vec<ModelOption> {
    let results: Vec<Mutex<Option<Detected>>> = models.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let workers = MAX_CONCURRENT_DETECTIONS.min(models.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(model) = models.get(index) else { break };
                let detected = detector.show(base_url, api_key, &model.id);
                if let Ok(mut slot) = results[index].lock() {
                    *slot = detected;
                }
            });
        }
    });
    models
        .into_iter()
        .zip(results)
        .map(|(mut model, result)| {
            let detected = result.into_inner().ok().flatten().unwrap_or_default();
            model.detected_context_size = detected.context_size;
            model.detected_capabilities = detected.capabilities;
            model
        })
        .collect()
}

/// AAP-FR-OVDB: how many models were sent and how many have detected values.
pub fn detection_counts(models: &[ModelOption]) -> (usize, usize) {
    let detected = models
        .iter()
        .filter(|m| m.detected_context_size.is_some() || m.detected_capabilities.is_some())
        .count();
    (models.len(), detected)
}
