//! AVI-FR-01 … AVI-FR-17 — what the two vendor Dockerfiles must say.

use super::*;

/// AVI-FR-01, AVI-FR-05 — two images, one per executable vendor, each from one base.
#[test]
fn two_images_exist_each_from_one_base() {
    for (vendor, dockerfile) in dockerfiles() {
        let all = directives(dockerfile);
        let from = all.iter().filter(|line| line.starts_with("FROM ")).count();
        assert_eq!(from, 1, "{vendor} must build from exactly one base");
    }
    // The two are distinct definitions, not one file referenced twice.
    assert_ne!(CLAUDE_DOCKERFILE, CODEX_DOCKERFILE);
}

/// AVI-FR-DHCF — both images build from Debian 13 (trixie). Its glibc is new
/// enough for the prebuilt ONNX Runtime that `ort-sys` links. The exact base is
/// the choice of the Dockerfile's author (AVI-FR-QWVT), and the glibc version
/// is a fact of the built image, so `verify.sh` asks it.
#[test]
fn both_images_build_from_a_trixie_base() {
    let bases: Vec<String> = dockerfiles()
        .iter()
        .map(|(vendor, dockerfile)| {
            directives(dockerfile)
                .into_iter()
                .find(|line| line.starts_with("FROM "))
                .unwrap_or_else(|| panic!("{vendor} names no base"))
                .to_string()
        })
        .collect();
    for base in &bases {
        assert!(
            base.contains("trixie"),
            "a base other than the Debian 13 one AVI-FR-DHCF names: {base}"
        );
    }
}

/// EMB-FR-YGXX — no image sets `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`. Either
/// one replaces the target flags of `src-tauri/.cargo/config.toml`, so the
/// `fp16` feature would be lost in the image where the aarch64 Linux build
/// runs. That the flag takes effect is shown only by a real aarch64 Linux build
/// from `src-tauri/`; no suite here compiles that target.
#[test]
fn no_image_overrides_the_cargo_target_flags() {
    for (vendor, dockerfile) in dockerfiles() {
        for assignment in env_assignments(dockerfile) {
            let name = assignment.split('=').next().unwrap_or_default();
            assert!(
                name != "RUSTFLAGS" && name != "CARGO_ENCODED_RUSTFLAGS",
                "{vendor} sets {name}, which replaces the fp16 flag of EMB-FR-YGXX"
            );
        }
    }
}

/// The package names of each `npm install` block, each without its `@version`.
///
/// A scoped name keeps its leading `@`. Only the name is read, so an install
/// passes whatever form of version it names (AVI-FR-QWVT).
fn npm_packages(dockerfile: &str) -> Vec<String> {
    let mut packages = Vec::new();
    let mut inside = false;
    let mut previous = String::new();
    for directive in directives(dockerfile) {
        for word in directive.split_whitespace() {
            let word = normalize(word);
            // A bare line continuation, once its backslash is gone.
            if word.is_empty() {
                continue;
            }
            if word == "install" && previous == "npm" {
                inside = true;
            } else if word == "&&" {
                inside = false;
            } else if inside && !word.starts_with('-') {
                let name = word.rsplit_once('@').filter(|(name, _)| !name.is_empty());
                packages.push(name.map_or(word.as_str(), |(name, _)| name).to_string());
            }
            previous = word;
        }
    }
    packages
}

/// AVI-FR-06, AVI-FR-QWVT — each image installs its own vendor's package and not the
/// other's. The version is the choice of the Dockerfile's author, so this test
/// does not read it.
#[test]
fn each_image_installs_its_own_vendor_cli() {
    assert!(CLAUDE_DOCKERFILE.contains("@anthropic-ai/claude-code"));
    assert!(!CLAUDE_DOCKERFILE.contains("@openai/codex"));
    assert!(CODEX_DOCKERFILE.contains("@openai/codex"));
    assert!(!CODEX_DOCKERFILE.contains("@anthropic-ai/claude-code"));
}

/// One Dockerfile word, stripped of everything that is punctuation around it
/// rather than part of it.
///
/// The brackets and commas are why this is a function rather than a `trim`
/// call: `RUN ["apt-get", "install", "git"]` is Docker's exec form, and its
/// tokens arrive as `["apt-get",` and `"git"]`. Trimming quotes alone leaves
/// the bracket in place, so every comparison below would miss a Git install
/// written that way — in a test whose whole purpose is to see one.
fn normalize(word: &str) -> String {
    word.trim_end_matches('\\').trim_matches(['"', '\'', '[', ']', ',', '(', ')']).to_string()
}

