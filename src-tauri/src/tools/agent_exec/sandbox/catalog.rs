//! EAC-FR-SQWO: a step that fails because the sandbox runtime cannot read its
//! own catalog is tried once more through a new connection to that catalog.
//! EAC-FR-NDDZ: a catalog guard runs beside the catalog connection.
//!
//! Every process that uses one runtime home shares one catalog. Where the
//! WAL and shared-memory files of that catalog are replaced while this process
//! holds connections to it, every new connection of this process attaches to
//! the shared memory the earlier ones hold, and reads the catalog as damaged
//! until all of them close.

use std::path::PathBuf;

use super::super::runtime::BoxFuture;
use super::{BootError, SandboxLaunch, SandboxPool, BUFFER, DOMAINS};
use crate::log_fields;
use crate::logging::{self, LogSink};

/// EAC-FR-NDDZ: what a request to keep the catalog guard running did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuardReport {
    /// The earlier guard, where one ended.
    pub ended: Option<GuardEnd>,
    pub outcome: GuardOutcome,
}

/// EAC-FR-NDDZ: an earlier guard that ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardEnd {
    pub pid: u32,
    /// `replaced` where a new catalog connection ended it, and `exited` where
    /// it had ended on its own.
    pub cause: &'static str,
    pub exit_code: Option<i32>,
}

/// EAC-FR-NDDZ, EAC-FR-PIHS: the guard after the request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GuardOutcome {
    /// This host needs no guard.
    #[default]
    NotNeeded,
    /// The guard that ran before still runs.
    Running { pid: u32 },
    /// A new guard started and holds its lock on `catalog`.
    Started { pid: u32, catalog: PathBuf },
    /// No guard could start. `stage` names what failed.
    Failed { stage: &'static str, message: String },
}

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
            Ok(()) => {
                // EAC-FR-NDDZ: the step opened a catalog connection.
                self.keep_catalog_guard(sink, launch, image_id, false).await;
                return Ok(());
            }
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
        // EAC-FR-NDDZ: a new connection ends the earlier guard and starts a
        // new one.
        self.keep_catalog_guard(sink, launch, image_id, true).await;
        step().await
    }

    /// EAC-FR-NDDZ: keep a catalog guard running beside the catalog
    /// connection. `renew` ends the earlier guard and starts a new one.
    /// EAC-FR-PIHS: a guard that cannot start is a `WARN`, and the launch goes
    /// on. EAC-FR-FUNB: each start, lock, and end is recorded.
    pub(super) async fn keep_catalog_guard<S>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
        image_id: Option<&str>,
        renew: bool,
    ) where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let report = self.runtime.guard_catalog(renew).await;
        let vm = launch.vm;
        let image = launch.spec.image.as_str();
        if let Some(ended) = &report.ended {
            logging::log_info(
                sink,
                &BUFFER,
                &DOMAINS,
                "sandbox catalog guard ended",
                log_fields! {
                    "vm" => vm,
                    "image" => image,
                    "image_id" => image_id,
                    "pid" => ended.pid,
                    "cause" => ended.cause,
                    "exit_code" => ended.exit_code,
                },
            );
        }
        match report.outcome {
            GuardOutcome::NotNeeded | GuardOutcome::Running { .. } => {}
            GuardOutcome::Started { pid, catalog } => {
                let catalog = catalog.display().to_string();
                for message in
                    ["sandbox catalog guard started", "sandbox catalog guard holds its lock"]
                {
                    logging::log_info(
                        sink,
                        &BUFFER,
                        &DOMAINS,
                        message,
                        log_fields! {
                            "vm" => vm,
                            "image" => image,
                            "image_id" => image_id,
                            "pid" => pid,
                            "catalog" => catalog.as_str(),
                        },
                    );
                }
            }
            GuardOutcome::Failed { stage, message } => {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox catalog guard could not start",
                    log_fields! {
                        "vm" => vm,
                        "image" => image,
                        "image_id" => image_id,
                        "stage" => stage,
                        "message" => message.as_str(),
                    },
                );
            }
        }
    }
}
