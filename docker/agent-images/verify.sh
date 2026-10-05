#!/usr/bin/env bash
# AVI-FR-13: confirm each manifest entry still describes what was published.
#
# Resolves every image by the digest `manifest.toml` records — not by its tag,
# which is exactly the substitution the digest exists to prevent — and checks
# that the image at that digest reports the manifest's `cli_version`, runs as
# the manifest's UID and GID, carries the Python floor AVI-FR-02 states, answers
# for the native libraries that floor also states, builds
# with the toolchains AVI-FR-17 states at the versions their Dockerfile pins,
# gives the launching user the writable home AVI-FR-18 states, and starts the
# browser AVI-FR-16 states — the last three under the launch conditions the
# executor imposes. A manifest that has drifted
# from what is in the registry fails here rather than failing a launch.
#
# Fails on the unpublished sentinel by design: an image that has never been
# pushed has no digest, and treating the placeholder as verified would make
# "verified" mean nothing.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
manifest="$here/manifest.toml"
sentinel="sha256:0000000000000000000000000000000000000000000000000000000000000000"

field() {
    # Read `key = "value"` (or a bare integer) from within a `[section]`.
    python3 - "$manifest" "$1" "$2" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
section, key = sys.argv[2], sys.argv[3]
body = re.split(r"^\[", text, flags=re.M)
for chunk in body:
    if chunk.startswith(section + "]"):
        m = re.search(rf'^{re.escape(key)}\s*=\s*"?([^"\n]+)"?\s*$', chunk, re.M)
        if m:
            print(m.group(1).strip())
            sys.exit(0)
sys.exit(f"{section}.{key} missing from the manifest")
PY
}

build_arg() {
    # Read the default of an `ARG name=value` from a vendor's Dockerfile. The
    # Dockerfile is the pin (AVI-FR-06, AVI-FR-17); an `ARG` can be overridden
    # at build time, so what a published image actually carries is a question
    # only the image can answer.
    local dockerfile="$here/${1//_/-}/Dockerfile"
    sed -n "s/^ARG $2=//p" "$dockerfile" | head -1
}

# The UID every probe below launches as.
#
# Not `nobody`. The executor runs a container as the *host* user's UID
# (EAC-FR-14), which has no account in the image — and Debian ships an account
# for 65534, so a probe running as that UID would be given a home from
# `/etc/passwd` and would never see what an accountless UID sees. 4242 is
# allocated to nothing in this image, which is the condition AVI-FR-16, AVI-FR-13,
# AVI-FR-17, AVI-FR-13, and EAC-FR-14, AVI-FR-18 all name.
foreign_uid=4242:4242

status=0

