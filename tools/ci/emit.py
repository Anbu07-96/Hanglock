#!/usr/bin/env python3
"""Publish a captured command log as GitHub Actions annotations, in chunks.

The runner raw logs live on hosts the analysis environment cannot reach, and a single
check-run annotation message is capped at ~4 KB, so a whole failure log has no other
channel that survives. This splits the captured output into line-packed chunks of
under 3.6 KB, each emitted as its own ::error:: line, and appends a tail chunk when
the log was too long to cover entirely — head chunks catch the first diagnostics,
the tail catches the cargo "could not compile" summary.

Usage: emit.py <title> <logfile>
"""
import pathlib
import sys

CAP = 3600
MAX_PARTS = 12


def escape(text: str) -> str:
    # Workflow-command escaping, % first because it introduces the other escapes.
    return text.replace("%", "%25").replace("\r", "").replace("\n", "%0A")


def pack(lines):
    parts, buf = [], ""
    for line in lines:
        while len(line) > CAP:  # pathological single line: hard-split
            parts.append(buf + line[:CAP])
            buf = ""
            line = line[CAP:]
        ln = line + "\n"
        if len(buf) + len(ln) > CAP and buf:
            parts.append(buf)
            buf = ""
        buf += ln
    if buf:
        parts.append(buf)
    return parts


def main() -> None:
    title, log_path = sys.argv[1], pathlib.Path(sys.argv[2])
    if log_path.exists():
        text = log_path.read_text(encoding="utf-8", errors="replace")
    else:
        text = "<missing log file: %s>" % log_path
    if not text.strip():
        text = "<empty log>"
    lines = text.splitlines()
    parts = pack(lines)
    shown = min(len(parts), MAX_PARTS)
    for i, part in enumerate(parts[:shown], 1):
        suffix = " [%d/%d]" % (i, shown) if shown > 1 else ""
        print("::error title=%s%s::%s" % (title.replace(",", " "), suffix, escape(part)))
    if len(parts) > shown:
        print("::error title=%s tail::%s" % (title.replace(",", " "), escape("\n".join(lines[-60:]))))


main()
