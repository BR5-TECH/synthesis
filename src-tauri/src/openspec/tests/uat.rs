//! OAT: the OpenSpec acceptance tests against a real OpenSpec CLI.
//!
//! Every test drives the production seam (`OpenSpec::new` with the real
//! runner, the real probe, the process environment, and the real wait) on a
//! fixture project that the CLI under test makes with `openspec init`. No
//! process, probe, or answer is substituted (OAT-FR-ZRQC).
//!
//! Ignored by default, and run by no pipeline lane: they need the OpenSpec
//! CLI on this machine. Run them with:
//!
//! ```text
//! task uat:openspec
//! OPENSPEC_UAT_BIN=/path/to/openspec task uat:openspec
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tempfile::TempDir;

use crate::logging::LogBuffer;
use crate::openspec::cli::{self, CliEnvironment, RealCliProbe, RealRunner, RealWait};
use crate::openspec::*;

#[path = "uat_reads.rs"]
mod reads;
#[path = "uat_runs.rs"]
mod runs;

/// OAT-FR-HCBT: the optional path of another `openspec` executable.
const BIN_VARIABLE: &str = "OPENSPEC_UAT_BIN";

#[derive(Clone, Default)]
struct Silent;
impl LogSink for Silent {
    fn publish(&self, _state: &crate::logging::BufferState) {}
}

fn buffer() -> &'static LogBuffer {
    Box::leak(Box::new(LogBuffer::new()))
}

/// OAT-FR-HCBT: the process environment, with the folder of
/// `OPENSPEC_UAT_BIN` first in the search path where the variable names a
/// file.
fn environment() -> CliEnvironment {
    let mut env = CliEnvironment::from_process();
    if let Some(bin) = std::env::var_os(BIN_VARIABLE).map(PathBuf::from) {
        assert!(bin.is_file(), "{BIN_VARIABLE} names {} and that is not a file", bin.display());
        let name = bin.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        assert!(
            cli::executable_names().contains(&name),
            "{BIN_VARIABLE} names {}, but the application finds the CLI by the file name {:?} alone",
            bin.display(),
            cli::executable_names()
        );
        let folder = bin.parent().expect("the folder of the CLI").to_string_lossy().into_owned();
        env.path_var = Some(match env.path_var {
            Some(rest) => format!("{folder}:{rest}"),
            None => folder,
        });
    }
    env
}

/// OAT-FR-ZRQC: the production seam over the real CLI and the real wait.
fn seam() -> OpenSpec {
    OpenSpec::new(Arc::new(RealRunner), Arc::new(RealCliProbe), environment())
        .with_wait(Arc::new(RealWait))
}

/// OAT-FR-UQCL: a setup run of the CLI sends no telemetry, as a run of the
/// production runner does not.
fn quiet(mut command: Command) -> Command {
    command.env("OPENSPEC_TELEMETRY", "0").env("DO_NOT_TRACK", "1");
    command
}

/// The CLI under test, as the production resolution finds it.
struct CliUnderTest {
    path: PathBuf,
    version: String,
}

/// OAT-FR-TJAV, OAT-FR-MSXV: the CLI that the resolution finds, and its
/// version. A missing or unusable CLI fails the test with its cause.
fn cli_under_test() -> CliUnderTest {
    let env = environment();
    let Some(path) = cli::resolve(&env, &RealCliProbe) else {
        panic!(
            "no `openspec` executable was found on PATH or in the per-user Node folders; \
             install the OpenSpec CLI (npm install -g @fission-ai/openspec) or set {BIN_VARIABLE}"
        );
    };
    if let Some(bin) = std::env::var_os(BIN_VARIABLE).map(PathBuf::from) {
        let same = |p: &Path| std::fs::canonicalize(p).ok();
        assert_eq!(
            same(&path),
            same(&bin),
            "the resolution found {}, not the CLI that {BIN_VARIABLE} names",
            path.display()
        );
    }
    let out =
        quiet(Command::new(&path)).arg("--version").output().expect("`openspec --version` runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    match cli::parse_version(&text) {
        Some((version, major)) if out.status.success() && major >= cli::MIN_MAJOR => {
            // OAT-FR-MSXV: the parser reads what the CLI printed, compared
            // with the printed text itself and not with the parser alone.
            assert!(
                text.trim().ends_with(&version),
                "the version {version} is not what `openspec --version` printed: {:?}",
                text.trim()
            );
            CliUnderTest { path, version }
        }
        _ => panic!(
            "the OpenSpec CLI at {} is not usable: `--version` printed {:?} with {}",
            path.display(),
            text.trim(),
            out.status
        ),
    }
}

/// A fixture project (OAT-FR-PLXY). The folder is removed when the value is
/// dropped, also when the test fails.
struct Fixture {
    dir: TempDir,
    cli: CliUnderTest,
}

impl Fixture {
    /// OAT-FR-PLXY, OAT-FR-GNEU: `git init`, `openspec init --tools none`,
    /// the capabilities and changes of the fixture, and one commit.
    fn new() -> Fixture {
        let cli = cli_under_test();
        let dir = tempfile::Builder::new().prefix("oat é ").tempdir().expect("a temporary folder");
        let fixture = Fixture { dir, cli };
        fixture.git(&["init", "-q"]);
        fixture.openspec_ok(&["init", "--tools", "none", "--no-animation", "."]);
        for (rel, body) in FILES {
            fixture.write(rel, body);
        }
        fixture.commit("fixture");
        fixture
    }

