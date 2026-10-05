//! AVI-FR-05 … AVI-FR-15 — the manifest, the README, the verify command, and image addressing.

use super::*;

/// AVI-FR-07 — the manifest is the only place the repository names an image
/// reference, a digest, or a CLI version.
#[test]
fn no_image_identity_is_named_outside_the_manifest() {
    for (name, source) in [
        ("agent_exec.rs", include_str!("../../agent_exec.rs")),
        ("descriptor.rs", include_str!("../descriptor.rs")),
        ("protocol.rs", include_str!("../protocol.rs")),
        ("runtime.rs", include_str!("../runtime.rs")),
    ] {
        for identity in ["ghcr.io/", "sha256:"] {
            // `descriptor.rs` names the unpublished sentinel by construction —
            // it is the constant AVI-FR-08 defines — and nothing else may.
            // (`synthesis-agent-` is deliberately not on this list: it is also
            // the container-name prefix, which is a different thing that
            // happens to share a token.)
            let allowed = name == "descriptor.rs" && identity == "sha256:";
            assert!(
                allowed || !source.contains(identity),
                "{name} names {identity}, which belongs in manifest.toml alone"
            );
        }
    }
    // The version strings are the manifest's too, and appear in no source file.
    for vendor in ["claude_code", "codex"] {
        let version = &descriptor::manifest_entry(vendor).expect("entry").cli_version;
        for (name, source) in [
            ("agent_exec.rs", include_str!("../../agent_exec.rs")),
            ("descriptor.rs", include_str!("../descriptor.rs")),
            ("runtime.rs", include_str!("../runtime.rs")),
        ] {
            assert!(
                !source.contains(version.as_str()),
                "{name} hardcodes {vendor}'s CLI version"
            );
        }
    }

    // And the manifest itself is where they live.
    assert!(MANIFEST.contains("image_ref"));
    assert!(MANIFEST.contains("image_digest"));
    assert!(MANIFEST.contains("cli_version"));

    // AVI-FR-07 / EAC-FR-10, EAC-FR-38: the vendor execution descriptors carry neither an
    // image reference nor a digest. The image a container is created from is
    // the one the open project commits (EAC-FR-38), so a launch reads the
    // shipped manifest not at all — and the machinery that reads it is
    // compiled under `cfg(test)` alone, which is what makes that a property of
    // the shipped binary rather than a convention.
    const DESCRIPTOR: &str = include_str!("../descriptor.rs");
    let struct_body = DESCRIPTOR
        .split_once("pub struct VendorExecutionDescriptor {")
        .expect("the descriptor struct")
        .1
        .split_once("\n}")
        .expect("the descriptor struct ends")
        .0;
    for field in ["image_ref", "image_digest"] {
        assert!(
            !struct_body.contains(field),
            "a vendor execution descriptor carries no {field}"
        );
    }
    for gated in [
        "#[cfg(test)]\nconst MANIFEST_TOML",
        "#[cfg(test)]\npub fn manifest_entry",
        "#[cfg(test)]\nfn manifest()",
    ] {
        assert!(
            DESCRIPTOR.contains(gated),
            "the shipped manifest is read under cfg(test) alone: {gated}"
        );
    }
}