/// The package set an `apt-get install` block names, normalized.
///
/// Everything between the `install` word and the end of the block, with the
/// flags, the line continuations, any quoting, and any `=version` pin removed.
/// The pin matters most: this repository pins versions by convention
/// (AVI-FR-06), so `git=1:2.39.5-0+deb12u2` is the likeliest way Git comes
/// back, and a check that compared whole words would not see it.
fn installed_packages(dockerfile: &str) -> Vec<String> {
    /// The system package managers a base image could carry. `npm` is
    /// deliberately absent: the vendor CLI is installed with it, and that
    /// install is AVI-FR-06's business rather than AVI-FR-02's package floor.
    const MANAGERS: [&str; 5] = ["apt-get", "apt", "apk", "dnf", "yum"];

    let mut packages: Vec<String> = Vec::new();
    let mut inside = false;
    let mut previous = String::new();
    for directive in directives(dockerfile) {
        for word in directive.split_whitespace() {
            let word = normalize(word);
            let word = word.as_str();
            // A bare line continuation, once its backslash is gone. It joins
            // two halves of one block rather than ending it.
            if word.is_empty() {
                continue;
            }
            if (word == "install" || word == "add") && MANAGERS.contains(&previous.as_str()) {
                inside = true;
                previous = word.to_string();
                continue;
            }
            previous = word.to_string();
            if !inside {
                continue;
            }
            // `&&` ends the install block; the next command is not part of it.
            if word == "&&" {
                inside = false;
                continue;
            }
            if word.starts_with('-') {
                continue;
            }
            packages.push(word.split('=').next().unwrap_or(word).to_string());
        }
    }
    packages.sort();
    packages
}

/// AVI-FR-16 — each image installs exactly the packages AVI-FR-02 names —
/// including Python with `pip` and `venv`, which repository work asks for
/// directly, `build-essential` for the linker the Rust toolchain of AVI-FR-17
/// calls, `pkg-config` with the GTK, WebKit, and OpenSSL headers that
/// toolchain's `-sys` build scripts ask it for, and the shared libraries and
/// font the browser of AVI-FR-16 links against — and Git is not among them.
///
/// The browser's libraries are named one by one rather than left to
/// `playwright install-deps`, which brings a display server and the fonts of
/// every writing system with it — 109 packages into an image that runs one
/// headless browser. Listing them is what lets this assertion see the floor at
/// all: a command that installs an unnamed set is a floor no test can read.
///
/// Both halves in one assertion, because they fail together. A container
/// reaches its own execution directory, the repository that
/// directory belongs to, and nothing else (EAC-FR-13, EAC-FR-ZKMR, EAC-FR-FNFV) and a run's working copy
/// keeps its Git metadata outside that directory, so a Git command in a
/// container could never read the repository it appears to stand in — shipping
/// the binary only lets an agent spend turns on a command that cannot work and
/// report the failure as though it were a fact about the work. The positive
/// half is what stops the removal from taking a line too many with it.
#[test]
fn each_image_installs_exactly_the_packages_avi_names() {
    for (vendor, dockerfile) in dockerfiles() {
        assert_eq!(
            installed_packages(dockerfile),
            [
                "build-essential",
                "ca-certificates",
                "curl",
                "fonts-liberation",
                "git",
                "libasound2t64",
                "libatk-bridge2.0-0t64",
                "libatk1.0-0t64",
                "libatspi2.0-0t64",
                "libcairo2",
                "libcups2t64",
                "libdbus-1-3",
                "libdrm2",
                "libgbm1",
                "libglib2.0-0t64",
                "libnspr4",
                "libnss3",
                "libpango-1.0-0",
                "libssl-dev",
                "libwebkit2gtk-4.1-dev",
                "libx11-6",
                "libxcb1",
                "libxcomposite1",
                "libxdamage1",
                "libxext6",
                "libxfixes3",
                "libxkbcommon0",
                "libxrandr2",
                "pkg-config",
                "python3",
                "python3-pip",
                "python3-venv"
            ],
            "{vendor} does not install exactly what AVI-FR-02 names"
        );
    }

    // AVI-FR-02: Git arrives by the package floor and by no other route — not
    // copied in from another stage, not fetched, not built. The floor is where
    // the version is the pinned base's, which is the reproducibility argument
    // every other package in that list rests on.
    for (vendor, dockerfile) in dockerfiles() {
        // A multi-line `RUN` is one directive spread over several lines, so the
        // package floor is read as the block it is: a line inside the install
        // list is where Git belongs, and every line outside one is where it does
        // not.
        let mut inside_install = false;
        for directive in directives(dockerfile) {
            if directive.contains("apt-get install") {
                inside_install = true;
            }
            let continues = directive.ends_with('\\');
            let was_inside = inside_install;
            if !continues {
                inside_install = false;
            }
            if was_inside {
                continue;
            }
            for word in directive.split_whitespace() {
                let word = word
                    .trim_end_matches('\\')
                    .trim_matches(['"', '\''])
                    .rsplit('/')
                    .next()
                    .unwrap_or_default();
                let name = word.split('=').next().unwrap_or(word);
                assert!(
                    name != "git" && !name.starts_with("git-"),
                    "{vendor} reaches Git outside the package floor: {directive}"
                );
            }
        }
    }
}

