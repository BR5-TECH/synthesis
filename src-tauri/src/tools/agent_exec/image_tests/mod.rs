//! AVI-FR-01, AVI-FR-05 … CIP-FR-15, AVI-FR-15 — the vendor image definitions
//! (`../infra/AVI-agent-vendor-images.md`).
//!
//! These assert the Dockerfiles, the manifest, and the README as *text*, which
//! is what they are: application-owned definitions compiled into no binary and
//! run by no test. That is deliberate rather than a shortcut — AVI-FR-12 says
//! the suite never builds, pulls, or runs an image, so what is left to check is
//! that the definitions say what the spec requires them to say.
//!
//! **Three scenarios are not automated here and cannot be**, because each needs
//! a built image and a container runtime:
//!
//! - **AVI-FR-16, AVI-FR-02** — the installed package set, and the absence of the other
//!   vendor's CLI and of any Synthesis source. Its Dockerfile half *is*
//!   asserted below: that neither definition installs Git. The other half —
//!   that Git is absent from the built image's path, which a base image could
//!   reintroduce without any directive changing — is `verify.sh`'s.
//! - **AVI-FR-03, AVI-FR-04** — the entrypoint runs as the manifest's UID/GID, not root.
//! - **AVI-FR-16, AVI-FR-13** — the browser starts and renders under the launch
//!   conditions of EAC-FR-13 and EAC-FR-14. Its Dockerfile half — one engine,
//!   pinned, at a path a foreign UID can reach — *is* asserted below.
//! - **AVI-FR-17, AVI-FR-13** — a Rust program compiles and links, `tsc` emits
//!   JavaScript, and `pnpm` runs, under those same launch conditions. Its
//!   Dockerfile half — the pinned versions and the checksummed installer — *is*
//!   asserted below.
//! - **AVI-FR-17, EAC-FR-14, AVI-FR-18** — `$HOME` in a running container is the directory the image
//!   names, and the running user may write it. Its Dockerfile half — the
//!   variables and the directory's mode — *is* asserted below.
//! - **AVI-FR-10** — no credential in any layer, environment variable, or file.
//!
//! They are covered by `docker/agent-images/verify.sh` and a manual run against
//! a published image. AVI-FR-06, AVI-FR-13's version check is likewise `verify.sh`'s; what
//! is asserted below is the half that lives in the Dockerfile.
//!
//! `EAC-FR-14` (a file the agent creates is owned by the host user) is in the
//! same category and is documented at its own test.

mod definitions;
mod manifest;

use super::descriptor;

const CLAUDE_DOCKERFILE: &str =
    include_str!("../../../../../docker/agent-images/claude-code/Dockerfile");
const CODEX_DOCKERFILE: &str = include_str!("../../../../../docker/agent-images/codex/Dockerfile");
const MANIFEST: &str = include_str!("../../../../../docker/agent-images/manifest.toml");
const README: &str = include_str!("../../../../../docker/agent-images/README.md");
const VERIFY_SH: &str = include_str!("../../../../../docker/agent-images/verify.sh");

fn dockerfiles() -> [(&'static str, &'static str); 2] {
    [
        ("claude-code", CLAUDE_DOCKERFILE),
        ("codex", CODEX_DOCKERFILE),
    ]
}

/// Directives only. A Dockerfile's comments legitimately discuss `latest`,
/// `VOLUME`, and credentials while explaining why none of them is used.
fn directives(dockerfile: &str) -> Vec<&str> {
    dockerfile
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Every `KEY=value` an image's `ENV` directives set, in the order the file
/// sets them.
///
/// A multi-line `ENV` is one directive spread over several lines, so the lines
/// of a block are read together: taking them one at a time would see the first
/// assignment of a block and none of the rest.
fn env_assignments(dockerfile: &str) -> Vec<String> {
    let mut assignments: Vec<String> = Vec::new();
    let mut inside = false;
    for directive in directives(dockerfile) {
        let continues = directive.ends_with('\\');
        let body = directive.trim_end_matches('\\').trim();
        let body = if inside {
            body
        } else if let Some(rest) = body.strip_prefix("ENV ") {
            rest
        } else {
            continue;
        };
        assignments.extend(
            body.split_whitespace()
                .filter(|word| word.contains('='))
                .map(str::to_string),
        );
        inside = continues;
    }
    assignments
}

/// One logical `RUN` directive — the line that opens it and every line a
/// backslash joins to it — as one string, chosen by something it contains.
///
/// A `RUN` that spans twenty lines is one shell script, and the order of the
/// commands in it is the whole point of some of them: a checksum that runs
/// after the binary it checks has already run is no checksum at all. Reading
/// the lines separately cannot see that; reading them joined can.
fn run_block(dockerfile: &str, containing: &str) -> String {
    let mut block: Vec<&str> = Vec::new();
    let mut collecting = false;
    for line in directives(dockerfile) {
        if collecting {
            block.push(line);
        } else if line.starts_with("RUN ") {
            block = vec![line];
            collecting = true;
        }
        if collecting && !line.ends_with('\\') {
            let joined = block.join(" ");
            if joined.contains(containing) {
                return joined;
            }
            collecting = false;
        }
    }
    panic!("no RUN directive contains {containing:?}");
}

/// Whether a `chmod` mode gives write permission to anyone but the owner,
/// which is the only question AVI-FR-16 asks of the browser path.
///
/// Read as a mode rather than matched against a list of spellings: `a+w`,
/// `a+rwX`, `go+w`, `+w`, and `0777` all grant it, and a list of the ones
/// someone thought of would miss `a+w,go+rX`.
fn grants_write_to_others(mode: &str) -> bool {
    if let Ok(bits) = u32::from_str_radix(mode, 8) {
        return bits & 0o022 != 0;
    }
    mode.split(',').any(|clause| {
        let Some((who, what)) = clause.split_once(['+', '=']) else {
            return false;
        };
        (who.is_empty() || who.contains(['a', 'g', 'o'])) && what.contains('w')
    })
}

/// Whether a directive grants write permission to every user on some path.
fn grants_write(directive: &str) -> bool {
    let words: Vec<&str> = directive.split_whitespace().collect();
    words.iter().enumerate().any(|(index, word)| {
        *word == "chmod"
            && words[index + 1..]
                .iter()
                .find(|word| !word.starts_with('-'))
                .is_some_and(|mode| grants_write_to_others(mode))
    })
}