for vendor in claude_code codex; do
    ref="$(field "$vendor" image_ref)"
    digest="$(field "$vendor" image_digest)"
    version="$(field "$vendor" cli_version)"
    uid="$(field "$vendor" uid)"
    gid="$(field "$vendor" gid)"

    if [[ "$digest" == "$sentinel" ]]; then
        echo "FAIL $vendor: image_digest is the unpublished sentinel — build and push the image, then record the digest it reported" >&2
        status=1
        continue
    fi

    # The repository without its tag, addressed by digest: a tag that has moved
    # since publication cannot satisfy this.
    pinned="${ref%%:*}@${digest}"

    if ! docker image inspect "$pinned" >/dev/null 2>&1; then
        if ! docker pull --quiet "$pinned" >/dev/null 2>&1; then
            echo "FAIL $vendor: $pinned is not in the registry or not pullable" >&2
            status=1
            continue
        fi
    fi

    reported="$(docker run --rm --entrypoint sh "$pinned" -c '
        if command -v claude >/dev/null 2>&1; then claude --version
        else codex --version
        fi' 2>/dev/null | tr -d "\r" | grep -oE "[0-9]+\.[0-9]+\.[0-9]+" | head -1 || true)"

    if [[ "$reported" != "$version" ]]; then
        echo "FAIL $vendor: manifest says cli_version $version, image reports '${reported:-nothing}'" >&2
        status=1
    fi

    # CCP-FR-26 / CCP-FR-25: the pinned Claude Code CLI refuses the streaming
    # output format under `--print` unless `--verbose` is present. The executor
    # generates both together on every turn, and a version that stopped
    # requiring the flag — or started requiring another — would take the whole
    # run out before it began, with an exit status and nothing else to read.
    # Verified here rather than in the suite, because it needs the image.
    if [[ "$vendor" == "claude_code" ]]; then
        refusal="$(docker run --rm --entrypoint claude "$pinned" \
            -p --output-format stream-json 2>&1 </dev/null || true)"
        if [[ "$refusal" != *"--verbose"* ]]; then
            echo "FAIL $vendor: the CLI no longer names --verbose when refusing stream-json under --print; CCP-FR-26 describes a requirement this image does not have" >&2
            status=1
        fi
    fi

    # AVI-FR-02 / AVI-FR-16: Git is part of the floor. A turn that revises or
    # judges a change set reads what changed rather than every file whole, and
    # the executor is what decides whether that turn can reach a repository at
    # all (EAC-FR-41, EAC-FR-FNFV). Checked against the built image rather than
    # the Dockerfile alone, because the client has to answer, not merely be
    # named in a directive.
    if ! docker run --rm --entrypoint sh "$pinned" -c 'git --version' >/dev/null 2>&1; then
        echo "FAIL $vendor: git is not on the image's path or does not run; AVI-FR-02 makes it part of the floor" >&2
        status=1
    fi

    # AVI-FR-02 / AVI-FR-16: Python is part of the package floor, and pip and
    # venv are part of what makes it usable. Checked as the agent user actually
    # uses it — build a virtual environment in a writable directory and install
    # into it — rather than by asking whether a binary is on the path, because
    # an interpreter without `ensurepip` passes the second test and fails the
    # first one the moment a turn needs it.
    if ! docker run --rm --entrypoint sh "$pinned" -c '
        python3 --version >/dev/null 2>&1 \
        && python3 -m venv /tmp/verify-venv >/dev/null 2>&1 \
        && /tmp/verify-venv/bin/pip --version >/dev/null 2>&1' >/dev/null 2>&1; then
        echo "FAIL $vendor: python3 with pip and venv is not usable by the agent user; AVI-FR-02 makes it part of the package floor" >&2
        status=1
    fi

    # AVI-FR-18 / AVI-FR-17, EAC-FR-14: the home directory the image names, writable by
    # the UID a launch supplies. Everything below depends on it — the vendor
    # CLI's scratch state, npm's cache, and pnpm's store all land under `$HOME`
    # — and a runtime that fell back to `/` would leave every one of them with
    # nowhere to write. Checked before the toolchains, so a failure names the
    # cause rather than the symptom.
    home="$(docker run --rm \
        --user "$foreign_uid" --cap-drop ALL --security-opt no-new-privileges \
        --entrypoint sh "$pinned" -c 'echo "$HOME"' 2>/dev/null | tr -d "\r")"
    if [[ "$home" != "/home/agent" ]]; then
        echo "FAIL $vendor: a UID with no account in the image gets '${home:-nothing}' as its home, not the one AVI-FR-18 names" >&2
        status=1
    fi

    # AVI-FR-17 / AVI-FR-18 / AVI-FR-13 / EAC-FR-14: the toolchains build
    # something, rather than only report a version.
    #
    # A `--version` proves a binary is on the path. It does not prove that the
    # linker `rustc` calls is installed, that `$HOME` and the two Cargo
    # directories are writable by the launching UID, or that pnpm has a store
    # it can write — and each of those leaves a Dockerfile that reads correctly
    # and a turn that fails on its first build. Every step below therefore
    # writes where the image says it may, under the conditions of EAC-FR-13 and
    # EAC-FR-14; `/tmp` alone would prove none of it, because `/tmp` is
    # writable by every UID whatever the image did.
    #
    # `--offline` throughout: this is a check of the image, not of a registry.
    probe="$(docker run --rm \
        --user "$foreign_uid" \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --entrypoint sh "$pinned" -c '
        set -e
        # Every tool in reach without a path in front of it (AVI-FR-17, EAC-FR-14, AVI-FR-18).
        command -v cargo rustc rustfmt cargo-clippy tsc pnpm >/dev/null
        # The three writable places AVI-FR-17 and AVI-FR-18 promise.
        touch "$HOME/.verify" "$CARGO_HOME/.verify" "$RUSTUP_HOME/.verify"
        # A crate that compiles, links, runs, and lints, and that writes the
        # Cargo caches into CARGO_HOME while it does.
        cd /tmp
        cargo new --bin --vcs none verify-crate >/dev/null 2>&1
        cd verify-crate
        cargo build --offline --quiet
        ./target/debug/verify-crate >/dev/null
        cargo clippy --offline --quiet
        cargo fmt --check
        # AVI-FR-02: the native floor the Rust toolchain builds through. A
        # `-sys` crate does not carry the library it binds; it asks pkg-config
        # where the headers are, and a missing answer stops the build inside a
        # transitive dependency before any code in the repository itself is
        # read. Asked of pkg-config rather than of dpkg, because what a build
        # script needs is an answer, not a package name.
        pkg-config --exists glib-2.0 gtk+-3.0 webkit2gtk-4.1 openssl
        # TypeScript that emits JavaScript the runtime can run.
        cd /tmp
        printf "export const value: number = 1;\n" > verify.ts
        tsc --outDir /tmp/verify-tsc /tmp/verify.ts
        node -e "require(\"/tmp/verify-tsc/verify.js\")"
        # pnpm with a store it can write, which lives under $HOME.
        mkdir -p /tmp/verify-pnpm
        cd /tmp/verify-pnpm
        printf "{\"name\":\"verify\",\"version\":\"1.0.0\",\"private\":true}\n" > package.json
        pnpm install --offline --ignore-scripts >/dev/null
        pnpm store path >/dev/null
        ' 2>&1)" || {
        echo "FAIL $vendor: the toolchains of AVI-FR-17 do not build as the executor launches the image" >&2
        echo "$probe" | sed "s/^/       /" >&2
        status=1
    }

    # AVI-FR-17: and at the versions the Dockerfile pins. An `ARG` default is
    # overridable with `--build-arg`, so an image can carry a toolchain its own
    # definition does not name and still build perfectly well.
    for tool in "rustc:RUST_VERSION" "pnpm:PNPM_VERSION" "tsc:TYPESCRIPT_VERSION"; do
        binary="${tool%%:*}"
        arg="${tool##*:}"
        want="$(build_arg "$vendor" "$arg")"
        got="$(docker run --rm --entrypoint "$binary" "$pinned" --version 2>/dev/null \
            | tr -d "\r" | grep -oE "[0-9]+\.[0-9]+\.[0-9]+" | head -1 || true)"
        if [[ "$got" != "$want" ]]; then
            echo "FAIL $vendor: the Dockerfile pins $arg=$want, the image reports '${got:-nothing}' from $binary" >&2
            status=1
        fi
    done

    # AVI-FR-16 / AVI-FR-13: the browser starts and draws a page — probed under
    # the conditions a real launch imposes rather than under a convenient
    # subset of them. The executor drops every capability, refuses new
    # privileges (EAC-FR-13), and runs the container as the *host* user's UID
    # (EAC-FR-14) — a UID with no account in the image. Each of those three
    # has its own way of stopping a browser
    # that starts perfectly well as the image's own user with default
    # privileges, and none of them is visible in a Dockerfile.
    #
    # A rendered PNG is the assertion, not an exit status: a headless browser
    # that fails to find a font, or a library, still exits 0 on some paths and
    # writes nothing.
    if ! docker run --rm \
        --user "$foreign_uid" \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --entrypoint sh "$pinned" -c '
        cd /tmp \
        && playwright screenshot --browser chromium \
            "data:text/html,<h1>verify</h1>" /tmp/verify.png >/dev/null 2>&1 \
        && test -s /tmp/verify.png' >/dev/null 2>&1; then
        echo "FAIL $vendor: the browser of AVI-FR-16 does not start and render as the executor launches it" >&2
        status=1
    fi

    ids="$(docker run --rm --entrypoint sh "$pinned" -c 'id -u; id -g' 2>/dev/null | tr '\n' ':' | sed 's/:$//')"
    if [[ "$ids" != "$uid:$gid" ]]; then
        echo "FAIL $vendor: manifest says $uid:$gid, image runs as '${ids:-unknown}'" >&2
        status=1
    fi

    [[ $status -eq 0 ]] && echo "ok   $vendor: $version at $digest, running as $uid:$gid, home at $home, toolchains build, browser renders"
done

exit "$status"
