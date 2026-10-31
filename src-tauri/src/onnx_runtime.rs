//! The one ONNX Runtime environment of the process.
//!
//! `ort` allows one environment per process, for the life of the process, and it
//! refuses a second one. The embedding models and the voice detector
//! (`VOP-voice-ai-providers.md` VOP-FR-SMSX) both need it, so both take it from
//! here. A session that built its own would stop the other for the whole run.

use std::sync::Mutex;

use ort::environment::Environment;

static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

pub(crate) fn shared_environment() -> Result<Environment, String> {
    let mut held = ENVIRONMENT.lock().map_err(|e| format!("onnx environment poisoned: {e}"))?;
    if let Some(environment) = held.as_ref() {
        return Ok(environment.clone());
    }
    let environment = ort::init().build().map_err(|e| e.to_string())?;
    *held = Some(environment.clone());
    Ok(environment)
}

#[cfg(test)]
mod tests {
    use super::shared_environment;

    // VOP-FR-SMSX: a second caller gets the same environment, not a refusal.
    #[test]
    fn a_second_caller_gets_the_environment_and_no_refusal() {
        assert!(shared_environment().is_ok());
        assert!(shared_environment().is_ok());
    }
}
