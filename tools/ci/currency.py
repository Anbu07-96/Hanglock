#!/usr/bin/env python3
"""Is the checked-in artefact still what the tool that produces it produces?

Two gates, one file, because they answer the same question about two different kinds of artefact:
`tests/golden/*` and `crates/hanglock-render/src/face_data.rs` are both *generated*, and both are
useless the moment they stop matching their generator. A golden that no longer reflects the model
pins nothing; face data that no longer reflects the outline table is a rendering bug with a
provenance claim attached.

    currency.py source <committed-file> -- <generator> [args...]
        Runs the generator, compares its stdout with the committed file *by content*.

    currency.py trace <fixture.txt> <fixture.json> -- <generator> [args...]
        Runs the reference model's `trace` command into a temp prefix and compares the .txt line by
        line and the .json byte for byte.

Why `source` is not a plain `diff`: the committed file has been through `cargo fmt`, which rewrites
every array literal onto multiple lines and appends a trailing comma before each closing delimiter.
A textual diff therefore reports a difference on every run, and a gate that always fails is a gate
that gets deleted. So the comparison normalises exactly the two things rustfmt owns — whitespace and
the trailing comma it appends before any closing delimiter — and demands everything else match
character for character,
comments included: a doc comment that has drifted from the data under it is still a defect, and it is
still worth a red build. The blind spot is whitespace *inside* a string literal, which neither of
these files contains.

Why `trace` is numeric rather than byte-exact: positions are stored rounded to 7 decimals, so the
last stored digit is a coin flip if a runner's libm lands 1 ulp the other side of a rounding
boundary. The check compares each number with 5e-7 of tolerance — 200x tighter than the 1e-4 px that
`tests/golden_trace.rs` allows the solver, and far tighter than any model change, while surviving a
platform's last-digit choice. Integers, field counts and the `F`/`S` markers are exact.
The .json is byte-compared because it is the file a human diffs by eye, and it reproduces byte for
byte on every machine this has been run on so far.

Exit 0 = in sync. Exit 1 = drift, with a diff on stdout for whoever reads the annotation.
"""
from __future__ import annotations

import difflib
import os
import re
import subprocess
import sys
import tempfile

# Half a millionth of a pixel: above one ulp of a rounding decision, far below anything a solver
# change can produce.
TRACE_TOL = 5e-7


def norm_source(text: str) -> str:
    """Strip the two things `cargo fmt` owns: layout, and commas before a closing delimiter.

    The comma rule covers all three delimiters rustfmt closes a vertical list with: `]` and `}` for
    arrays and structs, and `)` for the tuples in `GLYPHS`, which is why that one is here."""
    text = re.sub(r",\s*([\]})])", r"\1", text)
    return re.sub(r"\s+", "", text)


def num(a: str, b: str) -> bool:
    """One field of one trace line: numbers within TRACE_TOL, everything else exact."""
    if a == b:
        return True
    try:
        return abs(float(a) - float(b)) <= TRACE_TOL
    except ValueError:
        return False


def diff_lines(want: list[str], got: list[str], limit: int = 24) -> int:
    shown = 0
    for line in difflib.unified_diff(want, got, "committed", "generated", lineterm="", n=1):
        print(line)
        shown += 1
        if shown >= limit:
            print("... (truncated)")
            break
    return shown


def cmd_source(committed: str, gen: list[str]) -> int:
    with open(committed, encoding="utf-8") as f:
        have = f.read()
    got = subprocess.run(gen, check=True, capture_output=True, text=True).stdout
    if norm_source(have) == norm_source(got):
        print(
            "%s matches its generator (%d bytes, %d lines; layout normalised)"
            % (committed, len(got), got.count("\n"))
        )
        return 0
    print("::error title=generated file is stale::%s is not what %s emits" % (committed, gen[0]))
    # The raw diff is mostly layout noise, so point at the content first: the first place the two
    # normalised streams part company, with enough context on both sides to see what changed.
    n_h, n_g = norm_source(have), norm_source(got)
    at = next((i for i in range(min(len(n_h), len(n_g))) if n_h[i] != n_g[i]), min(len(n_h), len(n_g)))
    for name, n in (("committed", n_h), ("generated", n_g)):
        print("%s @%d: ...%s..." % (name, at, n[max(0, at - 90) : at + 120]))
    print("--- raw diff (includes formatting) ---")
    diff_lines(have.splitlines(), got.splitlines())
    return 1


def cmd_trace(txt: str, jsonpath: str, gen: list[str]) -> int:
    tmp = tempfile.mkdtemp(prefix="hanglock-trace-")
    out_json = os.path.join(tmp, "trace.json")
    subprocess.run(gen + [out_json], check=True, capture_output=True, text=True)
    with open(os.path.splitext(out_json)[0] + ".txt", encoding="utf-8") as f:
        got = f.read()
    with open(txt, encoding="utf-8") as f:
        have = f.read()
    bad = 0
    wl, gl = have.splitlines(), got.splitlines()
    if len(wl) != len(gl):
        print("::error title=golden trace is stale::line count %d vs %d" % (len(wl), len(gl)))
        bad = 1
    else:
        for i, (a, b) in enumerate(zip(wl, gl)):
            ta, tb = a.split(), b.split()
            if len(ta) != len(tb) or not all(num(x, y) for x, y in zip(ta, tb)):
                if bad < 8:
                    print("line %d differs:\n  committed: %s\n  generated: %s" % (i + 1, a, b))
                bad += 1
        if bad:
            print("::error title=golden trace is stale::%d of %d lines moved; regenerate with"
                  " `python3 %s trace %s`" % (bad, len(wl), gen[0], txt))
    with open(jsonpath, "rb") as f:
        have_json = f.read()
    with open(out_json, "rb") as f:
        got_json = f.read()
    if have_json != got_json:
        print("::error title=golden trace json is stale::%s differs byte-wise" % jsonpath)
        bad += 1
    if bad:
        return 1
    print(
        "%s reproduces to the digit: %d lines, %d frames, tolerance %g px; %s byte-identical"
        % (txt, len(wl), sum(1 for a in wl if a.startswith("F ")), TRACE_TOL, jsonpath)
    )
    return 0


def main() -> int:
    argv = sys.argv[1:]
    if len(argv) < 3 or argv[0] not in ("source", "trace") or argv.index("--") == -1:
        print(__doc__)
        return 2
    sep = argv.index("--")
    mode, files, gen = argv[0], argv[1:sep], argv[sep + 1 :]
    if not gen:
        print("currency.py: nothing to run after --")
        return 2
    if mode == "source":
        if len(files) != 1:
            print("currency.py source takes exactly one file")
            return 2
        return cmd_source(files[0], gen)
    if len(files) != 2:
        print("currency.py trace takes the .txt then the .json")
        return 2
    return cmd_trace(files[0], files[1], gen)


if __name__ == "__main__":
    raise SystemExit(main())
