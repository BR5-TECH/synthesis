//! The Docker invocation one launch is described by (EAC-FR-10, EAC-FR-13,
//! EAC-FR-39), and the names and paths it is composed from.
//!
//! Split out of `descriptor.rs` for size alone. Every item is re-exported there,
//! so a caller names it as `descriptor::<item>`.

use std::path::{Path, PathBuf};

use super::SECURITY_OPTS;

/// What one container launch needs, as a mount the invocation will express.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindMount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

impl BindMount {
    fn as_argument(&self) -> String {
        let mut value = format!("type=bind,source={},target={}", self.source, self.target);
        if self.read_only {
            value.push_str(",readonly");
        }
        value
    }
}

/// EAC-FR-10 / EAC-FR-39: one container launch, described whole.
///
/// Every field is a named constant, a field of the vendor's descriptor, or the
/// one resolved value that is not — the image, which is the project's own
/// (EAC-FR-38). It exists because two backends have to create the *same*
/// container: the Docker CLI backend renders it into the `docker run` vector
/// below, and the Docker Engine backend creates and starts it over Bollard's
/// connection. Neither composes a parameter of its own, so which backend a
/// machine uses changes how Docker is reached and nothing about what the agent
/// runs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub host_uid: u32,
    pub host_gid: u32,
    /// EAC-FR-ZKMR: the directory the turn stands in — the execution
    /// directory's own host path, or [`WORKSPACE_TARGET`] in the fallback shape.
    pub workdir: String,
    pub mounts: Vec<BindMount>,
    /// EAC-FR-DKRS: supplementary groups. Empty unless Docker-in-Docker is
    /// enabled, when it holds the group of the Docker socket.
    pub groups: Vec<u32>,
    /// EAC-FR-DKRS: host mappings. Empty unless Docker-in-Docker is enabled and
    /// the host gateway toggle is on.
    pub extra_hosts: Vec<String>,
    /// Variables the runtime forwards **by name**, taking their values from the
    /// client's own environment, so a secret never reaches an argument vector
    /// (EAC-FR-15).
    pub env_names: Vec<String>,
    /// Variables carried with their values, every one of them a container path
    /// or a host name this application chose rather than a secret.
    pub env_literals: Vec<(String, String)>,
    pub vendor_args: Vec<String>,
    /// EAC-FR-GKSW: the program that replaces the image entrypoint. `None` for
    /// every agent launch, which runs the entrypoint of the image.
    pub entrypoint: Option<String>,
    /// EAC-FR-13: [`NETWORK`] for every agent launch, and [`HELPER_NETWORK`]
    /// for the helper of EAC-FR-GKSW.
    pub network: &'static str,
}

impl ContainerSpec {
    /// The Docker CLI backend's rendering of this specification: the complete
    /// `docker run` argument vector (EAC-FR-13).
    ///
    /// The `env_names` field names variables whose *values* the runtime takes
    /// from the docker client's own environment. That indirection is the whole
    /// reason Claude's token never reaches an argument vector (EAC-FR-15):
    /// `--env NAME` is a name, and a name is not a secret.
    ///
    /// The `env_literals` field carries its values in the argv instead, which is
    /// correct only because every one of them is a container path this
    /// application chose — a mount target, never a secret. The two mechanisms are separate so that
    /// "forwarded by name" and "written in the vector" are decisions a reader makes
    /// per variable rather than one this method makes for all of them.
    pub fn docker_run_args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "run".into(),
            // EAC-FR-26: removed when the agent exits, on every path.
            "--rm".into(),
            // EAC-FR-09: the task reaches the agent on the container's stdin and
            // nowhere else, and `docker run` connects the client's stdin to the
            // container only when asked. Without this the payload is written to a
            // stream nothing reads, and both pinned CLIs — each of which takes its
            // whole prompt from stdin — exit non-zero on an empty one. Not `--tty`:
            // a terminal would let the CLI print progress decoration into the very
            // stream EAC-FR-19 parses for an envelope.
            "--interactive".into(),
            "--name".into(),
            self.name.clone(),
            "--network".into(),
            self.network.into(),
            // EAC-FR-14: the container's non-root user is mapped to the host user,
            // so a file the agent creates in the mount belongs to whoever launched
            // it rather than to a uid that exists only inside the container.
            "--user".into(),
            self.user(),
            "--workdir".into(),
            self.workdir.clone(),
        ];

        for option in SECURITY_OPTS {
            args.push((*option).to_string());
        }
        for mount in &self.mounts {
            args.push("--mount".into());
            args.push(mount.as_argument());
        }
        // EAC-FR-DKRS: the socket's group, so the non-root user can use it.
        for group in &self.groups {
            args.push("--group-add".into());
            args.push(group.to_string());
        }
        for host in &self.extra_hosts {
            args.push("--add-host".into());
            args.push(host.clone());
        }
        for name in &self.env_names {
            args.push("--env".into());
            args.push(name.clone());
        }
        for (name, value) in &self.env_literals {
            args.push("--env".into());
            args.push(format!("{name}={value}"));
        }

        if let Some(entrypoint) = &self.entrypoint {
            args.push("--entrypoint".into());
            args.push(entrypoint.clone());
        }
        args.push(self.image.clone());
        args.extend(self.vendor_args.iter().cloned());
        args
    }

    /// `--user` as the Engine API expects it, which is the same string the CLI
    /// vector carries.
    pub fn user(&self) -> String {
        format!("{}:{}", self.host_uid, self.host_gid)
    }

    /// The security options of [`SECURITY_OPTS`], as the Engine API's own
    /// `HostConfig` spells them: `--cap-drop ALL` is a dropped capability list
    /// and `--security-opt no-new-privileges` is a security option, where the
    /// CLI takes both as flags.
    pub fn dropped_capabilities(&self) -> Vec<String> {
        vec!["ALL".to_string()]
    }

    pub fn security_options(&self) -> Vec<String> {
        vec!["no-new-privileges".to_string()]
    }

    /// `--group-add` as the Engine API's `GroupAdd` spells it.
    pub fn group_add(&self) -> Vec<String> {
        self.groups.iter().map(u32::to_string).collect()
    }
}

