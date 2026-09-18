#!/usr/bin/env python3
"""Publish the tail of a captured log as one `::notice::` workflow command.

The failure channel (`emit.py`) has to carry a whole cargo session, so it ships a chunked gzip blob.
This is the opposite case: a gate that *passed* and printed a number worth keeping — the measured
microseconds per frame, the pixel budget the face costs. Those numbers are the point of running the
gate, and this repo's CI is read back through the annotations API because both raw-log hosts refuse
the request; a `::notice` line is retrievable there and a step log is not, so the notice is the
difference between the figure being in the record and being in a log nobody can open.

Nothing here can change a verdict: the caller's own exit status is untouched, `::` in the captured
text is removed so a log line cannot forge a workflow command, and the message is capped well below
GitHub's ~4 KB annotation limit.

Usage: notice.py <title> <logfile> [lines]
"""
import pathlib
import re
import sys

MAX_CHARS = 1000


def main() -> int:
    title, log = sys.argv[1], pathlib.Path(sys.argv[2])
    want = int(sys.argv[3]) if len(sys.argv) > 3 else 4
    text = log.read_text(encoding="utf-8", errors="replace") if log.exists() else "<missing log>"
    keep = [line.strip() for line in text.splitlines() if line.strip()]
    # The tables this captures are column-aligned with spaces; one space is enough for a log line
    # that will be read on a single wrapped line.
    one = re.sub(r" +", " ", " / ".join(keep[-want:])).replace("::", ":").replace("%", "%25")
    safe = title.replace(",", " ").replace(":", " ")
    print("::notice title=%s::%s" % (safe, one[:MAX_CHARS]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
