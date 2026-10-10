#!/bin/sh
# The linkage check of the macOS executable.
#
# Specification: specifications/infra/NLL-native-library-linkage.md
#
# Usage: check-linkage.sh <executable>
#
# It reads each library the executable loads (`otool -L`) and the run paths of
# the executable (`otool -l`). A library passes only when its path starts with
# a prefix that the specification lists (NLL-FR-NPMB). Each library that does
# not pass is named, and the script exits non-zero (NLL-FR-DQEK). An absent
# `otool` or an absent executable also exits non-zero (NLL-FR-QFZH), and so
# does an executable that `otool` cannot read or lists no library for
# (NLL-FR-FQPR).
#
# The `OTOOL` environment variable names the `otool` to run. The tests use it
# to supply a fake one.

set -u
# No pathname expansion: a library path is data, not a pattern.
set -f

me="check-linkage"
otool_cmd=${OTOOL:-otool}

if [ "$#" -ne 1 ]; then
  echo "$me: usage: check-linkage.sh <executable>" >&2
  exit 2
fi
exe=$1

if ! command -v "$otool_cmd" >/dev/null 2>&1; then
  echo "$me: the prerequisite \"otool\" is missing. No \"$otool_cmd\" executable is on PATH. Install the Xcode Command Line Tools." >&2
  exit 1
fi

if [ ! -f "$exe" ]; then
  echo "$me: the executable \"$exe\" does not exist." >&2
  exit 1
fi

if ! load_list=$("$otool_cmd" -L "$exe"); then
  echo "$me: \"otool -L\" cannot read \"$exe\"." >&2
  exit 1
fi
if ! load_commands=$("$otool_cmd" -l "$exe"); then
  echo "$me: \"otool -l\" cannot read \"$exe\"." >&2
  exit 1
fi

# A library line starts with white space and ends with the version clause. A
# header line ("<path>:" or "<path> (architecture arm64):") starts with no
# white space, so it is not a library.
libraries=$(printf '%s\n' "$load_list" |
  sed -n 's/^[[:space:]][[:space:]]*\(.*\) (compatibility version.*$/\1/p')

if [ -z "$libraries" ]; then
  echo "$me: \"otool -L\" lists no library for \"$exe\". It is not a Mach-O executable." >&2
  exit 1
fi

# An `@rpath/` library resolves inside the bundle only when the executable has
# run paths and every one is inside the bundle. With no run path, dyld cannot
# find the library at all.
rpaths=$(printf '%s\n' "$load_commands" |
  awk '$1 == "cmd" { in_rpath = ($2 == "LC_RPATH"); next }
       in_rpath && $1 == "path" { print $2; in_rpath = 0 }')
if [ -n "$rpaths" ]; then
  rpaths_inside=1
else
  rpaths_inside=0
fi
old_ifs=$IFS
IFS='
'
for rpath in $rpaths; do
  case $rpath in
    @executable_path/* | @loader_path/*) ;;
    *) rpaths_inside=0 ;;
  esac
done

failed=""
for library in $libraries; do
  case $library in
    /System/Library/* | /usr/lib/* | @executable_path/* | @loader_path/*) ;;
    @rpath/*)
      if [ "$rpaths_inside" -ne 1 ]; then
        failed="$failed
  $library"
      fi
      ;;
    *)
      failed="$failed
  $library"
      ;;
  esac
done
IFS=$old_ifs

if [ -n "$failed" ]; then
  echo "$me: \"$exe\" loads a library that is not a system library and not inside the bundle:$failed" >&2
  echo "$me: the bundle cannot start under the hardened runtime. Library validation refuses these libraries. Compile each one into the executable (specifications/infra/NLL-native-library-linkage.md NLL-FR-AVJK)." >&2
  exit 1
fi

echo "$me: every library that \"$exe\" loads is a system library or is inside the bundle."
