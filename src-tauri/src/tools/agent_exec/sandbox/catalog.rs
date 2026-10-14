//! EAC-FR-SQWO: a step that fails because the sandbox runtime cannot read its
//! own catalog is tried once more through a new connection to that catalog.
//!
//! Every process that uses one runtime home shares one catalog. Another
//! process can replace a file of that catalog while this process holds it
//! open, and this process then reads the catalog as damaged until it opens it
//! again.

use super::super::runtime::BoxFuture;
use super::{BootError, SandboxLaunch, SandboxPool, BUFFER, DOMAINS};
use crate::log_fields;
use crate::logging::{self, LogSink};

impl SandboxPool {
    /// EAC-FR-SQWO: run `step`. Where it fails on the catalog, open a new
    /// connection to the catalog and run it once more. A second catalog
    /// failure, or a connection that cannot be opened, is `Catalog`. The
    /// records name the VM, the image, and the image ID where it is known.
    pub(super) async fn with_catalog_retry<'a, S, F>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
        image_id: Option<&str>,
        step: F,
    ) -> Result<(), BootError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
        F: Fn() -> BoxFuture<'a, Result<(), BootError>>,
    {
        let message = match step().await {
            Err(BootError::Catalog(message)) => message,
            other => return other,
        };
        logging::log_warn(
            sink,
            &BUFFER,
            &DOMAINS,
            "sandbox runtime could not read its catalog, opening a new connection",
            log_fields! {
                "vm" => launch.vm,
                "image" => launch.spec.image.as_str(),
                "image_id" => image_id,
                "message" => message.as_str(),
            },
        );
        if let Err(error) = self.runtime.reopen_catalog().await {
            return Err(BootError::Catalog(error));
        }
        // EAC-FR-FUNB: every new connection to the catalog is recorded.
        logging::log_info(
            sink,
            &BUFFER,
            &DOMAINS,
            "sandbox runtime catalog opened again",
            log_fields! {
                "vm" => launch.vm,
                "image" => launch.spec.image.as_str(),
                "image_id" => image_id,
            },
        );
        step().await
    }
}
