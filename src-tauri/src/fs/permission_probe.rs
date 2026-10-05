//! Whether this process is actually bound by filesystem permission bits.
//! Compiled only under `cfg(test)`.
//!
//! It lives under `fs/` for the same reason `case_probe` does: a question about
//! the filesystem belongs there, and the sweep in `access_tests` leaves this
//! directory to `FsAccess` — the probes below touch the disk directly, which is
//! what that sweep forbids everywhere else.
//!
//! Several tests in this crate provoke a failure the only way a test reasonably
//! can: they `chmod` a file or a directory and then assert that the operation
//! under test refused, rolled back, and reported the typed error. That injection
//! rests on an assumption the suite cannot make silently — that the mode bits
//! bind the process running it. Root does not honour them, and neither does
//! anything else holding the relevant capability. Under such a process the
//! chmod is inert, the operation succeeds, and the test fails on an assertion
//! about rollback that was never reached, reporting a defect in code that is
//! behaving correctly.
//!
//! **Two questions, not one.** Linux splits the override across separate
//! capabilities: `CAP_DAC_OVERRIDE` covers read and write, `CAP_DAC_READ_SEARCH`
//! covers read and search alone. A process can hold the second without the
//! first — backup and scanner profiles are granted exactly that — so a single
//! write probe would answer "the bits bind" and send a *read* test into an
//! injection that its process silently overrides. That is the precise failure
//! this module exists to prevent, one capability over, so the two are probed
//! and cached apart.
//!
//! **Deviation from `case_probe`'s doctrine, stated deliberately.** That module
//! requires a test turning on its answer to branch and assert in both
//! directions rather than no-op on one. The callers here return instead, because
//! the behaviour under an overriding process is not a different outcome to
//! assert — it is the absence of the stimulus, so there is nothing to assert
//! about. The cost is real and is not hidden: under such a process those cases
//! pass without having been exercised.
//!
//! TODO: preserving the coverage needs a failure seam that does not go through
//! the kernel's permission check at all — a `#[cfg(test)]` injection predicate
//! on `FsAccess`, consulted in `write_bytes_at`, would fail these writes for any
//! euid and could target a single call rather than a whole directory. That is a
//! change to a production type and has not been made here. Until it is, an
//! unprivileged runner is the only environment that covers these eight cases,
//! and [`REQUIRE_ENFORCEMENT`] is how a suite refuses to run without one.

use std::path::Path;
use std::sync::OnceLock;

/// Whether a mode bit that forbids **writing** binds this process.
///
/// Probed rather than derived from the user. `std::env::var("USER") == "root"`
/// — which is what this replaced — is unset in a container shell, so the very
/// environment most likely to be root is the one it fails to recognise.
/// `geteuid() == 0` would be better but still misses a capability-bearing
/// non-root user and a filesystem mounted without permission enforcement.
/// Attempting the forbidden write answers the only question the callers have.
///
/// Probed once against `TMPDIR`, which is where every caller's fixture is built
/// (they all go through `tempfile`), so the answer describes the mount their
/// injection will actually run on. Cached because it cannot change within a
/// process and the callers run concurrently.
pub fn write_bits_bind() -> bool {
    static BINDS: OnceLock<bool> = OnceLock::new();
    *BINDS.get_or_init(|| in_scratch(probe_write))
}

/// Whether a mode bit that forbids **reading** binds this process.
///
/// Separate from [`write_bits_bind`] because the capabilities are separate; see
/// the module docs.
pub fn read_bits_bind() -> bool {
    static BINDS: OnceLock<bool> = OnceLock::new();
    *BINDS.get_or_init(|| in_scratch(probe_read))
}

/// What a caller's injection forbids, for the message it prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Injection {
    /// A read-only file or directory, made to fail a write, a create, a rename
    /// or a removal.
    Write,
    /// An unreadable file, made to fail a read.
    Read,
}

