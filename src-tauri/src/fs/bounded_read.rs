//! The bounded read of one regular file (FSA-FR-NRQE).
//!
//! The caller of this read cannot trust what stands at a path: a FIFO or a
//! device can block a read for ever, and a very large file can fill the memory.
//! So the file is opened without following a link and without blocking, the
//! opened handle is checked, and the read is capped.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use super::{FsError, FsResult};

/// How much is read at a time. The read never holds more than the limit plus
/// one chunk.
const CHUNK: usize = 64 * 1024;

#[cfg(unix)]
fn open_regular(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    // `O_NONBLOCK` makes the open of a FIFO return at once. `O_NOFOLLOW` makes
    // the open of a link fail, so a path swapped for a link after a check
    // reaches nothing.
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_regular(path: &Path) -> io::Result<fs::File> {
    // No open flag stops a link here, so the link check is a metadata check that
    // comes first.
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "symbolic link"));
    }
    fs::File::open(path)
}

/// Read the regular file at `path`, which the caller has already validated.
pub(super) fn read_regular_at(path: &Path, max_bytes: u64) -> FsResult<Vec<u8>> {
    let mut file = match open_regular(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(FsError::NotFound {
                path: path.to_path_buf(),
            })
        }
        // `ELOOP` is what `O_NOFOLLOW` answers for a link.
        Err(e) if is_link_refusal(&e) => {
            return Err(FsError::SymlinkRefused {
                path: path.to_path_buf(),
            })
        }
        Err(e) => return Err(FsError::Io(e)),
    };
    let meta = file.metadata().map_err(FsError::Io)?;
    if !meta.is_file() {
        return Err(FsError::NotRegular {
            path: path.to_path_buf(),
        });
    }
    if meta.len() > max_bytes {
        return Err(FsError::TooLarge {
            path: path.to_path_buf(),
            limit: max_bytes,
        });
    }
    // The size above can be out of date, so the read itself is capped too.
    let mut out = Vec::with_capacity(meta.len() as usize);
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let read = match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FsError::Io(e)),
        };
        if out.len() as u64 + read as u64 > max_bytes {
            return Err(FsError::TooLarge {
                path: path.to_path_buf(),
                limit: max_bytes,
            });
        }
        out.extend_from_slice(&chunk[..read]);
    }
    Ok(out)
}

#[cfg(unix)]
fn is_link_refusal(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
fn is_link_refusal(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::InvalidInput
}
