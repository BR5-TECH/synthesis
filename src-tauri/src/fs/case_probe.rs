//! Whether the filesystem under test folds case — the one question a good many
//! tests in this crate turn on. Compiled only under `cfg(test)`.
//!
//! It lives under `fs/` because that is where a question about the filesystem
//! belongs, and because the sweep in `access_tests` leaves this directory to
//! `FsAccess` — the probe below touches the disk directly, which is exactly what
//! that sweep forbids everywhere else.

use std::path::Path;

/// Whether the filesystem holding `dir` folds case: `UI` and `ui` are one
/// directory on APFS and NTFS and two on ext4.
///
/// **Probed, not inferred from `cfg!(target_os)`.** The answer is a property of
/// the mounted filesystem rather than of the platform — macOS mounts
/// case-sensitive volumes and Linux can mount folding ones — so a test that
/// guesses from the OS guesses wrong on exactly the machine that would have
/// caught the bug.
///
/// A test that turns on this answer must assert in **both** directions rather
/// than running its body on one filesystem and silently doing nothing on the
/// other. A test that quietly no-ops reports the same green as one that checked
/// something, so the guarantee it covers stops being covered on that platform
/// with nothing to say so. Where a branch genuinely cannot be reached on one
/// filesystem, name that in the test and assert the behaviour that filesystem
/// *does* have.
/// The probe builds its pair inside a directory of its own rather than directly
/// in `dir`, because `dir` is in practice the project root the caller is about
/// to make assertions over — and a stray entry appearing in someone's tree
/// listing for the lifetime of two syscalls is a confusing way to learn this
/// function exists. Inside its own directory it is invisible to any walk that
/// has not already descended into it, and it is removed whole.
pub fn folds_case(dir: &Path) -> bool {
    let probe = dir.join(".synthesis-case-probe");
    std::fs::create_dir(&probe).expect("the case probe directory is creatable");
    std::fs::create_dir(probe.join("Probe")).expect("the case probe is creatable");
    let folded = probe.join("probe").is_dir();
    std::fs::remove_dir_all(&probe).expect("the case probe is removable");
    folded
}

#[cfg(test)]
mod tests {
    use super::folds_case;
    use tempfile::TempDir;

    #[test]
    fn the_probe_leaves_nothing_behind_and_answers_the_same_way_twice() {
        // The helper is itself a filesystem operation, and one every caller
        // runs inside the tree it is about to make assertions over. Litter from
        // it would show up as a stray entry in someone else's directory
        // listing, which is a confusing way to learn about this function.
        let dir = TempDir::new().unwrap();

        let first = folds_case(dir.path());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "the probe directory was removed"
        );
        assert_eq!(first, folds_case(dir.path()), "and the answer is stable");
    }
}
