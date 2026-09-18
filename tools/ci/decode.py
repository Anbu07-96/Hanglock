#!/usr/bin/env python3
"""Read what a CI run actually said, from the only channel that survives on this box.

The runners' log *hosts* are unreachable from the environment this repo is developed in, so `gh run
view --log` is not an option. What does survive is the check-run annotations, because the workflow
publishes them itself: `tools/ci/emit.py` gzip-compresses a captured log, base64-encodes it, and prints
it as up to nine `::error title=<name> gz <i>/<n>::<part>` annotations. GitHub caps one annotation
message at about 4 KB and keeps only a handful per run, which is why the payload is chunked and why a
log past nine chunks gets a `TRUNCATED` marker instead of vanishing quietly.

This is the other end of that pipe: group the annotations back by title, join the parts in order, inflate.
The useful output of a red run is usually one line, and finding it by hand in six base64 blobs is not a
thing anyone does twice, so `--grep` does it and prints the few lines around every hit.

    decode.py <run-id>                  every job of a workflow run
    decode.py --check <check-run-id>    one check-run
    decode.py --stdin < annotations.json  no network, no token; the `gh api` output saved earlier

Anything that is not a chunk — a plain `::error`, or emit.py's own `TRUNCATED` notice — prints as it
stands, under the path GitHub filed it against.

    $ python3 tools/ci/emit.py fmt /tmp/some.log | python3 -c '
      import json, re, sys
      out = []
      for line in sys.stdin:
          m = re.match(r"::error title=(.*?) gz (\\d+)/(\\d+)::(.*)", line.strip())
          out.append({"title": m.group(1), "message": m.group(4)} if m else
                     {"title": "note", "message": line.strip()})
      json.dump(out, sys.stdout)' | python3 tools/ci/decode.py --stdin
"""

from __future__ import annotations

import argparse
import base64
import gzip
import json
import re
import subprocess
import sys
import zlib

REPO = "Anbu07-96/Hanglock"
CHUNK = re.compile(r"^(?P<name>.*) gz (?P<i>\d+)/(?P<n>\d+)$")
GZIP_B64 = "H4sI"  # base64 of a gzip magic number: how to spot a payload without a title


def gh(*args: str) -> str:
    """`gh api`, with the failure text kept rather than swallowed."""
    r = subprocess.run(("gh", "api", *args), capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"gh api failed: {r.stdout.strip() or r.stderr.strip()}")
    return r.stdout


def annotations_for(run_id: str) -> list[dict]:
    """Every annotation of every check-run of a workflow run.

    Two APIs describe the same work — Actions *jobs* and Checks *check-runs* — and the mapping between
    them (`actions/jobs/<id>/checks`) 404s for a workflow run on this repo, so the run's head commit is
    used instead: `commits/<sha>/check-runs` lists the checks, and each carries `annotations_count`, which
    is what says whether anything was published at all. Duplicated check-runs (the same name for the
    `push` and `pull_request` events on one SHA) are deduplicated by id: the annotations are the same
    bytes, and re-printing a 60 KB log twice helps nobody.
    """
    sha = json.loads(gh(f"repos/{REPO}/actions/runs/{run_id}"))["head_sha"]
    checks = json.loads(gh(f"repos/{REPO}/commits/{sha}/check-runs?per_page=50")).get("check_runs", [])
    out: list[dict] = []
    seen: set[int] = set()
    for check in checks:
        if not check.get("output", {}).get("annotations_count") or check["id"] in seen:
            continue
        seen.add(check["id"])
        url = check["output"].get("annotations_url") or f"repos/{REPO}/check-runs/{check['id']}/annotations"
        for ann in json.loads(gh(url)):
            ann = dict(ann)
            ann.setdefault("title", check.get("name", ""))
            out.append(ann)
    return out


