//! FSA-FR-19: `probe_unix_socket`, the one question about a host path that
//! stands outside every root an `FsAccess` instance holds.
//!
//! The Docker socket of Docker-in-Docker (PSS-FR-DIND) lies at a host path the
//! author names, such as `/var/run/docker.sock`. No instance may reach it, and
//! this function asks the operating system about the entry and nothing else. It
//! reads no content, opens no connection and mutates nothing.

use std::path::Path;

/// Why a path is not a usable Unix socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketProbeError {
    /// Nothing exists at the path, or the path is not absolute.
    Missing,
    /// An entry exists, and it is not a Unix socket.
    NotASocket,
    /// The platform has no Unix sockets.
    UnsupportedPlatform,
}

/// The numeric group of the Unix socket at `path`, following symbolic links.
#[cfg(unix)]
pub fn probe_unix_socket(path: &Path) -> Result<u32, SocketProbeError> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    if !path.is_absolute() {
        return Err(SocketProbeError::Missing);
    }
    // `metadata` follows links, so a link to the socket is the socket.
    let meta = std::fs::metadata(path).map_err(|_| SocketProbeError::Missing)?;
    if !meta.file_type().is_socket() {
        return Err(SocketProbeError::NotASocket);
    }
    Ok(meta.gid())
}

/// A platform without Unix sockets has no socket to find.
#[cfg(not(unix))]
pub fn probe_unix_socket(_path: &Path) -> Result<u32, SocketProbeError> {
    Err(SocketProbeError::UnsupportedPlatform)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    // FSA-FR-19: a socket answers with its group, and nothing else does.
    #[test]
    fn a_unix_socket_answers_with_its_group() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(probe_unix_socket(&path).is_ok());
    }

    // FSA-FR-19: a link to a socket is the socket.
    #[test]
    fn a_link_to_a_socket_is_followed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let link = dir.path().join("link.sock");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(probe_unix_socket(&link).is_ok());
    }

    // FSA-FR-19: the three typed answers.
    #[test]
    fn a_missing_path_a_file_and_a_relative_path_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            probe_unix_socket(&dir.path().join("none.sock")),
            Err(SocketProbeError::Missing)
        );
        let file = dir.path().join("plain");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(probe_unix_socket(&file), Err(SocketProbeError::NotASocket));
        assert_eq!(probe_unix_socket(dir.path()), Err(SocketProbeError::NotASocket));
        assert_eq!(probe_unix_socket(Path::new("docker.sock")), Err(SocketProbeError::Missing));
    }
}