/// AVI-FR-19, AVI-FR-07, AVI-FR-09 — a run reads the shipped manifest not at all, and writes it
/// never.
///
/// The image a container is created from is the project's own (AVI-FR-09,
/// EAC-FR-38), and a project image build writes neither a reference nor a digest
/// into the shipped manifest (PSS-FR-26). Asserted of the source rather than of
/// a run, because "nothing wrote this file" is a property of every path through
/// the code rather than of the one a test happened to take.
#[test]
fn nothing_in_the_application_writes_the_shipped_manifest() {
    for (name, source) in [
        ("agent_exec.rs", include_str!("../../agent_exec.rs")),
        ("runtime.rs", include_str!("../runtime.rs")),
        ("images.rs", include_str!("../../../project_settings/images.rs")),
        (
            "image_builder.rs",
            include_str!("../../../project_settings/image_builder.rs"),
        ),
    ] {
        assert!(
            !source.contains("manifest.toml"),
            "{name} names the shipped manifest at all"
        );
    }
    // And no path anywhere in the build or the launch reaches a registry: the
    // build surface is local-only (PSS-FR-26), so the tokens that would take it
    // to one appear in none of them.
    for (name, source) in [
        ("agent_exec.rs", include_str!("../../agent_exec.rs")),
        ("descriptor.rs", include_str!("../descriptor.rs")),
        ("runtime.rs", include_str!("../runtime.rs")),
        ("images.rs", include_str!("../../../project_settings/images.rs")),
        (
            "image_builder.rs",
            include_str!("../../../project_settings/image_builder.rs"),
        ),
    ] {
        // The production half alone: a module's own suite names the very
        // tokens it is asserting the absence of.
        let production = source
            .split_once("\n#[cfg(test)]\nmod tests {")
            .map_or(source, |(before, _)| before);
        let code: String = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| !line.trim_start().starts_with("///"))
            .filter(|line| !line.trim_start().starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["--push", "\"push\"", "\"login\""] {
            assert!(
                !code.contains(forbidden),
                "{name} names {forbidden}, and the build surface is local-only"
            );
        }
    }
    // The one mention of the shipped manifest is `descriptor.rs`'s read, which
    // is compiled under `cfg(test)` and is what the checks standing in for
    // `verify` use.
    let descriptor = include_str!("../descriptor.rs");
    assert_eq!(
        descriptor
            .matches("include_str!(\"../../../../docker/agent-images/manifest.toml\")")
            .count(),
        1,
        "the manifest is read once, by the check that stands in for verify"
    );
}

