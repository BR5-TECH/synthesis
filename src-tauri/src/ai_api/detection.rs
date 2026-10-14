//! Detection of an Ollama host's per-model values (AAP-FR-QRAX, AAP-FR-JQIJ,
//! AAP-FR-KJSL).
//!
//! After a valid Custom model list, verification asks `POST <base>/api/show`
//! for every model. A host that is not Ollama answers with an error, and that
//! only means "no detected values": detection never fails a verification and
//! never changes the model list.

use std::collections::BTreeMap;
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

impl Detected {
    /// Whether the response gave the model at least one value.
    pub fn has_values(&self) -> bool {
        self.context_size.is_some() || self.capabilities.is_some()
    }
}

/// Why one detection request gave no values (AAP-FR-KJSL). The kind alone:
/// it holds no URL, no body, and no secret, so a log record can name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionFailure {
    /// The host answered with a status that is not a success.
    Status(u16),
    /// The request did not complete in the bounded timeout.
    Timeout,
    /// The host could not be reached, or the connection failed.
    Network,
    /// The body is not the expected shape.
    BadBody,
}

impl DetectionFailure {
    /// The name of the kind, as a log record names it (AAP-FR-OVDB).
    pub fn kind(self) -> String {
        match self {
            DetectionFailure::Status(code) => format!("status_{code}"),
            DetectionFailure::Timeout => "timeout".to_string(),
            DetectionFailure::Network => "network".to_string(),
            DetectionFailure::BadBody => "bad_body".to_string(),
        }
    }
}

/// AAP-FR-OVDB: what one detection run did, in counts only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetectionReport {
    /// The number of models a request was sent for.
    pub sent: usize,
    /// The number of models with at least one detected value.
    pub detected: usize,
    /// The number of requests that failed.
    pub failed: usize,
    /// The failed requests by kind (`DetectionFailure::kind`).
    pub failures: BTreeMap<String, usize>,
}

impl DetectionReport {
    /// The failure kinds as one text, for example `status_404:2,timeout:1`.
    pub fn failure_kinds(&self) -> String {
        self.failures.iter().map(|(k, n)| format!("{k}:{n}")).collect::<Vec<_>>().join(",")
    }
}

/// The seam detection reaches a host through, so every rule here is testable
/// without a network.
pub trait ModelDetector: Send + Sync {
    /// The values the host reports for one model, or why it gave none
    /// (AAP-FR-KJSL). The secret goes into the request and nowhere else.
    fn show(
        &self,
        base_url: &str,
        api_key: Option<&str>,
        model_id: &str,
    ) -> Result<Detected, DetectionFailure>;
}

/// A detector that detects nothing and sends nothing. The default of a module
/// built for tests, so that no test sends a request by accident.
pub struct NoDetection;

impl ModelDetector for NoDetection {
    fn show(&self, _: &str, _: Option<&str>, _: &str) -> Result<Detected, DetectionFailure> {
        Ok(Detected::default())
    }
}

/// AAP-FR-JQIJ: read the detected values out of an `/api/show` body.
///
/// The context size is the positive whole number at
/// `model_info.<architecture>.context_length`, where the architecture is
/// `model_info["general.architecture"]`. The capabilities are the strings of
/// the top-level `capabilities` array. A value that is not in that form is
/// `None`, and a body that is not a JSON object is `None` as a whole.
///
/// A context size above `TOKEN_LIMIT_MAX` is not in the expected form either:
/// the store cannot hold it, and a value that fails the save would fail the
/// verification (AAP-FR-KJSL).
pub fn parse_show_payload(body: &str) -> Option<Detected> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let info = object.get("model_info").and_then(|i| i.as_object());
    let context_size = info.and_then(|info| {
        let architecture = info.get("general.architecture")?.as_str()?;
        info.get(&format!("{architecture}.context_length"))?
            .as_u64()
            .filter(|n| (1..=super::custom_limits::TOKEN_LIMIT_MAX).contains(n))
    });
    let capabilities = object.get("capabilities").and_then(|c| c.as_array()).and_then(|list| {
        list.iter().map(|c| c.as_str().map(str::to_string)).collect::<Option<Vec<_>>>()
    });
    Some(Detected { context_size, capabilities })
}

/// Production detector: one `POST <base>/api/show` under the bounded timeout of
/// AAP-FR-05, with the same bearer secret the model list was asked with. One
/// agent serves every request, so the requests share its connections.
pub struct HttpModelDetector {
    agent: ureq::Agent,
}

impl HttpModelDetector {
    pub fn new() -> Self {
        let config = crate::tls::ureq_config().timeout_global(Some(PROBE_TIMEOUT));
        Self { agent: config.build().into() }
    }
}

impl Default for HttpModelDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelDetector for HttpModelDetector {
    fn show(
        &self,
        base_url: &str,
        api_key: Option<&str>,
        model_id: &str,
    ) -> Result<Detected, DetectionFailure> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), SHOW_PATH);
        let mut call = self
            .agent
            .post(&url)
            .header("User-Agent", "synthesis")
            .header("Content-Type", "application/json");
        if let Some(key) = api_key.filter(|k| !k.is_empty()) {
            call = call.header("Authorization", &format!("Bearer {key}"));
        }
        // AAP-FR-KJSL: every failure is "no detected values". Only the kind of
        // the error is kept, so it cannot carry the request into a log.
        let mut response = call
            .send(serde_json::json!({ "model": model_id }).to_string())
            .map_err(|e| match e {
                ureq::Error::StatusCode(code) => DetectionFailure::Status(code),
                ureq::Error::Timeout(_) => DetectionFailure::Timeout,
                _ => DetectionFailure::Network,
            })?;
        let body = response.body_mut().read_to_string().map_err(|e| match e {
            ureq::Error::Timeout(_) => DetectionFailure::Timeout,
            _ => DetectionFailure::BadBody,
        })?;
        parse_show_payload(&body).ok_or(DetectionFailure::BadBody)
    }
}

/// AAP-FR-QRAX / AAP-FR-KJSL: ask the host for every model, with at most
/// `MAX_CONCURRENT_DETECTIONS` requests in flight, and replace the detected
/// values of each model. The list keeps its models and its order. The report
/// counts what happened (AAP-FR-OVDB).
pub fn detect_all(
    detector: &dyn ModelDetector,
    base_url: &str,
    api_key: Option<&str>,
    models: Vec<ModelOption>,
) -> (Vec<ModelOption>, DetectionReport) {
    type Slot = Mutex<Option<Result<Detected, DetectionFailure>>>;
    let results: Vec<Slot> = models.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let workers = MAX_CONCURRENT_DETECTIONS.min(models.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(model) = models.get(index) else { break };
                let outcome = detector.show(base_url, api_key, &model.id);
                if let Ok(mut slot) = results[index].lock() {
                    *slot = Some(outcome);
                }
            });
        }
    });
    let mut report = DetectionReport { sent: models.len(), ..Default::default() };
    let models = models
        .into_iter()
        .zip(results)
        .map(|(mut model, result)| {
            // A worker that panicked left its slot empty; that is a failure
            // like any other.
            let outcome =
                result.into_inner().ok().flatten().unwrap_or(Err(DetectionFailure::Network));
            let detected = match outcome {
                Ok(detected) => detected,
                Err(failure) => {
                    report.failed += 1;
                    *report.failures.entry(failure.kind()).or_default() += 1;
                    Detected::default()
                }
            };
            if detected.has_values() {
                report.detected += 1;
            }
            model.detected_context_size = detected.context_size;
            model.detected_capabilities = detected.capabilities;
            model
        })
        .collect();
    (models, report)
}
