//! The Docker daemon in the VM (EAC-FR-QMRW, EAC-FR-ZPTA).
//!
//! Split out of `sandbox.rs` for size alone.

use super::*;

impl SandboxPool {
    /// EAC-FR-QMRW: whether the daemon of a reused VM answers. The start is
    /// asked again, and it ends at once where the daemon answers; a daemon that
    /// stopped is started again. `false` makes the launch reboot the VM.
    pub(super) async fn reused_daemon_answers<S>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
    ) -> bool
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let Some(group) = owner_group(&launch.spec.owner) else {
            return false;
        };
        match self.runtime.start_docker_daemon(launch.vm, group).await {
            Ok(()) => true,
            Err(message) => {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM Docker daemon does not answer, so the VM is rebooted",
                    log_fields! { "vm" => launch.vm, "message" => message.as_str() },
                );
                false
            }
        }
    }

    /// EAC-FR-QMRW: start the Docker daemon in the VM this launch just booted.
    /// `Ok(Some(_))` is a launch cancelled while it waited; the VM is then
    /// stopped. EAC-FR-ZPTA: a daemon that is not available stops the VM and
    /// fails the launch, with no fallback.
    pub(super) async fn start_docker_daemon<S>(
        &self,
        sink: &S,
        launch: &SandboxLaunch<'_>,
        cancel: &CancellationToken,
    ) -> Result<Option<RuntimeOutcome>, SandboxLaunchError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        let Some(group) = owner_group(&launch.spec.owner) else {
            self.runtime.stop(launch.vm).await;
            return Err(self.failed(
                sink,
                launch.vm,
                &launch.spec.image,
                SandboxFailureCause::DockerDaemonUnavailable,
                format!("the owner {} names no group for the daemon socket", launch.spec.owner),
            ));
        };
        logging::log_info(
            sink,
            &BUFFER,
            &DOMAINS,
            "sandbox VM starts its Docker daemon",
            log_fields! { "vm" => launch.vm, "image" => launch.spec.image.as_str() },
        );
        let started = tokio::select! {
            biased;
            () = until_cancelled(cancel) => None,
            started = self.runtime.start_docker_daemon(launch.vm, group) => Some(started),
        };
        match started {
            None => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM stopped, because the run was cancelled as its Docker daemon started",
                    log_fields! { "vm" => launch.vm },
                );
                self.runtime.stop(launch.vm).await;
                Ok(Some(cancelled_outcome()))
            }
            Some(Ok(())) => {
                logging::log_info(
                    sink,
                    &BUFFER,
                    &DOMAINS,
                    "sandbox VM Docker daemon is ready",
                    log_fields! { "vm" => launch.vm },
                );
                Ok(None)
            }
            Some(Err(message)) => {
                self.runtime.stop(launch.vm).await;
                Err(self.failed(
                    sink,
                    launch.vm,
                    &launch.spec.image,
                    SandboxFailureCause::DockerDaemonUnavailable,
                    message,
                ))
            }
        }
    }
}

/// The GID of an owner `<uid>:<gid>` (EAC-FR-KTWO).
fn owner_group(owner: &str) -> Option<u32> {
    owner.split_once(':')?.1.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::owner_group;

    // EAC-FR-QMRW: the socket group is the host user's GID.
    #[test]
    fn the_socket_group_is_the_gid_of_the_owner() {
        assert_eq!(owner_group("501:20"), Some(20));
        assert_eq!(owner_group("1000:1000"), Some(1000));
        assert_eq!(owner_group("1000"), None);
        assert_eq!(owner_group("1000:staff"), None);
    }
}
