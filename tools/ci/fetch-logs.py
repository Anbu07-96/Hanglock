#!/usr/bin/env python3
"""Reassemble a run's captured logs from its gz annotations.

The steps that can fail ship their log as gzip chunks in annotations (`emit.py`, and
`tools/ci/run-and-annotate.sh`) because a step's stdout is a poor place to read a cargo session from
somewhere else; this puts the chunks back into files. Needs `gh` authenticated for the repo it is run
in, which is where the slug below comes from.

    python3 tools/ci/fetch-logs.py <run-id> <job-name-substring>   # -> /tmp/decoded_<stream>.txt
"""
import base64
import gzip
import json
import re
import subprocess
import sys

run = sys.argv[1]
want = sys.argv[2]  # job name substring
slug = subprocess.run(
    ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
    capture_output=True, text=True).stdout.strip()
jobs = json.loads(subprocess.run(
    ["gh", "run", "view", run, "--json", "jobs", "--jq", ".jobs"],
    capture_output=True, text=True).stdout)
for job in jobs:
    if want not in job["name"]:
        continue
    anns = json.loads(subprocess.run(
        ["gh", "api", f"repos/{slug}/check-runs/{job['databaseId']}/annotations", "--jq", "."],
        capture_output=True, text=True).stdout)
    streams = {}
    for a in anns:
        m = re.match(r"(.+) gz (\d+)/(\d+)$", a.get("title") or "")
        if m:
            streams.setdefault(m.group(1), {})[int(m.group(2))] = a["message"]
        elif (a.get("title") or "").endswith("TRUNCATED"):
            print("TRUNCATED:", a["title"], "-", a["message"])
    for name, chunks in streams.items():
        expected = max(k for k in chunks)
        have = sorted(chunks)
        print(f"# job={job['name']} stream={name} chunks={have} (expected up to {expected})")
        b64 = "".join(chunks[i] for i in have)
        try:
            log = gzip.decompress(base64.b64decode(b64)).decode("utf-8", "replace")
        except Exception as e:  # noqa: BLE001
            print("decode failed:", e)
            continue
        fn = f"/tmp/decoded_{re.sub(r'[^a-z0-9]+','_', name.lower())}.txt"
        open(fn, "w").write(log)
        print(f"wrote {fn} ({len(log)} bytes)")
