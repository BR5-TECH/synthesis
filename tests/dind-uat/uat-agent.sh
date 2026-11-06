#!/bin/bash
# EAC-FR-UDQN, EAC-FR-BWTF, EAC-FR-LJXE: the scripted entrypoint of the
# Docker-in-Docker acceptance test. It replaces the vendor CLI. The executor
# starts it with the vendor arguments, in the execution directory.
#
# It runs the checks, writes what it found to `.uat/result.json`, and then
# gives the vendor arguments to `agentic-cli-mock`, so the turn ends cleanly
# and no real agent runs. Each check records `pass` or `fail` and its own
# words. A failed check does not stop the checks after it.

set -u

out=.uat
mkdir -p "$out"
fragments="$out/fragments"
: > "$fragments"

# One line of text that is safe in a JSON string, at most 1500 characters.
# Every control character, an ANSI colour code too, becomes a space.
esc() {
    tr '\000-\037\177' ' ' | sed 's/  */ /g; s/^ //; s/ $//; s/\\/\\\\/g; s/"/\\"/g' | cut -c1-1500
}

field() {
    printf '  "%s": "%s",\n' "$1" "$(printf '%s' "$2" | esc)" >> "$fragments"
}

# step NAME COMMAND...: runs the command, keeps all it said in
# `.uat/NAME.log`, and records its status and the end of what it said.
step() {
    local name=$1
    shift
    local said
    said=$("$@" 2>&1)
    local code=$?
    printf '%s\n' "$said" > "$out/$name.log"
    if [ $code -eq 0 ]; then
        field "$name" pass
    else
        field "$name" fail
    fi
    field "${name}_said" "$(printf '%s' "$said" | tail -c 1500)"
}

# The facts that explain a failure: who runs, what it can reach, and how.
field uid "$(id -u)"
field gid "$(id -g)"
field groups "$(id -G)"
field docker_host "${DOCKER_HOST:-}"
field host_override "${TESTCONTAINERS_HOST_OVERRIDE:-}"
field docker_cli "$(command -v docker)"
field socket "$(ls -ln /var/run/docker.sock 2>&1)"
field socket_gid "$(stat -c %g /var/run/docker.sock 2>&1)"
field socket_mounts "$(grep -c ' /var/run/docker.sock ' /proc/mounts)"
field cap_eff "$(awk '/^CapEff/ {print $2}' /proc/self/status)"
field cap_bnd "$(awk '/^CapBnd/ {print $2}' /proc/self/status)"
field daemon_id "$(docker info --format '{{.ID}}' 2>&1)"
# EAC-FR-LJXE: how the daemon created this container, where it did. In a VM
# no container of that name exists, and the field holds the error.
field self_inspect "$(docker inspect "$(cat /etc/hostname)" --format 'privileged={{.HostConfig.Privileged}} cap_add={{json .HostConfig.CapAdd}} cap_drop={{json .HostConfig.CapDrop}} mounts={{range .Mounts}}{{.Destination}},{{end}}' 2>&1)"

# EAC-FR-BWTF: the four checks of every shape.
step docker_version docker version --format 'client={{.Client.Version}} server={{.Server.Version}}'
step docker_run sh -c 'docker run --rm busybox:1.36 echo uat-container-ran | grep uat-container-ran'
step testcontainers cargo test --offline --manifest-path /opt/uat/testapp/Cargo.toml -- --nocapture
field testcontainers_host "$(sed -n 's/^uat-testcontainers-host=//p' "$out/testcontainers.log" | head -n 1)"
step browser sh -c 'playwright screenshot --browser chromium "data:text/html,<h1>uat</h1>" "$PWD/.uat/browser.png" && test -s "$PWD/.uat/browser.png"'

result="{
$(cat "$fragments")
  \"version\": \"1\"
}"
printf '%s\n' "$result" > "$out/result.json"
# On stderr too, so a result that cannot be written still shows.
printf 'uat result: %s\n' "$result" | tr '\n' ' ' >&2
echo >&2

# The mock checks the vendor arguments and replays a protocol-valid turn. The
# session it reports is the one the executor assigned.
session=""
previous=""
for arg in "$@"; do
    if [ "$previous" = "--session-id" ]; then
        session=$arg
    fi
    previous=$arg
done
sed "s/{{SESSION_ID}}/$session/g" /opt/uat/scenario.template.json > "$out/scenario.json"
exec /opt/uat/agentic-cli-mock --tool claude --scenario "$out/scenario.json" -- "$@"
