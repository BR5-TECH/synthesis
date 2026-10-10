//! The guards for `specifications/infra/NLL-native-library-linkage.md`.
//!
//! Three kinds of test are here.
//!
//! - The manifest tests read `Cargo.toml` and `Cargo.lock`. They check that the
//!   macOS target compiles each known native library from source, and that
//!   every `-sys` crate in the tree is classified, so a new native dependency
//!   fails on every host, Linux CI included.
//! - The script tests run `tools/macos-linkage/check-linkage.sh` against the
//!   checked-in `fake-otool.sh` (through the `OTOOL` variable), so they also
//!   run on every host.
//! - On macOS, one more test runs the real check on this test binary.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn script() -> PathBuf {
    manifest_dir()
        .parent()
        .expect("src-tauri has a parent directory")
        .join("tools/macos-linkage/check-linkage.sh")
}

fn fake_otool() -> PathBuf {
    manifest_dir().join("src/native_linkage_tests/fake-otool.sh")
}

/// The text the check prints on a pass, and only on a pass.
const PASS_MESSAGE: &str = "is a system library or is inside the bundle";

/// The data files the fake `otool` prints, and an executable path for the
/// check to read. The tests write only data here; they run no file they wrote.
struct Fixture {
    dir: tempfile::TempDir,
    fail: Option<&'static str>,
}

impl Fixture {
    fn new(load_list: &str, load_commands: &str) -> Self {
        let dir = tempfile::tempdir().expect("temporary directory");
        std::fs::write(dir.path().join("load_list.txt"), load_list).unwrap();
        std::fs::write(dir.path().join("load_commands.txt"), load_commands).unwrap();
        std::fs::write(dir.path().join("synthesis"), b"not read by the fake").unwrap();
        Self { dir, fail: None }
    }

    /// The fake `otool` exits non-zero for `option` (`-L` or `-l`).
    fn failing(mut self, option: &'static str) -> Self {
        self.fail = Some(option);
        self
    }

    fn executable(&self) -> PathBuf {
        self.dir.path().join("synthesis")
    }

    fn run(&self) -> Output {
        self.run_with(&fake_otool(), &self.executable())
    }

