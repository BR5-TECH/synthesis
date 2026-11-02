//! Why a VM could not be established (EAC-FR-NCSA, GXD-FR-QUCP).
//!
//! Split out of `sandbox.rs` for size alone.

/// EAC-FR-NCSA: why a VM could not be established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxFailureCause {
    UnsupportedHost,
    RuntimeUnavailable,
    ImageUnavailable,
    ImageExportFailed,
    ImageLoadFailed,
    RuntimeCatalogFailed,
    GatewayUnresolved,
    /// EAC-FR-ZPTA: the Docker daemon of EAC-FR-QMRW is not available.
    DockerDaemonUnavailable,
    InsufficientMemory,
    BootFailed,
}

impl SandboxFailureCause {
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxFailureCause::UnsupportedHost => "unsupported_host",
            SandboxFailureCause::RuntimeUnavailable => "runtime_unavailable",
            SandboxFailureCause::ImageUnavailable => "image_unavailable",
            SandboxFailureCause::ImageExportFailed => "image_export_failed",
            SandboxFailureCause::ImageLoadFailed => "image_load_failed",
            SandboxFailureCause::RuntimeCatalogFailed => "runtime_catalog_failed",
            SandboxFailureCause::GatewayUnresolved => "gateway_unresolved",
            SandboxFailureCause::DockerDaemonUnavailable => "docker_daemon_unavailable",
            SandboxFailureCause::InsufficientMemory => "insufficient_memory",
            SandboxFailureCause::BootFailed => "boot_failed",
        }
    }

    /// GXD-FR-QUCP: the cause in words, for the run's detail.
    pub fn sentence(self) -> &'static str {
        match self {
            SandboxFailureCause::UnsupportedHost => {
                "this machine cannot run Microsandbox, because it has no usable hardware virtualisation"
            }
            SandboxFailureCause::RuntimeUnavailable => {
                "the sandbox runtime could not be installed or started"
            }
            SandboxFailureCause::ImageUnavailable => {
                "the image is in no registry and in no verified Docker backend"
            }
            SandboxFailureCause::ImageExportFailed => {
                "the image could not be exported from Docker"
            }
            SandboxFailureCause::ImageLoadFailed => {
                "the sandbox runtime could not load the image"
            }
            SandboxFailureCause::RuntimeCatalogFailed => {
                "the sandbox runtime could not read its own catalog"
            }
            SandboxFailureCause::GatewayUnresolved => {
                "the host of the Custom Gateway could not be resolved on this machine"
            }
            SandboxFailureCause::DockerDaemonUnavailable => {
                "the Docker daemon in the VM is not available"
            }
            SandboxFailureCause::InsufficientMemory => {
                "there is not enough memory for the configured RAM"
            }
            SandboxFailureCause::BootFailed => "the VM did not boot",
        }
    }
}

/// EAC-FR-NCSA: a VM that could not be established, with the runtime's own
/// message. The message goes to the log, and on one line to the run's detail
/// (GXD-FR-QUCP).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxFailure {
    pub cause: SandboxFailureCause,
    pub message: String,
}