/// Announce a skip and report whether the caller should take it.
///
/// Callers use it as `if skip_without_enforcement(name, Injection::Write) {
/// return; }`.
///
/// libtest captures the output of a passing test, so the announcement reaches a
/// reader only under `--nocapture` — it is a breadcrumb for whoever is asking
/// why a case did not run, not a banner on the default CI log. There is no
/// stable way for a test to report itself skipped to the harness, so the honest
/// summary of a root run is: these cases pass without having been exercised.
/// A suite that cannot accept that sets [`REQUIRE_ENFORCEMENT`].
pub fn skip_without_enforcement(test: &str, injection: Injection) -> bool {
    let binds = match injection {
        Injection::Write => write_bits_bind(),
        Injection::Read => read_bits_bind(),
    };
    if binds {
        return false;
    }
    // The guard against quiet rot: a runner that is containerised as root later
    // loses these eight cases permanently, and the only trace is a breadcrumb
    // nobody reads. Setting `SYNTHESIS_REQUIRE_PERMISSION_ENFORCEMENT` turns the
    // skip into a failure that says so. Nothing sets it today, so this changes
    // no run — it exists so that a suite which must not lose the coverage can
    // say so in one place rather than in eight.
    assert!(
        std::env::var_os(REQUIRE_ENFORCEMENT).is_none(),
        "{}",
        skip_message(test, injection),
    );
    eprintln!("{}", skip_message(test, injection));
    true
}

/// Set it to refuse the skip rather than take it.
pub const REQUIRE_ENFORCEMENT: &str = "SYNTHESIS_REQUIRE_PERMISSION_ENFORCEMENT";

/// Extracted so the wording is reachable from a test. On an unprivileged
/// machine the branch that prints it never runs, so without this the message —
/// and the `test` it interpolates — could be wrong indefinitely.
fn skip_message(test: &str, injection: Injection) -> String {
    let what = match injection {
        Injection::Write => "a chmod that forbids writing",
        Injection::Read => "a chmod that forbids reading",
    };
    format!(
        "SKIPPED {test}: this process overrides filesystem permission bits \
         (running as root?), so {what} cannot provoke the failure this test \
         injects. Run the suite as an unprivileged user to cover it."
    )
}

/// Run a probe in a scratch directory of its own.
///
/// Fails **open** — claiming the bits bind — when no scratch directory can be
/// made. A caller then runs its assertions rather than skipping them on a
/// guess, and a spurious failure is a better outcome than coverage lost with
/// nothing to say so. A machine that cannot create a temp directory will fail
/// the caller's own fixture a line later regardless.
fn in_scratch(probe: fn(&Path) -> bool) -> bool {
    match tempfile::TempDir::new() {
        Ok(dir) => probe(dir.path()),
        Err(_) => true,
    }
}

/// Whether a directory this process owns, with every write bit cleared, refuses
/// it a new entry.
///
/// A directory rather than a file because that is the shape most callers use:
/// they lock the parent so both halves of an atomic write — the staged temp
/// file and the rename onto the target — are refused together.
#[cfg(unix)]
fn probe_write(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let locked = dir.join("locked");
    if std::fs::create_dir(&locked).is_err() {
        return true;
    }
    if std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).is_err() {
        return true;
    }
    let binds = std::fs::write(locked.join("probe"), b"x").is_err();
    // Restored and removed whole, so a caller's `TempDir` — and the self-tests
    // below, which assert on it — find nothing left behind.
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
    let _ = std::fs::remove_dir_all(&locked);
    binds
}

/// Whether a file this process owns, with every bit cleared, refuses it a read.
#[cfg(unix)]
fn probe_read(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let secret = dir.join("secret");
    if std::fs::write(&secret, b"secret").is_err() {
        return true;
    }
    if std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).is_err() {
        return true;
    }
    let binds = std::fs::read(&secret).is_err();
    let _ = std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644));
    let _ = std::fs::remove_file(&secret);
    binds
}

/// Windows has no mode bits these probes can ask about.
///
/// Answers that they bind, which is the same fail-open bias as [`in_scratch`]:
/// three callers (`comments`, and both in `draft_proposals`) inject with
/// `Permissions::set_readonly`, which compiles on Windows and carries no
/// `#[cfg(unix)]`, so they do reach here. `set_readonly(true)` on a *directory*
/// is a no-op on Windows, so two of them likely fail there already — answering
/// `false` would bury that behind a silent skip rather than leave it visible.
#[cfg(not(unix))]
fn probe_write(_dir: &Path) -> bool {
    true
}

