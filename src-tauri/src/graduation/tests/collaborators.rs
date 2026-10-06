//! The collaborators the image preflight reads through (GSU-FR-HIPF).

use std::path::Path;

/// Every binary the registry names is there and runnable. The preflight reads
/// this rather than the machine, so a fixture that says yes is what makes a
/// configured vendor resolve.
pub(super) struct EverythingIsThere;

impl crate::agentic::FileProbe for EverythingIsThere {
    fn exists(&self, _path: &Path) -> bool {
        true
    }
    fn is_executable(&self, _path: &Path) -> bool {
        true
    }
    fn dir_exists(&self, _path: &Path) -> bool {
        true
    }
}

/// Nothing here runs a child process: the preflight probes nothing, and a test
/// that reached this would be launching something.
pub(super) struct NoCli;

impl crate::agentic::CliRunner for NoCli {
    fn run(
        &self,
        _path: &Path,
        _args: &[&str],
        _timeout: std::time::Duration,
    ) -> Result<crate::agentic::CliOutput, crate::agentic::RunError> {
        panic!("no turn of these tests runs a binary")
    }
}

/// The same for the network.
pub(super) struct NoEndpoint;

impl crate::ai_shared::EndpointProber for NoEndpoint {
    fn probe(
        &self,
        _request: &crate::ai_shared::ProbeRequest,
    ) -> Result<Vec<crate::ai_shared::ModelOption>, crate::ai_shared::ProbeError> {
        panic!("no turn of these tests contacts an endpoint")
    }
}

/// A vault that holds a credential for every vendor, and hands none of it to
/// anything: the preflight asks whether one is present and never what it is.
pub(super) struct EveryKeyIsThere;

impl crate::ai_shared::SecretStore for EveryKeyIsThere {
    fn set(&self, _id: &str, _secret: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
    fn get(&self, _id: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        Ok(Some("a credential this test never reads".into()))
    }
    fn delete(&self, _id: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        Ok(())
    }
}
