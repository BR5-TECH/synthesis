//! The production pool (EAC-FR-HMBJ, EAC-FR-ABHG).

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::*;

/// The one pool a shipped build launches through, created on first use.
pub fn production_pool() -> &'static SandboxPool {
    static POOL: OnceLock<SandboxPool> = OnceLock::new();
    POOL.get_or_init(|| SandboxPool::new(production_runtime()))
}

/// Linux and macOS: the embedded microsandbox runtime.
#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
fn production_runtime() -> Arc<dyn SandboxRuntime> {
    Arc::new(micro::MicrosandboxRuntime::default())
}

/// Every other build, and every test build: no VM can be booted, so every
/// sandbox launch reports `unsupported_host` (EAC-FR-HMBJ). A test binds its own
/// double through [`super::AgentCliExecutor::with_sandbox_pool`].
#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
fn production_runtime() -> Arc<dyn SandboxRuntime> {
    Arc::new(NoSandbox)
}

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
struct NoSandbox;

#[cfg(any(test, not(any(target_os = "linux", target_os = "macos"))))]
impl SandboxRuntime for NoSandbox {
    fn boot<'a>(
        &'a self,
        _vm: &'a str,
        _spec: &'a SandboxSpec,
        _image: &'a BootImage,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            Err(BootError::Failed(
                SandboxFailureCause::UnsupportedHost,
                "this build holds no microsandbox runtime".to_string(),
            ))
        })
    }

    fn load_export<'a>(
        &'a self,
        _archive: &'a Path,
        _reference: &'a str,
    ) -> BoxFuture<'a, Result<(), BootError>> {
        Box::pin(async move {
            Err(BootError::Failed(
                SandboxFailureCause::UnsupportedHost,
                "this build holds no microsandbox runtime".to_string(),
            ))
        })
    }

    fn reopen_catalog(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move { Err("this build holds no microsandbox runtime".to_string()) })
    }

    fn run<'a>(
        &'a self,
        _vm: &'a str,
        _process: SandboxProcess<'a>,
    ) -> BoxFuture<'a, Result<RuntimeOutcome, RuntimeError>> {
        Box::pin(async move { Err(RuntimeError::Unavailable) })
    }

    fn refresh<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Err("this build holds no microsandbox runtime".to_string()) })
    }

    fn stop<'a>(&'a self, _vm: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {})
    }

    fn start_docker_daemon<'a>(
        &'a self,
        _vm: &'a str,
        _group: u32,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Err("this build holds no microsandbox runtime".to_string()) })
    }
}
