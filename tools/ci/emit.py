#!/usr/bin/env python3
"""Publish a captured command log as one GitHub Actions annotation.

The runner raw logs live on hosts that the analysis environment can reach only
intermittently, and step summaries have not shown up through the API; check-run
annotations are the one failure channel proven readable from both the REST API and
the run page. The whole log goes into a single multi-line annotation message, so
file:line context survives, not just the first lines of it.

Usage: emit.py <title> <logfile>
"""
import pathlib
import sys


def main() -> None:
    title, log_path = sys.argv[1], pathlib.Path(sys.argv[2])
    if log_path.exists():
        text = log_path.read_text(encoding="utf-8", errors="replace")
    else:
        text = "<missing log file: %s>" % log_path
    if not text.strip():
        text = "<empty log>"
    # Workflow-command escaping: % first (it introduces the other escapes), then the
    # characters that would end the annotation or break the command line. Titles take
    # no commas; callers keep them to words.
    text = text[:60_000].replace("%", "%25").replace("\r", "").replace("\n", "%0A")
    print("::error title=%s::%s" % (title.replace(",", " "), text))


main()
