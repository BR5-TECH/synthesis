//! EAC-FR-DKRS and EAC-FR-HWQN: what Docker-in-Docker adds to one container.
//!
//! The four additions and no others: the socket mount, the socket's group, and
//! — where the host gateway toggle is on — the host mapping and the Testcontainers
//! variable. The user, the capabilities and the security options stay as
//! EAC-FR-13 and EAC-FR-14 set them. Nothing here reads a value from a task, an
//! instruction or an agent answer.

use super::descriptor::{
    self, BindMount, DOCKER_HOST_GATEWAY_MAPPING, DOCKER_SOCKET_TARGET,
    TESTCONTAINERS_HOST_OVERRIDE_ENV, TESTCONTAINERS_HOST_OVERRIDE_VALUE,
};
use super::{AgentExecutionError, DockerInDockerLaunch};

/// What one launch adds to its container specification.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DockerInDockerAdditions {
    pub mounts: Vec<BindMount>,
    pub groups: Vec<u32>,
    pub extra_hosts: Vec<String>,
    pub env_literals: Vec<(String, String)>,
}

/// EAC-FR-HWQN: checks the socket and reads its group at this launch.
///
/// Disabled settings add nothing, and read no path. An enabled setting whose
/// path is not a Unix socket fails as `DockerSocketUnavailable` before any
/// container exists.
pub(super) fn additions(
    settings: &DockerInDockerLaunch,
) -> Result<DockerInDockerAdditions, AgentExecutionError> {
    if !settings.enabled {
        return Ok(DockerInDockerAdditions::default());
    }
    let unavailable = || AgentExecutionError::DockerSocketUnavailable(settings.socket_path.clone());
    let path = std::path::Path::new(&settings.socket_path);
    let group = crate::fs::probe_unix_socket(path).map_err(|_| unavailable())?;
    let source = descriptor::mount_source(path).ok_or_else(unavailable)?;

    let mut added = DockerInDockerAdditions {
        mounts: vec![BindMount {
            source,
            target: DOCKER_SOCKET_TARGET.to_string(),
            read_only: false,
        }],
        groups: vec![group],
        ..DockerInDockerAdditions::default()
    };
    if settings.host_gateway {
        added.extra_hosts.push(DOCKER_HOST_GATEWAY_MAPPING.to_string());
        added.env_literals.push((
            TESTCONTAINERS_HOST_OVERRIDE_ENV.to_string(),
            TESTCONTAINERS_HOST_OVERRIDE_VALUE.to_string(),
        ));
    }
    Ok(added)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn settings(path: &str, host_gateway: bool) -> DockerInDockerLaunch {
        DockerInDockerLaunch { enabled: true, socket_path: path.to_string(), host_gateway }
    }

    // EAC-FR-DKRS: disabled settings add nothing and read no path.
    #[test]
    fn disabled_settings_add_nothing() {
        let off = DockerInDockerLaunch {
            enabled: false,
            socket_path: "/does/not/exist.sock".to_string(),
            host_gateway: true,
        };
        assert_eq!(additions(&off).unwrap(), DockerInDockerAdditions::default());
    }

    // EAC-FR-DKRS: the socket, its group, and nothing about the host gateway.
    #[test]
    fn a_socket_adds_its_mount_and_its_group_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let path_text = path.to_str().unwrap();

        let added = additions(&settings(path_text, false)).unwrap();
        assert_eq!(
            added.mounts,
            vec![BindMount {
                source: path_text.to_string(),
                target: DOCKER_SOCKET_TARGET.to_string(),
                read_only: false,
            }]
        );
        assert_eq!(added.groups.len(), 1);
        assert!(added.extra_hosts.is_empty());
        assert!(added.env_literals.is_empty());
    }

    // EAC-FR-DKRS: the host gateway adds the mapping and the variable.
    #[test]
    fn the_host_gateway_adds_the_mapping_and_the_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        let added = additions(&settings(path.to_str().unwrap(), true)).unwrap();
        assert_eq!(added.extra_hosts, vec![DOCKER_HOST_GATEWAY_MAPPING.to_string()]);
        assert_eq!(
            added.env_literals,
            vec![("TESTCONTAINERS_HOST_OVERRIDE".to_string(), "host.docker.internal".to_string())]
        );
    }

    // EAC-FR-HWQN: a missing path and a plain file both fail, naming the path.
    #[test]
    fn a_missing_socket_and_a_plain_file_fail_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.sock");
        // A directory is an entry that is not a socket, and needs no file write.
        let plain = dir.path().to_path_buf();
        for path in [missing, plain] {
            let text = path.to_str().unwrap().to_string();
            match additions(&settings(&text, false)) {
                Err(AgentExecutionError::DockerSocketUnavailable(named)) => {
                    assert_eq!(named, text)
                }
                other => panic!("expected DockerSocketUnavailable, got {other:?}"),
            }
        }
    }
}
