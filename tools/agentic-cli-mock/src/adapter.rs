//! The tool-adapter registry.
//!
//! ACM-FR-28: adding a tool is adding one adapter and registering it. Nothing
//! below this line knows what a scenario is, how an expectation is matched, how
//! a response is emitted, or which exit code classifies what — and nothing in
//! those shared paths knows that `claude`, `codex`, or `docker` exist.
//!
//! ## Why `docker` is an adapter and not a mode
//!
//! Standing in for the container runtime is the same job as standing in for a
//! CLI: a vector arrives, it is checked against what the caller claimed it
//! would send, and a configured response is replayed. A `docker run` vector
//! carries the vendor's own vector as its tail and forwards the caller's stdin
//! to the container, so one run of this adapter asserts the container
//! invocation and the vendor invocation together (ACM-FR-10, ACM-FR-12, ACM-FR-18, ACM-FR-20). Nothing about it
//! needs a second code path, which is exactly what ACM-FR-28 predicted.

use std::ffi::OsStr;

use crate::diagnostics::{Check, Mismatch, MismatchType};

/// One tool's own validation over the received argument vector.
pub trait ToolAdapter: Sync {
    /// The value `--tool` must carry to select this adapter.
    fn id(&self) -> &'static str;

    /// ACM-FR-18 / ACM-FR-19: the tool-specific half of validation. Runs before
    /// any scenario expectation and fails as an expectation mismatch, so a
    /// scenario can neither waive it nor be exempted by it.
    fn validate(&self, received: &[&OsStr]) -> Option<Mismatch>;
}

/// The shape every shipped adapter has: a headless convention expressed as one
/// or more tokens, each of which must appear somewhere in the vector.
///
/// ACM-FR-18: position is not constrained, and nothing else about the vector is.
///
/// More than one token is what lets a single adapter stand in for a whole
/// stack. A container runtime with an entrypoint passes everything after the
/// image straight to the program it launches, so the vector reaching this
/// process carries the runtime's own arguments *and* the vendor's, one after
/// the other. An adapter requiring both tokens therefore validates both halves
/// in one run — which is what makes replacing `docker + claude` with this
/// executable a like-for-like substitution rather than a partial one.
pub struct HeadlessTokenAdapter {
    id: &'static str,
    tokens: &'static [&'static str],
}

impl HeadlessTokenAdapter {
    /// Every token listed must appear somewhere in the received vector. An
    /// empty list would require nothing, so it is rejected at construction
    /// rather than silently accepting every invocation.
    pub const fn requiring(id: &'static str, tokens: &'static [&'static str]) -> Self {
        assert!(
            !tokens.is_empty(),
            "an adapter requiring no token would validate nothing"
        );
        HeadlessTokenAdapter { id, tokens }
    }
}

impl ToolAdapter for HeadlessTokenAdapter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn validate(&self, received: &[&OsStr]) -> Option<Mismatch> {
        let present = self.tokens.iter().all(|token| {
            received
                .iter()
                .any(|arg| arg.as_encoded_bytes() == token.as_bytes())
        });

        if present {
            None
        } else {
            // ACM-FR-24: the record names the category and nothing else. Which
            // token was wanted is documented, not disclosed per-run.
            Some(Mismatch::new(
                Check::Adapter,
                MismatchType::MissingHeadlessArgument,
            ))
        }
    }
}

/// Claude Code's headless convention.
static CLAUDE: HeadlessTokenAdapter = HeadlessTokenAdapter::requiring("claude", &["-p"]);
/// Codex's headless convention: the `exec` subcommand, which is the token the
/// real CLI accepts. ACM-FR-18 requires the real spelling precisely so a vector
/// a production call site generated reaches this adapter unmodified — a
/// mock-only flag would make an exact-vector expectation assert something
/// production never emits.
static CODEX: HeadlessTokenAdapter = HeadlessTokenAdapter::requiring("codex", &["exec"]);
/// The container runtime's own convention, on its own — for a caller checking
/// the runtime layer without caring what it launches.
static DOCKER: HeadlessTokenAdapter = HeadlessTokenAdapter::requiring("docker", &["run"]);