/// AVI-FR-11 — the working directory is `/workspace`, and nothing about what a
/// container can reach is decided by the image.
#[test]
fn each_image_declares_a_workspace_and_no_volume_or_port() {
    for (vendor, dockerfile) in dockerfiles() {
        let directives = directives(dockerfile);
        assert!(
            directives.iter().any(|d| *d == "WORKDIR /workspace"),
            "{vendor} does not set /workspace as its working directory"
        );
        for forbidden in ["VOLUME", "EXPOSE"] {
            assert!(
                !directives.iter().any(|d| d.starts_with(forbidden)),
                "{vendor} declares {forbidden}, which the invocation must decide"
            );
        }
        // AVI-FR-03/04: a non-root user at the manifest's ids, and the vendor
        // CLI as the entrypoint.
        assert!(
            directives.iter().any(|d| d.starts_with("USER 10001:10001")),
            "{vendor} does not run as the manifest's non-root user"
        );
        assert!(
            directives.iter().any(|d| d.starts_with("ENTRYPOINT")),
            "{vendor} declares no entrypoint"
        );
    }
    assert!(CLAUDE_DOCKERFILE.contains(r#"ENTRYPOINT ["claude"]"#));
    assert!(CODEX_DOCKERFILE.contains(r#"ENTRYPOINT ["codex"]"#));

    // The UID/GID in the Dockerfiles is the one the manifest records, and the
    // one the executor maps the host user onto (EAC-FR-14).
    for vendor in ["claude_code", "codex"] {
        let entry = descriptor::manifest_entry(vendor).expect("entry");
        assert_eq!((entry.uid, entry.gid), (10001, 10001));
    }
}

/// AVI-FR-10 — no credential is built into an image. The Dockerfiles are where
/// that would happen; AVI-FR-10 checks the built layers and needs a runtime.
#[test]
fn no_dockerfile_bakes_in_a_credential() {
    for (vendor, dockerfile) in dockerfiles() {
        for directive in directives(dockerfile) {
            for smell in [
                "sk-ant-",
                "OAUTH",
                "API_KEY",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "--mount=type=secret",
                "COPY .codex",
            ] {
                assert!(
                    !directive.contains(smell),
                    "{vendor} references {smell} in a build directive: {directive}"
                );
            }
        }
        // AVI-FR-16, AVI-FR-17, AVI-FR-18: an image's environment holds
        // nothing a credential could hide in. The rule is not "no environment
        // variable" — it is that every variable an image sets is one AVI names
        // and holds a container path, so a variable outside that set fails
        // here rather than arriving unread.
        //
        // The *values* are each owned by the requirement that introduces them
        // and asserted at that requirement's own test; what this owns is that
        // there is nothing else, in whatever order the file sets them.
        const DEFINED: [&str; 6] = [
            "CARGO_HOME",
            "HOME",
            "PATH",
            "PLAYWRIGHT_BROWSERS_PATH",
            "PNPM_HOME",
            "RUSTUP_HOME",
        ];
        let mut names: Vec<String> = env_assignments(dockerfile)
            .iter()
            .map(|assignment| {
                let (name, value) = assignment
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{vendor} sets an ENV with no value: {assignment}"));
                assert!(
                    value.starts_with('/'),
                    "{vendor} sets {name} to something other than a container path: {value}"
                );
                name.to_string()
            })
            .collect();
        names.sort();
        assert_eq!(
            names, DEFINED,
            "{vendor} sets an environment variable AVI does not define, or omits one it does"
        );
    }
}

/// AVI-FR-13 — the verify command exists, refuses the unpublished sentinel, and
/// checks the two things the manifest claims about a published image.
#[test]
fn the_verify_command_refuses_the_sentinel_and_checks_what_the_manifest_claims() {
    assert!(VERIFY_SH.starts_with("#!/usr/bin/env bash"));
    assert!(VERIFY_SH.contains("set -euo pipefail"));

    // It fails on the sentinel by design rather than treating it as verified.
    assert!(VERIFY_SH.contains(descriptor::UNPUBLISHED_DIGEST));
    assert!(VERIFY_SH.contains("unpublished sentinel"));

    // It addresses by digest, not by tag — the substitution the digest exists
    // to prevent.
    assert!(VERIFY_SH.contains("${ref%%:*}@${digest}"));
    // And checks both claims AVI-FR-13 names.
    assert!(VERIFY_SH.contains("cli_version"));
    assert!(VERIFY_SH.contains("id -u"));

    // CCP-FR-25, CCP-FR-26: it also probes the one thing about the pinned CLI that no
    // amount of reading a Dockerfile can establish — that the CLI still refuses
    // the streaming output format under `--print` without `--verbose`, which is
    // why the executor generates the two together. A version that stopped
    // requiring the flag, or started requiring another, would take a whole run
    // out before it began, and the only place that is discoverable is against
    // the image itself.
    assert!(VERIFY_SH.contains("--output-format stream-json"));
    assert!(VERIFY_SH.contains("--verbose"));
    assert!(VERIFY_SH.contains("CCP-FR-26"));

    // The Python floor of AVI-FR-02, probed the way a turn would use it:
    // a virtual environment built in the container and an install into it.
    // Reading a Dockerfile cannot establish that `ensurepip` is present or
    // that the agent user may write where a venv goes.
    assert!(VERIFY_SH.contains("python3 -m venv"));
    assert!(VERIFY_SH.contains("pip --version"));

    // AVI-FR-16, AVI-FR-13's other half, and the whole reason the browser check lives in
    // a script rather than here: that the browser *starts*, under the
    // conditions a real launch imposes — every capability dropped, no new
    // privileges (EAC-FR-13), and a UID with no account in the image
    // (EAC-FR-14). Each of those stops browsers that run perfectly well as the
    // image's own user, and none of them is visible in a Dockerfile.
    assert!(VERIFY_SH.contains("playwright screenshot"));
    assert!(VERIFY_SH.contains("--cap-drop ALL"));
    assert!(VERIFY_SH.contains("--security-opt no-new-privileges"));
    assert!(VERIFY_SH.contains(r#"--user "$foreign_uid""#));
    assert!(VERIFY_SH.contains("AVI-FR-16"));

    // AVI-FR-13's and EAC-FR-14, AVI-FR-18's other halves: the toolchains of AVI-FR-17
    // build something, and the home of AVI-FR-18 is where the image says and
    // writable, under those same launch conditions. A `--version` proves a
    // binary is on the path; it does not prove that the linker `rustc` calls
    // is installed, or that the directories a build writes to belong to the
    // user a launch supplies — and none of that is visible in a Dockerfile.
    //
    // Asserted as what the script must reach rather than as the commands it
    // reaches them with, so the probe can be strengthened without failing the
    // test that asks for it.
    for named in ["AVI-FR-17", "AVI-FR-18"] {
        assert!(VERIFY_SH.contains(named));
    }
    for built in ["cargo build", "cargo clippy", "tsc ", "pnpm install"] {
        assert!(
            VERIFY_SH.contains(built),
            "verify.sh does not build with the toolchain of AVI-FR-17: {built}"
        );
    }
    for writable in ["$HOME", "$CARGO_HOME", "$RUSTUP_HOME"] {
        assert!(
            VERIFY_SH.contains(writable),
            "verify.sh does not probe {writable}, so a directory the launching UID cannot write would pass"
        );
    }

    // And as a UID with no account in the image, which is the condition every
    // one of those scenarios names. Debian ships an account for 65534, so a
    // probe running as `nobody` is handed a home from `/etc/passwd` and never
    // sees what an accountless UID sees.
    assert!(
        !VERIFY_SH.contains("--user 65534"),
        "verify.sh probes as a UID the image has an account for, which is not the case EAC-FR-14 creates"
    );

    // AVI-FR-16, AVI-FR-02's other half that only a built image can answer: Git absent
    // from the path. A base image that started shipping it would put it back
    // without any directive in either Dockerfile changing, so reading the
    // definitions cannot establish this and the script has to.
    assert!(VERIFY_SH.contains("git --version"));
    assert!(VERIFY_SH.contains("AVI-FR-02"));
    // AVI-FR-13's other half that only a built image can answer: the native
    // floor of AVI-FR-02, asked of pkg-config rather than of a package list,
    // because what a `-sys` build script needs is an answer.
    assert!(VERIFY_SH.contains("pkg-config --exists"));
    for module in ["glib-2.0", "gtk+-3.0", "webkit2gtk-4.1", "openssl"] {
        assert!(
            VERIFY_SH.contains(module),
            "verify.sh does not ask pkg-config for {module}"
        );
    }
    // Deliberately not asserted here: the file's executable bit. Reading it
    // needs a direct `std::fs` call, and `FSA-FR-19`'s guard rightly refuses
    // one — an exception list meant for production paths outside every
    // allowlistable root should not be widened for a test's convenience. The
    // bit is set in the checkout and the README documents running the script.
}

/// AVI-FR-09 — the README documents what AVI-FR-14 requires of it.
#[test]
fn the_readme_documents_everything_avi_requires() {
    for (what, needle) in [
        ("the two definitions", "claude-code/Dockerfile"),
        ("the codex definition", "codex/Dockerfile"),
        ("the pinned base", "node:22-bookworm-slim"),
        ("the build command", "docker build"),
        ("the publish command", "docker push"),
        ("the verify command", "verify.sh"),
        ("the manifest's fields", "image_digest"),
        ("who reads the manifest", "vendor execution descriptors"),
        ("the non-root user contract", "10001/10001"),
        ("the no-credential rule", "No credential is ever built in"),
        ("the unpublished sentinel", "unpublished sentinel"),
        ("why there is no CI lane", "AVI-FR-15"),
        ("that Git is absent by decision", "no Git"),
        ("the browser", "PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright"),
        ("how a task reaches the browser", "playwright screenshot"),
        ("the Rust toolchain", "ARG RUST_VERSION"),
        ("the Rust installer's pin", "ARG RUSTUP_VERSION"),
        ("the pnpm pin", "ARG PNPM_VERSION"),
        ("the TypeScript pin", "ARG TYPESCRIPT_VERSION"),
        ("the rule for upgrading a toolchain", "both Dockerfiles"),
        ("the home directory the image names", "HOME=/home/agent"),
    ] {
        assert!(
            README.contains(needle),
            "docker/agent-images/README.md does not document {what} (looked for {needle:?})"
        );
    }
}

/// AVI-FR-08, AVI-FR-13 — a **published** image is addressed by digest, so a tag
/// republished against different content is not adopted; an unpublished one is
/// addressed by its tag, so an image built locally runs.
#[test]
fn a_published_image_is_addressed_by_digest_and_an_unpublished_one_by_its_tag() {
    for vendor in ["claude_code", "codex"] {
        let entry = descriptor::manifest_entry(vendor).expect("entry");

        // The published half. A stand-in, because the shipped manifest carries
        // the sentinel — asserting against the real entry would make this test
        // a claim about whether the images happen to be published today.
        let published = descriptor::ImageManifestEntry {
            image_digest: format!("sha256:{}", "ab".repeat(32)),
            ..entry.clone()
        };
        let pinned = published.launch_reference();
        assert!(pinned.contains('@'));
        assert!(pinned.ends_with(&published.image_digest));
        // The tag is gone from the address entirely, which is what makes a
        // republished tag unreachable rather than merely unpreferred.
        assert!(!pinned.contains(&format!(":{}", published.cli_version)));

        // The same repository under a moved tag resolves to the same pinned
        // address, because the tag contributes nothing to it.
        let moved = descriptor::ImageManifestEntry {
            image_ref: format!(
                "{}:99.99.99",
                published.image_ref.rsplit_once(':').unwrap().0
            ),
            ..published.clone()
        };
        assert_eq!(moved.launch_reference(), pinned);

        // The unpublished half: the tag, whole, and no digest anywhere in it.
        // A repository digest is issued by a registry, so a locally built image
        // has none — addressing it by one would ask Docker to resolve something
        // that cannot exist.
        let unpublished = descriptor::ImageManifestEntry {
            image_digest: descriptor::UNPUBLISHED_DIGEST.to_string(),
            ..entry.clone()
        };
        assert_eq!(unpublished.launch_reference(), unpublished.image_ref);
        assert!(!unpublished.launch_reference().contains('@'));
        // AVI-FR-08: and the tag it launches by names the CLI version with a
        // build revision after it. The rest of an image changes while its
        // vendor CLI does not — a package added to the floor, a toolchain
        // raised — and publishing that over an unchanged tag is the
        // substitution the digest exists to prevent, which this addressing
        // route has no digest to prevent it with.
        let launched = unpublished.launch_reference();
        let tag = launched.rsplit_once(':').expect("a tag").1;
        let revision = tag
            .strip_prefix(&format!("{}-", unpublished.cli_version))
            .unwrap_or_else(|| {
                panic!("{vendor}'s tag {tag:?} does not name its CLI version and a build revision")
            });
        assert!(
            !revision.is_empty() && revision.chars().all(|c| c.is_ascii_digit()),
            "{vendor}'s build revision is a number, not {revision:?}"
        );
    }
}

/// AVI-FR-07 — the manifest parses, and a malformed one is caught by the suite
/// rather than at a first launch in front of a user.
#[test]
fn the_manifest_parses_and_holds_an_entry_per_executable_vendor() {
    for vendor in ["claude_code", "codex"] {
        let entry = descriptor::manifest_entry(vendor).expect("an entry per vendor");
        assert!(!entry.image_ref.is_empty());
        assert!(entry.image_digest.starts_with("sha256:"));
        assert_eq!(
            entry.image_digest.len(),
            "sha256:".len() + 64,
            "a digest is 64 hex characters"
        );
        assert!(!entry.cli_version.is_empty());
    }
    // No entry for a vendor the executor cannot run.
    for vendor in ["opencode", "claude_agent_api", "custom_agent_api"] {
        assert!(descriptor::manifest_entry(vendor).is_none());
    }
}
