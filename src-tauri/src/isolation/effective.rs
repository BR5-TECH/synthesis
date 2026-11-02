//! The effective isolation of a graduation launch (EAC-FR-SNMS, EAC-FR-QMRW,
//! GSU-FR-RQDS).
//!
//! The global isolation mode and the project's Docker-in-Docker settings
//! together decide how a launch runs. The launch and the start preflight read
//! the decision from here, so the two can never disagree.

use super::ResolvedIsolation;
use crate::project_settings::UnderMicrosandbox;

/// EAC-FR-SNMS: the isolation a launch uses. It is `None` where the isolation
/// is Microsandbox, Docker-in-Docker is enabled, and the project selects the
/// host socket. In every other case it is the resolved isolation.
pub fn effective_isolation(
    resolved: ResolvedIsolation,
    docker_in_docker: bool,
    under_microsandbox: UnderMicrosandbox,
) -> ResolvedIsolation {
    match resolved {
        ResolvedIsolation::Microsandbox(_)
            if docker_in_docker && under_microsandbox == UnderMicrosandbox::HostSocket =>
        {
            ResolvedIsolation::None
        }
        other => other,
    }
}

/// EAC-FR-QMRW: whether a sandbox launch runs a Docker daemon in its VM. Only
/// meaningful for a launch whose effective isolation is Microsandbox.
pub fn runs_docker_daemon(docker_in_docker: bool, under_microsandbox: UnderMicrosandbox) -> bool {
    docker_in_docker && under_microsandbox == UnderMicrosandbox::VmDaemon
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isolation::SandboxSettings;

    fn sandbox() -> ResolvedIsolation {
        ResolvedIsolation::Microsandbox(SandboxSettings {
            cpus: 2,
            memory_gib: 4,
            disk_gib: 16,
            block_local_network: true,
            allowed: Vec::new(),
            unsupported_reason: None,
        })
    }

    // EAC-FR-SNMS, PSS-FR-DIND: the effective isolation for each combination
    // of the mode, the checkbox, and the choice.
    #[test]
    fn each_combination_resolves_to_its_effective_isolation() {
        use UnderMicrosandbox::{HostSocket, VmDaemon};
        let cases = [
            (ResolvedIsolation::None, false, VmDaemon, ResolvedIsolation::None),
            (ResolvedIsolation::None, false, HostSocket, ResolvedIsolation::None),
            (ResolvedIsolation::None, true, VmDaemon, ResolvedIsolation::None),
            (ResolvedIsolation::None, true, HostSocket, ResolvedIsolation::None),
            (sandbox(), false, VmDaemon, sandbox()),
            (sandbox(), false, HostSocket, sandbox()),
            (sandbox(), true, VmDaemon, sandbox()),
            (sandbox(), true, HostSocket, ResolvedIsolation::None),
        ];
        for (resolved, enabled, choice, expected) in cases {
            let label = format!("{resolved:?} enabled={enabled} {choice:?}");
            assert_eq!(effective_isolation(resolved, enabled, choice), expected, "{label}");
        }
    }

    // EAC-FR-QMRW: the VM runs a daemon only with the checkbox on and the
    // daemon choice.
    #[test]
    fn only_the_enabled_daemon_choice_runs_a_daemon() {
        assert!(runs_docker_daemon(true, UnderMicrosandbox::VmDaemon));
        assert!(!runs_docker_daemon(true, UnderMicrosandbox::HostSocket));
        assert!(!runs_docker_daemon(false, UnderMicrosandbox::VmDaemon));
        assert!(!runs_docker_daemon(false, UnderMicrosandbox::HostSocket));
    }
}