/// AVI-FR-13, AVI-FR-QWVT — the browser of AVI-FR-16: one engine, installed where a
/// foreign UID can reach it. The version is the choice of the Dockerfile's
/// author, so this test does not read it.
///
/// What this cannot assert is that the browser *starts*. That needs a built
/// image and a container runtime, under the launch conditions of EAC-FR-13 and
/// EAC-FR-14 — every capability dropped, no new privileges, and a UID with no
/// account in the image — and it is `verify.sh`'s.
#[test]
fn each_image_carries_one_browser_a_foreign_uid_can_reach() {
    for (vendor, dockerfile) in dockerfiles() {
        assert!(
            npm_packages(dockerfile).iter().any(|p| p == "playwright"),
            "{vendor} does not install the browser runtime"
        );

        // One engine. Each of the others is several hundred megabytes for a
        // capability no check of a view needs.
        assert!(
            dockerfile.contains("playwright install chromium"),
            "{vendor} does not install Chromium"
        );
        for other in ["install firefox", "install webkit", "install --with-deps"] {
            assert!(
                !dockerfile.contains(other),
                "{vendor} installs more than the one engine AVI-FR-16 states: {other}"
            );
        }

        // Reachable by whatever UID the launch supplies: a fixed path outside
        // any home directory, made readable and executable for everyone. The
        // executor runs a container as the host user's UID (EAC-FR-14), which
        // has no account in the image, and the layer that installs the browser
        // runs as root — so a browser left in an installing user's own cache
        // would be invisible to the process that needs it.
        assert!(
            dockerfile.contains("ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright"),
            "{vendor} does not name the browser path, so a foreign UID cannot find it"
        );
        assert!(
            dockerfile.contains("chmod -R a+rX /opt/ms-playwright"),
            "{vendor} does not make the browser readable to the UID a launch supplies"
        );
        // And not writable by it: the browser is part of the image.
        //
        // Scoped to the directives that name the browser path rather than
        // searched for across the file, because other paths in this image are
        // deliberately writable by whatever UID a launch supplies — Cargo's
        // caches (AVI-FR-17) and the home directory (AVI-FR-18) — and a
        // whole-file search cannot tell one `chmod` from another.
        for directive in directives(dockerfile)
            .into_iter()
            .filter(|directive| directive.contains("/opt/ms-playwright"))
        {
            assert!(
                !grants_write(directive),
                "{vendor} makes the browser path writable, which AVI-FR-16 does not: {directive}"
            );
            assert!(
                !directive.contains("chown 10001:10001 /opt"),
                "{vendor} gives the browser path away to a user, which AVI-FR-16 does not: {directive}"
            );
        }
    }
}

