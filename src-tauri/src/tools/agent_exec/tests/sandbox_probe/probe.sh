#!/bin/sh
# EAC-FR-RVQE: the probe. It runs in the execution directory, writes what it
# found to `.probe/result.json`, and then hands the vendor arguments to the mock.
# Every check prints a value and never stops the script, so one failed check
# does not hide the others.

value() { "$@" 2>/dev/null | tr '\n' ',' | sed 's/,$//; s/\\/\\\\/g; s/"/\\"/g'; }
answer() { if "$@" >/dev/null 2>&1; then echo yes; else echo no; fi; }

uid=$(value id -u)
gid=$(value id -g)
host_owner=$(value stat -c %u probe-host-file)
host_group=$(value stat -c %g probe-host-file)
host_mode=$(value stat -c %a probe-host-file)
boot_id=$(value cat /proc/sys/kernel/random/boot_id)
packed=$(value git rev-parse --verify -q refs/heads/probe-packed)
tool=$(value command -v probe-tool)
home=$(value printenv HOME)
optional_locks=$(value printenv GIT_OPTIONAL_LOCKS)
head=$(value git rev-parse HEAD)
diff=$(value git diff HEAD --name-only)
show=""
if [ -f probe-show ]; then
    show=$(value git show "$(cat probe-show)")
fi
git_dir=$(git rev-parse --absolute-git-dir 2>/dev/null)
git_dir_writable=no
if [ -n "$git_dir" ]; then
    git_dir_writable=$(answer touch "$git_dir/probe-write")
fi
common_dir=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
config_bytes=""
store_writable=no
if [ -n "$common_dir" ]; then
    config_bytes=$(value wc -c < "$common_dir/config")
    store_writable=$(answer touch "$common_dir/probe-store-write")
fi
# A file that stands in the store only when an earlier turn left it there.
store_leftover=$(answer test -e "$common_dir/probe-store-write-earlier")
if [ "$store_writable" = yes ]; then
    mv "$common_dir/probe-store-write" "$common_dir/probe-store-write-earlier" 2>/dev/null
fi
pin_writable=$(answer sh -c 'echo x >> .git')

writable=$(answer mkdir -p .probe)
if [ "$writable" = yes ]; then
    writable=$(answer sh -c 'echo probe > .probe/written')
fi
dir_writable=$(answer mkdir -p .probe/dir)
written_mode=$(value stat -c %a .probe/written)

result=$(cat <<JSON
{
  "uid": "$uid",
  "gid": "$gid",
  "host_owner": "$host_owner",
  "host_group": "$host_group",
  "host_mode": "$host_mode",
  "written_mode": "$written_mode",
  "boot_id": "$boot_id",
  "packed": "$packed",
  "store_writable": "$store_writable",
  "store_leftover": "$store_leftover",
  "pin_writable": "$pin_writable",
  "tool": "$tool",
  "home": "$home",
  "optional_locks": "$optional_locks",
  "head": "$head",
  "diff": "$diff",
  "show": "$show",
  "git_dir_writable": "$git_dir_writable",
  "config_bytes": "$config_bytes",
  "writable": "$writable",
  "dir_writable": "$dir_writable"
}
JSON
)
# On stderr always, so a probe that cannot write its working copy still says
# what it found.
printf 'probe result: %s\n' "$result" | tr '\n' ' ' >&2
echo >&2
if [ -d .probe ]; then
    printf '%s\n' "$result" > .probe/result.json
fi

# The mock checks the vendor arguments and replays a protocol-valid turn. The
# session it reports is the one the executor assigned (CCP-FR-16).
session=""
previous=""
for arg in "$@"; do
    if [ "$previous" = "--session-id" ]; then
        session=$arg
    fi
    previous=$arg
done
scenario=/opt/probe/scenario.template.json
if [ -d .probe ]; then
    sed "s/{{SESSION_ID}}/$session/g" /opt/probe/scenario.template.json > .probe/scenario.json
    scenario=.probe/scenario.json
fi
exec /opt/probe/agentic-cli-mock --tool claude --scenario "$scenario" -- "$@"
