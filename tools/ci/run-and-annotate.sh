#!/usr/bin/env bash
# Run one gating command with its combined output captured to a file: printed for the
# human-readable step log, and on failure echoed once more as a single multi-line
# annotation (tools/ci/emit.py), which is the channel this repo can read back through
# the API. Exit status is passed through unchanged — this wrapper cannot make a
# failing check pass, it only makes the failure legible.
#
# Usage: run-and-annotate.sh <title> <logfile> -- <command...>
set -u
title="$1"
log="$2"
shift 3 # title, logfile, the literal '--'
"$@" >"$log" 2>&1
ec=$?
cat -- "$log"
if [ "$ec" -ne 0 ]; then
  python3 "$(dirname "$0")/emit.py" "$title" "$log"
fi
exit "$ec"