/// AVI-FR-13, AVI-FR-QWVT — the toolchains of AVI-FR-17: each installed, and the one
/// installer that arrives over the network checked against a recorded digest
/// before it runs. The versions are the choice of the Dockerfile's author, so
/// this test does not read them.
///
/// What this cannot assert is that any of them *builds*. A linker that is
/// missing, and a cache directory the launching UID does not own, both leave a
/// Dockerfile that reads correctly and a toolchain that fails on its first
/// use — so the compile itself is `verify.sh`'s, under the launch conditions
/// of EAC-FR-13 and EAC-FR-14.
#[test]
fn each_image_installs_the_toolchains() {
    for (vendor, dockerfile) in dockerfiles() {
        // Each is installed. The npm packages are read as names of an install
        // block, because the bare word `pnpm` also stands in `PNPM_HOME`.
        let packages = npm_packages(dockerfile);
        for package in ["pnpm", "typescript"] {
            assert!(packages.iter().any(|p| p == package), "{vendor} does not install {package}");
        }
        assert!(
            dockerfile.contains("--default-toolchain"),
            "{vendor} does not install a Rust toolchain"
        );

        // The Rust components the work uses, and no more than those: the
        // toolchain is installed at its minimal profile.
        assert!(
            dockerfile.contains("--profile minimal"),
            "{vendor} installs more of the Rust toolchain than the work uses"
        );
        for component in ["--component rustfmt", "--component clippy"] {
            assert!(dockerfile.contains(component), "{vendor} does not install {component}");
        }

        // The installer is the one part of an image that arrives over the
        // network without a digest of its own. Read as one block, because
        // every claim below is about the order of a shell script rather than
        // about the presence of a line in a file.
        let rustup = run_block(dockerfile, "rustup-init");
        assert!(
            rustup.starts_with("RUN set -eux;"),
            "{vendor} runs the Rust installer in a shell that does not stop on a failed command, so a failed checksum would not stop the build"
        );

        // From the release archive over TLS — not from a mirror, and not over
        // a scheme a checksum cannot save.
        assert!(
            rustup.contains("https://static.rust-lang.org/rustup/archive/"),
            "{vendor} fetches the Rust installer from somewhere other than the release archive"
        );

        // And checked *before* it runs. Every other assertion here is a
        // substring, which a reordering would satisfy while verifying nothing.
        let checked = rustup.find("sha256sum --check --strict").unwrap_or_else(|| {
            panic!("{vendor} runs the Rust installer without checking it against a digest")
        });
        let executed = rustup
            .find("/tmp/rustup-init -y")
            .unwrap_or_else(|| panic!("{vendor} does not run the Rust installer it downloaded"));
        assert!(
            checked < executed,
            "{vendor} runs the Rust installer before checking it, which checks nothing"
        );

        // One checksum per architecture, each a real digest, and no two
        // architectures sharing one.
        let checksums: Vec<&str> =
            rustup.split("sha256='").skip(1).filter_map(|rest| rest.split('\'').next()).collect();
        assert_eq!(
            checksums.len(),
            2,
            "{vendor} records {} Rust installer checksums, not one per architecture",
            checksums.len()
        );
        for checksum in &checksums {
            assert!(
                checksum.len() == 64
                    && checksum.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "{vendor} records something other than a SHA-256: {checksum}"
            );
        }
        assert_ne!(
            checksums[0], checksums[1],
            "{vendor} records one checksum for two architectures, so one of them is unverified"
        );

        // An architecture with no checksum stops the build rather than
        // falling back to an unverified download.
        assert!(
            rustup.contains("no rustup-init checksum recorded for architecture"),
            "{vendor} does not say why an unknown architecture is refused"
        );
        assert!(
            rustup.contains("exit 1"),
            "{vendor} does not stop on an architecture it has no checksum for"
        );

        // Cargo's directories are writable by whatever UID a launch supplies:
        // Cargo writes its registry index and its package cache on every
        // build, and rustup writes a toolchain there when a repository asks
        // for a version the image does not carry.
        //
        // Asserted as the outcome rather than as a literal command, so a later
        // edit may change the form without failing a test that is about
        // whether the directories can be written.
        assert!(
            grants_write(&rustup)
                && rustup.contains("${RUSTUP_HOME}")
                && rustup.contains("${CARGO_HOME}"),
            "{vendor} leaves the Rust directories unwritable by the UID a launch supplies"
        );
    }
}

/// AVI-FR-17, EAC-FR-14 — the home directory of AVI-FR-18, which the image names rather
/// than leaving to the container runtime.
///
/// A runtime derives a home from the passwd entry of the user it starts, and
/// the UID the executor supplies (EAC-FR-14) has no entry in either image — so
/// an unnamed `$HOME` is `/`, which nothing may write, and the vendor CLI's
/// scratch state, npm's cache, and pnpm's store all fail there.
///
/// What this cannot assert is `$HOME`'s value inside a running container, which
/// needs a runtime and is `verify.sh`'s half of the scenario.
#[test]
fn each_image_names_a_writable_home_for_the_uid_a_launch_supplies() {
    for (vendor, dockerfile) in dockerfiles() {
        let envs = env_assignments(dockerfile);
        for named in ["HOME=/home/agent", "PNPM_HOME=/home/agent/.local/share/pnpm"] {
            assert!(
                envs.iter().any(|assignment| assignment == named),
                "{vendor} does not name {named}, so a foreign UID gets `/` as its home"
            );
        }

        // Cargo's binaries and pnpm's are both in reach without a path in
        // front of them; pnpm refuses a global install when its own bin
        // directory is not on `PATH`.
        let path = envs
            .iter()
            .find(|assignment| assignment.starts_with("PATH="))
            .unwrap_or_else(|| panic!("{vendor} sets no PATH"));
        for directory in ["/usr/local/cargo/bin", "/home/agent/.local/share/pnpm/bin"] {
            assert!(path.contains(directory), "{vendor} leaves {directory} off PATH: {path}");
        }

        // And the home is writable by whatever UID the launch supplies. A
        // container belongs to one agent alone, so this crosses no boundary
        // the launch has not already decided. Asserted as the outcome rather
        // than as a literal command, for the reason the Cargo directories are.
        let home = run_block(dockerfile, "/home/agent");
        assert!(
            grants_write(&home),
            "{vendor} leaves the home directory unwritable by the UID a launch supplies: {home}"
        );
    }
}
