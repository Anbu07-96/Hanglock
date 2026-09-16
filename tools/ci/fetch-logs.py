#!/usr/bin/env python3
"""Decode the gz annotation chunks of a run's job back into the captured log."""
import base64
import gzip
import json
import re
import subprocess
import sys

run = sys.argv[1]
want = sys.argv[2]  # job name substring
jobs = json.loads(subprocess.run(
    ["gh", "run", "view", run, "--json", "jobs", "--jq", ".jobs"],
    capture_output=True, text=True).stdout)
for job in jobs:
    if want not in job["name"]:
        continue
    anns = json.loads(subprocess.run(
        ["gh", "api", f"repos/Anbu07-96/Hanglock/check-runs/{job['databaseId']}/annotations", "--jq", "."],
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