def inflate(parts: list[str]) -> str:
    """Join base64 chunks and unpack; report rather than crash when the stream was cut short."""
    raw = base64.b64decode("".join(parts), validate=False)
    try:
        return gzip.decompress(raw).decode("utf-8", "replace")
    except (OSError, EOFError, zlib.error) as exc:
        return (
            f"<inflate failed: {exc}\n"
            "  the annotation stream was truncated by GitHub, not by emit.py; head follows>\n"
            + salvage(raw)
        )


def salvage(raw: bytes, limit: int = 4000) -> str:
    """As much of a cut-short stream as inflates, because the first error is near the start.

    `zlib` with a gzip window, deliberately: it returns what it decoded before the stream ended,
    where `gzip`'s own helpers raise. `max_length` keeps a 60 KB log from being rebuilt in full.
    """
    if not raw:
        return "<nothing recovered>"
    try:
        d = zlib.decompressobj(zlib.MAX_WBITS | 16)
        return d.decompress(raw, limit).decode("utf-8", "replace")
    except (OSError, EOFError, zlib.error):
        return "<nothing recovered>"


def group(anns: list[dict]) -> list[tuple[str, str]]:
    """(heading, text) per published log, chunks reassembled in order."""
    chunks: dict[str, dict[int, str]] = {}
    expected: dict[str, int] = {}
    order: list[str] = []
    plain: list[tuple[str, str]] = []
    for ann in anns:
        title = str(ann.get("title") or "")
        msg = str(ann.get("message") or "")
        m = CHUNK.match(title)
        if m:
            name = m.group("name")
            if name not in chunks:
                chunks[name] = {}
                expected[name] = int(m.group("n"))
                order.append(name)
            chunks[name][int(m.group("i"))] = msg
            continue
        where = f"{ann.get('path', '?')}:{ann.get('start_line', '?')}"
        if not title and msg.startswith(GZIP_B64):  # a single-chunk emit with no title
            chunks.setdefault("(untitled gz)", {})
            if "(untitled gz)" not in order:
                order.append("(untitled gz)")
            chunks["(untitled gz)"].setdefault(1, msg)
            continue
        plain.append((f"{title or 'annotation'} @ {where}", msg))
    out: list[tuple[str, str]] = []
    for name in order:
        got = chunks[name]
        want = list(range(1, max(expected[name], max(got)) + 1))
        missing = sorted(set(want) - set(got))
        text = inflate([got[i] for i in sorted(got)])
        if missing:
            text += (
                f"\n<chunks {missing} of {expected[name]} never arrived — GitHub keeps only a handful"
                " of annotations per run, so re-run the step with --grep to narrow what it publishes>"
            )
        out.append((name, text))
    out.extend(plain)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("run_id", nargs="?")
    ap.add_argument("--check", help="a single check-run id")
    ap.add_argument("--stdin", action="store_true", help="read an annotations JSON array from stdin")
    ap.add_argument("--grep", help="only print lines matching this regex, with 2 lines of context")
    args = ap.parse_args()

    if args.stdin:
        anns = json.load(sys.stdin)
    elif args.check:
        anns = json.loads(gh(f"repos/{REPO}/check-runs/{args.check}/annotations"))
    elif args.run_id:
        anns = annotations_for(args.run_id)
    else:
        ap.error("give a run id, --check <id>, or --stdin")

    if isinstance(anns, dict):  # an API error object rather than a list
        sys.exit(f"unexpected response: {json.dumps(anns)[:400]}")
    if not anns:
        print("no annotations — the job passed, or it failed before any step published one")
        return

    want = re.compile(args.grep) if args.grep else None
    for heading, text in group(anns):
        if want is None:
            print(f"\n########## {heading}\n{text.rstrip()}")
            continue
        lines = text.splitlines()
        hits = [i for i, ln in enumerate(lines) if want.search(ln)]
        if not hits:
            continue
        shown: set[int] = set()
        print(f"\n########## {heading}")
        for i in hits:
            for j in range(max(0, i - 2), min(len(lines), i + 3)):
                if j not in shown:
                    shown.add(j)
                    print(lines[j])


if __name__ == "__main__":
    main()