/// A container name no other request has used or will reuse (EAC-FR-13).
///
/// Process id, a monotonic counter, and the nanoseconds since the epoch
/// together: the counter separates two launches in one process, the pid
/// separates two processes, and the timestamp separates a relaunch that
/// happened to be given a recycled pid.
pub fn unique_container_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("synthesis-agent-{}-{}-{}", std::process::id(), nanos, sequence)
}

/// A session identifier for a vendor that lets the executor assign one
/// (EAC-FR-21, CCP-FR-16).
///
/// Shaped as a v4 UUID because the pinned CLI that accepts one requires that
/// shape. The bytes are derived from a SHA-256 over the process id, the
/// nanosecond clock, and a monotonic counter rather than from a random source:
/// what this value has to be is *unique*, not unguessable. It names a
/// conversation inside a container this application launched, it is not a
/// credential, and nothing authenticates with it — so a counter that cannot
/// repeat is exactly the property worth buying, and it is bought without adding
/// a dependency.
pub fn new_session_id() -> String {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());

    let mut hasher = Sha256::new();
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(nanos.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Version 4 and the RFC 4122 variant, so the value parses as the UUID the
    // CLI asks for rather than merely looking like one.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// The host directory a vendor's session state lives in (EAC-FR-31).
///
/// Per vendor and per project: two projects driving the same vendor keep
/// separate transcripts, and one vendor's state is never visible to the other.
/// The project key is hashed rather than used as a path component — it is an
/// absolute path, so it cannot be one, and hashing also keeps a project's
/// location off the filesystem under `app_data_dir()`.
pub fn session_state_dir(root: &Path, vendor: &str, project_key: &str) -> PathBuf {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(project_key.as_bytes());
    let digest = hasher.finalize();
    let slug: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();

    root.join(vendor).join(slug)
}

/// EAC-FR-FNFV: a short, stable name for a path, which is what distinguishes the
/// generated files one execution's repository access is made of.
///
/// Those files are a function of the execution directory alone: two launches
/// against one directory write the same bytes, and two against different ones
/// write different files. Without the distinction they share one path, and one
/// launch replaces the other's between the write and the moment the runtime
/// resolves the mount — leaving a turn standing on a `.git` that names some
/// other working copy.
pub fn path_slug(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.finalize()[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// The host user the container runs as (EAC-FR-14).
#[cfg(unix)]
pub fn host_ids() -> (u32, u32) {
    // SAFETY: `getuid`/`getgid` cannot fail and touch no memory the caller owns.
    unsafe { (libc_getuid(), libc_getgid()) }
}

#[cfg(unix)]
extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
    #[link_name = "getgid"]
    fn libc_getgid() -> u32;
}

/// Windows has no uid/gid to map, and Docker Desktop handles ownership in the
/// mount layer, so the manifest's own ids are used unchanged.
#[cfg(not(unix))]
pub fn host_ids() -> (u32, u32) {
    (0, 0)
}

/// Whether a path is usable as a bind-mount source, as a string.
///
/// Docker's `--mount` value is comma-separated, so a source containing a comma
/// would be read as a second option. Rejecting it here turns a confusing
/// runtime failure into a typed refusal.
pub fn mount_source(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.contains(',') || text.is_empty() {
        return None;
    }
    Some(text.to_string())
}