/// The whole stack: a container runtime launching Claude Code.
///
/// This is the substitution a caller actually makes. A runtime with an
/// entrypoint passes everything after the image straight through to the program
/// it launches and forwards stdin to it, so replacing the runtime with this
/// executable replaces the vendor CLI too — there is no second process to
/// stand in for. Requiring both tokens is what makes that a like-for-like
/// substitution: a vector missing either half fails, where `docker` alone would
/// have waved a missing `-p` through.
static DOCKER_CLAUDE: HeadlessTokenAdapter =
    HeadlessTokenAdapter::requiring("docker+claude", &["run", "-p"]);
/// The whole stack: a container runtime launching Codex.
static DOCKER_CODEX: HeadlessTokenAdapter =
    HeadlessTokenAdapter::requiring("docker+codex", &["run", "exec"]);

/// The registry. Adding a tool means adding one entry here.
pub static REGISTRY: &[&(dyn ToolAdapter + 'static)] =
    &[&CLAUDE, &CODEX, &DOCKER, &DOCKER_CLAUDE, &DOCKER_CODEX];

/// Resolve a `--tool` value against a registry. Returns `None` for a value that
/// names no adapter, which ACM-FR-05 makes a configuration error.
pub fn resolve<'a>(
    registry: &'a [&'a (dyn ToolAdapter + 'static)],
    id: &str,
) -> Option<&'a (dyn ToolAdapter + 'static)> {
    registry.iter().copied().find(|a| a.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn refs(args: &[OsString]) -> Vec<&OsStr> {
        args.iter().map(OsString::as_os_str).collect()
    }

    #[test]
    fn the_registry_ships_each_layer_and_each_whole_stack() {
        let ids: Vec<_> = REGISTRY.iter().map(|a| a.id()).collect();
        assert_eq!(
            ids,
            vec!["claude", "codex", "docker", "docker+claude", "docker+codex"]
        );
    }

    /// A composite adapter replaces the runtime *and* the vendor it launches,
    /// which is what a caller substituting this executable for `docker`
    /// actually needs: the entrypoint means there is no second process left to
    /// stand in for.
    #[test]
    fn a_composite_adapter_requires_both_halves_of_the_stack() {
        let stack = resolve(REGISTRY, "docker+claude").unwrap();

        // The whole vector a caller generates: runtime options, the image, then
        // the vendor's own arguments as the tail.
        let whole = os(&[
            "run",
            "--rm",
            "--workdir",
            "/workspace",
            "img@sha256:abc",
            "-p",
            "--output-format",
            "json",
        ]);
        assert!(stack.validate(&refs(&whole)).is_none());

        // Either half missing is a failure, where the single-layer adapters
        // would each have accepted it.
        let runtime_only = os(&["run", "--rm", "img@sha256:abc"]);
        assert!(stack.validate(&refs(&runtime_only)).is_some());
        assert!(resolve(REGISTRY, "docker")
            .unwrap()
            .validate(&refs(&runtime_only))
            .is_none());

        let vendor_only = os(&["-p", "--output-format", "json"]);
        assert!(stack.validate(&refs(&vendor_only)).is_some());
        assert!(resolve(REGISTRY, "claude")
            .unwrap()
            .validate(&refs(&vendor_only))
            .is_none());

        // And the codex stack wants its own vendor token, not Claude's.
        let codex_stack = resolve(REGISTRY, "docker+codex").unwrap();
        assert!(codex_stack.validate(&refs(&whole)).is_some());
        assert!(codex_stack
            .validate(&refs(&os(&["run", "img@sha256:abc", "exec", "-", "--json"])))
            .is_none());
    }

    #[test]
    fn resolve_finds_a_registered_adapter_and_rejects_anything_else() {
        assert_eq!(resolve(REGISTRY, "claude").map(|a| a.id()), Some("claude"));
        assert_eq!(resolve(REGISTRY, "codex").map(|a| a.id()), Some("codex"));
        assert_eq!(resolve(REGISTRY, "docker").map(|a| a.id()), Some("docker"));
        assert_eq!(
            resolve(REGISTRY, "docker+claude").map(|a| a.id()),
            Some("docker+claude")
        );
        assert_eq!(
            resolve(REGISTRY, "docker+codex").map(|a| a.id()),
            Some("docker+codex")
        );
        assert!(resolve(REGISTRY, "opencode").is_none());
        assert!(resolve(REGISTRY, "Claude").is_none());
        assert!(resolve(REGISTRY, "").is_none());
    }

    /// ACM-FR-18: Claude requires `-p`, at any position.
    #[test]
    fn claude_accepts_dash_p_wherever_it_appears() {
        let claude = resolve(REGISTRY, "claude").unwrap();

        for vector in [
            os(&["-p", "prompt"]),
            os(&["prompt", "-p"]),
            os(&["--model", "opus", "-p", "prompt"]),
        ] {
            assert!(claude.validate(&refs(&vector)).is_none());
        }
    }

    #[test]
    fn claude_rejects_a_vector_without_the_exact_token() {
        let claude = resolve(REGISTRY, "claude").unwrap();

        for vector in [
            os(&["--print", "prompt"]),
            os(&["-P", "prompt"]),
            os(&["-p=prompt"]),
            os(&["prompt"]),
            os(&[]),
        ] {
            let mismatch = claude.validate(&refs(&vector)).expect("should fail");
            assert_eq!(mismatch.check, Check::Adapter);
            assert_eq!(
                mismatch.mismatch_type,
                MismatchType::MissingHeadlessArgument
            );
            // ACM-FR-25: an adapter failure reports no location and no lengths.
            assert_eq!(mismatch.index, None);
            assert_eq!(mismatch.name, None);
            assert_eq!(mismatch.expected_length, None);
            assert_eq!(mismatch.actual_length, None);
        }
    }

    /// ACM-FR-18: Codex requires the literal `exec` subcommand — the token the
    /// real CLI accepts, so a production vector validates here unmodified.
    #[test]
    fn codex_requires_the_literal_exec_token() {
        let codex = resolve(REGISTRY, "codex").unwrap();

        assert!(codex.validate(&refs(&os(&["exec", "-"]))).is_none());
        assert!(codex.validate(&refs(&os(&["-", "exec"]))).is_none());

        // The obsolete mock-only flag is no longer accepted, so a call site
        // still emitting it fails the adapter rather than passing silently.
        for vector in [
            os(&["-exec", "run"]),
            os(&["--exec", "run"]),
            os(&["-p"]),
            os(&["Exec"]),
        ] {
            assert!(codex.validate(&refs(&vector)).is_some());
        }
    }

    /// ACM-FR-18: the container runtime requires `run`, at any position.
    #[test]
    fn docker_requires_the_literal_run_token() {
        let docker = resolve(REGISTRY, "docker").unwrap();

        assert!(docker
            .validate(&refs(&os(&["run", "--rm", "img", "exec"])))
            .is_none());
        assert!(docker.validate(&refs(&os(&["--rm", "run"]))).is_none());

        for vector in [os(&["create", "img"]), os(&["--run"]), os(&["Run"]), os(&[])] {
            let mismatch = docker.validate(&refs(&vector)).expect("should fail");
            assert_eq!(mismatch.check, Check::Adapter);
            assert_eq!(
                mismatch.mismatch_type,
                MismatchType::MissingHeadlessArgument
            );
        }
    }

    /// ACM-FR-10, ACM-FR-12, ACM-FR-18, ACM-FR-20: a `docker run` vector carries the vendor's own vector as its
    /// tail, so one adapter pass covers both halves of the invocation.
    #[test]
    fn the_docker_adapter_accepts_a_vector_carrying_a_vendor_tail() {
        let docker = resolve(REGISTRY, "docker").unwrap();

        let vector = os(&[
            "run",
            "--rm",
            "--workdir",
            "/workspace",
            "synthesis-agent-codex@sha256:abc",
            "exec",
            "-",
            "--json",
        ]);
        assert!(docker.validate(&refs(&vector)).is_none());
        // The same vector's tail is what the codex adapter would validate.
        let codex = resolve(REGISTRY, "codex").unwrap();
        assert!(codex.validate(&refs(&vector)).is_none());
    }

    /// ACM-FR-28: a third adapter is one value, and the trait is all the shared
    /// paths ever see.
    #[test]
    fn a_third_adapter_registers_without_touching_anything_shared() {
        static EXTRA: HeadlessTokenAdapter =
            HeadlessTokenAdapter::requiring("demo", &["--headless"]);
        let registry: &[&(dyn ToolAdapter + 'static)] = &[&CLAUDE, &CODEX, &EXTRA];

        let demo = resolve(registry, "demo").expect("registered");
        assert_eq!(demo.id(), "demo");
        assert!(demo.validate(&refs(&os(&["--headless", "x"]))).is_none());
        assert!(demo.validate(&refs(&os(&["-p"]))).is_some());
        // The shipped adapters are unaffected by the addition.
        assert!(resolve(registry, "claude").is_some());
    }
}