    /// OAT-FR-PLXY: a project right after `openspec init`, with nothing added.
    fn empty() -> Fixture {
        let cli = cli_under_test();
        let dir = tempfile::Builder::new().prefix("oat é ").tempdir().expect("a temporary folder");
        let fixture = Fixture { dir, cli };
        fixture.git(&["init", "-q"]);
        fixture.openspec_ok(&["init", "--tools", "none", "--no-animation", "."]);
        fixture
    }

    /// The fixture path in the form the temporary folder gives, which is a
    /// symbolic link on macOS (OAT-FR-RYAS).
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn root(&self) -> RootFs {
        RootFs::for_root(self.path())
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path().join(rel)).unwrap_or_default()
    }

    fn exists(&self, rel: &str) -> bool {
        self.path().join(rel).exists()
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=oat", "-c", "user.email=oat@example.invalid"])
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// One run of the CLI under test that must succeed, for the setup alone.
    fn openspec_ok(&self, args: &[&str]) {
        let out = quiet(Command::new(&self.cli.path))
            .args(args)
            .current_dir(self.path())
            .output()
            .expect("the OpenSpec CLI runs");
        assert!(
            out.status.success(),
            "openspec {} {args:?} failed: {}",
            self.cli.version,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// OAT-FR-PEFN: the context of every failed assertion.
    fn says(&self, test: &str, subcommand: &str) -> String {
        format!("[{test}] openspec {} `{subcommand}`", self.cli.version)
    }
}

/// OAT-FR-GNEU: two live capabilities, one capability whose id holds a `/`,
/// a change with every delta kind and two task groups, and a change that the
/// CLI does not accept.
const FILES: &[(&str, &str)] = &[
    (
        "openspec/specs/auth/spec.md",
        "# Auth Specification\n\n## Purpose\nUsers sign in to the application and sign out of it on every device.\n\n\
         ## Requirements\n### Requirement: Sign in\nThe system SHALL let a user sign in with an email and a password.\n\n\
         #### Scenario: Valid credentials\n- **WHEN** a user submits a valid email and password\n- **THEN** the system starts a session\n\n\
         ### Requirement: Session expiry\nThe system SHALL end a session after 30 minutes without activity.\n\n\
         #### Scenario: Idle session\n- **WHEN** a session is idle for 30 minutes\n- **THEN** the system ends the session\n",
    ),
    (
        "openspec/specs/billing/spec.md",
        "# Billing Specification\n\n## Purpose\nMonthly charges.\n\n\
         ## Requirements\n### Requirement: Monthly invoice\nThe system SHALL send an invoice on the first day of each month.\n\n\
         #### Scenario: First day\n- **WHEN** the date is the first day of a month\n- **THEN** the system sends an invoice\n",
    ),
    (
        "openspec/specs/ui/theme/spec.md",
        "# Theme Specification\n\n## Purpose\nThe application shows a light theme and a dark theme to each user of the application.\n\n\
         ## Requirements\n### Requirement: Dark theme\nThe system SHALL show a dark theme when the user selects it.\n\n\
         #### Scenario: Select dark\n- **WHEN** the user selects the dark theme\n- **THEN** the system shows the dark theme\n\n\
         ### Requirement: Old toggle\nThe system SHALL show a theme toggle in the footer.\n\n\
         #### Scenario: Footer\n- **WHEN** the user opens any page\n- **THEN** the footer shows the toggle\n",
    ),
    (
        "openspec/changes/add-logout/proposal.md",
        "## Why\nUsers cannot end a session on a shared computer, and the theme toggle is in the wrong place.\n\n\
         ## What Changes\n- Add a sign-out action.\n- Shorten the session expiry.\n- Remove the footer toggle.\n\n\
         ## Impact\n- Affected specs: auth, ui/theme\n",
    ),
    (
        "openspec/changes/add-logout/design.md",
        "## Context\nThe session store keeps one token for each user.\n\n## Decisions\n- The sign-out action removes the token.\n",
    ),
    (
        "openspec/changes/add-logout/tasks.md",
        "## 1. Backend\n- [x] 1.1 Add the sign-out endpoint\n- [ ] 1.2 Remove the token from the store\n\n\
         ## 2. Frontend\n- [ ] 2.1 Add the sign-out button\n",
    ),
    (
        "openspec/changes/add-logout/specs/auth/spec.md",
        "## ADDED Requirements\n### Requirement: Sign out\nThe system SHALL let a signed-in user end the session.\n\n\
         #### Scenario: Sign out\n- **WHEN** a signed-in user selects sign out\n- **THEN** the system ends the session\n\n\
         ## MODIFIED Requirements\n### Requirement: Session expiry\nThe system SHALL end a session after 15 minutes without activity.\n\n\
         #### Scenario: Idle session\n- **WHEN** a session is idle for 15 minutes\n- **THEN** the system ends the session\n\n\
         ## RENAMED Requirements\n- FROM: `### Requirement: Sign in`\n- TO: `### Requirement: Sign in with password`\n",
    ),
    (
        "openspec/changes/add-logout/specs/ui/theme/spec.md",
        "## REMOVED Requirements\n### Requirement: Old toggle\n**Reason**: The settings page holds the toggle.\n\
         **Migration**: Use the settings page.\n",
    ),
    ("openspec/changes/bad-change/proposal.md", "## Why\nShort.\n"),
];

/// The names of the fixture's changes.
const GOOD: &str = "add-logout";
const BAD: &str = "bad-change";
