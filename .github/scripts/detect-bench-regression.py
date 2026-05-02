#!/usr/bin/env python3
"""Parse `cargo bench -- --baseline ...` output, fail on real regressions.

Criterion already prints "Performance has regressed." when a change is
statistically significant (p < 0.05) and exceeds its (tiny) default noise
threshold of 1%. On shared CI runners that bar is too sensitive: noise
alone can push a stable benchmark over it. We layer an additional gate on
top: only fail if the upper bound of the 95% confidence interval crosses
`threshold_pct` (default 10%).

Usage:
    detect-bench-regression.py <criterion-log> [threshold-pct]

Exit codes:
    0  No benchmark regressed beyond the threshold.
    1  At least one benchmark regressed beyond the threshold.
    2  Argument or parsing error.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path


CHANGE_RE = re.compile(
    r"change:\s*\[\s*([+\-\d.]+)\s*%\s+([+\-\d.]+)\s*%\s+([+\-\d.]+)\s*%\s*\]"
)
REGRESSED_VERDICT = "Performance has regressed"
BENCH_HEADER_RE = re.compile(r"^Benchmarking (?P<name>.+?): Analyzing\s*$", re.MULTILINE)


def main() -> int:
    if len(sys.argv) < 2 or len(sys.argv) > 3:
        print(
            "usage: detect-bench-regression.py <criterion-log> [threshold-pct]",
            file=sys.stderr,
        )
        return 2

    log_path = Path(sys.argv[1])
    if not log_path.exists():
        print(f"error: log file not found: {log_path}", file=sys.stderr)
        return 2

    try:
        threshold = float(sys.argv[2]) if len(sys.argv) == 3 else 10.0
    except ValueError:
        print(f"error: threshold must be a number, got {sys.argv[2]!r}", file=sys.stderr)
        return 2

    text = log_path.read_text()

    # Each "Benchmarking <name>: Analyzing" line opens a record. The body
    # spans until the next such line. Use re.split to slice deterministically.
    parts = BENCH_HEADER_RE.split(text)
    # parts = [preamble, name1, body1, name2, body2, ...]
    if len(parts) <= 1:
        print(
            "error: no benchmark records found — was the bench actually run?",
            file=sys.stderr,
        )
        return 2

    failures: list[tuple[str, float]] = []
    total = 0
    flagged_by_criterion = 0

    for name, body in zip(parts[1::2], parts[2::2]):
        total += 1
        match = CHANGE_RE.search(body)
        if match is None:
            # First-ever run with no prior baseline — nothing to compare.
            continue
        upper = float(match.group(3))
        criterion_regressed = REGRESSED_VERDICT in body
        if criterion_regressed:
            flagged_by_criterion += 1
            if upper > threshold:
                failures.append((name, upper))

    print(
        f"Parsed {total} benchmark(s); criterion flagged {flagged_by_criterion}; "
        f"{len(failures)} exceed threshold of {threshold}%."
    )

    if failures:
        print("", file=sys.stderr)
        print("Regressions exceeding the threshold:", file=sys.stderr)
        for name, upper in failures:
            print(f"  - {name}: +{upper:.1f}% (CI upper bound)", file=sys.stderr)
        print(
            f"\nFAIL: {len(failures)} benchmark(s) regressed beyond {threshold}%.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: no benchmark regressed beyond {threshold}%.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