    fn run_with(&self, otool: &Path, executable: &Path) -> Output {
        let mut command = Command::new("sh");
        command
            .arg(script())
            .arg(executable)
            .env("OTOOL", otool)
            .env("FAKE_OTOOL_EXPECT", executable)
            .env("FAKE_OTOOL_LIST", self.dir.path().join("load_list.txt"))
            .env("FAKE_OTOOL_COMMANDS", self.dir.path().join("load_commands.txt"));
        match self.fail {
            Some(option) => command.env("FAKE_OTOOL_FAIL", option),
            None => command.env_remove("FAKE_OTOOL_FAIL"),
        };
        command.output().expect("sh runs the linkage check")
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A failure exits non-zero and does not print the pass message.
fn assert_failed(output: &Output) {
    assert!(!output.status.success(), "stdout: {}", stdout(output));
    assert!(
        !stdout(output).contains(PASS_MESSAGE),
        "a failure reported a pass: {}",
        stdout(output)
    );
}

fn assert_passed(output: &Output) {
    assert!(output.status.success(), "stderr: {}", stderr(output));
    assert!(stdout(output).contains(PASS_MESSAGE), "{}", stdout(output));
}

const HEADER: &str = "/tmp/synthesis.app/Contents/MacOS/synthesis:\n";

fn library_line(library: &str) -> String {
    format!("\t{library} (compatibility version 1.0.0, current version 1.0.0)\n")
}

fn load_list(libraries: &[&str]) -> String {
    let mut text = HEADER.to_string();
    for library in libraries {
        text.push_str(&library_line(library));
    }
    text
}

/// `otool -l` output: a segment, a dylinker, and a dylib load command, then
/// one `LC_RPATH` per run path. The `name` fields of the other commands must
/// not be read as run paths.
fn load_commands(rpaths: &[&str]) -> String {
    let mut text = String::from(
        "Load command 0\n      cmd LC_SEGMENT_64\n  cmdsize 72\n  segname __PAGEZERO\n\
         Load command 1\n          cmd LC_LOAD_DYLINKER\n      cmdsize 32\n         name /usr/lib/dyld (offset 12)\n\
         Load command 2\n          cmd LC_LOAD_DYLIB\n      cmdsize 56\n         name /usr/lib/libSystem.B.dylib (offset 24)\n",
    );
    for (index, rpath) in rpaths.iter().enumerate() {
        text.push_str(&format!(
            "Load command {}\n          cmd LC_RPATH\n      cmdsize 32\n         path {rpath} (offset 12)\n",
            index + 3
        ));
    }
    text
}

const SYSTEM_LIBRARIES: &[&str] = &[
    "/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit",
    "/usr/lib/libSystem.B.dylib",
    "/usr/lib/libobjc.A.dylib",
];

fn with(extra: &[&'static str]) -> Vec<&'static str> {
    let mut libraries = SYSTEM_LIBRARIES.to_vec();
    libraries.extend_from_slice(extra);
    libraries
}

#[cfg(unix)]
mod script {
    use super::*;

    // NLL-FR-NPMB: system libraries and libraries inside the bundle pass, and
    // the check exits with status zero.
    #[test]
    fn system_and_bundle_libraries_pass() {
        let libraries = with(&[
            "@executable_path/../Frameworks/Helper.dylib",
            "@loader_path/libLocal.dylib",
        ]);
        let fixture = Fixture::new(&load_list(&libraries), &load_commands(&[]));

        assert_passed(&fixture.run());
    }

    // NLL-FR-NPMB: a weak library line has a different suffix after the
    // version clause, and is still read and judged.
    #[test]
    fn a_weak_library_is_read() {
        let text = format!(
            "{}\t/opt/homebrew/lib/libweak.dylib (compatibility version 1.0.0, current version 1.0.0, weak)\n",
            load_list(SYSTEM_LIBRARIES)
        );
        let fixture = Fixture::new(&text, &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        assert!(stderr(&output).contains("/opt/homebrew/lib/libweak.dylib"));
    }

    // NLL-FR-NPMB, NLL-FR-DQEK: a universal binary has one header per
    // architecture. Each slice is read, so a Homebrew library in the second
    // slice only is still found.
    #[test]
    fn every_slice_of_a_universal_binary_is_read() {
        let text = format!(
            "/tmp/synthesis (architecture x86_64):\n{}/tmp/synthesis (architecture arm64):\n{}{}",
            library_line("/usr/lib/libSystem.B.dylib"),
            library_line("/usr/lib/libSystem.B.dylib"),
            library_line("/opt/homebrew/opt/libgit2/lib/libgit2.1.9.dylib"),
        );
        let fixture = Fixture::new(&text, &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        let message = stderr(&output);
        assert!(message.contains("/opt/homebrew/opt/libgit2/lib/libgit2.1.9.dylib"), "{message}");
        assert!(!message.contains("(architecture"), "{message}");
    }

    // NLL-FR-DQEK: Homebrew libraries fail the check. The diagnostic names
    // each library that does not pass, and only those, and says that the
    // bundle cannot start under the hardened runtime.
    #[test]
    fn homebrew_libraries_fail_and_are_named() {
        let libraries = with(&[
            "/opt/homebrew/opt/openssl@4/lib/libssl.4.dylib",
            "/opt/homebrew/opt/libgit2/lib/libgit2.1.9.dylib",
        ]);
        let fixture = Fixture::new(&load_list(&libraries), &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        let message = stderr(&output);
        assert!(message.contains("/opt/homebrew/opt/openssl@4/lib/libssl.4.dylib"), "{message}");
        assert!(message.contains("/opt/homebrew/opt/libgit2/lib/libgit2.1.9.dylib"), "{message}");
        assert!(message.contains("hardened runtime"), "{message}");
        assert!(!message.contains("/usr/lib/libSystem.B.dylib"), "{message}");
    }

    // NLL-FR-DQEK: a library in `/usr/local` is neither a system library nor
    // inside the bundle.
    #[test]
    fn usr_local_library_fails() {
        let libraries = with(&["/usr/local/lib/libz.1.dylib"]);
        let fixture = Fixture::new(&load_list(&libraries), &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        assert!(stderr(&output).contains("/usr/local/lib/libz.1.dylib"));
    }

    // NLL-FR-NPMB: an `@rpath/` library passes when every run path is inside
    // the bundle.
    #[test]
    fn rpath_library_passes_when_every_run_path_is_inside_the_bundle() {
        let libraries = with(&["@rpath/libBundled.dylib"]);
        let commands = load_commands(&["@executable_path/../Frameworks", "@loader_path/lib"]);
        let fixture = Fixture::new(&load_list(&libraries), &commands);

        assert_passed(&fixture.run());
    }

    // NLL-FR-NPMB, NLL-FR-DQEK: an `@rpath/` library fails when one run path
    // is outside the bundle.
    #[test]
    fn rpath_library_fails_when_a_run_path_is_outside_the_bundle() {
        let libraries = with(&["@rpath/libgit2.1.9.dylib"]);
        let commands = load_commands(&["@executable_path/../Frameworks", "/opt/homebrew/lib"]);
        let fixture = Fixture::new(&load_list(&libraries), &commands);

        let output = fixture.run();

        assert_failed(&output);
        assert!(stderr(&output).contains("@rpath/libgit2.1.9.dylib"));
    }

    // NLL-FR-NPMB, NLL-FR-DQEK: an `@rpath/` library fails when the executable
    // has no run path, because dyld cannot find it.
    #[test]
    fn rpath_library_fails_when_there_is_no_run_path() {
        let libraries = with(&["@rpath/libBundled.dylib"]);
        let fixture = Fixture::new(&load_list(&libraries), &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        assert!(stderr(&output).contains("@rpath/libBundled.dylib"));
    }

    // NLL-FR-NPMB: only libraries are judged. A run path outside the bundle
    // is not a failure when no `@rpath/` library uses it.
    #[test]
    fn a_run_path_alone_is_not_judged() {
        let fixture = Fixture::new(
            &load_list(SYSTEM_LIBRARIES),
            &load_commands(&["/opt/homebrew/lib"]),
        );

        assert_passed(&fixture.run());
    }

    // NLL-FR-QFZH: an absent `otool` fails the check with one diagnostic that
    // names it.
    #[test]
    fn absent_otool_fails_and_is_named() {
        let fixture = Fixture::new(&load_list(SYSTEM_LIBRARIES), &load_commands(&[]));
        let missing = fixture.dir.path().join("no-such-otool");

        let output = fixture.run_with(&missing, &fixture.executable());

        assert_failed(&output);
        let message = stderr(&output);
        assert!(message.contains("\"otool\" is missing"), "{message}");
        assert_eq!(message.lines().count(), 1, "{message}");
    }

    // NLL-FR-QFZH: an absent executable fails the check with one diagnostic
    // that names it.
    #[test]
    fn absent_executable_fails_and_is_named() {
        let fixture = Fixture::new(&load_list(SYSTEM_LIBRARIES), &load_commands(&[]));
        let missing = fixture.dir.path().join("no-such-executable");

        let output = fixture.run_with(&fake_otool(), &missing);

        assert_failed(&output);
        let message = stderr(&output);
        assert!(message.contains("no-such-executable"), "{message}");
        assert_eq!(message.lines().count(), 1, "{message}");
    }

    // NLL-FR-FQPR: output that lists no library (a file that is not Mach-O)
    // is not a pass.
    #[test]
    fn a_file_that_lists_no_library_is_not_a_pass() {
        let fixture = Fixture::new(HEADER, &load_commands(&[]));

        let output = fixture.run();

        assert_failed(&output);
        assert!(stderr(&output).contains("lists no library"), "{}", stderr(&output));
    }

    // NLL-FR-FQPR: when `otool -L` or `otool -l` cannot read the executable,
    // the check fails and names the executable.
    #[test]
    fn an_executable_otool_cannot_read_is_not_a_pass() {
        for option in ["-L", "-l"] {
            let fixture =
                Fixture::new(&load_list(SYSTEM_LIBRARIES), &load_commands(&[])).failing(option);

            let output = fixture.run();

            assert_failed(&output);
            let message = stderr(&output);
            assert!(message.contains(&format!("\"otool {option}\" cannot read")), "{message}");
            assert!(
                message.contains(&fixture.executable().display().to_string()),
                "{message}"
            );
        }
    }

    // NLL-FR-NPMB: the check takes exactly one argument.
    #[test]
    fn a_wrong_argument_count_is_a_usage_error() {
        for arguments in [vec![], vec!["one", "two"]] {
            let output = Command::new("sh")
                .arg(script())
                .args(&arguments)
                .env("OTOOL", fake_otool())
                .output()
                .unwrap();

            assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
            assert!(stderr(&output).contains("usage"));
        }
    }
}

/// The crates that the macOS target compiles from source through a feature
/// (NLL-FR-LTEO), with that feature.
const COMPILED_BY_FEATURE: &[(&str, &str)] = &[
    ("libgit2-sys", "vendored"),
    ("openssl-sys", "vendored"),
    ("libz-sys", "static"),
];

/// Every other `-sys` crate in `Cargo.lock`, and why it needs no feature on
/// macOS.
const CLASSIFIED: &[(&str, &str)] = &[
    // Always compiles its native code from source.
    ("libssh2-sys", "bundled source unless LIBSSH2_SYS_USE_PKG_CONFIG is set"),
    ("aws-lc-sys", "bundled source"),
    ("mac-notification-sys", "bundled source; system frameworks"),
    // Links only operating-system libraries or frameworks.
    ("core-foundation-sys", "system framework"),
    ("security-framework-sys", "system framework"),
    ("system-configuration-sys", "system framework"),
    ("fsevent-sys", "system framework"),
    ("kqueue-sys", "libc"),
    ("dirs-sys", "libc"),
    // No native library.
    ("js-sys", "wasm bindings"),
    ("web-sys", "wasm bindings"),
    ("jni-sys", "Android JNI declarations"),
    ("linux-raw-sys", "Linux syscall declarations"),
    // Not built for macOS.
    ("ndk-sys", "Android"),
    ("windows-sys", "Windows"),
    ("webview2-com-sys", "Windows"),
    ("vswhom-sys", "Windows"),
    ("inotify-sys", "Linux"),
    ("libdbus-sys", "Linux"),
    ("libappindicator-sys", "Linux"),
    ("atk-sys", "Linux GTK"),
    ("gdk-pixbuf-sys", "Linux GTK"),
    ("gdk-sys", "Linux GTK"),
    ("gdkwayland-sys", "Linux GTK"),
    ("gdkx11-sys", "Linux GTK"),
    ("gio-sys", "Linux GTK"),
    ("glib-sys", "Linux GTK"),
    ("gobject-sys", "Linux GTK"),
    ("gtk-sys", "Linux GTK"),
    ("pango-sys", "Linux GTK"),
    ("javascriptcore-rs-sys", "Linux WebKitGTK"),
    ("soup3-sys", "Linux WebKitGTK"),
    ("webkit2gtk-sys", "Linux WebKitGTK"),
];

// NLL-FR-LTEO: for the macOS target, `Cargo.toml` compiles each native library
// that can otherwise come from the build host from source.
#[test]
fn macos_target_compiles_native_libraries_from_source() {
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(manifest_dir().join("Cargo.toml")).unwrap())
            .unwrap();
    let macos = &manifest["target"]["cfg(target_os = \"macos\")"]["dependencies"];

    for (krate, feature) in COMPILED_BY_FEATURE {
        let features = macos
            .get(krate)
            .and_then(|dependency| dependency.get("features"))
            .and_then(|features| features.as_array())
            .unwrap_or_else(|| panic!("the macOS target names no features for {krate}"));
        assert!(
            features.iter().any(|f| f.as_str() == Some(feature)),
            "{krate} must enable `{feature}` for the macOS target"
        );
    }
}

// NLL-FR-AVJK: every `-sys` crate in `Cargo.lock` is classified. A new native
// dependency fails here, on every host, until it is compiled in for macOS
// (and added to COMPILED_BY_FEATURE) or classified in CLASSIFIED.
#[test]
fn every_sys_crate_in_the_lock_file_is_classified() {
    let lock: toml::Value =
        toml::from_str(&std::fs::read_to_string(manifest_dir().join("Cargo.lock")).unwrap())
            .unwrap();
    let in_lock: BTreeSet<&str> = lock["package"]
        .as_array()
        .expect("Cargo.lock has packages")
        .iter()
        .filter_map(|package| package["name"].as_str())
        .filter(|name| name.ends_with("-sys"))
        .collect();
    let known: BTreeSet<&str> = COMPILED_BY_FEATURE
        .iter()
        .chain(CLASSIFIED)
        .map(|(name, _)| *name)
        .collect();

    let unclassified: Vec<&str> = in_lock.difference(&known).copied().collect();
    assert!(
        unclassified.is_empty(),
        "these -sys crates are not classified: {unclassified:?}. If one is built for \
         macOS and links a native library, enable its `vendored` or `static` feature \
         in the macOS table of Cargo.toml (NLL-FR-AVJK) and add it to \
         COMPILED_BY_FEATURE. Otherwise add it to CLASSIFIED with the reason."
    );
    assert!(in_lock.len() > 10, "the lock file parse found almost no -sys crates");
}

// NLL-FR-VMBF, NLL-FR-AVJK: this test binary is built from the same native
// dependencies as the application, and approximates its linkage. It loads only
// system libraries, so no dependency links a library of the build host.
// `task build:macos` runs the same check on the application itself.
#[cfg(target_os = "macos")]
#[test]
fn this_binary_loads_only_system_libraries() {
    let executable = std::env::current_exe().expect("path of the test binary");

    let output = Command::new("sh")
        .arg(script())
        .arg(&executable)
        .env_remove("OTOOL")
        .output()
        .expect("sh runs the linkage check");

    assert_passed(&output);
}
