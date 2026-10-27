//! EAC-FR-QULL: the verified Docker backend as the sandbox export fallback
//! reads it — the image ID of a reference, and its `docker save` archive.
//!
//! A child of `runtime` so that both backends reach the program resolution and
//! the connection they already own.

use crate::embeddings::InheritedEnvironment;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use super::super::sandbox::ImageExporter;
use super::{BollardDocker, BoxFuture, HostDockerCli};
use crate::fs::FsAccess;

/// How long an export may take. An image of several gigabytes is written in
/// minutes, and a bound is still kept so a daemon that stops answering cannot
/// hold a launch for ever.
pub const EXPORT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How long the image ID query may take.
const ID_TIMEOUT: Duration = Duration::from_secs(30);

impl ImageExporter for HostDockerCli {
    fn image_id<'a>(&'a self, reference: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let run = tokio::process::Command::new(self.program().await)
                .with_inherited_hub_environment()
                .args(["image", "inspect", "--format", "{{.Id}}", reference])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output();
            match tokio::time::timeout(ID_TIMEOUT, run).await {
                Ok(Ok(output)) if output.status.success() => {
                    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if id.is_empty() {
                        Err("Docker reported no image ID".to_string())
                    } else {
                        Ok(id)
                    }
                }
                Ok(Ok(_)) => Err("the image is not in the Docker backend".to_string()),
                Ok(Err(error)) => Err(error.kind().to_string()),
                Err(_) => Err("the Docker backend did not answer".to_string()),
            }
        })
    }

    fn export<'a>(
        &'a self,
        reference: &'a str,
        destination: &'a Path,
        _fs: &'a FsAccess,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            // The Docker CLI writes the archive itself. The cache that asked
            // for it moves and deletes it through `FsAccess`.
            let run = tokio::process::Command::new(self.program().await)
                .with_inherited_hub_environment()
                .arg("save")
                .arg("--output")
                .arg(destination)
                .arg(reference)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .output();
            match tokio::time::timeout(EXPORT_TIMEOUT, run).await {
                Ok(Ok(output)) if output.status.success() => Ok(()),
                Ok(Ok(output)) => {
                    let message = String::from_utf8_lossy(&output.stderr);
                    let line = message.lines().next().unwrap_or("docker save failed");
                    Err(line.chars().take(300).collect())
                }
                Ok(Err(error)) => Err(error.kind().to_string()),
                Err(_) => Err("the image export did not finish in time".to_string()),
            }
        })
    }
}

#[cfg(not(test))]
impl ImageExporter for BollardDocker {
    fn image_id<'a>(&'a self, reference: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let docker = self.connect().map_err(|_| "the Docker backend did not answer")?;
            match tokio::time::timeout(ID_TIMEOUT, docker.inspect_image(reference)).await {
                Ok(Ok(image)) => image
                    .id
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| "Docker reported no image ID".to_string()),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("the Docker backend did not answer".to_string()),
            }
        })
    }

    fn export<'a>(
        &'a self,
        reference: &'a str,
        destination: &'a Path,
        fs: &'a FsAccess,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            use futures_util::StreamExt;

            let docker = crate::docker::connect(&self.endpoint, EXPORT_TIMEOUT)
                .map_err(|_| "the Docker backend did not answer".to_string())?;
            let write = async {
                let mut file = fs
                    .open_file(destination, crate::fs::OpenMode::Truncate)
                    .map_err(|e| e.to_string())?;
                let mut stream = docker.export_image(reference);
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| e.to_string())?;
                    file.write_all(&chunk).map_err(|e| e.to_string())?;
                }
                file.sync().map_err(|e| e.to_string())
            };
            match tokio::time::timeout(EXPORT_TIMEOUT, write).await {
                Ok(result) => result,
                Err(_) => Err("the image export did not finish in time".to_string()),
            }
        })
    }
}

/// Under `cfg(test)` this backend reaches nothing, as its runtime does not.
#[cfg(test)]
impl ImageExporter for BollardDocker {
    fn image_id<'a>(&'a self, _reference: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move { Err("no Docker backend under test".to_string()) })
    }

    fn export<'a>(
        &'a self,
        _reference: &'a str,
        _destination: &'a Path,
        _fs: &'a FsAccess,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { Err("no Docker backend under test".to_string()) })
    }
}
