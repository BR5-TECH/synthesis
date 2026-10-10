#!/bin/sh
# A fake `otool` for the linkage check tests (mod.rs beside this file).
#
# It is a checked-in file and not a file a test writes, so no test runs a file
# that is still open for writing. On Linux that can fail with ETXTBSY.
#
# FAKE_OTOOL_EXPECT    the executable path the check must pass
# FAKE_OTOOL_LIST      the file that `-L` prints
# FAKE_OTOOL_COMMANDS  the file that `-l` prints
# FAKE_OTOOL_FAIL      `-L` or `-l`: that option exits non-zero

if [ "$#" -ne 2 ] || [ "$2" != "$FAKE_OTOOL_EXPECT" ]; then
  echo "fake-otool: unexpected arguments: $*" >&2
  exit 64
fi
if [ "${FAKE_OTOOL_FAIL:-}" = "$1" ]; then
  echo "fake-otool: cannot read $2" >&2
  exit 1
fi
case "$1" in
  -L) cat "$FAKE_OTOOL_LIST" ;;
  -l) cat "$FAKE_OTOOL_COMMANDS" ;;
  *) exit 64 ;;
esac