#[cfg(not(unix))]
fn probe_read(_dir: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Both probes, in the shape `case_probe`'s own test uses: the answer is
    /// re-derived against a directory the test owns, so the cleanup is
    /// observable rather than hidden inside a `TempDir` the probe made itself.
    #[test]
    fn each_probe_removes_its_own_scratch_and_answers_the_same_way_twice() {
        for (name, probe) in [
            ("write", probe_write as fn(&Path) -> bool),
            ("read", probe_read as fn(&Path) -> bool),
        ] {
            let dir = TempDir::new().unwrap();
            let first = probe(dir.path());
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                0,
                "the {name} probe left its scratch behind",
            );
            assert_eq!(first, probe(dir.path()), "the {name} answer is stable");
        }
    }

    // Unix only: the non-unix probes return `true` unconditionally, so there
    // this would assert `true == true` and cover nothing.
    #[cfg(unix)]
    #[test]
    fn a_directory_it_cannot_use_answers_that_the_bits_bind() {
        // The fail-open path, which is otherwise unreachable from a test. A
        // probe that cannot run must not report "the bits do not bind" — that
        // would skip every caller on a machine that had simply run out of disk.
        let missing = Path::new("/definitely/not/a/directory/synthesis-probe");
        assert!(probe_write(missing));
        assert!(probe_read(missing));
    }

    /// The claim the callers actually rely on, restated against each of the
    /// three injections they use — asserted in both directions, so it holds on
    /// an unprivileged runner *and* on a root one.
    ///
    /// Hardcoding the probe's own implementation would make this a change
    /// detector; these are the callers' spellings instead, taken from their
    /// bodies.
    #[cfg(unix)]
    #[test]
    fn every_injection_the_callers_use_agrees_with_the_probe() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();

        // 1. `draft_proposals`, `draft_history`: create inside a 0o555 dir.
        let locked = dir.path().join("files");
        std::fs::create_dir(&locked).expect("dir");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let create_refused = std::fs::write(locked.join("x"), b"x").is_err();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        // 2. `notes`, `draft_history`: remove an entry inside a 0o500 dir.
        let sealed = dir.path().join("sealed");
        std::fs::create_dir(&sealed).expect("dir");
        let victim = sealed.join("victim");
        std::fs::write(&victim, b"x").expect("write");
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o500)).expect("chmod");
        let remove_refused = std::fs::remove_file(&victim).is_err();
        std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        // 3. `comments`: append to an existing read-only file.
        let log = dir.path().join("log");
        std::fs::write(&log, b"x").expect("write");
        let mut perms = std::fs::metadata(&log).expect("meta").permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&log, perms).expect("chmod");
        let append_refused = std::fs::OpenOptions::new().append(true).open(&log).is_err();
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        for (what, refused) in [
            ("creating in a read-only directory", create_refused),
            ("removing from a read-only directory", remove_refused),
            ("appending to a read-only file", append_refused),
        ] {
            assert_eq!(
                refused,
                write_bits_bind(),
                "{what}: the probe and the callers' injection must agree, or a \
                 skip is taken on a run that could have covered the case (or a \
                 test runs into an injection its process overrides)",
            );
        }

        // 4. `fs::access_tests::fr6`: read a 0o000 file. The capability here is
        // `CAP_DAC_READ_SEARCH` rather than `CAP_DAC_OVERRIDE`, which is why it
        // is asked of `read_bits_bind` and not the write probe.
        let secret = dir.path().join("secret");
        std::fs::write(&secret, b"s").expect("write");
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let read_refused = std::fs::read(&secret).is_err();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(read_refused, read_bits_bind(), "reading a 0o000 file");
    }

    #[test]
    fn the_announcement_names_the_test_and_why_it_did_not_run() {
        // Reached on every machine, unlike the branch that prints it.
        let write = skip_message("a_named_test", Injection::Write);
        assert!(write.contains("SKIPPED a_named_test"), "{write}");
        assert!(write.contains("writing"), "{write}");
        assert!(write.contains("root"), "{write}");

        let read = skip_message("a_named_test", Injection::Read);
        assert!(read.contains("reading"), "{read}");
        assert_ne!(write, read, "the two injections read differently");
    }

    #[test]
    fn a_bound_process_takes_no_skip() {
        // The one thing the helper must never do: suppress a test on a runner
        // that can actually exercise it. Only that direction is asserted, and
        // only where it applies — calling the helper when the bits do NOT bind
        // would take the skip path for real, which prints a `SKIPPED a_test`
        // line that reads like a ninth caller in the log, and panics outright
        // under `REQUIRE_ENFORCEMENT`. A self-test must not trip the guard it
        // is testing; the other direction is covered by the callers themselves.
        if write_bits_bind() {
            assert!(!skip_without_enforcement("a_test", Injection::Write));
        }
        if read_bits_bind() {
            assert!(!skip_without_enforcement("a_test", Injection::Read));
        }
    }
}
