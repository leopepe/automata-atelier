#!/usr/bin/env python3
"""Parse `cargo bench -- --baseline ...` output, fail on real regressions.

Criterion already prints "Performance has regressed." when a change is
statistically significant (p < 0.05) and exceeds its (tiny) default noise
threshold of 1%. On shared CI runners that bar is too sensitive: noise
alone can push a stable benchmark over it. We layer two gates on top.

Gate 1 — robust regression: only fail if the **lower bound** of the 95%
confidence interval crosses the threshold. Using the lower bound (rather
than the mean or upper bound) means a bench fails only when even the
optimistic read of the data shows a regression. This trades sensitivity
for false-positive rate — a deliberate choice for shared-runner CI where
small benches drift by tens of percent under neighbour load.

Gate 2 — size-aware threshold: a fixed percentage threshold treats a
"+25 % regression" on an 8 ns op the same as a "+25 % regression" on an
8 ms op, but the absolute drift on the 8 ns op (≈ 2 ns) is well within
the runner's clock-quantum + scheduling jitter, while +25 % on the 8 ms
op is a real 2 ms slowdown. To compensate, we widen the threshold for
fast benches:

    effective_threshold_pct = max(base_threshold_pct,
                                  NOISE_FLOOR_NS / observed_median_ns * 100)

`NOISE_FLOOR_NS` is the absolute drift (in nanoseconds) we treat as
indistinguishable from runner jitter. At 2 ns it's roughly two CPU clock
periods plus typical Linux scheduling slack — small enough to pass real
slowdowns through, large enough to absorb the kind of background-process
hiccup that shows up as a 25 % regression on a sub-30 ns bench.

Worked examples (with NOISE_FLOOR_NS = 2.0 and base = 10.0 %):
    8 ns bench   → max(10.0, 25.0) = 25.0 %
    20 ns bench  → max(10.0, 10.0) = 10.0 %
    100 ns bench → max(10.0, 2.0)  = 10.0 %
    1 µs bench   → max(10.0, 0.2)  = 10.0 %

In other words: anything ≥ 20 ns observed median uses the base threshold
unchanged; smaller benches get a soft cap derived from absolute drift.

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
# Criterion emits e.g. "time:   [10.123 ns 10.456 ns 10.789 ns]". Units
# vary across benches (ns / µs / us / ms / s). Capture the median (middle)
# value with its unit so we can convert to nanoseconds for the size-aware
# threshold.
TIME_RE = re.compile(
    r"time:\s*\[\s*"
    r"[\d.]+\s*(?:ns|µs|us|ms|s)\s+"
    r"([\d.]+)\s*(ns|µs|us|ms|s)\s+"
    r"[\d.]+\s*(?:ns|µs|us|ms|s)\s*\]"
)
REGRESSED_VERDICT = "Performance has regressed"
BENCH_HEADER_RE = re.compile(r"^Benchmarking (?P<name>.+?): Analyzing\s*$", re.MULTILINE)

# Absolute drift below which we treat a regression as runner jitter
# rather than a real slowdown. See module docstring for the rationale.
NOISE_FLOOR_NS = 2.0

UNIT_TO_NS: dict[str, float] = {
    "ns": 1.0,
    "µs": 1_000.0,
    "us": 1_000.0,
    "ms": 1_000_000.0,
    "s": 1_000_000_000.0,
}


def parse_median_ns(body: str) -> float | None:
    """Pull the median time out of criterion's `time: [low med high]` line
    and convert to nanoseconds. Returns `None` if the line is missing
    (which would indicate either a parsing failure or a bench format
    change worth investigating)."""
    match = TIME_RE.search(body)
    if match is None:
        return None
    value = float(match.group(1))
    unit = match.group(2)
    return value * UNIT_TO_NS[unit]


def effective_threshold(base_pct: float, observed_ns: float | None) -> float:
    """Size-aware threshold: widen for fast benches whose absolute drift
    is dominated by clock-quantum + scheduling jitter. See module
    docstring for the formula."""
    if observed_ns is None or observed_ns <= 0.0:
        return base_pct
    floor_pct = NOISE_FLOOR_NS / observed_ns * 100.0
    return max(base_pct, floor_pct)


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
        base_threshold = float(sys.argv[2]) if len(sys.argv) == 3 else 10.0
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

    failures: list[tuple[str, float, float, float, float]] = []
    softened: list[tuple[str, float, float, float, float, float]] = []
    total = 0
    flagged_by_criterion = 0

    for name, body in zip(parts[1::2], parts[2::2]):
        total += 1
        match = CHANGE_RE.search(body)
        if match is None:
            # First-ever run with no prior baseline — nothing to compare.
            continue
        lower = float(match.group(1))
        median = float(match.group(2))
        upper = float(match.group(3))
        observed_ns = parse_median_ns(body)
        threshold = effective_threshold(base_threshold, observed_ns)
        criterion_regressed = REGRESSED_VERDICT in body
        if criterion_regressed:
            flagged_by_criterion += 1
            # Robust gate: even the optimistic (lower) bound of the
            # 95% CI must exceed the (size-aware) threshold for this
            # to count as a regression.
            if lower > threshold:
                failures.append((name, lower, median, upper, threshold))
            elif lower > base_threshold:
                # Would have failed under the flat base threshold but
                # is absorbed by the size-aware widening — surface it
                # so the gate's behaviour is auditable in the log.
                softened.append(
                    (name, lower, median, upper, threshold, observed_ns or 0.0)
                )

    print(
        f"Parsed {total} benchmark(s); criterion flagged {flagged_by_criterion}; "
        f"{len(failures)} exceed threshold (base {base_threshold}%, "
        f"size-aware floor {NOISE_FLOOR_NS} ns)."
    )

    if softened:
        print(
            "\nSize-aware widening absorbed these regressions "
            "(would have failed at flat threshold):"
        )
        for name, lower, median, upper, eff, ns in softened:
            print(
                f"  - {name}: median +{median:.1f}% "
                f"(CI [+{lower:.1f}%, +{upper:.1f}%]) — "
                f"observed {ns:.1f} ns, threshold widened to {eff:.1f}%"
            )

    if failures:
        print("", file=sys.stderr)
        print(
            "Regressions exceeding the threshold (lower CI bound > threshold):",
            file=sys.stderr,
        )
        for name, lower, median, upper, eff in failures:
            print(
                f"  - {name}: median +{median:.1f}% "
                f"(CI [+{lower:.1f}%, +{upper:.1f}%]) — threshold {eff:.1f}%",
                file=sys.stderr,
            )
        print(
            f"\nFAIL: {len(failures)} benchmark(s) regressed beyond their "
            f"size-aware threshold (base {base_threshold}%, "
            f"floor {NOISE_FLOOR_NS} ns).",
            file=sys.stderr,
        )
        return 1

    print(f"OK: no benchmark regressed beyond its size-aware threshold.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
