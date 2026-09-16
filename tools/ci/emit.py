#!/usr/bin/env python3
"""Publish a captured command log losslessly through check-run annotations.

Constraints this routes around: both raw-log hosts are unreachable from the analysis
environment, step summaries never surface via the API, GitHub silently drops
annotations beyond a small per-run count, and one annotation message caps at ~4 KB.
So the log is gzip-compressed, base64-encoded, and emitted as at most nine [i/n]
chunk annotations — a whole rustfmt diff or cargo session fits in six and survives
byte-exact. `git`-style decoding on the other side: join parts, b64decode, gunzip.

Usage: emit.py <title> <logfile>
"""
import base64
import gzip
import pathlib
import sys

CAP = 3400
MAX_PARTS = 9


def main() -> None:
    title, log_path = sys.argv[1], pathlib.Path(sys.argv[2])
    if log_path.exists():
        text = log_path.read_text(encoding="utf-8", errors="replace")
    else:
        text = "<missing log file: %s>" % log_path
    if not text.strip():
        text = "<empty log>"
    safe_title = title.replace(",", " ").replace(":", " ")
    b64 = base64.b64encode(gzip.compress(text.encode("utf-8", "replace"), 9)).decode()
    parts = [b64[i:i + CAP] for i in range(0, len(b64), CAP)]
    truncated = len(parts) > MAX_PARTS
    for i, part in enumerate(parts[:MAX_PARTS], 1):
        print("::error title=%s gz %d/%d::%s" % (safe_title, i, len(parts), part))
    if truncated:
        print("::error title=%s TRUNCATED::dropped %d of %d chunks (%d raw bytes)"
              % (safe_title, len(parts) - MAX_PARTS, len(parts), len(text)))


main()
